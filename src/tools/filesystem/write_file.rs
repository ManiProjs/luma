use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::tools::Tool;

pub struct WriteFile;

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write a complete file using JSON: {\"path\":\"file\", \"content\":\"text\"}"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let json: Value =
            serde_json::from_str(input).map_err(|e| anyhow!("Invalid JSON: {}", e))?;

        let path = json
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing file path"))?;

        let content = json
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing file content"))?;

        if content.trim().is_empty() {
            return Err(anyhow!("Missing file content"));
        }

        std::fs::write(path, content)?;

        Ok(format!("Successfully wrote {}", path))
    }
}
