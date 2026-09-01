use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::{
    error::Error as StdError,
    fmt,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

// Bifrost usage scans can consume their full five-minute batch budget before
// cooperative shutdown, serialization, and response delivery. Match the
// runner's ten-minute process envelope so cold-workspace cleanup cannot turn
// structured incomplete evidence into a client-side timeout.
//
// This is the envelope, not the scan budget. Bifrost no longer accepts a
// per-request deadline -- it leaves deadline policy to the frontend -- so a
// caller that wants a tighter bound on one call sets it with
// `ToolClient::set_request_timeout`.
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SNAPSHOT_NOT_READY_CODE: i64 = -32603;
const SNAPSHOT_NOT_READY_MESSAGE: &str =
    "workspace snapshot was not ready within the request-wide time budget; retry after workspace initialization completes";
const SNAPSHOT_NOT_READY_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

#[derive(Debug, Clone)]
struct McpResponseError {
    label: String,
    tool: String,
    code: i64,
    message: String,
    data: Option<Value>,
}

impl McpResponseError {
    fn is_snapshot_not_ready(&self) -> bool {
        self.code == SNAPSHOT_NOT_READY_CODE && self.message == SNAPSHOT_NOT_READY_MESSAGE
    }
}

impl fmt::Display for McpResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} MCP request failed for `{}`: code {}: {}",
            self.label, self.tool, self.code, self.message
        )?;
        if let Some(data) = &self.data {
            write!(formatter, "; data: {data}")?;
        }
        Ok(())
    }
}

impl StdError for McpResponseError {}

pub(crate) fn is_snapshot_not_ready_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<McpResponseError>()
            .is_some_and(McpResponseError::is_snapshot_not_ready)
    })
}

pub(crate) trait ToolClient {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value>;

    /// Bound the wall clock of subsequent calls.
    ///
    /// Bifrost stopped accepting a per-request `max_duration_secs` and leaves
    /// deadline policy to the frontend, so a budget the caller wants honoured
    /// has to be enforced here. Clients with no clock of their own ignore it.
    fn set_request_timeout(&mut self, _timeout: Duration) {}
}

pub(crate) struct McpSession {
    label: String,
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Receiver<Result<String, String>>,
    next_id: u64,
    snapshot_not_ready: Option<McpResponseError>,
    workspace_readiness_duration: Duration,
    request_timeout: Duration,
}

impl McpSession {
    pub(crate) fn start(command: &mut Command, label: impl Into<String>) -> Result<Self> {
        let label = label.into();
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {label} MCP server"))?;
        let stdin = child.stdin.take().context("missing MCP stdin")?;
        let stdout = child.stdout.take().context("missing MCP stdout")?;
        let stderr = child.stderr.take().context("missing MCP stderr")?;
        let stderr_output = Arc::new(Mutex::new(String::new()));
        let stderr_capture = Arc::clone(&stderr_output);
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = BufReader::new(stderr)
                .take(64 * 1024)
                .read_to_end(&mut bytes);
            *stderr_capture.lock().expect("MCP stderr lock poisoned") =
                String::from_utf8_lossy(&bytes).into_owned();
        });
        let (sender, stdout_lines) = mpsc::channel();
        let reader_label = label.clone();
        let reader_stderr = Arc::clone(&stderr_output);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        thread::yield_now();
                        let stderr = reader_stderr.lock().expect("MCP stderr lock poisoned");
                        let message = if stderr.trim().is_empty() {
                            format!("{reader_label} MCP server closed stdout")
                        } else {
                            format!(
                                "{reader_label} MCP server closed stdout; stderr: {}",
                                stderr.trim()
                            )
                        };
                        let _ = sender.send(Err(message));
                        break;
                    }
                    Ok(_) => {
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(format!("read MCP response: {error}")));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            label,
            child,
            stdin,
            stdout_lines,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
            request_timeout: MCP_REQUEST_TIMEOUT,
        })
    }

    pub(crate) fn initialize(&mut self) -> Result<()> {
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "usagebench",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }))?;
        if let Some(error) = response.get("error") {
            bail!("{} initialize failed: {error}", self.label);
        }
        self.notify(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
    }

    fn request(&mut self, payload: Value) -> Result<Value> {
        let expected_id = payload
            .get("id")
            .cloned()
            .context("JSON-RPC request missing id")?;
        self.write_line(&payload)?;
        read_json_rpc_response(
            &self.stdout_lines,
            expected_id,
            &self.label,
            self.request_timeout,
        )
    }

    fn notify(&mut self, payload: Value) -> Result<()> {
        self.write_line(&payload)
    }

    fn write_line(&mut self, payload: &Value) -> Result<()> {
        writeln!(self.stdin, "{payload}")
            .and_then(|_| self.stdin.flush())
            .context("write MCP request")
    }
}

