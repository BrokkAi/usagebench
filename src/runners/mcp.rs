use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::{
    error::Error as StdError,
    fmt,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
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
// caller that wants a tighter bound on one call uses
// `ToolClient::call_tool_with_timeout`.
const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The runner's process envelope, exposed so tests choosing their own
/// deadline can check theirs still fits inside it.
#[cfg(test)]
pub(crate) fn request_envelope() -> Duration {
    MCP_REQUEST_TIMEOUT
}
const SNAPSHOT_NOT_READY_CODE: i64 = -32603;
const SNAPSHOT_NOT_READY_MESSAGE: &str =
    "workspace snapshot was not ready within the request-wide time budget; retry after workspace initialization completes";
const SNAPSHOT_NOT_READY_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
// Bifrost answers a cold-workspace call at once with an incomplete tool result
// instead of holding the request open, so the client polls until the logical
// call deadline. Short early polls keep warm fixtures fast.
const WORKSPACE_PREPARING_POLL_DELAYS: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];
const WORKSPACE_PREPARING_FINAL_POLL_RESERVE: Duration = Duration::from_millis(100);
const BIFROST_INCOMPLETE_META_KEY: &str = "io.bifrost/incomplete";
const MCP_STDERR_TAIL_BYTES: u64 = 64 * 1024;
const MCP_STDOUT_READER_JOIN_TIMEOUT: Duration = Duration::from_secs(1);
const MCP_STDOUT_READER_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

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

#[derive(Debug)]
struct McpRequestTimeout {
    label: String,
    timeout: Duration,
}

impl fmt::Display for McpRequestTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "timed out after {} seconds waiting for {} MCP response",
            self.timeout.as_secs(),
            self.label
        )
    }
}

impl StdError for McpRequestTimeout {}

#[derive(Debug)]
struct McpCleanupFailure {
    details: String,
}

impl fmt::Display for McpCleanupFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MCP session cleanup failed: {}", self.details)
    }
}

impl StdError for McpCleanupFailure {}

/// Bifrost's incomplete tool result for a call that arrived while workspace
/// preparation was still running. The preparation keeps going server side, so
/// the same call can be retried.
#[derive(Debug, Clone)]
struct McpWorkspacePreparing {
    label: String,
    tool: String,
    message: String,
    incomplete: Value,
}

impl McpWorkspacePreparing {
    fn from_tool_result(label: &str, tool: &str, result: &Value, message: &str) -> Option<Self> {
        let incomplete = result.get("_meta")?.get(BIFROST_INCOMPLETE_META_KEY)?;
        if incomplete
            .get("workspace_preparation")
            .and_then(Value::as_str)
            != Some("continues")
        {
            return None;
        }
        Some(Self {
            label: label.to_string(),
            tool: tool.to_string(),
            message: message.to_string(),
            incomplete: incomplete.clone(),
        })
    }
}

impl fmt::Display for McpWorkspacePreparing {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} tool `{}` failed: {}; incomplete: {}",
            self.label, self.tool, self.message, self.incomplete
        )
    }
}

impl StdError for McpWorkspacePreparing {}

/// A readiness failure that ended a call and is replayed for the rest of the
/// session, so later cases fail fast instead of each waiting out a deadline.
#[derive(Debug, Clone)]
enum WorkspaceNotReady {
    Snapshot(McpResponseError),
    Preparing(McpWorkspacePreparing),
}

impl WorkspaceNotReady {
    fn find(error: &anyhow::Error) -> Option<Self> {
        error.chain().find_map(|cause| {
            if let Some(error) = cause
                .downcast_ref::<McpResponseError>()
                .filter(|error| error.is_snapshot_not_ready())
            {
                return Some(Self::Snapshot(error.clone()));
            }
            cause
                .downcast_ref::<McpWorkspacePreparing>()
                .map(|error| Self::Preparing(error.clone()))
        })
    }
}

impl From<WorkspaceNotReady> for anyhow::Error {
    fn from(error: WorkspaceNotReady) -> Self {
        match error {
            WorkspaceNotReady::Snapshot(error) => error.into(),
            WorkspaceNotReady::Preparing(error) => error.into(),
        }
    }
}

