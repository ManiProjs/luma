//! Persistent, user-readable conversation archives.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
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
    pub title: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSearchResult {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub model: String,
    pub updated_at: u64,
    pub role: String,
    pub tool: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SearchFilters {
    workspace: Option<String>,
    model: Option<String>,
    tool: Option<String>,
    text: String,
}

fn parse_search_filters(query: &str) -> Result<SearchFilters> {
    let mut filters = SearchFilters::default();
    let mut terms = Vec::new();
    for token in query.split_whitespace() {
        let (key, value) = token.split_once(':').unwrap_or(("", ""));
        let target = match key.to_ascii_lowercase().as_str() {
            "workspace" => Some(&mut filters.workspace),
            "model" => Some(&mut filters.model),
            "tool" => Some(&mut filters.tool),
            _ => None,
        };
        if let Some(target) = target {
            if value.trim().is_empty() {
                bail!("{key}: filter requires a value");
            }
            *target = Some(value.to_lowercase());
        } else {
            terms.push(token);
        }
    }
    filters.text = terms.join(" ").to_lowercase();
    if filters.text.is_empty() {
        bail!("conversation search requires text after any workspace:, model:, or tool: filters");
    }
    Ok(filters)
}

fn matches_filter(value: &str, filter: &Option<String>) -> bool {
    filter
        .as_deref()
        .is_none_or(|needle| value.to_lowercase().contains(needle))
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

pub fn delete(id: &str) -> Result<bool> {
    let path = path_for(id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("deleting chat archive {}", path.display()))
        }
    }
}

/// Permanently remove every saved chat archive. Returns the number of files removed.
pub fn prune() -> Result<usize> {
    let directory = chats_dir();
    prune_directory(&directory)
}

fn prune_directory(directory: &Path) -> Result<usize> {
    if !directory.exists() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            continue;
        }
        fs::remove_file(&path)
            .with_context(|| format!("removing saved chat {}", path.display()))?;
        removed += 1;
    }
    Ok(removed)
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
            .title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| {
                chat.messages
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
                    .unwrap_or_else(|| "Untitled chat".into())
            });
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
    chats.sort_by_key(|chat| std::cmp::Reverse(chat.updated_at));
    Ok(chats)
}

pub fn search(query: &str) -> Result<Vec<ChatSearchResult>> {
    let filters = parse_search_filters(query)?;
    let needle = &filters.text;
    let mut results = Vec::new();
    for summary in list()? {
        let chat = load(&summary.id)?;
        if !matches_filter(&summary.workspace, &filters.workspace)
            || !matches_filter(&chat.info.model, &filters.model)
            || filters.tool.as_ref().is_some_and(|tool| {
                !chat.messages.iter().any(|message| {
                    message
                        .tool
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase().contains(tool))
                })
            })
        {
            continue;
        }
        let mut chat_matches = 0;
        if filters.tool.is_none() && summary.title.to_lowercase().contains(needle) {
            results.push(ChatSearchResult {
                id: summary.id.clone(),
                title: summary.title.clone(),
                workspace: summary.workspace.clone(),
                model: chat.info.model.clone(),
                updated_at: summary.updated_at,
                role: "title".into(),
                tool: None,
                snippet: summary.title.clone(),
            });
            chat_matches += 1;
        }
        if filters.tool.is_none() && summary.workspace.to_lowercase().contains(needle) {
            results.push(ChatSearchResult {
                id: summary.id.clone(),
                title: summary.title.clone(),
                workspace: summary.workspace.clone(),
                model: chat.info.model.clone(),
                updated_at: summary.updated_at,
                role: "workspace".into(),
                tool: None,
                snippet: summary.workspace.clone(),
            });
            chat_matches += 1;
        }
        if filters.tool.is_none()
            && summary
                .summary
                .as_deref()
                .is_some_and(|value| value.to_lowercase().contains(needle))
        {
            results.push(ChatSearchResult {
                id: summary.id.clone(),
                title: summary.title.clone(),
                workspace: summary.workspace.clone(),
                model: chat.info.model.clone(),
                updated_at: summary.updated_at,
                role: "summary".into(),
                tool: None,
                snippet: summary
                    .summary
                    .clone()
                    .unwrap_or_default()
                    .chars()
                    .take(320)
                    .collect(),
            });
            chat_matches += 1;
        }
        for message in chat
            .messages
            .iter()
            .filter(|message| message.content.to_lowercase().contains(needle))
            .filter(|message| {
                filters.tool.as_ref().is_none_or(|tool| {
                    message
                        .tool
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase().contains(tool))
                })
            })
        {
            results.push(ChatSearchResult {
                id: summary.id.clone(),
                title: summary.title.clone(),
                workspace: summary.workspace.clone(),
                model: chat.info.model.clone(),
                updated_at: summary.updated_at,
                role: format!("{:?}", message.role).to_lowercase(),
                tool: message.tool.clone(),
                snippet: message.content.chars().take(320).collect(),
            });
            chat_matches += 1;
            if chat_matches >= 4 {
                break;
            }
        }
        if results.len() >= 100 {
            break;
        }
    }
    results.sort_by_key(|result| std::cmp::Reverse(result.updated_at));
    results.truncate(100);
    Ok(results)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_search_rejects_an_empty_query() {
        assert!(search("  ").is_err());
    }

    #[test]
    fn conversation_search_parses_workspace_model_and_tool_filters() {
        assert_eq!(
            parse_search_filters("workspace:luma model:claude tool:read_file context reload")
                .unwrap(),
            SearchFilters {
                workspace: Some("luma".into()),
                model: Some("claude".into()),
                tool: Some("read_file".into()),
                text: "context reload".into(),
            }
        );
        assert!(parse_search_filters("tool: context").is_err());
        assert!(parse_search_filters("workspace:luma").is_err());
    }

    #[test]
    fn conversation_search_filter_matching_is_case_insensitive() {
        assert!(matches_filter(
            "/Users/mani/Projects/Luma",
            &Some("projects/luma".into())
        ));
        assert!(!matches_filter(
            "/Users/mani/Projects/Other",
            &Some("projects/luma".into())
        ));
        assert!(matches_filter("Claude Sonnet 4.5", &Some("sonnet".into())));
        assert!(matches_filter("anything", &None));
    }

    #[test]
    fn delete_rejects_invalid_ids_and_reports_missing_archives() {
        assert!(delete("../config").is_err());
        assert!(!delete(&new_id()).unwrap());
    }

    #[test]
    fn prune_removes_every_chat_archive_and_keeps_other_files() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("chat-one.json"), "{}").unwrap();
        fs::write(directory.path().join("chat-two.json"), "not valid JSON").unwrap();
        fs::write(directory.path().join("notes.txt"), "keep me").unwrap();
        fs::create_dir(directory.path().join("nested.json")).unwrap();

        assert_eq!(prune_directory(directory.path()).unwrap(), 2);
        assert!(!directory.path().join("chat-one.json").exists());
        assert!(!directory.path().join("chat-two.json").exists());
        assert!(directory.path().join("notes.txt").exists());
        assert!(directory.path().join("nested.json").is_dir());
        assert_eq!(prune_directory(directory.path()).unwrap(), 0);
    }
}
