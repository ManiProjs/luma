use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::time::Duration;

use super::{McpServerConfig, McpTransport};

const PROTOCOL_VERSION: &str = "2025-03-26";
const IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Optional server-provided guidance for the model; falls back to description.
    #[serde(default, rename = "modelGuide")]
    pub model_guide: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

struct McpProcess {
    child: Child,
    input: ChildStdin,
    output: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

impl McpProcess {
    async fn start(server: &McpServerConfig) -> Result<Self> {
        let mut command = tokio::process::Command::new(&server.command);
        command
            .env_clear()
            .args(&server.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        for key in ["PATH", "HOME", "TMPDIR", "TEMP", "TMP", "SYSTEMROOT"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        for (key, value) in &server.env {
            let resolved = if value.starts_with("${") && value.ends_with('}') {
                let variable = &value[2..value.len() - 1];
                std::env::var(variable).with_context(|| {
                    format!(
                        "MCP server '{}' requires environment variable {variable}",
                        server.id
                    )
                })?
            } else {
                value.clone()
            };
            command.env(key, resolved);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("starting MCP server '{}'", server.id))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("MCP stdin was unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("MCP stdout was unavailable"))?;
        let mut process = Self {
            child,
            input,
            output: BufReader::new(output).lines(),
        };
        process.initialize().await?;
        Ok(process)
    }

    async fn initialize(&mut self) -> Result<()> {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "Luma", "version": env!("CARGO_PKG_VERSION") }
            }
        }))
        .await?;
        let response = self.read_response(1).await?;
        if let Some(error) = response.get("error") {
            bail!("MCP initialize failed: {error}");
        }
        let negotiated_version = response
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("MCP initialize response omitted protocolVersion"))?;
        if negotiated_version != PROTOCOL_VERSION {
            bail!("MCP server negotiated unsupported protocol version {negotiated_version}");
        }
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await?;
        Ok(())
    }

    async fn send(&mut self, value: Value) -> Result<()> {
        let line = serde_json::to_vec(&value)?;
        if line.len() > 1_000_000 {
            bail!("MCP JSON-RPC message exceeded the 1 MB limit");
        }
        tokio::time::timeout(IO_TIMEOUT, async {
            self.input.write_all(&line).await?;
            self.input.write_all(b"\n").await?;
            self.input.flush().await
        })
        .await
        .context("timed out writing to MCP server")??;
        Ok(())
    }

    async fn read_response(&mut self, id: u64) -> Result<Value> {
        loop {
            let line = tokio::time::timeout(IO_TIMEOUT, self.output.next_line())
                .await
                .context("timed out waiting for MCP response")??
                .ok_or_else(|| anyhow!("MCP server closed stdout before responding"))?;
            if line.trim().is_empty() {
                continue;
            }
            if line.len() > 1_000_000 {
                bail!("MCP server response exceeded the 1 MB limit");
            }
            let value: Value =
                serde_json::from_str(&line).context("MCP server emitted invalid JSON-RPC")?;
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
            // Notifications and unrelated server requests are ignored for
            // now; Luma does not advertise server-side client capabilities.
        }
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        self.send(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }))
            .await?;
        let response = self.read_response(2).await?;
        if let Some(error) = response.get("error") {
            bail!("MCP tools/list failed: {error}");
        }
        let tools = response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        tools
            .into_iter()
            .map(|tool| serde_json::from_value(tool).context("invalid MCP tool definition"))
            .collect()
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String> {
        self.send(json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } })).await?;
        let response = self.read_response(3).await?;
        if let Some(error) = response.get("error") {
            bail!("MCP tools/call failed: {error}");
        }
        let result = response
            .get("result")
            .ok_or_else(|| anyhow!("MCP tools/call returned no result"))?;
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            let message = result
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("MCP tool returned an error: {message}");
        }
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.to_string());
        }
        let text = result
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }
}

struct McpHttpClient {
    client: reqwest::Client,
    server: McpServerConfig,
    session_id: Option<String>,
}

#[derive(Debug)]
struct HttpStatus(reqwest::StatusCode);

impl std::fmt::Display for HttpStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "MCP HTTP server returned {}", self.0)
    }
}

impl std::error::Error for HttpStatus {}

