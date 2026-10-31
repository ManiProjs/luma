//! Read-only workspace and Git inspection tools.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use anyhow::{Context, Result, anyhow, bail};
use ignore::WalkBuilder;
use serde::Deserialize;

use super::Tool;

const MAX_OUTPUT: usize = 16_000;

#[derive(Debug, Default, Deserialize)]
struct RepoInput {
    path: Option<String>,
    limit: Option<usize>,
    file: Option<String>,
    revision: Option<String>,
    range: Option<String>,
}

pub struct GitLog;
pub struct GitShow;
pub struct GitBlame;
pub struct GitBranches;
pub struct FileMetadata;
pub struct FindTodos;
pub struct CountLines;
pub struct InspectProject;

impl Tool for GitLog {
    fn name(&self) -> &str {
        "git_log"
    }
    fn description(&self) -> &str {
        "Show recent commits with hash, author, date, and subject. JSON input: {\"path\":\"optional repo\",\"limit\":10} (limit is capped at 50). Read-only."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let args: RepoInput = parse_json(input, "git_log")?;
        let limit = args.limit.unwrap_or(10).clamp(1, 50).to_string();
        git(args.path.as_deref().unwrap_or("."))
            .args([
                "log",
                "--date=short",
                "--pretty=format:%h%x09%ad%x09%an%x09%s",
                "-n",
                &limit,
            ])
            .output()
            .context("failed to execute git log")?
            .into_result("git log")
    }
}

impl Tool for GitShow {
    fn name(&self) -> &str {
        "git_show"
    }
    fn description(&self) -> &str {
        "Inspect a commit or object without changing the worktree. JSON input: {\"path\":\"optional repo\",\"revision\":\"HEAD\",\"file\":\"optional path\"}. Defaults to HEAD."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let args: RepoInput = parse_json(input, "git_show")?;
        let revision = args.revision.as_deref().unwrap_or("HEAD");
        if revision.trim().is_empty() || revision.starts_with('-') {
            bail!("git_show: revision must be a non-option Git revision");
        }
        let mut command = git(args.path.as_deref().unwrap_or("."));
        command.args(["show", "--no-ext-diff", "--format=fuller", revision, "--"]);
        if let Some(file) = args.file.as_deref() {
            command.arg(file);
        }
        command
            .output()
            .context("failed to execute git show")?
            .into_result("git show")
    }
}

impl Tool for GitBlame {
    fn name(&self) -> &str {
        "git_blame"
    }
    fn description(&self) -> &str {
        "Show line-by-line commit attribution for a tracked file. JSON input: {\"path\":\"repo\",\"file\":\"src/main.rs\",\"range\":\"10,30\"}. Read-only."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let args: RepoInput = parse_json(input, "git_blame")?;
        let file = args
            .file
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("git_blame: file is required"))?;
        let mut command = git(args.path.as_deref().unwrap_or("."));
        command.args(["blame", "--date=short"]);
        if let Some(range) = args.range.as_deref() {
            if !valid_blame_range(range) {
                bail!("git_blame: range must be START,END using positive line numbers");
            }
            command.arg("-L").arg(range);
        }
        command.arg("--").arg(file);
        command
            .output()
            .context("failed to execute git blame")?
            .into_result("git blame")
    }
}

impl Tool for GitBranches {
    fn name(&self) -> &str {
        "git_branches"
    }
    fn description(&self) -> &str {
        "List local and remote Git branches with their latest commit. JSON input: {\"path\":\"optional repo\"}. Read-only."
    }
    fn execute(&self, input: &str) -> Result<String> {
        let args: RepoInput = parse_json(input, "git_branches")?;
        git(args.path.as_deref().unwrap_or("."))
            .args(["branch", "--all", "--verbose", "--no-abbrev"])
            .output()
            .context("failed to execute git branch")?
            .into_result("git branch")
    }
}

impl Tool for FileMetadata {
    fn name(&self) -> &str {
        "file_metadata"
    }
    fn description(&self) -> &str {
        "Inspect a file or directory's metadata (type, size, modified time, permissions, symlink status). JSON input: {\"path\":\"path/to/item\"}. Does not read file contents."
    }
    fn execute(&self, input: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Input {
            path: String,
        }
        let args: Input = parse_json(input, "file_metadata")?;
        let path = Path::new(&args.path);
        let link = fs::symlink_metadata(path)
            .with_context(|| format!("cannot inspect {}", path.display()))?;
        let metadata = fs::metadata(path)
            .with_context(|| format!("cannot inspect target {}", path.display()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|time| time.as_secs().to_string())
            .unwrap_or_else(|| "unknown".into());
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            format!("{:04o}", metadata.permissions().mode() & 0o7777)
        };
        #[cfg(not(unix))]
        let permissions = if metadata.permissions().readonly() {
            "readonly"
        } else {
            "writable"
        }
        .to_string();
        Ok(format!(
            "path: {}\ntype: {}\nsymlink: {}\nsize: {} bytes\nmodified_unix: {}\npermissions: {}",
            path.display(),
            if metadata.is_dir() {
                "directory"
            } else if metadata.is_file() {
                "file"
            } else {
                "other"
            },
            link.file_type().is_symlink(),
            metadata.len(),
            modified,
            permissions
        ))
    }
}

