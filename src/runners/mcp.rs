use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) trait ToolClient {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value>;
}

pub(crate) struct McpSession {
    label: String,
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Receiver<Result<String, String>>,
    next_id: u64,
}

impl McpSession {
    pub(crate) fn start(command: &mut Command, label: impl Into<String>) -> Result<Self> {
        let label = label.into();
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn {label} MCP server"))?;
        let stdin = child.stdin.take().context("missing MCP stdin")?;
        let stdout = child.stdout.take().context("missing MCP stdout")?;
        let (sender, stdout_lines) = mpsc::channel();
        let reader_label = label.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ =
                            sender.send(Err(format!("{reader_label} MCP server closed stdout")));
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
        read_json_rpc_response(&self.stdout_lines, expected_id, &self.label)
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
) -> Result<Value> {
    loop {
        let line = stdout_lines
            .recv_timeout(MCP_REQUEST_TIMEOUT)
            .with_context(|| {
                format!(
                    "timed out after {} seconds waiting for {label} MCP response",
                    MCP_REQUEST_TIMEOUT.as_secs()
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
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
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

        let response = read_json_rpc_response(&receiver, json!(7), "test").unwrap();

        assert_eq!(response["result"]["ok"], true);
    }
}