fn should_reinitialize_after_404(status: reqwest::StatusCode, has_session: bool) -> bool {
    status == reqwest::StatusCode::NOT_FOUND && has_session
}

fn session_delete_status_is_acceptable(status: reqwest::StatusCode) -> bool {
    status.is_success()
        || matches!(
            status,
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
        )
}

fn preserve_operation_error<T>(operation: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (operation, cleanup) {
        (Err(operation_error), _) => Err(operation_error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

impl McpHttpClient {
    fn new(server: McpServerConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(IO_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            server,
            session_id: None,
        })
    }

    fn url(&self) -> Result<&str> {
        self.server
            .url
            .as_deref()
            .ok_or_else(|| anyhow!("MCP HTTP server URL is missing"))
    }

    fn headers(&self, initialized: bool) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream".parse()?,
        );
        headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse()?);
        if initialized {
            headers.insert("mcp-protocol-version", PROTOCOL_VERSION.parse()?);
        }
        if let Some(session_id) = &self.session_id {
            headers.insert("mcp-session-id", session_id.parse()?);
        }
        for (name, value) in &self.server.headers {
            let parsed_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())?;
            if matches!(
                parsed_name.as_str(),
                "accept" | "content-type" | "mcp-protocol-version" | "mcp-session-id"
            ) {
                bail!(
                    "MCP custom header '{}' is managed by the HTTP transport",
                    name
                );
            }
            let resolved = if value.starts_with("${") && value.ends_with('}') {
                let variable = &value[2..value.len() - 1];
                std::env::var(variable).with_context(|| {
                    format!(
                        "MCP server '{}' requires environment variable {variable}",
                        self.server.id
                    )
                })?
            } else {
                value.clone()
            };
            headers.insert(
                parsed_name,
                reqwest::header::HeaderValue::from_str(&resolved)?,
            );
        }
        Ok(headers)
    }

    async fn post(&mut self, message: Value, initialized: bool) -> Result<Option<Value>> {
        let retry_message = message.clone();
        match self.post_once(message, initialized).await {
            Err(error)
                if error
                    .downcast_ref::<HttpStatus>()
                    .is_some_and(|status| status.0 == reqwest::StatusCode::NOT_FOUND) =>
            {
                self.session_id = None;
                self.initialize_once().await?;
                self.post_once(retry_message, true).await
            }
            result => result,
        }
    }

    async fn post_once(&mut self, message: Value, initialized: bool) -> Result<Option<Value>> {
        let expected_id = message.get("id").cloned();
        let body = serde_json::to_vec(&message)?;
        if body.len() > 1_000_000 {
            bail!("MCP JSON-RPC message exceeded the 1 MB limit");
        }
        let response = self
            .client
            .post(self.url()?)
            .headers(self.headers(initialized)?)
            .body(body)
            .send()
            .await
            .with_context(|| format!("sending request to MCP HTTP server '{}'", self.server.id))?;
        if response.status() == reqwest::StatusCode::ACCEPTED {
            return Ok(None);
        }
        let status = response.status();
        if should_reinitialize_after_404(status, self.session_id.is_some()) {
            return Err(HttpStatus(status).into());
        }
        if let Some(session_id) = response.headers().get("mcp-session-id") {
            self.session_id = Some(session_id.to_str()?.to_owned());
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !status.is_success() {
            let mut response = response;
            let mut detail = Vec::new();
            while detail.len() < 2048 {
                let Some(chunk) = response.chunk().await? else {
                    break;
                };
                let remaining = 2048 - detail.len();
                detail.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            }
            let detail = String::from_utf8_lossy(&detail);
            bail!(
                "MCP HTTP server returned {status}: {}",
                truncate(&detail, 2048)
            );
        }
        if content_type.starts_with("application/json") {
            let bytes = read_limited_body(response, 1_000_000).await?;
            let value: Value =
                serde_json::from_slice(&bytes).context("MCP HTTP server returned invalid JSON")?;
            if let Some(expected_id) = expected_id {
                validate_json_rpc_response(&value, &expected_id)?;
            }
            return Ok(Some(value));
        }
        if content_type.starts_with("text/event-stream") {
            let Some(expected_id) = expected_id else {
                return Ok(None);
            };
            return read_sse_response(response, &expected_id).await.map(Some);
        }
        bail!("MCP HTTP server used unsupported content type '{content_type}'");
    }

    async fn initialize(&mut self) -> Result<()> {
        self.initialize_once().await
    }

    async fn initialize_once(&mut self) -> Result<()> {
        let response = self
            .post_once(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": {},
                        "clientInfo": { "name": "Luma", "version": env!("CARGO_PKG_VERSION") }
                    }
                }),
                false,
            )
            .await?
            .ok_or_else(|| anyhow!("MCP HTTP initialize returned 202 without a response"))?;
        if let Some(error) = response.get("error") {
            bail!("MCP HTTP initialize failed: {error}");
        }
        let version = response
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("MCP HTTP initialize response omitted protocolVersion"))?;
        if version != PROTOCOL_VERSION {
            bail!("MCP HTTP server negotiated unsupported protocol version {version}");
        }
        let _ = self
            .post_once(
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                true,
            )
            .await?;
        Ok(())
    }

    async fn close_session(&mut self) -> Result<()> {
        if self.session_id.is_none() {
            return Ok(());
        }
        let response = self
            .client
            .delete(self.url()?)
            .headers(self.headers(true)?)
            .send()
            .await
            .with_context(|| format!("closing MCP HTTP session for '{}'", self.server.id));
        let result = match response {
            Ok(response) if session_delete_status_is_acceptable(response.status()) => Ok(()),
            Ok(response) => Err(anyhow!(
                "MCP HTTP session DELETE returned unexpected status {}",
                response.status()
            )),
            Err(error) => Err(error),
        };
        self.session_id = None;
        result
    }

    async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut request_id = 2_u64;
        for _ in 0..100 {
            let mut params = serde_json::Map::new();
            if let Some(cursor) = &cursor {
                params.insert("cursor".into(), Value::String(cursor.clone()));
            }
            let response = self
                .post(
                    json!({ "jsonrpc": "2.0", "id": request_id, "method": "tools/list", "params": params }),
                    true,
                )
                .await?
                .ok_or_else(|| anyhow!("MCP HTTP tools/list returned no response"))?;
            if let Some(error) = response.get("error") {
                bail!("MCP HTTP tools/list failed: {error}");
            }
            let (mut page_tools, next_cursor) = parse_tools_page(&response)?;
            tools.append(&mut page_tools);
            cursor = next_cursor;
            if cursor.is_none() {
                return Ok(tools);
            }
            request_id = request_id
                .checked_add(1)
                .ok_or_else(|| anyhow!("MCP request id overflow"))?;
        }
        bail!("MCP HTTP tools/list exceeded the 100-page pagination limit")
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String> {
        let response = self
            .post(
                json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
                true,
            )
            .await?
            .ok_or_else(|| anyhow!("MCP HTTP tools/call returned no response"))?;
        response_text(response)
    }
}

