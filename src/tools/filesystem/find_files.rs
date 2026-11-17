use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use serde::Deserialize;

use crate::tools::Tool;

const DEFAULT_LIMIT: usize = 30;
const MAX_LIMIT: usize = 100;
const MAX_QUERY_CHARS: usize = 256;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Deserialize)]
struct FindFilesInput {
    path: Option<String>,
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
struct FileMatch {
    path: String,
    score: i64,
}

pub struct FindFiles;

impl Tool for FindFiles {
    fn name(&self) -> &str {
        "find_files"
    }

    fn description(&self) -> &str {
        "Find workspace files by fuzzy path match, respecting .gitignore and skipping .git, node_modules, target, dist and common generated folders. Input JSON: {\"query\":\"agent loop\",\"path\":\"optional workspace-relative directory\",\"limit\":30}. Returns up to 100 ranked workspace-relative paths."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let args: FindFilesInput =
            serde_json::from_str(input).context("invalid find_files JSON")?;
        let query = args.query.trim();
        if query.is_empty() {
            bail!("find_files requires a non-empty query");
        }
        if query.chars().count() > MAX_QUERY_CHARS {
            bail!("find_files query is too long (maximum {MAX_QUERY_CHARS} characters)");
        }

        let workspace = std::env::current_dir()?.canonicalize()?;
        let root = resolve_root(&workspace, args.path.as_deref())?;
        let results = find_files_in(&root, query, args.limit.unwrap_or(DEFAULT_LIMIT))?;
        Ok(serde_json::to_string_pretty(&results)?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

fn resolve_root(workspace: &Path, requested: Option<&str>) -> Result<PathBuf> {
    let candidate = requested.map_or_else(|| workspace.to_path_buf(), |path| workspace.join(path));
    let root = candidate
        .canonicalize()
        .context("file finder directory does not exist")?;
    if !root.starts_with(workspace) {
        bail!("file finder path must stay inside the active workspace");
    }
    if !root.is_dir() {
        bail!("file finder path must be a directory");
    }
    Ok(root)
}

fn find_files_in(root: &Path, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut matches = Vec::new();
    let mut walker = WalkBuilder::new(root);
    walker
        .max_depth(Some(MAX_DEPTH))
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_string_lossy().as_ref(),
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "out"
                    | "__pycache__"
                    | ".venv"
                    | "venv"
                    | "env"
            )
        });

    for result in walker.build() {
        let entry = result.context("failed to read workspace entry")?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .context("file finder returned a path outside its root")?;
        let path = relative.to_string_lossy().replace('\\', "/");
        if let Some(score) = fuzzy_score(&path, query) {
            matches.push(FileMatch { path, score });
        }
    }

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
            .then_with(|| left.path.cmp(&right.path))
    });
    matches.truncate(limit);
    Ok(matches)
}

fn fuzzy_score(candidate: &str, query: &str) -> Option<i64> {
    let candidate = candidate.to_lowercase();
    let query = query.to_lowercase();
    if candidate == query {
        return Some(10_000);
    }
    if candidate.starts_with(&query) {
        return Some(8_000 - candidate.chars().count() as i64);
    }
    if let Some(index) = candidate.find(&query) {
        return Some(6_000 - index as i64 - candidate.chars().count() as i64 / 20);
    }

    let candidate_chars: Vec<char> = candidate.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    let mut cursor = 0;
    let mut previous = None;
    let mut score = 0_i64;
    for character in query_chars {
        let relative = candidate_chars[cursor..]
            .iter()
            .position(|candidate| *candidate == character)?;
        let index = cursor + relative;
        let consecutive = previous.is_some_and(|previous| index == previous + 1);
        let boundary = index == 0
            || candidate_chars
                .get(index.wrapping_sub(1))
                .is_some_and(|previous| matches!(previous, '/' | '_' | '-' | '.' | ' '));
        score += if consecutive {
            24
        } else {
            12 - relative.min(10) as i64
        };
        if boundary {
            score += 16;
        }
        previous = Some(index);
        cursor = index + 1;
        if cursor >= candidate_chars.len() && previous != Some(candidate_chars.len() - 1) {
            return None;
        }
    }
    Some(score - candidate_chars.len() as i64 / 10)
}

#[cfg(test)]
mod tests {
    use super::{find_files_in, fuzzy_score, resolve_root};
    use std::fs;

    #[test]
    fn fuzzy_ranking_prefers_exact_and_consecutive_matches() {
        assert!(
            fuzzy_score("src/agent.rs", "agent") > fuzzy_score("src/async_generator.rs", "agent")
        );
        assert!(
            fuzzy_score("src/agent.rs", "src/agent")
                > fuzzy_score("src/async_generator.rs", "src/agent")
        );
    }

    #[test]
    fn file_search_respects_ignores_and_result_cap() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::create_dir_all(root.path().join("target")).unwrap();
        for index in 0..140 {
            fs::write(
                root.path().join("src").join(format!("agent-{index:03}.rs")),
                "",
            )
            .unwrap();
        }
        fs::write(root.path().join("target/agent-generated.rs"), "").unwrap();
        fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(root.path().join("ignored")).unwrap();
        fs::write(root.path().join("ignored/agent.rs"), "").unwrap();

        let matches = find_files_in(root.path(), "agent", 500).unwrap();
        assert_eq!(matches.len(), 100);
        assert!(matches.iter().all(|item| item.path.starts_with("src/")));
    }

    #[test]
    fn file_search_does_not_follow_symlinks_outside_root() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret-agent.rs"), "").unwrap();
        symlink(outside.path(), root.path().join("external")).unwrap();
        let matches = find_files_in(root.path(), "agent", 100).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn resolve_root_rejects_traversal_outside_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        assert!(resolve_root(workspace.path(), Some(outside.path().to_str().unwrap())).is_err());
        assert!(resolve_root(workspace.path(), Some("../outside")).is_err());
    }
}