fn read_json_rpc_response(
    stdout_lines: &Receiver<Result<String, String>>,
    expected_id: Value,
    label: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!(
                "timed out after {} seconds waiting for {label} MCP response",
                timeout.as_secs()
            );
        }
        let line = stdout_lines
            .recv_timeout(remaining)
            .with_context(|| {
                format!(
                    "timed out after {} seconds waiting for {label} MCP response",
                    timeout.as_secs()
                )
            })?
            .map_err(|message| anyhow!(message))?;
        let response: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse MCP JSON response: {line}"))?;
        if response.get("id") == Some(&expected_id) {
            return Ok(response);
        }
    }
}

impl ToolClient for McpSession {
    fn set_request_timeout(&mut self, timeout: Duration) {
        // Never widen the process envelope: a caller may tighten its own
        // deadline, but the run still has to finish inside the runner's.
        self.request_timeout = timeout.min(MCP_REQUEST_TIMEOUT);
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        if let Some(error) = &self.snapshot_not_ready {
            return Err(error.clone().into());
        }
        let mut readiness_duration = Duration::ZERO;
        let result = retry_snapshot_not_ready(
            || self.call_tool_once(name, arguments.clone()),
            thread::sleep,
            |duration| readiness_duration += duration,
        );
        self.workspace_readiness_duration += readiness_duration;
        if let Err(error) = &result {
            self.snapshot_not_ready = error.chain().find_map(|cause| {
                cause
                    .downcast_ref::<McpResponseError>()
                    .filter(|error| error.is_snapshot_not_ready())
                    .cloned()
            });
        }
        result
    }
}

impl McpSession {
    pub(crate) fn take_workspace_readiness_duration(&mut self) -> Duration {
        std::mem::take(&mut self.workspace_readiness_duration)
    }

    pub(crate) fn take_snapshot_not_ready_error(&mut self) -> Option<anyhow::Error> {
        self.snapshot_not_ready.take().map(Into::into)
    }

    fn call_tool_once(&mut self, name: &str, arguments: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
            }
        }))?;
        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(Value::as_i64);
            let message = error.get("message").and_then(Value::as_str);
            if let (Some(code), Some(message)) = (code, message) {
                return Err(McpResponseError {
                    label: self.label.clone(),
                    tool: name.to_string(),
                    code,
                    message: message.to_string(),
                    data: error.get("data").cloned(),
                }
                .into());
            }
            bail!("{} MCP request failed for `{name}`: {error}", self.label);
        }
        let result = response
            .get("result")
            .with_context(|| format!("{} MCP response missing result", self.label))?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let message = result
                .get("content")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("tool returned isError without text");
            bail!("{} tool `{name}` failed: {message}", self.label);
        }
        result.get("structuredContent").cloned().with_context(|| {
            format!(
                "{} tool `{name}` response missing structuredContent",
                self.label
            )
        })
    }
}

