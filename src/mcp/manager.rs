use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

use super::{McpTool, discover_tools};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Values may reference environment variables as `${VARIABLE}` so API
    /// keys need not be written into the server registry file.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Default)]
pub struct McpManager;

impl McpManager {
    fn root(&self) -> PathBuf {
        crate::config::config_dir().join("mcp")
    }
    fn path(&self) -> PathBuf {
        self.root().join("servers.json")
    }

    fn read(&self) -> Result<Vec<McpServerConfig>> {
        let path = self.path();
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let content =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let servers: Vec<McpServerConfig> =
            serde_json::from_str(&content).context("parsing MCP server registry")?;
        let mut ids = std::collections::HashSet::new();
        for server in &servers {
            validate_server_id(&server.id)?;
            if !ids.insert(&server.id) {
                bail!("MCP registry contains duplicate server id '{}'", server.id);
            }
            if server.command.trim().is_empty() {
                bail!("MCP server '{}' has an empty command", server.id);
            }
            for (key, value) in &server.env {
                if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0')
                {
                    bail!(
                        "MCP server '{}' has an invalid environment variable",
                        server.id
                    );
                }
            }
        }
        Ok(servers)
    }

    fn write(&self, servers: &[McpServerConfig]) -> Result<()> {
        let root = self.root();
        fs::create_dir_all(&root)?;
        let temporary = root.join("servers.json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(servers)?)?;
        set_private_permissions(&temporary)?;
        fs::rename(&temporary, self.path())?;
        set_private_permissions(&self.path())?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<McpServerConfig>> {
        self.read()
    }

    pub fn add(
        &self,
        id: String,
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    ) -> Result<McpServerConfig> {
        validate_server_id(&id)?;
        if command.trim().is_empty() {
            bail!("MCP server command must not be empty");
        }
        for (key, value) in &env {
            if key.is_empty() || key.contains('=') || key.contains('\0') || value.contains('\0') {
                bail!("invalid MCP environment variable '{key}'");
            }
        }
        let mut servers = self.read()?;
        if servers.iter().any(|server| server.id == id) {
            bail!("MCP server '{id}' is already registered");
        }
        let server = McpServerConfig {
            id: id.clone(),
            name: id,
            command,
            args,
            env,
            enabled: false,
        };
        servers.push(server.clone());
        self.write(&servers)?;
        Ok(server)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<McpServerConfig> {
        validate_server_id(id)?;
        let mut servers = self.read()?;
        let server = servers
            .iter_mut()
            .find(|server| server.id == id)
            .ok_or_else(|| anyhow!("MCP server '{id}' is not registered"))?;
        server.enabled = enabled;
        let updated = server.clone();
        self.write(&servers)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        validate_server_id(id)?;
        let mut servers = self.read()?;
        let before = servers.len();
        servers.retain(|server| server.id != id);
        if servers.len() == before {
            return Ok(false);
        }
        self.write(&servers)?;
        Ok(true)
    }

    pub fn tools(&self, id: &str) -> Result<Vec<super::McpToolDefinition>> {
        validate_server_id(id)?;
        let server = self
            .read()?
            .into_iter()
            .find(|server| server.id == id)
            .ok_or_else(|| anyhow!("MCP server '{id}' is not registered"))?;
        discover_tools(&server)
    }

    pub fn enabled_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        for server in self.read()?.into_iter().filter(|server| server.enabled) {
            let result = (|| -> Result<Vec<Box<dyn Tool>>> {
                let definitions = discover_tools(&server).with_context(|| {
                    format!("discovering tools from MCP server '{}'", server.id)
                })?;
                let mut server_tools: Vec<Box<dyn Tool>> = Vec::new();
                let mut server_names = std::collections::HashSet::new();
                for definition in definitions {
                    if definition.name.trim().is_empty() {
                        bail!(
                            "MCP server '{}' declared a tool with an empty name",
                            server.id
                        );
                    }
                    let registry_name = super::tool::registry_name(&server.id, &definition.name);
                    if !server_names.insert(registry_name.clone()) {
                        bail!(
                            "MCP server '{}' exposes colliding tool names after normalization: {registry_name}",
                            server.id
                        );
                    }
                    server_tools.push(Box::new(McpTool::new(server.clone(), definition)));
                }
                Ok(server_tools)
            })();
            match result {
                Ok(server_tools) => tools.extend(server_tools),
                Err(error) => {
                    tracing::warn!(server = %server.id, %error, "skipping unavailable MCP server")
                }
            }
        }
        Ok(tools)
    }
}

fn validate_server_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        bail!(
            "MCP server IDs must be 1–64 lowercase ASCII letters, digits, hyphens or underscores"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_server_id;

    #[test]
    fn validates_mcp_server_ids_without_relying_on_the_plugin_manifest() {
        assert!(validate_server_id("filesystem-server_1").is_ok());
        assert!(validate_server_id("../outside").is_err());
        assert!(validate_server_id("Uppercase").is_err());
        assert!(validate_server_id(&"a".repeat(65)).is_err());
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
