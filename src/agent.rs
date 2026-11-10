use std::{
    path::{Component, Path, PathBuf},
    time::Instant,
};

use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;

use crate::{
    context::{Context, Message, MessageRole},
    event::{AgentEvent, AgentStatus},
    history::{History, HistoryMessage},
    model::{CompletionRequest, Model},
    react::{ReActAction, ReActTrait},
    runtime::{PermissionDecision, PermissionProfile},
    session::RequestCancellation,
    tools::ToolRegistry,
};

// ============================================================================
// Limits
// ============================================================================

/// Maximum amount of output stored for an individual tool observation.
///
/// The full tool result is still displayed to the user by the tool event,
/// but the react doesn't need megabytes of compiler output.
const MAX_TOOL_OBSERVATION: usize = 8_000;

/// Maximum number of react iterations for one user request.
const MAX_STEPS: usize = 12;

// ============================================================================
// Pricing
// ============================================================================

/// Cost per 1M tokens in USD.
const INPUT_COST_PER_MILLION: f64 = 3.0;
const OUTPUT_COST_PER_MILLION: f64 = 15.0;

fn calculate_cost(prompt_tokens: u64, completion_tokens: u64) -> f64 {
    (prompt_tokens as f64 / 1_000_000.0) * INPUT_COST_PER_MILLION
        + (completion_tokens as f64 / 1_000_000.0) * OUTPUT_COST_PER_MILLION
}

fn checkpoint_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn preview_file_change(name: &str, input: &str) -> Result<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(input)?;
    let path = value
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| anyhow!("{name}: missing file path"))?;
    let old = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && name == "write_file" => {
            String::new()
        }
        Err(error) => return Err(error.into()),
    };
    let new = match name {
        "write_file" => value
            .get("content")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("write_file: missing content"))?
            .to_owned(),
        "patch_file" => {
            let before = value
                .get("old")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("patch_file: missing old text"))?;
            let after = value
                .get("new")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("patch_file: missing new text"))?;
            if old.matches(before).count() != 1 {
                return Err(anyhow!(
                    "patch_file preview refused: old text must match exactly once"
                ));
            }
            old.replacen(before, after, 1)
        }
        _ => return Err(anyhow!("unsupported preview operation: {name}")),
    };
    if old == new {
        return Err(anyhow!("{name}: proposed file contents are unchanged"));
    }
    Ok((path.to_owned(), unified_preview(path, &old, &new)))
}

