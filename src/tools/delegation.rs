//! Bounded delegation between persistent Teammates.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::runtime::{AgentKind, PermissionProfile, RuntimeStore, TaskStatus};

use super::Tool;

pub struct ListTeammates {
    store: RuntimeStore,
}

pub struct DelegateTask {
    store: RuntimeStore,
    parent_task_id: String,
    parent_agent_id: String,
    parent_permissions: PermissionProfile,
}

pub struct GetDelegatedTask {
    store: RuntimeStore,
    parent_agent_id: String,
    conversation_id: String,
}

pub struct ListDelegatedTasks {
    store: RuntimeStore,
    parent_agent_id: String,
    conversation_id: String,
}

#[derive(Deserialize)]
struct DelegateInput {
    agent_id: String,
    task: String,
}

#[derive(Deserialize)]
struct ChildInput {
    task_id: String,
}

impl ListTeammates {
    pub fn new(store: RuntimeStore) -> Self {
        Self { store }
    }
}

impl DelegateTask {
    pub fn new(
        store: RuntimeStore,
        parent_task_id: impl Into<String>,
        parent_agent_id: impl Into<String>,
        parent_permissions: PermissionProfile,
    ) -> Self {
        Self {
            store,
            parent_task_id: parent_task_id.into(),
            parent_agent_id: parent_agent_id.into(),
            parent_permissions,
        }
    }
}

impl GetDelegatedTask {
    pub fn new(
        store: RuntimeStore,
        parent_agent_id: impl Into<String>,
        conversation_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            parent_agent_id: parent_agent_id.into(),
            conversation_id: conversation_id.into(),
        }
    }
}

impl ListDelegatedTasks {
    pub fn new(
        store: RuntimeStore,
        parent_agent_id: impl Into<String>,
        conversation_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            parent_agent_id: parent_agent_id.into(),
            conversation_id: conversation_id.into(),
        }
    }
}

impl Tool for ListTeammates {
    fn name(&self) -> &str {
        "list_teammates"
    }

    fn description(&self) -> &str {
        "List available Teammates and their stable IDs/status. Use this to choose another Teammate for an independent subtask. Input JSON: {}."
    }

    fn execute(&self, _input: &str) -> Result<String> {
        let teammates = self
            .store
            .list_agents()?
            .into_iter()
            .filter(|agent| agent.kind == AgentKind::Teammates)
            .map(|agent| {
                serde_json::json!({
                    "id": agent.id,
                    "name": agent.name,
                    "description": agent.description,
                    "status": agent.status,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::to_string_pretty(&teammates)?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for DelegateTask {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "Delegate an independent subtask to another Teammate. This queues a child task and returns its ID; it does not block the current task. Use list_teammates first, include enough context in the task, and do not delegate secrets or work requiring broader permissions. Input JSON: {\"agent_id\":\"target teammate ID\",\"task\":\"specific independent subtask\"}. Delegation requires the current task to be running and is bounded to depth 3, 3 children per task, and 8 descendants per root task."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: DelegateInput =
            serde_json::from_str(input).context("delegate_task: invalid JSON input")?;
        let child = self.store.create_child_task(
            &self.parent_task_id,
            &self.parent_agent_id,
            input.agent_id.trim(),
            &input.task,
            &self.parent_permissions,
        )?;
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "task_id": child.id,
            "conversation_id": child.conversation_id,
            "agent_id": child.agent_id,
            "title": child.title,
            "status": child.status,
            "parent_task_id": child.parent_task_id,
            "delegation_depth": child.delegation_depth,
            "message": "Child task queued. Continue your own task, then use get_delegated_task to inspect its result.",
        }))?)
    }
}

impl Tool for GetDelegatedTask {
    fn name(&self) -> &str {
        "get_delegated_task"
    }