async fn read_limited_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            bail!("MCP HTTP response exceeded the {} byte limit", limit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn read_sse_response(mut response: reqwest::Response, expected_id: &Value) -> Result<Value> {
    let mut parser = SseParser::default();
    let mut total = 0_usize;
    while let Some(chunk) = response.chunk().await? {
        total = total.saturating_add(chunk.len());
        if total > 1_000_000 {
            bail!("MCP HTTP response exceeded the 1 MB limit");
        }
        if let Some(value) = parser.push(&chunk, expected_id)? {
            return Ok(value);
        }
    }
    if let Some(value) = parser.finish(expected_id)? {
        return Ok(value);
    }
    bail!("MCP SSE response ended without a JSON-RPC response matching id {expected_id}")
}

#[derive(Default)]
struct SseParser {
    line: Vec<u8>,
    data_lines: Vec<String>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8], expected_id: &Value) -> Result<Option<Value>> {
        for byte in chunk {
            if *byte == b'\n' {
                if self.line.last() == Some(&b'\r') {
                    self.line.pop();
                }
                if self.line.is_empty() {
                    if let Some(value) = parse_sse_event(&self.data_lines, expected_id)? {
                        return Ok(Some(value));
                    }
                    self.data_lines.clear();
                } else if self.line.starts_with(b"data:") {
                    let data = self.line.strip_prefix(b"data:").unwrap_or_default();
                    let data = data.strip_prefix(b" ").unwrap_or(data);
                    self.data_lines.push(
                        String::from_utf8(data.to_vec()).context("MCP SSE data was not UTF-8")?,
                    );
                }
                self.line.clear();
            } else {
                self.line.push(*byte);
            }
        }
        Ok(None)
    }

    fn finish(&mut self, expected_id: &Value) -> Result<Option<Value>> {
        if !self.line.is_empty() {
            if self.line.last() == Some(&b'\r') {
                self.line.pop();
            }
            if self.line.starts_with(b"data:") {
                let data = self.line.strip_prefix(b"data:").unwrap_or_default();
                let data = data.strip_prefix(b" ").unwrap_or(data);
                self.data_lines
                    .push(String::from_utf8(data.to_vec()).context("MCP SSE data was not UTF-8")?);
            }
        }
        parse_sse_event(&self.data_lines, expected_id)
    }
}

