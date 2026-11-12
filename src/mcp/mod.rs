//! Model Context Protocol support. MCP servers are managed and executed
//! independently from native Luma plugins.

mod client;
mod manager;
mod tool;

pub use client::{McpToolDefinition, discover_tools};
pub use manager::{McpManager, McpServerConfig};
pub use tool::McpTool;

use crate::tools::ToolRegistry;
use anyhow::Result;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

pub fn register_enabled_tools(registry: &mut ToolRegistry) -> Result<usize> {
    let tools = match McpManager::default().enabled_tools() {
        Ok(tools) => tools,
        Err(error) => {
            tracing::warn!(%error, "could not load the MCP server registry");
            return Ok(0);
        }
    };
    let count = tools.len();
    for tool in tools {
        registry.register_boxed(tool);
    }
    Ok(count)
}