fn retry_snapshot_not_ready<T>(
    mut operation: impl FnMut() -> Result<T>,
    mut sleep: impl FnMut(Duration),
    mut record_readiness: impl FnMut(Duration),
) -> Result<T> {
    for delay in SNAPSHOT_NOT_READY_RETRY_DELAYS {
        let attempt_started = Instant::now();
        match operation() {
            Err(error) if is_snapshot_not_ready_error(&error) => {
                let attempt_duration = attempt_started.elapsed();
                sleep(delay);
                record_readiness(attempt_duration.saturating_add(delay));
            }
            result => return result,
        }
    }
    match operation() {
        Err(error) if is_snapshot_not_ready_error(&error) => Err(error).with_context(|| {
            format!(
                "workspace readiness retries exhausted after {} attempts",
                SNAPSHOT_NOT_READY_RETRY_DELAYS.len() + 1
            )
        }),
        result => result,
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_timeout_exceeds_maximum_bifrost_scan_budget() {
        assert!(
            MCP_REQUEST_TIMEOUT
                >= Duration::from_secs(
                    crate::runners::bifrost::MAX_SCAN_USAGES_MAX_DURATION_SECS + 5 * 60
                )
        );
    }

    #[test]
    fn response_reader_skips_notifications_and_other_ids() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Ok(
                r#"{"jsonrpc":"2.0","method":"notifications/message"}"#.to_string()
            ))
            .unwrap();
        sender
            .send(Ok(r#"{"jsonrpc":"2.0","id":8,"result":{}}"#.to_string()))
            .unwrap();
        sender
            .send(Ok(
                r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#.to_string()
            ))
            .unwrap();

        let response =
            read_json_rpc_response(&receiver, json!(7), "test", MCP_REQUEST_TIMEOUT).unwrap();

        assert_eq!(response["result"]["ok"], true);
    }

    fn snapshot_not_ready_error() -> anyhow::Error {
        McpResponseError {
            label: "Bifrost".to_string(),
            tool: "search_symbols".to_string(),
            code: SNAPSHOT_NOT_READY_CODE,
            message: SNAPSHOT_NOT_READY_MESSAGE.to_string(),
            data: None,
        }
        .into()
    }

    #[test]
    fn retries_snapshot_not_ready_then_succeeds() {
        let mut attempts = 0;
        let mut delays = Vec::new();

        let value = retry_snapshot_not_ready(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(snapshot_not_ready_error())
                } else {
                    Ok("ready")
                }
            },
            |delay| delays.push(delay),
            |_| {},
        )
        .unwrap();

        assert_eq!(value, "ready");
        assert_eq!(attempts, 3);
        assert_eq!(delays, SNAPSHOT_NOT_READY_RETRY_DELAYS[..2]);
    }

    #[cfg(unix)]
    #[test]
    fn retried_tool_calls_use_fresh_request_ids() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("cat >/dev/null")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Ok(format!(
                r#"{{"jsonrpc":"2.0","id":1,"error":{{"code":{SNAPSHOT_NOT_READY_CODE},"message":"{SNAPSHOT_NOT_READY_MESSAGE}"}}}}"#
            )))
            .unwrap();
        sender
            .send(Ok(
                r#"{"jsonrpc":"2.0","id":2,"result":{"structuredContent":{"ready":true}}}"#
                    .to_string(),
            ))
            .unwrap();
        let mut session = McpSession {
            label: "Bifrost".to_string(),
            child,
            stdin,
            stdout_lines: receiver,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
            request_timeout: MCP_REQUEST_TIMEOUT,
        };

        let result = retry_snapshot_not_ready(
            || session.call_tool_once("search_symbols", json!({})),
            |_| {},
            |_| {},
        )
        .unwrap();

        assert_eq!(result["ready"], true);
        assert_eq!(session.next_id, 3);
    }

    #[test]
    fn exhausts_bounded_snapshot_not_ready_retries() {
        let mut attempts = 0;

        let error = retry_snapshot_not_ready(
            || {
                attempts += 1;
                Err::<(), _>(snapshot_not_ready_error())
            },
            |_| {},
            |_| {},
        )
        .unwrap_err();

        assert!(is_snapshot_not_ready_error(&error));
        assert_eq!(attempts, SNAPSHOT_NOT_READY_RETRY_DELAYS.len() + 1);
        assert!(format!("{error:#}").contains("retries exhausted after 4 attempts"));
    }

    #[test]
    fn does_not_retry_other_internal_errors_or_similar_messages() {
        for error in [
            McpResponseError {
                label: "Bifrost".to_string(),
                tool: "search_symbols".to_string(),
                code: SNAPSHOT_NOT_READY_CODE,
                message: "different internal error".to_string(),
                data: None,
            },
            McpResponseError {
                label: "Bifrost".to_string(),
                tool: "search_symbols".to_string(),
                code: -32000,
                message: SNAPSHOT_NOT_READY_MESSAGE.to_string(),
                data: None,
            },
        ] {
            let mut attempts = 0;
            let result = retry_snapshot_not_ready(
                || {
                    attempts += 1;
                    Err::<(), _>(error.clone().into())
                },
                |_| panic!("non-readiness errors must not sleep"),
                |_| {},
            );
            assert!(result.is_err());
            assert_eq!(attempts, 1);
        }
    }
}