fn is_workspace_preparing_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<McpWorkspacePreparing>().is_some())
}

pub(crate) fn is_cleanup_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<McpCleanupFailure>().is_some())
}

/// Whether a call failed because the analyzer workspace was not ready, either
/// through the legacy snapshot error or Bifrost's incomplete readiness result.
pub(crate) fn is_snapshot_not_ready_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<McpResponseError>()
            .is_some_and(McpResponseError::is_snapshot_not_ready)
            || cause.downcast_ref::<McpWorkspacePreparing>().is_some()
    })
}

pub(crate) trait ToolClient {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value>;

    /// Bound the wall clock of this call without changing later calls.
    fn call_tool_with_timeout(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value>;
}

pub(crate) struct McpSession {
    label: String,
    child: Child,
    stdin: ChildStdin,
    stdout_lines: Receiver<Result<String, String>>,
    stdout_reader: Option<thread::JoinHandle<()>>,
    stderr_path: PathBuf,
    finished: bool,
    cleanup_failure: Option<String>,
    next_id: u64,
    snapshot_not_ready: Option<WorkspaceNotReady>,
    workspace_readiness_duration: Duration,
}

impl McpSession {
    /// Start an MCP server while retaining all stderr at `evidence_path`.
    /// The path must not already exist; this prevents silently overwriting
    /// evidence from another run.
    pub(crate) fn start_with_evidence(
        command: &mut Command,
        label: impl Into<String>,
        evidence_path: &Path,
    ) -> Result<Self> {
        let stderr_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(evidence_path)
            .with_context(|| {
                format!(
                    "create MCP stderr evidence file {}",
                    evidence_path.display()
                )
            })?;
        let stderr_path = evidence_path.to_path_buf();
        let label = label.into();
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_file))
            .spawn()
            .with_context(|| format!("spawn {label} MCP server"))?;
        let stdin = child.stdin.take().context("missing MCP stdin")?;
        let stdout = child.stdout.take().context("missing MCP stdout")?;
        let (sender, stdout_lines) = mpsc::channel();
        let reader_label = label.clone();
        let reader_stderr_path = stderr_path.clone();
        let stdout_reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let message = match read_stderr_tail(&reader_stderr_path) {
                            Ok(stderr) if stderr.trim().is_empty() => {
                                format!("{reader_label} MCP server closed stdout")
                            }
                            Ok(stderr) => format!(
                                "{reader_label} MCP server closed stdout; stderr: {}",
                                stderr.trim()
                            ),
                            Err(error) => format!(
                                "{reader_label} MCP server closed stdout; failed to read stderr log {}: {error:#}",
                                reader_stderr_path.display()
                            ),
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
            stdout_reader: Some(stdout_reader),
            stderr_path,
            finished: false,
            cleanup_failure: None,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
        })
    }

    /// Stop the server and join its stdout reader before the caller removes
    /// any workspace state the server may have been using.
    pub(crate) fn finish(&mut self) -> Result<()> {
        if self.finished {
            return match &self.cleanup_failure {
                Some(details) => Err(McpCleanupFailure {
                    details: details.clone(),
                }
                .into()),
                None => Ok(()),
            };
        }

        let mut errors = Vec::new();
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = self.child.kill() {
                    // `kill` reports InvalidInput when the process exited in
                    // the small interval after try_wait; wait below confirms
                    // and reaps it.
                    if error.kind() != std::io::ErrorKind::InvalidInput {
                        errors.push(
                            anyhow!(error).context(format!("kill {} MCP server", self.label)),
                        );
                    }
                }
            }
            Err(error) => {
                errors.push(
                    anyhow!(error).context(format!("check {} MCP server status", self.label)),
                );
                if let Err(error) = self.child.kill() {
                    if error.kind() != std::io::ErrorKind::InvalidInput {
                        errors.push(
                            anyhow!(error).context(format!("kill {} MCP server", self.label)),
                        );
                    }
                }
            }
        }

        let waited = match self.child.wait() {
            Ok(_) => true,
            Err(error) => {
                errors.push(anyhow!(error).context(format!("wait for {} MCP server", self.label)));
                false
            }
        };
        if waited && !self.stderr_path.as_os_str().is_empty() {
            if let Err(error) = sync_stderr_evidence(&self.stderr_path) {
                errors.push(error.context(format!("sync {} MCP stderr evidence", self.label)));
            }
        }
        if let Some(stdout_reader) = self.stdout_reader.take() {
            let deadline = Instant::now() + MCP_STDOUT_READER_JOIN_TIMEOUT;
            while !stdout_reader.is_finished() && Instant::now() < deadline {
                thread::sleep(MCP_STDOUT_READER_JOIN_POLL_INTERVAL);
            }
            if stdout_reader.is_finished() {
                if stdout_reader.join().is_err() {
                    errors.push(anyhow!("stdout reader thread panicked"));
                }
            } else {
                // A descendant may have inherited stdout and kept the pipe
                // open after the MCP process exited. Do not let that block
                // workspace cleanup; this reader only reads stdout and never
                // writes the retained stderr evidence file.
                let (_sender, replacement_receiver) = mpsc::channel();
                drop(std::mem::replace(
                    &mut self.stdout_lines,
                    replacement_receiver,
                ));
                drop(stdout_reader);
                errors.push(anyhow!(
                    "stdout reader did not stop within {} ms after {} MCP server exited",
                    MCP_STDOUT_READER_JOIN_TIMEOUT.as_millis(),
                    self.label
                ));
            }
        }
        self.finished = waited && self.stdout_reader.is_none();

        if errors.is_empty() {
            self.cleanup_failure = None;
            Ok(())
        } else {
            let primary = errors.remove(0);
            let mut details = vec![format!("{primary:#}")];
            details.extend(errors.iter().map(|error| format!("{error:#}")));
            let details = format!(
                "finish {} MCP session: {}",
                self.label,
                details.join("; additional cleanup errors: ")
            );
            if self.finished {
                self.cleanup_failure = Some(details.clone());
            }
            Err(McpCleanupFailure { details }.into())
        }
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
        self.request_with_timeout(payload, MCP_REQUEST_TIMEOUT)
    }

    /// List the server's tool descriptors via the standard `tools/list`
    /// request.
    ///
    /// The descriptor set is how an MCP server states which arguments each of
    /// its tools accepts, so a caller that must adapt to the server it was
    /// given probes this once per session instead of keeping a version table.
    pub(crate) fn tool_descriptors(&mut self) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        }))?;
        if let Some(error) = response.get("error") {
            bail!("{} tools/list failed: {error}", self.label);
        }
        response
            .get("result")
            .and_then(|result| result.get("tools"))
            .cloned()
            .with_context(|| format!("{} tools/list response lacks result.tools", self.label))
    }

    fn request_with_timeout(&mut self, payload: Value, timeout: Duration) -> Result<Value> {
        let expected_id = payload
            .get("id")
            .cloned()
            .context("JSON-RPC request missing id")?;
        self.write_line(&payload)?;
        match read_json_rpc_response(
            &self.stdout_lines,
            expected_id.clone(),
            &self.label,
            timeout,
        ) {
            Err(error) if error.downcast_ref::<McpRequestTimeout>().is_some() => {
                let cancellation = json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": {
                        "requestId": expected_id,
                        "reason": format!(
                            "UsageBench request timed out after {} seconds",
                            timeout.as_secs()
                        ),
                    }
                });
                if let Err(cancel_error) = self.notify(cancellation) {
                    return Err(error).context(format!(
                        "also failed to cancel timed-out {} MCP request: {cancel_error:#}",
                        self.label
                    ));
                }
                Err(error)
            }
            result => result,
        }
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
            return Err(McpRequestTimeout {
                label: label.to_string(),
                timeout,
            }
            .into());
        }
        let line = match stdout_lines.recv_timeout(remaining) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                return Err(McpRequestTimeout {
                    label: label.to_string(),
                    timeout,
                }
                .into());
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("{label} MCP response channel disconnected")
            }
        }
        .map_err(|message| anyhow!(message))?;
        let response: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse MCP JSON response: {line}"))?;
        if response.get("id") == Some(&expected_id) {
            return Ok(response);
        }
    }
}

