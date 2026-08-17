use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Serialize, Deserialize)]
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
        let home = dirs::home_dir().expect("No home directory");

        home.join(".config").join("luma").join("history.json")
    }

    pub fn load() -> Self {
        let path = Self::path();

        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(history) = serde_json::from_str(&data) {
                return history;
            }
        }

        Self {
            messages: Vec::new(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, serde_json::to_string_pretty(self)?)?;

        Ok(())
    }
}
