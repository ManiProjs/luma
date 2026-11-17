//! Durable, bounded checkpoints for work that may outlive a Luma process.

use anyhow::Result;
use serde::{Deserialize, Serialize};

const MAX_WORKSPACE_CHARS: usize = 4_096;
const MAX_REQUEST_CHARS: usize = 16_000;
const MAX_PLAN_CHARS: usize = 16_000;
const MAX_OBSERVATIONS: usize = 40;
const MAX_OBSERVATION_CHARS: usize = 8_000;
const MAX_COMPLETED_ACTIONS: usize = 100;
const MAX_ACTION_CHARS: usize = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    pub workspace: String,
    pub request: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub plan_approved: bool,
    #[serde(default)]
    pub observations: Vec<String>,
    #[serde(default)]
    pub completed_actions: Vec<String>,
    pub status: TaskStatus,
    pub updated_at: u64,
}

fn current_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    InProgress,
    Interrupted,
    Completed,
}

impl TaskCheckpoint {
    /// Bound persisted context so a long-running task cannot grow its checkpoint forever.
    pub fn normalize(&mut self) {
        self.schema_version = current_schema_version();
        self.workspace = bounded_text(&self.workspace, MAX_WORKSPACE_CHARS);
        self.request = bounded_text(&self.request, MAX_REQUEST_CHARS);
        self.plan = self
            .plan
            .as_deref()
            .map(|value| bounded_text(value, MAX_PLAN_CHARS));
        bound_history(
            &mut self.observations,
            MAX_OBSERVATIONS,
            MAX_OBSERVATION_CHARS,
        );
        bound_history(
            &mut self.completed_actions,
            MAX_COMPLETED_ACTIONS,
            MAX_ACTION_CHARS,
        );
    }
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let output: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{output}\n[truncated by Luma]")
    } else {
        output
    }
}

fn bound_history(items: &mut Vec<String>, max_items: usize, max_chars: usize) {
    if items.len() > max_items {
        items.drain(..items.len() - max_items);
    }
    for item in items {
        *item = bounded_text(item, max_chars);
    }
}

#[cfg(not(test))]
fn checkpoint_path() -> Result<std::path::PathBuf> {
    use anyhow::Context;
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let workspace = std::env::current_dir().context("could not determine workspace")?;
    let mut hasher = DefaultHasher::new();
    workspace.hash(&mut hasher);
    Ok(crate::config::config_dir()
        .join("tasks")
        .join(format!("{:016x}.json", hasher.finish())))
}

pub fn save(checkpoint: &TaskCheckpoint) -> Result<()> {
    #[cfg(test)]
    {
        let _ = checkpoint;
        Ok(())
    }
    #[cfg(not(test))]
    {
        write_checkpoint(&checkpoint_path()?, checkpoint)
    }
}

pub fn load() -> Result<Option<TaskCheckpoint>> {
    #[cfg(test)]
    {
        Ok(None)
    }
    #[cfg(not(test))]
    {
        read_checkpoint(&checkpoint_path()?)
    }
}

static TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn write_checkpoint(path: &std::path::Path, checkpoint: &TaskCheckpoint) -> Result<()> {
    use anyhow::Context;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
    };

    let parent = path.parent().context("checkpoint path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating checkpoint directory {}", parent.display()))?;

    let mut normalized = checkpoint.clone();
    normalized.normalize();
    let data = serde_json::to_vec_pretty(&normalized)?;
    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = path.with_extension(format!("json.tmp-{}-{sequence}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("creating temporary checkpoint {}", temporary.display())
                });
            }
        }
    };

    let result = (|| -> Result<()> {
        file.write_all(&data)
            .with_context(|| format!("writing temporary checkpoint {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing temporary checkpoint {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, path)
            .with_context(|| format!("replacing checkpoint {}", path.display()))?;
        // Persist the directory entry update as well as the file contents.
        #[cfg(unix)]
        OpenOptions::new()
            .read(true)
            .open(parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("syncing checkpoint directory {}", parent.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_checkpoint(path: &std::path::Path) -> Result<Option<TaskCheckpoint>> {
    use anyhow::Context;
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let mut checkpoint: TaskCheckpoint =
        serde_json::from_slice(&data).with_context(|| format!("parsing {}", path.display()))?;
    checkpoint.normalize();
    Ok(Some(checkpoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TaskCheckpoint {
        TaskCheckpoint {
            schema_version: 1,
            workspace: "/tmp/project".into(),
            request: "add a feature".into(),
            plan: Some("inspect, implement, test".into()),
            plan_approved: true,
            observations: vec!["read src/main.rs".into()],
            completed_actions: vec!["read_file".into()],
            status: TaskStatus::Interrupted,
            updated_at: 42,
        }
    }

    #[test]
    fn checkpoint_round_trips() {
        let checkpoint = sample();
        let encoded = serde_json::to_string(&checkpoint).unwrap();
        let decoded: TaskCheckpoint = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.request, checkpoint.request);
        assert_eq!(decoded.status, TaskStatus::Interrupted);
        assert_eq!(decoded.observations, checkpoint.observations);
    }

    #[test]
    fn old_checkpoint_json_restores_with_defaults() {
        let json = r#"{"workspace":"/tmp/project","request":"continue","status":"interrupted","updated_at":7}"#;
        let checkpoint: TaskCheckpoint = serde_json::from_str(json).unwrap();
        assert_eq!(checkpoint.schema_version, 1);
        assert!(checkpoint.observations.is_empty());
        assert!(checkpoint.completed_actions.is_empty());
        assert!(!checkpoint.plan_approved);
    }

    #[test]
    fn checkpoint_history_is_bounded_to_recent_entries() {
        let mut checkpoint = sample();
        checkpoint.observations = (0..MAX_OBSERVATIONS + 3)
            .map(|index| format!("observation {index}"))
            .collect();
        checkpoint.completed_actions = (0..MAX_COMPLETED_ACTIONS + 2)
            .map(|index| format!("action {index}"))
            .collect();
        checkpoint.request = "界".repeat(MAX_REQUEST_CHARS + 5);
        checkpoint.normalize();
        assert_eq!(checkpoint.observations.len(), MAX_OBSERVATIONS);
        assert_eq!(checkpoint.observations[0], "observation 3");
        assert_eq!(checkpoint.completed_actions.len(), MAX_COMPLETED_ACTIONS);
        assert!(
            checkpoint
                .request
                .starts_with(&"界".repeat(MAX_REQUEST_CHARS))
        );
        assert!(checkpoint.request.ends_with("[truncated by Luma]"));
    }

    #[test]
    fn checkpoint_write_is_atomic_and_loadable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tasks/checkpoint.json");
        write_checkpoint(&path, &sample()).unwrap();
        let mut replacement = sample();
        replacement.request = "updated request".into();
        write_checkpoint(&path, &replacement).unwrap();
        let loaded = read_checkpoint(&path).unwrap().unwrap();
        assert_eq!(loaded.request, "updated request");
        assert_eq!(loaded.status, TaskStatus::Interrupted);
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }
}
