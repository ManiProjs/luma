use anyhow::{Context, Result, anyhow};
use serde_json::Value;

use super::{McpServerConfig, McpToolDefinition, client};
use crate::tools::Tool;

pub struct McpTool {
    server: McpServerConfig,
    definition: McpToolDefinition,
    name: String,
    description: String,
    model_guide: String,
}

impl McpTool {
    pub(super) fn new(server: McpServerConfig, definition: McpToolDefinition) -> Self {
        let name = registry_name(&server.id, &definition.name);
        let description = format!(
            "{} (MCP server: {}). Input schema: {}. This external MCP action requires approval.",
            if definition.description.trim().is_empty() {
                "Call the MCP tool."
            } else {
                &definition.description
            },
            server.name,
            definition.input_schema,
        );
        let model_guide = definition
            .model_guide
            .clone()
            .filter(|guide| !guide.trim().is_empty())
            .unwrap_or_else(|| description.clone());
        Self {
            server,
            definition,
            name,
            description,
            model_guide,
        }
    }
}

impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn model_guide(&self) -> &str {
        &self.model_guide
    }
    fn requires_network(&self) -> bool {
        // MCP servers are arbitrary child processes. Their metadata cannot
        // prove that they do not access the network.
        true
    }

    fn execute(&self, input: &str) -> Result<String> {
        if input.len() > 1_000_000 {
            return Err(anyhow!("MCP tool input exceeded the 1 MB limit"));
        }
        let arguments: Value =
            serde_json::from_str(input).context("MCP tool input must be JSON")?;
        if !arguments.is_object() {
            return Err(anyhow!("MCP tool arguments must be a JSON object"));
        }
        client::call_tool(&self.server, &self.definition.name, arguments)
    }
}

pub(super) fn registry_name(server_id: &str, value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("mcp__{server_id}__{}", sanitized.trim_matches('_'))
}

#[allow(dead_code)]
fn _empty_object() -> Value {
    Value::Object(Default::default())
}
