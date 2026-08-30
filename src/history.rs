use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<HistoryMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

impl History {
    fn path() -> PathBuf {
        dirs::home_dir()
            .expect("could not determine home directory")
            .join(".config")
            .join("luma")
            .join("history.json")
    }

    pub fn load() -> Self {
        let path = Self::path();

        match fs::read_to_string(path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data)?;

        Ok(())
    }
}
