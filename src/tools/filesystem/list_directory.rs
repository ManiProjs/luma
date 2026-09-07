use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use serde::Deserialize;

use crate::tools::Tool;

/// Maximum depth to recurse. Keeps huge monorepos from exploding the output.
const MAX_DEPTH: usize = 3;

/// Maximum total entries returned. Prevents context flooding.
const MAX_ENTRIES: usize = 200;

#[derive(Debug, Deserialize)]
struct ListInput {
    path: Option<String>,
    depth: Option<usize>,
}

pub struct ListDirectory;

impl Tool for ListDirectory {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories recursively, respecting .gitignore. \
         Input is JSON: {\"path\":\"dir\",\"depth\":2} or a plain path string. \
         Depth defaults to 3, results capped at 200 entries."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input = parse_input(input)?;

        let path = input.path.as_deref().unwrap_or(".");
        let depth = input.depth.unwrap_or(MAX_DEPTH).min(MAX_DEPTH);

        let mut entries = Vec::new();
        let mut walker = WalkBuilder::new(path);

        walker
            .max_depth(Some(depth))
            .hidden(false) // show hidden files (except .git — see below)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .require_git(false) // respect .gitignore even outside a git repo
            .filter_entry(|entry| {
                // Always skip .git and common build/dependency dirs.
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git"
                        | "target"
                        | "node_modules"
                        | "dist"
                        | "build"
                        | "out"
                        | "__pycache__"
                        | ".venv"
                        | "venv"
                        | "env"
                        | ".idea"
                        | ".vscode"
                )
            });

        for result in walker.build() {
            let entry = result.context("failed to read directory entry")?;

            let display_path = entry
                .path()
                .strip_prefix(path)
                .unwrap_or(entry.path())
                .display()
                .to_string();

            // Skip the root itself.
            if display_path.is_empty() {
                continue;
            }

            let suffix = if entry.file_type().is_some_and(|t| t.is_dir()) {
                "/"
            } else {
                ""
            };

            entries.push(format!("{}{}", display_path, suffix));

            if entries.len() >= MAX_ENTRIES {
                entries.push(format!(
                    "... [listing capped at {} entries — narrow the path or increase depth]",
                    MAX_ENTRIES
                ));
                break;
            }
        }

        if entries.is_empty() {
            bail!("list_directory: no entries found in '{}'", path);
        }

        Ok(entries.join("\n"))
    }
}

fn parse_input(input: &str) -> Result<ListInput> {
    let trimmed = input.trim();

    if trimmed.starts_with('{') {
        serde_json::from_str(trimmed).context("invalid list_directory JSON")
    } else {
        Ok(ListInput {
            path: Some(trimmed.to_string()),
            depth: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_input() {
        let input = parse_input(r#"{"path":"src","depth":2}"#).unwrap();
        assert_eq!(input.path.as_deref(), Some("src"));
        assert_eq!(input.depth, Some(2));
    }

    #[test]
    fn parses_plain_string() {
        let input = parse_input("src").unwrap();
        assert_eq!(input.path.as_deref(), Some("src"));
        assert!(input.depth.is_none());
    }

    #[test]
    fn lists_current_directory() {
        // These tests must run from the repo root — they are cwd-relative.
        // When the whole test binary runs, agent-loop tests may have already
        // changed the process cwd, so we create our own temp dir instead.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let input = serde_json::json!({"path": dir.path().to_str().unwrap()}).to_string();
        let out = ListDirectory.execute(&input).unwrap();
        assert!(out.contains("src/"));
        assert!(out.contains("Cargo.toml"));
    }

    #[test]
    fn respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(dir.path().join("ignored.txt"), "x").unwrap();
        std::fs::write(dir.path().join("kept.txt"), "x").unwrap();

        let input = serde_json::json!({"path": dir.path().to_str().unwrap()}).to_string();
        let out = ListDirectory.execute(&input).unwrap();
        assert!(out.contains("kept.txt"));
        assert!(!out.contains("ignored.txt"));
    }
}
