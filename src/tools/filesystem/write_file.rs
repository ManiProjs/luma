use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::tools::Tool;

pub struct WriteFile;

#[derive(Deserialize)]
struct WriteFileInput {
    path: String,
    content: String,
}

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write a complete file. Input must be JSON: {\"path\":\"file\",\"content\":\"text\"}"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let data: WriteFileInput =
            serde_json::from_str(input).map_err(|e| anyhow!("Invalid JSON: {}", e))?;

        if data.content.trim().is_empty() {
            return Err(anyhow!("Missing file content"));
        }

        std::fs::write(&data.path, data.content)?;

        Ok(format!("Successfully wrote {}", data.path))
    }
}
