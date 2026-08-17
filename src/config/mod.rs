pub mod setup;

use anyhow::Result;
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub model: ModelConfig,

    pub planner: ModelConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    pub provider: String,

    pub endpoint: String,

    pub name: String,

    pub api_key: Option<String>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("luma")
        .join("config.toml")
}

pub fn exists() -> bool {
    config_path().exists()
}

pub fn load() -> Result<Config> {
    let path = config_path();

    let content = fs::read_to_string(path)?;

    let config = toml::from_str(&content)?;

    Ok(config)
}
