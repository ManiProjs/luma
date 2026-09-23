pub mod setup;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: ModelConfig,

    pub planner: ModelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,

    pub endpoint: String,

    pub name: String,

    pub api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            planner: ModelConfig::default(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            endpoint: String::new(),
            name: String::new(),
            api_key: None,
        }
    }
}

/// Returns the directory containing Luma's configuration.
///
/// macOS:
/// ~/Library/Application Support/luma
///
/// Linux:
/// ~/.config/luma
///
/// Windows:
/// %APPDATA%/luma
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("luma")
}

/// Returns the path to Luma's configuration file.
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn exists() -> bool {
    config_path().is_file()
}

pub fn load() -> Result<Config> {
    let path = config_path();

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Luma config at {}", path.display()))?;

    let config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse Luma config at {}", path.display()))?;

    Ok(config)
}

pub fn load_or_default() -> Result<Config> {
    if !exists() {
        return Ok(Config::default());
    }

    load()
}

pub fn save(config: &Config) -> Result<()> {
    let directory = config_dir();
    let path = config_path();

    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "Failed to create Luma config directory at {}",
            directory.display()
        )
    })?;

    let content = toml::to_string_pretty(config).context("Failed to serialize Luma config")?;

    let temporary_path = directory.join("config.toml.tmp");

    fs::write(&temporary_path, content).with_context(|| {
        format!(
            "Failed to write temporary config at {}",
            temporary_path.display()
        )
    })?;

    fs::rename(&temporary_path, &path)
        .with_context(|| format!("Failed to move config into place at {}", path.display()))?;

    Ok(())
}
