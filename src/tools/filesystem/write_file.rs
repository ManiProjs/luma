use anyhow::Result;
use std::fs;

use super::Tool;

pub struct WriteFile;

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let mut parts = input.splitn(2, '\n');

        let path = parts.next().unwrap_or("");

        let content = parts.next().unwrap_or("");

        fs::write(path, content)?;

        Ok(format!("Written {}", path))
    }
}