fn validate_agent_tool_paths(root: &Path, name: &str, input: &str) -> Result<()> {
    let value = serde_json::from_str::<serde_json::Value>(input).ok();
    let string_field = |field: &str| {
        value
            .as_ref()
            .and_then(|value| value.get(field))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    let mut paths = Vec::<String>::new();
    match name {
        "read_file" => {
            let path = input
                .rsplit_once(':')
                .filter(|(_, range)| range.contains('-'))
                .map(|(path, _)| path)
                .unwrap_or(input)
                .trim();
            if !path.is_empty() {
                paths.push(path.to_owned());
            }
        }
        "write_file" | "patch_file" | "file_metadata" => {
            if let Some(path) = string_field("path") {
                paths.push(path);
            }
        }
        "list_directory" | "git_status" => {
            if let Some(path) = string_field("path") {
                paths.push(path);
            } else if value.is_none() && !input.trim().is_empty() {
                paths.push(input.trim().to_owned());
            }
        }
        "search_files" => {
            if let Some(path) = string_field("path") {
                paths.push(path);
            }
        }
        "git_diff" | "git_show" | "git_blame" => {
            let repo = string_field("path").unwrap_or_else(|| ".".into());
            paths.push(repo.clone());
            if let Some(file) = string_field("file") {
                let file = Path::new(&file);
                if file.is_absolute() {
                    paths.push(file.display().to_string());
                } else {
                    paths.push(Path::new(&repo).join(file).display().to_string());
                }
            }
        }
        "git_log" | "git_branches" | "find_todos" | "count_lines" | "inspect_project"
        | "find_symbols" | "find_references" => {
            if let Some(path) = string_field("path") {
                paths.push(path);
            }
        }
        _ => {}
    }

    for path in paths {
        let path = Path::new(&path);
        if !path.is_absolute() && path.components().any(|part| part == Component::ParentDir) {
            anyhow::bail!(
                "workspace policy denied path traversal in `{}`",
                path.display()
            );
        }
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        let resolved = if std::fs::symlink_metadata(&candidate).is_ok() {
            candidate.canonicalize()?
        } else {
            let parent = candidate
                .parent()
                .ok_or_else(|| anyhow!("path has no parent"))?;
            let name = candidate
                .file_name()
                .ok_or_else(|| anyhow!("path has no file name"))?;
            parent.canonicalize()?.join(name)
        };
        if !resolved.starts_with(root) {
            anyhow::bail!(
                "workspace policy denied access outside `{}`",
                root.display()
            );
        }
    }
    Ok(())
}

fn unified_preview(path: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    if old_lines.len().saturating_mul(new_lines.len()) > 250_000 {
        let mut output = format!("--- a/{path}\n+++ b/{path}\n");
        for line in &old_lines {
            output.push('-');
            output.push_str(line);
            output.push('\n');
            if output.len() > 6_000 {
                break;
            }
        }
        for line in &new_lines {
            output.push('+');
            output.push_str(line);
            output.push('\n');
            if output.len() > 12_000 {
                break;
            }
        }
        output.push_str("… large diff truncated\n");
        return output;
    }
    let width = new_lines.len() + 1;
    let mut lcs = vec![0u32; (old_lines.len() + 1) * width];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            let index = i * width + j;
            lcs[index] = if old_lines[i] == new_lines[j] {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut output = format!(
        "--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
        old_lines.len(),
        new_lines.len()
    );
    let (mut i, mut j) = (0, 0);
    while i < old_lines.len() || j < new_lines.len() {
        if i < old_lines.len() && j < new_lines.len() && old_lines[i] == new_lines[j] {
            output.push(' ');
            output.push_str(old_lines[i]);
            output.push('\n');
            i += 1;
            j += 1;
        } else if i < old_lines.len()
            && (j == new_lines.len() || lcs[(i + 1) * width + j] >= lcs[i * width + j + 1])
        {
            output.push('-');
            output.push_str(old_lines[i]);
            output.push('\n');
            i += 1;
        } else {
            output.push('+');
            output.push_str(new_lines[j]);
            output.push('\n');
            j += 1;
        }
        if output.len() > 12_000 {
            output.push_str("… diff truncated\n");
            break;
        }
    }
    if old.ends_with('\n') != new.ends_with('\n') {
        if !old.ends_with('\n') {
            output.push_str("\\ No newline at end of old file\n");
        }
        if !new.ends_with('\n') {
            output.push_str("\\ No newline at end of new file\n");
        }
    }
    output
}

#[cfg(test)]
mod change_preview_tests {
    use super::*;

    #[test]
    fn preview_shows_added_and_removed_lines() {
        let diff = unified_preview("sample.txt", "before\nkeep\n", "after\nkeep\n");
        assert!(diff.contains("--- a/sample.txt"));
        assert!(diff.contains("-before"));
        assert!(diff.contains("+after"));
        assert!(diff.contains(" keep"));
    }

    #[test]
    fn patch_preview_requires_one_exact_match() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.txt");
        std::fs::write(&path, "same\nsame\n").unwrap();
        let input = serde_json::json!({"path":path,"old":"same","new":"changed"}).to_string();
        assert!(preview_file_change("patch_file", &input).is_err());
    }

    #[test]
    fn teammate_file_tools_cannot_escape_the_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("src")).unwrap();
        std::fs::write(outside.path().join("secret.txt"), "private").unwrap();
        let root = workspace.path().canonicalize().unwrap();

        assert!(validate_agent_tool_paths(&root, "read_file", "../secret.txt").is_err());
        assert!(
            validate_agent_tool_paths(
                &root,
                "read_file",
                outside.path().join("secret.txt").to_str().unwrap(),
            )
            .is_err()
        );
        assert!(
            validate_agent_tool_paths(
                &root,
                "write_file",
                &serde_json::json!({"path":"src/new.rs","content":"fn main() {}"}).to_string(),
            )
            .is_ok()
        );
        assert!(
            validate_agent_tool_paths(
                &root,
                "git_diff",
                &serde_json::json!({"path":".","file":"../../secret.txt"}).to_string(),
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn teammate_file_tools_resolve_symlinks_before_granting_access() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "private").unwrap();
        symlink(outside.path(), workspace.path().join("external")).unwrap();
        let root = workspace.path().canonicalize().unwrap();

        assert!(validate_agent_tool_paths(&root, "read_file", "external/secret.txt").is_err());
        assert!(
            validate_agent_tool_paths(
                &root,
                "write_file",
                &serde_json::json!({"path":"external/new.txt","content":"nope"}).to_string(),
            )
            .is_err()
        );
    }

    #[test]
    fn preview_reports_missing_final_newline() {
        let diff = unified_preview("sample.txt", "same", "same\n");
        assert!(diff.contains("No newline at end of old file"));
    }
}

// ============================================================================
// Confirmation
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    Allow,
    Deny,
}

// ============================================================================
// Planning state
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningState {
    Exploring,
    Implementing,
}

// ============================================================================
// Agent
// ============================================================================