fn parse_sse_event(data_lines: &[String], expected_id: &Value) -> Result<Option<Value>> {
    if data_lines.is_empty() {
        return Ok(None);
    }
    let data = data_lines.join("\n");
    if data == "[DONE]" {
        return Ok(None);
    }
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return Ok(None);
    };
    if value.get("id") != Some(expected_id) {
        return Ok(None);
    }
    validate_json_rpc_response(&value, expected_id)?;
    Ok(Some(value))
}

fn validate_json_rpc_response(response: &Value, expected_id: &Value) -> Result<()> {
    if response.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        bail!("MCP HTTP response omitted JSON-RPC version 2.0");
    }
    if response.get("id") != Some(expected_id) {
        bail!("MCP HTTP response id did not match request id {expected_id}");
    }
    if response.get("result").is_none() && response.get("error").is_none() {
        bail!("MCP HTTP response contained neither result nor error");
    }
    if response.get("result").is_some() && response.get("error").is_some() {
        bail!("MCP HTTP response contained both result and error");
    }
    Ok(())
}

fn parse_tools_page(response: &Value) -> Result<(Vec<McpToolDefinition>, Option<String>)> {
    if let Some(error) = response.get("error") {
        bail!("MCP HTTP tools/list failed: {error}");
    }
    let page_tools = response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("MCP HTTP tools/list response omitted result.tools array"))?;
    let tools = page_tools
        .iter()
        .cloned()
        .map(|tool| serde_json::from_value(tool).context("invalid MCP tool definition"))
        .collect::<Result<Vec<_>>>()?;
    let cursor = response
        .pointer("/result/nextCursor")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok((tools, cursor))
}

fn response_text(response: Value) -> Result<String> {
    if let Some(error) = response.get("error") {
        bail!("MCP tools/call failed: {error}");
    }
    let result = response
        .get("result")
        .ok_or_else(|| anyhow!("MCP tools/call returned no result"))?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let message = result
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("MCP tool returned an error: {message}");
    }
    if let Some(structured) = result.get("structuredContent") {
        return Ok(structured.to_string());
    }
    Ok(result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_owned()
    } else {
        let mut boundary = max.min(value.len());
        while !value.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}…", &value[..boundary])
    }
}

