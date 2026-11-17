use std::{
    fs,
    path::{Component, Path},
    process::Command,
};

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

#[derive(Debug, Deserialize)]
struct GitFileInput {
    path: String,
    file: String,
}

pub struct GitStatus;
pub struct GitDiff;
pub struct GitChanges;
pub struct GitStage;
pub struct GitUnstage;
pub struct GitRevert;

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

        let repository = args.path.as_deref().unwrap_or(".");
        if let Some(file) = args.file.as_deref() {
            if file.trim().is_empty() {
                bail!("git_diff: file path cannot be empty");
            }
            let (repository_path, _) = validate_git_file(repository, file, false)?;
            let target = repository_path.join(file);
            if !args.staged && target.exists() {
                let canonical_target = target
                    .canonicalize()
                    .context("Git diff path does not exist")?;
                if !canonical_target.starts_with(&repository_path) {
                    bail!("Git diff path resolves outside the repository");
                }
                let repository_string = repository_path
                    .to_str()
                    .ok_or_else(|| anyhow!("repository path is not valid UTF-8"))?;
                let tracked = git(repository_string)
                    .args(["ls-files", "--error-unmatch", "--", file])
                    .output()
                    .context("failed to inspect Git index")?;
                if !tracked.status.success() {
                    let metadata = fs::metadata(&canonical_target)?;
                    if !metadata.is_file() {
                        return Ok(
                            "Untracked directory. Select a file inside it to inspect its contents."
                                .into(),
                        );
                    }
                    if metadata.len() > 2 * 1024 * 1024 {
                        bail!("untracked files larger than 2 MiB are not previewed");
                    }
                    let bytes = fs::read(&canonical_target)?;
                    if bytes.contains(&0) {
                        bail!("binary files do not have a text diff preview");
                    }
                    let content = String::from_utf8_lossy(&bytes);
                    return Ok(truncate(&format!(
                        "New file: {file}\n\n{}",
                        content
                            .lines()
                            .map(|line| format!("+{line}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )));
                }
            }
        }

        let mut command = git(repository);
        command.args(["diff", "--no-ext-diff", "--unified=3"]);
        if args.staged {
            command.arg("--cached");
        }
        if let Some(file) = args.file.as_deref() {
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

impl Tool for GitChanges {
    fn name(&self) -> &str {
        "git_changes"
    }

    fn description(&self) -> &str {
        "Return structured repository status for the Git Explorer. Input JSON: {\"path\":\"repository directory\"}. Includes branch, index/worktree status, file kind, and optional diff statistics."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args = parse_status_input(input)?;
        let repository = args.path.as_deref().unwrap_or(".");
        let branch = git(repository)
            .args(["branch", "--show-current"])
            .output()
            .context("failed to execute git branch; is Git installed?")?;
        let branch = command_output("git branch", branch)?.trim().to_owned();
        let status = git(repository)
            .args(["status", "--porcelain=v1", "--untracked-files=all"])
            .output()
            .context("failed to execute git status; is Git installed?")?;
        let status = command_output("git status", status)?;
        let numstat = git(repository)
            .args(["diff", "HEAD", "--no-renames", "--numstat"])
            .output()
            .context("failed to execute git diff --numstat; is Git installed?")?;
        let numstat = if numstat.status.success() {
            String::from_utf8_lossy(&numstat.stdout).into_owned()
        } else {
            String::new()
        };
        let mut stats = std::collections::HashMap::new();
        for line in numstat.lines() {
            let mut fields = line.splitn(3, '\t');
            let (Some(additions), Some(deletions), Some(path)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            if additions == "-" || deletions == "-" {
                continue;
            }
            if let (Ok(additions), Ok(deletions)) =
                (additions.parse::<u64>(), deletions.parse::<u64>())
            {
                stats.insert(path.to_owned(), (additions, deletions));
            }
        }
        let files = status
            .lines()
            .filter(|line| line.len() >= 3)
            .map(|line| {
                let index_status = line.chars().next().unwrap_or(' ');
                let worktree_status = line.chars().nth(1).unwrap_or(' ');
                let raw_path = line.get(3..).unwrap_or_default();
                let renamed = raw_path.contains(" -> ");
                let path = if renamed {
                    raw_path
                        .rsplit_once(" -> ")
                        .map_or(raw_path, |(_, new)| new)
                } else {
                    raw_path
                };
                let code = if index_status == '?' {
                    '?'
                } else if index_status != ' ' {
                    index_status
                } else {
                    worktree_status
                };
                let kind = match code {
                    '?' => "untracked",
                    'A' => "added",
                    'D' => "deleted",
                    'R' | 'C' if renamed => "renamed",
                    _ => "modified",
                };
                let (additions, deletions) = stats.get(path).copied().unzip();
                serde_json::json!({
                    "path": path,
                    "indexStatus": index_status.to_string(),
                    "worktreeStatus": worktree_status.to_string(),
                    "kind": kind,
                    "additions": additions,
                    "deletions": deletions,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "branch": if branch.is_empty() { "HEAD" } else { &branch },
            "files": files,
        }))?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for GitStage {
    fn name(&self) -> &str {
        "git_stage"
    }

    fn description(&self) -> &str {
        "Stage one existing file inside a Git repository. Input JSON: {\"path\":\"repository directory\",\"file\":\"workspace-relative file path\"}. Absolute paths, parent traversal, and symlink escapes are rejected."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: GitFileInput = serde_json::from_str(input).context("invalid git_stage JSON")?;
        // `git add` also stages tracked deletions, so the final path may no
        // longer exist; the parent directory is still canonicalized below.
        let (repository, file) = validate_git_file(&args.path, &args.file, false)?;
        let output = git(repository
            .to_str()
            .ok_or_else(|| anyhow!("repository path is not valid UTF-8"))?)
        .args(["add", "--"])
        .arg(file)
        .output()
        .context("failed to execute git add; is Git installed?")?;
        command_output("git add", output)?;
        Ok(format!("Staged {}.", args.file))
    }
}

impl Tool for GitUnstage {
    fn name(&self) -> &str {
        "git_unstage"
    }

    fn description(&self) -> &str {
        "Unstage one file inside a Git repository without discarding its working-tree changes. Input JSON: {\"path\":\"repository directory\",\"file\":\"workspace-relative file path\"}. Absolute paths, parent traversal, and symlink escapes are rejected."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: GitFileInput = serde_json::from_str(input).context("invalid git_unstage JSON")?;
        let (repository, file) = validate_git_file(&args.path, &args.file, false)?;
        let output = git(repository
            .to_str()
            .ok_or_else(|| anyhow!("repository path is not valid UTF-8"))?)
        .args(["restore", "--staged", "--"])
        .arg(file)
        .output()
        .context("failed to execute git restore --staged; is Git installed?")?;
        command_output("git restore --staged", output)?;
        Ok(format!(
            "Unstaged {}. Working-tree changes were preserved.",
            args.file
        ))
    }
}

impl Tool for GitRevert {
    fn name(&self) -> &str {
        "git_revert"
    }

    fn description(&self) -> &str {
        "Discard staged and working-tree changes to one tracked file by restoring it from HEAD. Input JSON: {\"path\":\"repository directory\",\"file\":\"workspace-relative file path\"}. The UI must ask for confirmation before invoking this destructive operation."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: GitFileInput = serde_json::from_str(input).context("invalid git_revert JSON")?;
        let (repository, file) = validate_git_file(&args.path, &args.file, false)?;
        let tracked = git(repository
            .to_str()
            .ok_or_else(|| anyhow!("repository path is not valid UTF-8"))?)
        .args(["ls-files", "--error-unmatch", "--", &file])
        .output()
        .context("failed to inspect tracked Git paths")?;
        if !tracked.status.success() {
            bail!("only tracked files can be reverted from the Git Explorer");
        }
        let output = git(repository
            .to_str()
            .ok_or_else(|| anyhow!("repository path is not valid UTF-8"))?)
        .args([
            "restore",
            "--source=HEAD",
            "--staged",
            "--worktree",
            "--",
            &file,
        ])
        .output()
        .context("failed to execute git restore")?;
        command_output("git restore", output)?;
        Ok(format!("Restored {} from HEAD.", args.file))
    }
}

fn validate_git_file(
    repository: &str,
    file: &str,
    must_exist: bool,
) -> Result<(std::path::PathBuf, String)> {
    let repository = Path::new(repository)
        .canonicalize()
        .context("Git repository path does not exist")?;
    let relative = Path::new(file);
    if file.trim().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("Git file path must be a non-empty workspace-relative path without traversal");
    }
    let parent = relative
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical_parent = repository
        .join(parent)
        .canonicalize()
        .context("Git file parent directory does not exist")?;
    if !canonical_parent.starts_with(&repository) {
        bail!("Git file path resolves outside the repository");
    }
    if must_exist {
        let target = repository
            .join(relative)
            .canonicalize()
            .context("Git file does not exist")?;
        if !target.starts_with(&repository) || !target.is_file() {
            bail!("Git file path resolves outside the repository or is not a file");
        }
    }
    Ok((repository, file.to_string()))
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
    fn structured_changes_include_index_and_worktree_status() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "changed\n").unwrap();
        let input = serde_json::json!({"path": dir.path()}).to_string();
        let output: serde_json::Value =
            serde_json::from_str(&GitChanges.execute(&input).unwrap()).unwrap();
        assert_eq!(output["files"][0]["path"], "sample.txt");
        assert_eq!(output["files"][0]["worktreeStatus"], "M");
        assert_eq!(output["files"][0]["indexStatus"], " ");
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
    fn diff_previews_untracked_files() {
        let dir = repository();
        fs::write(dir.path().join("new.txt"), "first\nsecond\n").unwrap();
        let input = serde_json::json!({"path": dir.path(), "file": "new.txt"}).to_string();
        let output = GitDiff.execute(&input).unwrap();
        assert!(output.contains("New file: new.txt"));
        assert!(output.contains("+first"));
        assert!(output.contains("+second"));
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

    #[test]
    fn stage_and_unstage_preserve_worktree_changes() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "changed\n").unwrap();
        let input = serde_json::json!({"path": dir.path(), "file": "sample.txt"}).to_string();
        assert!(GitStage.execute(&input).unwrap().contains("Staged"));
        let staged = GitStatus.execute(dir.path().to_str().unwrap()).unwrap();
        assert!(staged.contains("M  sample.txt"), "{staged}");
        assert!(GitUnstage.execute(&input).unwrap().contains("Unstaged"));
        let unstaged = GitStatus.execute(dir.path().to_str().unwrap()).unwrap();
        assert!(unstaged.contains(" M sample.txt"), "{unstaged}");
    }

    #[test]
    fn stage_can_include_a_tracked_deletion() {
        let dir = repository();
        fs::remove_file(dir.path().join("sample.txt")).unwrap();
        let input = serde_json::json!({"path": dir.path(), "file": "sample.txt"}).to_string();
        GitStage.execute(&input).unwrap();
        let status = GitStatus.execute(dir.path().to_str().unwrap()).unwrap();
        assert!(status.contains("D  sample.txt"), "{status}");
    }

    #[test]
    fn revert_restores_tracked_file_from_head() {
        let dir = repository();
        fs::write(dir.path().join("sample.txt"), "changed\n").unwrap();
        let input = serde_json::json!({"path": dir.path(), "file": "sample.txt"}).to_string();
        assert!(GitRevert.execute(&input).unwrap().contains("Restored"));
        assert_eq!(
            fs::read_to_string(dir.path().join("sample.txt")).unwrap(),
            "original\n"
        );
    }

    #[test]
    fn stage_and_unstage_reject_traversal_paths() {
        let dir = repository();
        let input = serde_json::json!({"path": dir.path(), "file": "../outside.txt"}).to_string();
        assert!(GitStage.execute(&input).is_err());
        assert!(GitUnstage.execute(&input).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn stage_rejects_symlink_parent_escaping_repository() {
        use std::os::unix::fs::symlink;
        let dir = repository();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "outside\n").unwrap();
        symlink(outside.path(), dir.path().join("escape")).unwrap();
        let input =
            serde_json::json!({"path": dir.path(), "file": "escape/secret.txt"}).to_string();
        assert!(GitStage.execute(&input).is_err());
    }
}
