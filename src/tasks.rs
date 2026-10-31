//! Durable checkpoints for work that may outlive a Luma process.

#[cfg(not(test))]
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::fs;
#[cfg(not(test))]
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCheckpoint {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    InProgress,
    Interrupted,
    Completed,
}

#[cfg(not(test))]
fn checkpoint_path() -> Result<PathBuf> {
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
        return Ok(());
    }
    #[cfg(not(test))]
    {
        let path = checkpoint_path()?;
        fs::create_dir_all(path.parent().expect("checkpoint path has parent"))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(checkpoint)?)
            .with_context(|| format!("writing task checkpoint {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("saving task checkpoint {}", path.display()))
    }
}

pub fn load() -> Result<Option<TaskCheckpoint>> {
    #[cfg(test)]
    {
        return Ok(None);
    }
    #[cfg(not(test))]
    {
        let path = checkpoint_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let checkpoint =
            serde_json::from_slice(&data).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(checkpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_round_trips() {
        let checkpoint = TaskCheckpoint {
            workspace: "/tmp/project".into(),
            request: "add a feature".into(),
            plan: Some("inspect, implement, test".into()),
            plan_approved: true,
            observations: vec!["read src/main.rs".into()],
            completed_actions: vec!["read_file".into()],
            status: TaskStatus::Interrupted,
            updated_at: 42,
        };
        let encoded = serde_json::to_string(&checkpoint).unwrap();
        let decoded: TaskCheckpoint = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.request, checkpoint.request);
        assert_eq!(decoded.status, TaskStatus::Interrupted);
        assert_eq!(decoded.observations, checkpoint.observations);
    }
}