fn read_stderr_tail(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let length = file
        .metadata()
        .with_context(|| format!("read metadata for {}", path.display()))?
        .len();
    let tail_length = length.min(MCP_STDERR_TAIL_BYTES);
    file.seek(SeekFrom::End(-(tail_length as i64)))
        .with_context(|| format!("seek to stderr tail in {}", path.display()))?;
    let mut bytes = Vec::with_capacity(tail_length as usize);
    file.take(MCP_STDERR_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read stderr tail from {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn sync_stderr_evidence(path: &Path) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .open(path)
        .with_context(|| format!("open {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))
}

impl ToolClient for McpSession {
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        self.call_tool_with_timeout(name, arguments, MCP_REQUEST_TIMEOUT)
    }

    fn call_tool_with_timeout(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        if let Some(error) = &self.snapshot_not_ready {
            return Err(error.clone().into());
        }
        // A benchmark may tighten a request deadline, but may not widen the
        // runner's process envelope.
        let timeout = timeout.min(MCP_REQUEST_TIMEOUT);
        let mut readiness_duration = Duration::ZERO;
        let label = self.label.clone();
        let result = retry_snapshot_not_ready_until(
            timeout,
            |remaining| self.call_tool_once(name, arguments.clone(), remaining),
            thread::sleep,
            |duration| readiness_duration += duration,
            || {
                McpRequestTimeout {
                    label: label.clone(),
                    timeout,
                }
                .into()
            },
        );
        self.workspace_readiness_duration += readiness_duration;
        if let Err(error) = &result {
            self.snapshot_not_ready = WorkspaceNotReady::find(error);
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

    fn call_tool_once(&mut self, name: &str, arguments: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self.request_with_timeout(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments,
                }
            }),
            timeout,
        )?;
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
            if let Some(preparing) =
                McpWorkspacePreparing::from_tool_result(&self.label, name, result, message)
            {
                return Err(preparing.into());
            }
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

#[cfg(test)]
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

fn retry_snapshot_not_ready_until<T>(
    timeout: Duration,
    mut operation: impl FnMut(Duration) -> Result<T>,
    mut sleep: impl FnMut(Duration),
    mut record_readiness: impl FnMut(Duration),
    timeout_error: impl Fn() -> anyhow::Error,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    let mut attempted = false;
    let mut snapshot_retries = 0;
    let mut preparing_polls = 0;
    let mut final_preparing_poll = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if attempted && remaining.is_zero() {
            return Err(timeout_error());
        }
        attempted = true;
        let attempt_started = Instant::now();
        let (error, delay) = match operation(remaining) {
            Err(error) if is_workspace_preparing_error(&error) => {
                let delay = WORKSPACE_PREPARING_POLL_DELAYS
                    [preparing_polls.min(WORKSPACE_PREPARING_POLL_DELAYS.len() - 1)];
                preparing_polls += 1;
                (error, delay)
            }
            Err(error) if is_snapshot_not_ready_error(&error) => {
                let Some(&delay) = SNAPSHOT_NOT_READY_RETRY_DELAYS.get(snapshot_retries) else {
                    return Err(error).with_context(|| {
                        format!(
                            "workspace readiness retries exhausted after {} attempts",
                            SNAPSHOT_NOT_READY_RETRY_DELAYS.len() + 1
                        )
                    });
                };
                snapshot_retries += 1;
                (error, delay)
            }
            result => return result,
        };
        let attempt_duration = attempt_started.elapsed();
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !is_workspace_preparing_error(&error) {
            if remaining.is_zero() {
                return Err(timeout_error());
            }
            let bounded_delay = delay.min(remaining);
            sleep(bounded_delay);
            record_readiness(attempt_duration.saturating_add(bounded_delay));
            if bounded_delay < delay {
                return Err(timeout_error());
            }
            continue;
        }
        if final_preparing_poll || remaining <= WORKSPACE_PREPARING_FINAL_POLL_RESERVE {
            // Keep the typed readiness error so the session stops retrying it
            // and the report names the real cause.
            record_readiness(attempt_duration);
            return Err(error).with_context(|| {
                format!(
                    "workspace preparation did not finish within {} seconds ({} polls)",
                    timeout.as_secs(),
                    preparing_polls
                )
            });
        }
        // When less than a full poll interval is left, wait out the window
        // but keep a reserve so one last poll still runs before the deadline.
        let window = remaining - WORKSPACE_PREPARING_FINAL_POLL_RESERVE;
        final_preparing_poll = window <= delay;
        let bounded_delay = delay.min(window);
        sleep(bounded_delay);
        record_readiness(attempt_duration.saturating_add(bounded_delay));
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Err(error) = self.finish() {
            eprintln!("failed to finish {} MCP session: {error:#}", self.label);
        }
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

    #[cfg(unix)]
    #[test]
    fn timed_out_call_is_cancelled_without_poisoning_the_next_call() {
        let tempdir = tempfile::tempdir().unwrap();
        let requests_path = tempdir.path().join("requests.jsonl");
        let mut child = Command::new("sh")
            .args(["-c", "tee \"$1\" >/dev/null", "usagebench-mcp-test"])
            .arg(&requests_path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Ok(
                r#"{"jsonrpc":"2.0","id":1,"result":{"structuredContent":{"late":true}}}"#
                    .to_string(),
            ))
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
            stdout_reader: None,
            stderr_path: PathBuf::new(),
            finished: false,
            cleanup_failure: None,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
        };

        let error = session
            .call_tool_with_timeout("scan_usages_by_location", json!({}), Duration::ZERO)
            .unwrap_err();
        assert!(error.downcast_ref::<McpRequestTimeout>().is_some());

        let next = session.call_tool("search_symbols", json!({})).unwrap();
        assert_eq!(next["ready"], true);
        session.stdin.flush().unwrap();

        let mut written = None;
        for _ in 0..100 {
            if let Ok(contents) = std::fs::read_to_string(&requests_path) {
                if contents.lines().count() == 3 {
                    written = Some(contents);
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        let written = written.expect("MCP requests were not written");
        let messages = written
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(messages[0]["id"], 1);
        assert_eq!(messages[0]["params"]["name"], "scan_usages_by_location");
        assert_eq!(messages[1]["method"], "notifications/cancelled");
        assert_eq!(messages[1]["params"]["requestId"], 1);
        assert_eq!(
            messages[1]["params"]["reason"],
            "UsageBench request timed out after 0 seconds"
        );
        assert!(messages[1].get("id").is_none());
        assert_eq!(messages[2]["id"], 2);
        assert_eq!(messages[2]["params"]["name"], "search_symbols");
    }

    #[cfg(unix)]
    #[test]
    fn readiness_retries_share_one_logical_call_deadline() {
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
        let mut session = McpSession {
            label: "Bifrost".to_string(),
            child,
            stdin,
            stdout_lines: receiver,
            stdout_reader: None,
            stderr_path: PathBuf::new(),
            finished: false,
            cleanup_failure: None,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
        };

        let started = Instant::now();
        let error = session
            .call_tool_with_timeout(
                "scan_usages_by_location",
                json!({}),
                Duration::from_millis(20),
            )
            .unwrap_err();

        assert!(error.downcast_ref::<McpRequestTimeout>().is_some());
        assert_eq!(session.next_id, 2, "deadline must prevent a second attempt");
        assert!(started.elapsed() < Duration::from_secs(1));
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
            stdout_reader: None,
            stderr_path: PathBuf::new(),
            finished: false,
            cleanup_failure: None,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
        };

        let result = retry_snapshot_not_ready(
            || session.call_tool_once("search_symbols", json!({}), MCP_REQUEST_TIMEOUT),
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

    #[cfg(unix)]
    fn session_with_responses(responses: &[String]) -> McpSession {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("cat >/dev/null")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        for response in responses {
            sender.send(Ok(response.clone())).unwrap();
        }
        McpSession {
            label: "Bifrost".to_string(),
            child,
            stdin,
            stdout_lines: receiver,
            stdout_reader: None,
            stderr_path: PathBuf::new(),
            finished: false,
            cleanup_failure: None,
            next_id: 1,
            snapshot_not_ready: None,
            workspace_readiness_duration: Duration::ZERO,
        }
    }

    /// The tool result Bifrost returns when a call arrives before the
    /// workspace analyzer is ready (rmcp_host `workspace_not_ready_result`).
    fn workspace_not_ready_response(id: u64) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": true,
                "content": [{
                    "type": "text",
                    "text": "workspace not ready at workspace_readiness while preparing workspace analyzer; workspace preparation continues; check get_active_workspace or retry"
                }],
                "_meta": {
                    "io.bifrost/incomplete": {
                        "status": "inconclusive",
                        "completion": {"type": "inconclusive", "reasons": ["workspace_not_ready"]},
                        "stage": "workspace_readiness",
                        "phase": "preparing workspace analyzer",
                        "partial": false,
                        "workspace_preparation": "continues"
                    }
                }
            }
        })
        .to_string()
    }

    #[cfg(unix)]
    #[test]
    fn polls_workspace_not_ready_result_until_ready() {
        let mut session = session_with_responses(&[
            workspace_not_ready_response(1),
            workspace_not_ready_response(2),
            r#"{"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"ready":true}}}"#.to_string(),
        ]);

        let result = session.call_tool("search_symbols", json!({})).unwrap();

        assert_eq!(result["ready"], true);
        assert_eq!(session.next_id, 4);
        assert!(session.snapshot_not_ready.is_none());
        assert!(session.take_workspace_readiness_duration() > Duration::ZERO);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_still_preparing_at_deadline_latches_typed_error() {
        let responses: Vec<String> = (1..=20).map(workspace_not_ready_response).collect();
        let mut session = session_with_responses(&responses);

        let error = session
            .call_tool_with_timeout("search_symbols", json!({}), Duration::from_millis(400))
            .unwrap_err();

        assert!(is_snapshot_not_ready_error(&error));
        assert!(format!("{error:#}").contains("workspace preparation did not finish"));
        let attempts = session.next_id;
        let replayed = session
            .call_tool("get_definitions_by_location", json!({}))
            .unwrap_err();
        assert!(is_snapshot_not_ready_error(&replayed));
        assert_eq!(session.next_id, attempts, "latched error must not re-query");
        assert!(session.take_snapshot_not_ready_error().is_some());
    }

    #[test]
    fn polls_once_more_when_less_than_a_poll_interval_remains() {
        let preparing = || -> anyhow::Error {
            McpWorkspacePreparing {
                label: "Bifrost".to_string(),
                tool: "search_symbols".to_string(),
                message: "workspace not ready".to_string(),
                incomplete: json!({"workspace_preparation": "continues"}),
            }
            .into()
        };
        let mut attempts = 0;
        let mut delays = Vec::new();

        // 400 ms leaves room for the 250 ms first poll delay but not the
        // 500 ms second one; the shortened tail must still end in a poll.
        let value = retry_snapshot_not_ready_until(
            Duration::from_millis(400),
            |_| {
                attempts += 1;
                if attempts < 3 {
                    Err(preparing())
                } else {
                    Ok("ready")
                }
            },
            |delay| {
                delays.push(delay);
                thread::sleep(delay);
            },
            |_| {},
            || anyhow!("timed out"),
        )
        .unwrap();

        assert_eq!(value, "ready");
        assert_eq!(attempts, 3);
        assert_eq!(delays[0], WORKSPACE_PREPARING_POLL_DELAYS[0]);
        assert!(delays[1] < WORKSPACE_PREPARING_POLL_DELAYS[1]);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_retry_tool_errors_without_workspace_preparation() {
        let mut session = session_with_responses(&[json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "isError": true,
                "content": [{"type": "text", "text": "request deadline exceeded at execution"}],
                "_meta": {
                    "io.bifrost/incomplete": {
                        "status": "inconclusive",
                        "stage": "execution",
                        "partial": false
                    }
                }
            }
        })
        .to_string()]);

        let error = session.call_tool("search_symbols", json!({})).unwrap_err();

        assert!(!is_snapshot_not_ready_error(&error));
        assert_eq!(session.next_id, 2);
        assert!(session.snapshot_not_ready.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn stderr_evidence_survives_finish_after_large_write_and_rpc_reply() {
        let tempdir = tempfile::tempdir().unwrap();
        let evidence_path = tempdir.path().join("mcp.stderr.log");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "IFS= read -r request; head -c 131072 /dev/zero | tr '\\000' x >&2; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"structuredContent\":{\"ok\":true}}}'; exec cat >/dev/null",
        ]);
        let mut session =
            McpSession::start_with_evidence(&mut command, "test", &evidence_path).unwrap();

        let result = session.call_tool("search_symbols", json!({})).unwrap();
        assert_eq!(result["ok"], true);
        session.finish().unwrap();

        let evidence = std::fs::read(&evidence_path).unwrap();
        assert!(evidence.len() >= 131_072);
        assert!(evidence.iter().all(|byte| *byte == b'x'));
    }

    #[cfg(unix)]
    #[test]
    fn abnormal_stdout_eof_includes_bounded_stderr_tail_and_retains_evidence() {
        let tempdir = tempfile::tempdir().unwrap();
        let evidence_path = tempdir.path().join("mcp.stderr.log");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "IFS= read -r request; head -c 131072 /dev/zero | tr '\\000' z >&2; printf 'abnormal-evidence-marker\\n' >&2; exit 19",
        ]);
        let mut session =
            McpSession::start_with_evidence(&mut command, "test", &evidence_path).unwrap();

        let error = session
            .call_tool_with_timeout("search_symbols", json!({}), Duration::from_secs(2))
            .unwrap_err();
        assert!(format!("{error:#}").contains("abnormal-evidence-marker"));
        session.finish().unwrap();

        let evidence = std::fs::read_to_string(&evidence_path).unwrap();
        assert!(evidence.len() > 64 * 1024);
        assert!(evidence.ends_with("abnormal-evidence-marker\n"));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_evidence_survives_finish() {
        let tempdir = tempfile::tempdir().unwrap();
        let evidence_path = tempdir.path().join("mcp.stderr.log");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf 'timeout-evidence-marker\\n' >&2; exec sleep 30",
        ]);
        let mut session =
            McpSession::start_with_evidence(&mut command, "test", &evidence_path).unwrap();

        let error = session
            .call_tool_with_timeout("search_symbols", json!({}), Duration::from_millis(30))
            .unwrap_err();
        assert!(error.downcast_ref::<McpRequestTimeout>().is_some());
        session.finish().unwrap();

        let evidence = std::fs::read_to_string(&evidence_path).unwrap();
        assert!(evidence.contains("timeout-evidence-marker"));
    }

    #[cfg(unix)]
    #[test]
    fn inherited_stdout_cleanup_failure_is_typed_and_bounded() {
        let tempdir = tempfile::tempdir().unwrap();
        let evidence_path = tempdir.path().join("mcp.stderr.log");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf 'descendant-evidence-marker\\n' >&2; sleep 3 & exit 0",
        ]);
        let mut session =
            McpSession::start_with_evidence(&mut command, "test", &evidence_path).unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut child_exited = false;
        while Instant::now() < deadline {
            if session.child.try_wait().unwrap().is_some() {
                child_exited = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(child_exited, "MCP shell did not exit before its descendant");

        let finish_started = Instant::now();
        let error = session.finish().unwrap_err();
        assert!(is_cleanup_error(&error));
        assert!(format!("{error:#}").contains("stdout reader did not stop"));
        assert!(finish_started.elapsed() < Duration::from_secs(2));
        assert!(std::fs::read_to_string(&evidence_path)
            .unwrap()
            .contains("descendant-evidence-marker"));
    }
}
