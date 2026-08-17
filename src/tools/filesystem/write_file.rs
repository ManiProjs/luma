use anyhow::Result;
use std::fs;

use crate::tools::Tool;

pub struct WriteFile;

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Writes content to a file. Input MUST be:
    
    <file path>
    <complete file content>

    Example:
    src/main.rs
    fn main() {
        println!(\"Hello\");
    }"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let mut parts = input.splitn(2, '\n');

        let path = parts.next().unwrap_or("").trim();

        let content = parts.next().unwrap_or("");

        if path.is_empty() {
            anyhow::bail!("Missing file path");
        }

        if content.is_empty() {
            anyhow::bail!("Missing file content for {}", path);
        }

        fs::write(path, content)?;

        Ok(format!("Wrote {}", path))
    }
}
