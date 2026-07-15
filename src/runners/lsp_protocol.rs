use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) struct InitializeResult {
    pub(crate) capabilities: Value,
    pub(crate) server_name: Option<String>,
    pub(crate) server_version: Option<String>,
}

pub(crate) struct LspSession {
    label: String,
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Result<Value, String>>,
    next_id: u64,
    configuration: Value,
    accept_first_action_requests: bool,
    workspace_uri: String,
    stderr_text: Arc<Mutex<String>>,
    request_timeout: Duration,
}

impl LspSession {
    pub(crate) fn start(
        command: &mut Command,
        label: impl Into<String>,
        workspace_uri: String,
        configuration: Value,
        accept_first_action_requests: bool,
        request_timeout: Duration,
    ) -> Result<Self> {
        let label = label.into();
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {label} language server"))?;
        let stdin = child.stdin.take().context("missing LSP stdin")?;
        let stdout = child.stdout.take().context("missing LSP stdout")?;
        let stderr = child.stderr.take().context("missing LSP stderr")?;
        let stderr_text = Arc::new(Mutex::new(String::new()));
        let stderr_capture = Arc::clone(&stderr_text);
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Ok(mut captured) = stderr_capture.lock() {
                    captured.push_str(&line);
                    captured.push('\n');
                    if captured.len() > 64 * 1024 {
                        let keep_from = captured.len() - 64 * 1024;
                        *captured = captured.split_off(keep_from);
                    }
                }
            }
        });
        let (sender, messages) = mpsc::channel();
        let reader_label = label.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(message)) => {
                        if sender.send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = sender
                            .send(Err(format!("{reader_label} language server closed stdout")));
                        break;
                    }
                    Err(error) => {
                        let _ =
                            sender.send(Err(format!("read {reader_label} response: {error:#}")));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            label,
            child,
            stdin,
            messages,
            next_id: 1,
            configuration,
            accept_first_action_requests,
            workspace_uri,
            stderr_text,
            request_timeout,
        })
    }

    pub(crate) fn initialize(
        &mut self,
        process_id: u32,
        workspace_path: &Path,
        initialization_options: &Value,
        additional_client_capabilities: &Value,
    ) -> Result<InitializeResult> {
        let mut client_capabilities = json!({
            "general": {
                "positionEncodings": ["utf-16"]
            },
            "workspace": {
                "configuration": true,
                "workspaceFolders": true
            },
            "textDocument": {
                "definition": {
                    "dynamicRegistration": false,
                    "linkSupport": true
                },
                "references": {
                    "dynamicRegistration": false
                },
                "typeDefinition": {
                    "dynamicRegistration": false,
                    "linkSupport": true
                },
                "synchronization": {
                    "dynamicRegistration": false,
                    "didSave": false,
                    "willSave": false,
                    "willSaveWaitUntil": false
                }
            }
        });
        merge_json(&mut client_capabilities, additional_client_capabilities);
        let response = self.request(
            "initialize",
            json!({
                "processId": process_id,
                "clientInfo": {
                    "name": "usagebench",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "rootPath": workspace_path,
                "rootUri": self.workspace_uri,
                "workspaceFolders": [{
                    "uri": self.workspace_uri,
                    "name": "usagebench"
                }],
                "capabilities": client_capabilities,
                "initializationOptions": initialization_options
            }),
        )?;
        if let Some(error) = response.get("error") {
            bail!("{} initialize failed: {error}", self.label);
        }
        let result = response
            .get("result")
            .context("LSP initialize response missing result")?;
        let capabilities = result
            .get("capabilities")
            .cloned()
            .context("LSP initialize result missing capabilities")?;
        let server_info = result.get("serverInfo");
        let server_name = server_info
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let server_version = server_info
            .and_then(|value| value.get("version"))
            .and_then(Value::as_str)
            .map(str::to_string);
        self.notify("initialized", json!({}))?;
        if !self.configuration.is_null() {
            self.notify(
                "workspace/didChangeConfiguration",
                json!({"settings": self.configuration}),
            )?;
        }
        Ok(InitializeResult {
            capabilities,
            server_name,
            server_version,
        })
    }

    pub(crate) fn did_open(&mut self, uri: &str, language_id: &str, text: &str) -> Result<()> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        )
    }

    pub(crate) fn query(&mut self, method: &str, params: Value) -> Result<Value> {
        let response = self.request(method, params)?;
        if let Some(error) = response.get("error") {
            bail!(
                "{} `{method}` failed: {error}\nserver stderr:\n{}",
                self.label,
                self.stderr_snapshot()
            );
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    pub(crate) fn wait_for_notification(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!(
                    "timed out after {:.1} seconds waiting for {} `{method}` notification\nserver stderr:\n{}",
                    timeout.as_secs_f64(),
                    self.label,
                    self.stderr_snapshot()
                );
            }
            let message = match self.messages.recv_timeout(remaining) {
                Ok(Ok(message)) => message,
                Ok(Err(message)) => bail!(
                    "{message}\nserver stderr:\n{}",
                    self.stderr_snapshot()
                ),
                Err(RecvTimeoutError::Timeout) => bail!(
                    "timed out after {:.1} seconds waiting for {} `{method}` notification\nserver stderr:\n{}",
                    timeout.as_secs_f64(),
                    self.label,
                    self.stderr_snapshot()
                ),
                Err(RecvTimeoutError::Disconnected) => bail!(
                    "{} response channel disconnected\nserver stderr:\n{}",
                    self.label,
                    self.stderr_snapshot()
                ),
            };
            if is_server_request(&message) {
                self.respond_to_server_request(&message)?;
                continue;
            }
            if message.get("id").is_none()
                && message.get("method").and_then(Value::as_str) == Some(method)
            {
                return Ok(message.get("params").cloned().unwrap_or(Value::Null));
            }
        }
    }

    pub(crate) fn pump_for(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            let message = match self.messages.recv_timeout(remaining) {
                Ok(Ok(message)) => message,
                Ok(Err(message)) => bail!("{message}\nserver stderr:\n{}", self.stderr_snapshot()),
                Err(RecvTimeoutError::Timeout) => return Ok(()),
                Err(RecvTimeoutError::Disconnected) => bail!(
                    "{} response channel disconnected\nserver stderr:\n{}",
                    self.label,
                    self.stderr_snapshot()
                ),
            };
            if is_server_request(&message) {
                self.respond_to_server_request(&message)?;
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        let deadline = Instant::now() + self.request_timeout;
        let mut observed_messages = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!(
                    "timed out after {:.1} seconds waiting for {} `{method}` response\nserver stderr:\n{}\nlast messages: {}",
                    self.request_timeout.as_secs_f64(),
                    self.label,
                    self.stderr_snapshot(),
                    observed_messages.join(", ")
                );
            }
            let message = match self.messages.recv_timeout(remaining) {
                Ok(Ok(message)) => message,
                Ok(Err(message)) => {
                    let stderr = self.stderr_snapshot();
                    if stderr.is_empty() {
                        return Err(anyhow!(message));
                    }
                    bail!("{message}\nserver stderr:\n{stderr}");
                }
                Err(RecvTimeoutError::Timeout) => bail!(
                    "timed out after {:.1} seconds waiting for {} `{method}` response\nserver stderr:\n{}\nlast messages: {}",
                    self.request_timeout.as_secs_f64(),
                    self.label,
                    self.stderr_snapshot(),
                    observed_messages.join(", ")
                ),
                Err(RecvTimeoutError::Disconnected) => bail!(
                    "{} response channel disconnected\nserver stderr:\n{}",
                    self.label,
                    self.stderr_snapshot()
                ),
            };
            observed_messages.push(describe_message(&message));
            if observed_messages.len() > 20 {
                observed_messages.remove(0);
            }
            if is_server_request(&message) {
                self.respond_to_server_request(&message)?;
                continue;
            }
            if is_response_for(&message, id) {
                return Ok(message);
            }
        }
    }

    pub(crate) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }))
    }

    fn respond_to_server_request(&mut self, request: &Value) -> Result<()> {
        let id = request
            .get("id")
            .cloned()
            .context("server request missing id")?;
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .context("server request missing method")?;
        let result = match method {
            "workspace/configuration" => {
                let items = request
                    .pointer("/params/items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                Value::Array(
                    items
                        .iter()
                        .map(|item| configuration_section(&self.configuration, item))
                        .collect(),
                )
            }
            "workspace/workspaceFolders" => json!([{
                "uri": self.workspace_uri,
                "name": "usagebench"
            }]),
            "workspace/applyEdit" => json!({"applied": false}),
            "window/showMessageRequest" if self.accept_first_action_requests => request
                .pointer("/params/actions/0")
                .cloned()
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }))
    }

    fn write_message(&mut self, payload: &Value) -> Result<()> {
        let body = serde_json::to_vec(payload).context("serialize LSP message")?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())
            .and_then(|_| self.stdin.write_all(&body))
            .and_then(|_| self.stdin.flush())
            .context("write LSP message")
    }

    fn stderr_snapshot(&self) -> String {
        self.stderr_text
            .lock()
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    }
}

