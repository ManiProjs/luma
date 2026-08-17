use std::fs;

use anyhow::Result;

use crate::tools::Tool;

pub struct ListDirectory;

impl Tool for ListDirectory {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "Lists files in a directory"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let entries = fs::read_dir(input)?
            .map(|entry| entry.map(|e| e.path().display().to_string()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries.join("\n"))
    }
}