impl Tool for FindTodos {
    fn name(&self) -> &str {
        "find_todos"
    }
    fn description(&self) -> &str {
        "Find TODO, FIXME, XXX, HACK, and NOTE comments across the workspace. JSON input: {\"path\":\"optional directory\",\"glob\":\"optional file glob\"}. Results are capped."
    }
    fn execute(&self, input: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Input {
            path: Option<String>,
            glob: Option<String>,
        }
        let args: Input = parse_json(input, "find_todos")?;
        let mut command = Command::new("rg");
        command.args([
            "--line-number",
            "--with-filename",
            "--no-heading",
            "--max-count",
            "30",
            "--glob",
            "!.git/",
            "--glob",
            "!target/",
            "--glob",
            "!node_modules/",
            "--glob",
            "!dist/",
            "--glob",
            "!build/",
        ]);
        if let Some(glob) = args.glob.as_deref() {
            command.arg("--glob").arg(glob);
        }
        command
            .arg("--")
            .arg(r"(?i)\b(TODO|FIXME|XXX|HACK|NOTE)\b")
            .arg(args.path.as_deref().unwrap_or("."));
        match command
            .output()
            .context("failed to run rg for TODO comments; install ripgrep")?
        {
            output if output.status.success() => {
                Ok(truncate(&String::from_utf8_lossy(&output.stdout)))
            }
            output if output.status.code() == Some(1) => Ok("No TODO-style comments found.".into()),
            output => bail!(
                "find_todos failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }
    }
}

impl Tool for CountLines {
    fn name(&self) -> &str {
        "count_lines"
    }
    fn description(&self) -> &str {
        "Count source lines by file extension while respecting ignore files and skipping common build/vendor directories. JSON input: {\"path\":\"optional directory\",\"glob\":\"optional glob such as *.rs\"}. Capped at 2,000 files."
    }
    fn execute(&self, input: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Input {
            path: Option<String>,
            glob: Option<String>,
        }
        let args: Input = parse_json(input, "count_lines")?;
        let root = args.path.as_deref().unwrap_or(".");
        let mut walker = WalkBuilder::new(root);
        walker
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .require_git(false)
            .filter_entry(|entry| {
                !matches!(
                    entry.file_name().to_string_lossy().as_ref(),
                    ".git"
                        | "target"
                        | "node_modules"
                        | "dist"
                        | "build"
                        | "out"
                        | ".venv"
                        | "venv"
                        | "__pycache__"
                )
            });
        let mut totals = std::collections::BTreeMap::<String, (usize, usize)>::new();
        let mut files = 0usize;
        for entry in walker.build() {
            let entry = entry.context("failed to walk workspace")?;
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            if let Some(glob) = args.glob.as_deref() {
                if !path
                    .to_string_lossy()
                    .ends_with(glob.trim_start_matches('*'))
                {
                    continue;
                }
            }
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("[no extension]")
                .to_ascii_lowercase();
            let Ok(content) = fs::read(path) else {
                continue;
            };
            let lines = if content.is_empty() {
                0
            } else {
                content.iter().filter(|byte| **byte == b'\n').count()
                    + usize::from(!content.ends_with(b"\n"))
            };
            let total = totals.entry(extension).or_default();
            total.0 += 1;
            total.1 += lines;
            files += 1;
            if files >= 2_000 {
                break;
            }
        }
        let mut output = format!(
            "Files counted: {}{}\n",
            files,
            if files == 2_000 { " (cap reached)" } else { "" }
        );
        for (extension, (count, lines)) in totals {
            output.push_str(&format!("{extension}: {count} files, {lines} lines\n"));
        }
        Ok(output)
    }
}

impl Tool for InspectProject {
    fn name(&self) -> &str {
        "inspect_project"
    }
    fn description(&self) -> &str {
        "Identify project manifests and summarize available scripts/targets. Supports Cargo, Node, Python, Flutter/Dart, Go, and Swift manifests. JSON input: {\"path\":\"optional project directory\"}."
    }
    fn execute(&self, input: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Input {
            path: Option<String>,
        }
        let args: Input = parse_json(input, "inspect_project")?;
        let root = Path::new(args.path.as_deref().unwrap_or("."));
        let manifests = [
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "requirements.txt",
            "pubspec.yaml",
            "go.mod",
            "Package.swift",
            "Makefile",
        ];
        let mut output = format!(
            "Project directory: {}\n",
            root.canonicalize()
                .unwrap_or_else(|_| root.to_path_buf())
                .display()
        );
        let mut found = 0;
        for name in manifests {
            let path = root.join(name);
            if !path.is_file() {
                continue;
            }
            found += 1;
            output.push_str(&format!("\n[{name}]\n"));
            let content = fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<_> = content.lines().take(35).collect();
            output.push_str(&lines.join("\n"));
            if content.lines().count() > lines.len() {
                output.push_str("\n... [manifest excerpt truncated]");
            }
            output.push('\n');
            if output.len() > MAX_OUTPUT {
                break;
            }
        }
        if found == 0 {
            output.push_str("No supported project manifests found at this directory level.");
        }
        Ok(truncate(&output))
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(input: &str, name: &str) -> Result<T> {
    serde_json::from_str(input).with_context(|| format!("invalid {name} JSON input"))
}
fn git(path: &str) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(path);
    command
}
trait OutputResult {
    fn into_result(self, name: &str) -> Result<String>;
}
impl OutputResult for Output {
    fn into_result(self, name: &str) -> Result<String> {
        if !self.status.success() {
            bail!(
                "{name} failed: {}",
                String::from_utf8_lossy(&self.stderr).trim()
            );
        }
        Ok(truncate(&String::from_utf8_lossy(&self.stdout)))
    }
}
fn truncate(value: &str) -> String {
    if value.len() <= MAX_OUTPUT {
        return value.to_owned();
    }
    let mut end = MAX_OUTPUT;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n... [output truncated at {MAX_OUTPUT} bytes]",
        &value[..end]
    )
}
fn valid_blame_range(range: &str) -> bool {
    range.split_once(',').is_some_and(|(start, end)| {
        start.parse::<usize>().is_ok_and(|n| n > 0)
            && end
                .parse::<usize>()
                .is_ok_and(|n| n >= start.parse::<usize>().unwrap_or(usize::MAX))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let output = git(dir.path().to_str().unwrap())
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
        run(&["config", "user.name", "Luma"]);
        run(&["config", "user.email", "luma@example.invalid"]);
        let mut file = fs::File::create(dir.path().join("sample.rs")).unwrap();
        writeln!(file, "fn alpha() {{}}\n// TODO: improve").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "initial commit"]);
        dir
    }

    #[test]
    fn git_log_lists_commits() {
        let dir = repo();
        let out = GitLog
            .execute(&serde_json::json!({"path":dir.path()}).to_string())
            .unwrap();
        assert!(out.contains("initial commit"));
    }
    #[test]
    fn git_show_reads_a_revision() {
        let dir = repo();
        let out = GitShow
            .execute(
                &serde_json::json!({"path":dir.path(),"revision":"HEAD","file":"sample.rs"})
                    .to_string(),
            )
            .unwrap();
        assert!(out.contains("alpha"));
    }
    #[test]
    fn git_show_rejects_option_injection() {
        assert!(GitShow.execute(r#"{"revision":"--help"}"#).is_err());
    }
    #[test]
    fn git_blame_reads_a_line_range() {
        let dir = repo();
        let out = GitBlame
            .execute(
                &serde_json::json!({"path":dir.path(),"file":"sample.rs","range":"1,1"})
                    .to_string(),
            )
            .unwrap();
        assert!(out.contains("alpha"));
    }
    #[test]
    fn git_branches_lists_current_branch() {
        let dir = repo();
        let out = GitBranches
            .execute(&serde_json::json!({"path":dir.path()}).to_string())
            .unwrap();
        assert!(out.contains("initial commit"));
    }
    #[test]
    fn metadata_reports_file_type_and_size() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().write_all(b"hello").unwrap();
        let out = FileMetadata
            .execute(&serde_json::json!({"path":file.path()}).to_string())
            .unwrap();
        assert!(out.contains("type: file"));
        assert!(out.contains("size: 5 bytes"));
    }
    #[test]
    fn finds_todo_comments() {
        let dir = repo();
        let out = FindTodos
            .execute(&serde_json::json!({"path":dir.path()}).to_string())
            .unwrap();
        assert!(out.contains("TODO"));
    }
    #[test]
    fn counts_lines_by_extension() {
        let dir = repo();
        let out = CountLines
            .execute(&serde_json::json!({"path":dir.path()}).to_string())
            .unwrap();
        assert!(out.contains("rs: 1 files, 2 lines"));
    }
    #[test]
    fn inspects_project_manifests() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname='demo'\n").unwrap();
        let out = InspectProject
            .execute(&serde_json::json!({"path":dir.path()}).to_string())
            .unwrap();
        assert!(out.contains("Cargo.toml"));
        assert!(out.contains("name='demo'"));
    }
}
