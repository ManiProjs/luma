use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::time::Duration;

use super::McpServerConfig;

const PROTOCOL_VERSION: &str = "2025-03-26";
const IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
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

pub fn discover_tools(server: &McpServerConfig) -> Result<Vec<McpToolDefinition>> {
    run_async({
        let server = server.clone();
        async move {
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
