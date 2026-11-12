use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

use super::{McpTool, McpTransport, discover_tools};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
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
            if server.transport == McpTransport::Stdio && server.command.trim().is_empty() {
                bail!("MCP server '{}' has an empty command", server.id);
            }
            if server.transport == McpTransport::Http {
                validate_http_url(server.url.as_deref().unwrap_or_default())?;
                validate_headers(&server.headers)?;
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
            transport: McpTransport::Stdio,
            url: None,
            headers: BTreeMap::new(),
            command,
            args,
            env,
            enabled: false,
        };
        servers.push(server.clone());
        self.write(&servers)?;
        Ok(server)
    }

    pub fn add_http(
        &self,
        id: String,
        url: String,
        headers: BTreeMap<String, String>,
    ) -> Result<McpServerConfig> {
        validate_server_id(&id)?;
        validate_http_url(&url)?;
        validate_headers(&headers)?;
        let mut servers = self.read()?;
        if servers.iter().any(|server| server.id == id) {
            bail!("MCP server '{id}' is already registered");
        }
        let server = McpServerConfig {
            id: id.clone(),
            name: id,
            transport: McpTransport::Http,
            url: Some(url),
            headers,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
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

fn validate_http_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("MCP HTTP URL is invalid")?;
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        bail!("MCP HTTP endpoints must use HTTPS (HTTP is allowed only for localhost)");
    }
    if url.username() != "" || url.password().is_some() {
        bail!("MCP HTTP credentials must be supplied as headers, not embedded in the URL");
    }
    Ok(())
}

fn validate_headers(headers: &BTreeMap<String, String>) -> Result<()> {
    for (name, value) in headers {
        reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid MCP HTTP header name '{name}'"))?;
        if value.contains('\r') || value.contains('\n') || value.contains('\0') {
            bail!("invalid value for MCP HTTP header '{name}'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_headers, validate_http_url, validate_server_id};
    use std::collections::BTreeMap;

    #[test]
    fn validates_mcp_server_ids_without_relying_on_the_plugin_manifest() {
        assert!(validate_server_id("filesystem-server_1").is_ok());
        assert!(validate_server_id("../outside").is_err());
        assert!(validate_server_id("Uppercase").is_err());
        assert!(validate_server_id(&"a".repeat(65)).is_err());
    }

    #[test]
    fn validates_http_mcp_endpoints_and_headers() {
        assert!(validate_http_url("https://mcp.example.com/rpc").is_ok());
        assert!(validate_http_url("http://localhost:8080/mcp").is_ok());
        assert!(validate_http_url("http://[::1]:8080/mcp").is_ok());
        assert!(validate_http_url("http://mcp.example.com/rpc").is_err());
        assert!(validate_http_url("https://user:secret@mcp.example.com/rpc").is_err());
        assert!(
            validate_headers(&BTreeMap::from([(
                "Authorization".into(),
                "Bearer token".into()
            )]))
            .is_ok()
        );
        assert!(
            validate_headers(&BTreeMap::from([(
                "Authorization".into(),
                "Bearer\ntoken".into()
            )]))
            .is_err()
        );
    }

    #[test]
    fn legacy_mcp_registry_entries_default_to_stdio_transport() {
        let server: super::McpServerConfig = serde_json::from_value(serde_json::json!({
            "id": "legacy", "name": "Legacy", "command": "node", "args": [], "env": {}, "enabled": false
        })).unwrap();
        assert_eq!(server.transport, crate::mcp::McpTransport::Stdio);
        assert!(server.url.is_none());
        assert!(server.headers.is_empty());
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
