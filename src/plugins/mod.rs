//! Native Luma plugins. This is intentionally separate from the MCP client:
//! plugins use Luma's own manifest and one-shot JSON process protocol.

mod manager;
pub(crate) mod manifest;
mod tool;

pub use manager::PluginManager;
pub use manifest::{PluginManifest, PluginToolManifest};
pub use tool::PluginTool;

use anyhow::Result;

use crate::tools::ToolRegistry;

pub fn register_enabled_tools(registry: &mut ToolRegistry) -> Result<usize> {
    let tools = match PluginManager::default().enabled_tools() {
        Ok(tools) => tools,
        Err(error) => {
            tracing::warn!(%error, "could not load the Luma plugin registry");
            return Ok(0);
        }
    };
    let count = tools.len();
    for tool in tools {
        registry.register_boxed(tool);
    }
    Ok(count)
}
