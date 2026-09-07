use anyhow::{Context, Result, bail};
use std::fs;

use crate::tools::Tool;

/// Default number of lines returned when no range is given.
const DEFAULT_LINES: usize = 120;

/// Maximum bytes of file content kept. Prevents a single large file
/// from consuming the entire model context.
const MAX_OUTPUT_BYTES: usize = 16_000;

pub struct ReadFile;

impl Tool for ReadFile {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file. Supports optional line ranges like file.rs:10-50. \
         For large files, the first 120 lines are returned with a note \
         showing the total line count and how to request more."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let (path, range) = match input.rsplit_once(':') {
            Some((file, lines)) if lines.contains('-') => (file, Some(lines)),
            _ => (input, None),
        };

        let content =
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let (start, end) = match range {
            Some(range) => {
                let (start_str, end_str) = range
                    .split_once('-')
                    .context("invalid range format — use file.rs:10-50")?;

                let start: usize = start_str
                    .parse()
                    .with_context(|| format!("invalid start line: {}", start_str))?;

                let end: usize = end_str
                    .parse()
                    .with_context(|| format!("invalid end line: {}", end_str))?;

                if start == 0 {
                    bail!("line numbers start at 1");
                }

                if start > total_lines {
                    bail!(
                        "start line {} is beyond the end of the file ({} lines)",
                        start,
                        total_lines
                    );
                }

                (start, end.min(total_lines))
            }
            None => (1, DEFAULT_LINES.min(total_lines)),
        };

        let selected: Vec<String> = lines
            .iter()
            .skip(start.saturating_sub(1))
            .take(end - start + 1)
            .enumerate()
            .map(|(i, line)| format!("{}: {}", start + i, line))
            .collect();

        let mut output = selected.join("\n");

        // Truncate if the selected range is still too large.
        if output.len() > MAX_OUTPUT_BYTES {
            let mut end_byte = MAX_OUTPUT_BYTES;
            while end_byte > 0 && !output.is_char_boundary(end_byte) {
                end_byte -= 1;
            }

            output.truncate(end_byte);
            output.push_str(&format!(
                "\n... [content truncated at {} bytes — request a smaller range]",
                MAX_OUTPUT_BYTES
            ));
        }

        // Add context about the file so the agent knows what it's looking at.
        let mut header = String::new();

        if total_lines > DEFAULT_LINES {
            header.push_str(&format!(
                "File: {} ({} lines total, showing {}-{})\n",
                path, total_lines, start, end
            ));

            if end < total_lines {
                header.push_str(&format!(
                    "To see more, use: {}:{}-{}\n",
                    path,
                    end + 1,
                    (end + DEFAULT_LINES).min(total_lines)
                ));
            }

            header.push_str("---\n");
        }

        Ok(format!("{}{}", header, output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_small_file() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "line one").unwrap();
        writeln!(file, "line two").unwrap();

        let out = ReadFile.execute(file.path().to_str().unwrap()).unwrap();
        assert!(out.contains("1: line one"));
        assert!(out.contains("2: line two"));
        assert!(!out.contains("lines total"));
    }

    #[test]
    fn reads_range() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for i in 1..=200 {
            writeln!(file, "line {}", i).unwrap();
        }

        let path = file.path().to_str().unwrap();
        let out = ReadFile.execute(&format!("{}:50-60", path)).unwrap();
        assert!(out.contains("50: line 50"));
        assert!(out.contains("60: line 60"));
        assert!(!out.contains("61: line 61"));
    }

    #[test]
    fn reports_total_lines_and_next_range() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for i in 1..=200 {
            writeln!(file, "line {}", i).unwrap();
        }

        let path = file.path().to_str().unwrap();
        let out = ReadFile.execute(path).unwrap();
        assert!(out.contains("200 lines total"));
        assert!(out.contains("showing 1-120"));
        assert!(out.contains("To see more"));
    }

    #[test]
    fn rejects_invalid_range() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "hello").unwrap();

        let path = file.path().to_str().unwrap();
        assert!(ReadFile.execute(&format!("{}:0-10", path)).is_err());
        assert!(ReadFile.execute(&format!("{}:99-100", path)).is_err());
    }
}
