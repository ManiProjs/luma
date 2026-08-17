use anyhow::Result;
use std::fs;

use crate::tools::Tool;

pub struct ReadFile;

impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file. Supports optional line ranges like file.rs:10-50"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let (path, range) = match input.rsplit_once(':') {
            Some((file, lines)) if lines.contains('-') => (file, Some(lines)),

            _ => (input, None),
        };

        let content = fs::read_to_string(path)?;

        let lines: Vec<&str> = content.lines().collect();

        let output = match range {
            Some(range) => {
                let (start, end) = range.split_once('-').unwrap();

                let start: usize = start.parse()?;

                let end: usize = end.parse()?;

                lines
                    .iter()
                    .skip(start.saturating_sub(1))
                    .take(end - start + 1)
                    .enumerate()
                    .map(|(i, line)| format!("{}: {}", start + i, line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }

            None => {
                const MAX_LINES: usize = 120;

                lines
                    .iter()
                    .take(MAX_LINES)
                    .enumerate()
                    .map(|(i, line)| format!("{}: {}", i + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        Ok(output)
    }
}
