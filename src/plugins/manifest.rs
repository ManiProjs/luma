use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub tools: Vec<PluginToolManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default = "default_requires_network")]
    pub requires_network: bool,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

fn default_requires_network() -> bool {
    true
}

impl PluginManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let manifest_path = path.join("plugin.toml");
        let source = std::fs::read_to_string(&manifest_path).with_context(|| {
            format!("could not read plugin manifest {}", manifest_path.display())
        })?;
        let manifest: Self = toml::from_str(&source).with_context(|| {
            format!(
                "could not parse plugin manifest {}",
                manifest_path.display()
            )
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!(
                "unsupported Luma plugin manifest version {}",
                self.schema_version
            );
        }
        validate_id(&self.id)?;
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            bail!("plugin name and version must not be empty");
        }
        if self.command.trim().is_empty() {
            bail!("plugin command must not be empty");
        }
        if self.tools.is_empty() {
            bail!("plugin must declare at least one tool");
        }
        let mut tool_names = std::collections::HashSet::new();
        for tool in &self.tools {
            validate_id(&tool.name)
                .with_context(|| format!("invalid tool name in plugin {}", self.id))?;
            if !tool_names.insert(&tool.name) {
                bail!(
                    "plugin '{}' declares duplicate tool '{}'",
                    self.id,
                    tool.name
                );
            }
            if tool.description.trim().is_empty() {
                bail!("plugin tool '{}' must have a description", tool.name);
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
    {
        bail!("IDs must be 1–64 lowercase ASCII letters, digits, hyphens or underscores");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PluginManifest;

    #[test]
    fn validates_a_native_plugin_manifest() {
        let manifest: PluginManifest = toml::from_str(
            r#"
schema_version = 1
id = "sample-plugin"
name = "Sample Plugin"
version = "1.0.0"
command = "python3"
args = ["main.py"]
env = { MODE = "local" }
[[tools]]
name = "summarize"
description = "Summarize supplied text"
read_only = true
requires_network = false
input_schema = { type = "object" }
"#,
        )
        .unwrap();
        manifest.validate().unwrap();
        assert!(manifest.tools[0].read_only);
        assert!(!manifest.tools[0].requires_network);
    }

    #[test]
    fn rejects_invalid_plugin_ids() {
        let mut manifest: PluginManifest = toml::from_str(
            r#"
schema_version = 1
id = "sample-plugin"
name = "Sample"
version = "1.0.0"
command = "runner"
[[tools]]
name = "run"
description = "Run something"
"#,
        )
        .unwrap();
        manifest.id = "../outside".into();
        assert!(manifest.validate().is_err());
    }
}