pub struct Agent<M, P> {
    model: M,
    react: P,
    context: Context,
    tools: ToolRegistry,
    history: History,

    planning_state: PlanningState,

    checkpoint: Option<crate::tasks::TaskCheckpoint>,
    plan_only: bool,
    permission_profile: PermissionProfile,
    workspace_root: Option<PathBuf>,
}

impl<M, P> Agent<M, P>
where
    M: Model,
    P: ReActTrait,
{
    pub fn new(model: M, react: P, tools: ToolRegistry, history: History, galaxy: String) -> Self {
        let mut context = Context::new();

        if !galaxy.trim().is_empty() {
            context.add(
                MessageRole::System,
                format!(
                    "# Luma Workspace Memory\n\n\
                     The following information comes from GALAXY.md.\n\
                     Treat it as project memory.\n\n---\n\n{}\n\n---",
                    galaxy
                ),
            );
        }

        Self {
            model,
            react,
            context,
            tools,
            history,

            planning_state: PlanningState::Exploring,

            checkpoint: None,
            plan_only: false,
            permission_profile: PermissionProfile::default(),
            workspace_root: None,
        }
    }

    pub fn set_permission_profile(&mut self, profile: PermissionProfile) {
        self.permission_profile = profile;
    }

    pub fn set_workspace_root(&mut self, path: &Path) -> Result<()> {
        self.workspace_root = Some(path.canonicalize()?);
        Ok(())
    }

    pub fn add_system_context(&mut self, content: String) {
        if !content.trim().is_empty() {
            self.context.add(MessageRole::System, content);
        }
    }

    // ========================================================================
    // Main loop
    // ========================================================================

    pub async fn run(
        &mut self,
        mut input_rx: Receiver<String>,
        event_tx: Sender<AgentEvent>,
        shutdown: CancellationToken,
        request_cancel: RequestCancellation,
        confirmation_rx: Receiver<Confirmation>,
    ) -> Result<()> {
        let mut confirmation_rx = confirmation_rx;

        loop {
            let input = tokio::select! {
                _ = shutdown.cancelled() => break,
                input = input_rx.recv() => match input {
                    Some(input) => input,
                    None => break,
                },
            };
            let cancel = request_cancel.current();
            if input.trim().is_empty() {
                continue;
            }
            if cancel.is_cancelled() {
                event_tx.send(AgentEvent::Cancelled).await?;
                event_tx.send(AgentEvent::Finished).await?;
                continue;
            }
            event_tx
                .send(AgentEvent::Status {
                    state: AgentStatus::Thinking,
                })
                .await?;
            if let Err(error) = self
                .handle_input(input, &event_tx, &cancel, &mut confirmation_rx)
                .await
            {
                event_tx.send(AgentEvent::Error(error.to_string())).await?;
                self.finish(&event_tx, &cancel).await?;
            }
        }

        Ok(())
    }

    async fn handle_input(
        &mut self,
        input: String,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        let input = input.trim();

        if input.is_empty() {
            return Ok(());
        }

        if let Some(snapshot) = input.strip_prefix("\0LUMA_RESTORE:") {
            let messages: Vec<crate::session::MessageView> = serde_json::from_str(snapshot)?;
            let restored = messages
                .into_iter()
                .map(|message| {
                    let (role, content) = match message.role {
                        crate::session::MessageRole::System => {
                            (MessageRole::System, message.content)
                        }
                        crate::session::MessageRole::User => (MessageRole::User, message.content),
                        crate::session::MessageRole::Assistant => {
                            (MessageRole::Assistant, message.content)
                        }
                        crate::session::MessageRole::Tool => (
                            MessageRole::Observation,
                            format!(
                                "Tool {}: {}",
                                message.tool.unwrap_or_else(|| "output".into()),
                                message.content
                            ),
                        ),
                        crate::session::MessageRole::Plan => {
                            (MessageRole::System, format!("Plan: {}", message.content))
                        }
                        crate::session::MessageRole::Preview => (
                            MessageRole::Observation,
                            format!("Previously proposed change:\n{}", message.content),
                        ),
                        crate::session::MessageRole::Error => (
                            MessageRole::System,
                            format!("Previous error: {}", message.content),
                        ),
                    };
                    Message { role, content }
                })
                .collect::<Vec<_>>();
            self.context.restore(restored);
            self.finish(tx, cancel).await?;
            return Ok(());
        }

        if let Some(snapshot) = input.strip_prefix("\0LUMA_RESUME:") {
            let checkpoint: crate::tasks::TaskCheckpoint = serde_json::from_str(snapshot)?;
            self.context.add(
                MessageRole::System,
                format!(
                    "Resuming an interrupted task. Previously approved plan:\n{}\n\nCompleted actions: {}. Do not repeat completed work; inspect the current workspace state before continuing.",
                    checkpoint.plan.as_deref().unwrap_or("No plan was saved."),
                    checkpoint.completed_actions.join(", ")
                ),
            );
            self.context
                .add(MessageRole::User, checkpoint.request.clone());
            self.planning_state = if checkpoint.plan_approved {
                PlanningState::Implementing
            } else {
                PlanningState::Exploring
            };
            self.checkpoint = Some(checkpoint);
            if let Some(checkpoint) = self.checkpoint.as_mut() {
                checkpoint.status = crate::tasks::TaskStatus::InProgress;
            }
            self.save_checkpoint();
            self.run_agent_loop(tx, cancel, confirmation_rx).await?;
            self.finish_checkpoint(cancel);
            self.finish(tx, cancel).await?;
            return Ok(());
        }

        if input == "/init" {
            self.initialize_workspace(tx, cancel, confirmation_rx)
                .await?;

            self.finish(tx, cancel).await?;
            return Ok(());
        }

        self.plan_only = input.starts_with("/plan ");
        let input = if let Some(task) = input.strip_prefix("/plan ") {
            format!(
                "Create an explicit implementation plan for this task, but do not implement it yet: {task}"
            )
        } else {
            input.to_owned()
        };

        self.begin_request(&input)?;

        if self.plan_only || self.needs_workspace(&input) {
            self.run_agent_loop(tx, cancel, confirmation_rx).await?;
        } else {
            self.run_conversation(tx, cancel).await?;
        }

        self.finish_checkpoint(cancel);
        self.finish(tx, cancel).await
    }

    async fn finish(&self, tx: &Sender<AgentEvent>, cancel: &CancellationToken) -> Result<()> {
        if cancel.is_cancelled() {
            tx.send(AgentEvent::Cancelled).await?;
        }
        tx.send(AgentEvent::Finished).await?;
        Ok(())
    }

    // ========================================================================
    // Request / history
    // ========================================================================

    fn begin_request(&mut self, input: &str) -> Result<()> {
        self.planning_state = PlanningState::Exploring;

        self.context.add(MessageRole::User, input.to_owned());

        let workspace = std::env::current_dir()?.display().to_string();
        let is_task = self.needs_workspace(input) || self.plan_only;
        self.checkpoint = is_task.then(|| crate::tasks::TaskCheckpoint {
            workspace: workspace.clone(),
            request: input.to_owned(),
            plan: None,
            plan_approved: false,
            observations: Vec::new(),
            completed_actions: Vec::new(),
            status: crate::tasks::TaskStatus::InProgress,
            updated_at: checkpoint_now(),
        });
        self.save_checkpoint();

        self.history.messages.push(HistoryMessage {
            role: "user".into(),
            content: input.to_owned(),
        });

        self.history.save()
    }

    fn store_assistant_response(&mut self, response: &str) -> Result<()> {
        if response.trim().is_empty() {
            return Ok(());
        }

        self.context
            .add(MessageRole::Assistant, response.to_owned());

        self.history.messages.push(HistoryMessage {
            role: "assistant".into(),
            content: response.to_owned(),
        });

        self.history.save()
    }

    // ========================================================================
    // Conversation
    // ========================================================================

    async fn run_conversation(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        match self.generate_answer(tx, cancel).await {
            Ok(response) if !cancel.is_cancelled() => self.store_assistant_response(&response)?,
            Ok(_) => {}

            Err(error) => {
                tx.send(AgentEvent::Error(error.to_string())).await?;
            }
        }

        Ok(())
    }

    async fn generate_answer(
        &self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        tx.send(AgentEvent::Status {
            state: AgentStatus::Talking,
        })
        .await?;
        let mut messages = Vec::with_capacity(self.context.messages().len() + 1);

        messages.push(Message {
            role: MessageRole::System,
            content: self.answer_system_prompt(),
        });

        messages.extend(self.context.messages().iter().cloned());

        let started = Instant::now();

        let mut stream = tokio::select! {
            _ = cancel.cancelled() => return Ok(String::new()),
            result = self.model.stream(CompletionRequest { messages }) => result?,
        };

        let mut response = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    break;
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    let chunk = chunk?;

                    response.push_str(&chunk);

                    tx.send(AgentEvent::TextDelta(chunk))
                        .await?;
                }
            }
        }

        if let Some(usage) = self.model.usage() {
            let cost = calculate_cost(usage.prompt_tokens, usage.completion_tokens);

            tx.send(AgentEvent::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                cost_usd: cost,
            })
            .await?;
        }

        let _elapsed = started.elapsed();

        Ok(response)
    }

    fn answer_system_prompt(&self) -> String {
        "\
You are Luma, a local-first coding agent.

Be concise, technical, and practical.

Use workspace observations when answering workspace questions.
Never invent files, commands, project structure, dependencies,
or test results.
"
        .into()
    }

    // ========================================================================
    // Agent loop
    // ========================================================================

    async fn run_agent_loop(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        tx.send(AgentEvent::Thinking).await?;

        for step in 0..MAX_STEPS {
            tx.send(AgentEvent::Status {
                state: AgentStatus::Thinking,
            })
            .await?;
            if cancel.is_cancelled() {
                self.interrupt_checkpoint();
                return Ok(());
            }

            // ------------------------------------------------------------
            // ReAct
            // ------------------------------------------------------------

            let action_started = Instant::now();

            let action = match self.next_react_action(cancel).await {
                Ok(action) => action,

                Err(error) => {
                    tx.send(AgentEvent::Error(format!("ReAct failed: {}", error)))
                        .await?;

                    self.run_conversation(tx, cancel).await?;
                    return Ok(());
                }
            };

            let _react_elapsed = action_started.elapsed();

            if let Some(usage) = self.model.usage() {
                tx.send(AgentEvent::Usage {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                    cost_usd: calculate_cost(usage.prompt_tokens, usage.completion_tokens),
                })
                .await?;
            }

            match action {
                // --------------------------------------------------------
                // Single tool
                // --------------------------------------------------------
                ReActAction::Tool { name, input } => {
                    if self.planning_state == PlanningState::Exploring
                        && matches!(name.as_str(), "write_file" | "patch_file")
                    {
                        let message = format!(
                            "Cannot execute '{name}' before an implementation plan is approved. Ask Luma to propose a plan first."
                        );
                        tx.send(AgentEvent::Error(message.clone())).await?;
                        self.record_observation("planning", &message);
                        continue;
                    }
                    self.execute_tool(&name, &input, tx, cancel, confirmation_rx)
                        .await?;
                }

                // --------------------------------------------------------
                // Answer
                // --------------------------------------------------------
                ReActAction::Answer { content } => {
                    if !cancel.is_cancelled() {
                        tx.send(AgentEvent::Status {
                            state: AgentStatus::Talking,
                        })
                        .await?;
                        tx.send(AgentEvent::TextDelta(content.clone())).await?;
                        self.store_assistant_response(&content)?;
                    }
                    return Ok(());
                }

                // --------------------------------------------------------
                // Explicit plan
                // --------------------------------------------------------
                ReActAction::Plan { content } => {
                    tx.send(AgentEvent::PlanGenerated(content.clone())).await?;

                    if let Some(checkpoint) = self.checkpoint.as_mut() {
                        checkpoint.plan = Some(content.clone());
                    }
                    self.save_checkpoint();

                    let display = format!("Plan:\n{}", content);

                    let allowed = self
                        .request_confirmation("approve_plan", &display, tx, cancel, confirmation_rx)
                        .await?;

                    if allowed {
                        if self.plan_only {
                            self.context.add(
                                MessageRole::System,
                                "Plan approved and saved. Implementation was not started because this was a plan-only request.",
                            );
                            return Ok(());
                        }
                        self.planning_state = PlanningState::Implementing;

                        if let Some(checkpoint) = self.checkpoint.as_mut() {
                            checkpoint.plan = Some(content.clone());
                            checkpoint.plan_approved = true;
                        }
                        self.save_checkpoint();

                        self.context.add(
                            MessageRole::System,
                            format!(
                                "Plan approved. Proceeding with implementation:\n{}",
                                content
                            ),
                        );
                    } else {
                        self.planning_state = PlanningState::Exploring;

                        self.context.add(
                            MessageRole::System,
                            "Plan rejected. Rethink the approach and either \
                             explore more or propose a revised plan."
                                .to_string(),
                        );
                    }
                }
            }

            // Don't waste another react request after the final allowed
            // iteration.
            if step + 1 == MAX_STEPS {
                self.run_conversation(tx, cancel).await?;
                return Ok(());
            }
        }

        Ok(())
    }

    fn next_react_action<'a>(
        &'a self,
        cancel: &'a CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ReActAction>> + Send + 'a>> {
        Box::pin(async move {
            self.react
                .next_action(self.context.messages().to_vec(), cancel.clone())
                .await
        })
    }

    async fn execute_tool(
        &mut self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        if cancel.is_cancelled() {
            return Ok(());
        }

        if let Some(root) = &self.workspace_root {
            if let Err(error) = validate_agent_tool_paths(root, name, input) {
                let message = format!("Workspace policy denied tool `{name}`: {error}");
                tx.send(AgentEvent::Error(message.clone())).await?;
                self.record_observation(name, &message);
                return Ok(());
            }
        }

        tx.send(AgentEvent::Status {
            state: AgentStatus::Doing,
        })
        .await?;
        let display = Self::format_tool_display(name, input);

        let mut tool_diff = None;
        if matches!(name, "write_file" | "patch_file") {
            let (path, diff) = match preview_file_change(name, input) {
                Ok(preview) => preview,
                Err(error) => {
                    let message = format!("{name} failed: {error}");
                    tx.send(AgentEvent::Error(message.clone())).await?;
                    self.record_observation(name, &message);
                    return Ok(());
                }
            };
            if name == "patch_file" {
                tool_diff = Some(diff.clone());
            }
            tx.send(AgentEvent::ChangePreview { path, diff }).await?;
            let allowed = match self.permission_profile.decide(name, true) {
                PermissionDecision::Allow => true,
                PermissionDecision::Deny => false,
                PermissionDecision::RequireApproval => {
                    self.request_confirmation(name, input, tx, cancel, confirmation_rx)
                        .await?
                }
            };
            if !allowed {
                return Ok(());
            }
        } else {
            let mutating = name == "run_command";
            match self.permission_profile.decide(name, mutating) {
                PermissionDecision::Allow => {}
                PermissionDecision::Deny => {
                    let message = format!("Permission policy denied tool `{name}`.");
                    tx.send(AgentEvent::Error(message.clone())).await?;
                    self.record_observation(name, &message);
                    return Ok(());
                }
                PermissionDecision::RequireApproval => {
                    if !self
                        .request_confirmation(name, &display, tx, cancel, confirmation_rx)
                        .await?
                    {
                        return Ok(());
                    }
                }
            }
        }

        tx.send(AgentEvent::ToolStarted {
            name: name.to_owned(),
            input: display.clone(),
        })
        .await?;
        if cancel.is_cancelled() {
            return Ok(());
        }

        let started = Instant::now();
        let execution = if name == "run_command" {
            let command = input.trim().to_owned();
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(120),
                    crate::tools::shell::run_process(command),
                ) => match result {
                    Ok(output) => Ok(output),
                    Err(_) => Ok("ERROR: command timed out after 120s and was terminated.".to_owned()),
                }
            }
        } else {
            self.tools.execute(name, input.trim())
        };

        let (result, failed) = match execution {
            Ok(result) => (result, false),
            Err(error) => {
                let message = format!("{} failed: {}", name, error);
                tx.send(AgentEvent::ToolFinished {
                    name: name.to_owned(),
                    duration_ms: started.elapsed().as_millis(),
                    failed: true,
                    diff: None,
                })
                .await?;
                tx.send(AgentEvent::Error(message.clone())).await?;
                self.record_observation(name, &message);
                return Ok(());
            }
        };

        if cancel.is_cancelled() {
            return Ok(());
        }
        tx.send(AgentEvent::ToolFinished {
            name: name.to_owned(),
            duration_ms: started.elapsed().as_millis(),
            failed,
            diff: tool_diff,
        })
        .await?;
        self.record_observation(name, &result);
        if let Some(checkpoint) = self.checkpoint.as_mut() {
            checkpoint
                .completed_actions
                .push(format!("{name}: {display}"));
        }
        self.save_checkpoint();
        Ok(())
    }

    fn record_observation(&mut self, tool: &str, result: &str) {
        let observation = format!(
            "Observation from `{}`:\n{}",
            tool,
            compact_tool_output(result)
        );
        self.context
            .add(MessageRole::Observation, observation.clone());
        if let Some(checkpoint) = self.checkpoint.as_mut() {
            checkpoint.observations.push(observation);
        }
        self.save_checkpoint();
    }

    fn save_checkpoint(&self) {
        if let Some(checkpoint) = &self.checkpoint {
            let mut checkpoint = checkpoint.clone();
            checkpoint.updated_at = checkpoint_now();
            if let Err(error) = crate::tasks::save(&checkpoint) {
                tracing::warn!(%error, "could not save task checkpoint");
            }
        }
    }

    fn interrupt_checkpoint(&mut self) {
        if let Some(checkpoint) = self.checkpoint.as_mut() {
            checkpoint.status = crate::tasks::TaskStatus::Interrupted;
        }
        self.save_checkpoint();
    }

    fn complete_checkpoint(&mut self) {
        if let Some(checkpoint) = self.checkpoint.as_mut() {
            if checkpoint.status == crate::tasks::TaskStatus::InProgress {
                checkpoint.status = crate::tasks::TaskStatus::Completed;
            }
        }
        self.save_checkpoint();
    }

    fn finish_checkpoint(&mut self, cancel: &CancellationToken) {
        if cancel.is_cancelled() {
            self.interrupt_checkpoint();
        } else {
            self.complete_checkpoint();
        }
    }

    async fn request_confirmation(
        &self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<bool> {
        tx.send(AgentEvent::ConfirmationRequired {
            name: name.to_owned(),
            input: input.to_owned(),
        })
        .await?;

        let confirmation = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(false);
            }

            result = confirmation_rx.recv() => {
                result.ok_or_else(|| {
                    anyhow!("Confirmation channel closed")
                })?
            }
        };

        match confirmation {
            Confirmation::Allow => Ok(true),

            Confirmation::Deny => {
                tx.send(AgentEvent::SystemMessage(format!("Skipped {}.", name)))
                    .await?;

                Ok(false)
            }
        }
    }

    fn format_tool_display(name: &str, input: &str) -> String {
        match name {
            "read_file" => {
                format!("read_file → {}", input)
            }

            "write_file" | "patch_file" => match serde_json::from_str::<serde_json::Value>(input) {
                Ok(json) => {
                    let path = json
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("?");

                    format!("{} → {}", name, path)
                }

                Err(_) => {
                    format!("{} → invalid JSON", name)
                }
            },

            "run_command" => {
                format!("run_command → {}", input)
            }

            _ => {
                format!("{} {}", name, input)
            }
        }
    }

    // ========================================================================
    // Workspace detection
    // ========================================================================

    fn needs_workspace(&self, input: &str) -> bool {
        let input = input.trim().to_lowercase();

        if input.is_empty() {
            return false;
        }

        if Self::contains_path_reference(&input) {
            return true;
        }

        const FILE_ACTIONS: &[&str] = &[
            "read", "open", "edit", "modify", "change", "update", "rewrite", "refactor", "rename",
            "move", "delete", "remove", "create", "write", "patch", "fix", "replace", "add",
        ];

        const FILE_TARGETS: &[&str] = &[
            "file",
            "files",
            "code",
            "source",
            "source code",
            "implementation",
            "function",
            "method",
            "class",
            "module",
            "struct",
            "enum",
            "trait",
            "variable",
            "config",
            "configuration",
            "script",
        ];

        if Self::contains_action_target(&input, FILE_ACTIONS, FILE_TARGETS) {
            return true;
        }

        const WORKSPACE_TERMS: &[&str] = &[
            "workspace",
            "project",
            "repository",
            "repo",
            "codebase",
            "code base",
            "current project",
            "this project",
            "my project",
            "my code",
            "this code",
            "current code",
            "working directory",
            "directory",
            "folder",
            "file tree",
            "project tree",
            "project structure",
            "directory structure",
            "project layout",
            "codebase structure",
        ];

        if Self::contains_any(&input, WORKSPACE_TERMS) {
            return true;
        }

        const INSPECTION_PATTERNS: &[&str] = &[
            "search for ",
            "search ",
            "find ",
            "where is ",
            "locate ",
            "show me the files",
            "show the files",
            "show files",
            "list the files",
            "list files",
            "list the directory",
            "list directory",
            "show the directory",
            "show directory",
            "what files",
            "which files",
            "find the file",
            "find files",
            "find where",
            "search the code",
            "search my code",
            "search the project",
            "search the repository",
            "look through the code",
            "look through my code",
            "look through the project",
            "browse the project",
            "explore the project",
            "inspect the project",
            "inspect the code",
            "inspect my code",
            "what's in this project",
            "what is in this project",
            "what's in the project",
            "what is in the project",
            "how is this project structured",
            "how is the project structured",
        ];

        if Self::contains_any(&input, INSPECTION_PATTERNS) {
            return true;
        }

        const WORKSPACE_OPERATIONS: &[&str] = &[
            "build this",
            "build the project",
            "build my project",
            "compile this",
            "compile the project",
            "run the tests",
            "run tests",
            "test this",
            "test the project",
            "run the project",
            "run this project",
            "start the project",
            "launch the project",
            "lint the project",
            "format the project",
            "format this project",
            "check the project",
            "check my code",
            "debug this",
            "debug the project",
            "debug my code",
            "why does this fail",
            "why is this failing",
            "why doesn't this work",
            "why does this not work",
            "fix this error",
            "fix this bug",
            "fix the error",
            "fix the bug",
        ];

        if Self::contains_any(&input, WORKSPACE_OPERATIONS) {
            return true;
        }

        const TOOL_COMMANDS: &[&str] = &[
            "cargo ",
            "cargo",
            "npm ",
            "npm",
            "pnpm ",
            "pnpm",
            "yarn ",
            "yarn",
            "bun ",
            "bun",
            "python ",
            "pytest ",
            "pip ",
            "poetry ",
            "uv ",
            "go ",
            "go test",
            "go build",
            "cmake ",
            "make ",
            "swift ",
            "xcodebuild ",
            "dotnet ",
            "gradle ",
            "mvn ",
        ];

        Self::contains_any(&input, TOOL_COMMANDS)
    }

    fn contains_any(input: &str, terms: &[&str]) -> bool {
        terms.iter().any(|term| input.contains(term))
    }

    fn contains_action_target(input: &str, actions: &[&str], targets: &[&str]) -> bool {
        actions.iter().any(|action| {
            targets.iter().any(|target| {
                input.contains(&format!("{action} {target}"))
                    || input.contains(&format!("{action} the {target}"))
                    || input.contains(&format!("{action} my {target}"))
                    || input.contains(&format!("{action} this {target}"))
            })
        })
    }

    fn contains_path_reference(input: &str) -> bool {
        if input.contains('/') {
            return true;
        }

        if input.contains("~/") {
            return true;
        }

        const EXTENSIONS: &[&str] = &[
            ".rs", ".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".java", ".kt", ".swift", ".c",
            ".h", ".cpp", ".hpp", ".cc", ".zig", ".lua", ".rb", ".php", ".cs", ".fs", ".fsx",
            ".dart", ".vue", ".svelte", ".html", ".css", ".scss", ".json", ".toml", ".yaml",
            ".yml", ".xml", ".md", ".txt", ".lock",
        ];

        EXTENSIONS.iter().any(|ext| input.contains(ext))
    }

    // ========================================================================
    // Inspection
    // ========================================================================

    async fn initialize_workspace(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        self.execute_tool("list_directory", ".", tx, cancel, confirmation_rx)
            .await?;

        let (project, language, important_files) = self.detect_project();

        let structure = self.directory_structure();

        let galaxy = format!(
            "# GALAXY.md\n\n\
             Generated by Luma.\n\n\
             ## Project\n\n\
             {}\n\n\
             ## Language\n\n\
             {}\n\n\
             ## Important Files\n\n\
             {}\n\n\
             ## Structure\n\n\
             {}\n\n\
             ## Notes\n\n\
             This file is Luma's workspace memory.\n\n\
             Update it when major architecture changes happen.\n",
            project,
            language,
            if important_files.is_empty() {
                "No important files detected.".to_owned()
            } else {
                important_files.join("\n")
            },
            structure,
        );

        let input = serde_json::json!({
            "path": "GALAXY.md",
            "content": galaxy,
        });

        self.execute_tool(
            "write_file",
            &input.to_string(),
            tx,
            cancel,
            confirmation_rx,
        )
        .await?;

        self.context.add(
            MessageRole::System,
            format!("Updated GALAXY.md workspace memory:\n\n{}", galaxy),
        );

        Ok(())
    }

    fn detect_project(&self) -> (String, String, Vec<String>) {
        let mut project = "Unknown project".to_owned();
        let mut language = "Unknown".to_owned();
        let mut files = Vec::new();

        if Path::new("Cargo.toml").exists() {
            language = "Rust".into();
            files.push("Cargo.toml".into());

            if let Ok(content) = std::fs::read_to_string("Cargo.toml") {
                for line in content.lines() {
                    let Some(name) = line.strip_prefix("name =") else {
                        continue;
                    };

                    project = name.replace('"', "").trim().to_owned();
                    break;
                }
            }
        }

        if Path::new("package.json").exists() {
            language = "JavaScript / TypeScript".into();
            files.push("package.json".into());
        }

        if Path::new("pyproject.toml").exists() || Path::new("requirements.txt").exists() {
            language = "Python".into();

            if Path::new("pyproject.toml").exists() {
                files.push("pyproject.toml".into());
            }

            if Path::new("requirements.txt").exists() {
                files.push("requirements.txt".into());
            }
        }

        if language == "Unknown"
            && let Ok(entries) = std::fs::read_dir(".")
        {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name().to_string_lossy().into_owned();
                let detected = crate::workspace::language::detect_from_file(&name);
                if detected != crate::workspace::language::ProgrammingLanguage::Unknown {
                    language = format!("{detected:?}");
                    break;
                }
            }
        }

        (project, language, files)
    }

    fn directory_structure(&self) -> String {
        std::fs::read_dir(".")
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|_| "Unable to inspect.".into())
    }
}

// ============================================================================
// Output compaction
// ============================================================================

/// Make tool output suitable for another LLM.
///
/// This is deliberately separate from the actual tool output shown in the UI.
/// The react generally does not need every byte of a compiler log or file.
fn compact_tool_output(result: &str) -> String {
    truncate_text(result, MAX_TOOL_OBSERVATION)
}

/// UTF-8-safe truncation.
///
/// Never slice a String by byte index because that can split a UTF-8
/// character and panic.
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let mut output = String::with_capacity(max_chars + 64);

    for ch in text.chars().take(max_chars) {
        output.push(ch);
    }

    output.push_str("\n\n[output truncated by Luma]");

    output
}

// ============================================================================
// Helpers
// ============================================================================
