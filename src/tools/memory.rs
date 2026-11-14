//! Durable memory tools exposed to Teammates.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::runtime::{GLOBAL_AGENT_ID, MemoryScope, RuntimeStore};

use super::Tool;

pub struct SaveMemory {
    store: RuntimeStore,
    agent_id: String,
}

pub struct RecallMemory {
    store: RuntimeStore,
    agent_id: String,
    workspace_id: String,
    inherit_global: bool,
}

impl SaveMemory {
    pub fn new(store: RuntimeStore, agent_id: impl Into<String>) -> Self {
        Self {
            store,
            agent_id: agent_id.into(),
        }
    }
}

impl RecallMemory {
    pub fn new(
        store: RuntimeStore,
        agent_id: impl Into<String>,
        workspace_id: impl Into<String>,
        inherit_global: bool,
    ) -> Self {
        Self {
            store,
            agent_id: agent_id.into(),
            workspace_id: workspace_id.into(),
            inherit_global,
        }
    }
}

#[derive(Deserialize)]
struct SaveInput {
    content: String,
}

#[derive(Deserialize)]
struct RecallInput {
    query: String,
}

impl Tool for SaveMemory {
    fn name(&self) -> &str {
        "save_memory"
    }

    fn description(&self) -> &str {
        "Save a durable fact, user preference, project decision, or useful lesson for this teammate to remember across future tasks. Do not save temporary task state, secrets, credentials, or sensitive personal information. Input JSON: {\"content\":\"...\"}. Saves to this teammate's private memory."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: SaveInput =
            serde_json::from_str(input).context("save_memory: invalid JSON input")?;
        let content = input.content.trim();
        if content.is_empty() || content.len() > 8_000 {
            bail!("save_memory: content must contain 1–8000 characters");
        }
        let existing =
            self.store
                .list_memory(MemoryScope::Agent, &self.agent_id, Some(content), 5)?;
        if let Some(entry) = existing.into_iter().find(|entry| entry.content == content) {
            return Ok(format!("Already remembered ({}).", entry.id));
        }
        let entry = self.store.add_memory(
            MemoryScope::Agent,
            &self.agent_id,
            content,
            Some("teammate".into()),
        )?;
        Ok(format!(
            "Saved to your long-term memory ({}). You can recall it in future tasks.",
            entry.id
        ))
    }
}

impl Tool for RecallMemory {
    fn name(&self) -> &str {
        "recall_memory"
    }

    fn description(&self) -> &str {
        "Search durable memories before making assumptions about previously discussed preferences, decisions, project context, or lessons. Input JSON: {\"query\":\"keywords or topic\"}. Searches this teammate's memory and any shared context it is allowed to use."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: RecallInput =
            serde_json::from_str(input).context("recall_memory: invalid JSON input")?;
        let query = input.query.trim();
        if query.is_empty() || query.len() > 1_000 {
            bail!("recall_memory: query must contain 1–1000 characters");
        }
        let mut scopes = vec![
            (MemoryScope::Agent, self.agent_id.as_str(), "this teammate"),
            (
                MemoryScope::Project,
                self.workspace_id.as_str(),
                "this workspace",
            ),
        ];
        if self.inherit_global {
            scopes.push((MemoryScope::Agent, GLOBAL_AGENT_ID, "Global teammate"));
            scopes.push((MemoryScope::User, "default", "shared user"));
        }
        let mut matches = Vec::new();
        for (scope, scope_id, label) in scopes {
            for entry in self.store.list_memory(scope, scope_id, Some(query), 20)? {
                matches.push(format!("- [{label}] {}", entry.content));
            }
        }
        if matches.is_empty() {
            Ok("No matching saved memories. You can still answer using the current conversation and available project context.".into())
        } else {
            Ok(format!("Relevant saved memories:\n{}", matches.join("\n")))
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{AgentConfiguration, AgentKind};

    #[test]
    fn teammate_can_save_and_recall_its_own_memories() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agent = store
            .create_agent(
                "Researcher",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let save = SaveMemory::new(store.clone(), agent.id.clone());
        let recall = RecallMemory::new(store.clone(), agent.id.clone(), "/workspace", false);
        assert!(
            save.execute(r#"{"content":"The project uses SQLite for durable runtime state."}"#)
                .unwrap()
                .contains("Saved")
        );
        let result = recall.execute(r#"{"query":"SQLite"}"#).unwrap();
        assert!(result.contains("SQLite for durable runtime state"));
        assert!(!result.contains("shared user"));
        assert!(
            save.execute(r#"{"content":"The project uses SQLite for durable runtime state."}"#)
                .unwrap()
                .contains("Already remembered")
        );
        assert_eq!(
            store
                .list_memory(MemoryScope::Agent, &agent.id, None, 20)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn recall_only_includes_shared_memories_when_global_context_is_enabled() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agent = store
            .create_agent(
                "Private",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        store
            .add_memory(
                MemoryScope::Agent,
                GLOBAL_AGENT_ID,
                "Shared project convention: use conventional commits.",
                Some("test".into()),
            )
            .unwrap();
        let private_recall =
            RecallMemory::new(store.clone(), agent.id.clone(), "/workspace", false);
        assert!(
            private_recall
                .execute(r#"{"query":"conventional commits"}"#)
                .unwrap()
                .contains("No matching saved memories")
        );
        let shared_recall = RecallMemory::new(store, agent.id, "/workspace", true);
        assert!(
            shared_recall
                .execute(r#"{"query":"conventional commits"}"#)
                .unwrap()
                .contains("Shared project convention")
        );
    }
}
