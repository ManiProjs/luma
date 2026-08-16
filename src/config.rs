use anyhow::Result;
use dialoguer::{Input, Select};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub model: ModelConfig,
    pub planner: ModelConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelConfig {
    pub endpoint: String,
    pub name: String,
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let mut path = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("No config directory"))?;

        path.push("luma.toml");

        Ok(path)
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;

        if !path.exists() {
            let config = Self::setup()?;

            config.save()?;

            return Ok(config);
        }

        let content = fs::read_to_string(path)?;

        Ok(toml::from_str(&content)?)
    }

    fn setup() -> Result<Self> {
        println!("✨ Welcome to Luma setup!\n");

        let providers = vec!["Ollama", "LM Studio", "Custom OpenAI-compatible API"];

        let choice = Select::new()
            .with_prompt("Choose your model provider")
            .items(&providers)
            .default(0)
            .interact()?;

        let endpoint = match choice {
            0 => "http://localhost:11434/v1/chat/completions".to_string(),

            1 => "http://localhost:1234/v1/chat/completions".to_string(),

            _ => Input::new().with_prompt("API endpoint").interact_text()?,
        };

        let model = Input::<String>::new()
            .with_prompt("Model name")
            .default("qwen2.5-coder:3b".into())
            .interact_text()?;

        println!("\n✓ Configuration created");

        Ok(Self {
            model: ModelConfig {
                endpoint: endpoint.clone(),

                name: model.clone(),
            },

            planner: ModelConfig {
                endpoint,

                name: model,
            },
        })
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = toml::to_string_pretty(self)?;

        fs::write(path, data)?;

        Ok(())
    }
}