#[cfg(test)]
mod http_tests {
    use super::{
        McpHttpClient, SseParser, parse_sse_event, parse_tools_page, preserve_operation_error,
        response_text, session_delete_status_is_acceptable, should_reinitialize_after_404,
        truncate, validate_json_rpc_response,
    };
    use crate::mcp::{McpServerConfig, McpTransport};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn extracts_text_and_structured_mcp_tool_results() {
        assert_eq!(
            response_text(json!({"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"hello"}]}})).unwrap(),
            "hello"
        );
        assert_eq!(
            response_text(
                json!({"jsonrpc":"2.0","id":3,"result":{"structuredContent":{"ok":true}}})
            )
            .unwrap(),
            "{\"ok\":true}"
        );
        assert!(
            response_text(json!({"jsonrpc":"2.0","id":3,"error":{"code":-1,"message":"no"}}))
                .is_err()
        );
        assert!(response_text(json!({"jsonrpc":"2.0","id":3,"result":{"isError":true,"content":[{"type":"text","text":"failed"}]}})).is_err());
    }

    #[test]
    fn truncation_never_splits_utf8_characters() {
        assert_eq!(truncate("aéz", 2), "a…");
        assert_eq!(truncate("short", 20), "short");
    }

    #[test]
    fn validates_json_rpc_version_and_matching_response_id() {
        let valid = json!({"jsonrpc":"2.0","id":7,"result":{}});
        assert!(validate_json_rpc_response(&valid, &json!(7)).is_ok());
        assert!(validate_json_rpc_response(&valid, &json!(8)).is_err());
        assert!(validate_json_rpc_response(&json!({"id":7,"result":{}}), &json!(7)).is_err());
        assert!(validate_json_rpc_response(&json!({"jsonrpc":"2.0","id":7}), &json!(7)).is_err());
    }

    #[test]
    fn sse_ignores_progress_notifications_until_matching_response() {
        let progress =
            vec![r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{}}"#.to_owned()];
        assert!(parse_sse_event(&progress, &json!(9)).unwrap().is_none());
        let unrelated = vec![r#"{"jsonrpc":"2.0","id":8,"result":{}}"#.to_owned()];
        assert!(parse_sse_event(&unrelated, &json!(9)).unwrap().is_none());
        let matching = vec![r#"{"jsonrpc":"2.0","id":9,"result":{"ok":true}}"#.to_owned()];
        assert_eq!(
            parse_sse_event(&matching, &json!(9)).unwrap().unwrap()["result"]["ok"],
            true
        );
    }

    #[test]
    fn sse_parser_handles_crlf_and_arbitrary_chunk_boundaries() {
        let stream = b"data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{\"ok\":true}}\r\n\r\n";
        let mut parser = SseParser::default();
        let mut found = None;
        for chunk in stream.chunks(7) {
            if let Some(value) = parser.push(chunk, &json!(9)).unwrap() {
                found = Some(value);
                break;
            }
        }
        assert_eq!(found.unwrap()["result"]["ok"], true);
    }

    #[test]
    fn tools_pages_require_array_and_extract_next_cursor() {
        let (first, cursor) =
            parse_tools_page(&json!({"result":{"tools":[{"name":"one"}],"nextCursor":"page-2"}}))
                .unwrap();
        assert_eq!(first[0].name, "one");
        assert_eq!(cursor.as_deref(), Some("page-2"));
        let (last, cursor) =
            parse_tools_page(&json!({"result":{"tools":[{"name":"two"}]}})).unwrap();
        assert_eq!(last[0].name, "two");
        assert!(cursor.is_none());
        assert!(parse_tools_page(&json!({"result":{}})).is_err());
    }

    #[test]
    fn custom_headers_cannot_override_transport_headers() {
        let mut server = McpServerConfig {
            id: "headers".into(),
            name: "Headers".into(),
            transport: McpTransport::Http,
            url: Some("http://localhost".into()),
            headers: BTreeMap::new(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            enabled: true,
        };
        server.headers.insert("X-Token".into(), "secret".into());
        let client = McpHttpClient::new(server.clone()).unwrap();
        assert_eq!(client.headers(false).unwrap()["x-token"], "secret");
        server
            .headers
            .insert("MCP-SESSION-ID".into(), "forged".into());
        let client = McpHttpClient::new(server).unwrap();
        assert!(
            client
                .headers(true)
                .unwrap_err()
                .to_string()
                .contains("managed")
        );
    }

    #[test]
    fn established_session_404_triggers_reinitialize_but_initial_404_does_not() {
        assert!(should_reinitialize_after_404(
            reqwest::StatusCode::NOT_FOUND,
            true
        ));
        assert!(!should_reinitialize_after_404(
            reqwest::StatusCode::NOT_FOUND,
            false
        ));
        assert!(!should_reinitialize_after_404(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            true
        ));
    }

    #[test]
    fn session_delete_ignores_missing_or_unsupported_sessions_only() {
        assert!(session_delete_status_is_acceptable(
            reqwest::StatusCode::NO_CONTENT
        ));
        assert!(session_delete_status_is_acceptable(
            reqwest::StatusCode::NOT_FOUND
        ));
        assert!(session_delete_status_is_acceptable(
            reqwest::StatusCode::METHOD_NOT_ALLOWED
        ));
        assert!(!session_delete_status_is_acceptable(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
    }

    #[test]
    fn cleanup_error_does_not_replace_operation_error() {
        let operation_error = anyhow::anyhow!("operation failed");
        let cleanup_error = anyhow::anyhow!("cleanup failed");
        let result: anyhow::Result<()> =
            preserve_operation_error(Err(operation_error), Err(cleanup_error));
        assert_eq!(result.unwrap_err().to_string(), "operation failed");

        let result = preserve_operation_error(Ok(42), Err(anyhow::anyhow!("cleanup failed")));
        assert_eq!(result.unwrap_err().to_string(), "cleanup failed");
        assert_eq!(preserve_operation_error(Ok(42), Ok(())).unwrap(), 42);
    }

    fn read_http_request(
        stream: &mut BufReader<TcpStream>,
    ) -> (String, BTreeMap<String, String>, Value) {
        let mut request_line = String::new();
        stream.read_line(&mut request_line).unwrap();
        assert!(!request_line.is_empty(), "client closed before request");
        let mut headers = BTreeMap::new();
        loop {
            let mut line = String::new();
            stream.read_line(&mut line).unwrap();
            if line == "\r\n" || line == "\n" {
                break;
            }
            let (name, value) = line.split_once(':').unwrap();
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
        let content_length = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0; content_length];
        stream.read_exact(&mut body).unwrap();
        let body = if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        };
        (request_line, headers, body)
    }

    fn write_http_response(
        stream: &mut TcpStream,
        status: &str,
        content_type: Option<&str>,
        session_id: Option<&str>,
        body: &[u8],
    ) {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(content_type) = content_type {
            response.push_str(&format!("Content-Type: {content_type}\r\n"));
        }
        if let Some(session_id) = session_id {
            response.push_str(&format!("MCP-Session-Id: {session_id}\r\n"));
        }
        response.push_str("\r\n");
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn streamable_http_full_session_lifecycle_sends_managed_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server_thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let (line, headers, body) = read_http_request(&mut request);
            assert!(line.starts_with("POST "));
            assert_eq!(body["method"], "initialize");
            assert_eq!(headers["accept"], "application/json, text/event-stream");
            assert_eq!(headers["content-type"], "application/json");
            assert_eq!(headers["x-test-auth"], "mock-secret");
            assert!(!headers.contains_key("mcp-protocol-version"));
            assert!(!headers.contains_key("mcp-session-id"));
            write_http_response(
                &mut stream,
                "200 OK",
                Some("application/json"),
                Some("mock-session-123"),
                br#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"mock","version":"1"}}}"#,
            );

            let (mut stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let (line, headers, body) = read_http_request(&mut request);
            assert!(line.starts_with("POST "));
            assert_eq!(body["method"], "notifications/initialized");
            assert_eq!(headers["mcp-protocol-version"], "2025-03-26");
            assert_eq!(headers["mcp-session-id"], "mock-session-123");
            assert_eq!(headers["x-test-auth"], "mock-secret");
            write_http_response(&mut stream, "202 Accepted", None, None, b"");

            let (mut stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let (line, headers, body) = read_http_request(&mut request);
            assert!(line.starts_with("POST "));
            assert_eq!(body["method"], "tools/list");
            assert_eq!(headers["mcp-protocol-version"], "2025-03-26");
            assert_eq!(headers["mcp-session-id"], "mock-session-123");
            assert_eq!(headers["x-test-auth"], "mock-secret");
            write_http_response(
                &mut stream,
                "200 OK",
                Some("application/json"),
                None,
                br#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"Echo","inputSchema":{"type":"object"}}]}}"#,
            );

            let (mut stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let (line, headers, body) = read_http_request(&mut request);
            assert!(line.starts_with("POST "));
            assert_eq!(body["method"], "tools/call");
            assert_eq!(body["params"]["name"], "echo");
            assert_eq!(body["params"]["arguments"]["message"], "hello");
            assert_eq!(headers["mcp-protocol-version"], "2025-03-26");
            assert_eq!(headers["mcp-session-id"], "mock-session-123");
            assert_eq!(headers["x-test-auth"], "mock-secret");
            write_http_response(
                &mut stream,
                "200 OK",
                Some("application/json"),
                None,
                br#"{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"hello back"}]}}"#,
            );

            let (mut stream, _) = listener.accept().unwrap();
            let mut request = BufReader::new(stream.try_clone().unwrap());
            let (line, headers, _) = read_http_request(&mut request);
            assert!(line.starts_with("DELETE "));
            assert_eq!(headers["mcp-protocol-version"], "2025-03-26");
            assert_eq!(headers["mcp-session-id"], "mock-session-123");
            assert_eq!(headers["x-test-auth"], "mock-secret");
            write_http_response(&mut stream, "204 No Content", None, None, b"");
        });

        let mut server = McpServerConfig {
            id: "http-lifecycle".into(),
            name: "HTTP lifecycle mock".into(),
            transport: McpTransport::Http,
            url: Some(format!("http://{address}")),
            headers: BTreeMap::new(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            enabled: true,
        };
        server
            .headers
            .insert("X-Test-Auth".into(), "mock-secret".into());

        let result = super::run_async(async move {
            let mut client = McpHttpClient::new(server)?;
            client.initialize().await?;
            let tools = client.list_tools().await?;
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0].name, "echo");
            let call = client.call_tool("echo", json!({"message":"hello"})).await?;
            assert_eq!(call, "hello back");
            client.close_session().await?;
            Ok(())
        });
        result.unwrap();
        server_thread.join().unwrap();
    }
}

pub fn discover_tools(server: &McpServerConfig) -> Result<Vec<McpToolDefinition>> {
    run_async({
        let server = server.clone();
        async move {
            if server.transport == McpTransport::Http {
                let mut client = McpHttpClient::new(server)?;
                let operation = match client.initialize().await {
                    Ok(()) => client.list_tools().await,
                    Err(error) => Err(error),
                };
                let cleanup = client.close_session().await;
                return preserve_operation_error(operation, cleanup);
            }
            let mut process = McpProcess::start(&server).await?;
            let tools = process.list_tools().await?;
            let _ = process.child.kill().await;
            Ok(tools)
        }
    })
}

pub fn call_tool(server: &McpServerConfig, tool: &str, arguments: Value) -> Result<String> {
    let server = server.clone();
    let tool = tool.to_owned();
    run_async(async move {
        if server.transport == McpTransport::Http {
            let mut client = McpHttpClient::new(server)?;
            let operation = match client.initialize().await {
                Ok(()) => client.call_tool(&tool, arguments).await,
                Err(error) => Err(error),
            };
            let cleanup = client.close_session().await;
            return preserve_operation_error(operation, cleanup);
        }
        let mut process = McpProcess::start(&server).await?;
        let result = process.call_tool(&tool, arguments).await;
        let _ = process.child.kill().await;
        result
    })
}

fn run_async<F, T>(future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => Err(anyhow!(error)),
        };
        let _ = tx.send(result);
    });
    rx.recv()
        .map_err(|_| anyhow!("MCP worker thread panicked"))?
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn fake_server() -> McpServerConfig {
        let script = r#"while IFS= read -r line; do
case "$line" in
  *'"method":"initialize"'*) printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}' ;;
  *'"method":"tools/list"'*) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hello","description":"Say hello","inputSchema":{"type":"object"}}]}}' ;;
  *'"method":"tools/call"'*) printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"mcp-ok"}]}}' ;;
esac
done"#;
        McpServerConfig {
            id: "fake-mcp".into(),
            name: "Fake MCP".into(),
            transport: McpTransport::Stdio,
            url: None,
            headers: BTreeMap::new(),
            command: "sh".into(),
            args: vec!["-c".into(), script.into()],
            env: BTreeMap::new(),
            enabled: false,
        }
    }

    #[test]
    fn discovers_and_calls_tools_using_mcp_json_rpc() {
        let server = fake_server();
        let tools = discover_tools(&server).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "hello");
        assert_eq!(
            call_tool(&server, "hello", serde_json::json!({})).unwrap(),
            "mcp-ok"
        );
    }
}
