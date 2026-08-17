use anyhow::{Result, anyhow};

use crate::tools::Tool;

pub struct WriteFile;

impl Tool for WriteFile {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write a complete file. First line is the path, remaining lines are the file contents."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let mut lines = input.lines();

        let path = lines.next().ok_or_else(|| anyhow!("Missing file path"))?;

        let content: String = lines.collect::<Vec<_>>().join("\n");

        if content.trim().is_empty() {
            return Err(anyhow!(
                "Missing file content. write_file requires:\n\
                     <path>\n\
                     <complete file contents>"
            ));
        }

        std::fs::write(path, content)?;

        Ok(format!("Successfully wrote {}", path))
    }
}
