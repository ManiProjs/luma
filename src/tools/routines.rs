//! Reusable Teammate routines and workflow learning.
//!
//! Learned routines are deliberately written as reviewable Markdown drafts.
//! They are not executable scripts: a user promotes a reviewed draft into
//! `.luma/routines`, where Teammates can discover and load it as instructions.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

use crate::runtime::RuntimeStore;

use super::Tool;

const MAX_ROUTINE_BYTES: usize = 128 * 1024;

pub struct ListRoutines;
pub struct LoadRoutine;
pub struct LearnWorkflow {
    store: RuntimeStore,
    task_id: String,
}

#[derive(Deserialize)]
struct LoadInput {
    name: String,
}

#[derive(Deserialize)]
struct LearnInput {
    name: String,
    title: String,
    description: String,
    instructions: String,
}

impl LearnWorkflow {
    pub fn new(store: RuntimeStore, task_id: impl Into<String>) -> Self {
        Self {
            store,
            task_id: task_id.into(),
        }
    }
}

impl Tool for ListRoutines {
    fn name(&self) -> &str {
        "list_routines"
    }

    fn description(&self) -> &str {
        "List reusable routines installed for this workspace or user. Routines are reviewed Markdown procedures stored in .luma/routines. Input: {}."
    }

    fn execute(&self, _input: &str) -> Result<String> {
        let routines = discover_routines(&workspace_dir()?);
        if routines.is_empty() {
            return Ok("No routines installed. Review a learned draft in .luma/routine-drafts, then move its directory into .luma/routines.".into());
        }
        Ok(routines
            .iter()
            .map(|(name, path, title, description)| {
                format!("{name} — {title}\n  {description}\n  {}", path.display())
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for LoadRoutine {
    fn name(&self) -> &str {
        "load_routine"
    }

    fn description(&self) -> &str {
        "Load the complete instructions for an installed routine. Input JSON: {\"name\":\"routine-name\"}. Follow its steps while respecting the current task's permissions and approval requirements."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: LoadInput =
            serde_json::from_str(input).context("load_routine: invalid JSON input")?;
        validate_name(&input.name)?;
        let routine = discover_routines(&workspace_dir()?)
            .into_iter()
            .find(|(name, _, _, _)| name == &input.name)
            .ok_or_else(|| {
                anyhow!(
                    "Routine '{}' was not found. Run list_routines to discover installed routines.",
                    input.name
                )
            })?;
        let metadata = fs::metadata(&routine.1)?;
        if metadata.len() as usize > MAX_ROUTINE_BYTES {
            bail!("routine exceeds the 128 KiB size limit");
        }
        let content = fs::read_to_string(&routine.1)
            .with_context(|| format!("reading routine {}", routine.1.display()))?;
        let body = content
            .split_once("---\n")
            .and_then(|(_, rest)| rest.split_once("---\n").map(|(_, body)| body))
            .unwrap_or(&content);
        Ok(format!(
            "# Routine: {}\n\nDescription: {}\n\n{}",
            routine.2,
            routine.3,
            body.trim()
        ))
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for LearnWorkflow {
    fn name(&self) -> &str {
        "learn_workflow"
    }

    fn description(&self) -> &str {
        "Turn a successful workflow into a reviewable reusable routine draft. Include clear, reusable instructions; Luma also attaches a redacted summary of this task's recorded tool actions as reference. Drafts are never run automatically. Input JSON: {\"name\":\"weekly-project-review\",\"title\":\"Weekly project review\",\"description\":\"Review project health and summarize risks\",\"instructions\":\"1. Inspect... 2. ...\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: LearnInput =
            serde_json::from_str(input).context("learn_workflow: invalid JSON input")?;
        validate_name(&input.name)?;
        validate_text("title", &input.title, 160)?;
        validate_text("description", &input.description, 1_024)?;
        validate_text(
            "instructions",
            &input.instructions,
            MAX_ROUTINE_BYTES - 16_384,
        )?;

        let actions = self.store.list_task_actions(&self.task_id)?;
        let mut trace = String::new();
        for action in actions
            .iter()
            .filter(|action| action.status == "finished")
            .take(80)
        {
            trace.push_str(&format!(
                "\n- `{}` — `{}`",
                action.tool_name,
                action.input.replace('\n', " ")
            ));
        }
        if trace.is_empty() {
            trace.push_str("\n- No completed tool actions were recorded for this task.");
        }

        let workspace = workspace_dir()?;
        let path = write_draft(&workspace, &input, &trace)?;
        Ok(format!(
            "Learned workflow '{}' and saved a reviewable routine draft at {}. The recorded action trace is reference material, not a script. Review the steps and move the routine directory into .luma/routines/ to install it. You can then load it with load_routine and use existing schedules or event triggers to automate it.",
            input.title,
            path.display()
        ))
    }
}

fn workspace_dir() -> Result<PathBuf> {
    std::env::current_dir().context("could not determine workspace directory")
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!(
            "routine name must use 1–64 lowercase letters, digits, or hyphens and start with a letter"
        );
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max {
        bail!("routine {label} must contain 1–{} characters", max);
    }
    Ok(())
}

fn write_draft(workspace: &Path, input: &LearnInput, trace: &str) -> Result<PathBuf> {
    let workspace = workspace.canonicalize()?;
    let metadata = workspace.join(".luma");
    if fs::symlink_metadata(&metadata).is_ok_and(|item| item.file_type().is_symlink()) {
        bail!("refusing to write routine drafts through a .luma symlink");
    }
    let drafts = metadata.join("routine-drafts");
    fs::create_dir_all(&drafts)?;
    let drafts = fs::canonicalize(&drafts)?;
    if !drafts.starts_with(&workspace) {
        bail!("routine draft directory resolves outside the workspace");
    }
    let directory = drafts.join(&input.name);
    fs::create_dir(&directory).with_context(|| {
        format!(
            "creating routine draft {}; it may already exist",
            input.name
        )
    })?;
    let description = serde_json::to_string(input.description.trim())?;
    let title = serde_json::to_string(input.title.trim())?;
    let content = format!(
        "---\nname: {}\ntitle: {}\ndescription: {}\nstatus: draft\n---\n\n# Purpose\n\n{}\n\n# Procedure\n\n{}\n\n# Observed execution reference\n\nThe following tool actions were recorded during the task that produced this draft. They are context for reviewing the procedure, not commands to replay. Re-check current state and permissions whenever the routine is used.\n{}\n",
        input.name,
        title,
        description,
        input.description.trim(),
        input.instructions.trim(),
        trace
    );
    if content.len() > MAX_ROUTINE_BYTES {
        let _ = fs::remove_dir(&directory);
        bail!("routine draft exceeds the 128 KiB size limit");
    }
    let path = directory.join("ROUTINE.md");
    if let Err(error) = fs::write(&path, content) {
        let _ = fs::remove_dir_all(&directory);
        return Err(error).with_context(|| format!("writing routine draft {}", path.display()));
    }
    Ok(path)
}

fn discover_routines(workspace: &Path) -> Vec<(String, PathBuf, String, String)> {
    let mut roots = vec![workspace.join(".luma/routines")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".config/luma/routines"));
    }
    let mut routines = Vec::new();
    for root in roots {
        let Ok(root) = fs::canonicalize(root) else {
            continue;
        };
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if validate_name(&name).is_err() {
                continue;
            }
            let path = entry.path().join("ROUTINE.md");
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() as usize > MAX_ROUTINE_BYTES
            {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let title =
                frontmatter_value(&content, "title").unwrap_or_else(|| name.replace('-', " "));
            let description = frontmatter_value(&content, "description")
                .unwrap_or_else(|| "Reusable workflow".into());
            routines.push((name, path, title, description));
        }
    }
    routines.sort_by(|left, right| left.0.cmp(&right.0));
    routines
}

fn frontmatter_value(content: &str, key: &str) -> Option<String> {
    let frontmatter = content.strip_prefix("---\n")?.split_once("\n---")?.0;
    let value = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))?;
    serde_json::from_str::<String>(value)
        .ok()
        .or_else(|| Some(value.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{LearnInput, discover_routines, validate_name, write_draft};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn routine_names_are_safe_path_segments() {
        assert!(validate_name("weekly-review").is_ok());
        for name in ["../oops", "Upper", "bad_name", "-bad", ""] {
            assert!(validate_name(name).is_err());
        }
    }

    #[test]
    fn learned_workflows_are_reviewable_drafts_not_installed_routines() {
        let temp = tempdir().unwrap();
        let input = LearnInput {
            name: "weekly-review".into(),
            title: "Weekly review".into(),
            description: "Review project health".into(),
            instructions: "1. Inspect status.\n2. Summarize risks.".into(),
        };
        let path = write_draft(temp.path(), &input, "\n- `git_status` — `{}`").unwrap();
        assert!(path.ends_with(".luma/routine-drafts/weekly-review/ROUTINE.md"));
        assert!(discover_routines(temp.path()).is_empty());
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("not commands to replay"));
    }
}
