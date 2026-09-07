use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::Command;

use crate::tools::Tool;

/// Maximum number of matching lines returned. Keeps a broad search from
/// flooding the model context.
const MAX_MATCH_LINES: usize = 50;

/// Maximum bytes of search output kept.
const MAX_OUTPUT_BYTES: usize = 8_000;

#[derive(Debug, Deserialize)]
struct SearchInput {
    pattern: String,
    path: Option<String>,
    glob: Option<String>,
}

pub struct SearchFiles;

impl Tool for SearchFiles {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search file contents with ripgrep. \
         Input is JSON: {\"pattern\":\"text or regex\",\"path\":\"optional dir\",\"glob\":\"optional glob\"}. \
         A plain string is also accepted and treated as the pattern. \
         Results are capped at 50 matching lines."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input = parse_input(input)?;

        if input.pattern.trim().is_empty() {
            bail!("search_files: pattern cannot be empty");
        }

        let path = input.path.as_deref().unwrap_or(".");

        let mut command = Command::new("rg");
        command
            .arg("--line-number")
            .arg("--with-filename")
            .arg("--heading")
            .arg("--max-count")
            .arg(MAX_MATCH_LINES.to_string())
            .arg("--glob")
            .arg(input.glob.as_deref().unwrap_or("!target/"))
            .arg("--glob")
            .arg("!.git/")
            .arg("--")
            .arg(&input.pattern)
            .arg(path);

        let output = command
            .output()
            .context("failed to run ripgrep — is `rg` installed?")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();

        match output.status.code() {
            // ripgrep: 0 = matches found, 1 = no matches, 2+ = error
            Some(0) => {
                let truncated = truncate_output(&stdout);
                result.push_str(&truncated);

                let match_count = stdout.lines().filter(|l| !l.is_empty()).count();
                if match_count >= MAX_MATCH_LINES {
                    result.push_str(&format!(
                        "\n[search capped at {} matching lines — refine the pattern to see more]",
                        MAX_MATCH_LINES
                    ));
                }
            }
            Some(1) => {
                result.push_str("No matches found.");
            }
            Some(code) => {
                result.push_str(&format!("ripgrep failed (exit code {code}):\n{stderr}"));
            }
            None => {
                result.push_str("ripgrep terminated unexpectedly.");
            }
        }

        Ok(result)
    }
}

fn parse_input(input: &str) -> Result<SearchInput> {
    let trimmed = input.trim();

    // Try JSON first; fall back to treating the whole input as the pattern.
    if trimmed.starts_with('{') {
        serde_json::from_str(trimmed).context("invalid search_files JSON")
    } else {
        Ok(SearchInput {
            pattern: trimmed.to_string(),
            path: None,
            glob: None,
        })
    }
}

fn truncate_output(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.to_string();
    }

    let mut end = MAX_OUTPUT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    format!(
        "{}\n... [search output truncated at {} bytes] ...",
        &text[..end],
        MAX_OUTPUT_BYTES
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_input() {
        let input = parse_input(r#"{"pattern":"fn main","path":"src","glob":"*.rs"}"#).unwrap();
        assert_eq!(input.pattern, "fn main");
        assert_eq!(input.path.as_deref(), Some("src"));
        assert_eq!(input.glob.as_deref(), Some("*.rs"));
    }

    #[test]
    fn parses_plain_string() {
        let input = parse_input("fn main").unwrap();
        assert_eq!(input.pattern, "fn main");
        assert!(input.path.is_none());
    }

    #[test]
    fn rejects_empty_pattern() {
        assert!(SearchFiles.execute(r#"{"pattern":"  "}"#).is_err());
    }
}
