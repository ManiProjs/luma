use anyhow::Result;
use std::process::Command;

use crate::tools::Tool;

pub struct SearchFiles;

impl Tool for SearchFiles {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for text patterns in workspace files using ripgrep"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let output = Command::new("rg")
            .args(["--hidden", "--glob", "!.git", input])
            .output()?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