fn configuration_section(configuration: &Value, item: &Value) -> Value {
    let Some(section) = item.get("section").and_then(Value::as_str) else {
        return configuration.clone();
    };
    if let Some(value) = configuration.get(section) {
        return value.clone();
    }
    let mut value = configuration;
    for part in section.split('.') {
        let Some(next) = value.get(part) else {
            return Value::Null;
        };
        value = next;
    }
    value.clone()
}

fn merge_json(target: &mut Value, source: &Value) {
    if source.is_null() {
        return;
    }
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                merge_json(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn describe_message(message: &Value) -> String {
    let id = message
        .get("id")
        .map(Value::to_string)
        .unwrap_or_else(|| "-".to_string());
    let method = message.get("method").and_then(Value::as_str).unwrap_or(
        if message.get("result").is_some() {
            "<response>"
        } else {
            "<message>"
        },
    );
    format!("{method}#{id}")
}

fn is_server_request(message: &Value) -> bool {
    message.get("id").is_some() && message.get("method").is_some()
}

fn is_response_for(message: &Value, id: u64) -> bool {
    !is_server_request(message) && message.get("id") == Some(&json!(id))
}

impl Drop for LspSession {
    fn drop(&mut self) {
        let _ = self.write_message(&json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).context("read LSP header")? == 0 {
            return Ok(None);
        }
        if header == "\r\n" || header == "\n" {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            bail!("invalid LSP header: {}", header.trim_end());
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .with_context(|| format!("parse LSP Content-Length `{}`", value.trim()))?,
            );
        }
    }
    let content_length = content_length.context("LSP message missing Content-Length")?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).context("read LSP body")?;
    serde_json::from_slice(&body)
        .map(Some)
        .with_context(|| format!("parse LSP JSON: {}", String::from_utf8_lossy(&body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_content_length_framed_message() {
        let body = r#"{"jsonrpc":"2.0","id":7,"result":null}"#;
        let input = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let message = read_message(&mut Cursor::new(input)).unwrap().unwrap();
        assert_eq!(message["id"], 7);
    }

    #[test]
    fn accepts_additional_headers() {
        let body = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let input = format!(
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let message = read_message(&mut Cursor::new(input)).unwrap().unwrap();
        assert_eq!(message["method"], "initialized");
    }

    #[test]
    fn server_request_id_does_not_collide_with_client_response_id() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "workspace/configuration",
            "params": {"items": []}
        });
        assert!(is_server_request(&request));
        assert!(!is_response_for(&request, 2));

        let response = json!({"jsonrpc": "2.0", "id": 2, "result": null});
        assert!(!is_server_request(&response));
        assert!(is_response_for(&response, 2));
    }

    #[test]
    fn selects_requested_configuration_section() {
        let configuration = json!({
            "metals": {"autoImportBuilds": "all"},
            "rust-analyzer": {"checkOnSave": false}
        });
        assert_eq!(
            configuration_section(&configuration, &json!({"section": "metals"})),
            json!({"autoImportBuilds": "all"})
        );
        assert_eq!(
            configuration_section(
                &configuration,
                &json!({"section": "metals.autoImportBuilds"})
            ),
            json!("all")
        );
    }

    #[test]
    fn merges_profile_client_capabilities_without_dropping_standard_fields() {
        let mut capabilities = json!({"workspace": {"configuration": true}});
        merge_json(
            &mut capabilities,
            &json!({"workspace": {"_vs_projectContext": {"refreshSupport": true}}}),
        );
        assert_eq!(capabilities["workspace"]["configuration"], true);
        assert_eq!(
            capabilities["workspace"]["_vs_projectContext"]["refreshSupport"],
            true
        );
        merge_json(&mut capabilities, &Value::Null);
        assert_eq!(capabilities["workspace"]["configuration"], true);
    }
}
