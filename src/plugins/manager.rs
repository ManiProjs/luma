use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{PluginManifest, PluginTool};
use crate::tools::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub enabled: bool,
    pub installed_at: u64,
}

#[derive(Default)]
pub struct PluginManager;

impl PluginManager {
    fn root(&self) -> PathBuf {
        crate::config::config_dir().join("plugins")
    }

    fn registry_path(&self) -> PathBuf {
        self.root().join("registry.json")
    }

    fn records(&self) -> Result<Vec<InstalledPlugin>> {
        let path = self.registry_path();
        if !path.is_file() {
            return Ok(Vec::new());
        }
        let content =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let records: Vec<InstalledPlugin> =
            serde_json::from_str(&content).context("parsing Luma plugin registry")?;
        let mut ids = std::collections::HashSet::new();
        for record in &records {
            super::manifest::validate_id(&record.id)?;
            if !ids.insert(&record.id) {
                bail!("plugin registry contains duplicate id '{}'", record.id);
            }
        }
        Ok(records)
    }

    fn save_records(&self, records: &[InstalledPlugin]) -> Result<()> {
        let root = self.root();
        fs::create_dir_all(&root)?;
        let path = self.registry_path();
        let temporary = root.join("registry.json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(records)?)?;
        set_private_permissions(&temporary)?;
        fs::rename(&temporary, &path).with_context(|| format!("updating {}", path.display()))?;
        set_private_permissions(&path)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<(InstalledPlugin, PluginManifest)>> {
        self.records()?
            .into_iter()
            .map(|record| {
                let manifest = PluginManifest::load(&self.root().join(&record.id))?;
                if manifest.id != record.id {
                    bail!(
                        "plugin registry id '{}' does not match its manifest id '{}'",
                        record.id,
                        manifest.id
                    );
                }
                Ok((record, manifest))
            })
            .collect()
    }

    /// Copy a local plugin package into Luma's managed directory. Plugins are
    /// installed disabled so the user can inspect the manifest before enabling.
    pub fn install(&self, source: &Path) -> Result<InstalledPlugin> {
        let source = source
            .canonicalize()
            .with_context(|| format!("resolving plugin path {}", source.display()))?;
        let manifest = PluginManifest::load(&source)?;
        let root = self.root();
        fs::create_dir_all(&root)?;
        let destination = root.join(&manifest.id);
        if destination.exists() {
            bail!("plugin '{}' is already installed", manifest.id);
        }
        if let Err(error) = copy_package(&source, &destination) {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        if let Err(error) = validate_command_path(&destination, &manifest.command) {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        let record = InstalledPlugin {
            id: manifest.id,
            enabled: false,
            installed_at: now_unix(),
        };
        let mut records = self.records()?;
        records.push(record.clone());
        if let Err(error) = self.save_records(&records) {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(record)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<InstalledPlugin> {
        super::manifest::validate_id(id)?;
        let mut records = self.records()?;
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow!("plugin '{id}' is not installed"))?;
        if enabled {
            let manifest = PluginManifest::load(&self.root().join(id))?;
            if manifest.id != id {
                bail!(
                    "plugin registry id '{id}' does not match manifest id '{}'",
                    manifest.id
                );
            }
        }
        record.enabled = enabled;
        let updated = record.clone();
        self.save_records(&records)?;
        Ok(updated)
    }

    pub fn uninstall(&self, id: &str) -> Result<bool> {
        super::manifest::validate_id(id)?;
        let mut records = self.records()?;
        let Some(index) = records.iter().position(|record| record.id == id) else {
            return Ok(false);
        };
        let path = self.root().join(id);
        if path.exists() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("removing plugin directory {}", path.display()))?;
        }
        records.remove(index);
        self.save_records(&records)?;
        Ok(true)
    }

    pub fn enabled_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        for record in self.records()?.into_iter().filter(|record| record.enabled) {
            let result = (|| -> Result<Vec<Box<dyn Tool>>> {
                let root = self.root().join(&record.id);
                let manifest = PluginManifest::load(&root)?;
                if manifest.id != record.id {
                    bail!(
                        "plugin registry id '{}' does not match its manifest id '{}'",
                        record.id,
                        manifest.id
                    );
                }
                validate_command_path(&root, &manifest.command)?;
                Ok(manifest
                    .tools
                    .iter()
                    .cloned()
                    .map(|tool| {
                        Box::new(PluginTool::new(root.clone(), manifest.clone(), tool))
                            as Box<dyn Tool>
                    })
                    .collect())
            })();
            match result {
                Ok(plugin_tools) => tools.extend(plugin_tools),
                Err(error) => {
                    tracing::warn!(plugin = %record.id, %error, "skipping unavailable Luma plugin")
                }
            }
        }
        Ok(tools)
    }
}

fn copy_package(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)
        .with_context(|| format!("reading plugin package {}", source.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_symlink() {
            bail!(
                "plugin packages may not contain symbolic links: {}",
                entry.path().display()
            );
        } else if file_type.is_dir() {
            copy_package(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        } else {
            bail!(
                "unsupported file type in plugin package: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn validate_command_path(root: &Path, command: &str) -> Result<()> {
    let path = Path::new(command);
    if path.is_absolute() {
        bail!(
            "plugin command paths must be relative to the installed package or an executable name"
        );
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("plugin command path cannot escape its package directory");
    }
    if command.contains('/') || command.contains('\\') || command.starts_with('.') {
        let target = root
            .join(path)
            .canonicalize()
            .with_context(|| format!("resolving plugin command {command}"))?;
        if !target.starts_with(root.canonicalize()?) || !target.is_file() {
            bail!("plugin command must resolve to a file inside its installed package");
        }
    }
    Ok(())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