    fn description(&self) -> &str {
        "Inspect a child task previously delegated anywhere in this conversation. Returns status, error, and any recorded assistant response. Input JSON: {\"task_id\":\"child task ID returned by delegate_task or list_delegated_tasks\"}."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: ChildInput =
            serde_json::from_str(input).context("get_delegated_task: invalid JSON input")?;
        let child = self
            .store
            .conversation_child_task(
                &self.parent_agent_id,
                &self.conversation_id,
                input.task_id.trim(),
            )?
            .ok_or_else(|| anyhow::anyhow!("child task not found for this parent task"))?;
        let response = self.store.task_text_response(&child.id)?;
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "task_id": child.id,
            "agent_id": child.agent_id,
            "title": child.title,
            "status": child.status,
            "error": child.error,
            "response": response,
            "complete": matches!(child.status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Interrupted),
        }))?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for ListDelegatedTasks {
    fn name(&self) -> &str {
        "list_delegated_tasks"
    }

    fn description(&self) -> &str {
        "List child tasks delegated during this conversation, including tasks delegated in earlier turns. Use this in a follow-up turn to collect completed work after the parent task has released the serialized gateway. Input JSON: {}."
    }

    fn execute(&self, _input: &str) -> Result<String> {
        let tasks = self
            .store
            .list_conversation_child_tasks(&self.parent_agent_id, &self.conversation_id)?
            .into_iter()
            .map(|task| {
                serde_json::json!({
                    "task_id": task.id,
                    "agent_id": task.agent_id,
                    "title": task.title,
                    "status": task.status,
                    "error": task.error,
                    "parent_task_id": task.parent_task_id,
                    "delegation_depth": task.delegation_depth,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::to_string_pretty(&tasks)?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{AgentConfiguration, AgentStatus, TaskStatus};

    #[test]
    fn teammate_can_list_peers_and_inspect_only_its_own_child_tasks() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let mut coordinator_configuration = AgentConfiguration::default();
        coordinator_configuration.network_access = Some(false);
        let first = store
            .create_agent(
                "Coordinator",
                None,
                AgentKind::Teammates,
                coordinator_configuration,
            )
            .unwrap();
        let second = store
            .create_agent(
                "Researcher",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let parent = store
            .create_task(&first.id, "Coordinate research", None, None, false, None)
            .unwrap();
        store
            .transition_task(&parent.id, TaskStatus::Running, None, None)
            .unwrap();
        let delegate = DelegateTask::new(
            store.clone(),
            parent.id.clone(),
            first.id.clone(),
            first.configuration.permissions.clone(),
        );
        let output = delegate
            .execute(&format!(
                "{{\"agent_id\":\"{}\",\"task\":\"Research the API design\"}}",
                second.id
            ))
            .unwrap();
        let child_id = serde_json::from_str::<serde_json::Value>(&output).unwrap()["task_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(store.list_child_tasks(&parent.id).unwrap().len(), 1);
        let child = store
            .delegated_task(&parent.id, &child_id)
            .unwrap()
            .unwrap();
        assert_eq!(child.status, TaskStatus::Queued);
        assert!(
            store
                .list_tasks(None, 500)
                .unwrap()
                .iter()
                .any(|task| task.id == child_id && task.status == TaskStatus::Queued)
        );
        assert_eq!(child.parent_task_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.delegation_depth, 1);
        assert_eq!(child.conversation_id, parent.conversation_id);
        assert!(!child.permission_ceiling.as_ref().unwrap().allow_network);
        assert!(
            store
                .delegated_task("another-parent", &child_id)
                .unwrap()
                .is_none()
        );
        let peers = ListTeammates::new(store.clone()).execute("{}").unwrap();
        assert!(peers.contains("Researcher"));
        let child_list = ListDelegatedTasks::new(
            store.clone(),
            first.id.clone(),
            parent.conversation_id.clone(),
        )
        .execute("{}")
        .unwrap();
        assert!(child_list.contains(&child_id));
        let child_result =
            GetDelegatedTask::new(store, first.id.clone(), parent.conversation_id.clone())
                .execute(&format!("{{\"task_id\":\"{child_id}\"}}"))
                .unwrap();
        assert!(child_result.contains("queued"));
    }

    #[test]
    fn delegation_rejects_paused_target_and_caps_permissions() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let parent_agent = store
            .create_agent(
                "Coordinator",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let target_agent = store
            .create_agent(
                "Restricted",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        store
            .set_agent_status(&target_agent.id, AgentStatus::Paused)
            .unwrap();
        let parent = store
            .create_task(&parent_agent.id, "Coordinate", None, None, false, None)
            .unwrap();
        store
            .transition_task(&parent.id, TaskStatus::Running, None, None)
            .unwrap();
        assert!(
            store
                .create_child_task(
                    &parent.id,
                    &parent_agent.id,
                    &target_agent.id,
                    "Research",
                    &parent_agent.configuration.permissions,
                )
                .is_err()
        );

        let mut parent_permissions = parent_agent.configuration.permissions.clone();
        parent_permissions.allow_network = false;
        let mut target_permissions = target_agent.configuration.permissions.clone();
        target_permissions.allow_network = true;
        let intersection = parent_permissions.intersect(&target_permissions);
        assert!(!intersection.allow_network);
        assert!(intersection.require_approval_for_mutations);

        parent_permissions.allowed_tools = vec!["read_file".into(), "git_status".into()];
        parent_permissions.denied_tools = vec!["git_status".into()];
        target_permissions.allowed_tools = vec!["read_file".into(), "write_file".into()];
        target_permissions.denied_tools = vec!["write_file".into()];
        let intersection = parent_permissions.intersect(&target_permissions);
        assert_eq!(intersection.allowed_tools, vec!["read_file"]);
        assert_eq!(intersection.denied_tools, vec!["git_status", "write_file"]);
    }

    #[test]
    fn delegation_enforces_depth_and_direct_child_limits() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agents = (0..5)
            .map(|index| {
                store
                    .create_agent(
                        &format!("Teammate {index}"),
                        None,
                        AgentKind::Teammates,
                        AgentConfiguration::default(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let root = store
            .create_task(&agents[0].id, "Root task", None, None, false, None)
            .unwrap();
        store
            .transition_task(&root.id, TaskStatus::Running, None, None)
            .unwrap();
        for index in 1..=3 {
            store
                .create_child_task(
                    &root.id,
                    &agents[0].id,
                    &agents[index].id,
                    "Independent child",
                    &agents[0].configuration.permissions,
                )
                .unwrap();
        }
        assert!(
            store
                .create_child_task(
                    &root.id,
                    &agents[0].id,
                    &agents[4].id,
                    "Fourth direct child",
                    &agents[0].configuration.permissions,
                )
                .is_err()
        );

        let mut parent_id = root.id.clone();
        for depth in 1..=3 {
            let parent_agent = &agents[depth];
            let child = if depth == 1 {
                store.list_child_tasks(&root.id).unwrap()[0].clone()
            } else {
                store.list_child_tasks(&parent_id).unwrap()[0].clone()
            };
            assert_eq!(child.delegation_depth, depth as u32);
            store
                .transition_task(&child.id, TaskStatus::Running, None, None)
                .unwrap();
            parent_id = child.id.clone();
            if depth < 3 {
                store
                    .create_child_task(
                        &child.id,
                        &child.agent_id,
                        &agents[depth + 1].id,
                        "Next level child",
                        &parent_agent.configuration.permissions,
                    )
                    .unwrap();
            } else {
                assert!(
                    store
                        .create_child_task(
                            &child.id,
                            &child.agent_id,
                            &agents[4].id,
                            "Too deep",
                            &agents[depth].configuration.permissions,
                        )
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn delegation_enforces_eight_descendants_across_the_entire_tree() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agents = (0..10)
            .map(|index| {
                store
                    .create_agent(
                        &format!("Tree teammate {index}"),
                        None,
                        AgentKind::Teammates,
                        AgentConfiguration::default(),
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let root = store
            .create_task(&agents[0].id, "Root", None, None, false, None)
            .unwrap();
        store
            .transition_task(&root.id, TaskStatus::Running, None, None)
            .unwrap();
        let mut children = Vec::new();
        for target in &agents[1..=3] {
            children.push(
                store
                    .create_child_task(
                        &root.id,
                        &agents[0].id,
                        &target.id,
                        "First-level work",
                        &agents[0].configuration.permissions,
                    )
                    .unwrap(),
            );
        }

        let mut descendants = 3;
        for (index, child) in children.iter().enumerate() {
            store
                .transition_task(&child.id, TaskStatus::Running, None, None)
                .unwrap();
            let count = if index < 2 { 2 } else { 1 };
            for offset in 0..count {
                let target = &agents[4 + descendants - 3 + offset];
                store
                    .create_child_task(
                        &child.id,
                        &child.agent_id,
                        &target.id,
                        "Second-level work",
                        &agents[index + 1].configuration.permissions,
                    )
                    .unwrap();
                descendants += 1;
            }
        }
        assert_eq!(descendants, 8);
        assert!(
            store
                .create_child_task(
                    &children[2].id,
                    &children[2].agent_id,
                    &agents[9].id,
                    "Ninth descendant",
                    &agents[3].configuration.permissions,
                )
                .is_err()
        );
    }
}
