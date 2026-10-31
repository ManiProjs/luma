//! Persistent, user-readable conversation archives.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::session::{MessageView, SessionInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatArchive {
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub info: SessionInfo,
    pub messages: Vec<MessageView>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSummary {
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub workspace: String,
    pub title: String,
    pub messages: usize,
    pub summary: Option<String>,
}

pub fn chats_dir() -> PathBuf {
    crate::config::config_dir().join("chats")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{nanos}-{}", std::process::id())
}

fn path_for(id: &str) -> Result<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("invalid chat id: {id}");
    }
    Ok(chats_dir().join(format!("{id}.json")))
}

pub fn save(chat: &ChatArchive) -> Result<()> {
    let path = path_for(&chat.id)?;
    fs::create_dir_all(path.parent().expect("chat path has parent"))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(chat)?)
        .with_context(|| format!("writing chat archive {}", temp.display()))?;
    fs::rename(&temp, &path).with_context(|| format!("saving chat archive {}", path.display()))
}

pub fn load(id: &str) -> Result<ChatArchive> {
    let path = path_for(id)?;
    let data = fs::read(&path).with_context(|| format!("reading chat {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("parsing chat {}", path.display()))
}

pub fn list() -> Result<Vec<ChatSummary>> {
    let directory = chats_dir();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut chats = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(data) = fs::read(&path) else { continue };
        let Ok(chat) = serde_json::from_slice::<ChatArchive>(&data) else {
            continue;
        };
        let title = chat
            .messages
            .iter()
            .find(|m| m.role == crate::session::MessageRole::User)
            .map(|m| {
                m.content
                    .lines()
                    .next()
                    .unwrap_or("Untitled chat")
                    .chars()
                    .take(72)
                    .collect()
            })
            .unwrap_or_else(|| "Untitled chat".into());
        chats.push(ChatSummary {
            id: chat.id,
            created_at: chat.created_at,
            updated_at: chat.updated_at,
            workspace: chat.info.workspace,
            title,
            messages: chat.messages.len(),
            summary: chat.summary,
        });
    }
    chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(chats)
}

pub fn add_summary(id: &str, summary: String) -> Result<()> {
    if summary.trim().is_empty() {
        bail!("summary cannot be empty");
    }
    let mut chat = load(id)?;
    chat.summary = Some(summary.trim().to_owned());
    chat.updated_at = now();
    save(&chat)
}
