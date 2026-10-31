//! Lightweight structural navigation built on ripgrep.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::process::Command;

use super::Tool;

const MAX_RESULTS: usize = 100;
const MAX_BYTES: usize = 12_000;

#[derive(Debug, Deserialize)]
struct SearchInput {
    #[serde(default)]
    query: String,
    path: Option<String>,
    language: Option<String>,
}

pub struct FindSymbols;
pub struct FindReferences;

impl Tool for FindSymbols {
    fn name(&self) -> &str {
        "find_symbols"
    }

    fn description(&self) -> &str {
        "Find function, type, class, trait, enum, and module declarations. Input JSON: {\"query\":\"optional symbol-name regex\",\"path\":\"optional directory\",\"language\":\"optional rust|python|typescript|javascript|go|swift|java|c|cpp\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: SearchInput = serde_json::from_str(input).context("invalid find_symbols JSON")?;
        let query = if args.query.trim().is_empty() {
            ".*"
        } else {
            args.query.trim()
        };
        let language = args.language.as_deref().unwrap_or("").to_ascii_lowercase();
        let declarations = match language.as_str() {
            "rust" | "rs" => {
                r"(?m)^\s*(pub(?:\([^)]*\))?\s+)?(async\s+)?(fn|struct|enum|trait|type|mod|const|static)\s+"
            }
            "python" | "py" => r"(?m)^\s*(async\s+)?(def|class)\s+",
            "typescript" | "ts" | "javascript" | "js" => {
                r"(?m)^\s*(export\s+)?(default\s+)?(async\s+)?(function|class|interface|type|enum|const|let)\s+"
            }
            "go" => r"(?m)^\s*(func|type|var|const)\s+",
            "swift" => {
                r"(?m)^\s*(public\s+|private\s+|internal\s+|open\s+)*(func|class|struct|enum|protocol|typealias)\s+"
            }
            "java" => {
                r"(?m)^\s*(public|protected|private|static|final|abstract|class|interface|enum|record)\s+"
            }
            "c" | "cpp" | "c++" => {
                r"(?m)^\s*(typedef|struct|enum|class|namespace|[A-Za-z_][\w:*&<>\s]+\s+\**)[A-Za-z_][A-Za-z0-9_]*\s*\("
            }
            _ => {
                r"(?m)^\s*(pub\s+)?(async\s+)?(fn|struct|enum|trait|type|mod|class|def|function|interface|const|func)\s+"
            }
        };
        let pattern = format!("{declarations}.*{query}");
        run_search(&pattern, args.path.as_deref().unwrap_or("."))
    }
}

impl Tool for FindReferences {
    fn name(&self) -> &str {
        "find_references"
    }

    fn description(&self) -> &str {
        "Find usages of a symbol or text across the workspace. Input JSON: {\"query\":\"symbol name\",\"path\":\"optional directory\",\"language\":\"optional file extension\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: SearchInput =
            serde_json::from_str(input).context("invalid find_references JSON")?;
        if args.query.trim().is_empty() {
            bail!("find_references: query cannot be empty");
        }
        let mut pattern = args.query.trim().to_owned();
        if pattern
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            pattern = format!(r"\b{}\b", regex_escape(&pattern));
        }
        run_search(&pattern, args.path.as_deref().unwrap_or(".")).map(|output| {
            if let Some(language) = args.language.as_deref() {
                let language = language.trim_start_matches('.').to_ascii_lowercase();
                let extension = match language.as_str() {
                    "rust" => "rs",
                    "python" => "py",
                    "typescript" => "ts",
                    "javascript" => "js",
                    "c++" => "cpp",
                    other => other,
                };
                output
                    .lines()
                    .filter(|line| line.contains(&format!(".{extension}:")))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                output
            }
        })
    }
}

fn regex_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn run_search(pattern: &str, path: &str) -> Result<String> {
    let mut command = Command::new("rg");
    command.args([
        "--line-number",
        "--with-filename",
        "--max-count",
        &MAX_RESULTS.to_string(),
        "--glob",
        "!.git/",
        "--glob",
        "!target/",
        "--glob",
        "!node_modules/",
    ]);
    command.arg("--").arg(pattern).arg(path);
    let output = command
        .output()
        .context("failed to run ripgrep for code intelligence; install rg")?;
    match output.status.code() {
        Some(0) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut end = text.len().min(MAX_BYTES);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let mut result = text[..end].to_owned();
            if text.len() > MAX_BYTES {
                result.push_str("\n… results truncated");
            }
            Ok(result)
        }
        Some(1) => Ok("No matches found.".into()),
        Some(code) => bail!(
            "ripgrep failed (exit {code}): {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        None => bail!("ripgrep terminated unexpectedly"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_rust_declarations() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("sample.rs"),
            "pub fn calculate() {}\nstruct Engine;\n",
        )
        .unwrap();
        let input =
            serde_json::json!({"query":"calculate", "path":directory.path(), "language":"rust"})
                .to_string();
        let output = FindSymbols.execute(&input).unwrap();
        assert!(output.contains("calculate"));
        assert!(!output.contains("struct Engine"));
    }

    #[test]
    fn finds_symbol_references_as_whole_words() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("sample.py"), "render()\nrenderer()\n").unwrap();
        let input = serde_json::json!({"query":"render", "path":directory.path()}).to_string();
        let output = FindReferences.execute(&input).unwrap();
        assert!(output.contains("render()"));
        assert!(!output.contains("renderer()"));
    }
}
