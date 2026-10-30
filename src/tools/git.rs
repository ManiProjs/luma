use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use super::Tool;

const MAX_OUTPUT_BYTES: usize = 20_000;

#[derive(Debug, Default, Deserialize)]
struct GitStatusInput {
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GitDiffInput {
    path: Option<String>,
    file: Option<String>,
    #[serde(default)]
    staged: bool,
}

pub struct GitStatus;
pub struct GitDiff;

impl Tool for GitStatus {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Inspect repository state without changing files. Input is JSON: {\"path\":\"optional repository directory\"}, or a plain directory path. Reports branch, upstream, and changed files."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args = parse_status_input(input)?;
        let output = git(args.path.as_deref().unwrap_or("."))
            .args(["status", "--short", "--branch"])
            .output()
            .context("failed to execute git status; is Git installed?")?;

        command_output("git status", output)
    }
}

impl Tool for GitDiff {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Read a repository diff without changing files. Input is JSON: {\"path\":\"repository directory\",\"file\":\"optional file path\",\"staged\":false}. Set staged=true to inspect the index instead of the working tree. Output is capped at 20 KB."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: GitDiffInput = if input.trim().is_empty() {
            GitDiffInput::default()
        } else {
            serde_json::from_str(input).context("invalid git_diff JSON")?
        };

        let mut command = git(args.path.as_deref().unwrap_or("."));
        command.args(["diff", "--no-ext-diff", "--unified=3"]);
        if args.staged {
            command.arg("--cached");
        }
        if let Some(file) = args.file.as_deref() {
            if file.trim().is_empty() {
                bail!("git_diff: file path cannot be empty");
            }
            command.arg("--").arg(file);
        }

        let output = command
            .output()
            .context("failed to execute git diff; is Git installed?")?;
        let result = command_output("git diff", output)?;
        if result.is_empty() {
            Ok("No differences found.".to_string())
        } else {
            Ok(truncate(&result))
        }
    }
}

fn git(path: &str) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(path);
    command
}

fn parse_status_input(input: &str) -> Result<GitStatusInput> {
    let input = input.trim();
    if input.is_empty() {
        Ok(GitStatusInput::default())
    } else if input.starts_with('{') {
        serde_json::from_str(input).context("invalid git_status JSON")
    } else {
        Ok(GitStatusInput {
            path: Some(input.to_string()),
        })
    }
}

fn command_output(name: &str, output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("{name} failed: {}", stderr.trim()));
    }
    Ok(truncate(&String::from_utf8_lossy(&output.stdout)))
}

fn truncate(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n... [git output truncated at {} bytes] ...",
        &text[..end],
        MAX_OUTPUT_BYTES
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.name", "Luma Tests"]);
        run(&["config", "user.email", "luma-tests@example.invalid"]);
        fs::write(dir.path().join("sample.txt"), "original\n").unwrap();
        run(&["add", "sample.txt"]);
        run(&["commit", "-qm", "initial"]);
        dir
    }

    #[test]
    fn status_reports_branch_and_changes() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "changed\n").unwrap();
        let input = serde_json::json!({"path": dir.path()}).to_string();
        let output = GitStatus.execute(&input).unwrap();
        assert!(output.contains("## "));
        assert!(output.contains("sample.txt"));
    }

    #[test]
    fn diff_reports_worktree_changes_and_can_filter_file() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "updated\n").unwrap();
        let input = serde_json::json!({"path": dir.path(), "file": "sample.txt"}).to_string();
        let output = GitDiff.execute(&input).unwrap();
        assert!(output.contains("-original"));
        assert!(output.contains("+updated"));
    }

    #[test]
    fn diff_can_inspect_staged_changes() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "staged\n").unwrap();
        let add = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "sample.txt"])
            .status()
            .unwrap();
        assert!(add.success());
        let input = serde_json::json!({"path": dir.path(), "staged": true}).to_string();
        let output = GitDiff.execute(&input).unwrap();
        assert!(output.contains("+staged"));
    }

    #[test]
    fn status_rejects_non_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert!(GitStatus.execute(dir.path().to_str().unwrap()).is_err());
    }
}
