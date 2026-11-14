//! Isolated execution process for durable runtime tasks.
//!
//! Each task runs in its own `luma-core task-worker` process. This gives it a
//! real workspace working directory and lets the supervisor terminate shell
//! subprocess trees without changing the server's global current directory.

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::{Agent, Confirmation},
    config::{self, ModelConfig},
    context::{Message, MessageRole},
    event::AgentEvent,
    history::{History, HistoryMessage},
    model::{CompletionRequest, DynamicModel, Model},
    react::ReAct,
    runtime::{AgentStatus, MemoryScope, RuntimeStore, TaskStatus},
    session::RequestCancellation,
    tools,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerCommand {
    Confirm { allowed: bool },
    AnswerQuestion { question_id: String, answer: String },
    Cancel,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum WorkerOutput {
    Event(AgentEvent),
    Ready {
        task_id: String,
    },
    Error {
        message: String,
    },
    /// Signals that the worker no longer needs commands on stdin. Tokio's
    /// stdin reader uses a blocking thread that cannot be cancelled merely
    /// by dropping its future, so the parent must close the pipe at this point
    /// before waiting for the child process to exit.
    Done,
}

/// Entry point for the hidden worker subcommand.
pub async fn run_task_worker(task_id: &str) -> Result<()> {
    let store = RuntimeStore::open_default()?;
    let task = store
        .get_task(task_id)?
        .ok_or_else(|| anyhow!("task not found: {task_id}"))?;
    let agent_record = store
        .get_agent(&task.agent_id)?
        .ok_or_else(|| anyhow!("agent not found: {}", task.agent_id))?;
    if task.status != TaskStatus::Running {
        bail!("task worker can only attach to a running task");
    }

    let config = config::load_or_default()?;
    let global = store.get_agent(crate::runtime::GLOBAL_AGENT_ID)?;
    let inherit_global = agent_record.id == crate::runtime::GLOBAL_AGENT_ID
        || agent_record.configuration.use_global_context;
    let global_configuration = global.as_ref().map(|record| &record.configuration);
    let mut model_config: ModelConfig = config.model.clone();
    if let Some(provider) = agent_record.configuration.provider.as_ref().or_else(|| {
        if inherit_global {
            global_configuration.and_then(|value| value.provider.as_ref())
        } else {
            None
        }
    }) {
        model_config.provider = provider.clone();
        model_config.api_key = config.provider_api_keys.get(provider).cloned().or_else(|| {
            (config.model.provider == *provider)
                .then(|| config.model.api_key.clone())
                .flatten()
        });
    }
    if let Some(model) = agent_record.configuration.model.as_ref().or_else(|| {
        if inherit_global {
            global_configuration.and_then(|value| value.model.as_ref())
        } else {
            None
        }
    }) {
        model_config.name = model.clone();
    }
    if let Some(endpoint) = agent_record.configuration.endpoint.as_ref().or_else(|| {
        if inherit_global {
            global_configuration.and_then(|value| value.endpoint.as_ref())
        } else {
            None
        }
    }) {
        model_config.endpoint = endpoint.clone();
    }

    let mut registry = if agent_record.kind == crate::runtime::AgentKind::Teammates {
        tools::teammate_registry()
    } else {
        tools::default_registry()
    };
    // Native Luma plugins and MCP servers are global connectors. A Teammate
    // receives them only when it opts into the Global teammate's context.
    if inherit_global {
        crate::plugins::register_enabled_tools(&mut registry)?;
        crate::mcp::register_enabled_tools(&mut registry)?;
    }
    // A Teammate still owns its Code tools, web access and attached skills;
    // inheriting global context only adds shared defaults and connectors.
    let enabled_tools = agent_record.configuration.enabled_tools.as_slice();
    if !enabled_tools.is_empty() {
        // The runtime only exposes explicitly configured tools. Read-only
        // policy is enforced again inside Agent before invocation.
        registry.retain(enabled_tools);
    }
    let model = DynamicModel::new(&model_config);
    let title_model = model.clone();
    let react = ReAct::new(model.clone(), &registry);
    let galaxy_path = std::env::current_dir()?.join("GALAXY.md");
    let galaxy = std::fs::read_to_string(&galaxy_path).unwrap_or_default();
    let mut history = History::default();
    let mut previous_tasks = store.list_tasks(Some(&agent_record.id), 500)?;
    previous_tasks.retain(|previous| previous.id != task.id && previous.schedule_id.is_none());
    previous_tasks.reverse();
    for previous in previous_tasks {
        let answer = store.task_text_response(&previous.id)?;
        history.messages.push(HistoryMessage {
            role: "user".into(),
            content: previous.request,
        });
        if !answer.trim().is_empty() {
            history.messages.push(HistoryMessage {
                role: "assistant".into(),
                content: answer,
            });
        }
    }
    let mut agent = Agent::new(model, react, registry, history, galaxy);
    let mut permissions = agent_record.configuration.permissions.clone();
    if agent_record.kind == crate::runtime::AgentKind::Teammates {
        permissions.allow_network = agent_record.configuration.network_access.unwrap_or(true);
        if permissions.allow_network {
            for tool in ["web_search", "fetch_url"] {
                if !permissions.denied_tools.iter().any(|item| item == tool)
                    && !permissions.allowed_tools.iter().any(|item| item == tool)
                {
                    permissions.allowed_tools.push(tool.to_owned());
                }
            }
        }
    }
    agent.set_permission_profile(permissions);
    agent.set_workspace_root(&std::env::current_dir()?)?;
    if inherit_global {
        if let Some(global_configuration) = global_configuration {
            if !global_configuration.system_prompt.trim().is_empty() {
                agent.add_system_context(global_configuration.system_prompt.clone());
            }
            for skill_name in &global_configuration.skills {
                let skill = tools::skills::load_agent_skill(&std::env::current_dir()?, skill_name)
                    .with_context(|| format!("loading global skill '{skill_name}'"))?;
                agent.add_system_context(skill);
            }
        }
    }
    if !agent_record.configuration.system_prompt.trim().is_empty() {
        agent.add_system_context(agent_record.configuration.system_prompt.clone());
    }
    // An agent's own skills remain its own even when it also opts into the
    // Global teammate's shared context.
    for skill_name in &agent_record.configuration.skills {
        let skill = tools::skills::load_agent_skill(&std::env::current_dir()?, skill_name)
            .with_context(|| {
                format!(
                    "loading skill '{skill_name}' for teammate {}",
                    agent_record.name
                )
            })?;
        agent.add_system_context(skill);
    }
    let workspace_id = std::env::current_dir()?
        .canonicalize()?
        .display()
        .to_string();
    let mut memory_scopes = vec![
        (MemoryScope::Project, workspace_id, "Project memory"),
        (MemoryScope::Agent, agent_record.id.clone(), "Agent memory"),
        (MemoryScope::Task, task.id.clone(), "Task memory"),
    ];
    if inherit_global {
        memory_scopes.push((
            MemoryScope::Agent,
            crate::runtime::GLOBAL_AGENT_ID.to_owned(),
            "Global teammate memory",
        ));
        memory_scopes.push((MemoryScope::User, "default".to_owned(), "User memory"));
    }
    for (scope, scope_id, label) in memory_scopes {
        for entry in store.list_memory(scope, &scope_id, None, 100)? {
            agent.add_system_context(format!("{label} ({}): {}", entry.id, entry.content));
        }
    }

    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(128);
    let (input_tx, input_rx) = mpsc::channel::<String>(1);
    let (confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(8);
    let (question_tx, question_rx) = mpsc::channel::<crate::agent::QuestionAnswer>(1);
    let worker_question_tx = question_tx.clone();
    let shutdown = CancellationToken::new();
    let request_cancel = RequestCancellation::new(shutdown.clone());
    let worker_request_cancel = request_cancel.clone();
    let worker_cancel = shutdown.clone();
    let worker = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                worker_cancel,
                worker_request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });
    let task_for_title = task.clone();
    input_tx.send(task.request).await?;
    drop(input_tx);

    let stdin = tokio::io::stdin();
    let mut stdin = BufReader::new(stdin);
    let mut line = String::new();
    let stdout = tokio::io::stdout();
    let mut stdout = tokio::io::BufWriter::new(stdout);
    write_worker_output(
        &mut stdout,
        WorkerOutput::Ready {
            task_id: task_id.into(),
        },
    )
    .await?;

    let mut finished = false;
    let mut cancelled = false;
    let mut final_response = String::new();
    while !finished {
        tokio::select! {
            event = event_rx.recv() => match event {
                Some(event) => {
                    if let AgentEvent::TextDelta(chunk) = &event {
                        final_response.push_str(chunk);
                    }
                    if matches!(event, AgentEvent::Cancelled) {
                        cancelled = true;
                    }
                    finished = matches!(event, AgentEvent::Finished);
                    write_worker_output(&mut stdout, WorkerOutput::Event(event)).await?;
                }
                None => break,
            },
            read = stdin.read_line(&mut line) => {
                if read? == 0 { shutdown.cancel(); break; }
                if let Ok(command) = serde_json::from_str::<WorkerCommand>(line.trim()) {
                    match command {
                        WorkerCommand::Confirm { allowed } => {
                            let confirmation = if allowed { Confirmation::Allow } else { Confirmation::Deny };
                            let _ = confirmation_tx.send(confirmation).await;
                        }
                        WorkerCommand::AnswerQuestion { question_id, answer } => {
                            let _ = worker_question_tx.send(crate::agent::QuestionAnswer { question_id, answer }).await;
                        }
                        WorkerCommand::Cancel => {
                            cancelled = true;
                            request_cancel.cancel();
                        }
                    }
                }
                line.clear();
            }
        }
    }
    shutdown.cancel();
    let result = match worker.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            write_worker_output(
                &mut stdout,
                WorkerOutput::Error {
                    message: error.to_string(),
                },
            )
            .await?;
            Err(error)
        }
        Err(error) => Err(error.into()),
    };
    if result.is_ok() && finished && !cancelled {
        let title_cancel = CancellationToken::new();
        let title_generation = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            generate_task_title(
                &store,
                &title_model,
                &task_for_title,
                &final_response,
                &title_cancel,
            ),
        );
        tokio::pin!(title_generation);
        loop {
            tokio::select! {
                result = &mut title_generation => {
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => tracing::warn!(task_id = %task_for_title.id, %error, "could not generate teammate chat title"),
                        Err(_) => tracing::warn!(task_id = %task_for_title.id, "chat title generation timed out; keeping the original title"),
                    }
                    break;
                }
                read = stdin.read_line(&mut line) => {
                    if read? == 0 {
                        break;
                    }
                    if matches!(serde_json::from_str::<WorkerCommand>(line.trim()), Ok(WorkerCommand::Cancel)) {
                        title_cancel.cancel();
                        break;
                    }
                    line.clear();
                }
            }
        }
    }
    write_worker_output(&mut stdout, WorkerOutput::Done).await?;
    result
}

async fn generate_task_title(
    store: &RuntimeStore,
    model: &DynamicModel,
    original_task: &crate::runtime::TaskRecord,
    response: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let Some(current) = store.get_task(&original_task.id)? else {
        return Ok(());
    };
    let fallback_title: String = original_task
        .request
        .lines()
        .next()
        .unwrap_or("New chat")
        .chars()
        .take(100)
        .collect();
    if current.title != fallback_title || current.status != TaskStatus::Running {
        return Ok(());
    }
    if response.trim().is_empty() {
        return Ok(());
    }
    let request = CompletionRequest { messages: vec![
        Message { role: MessageRole::System, content: "Create a concise chat title from the user's request and assistant response. Return only the title, without quotes or punctuation at the end. Use at most 7 words.".into() },
        Message { role: MessageRole::User, content: format!("User request:\n{}\n\nAssistant response:\n{}", original_task.request, response.chars().take(2400).collect::<String>()) },
    ] };
    let mut stream = tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        result = model.stream(request) => result?,
    };
    let mut title = String::new();
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return Ok(()),
            chunk = stream.next() => {
                let Some(chunk) = chunk else { break; };
                title.push_str(&chunk?);
                if title.len() > 512 { break; }
            }
        }
    }
    let title = title
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(['"', '\'', '`'])
        .trim();
    let mut title_bytes = 0;
    let title: String = title
        .chars()
        .take_while(|character| {
            title_bytes += character.len_utf8();
            title_bytes <= 120
        })
        .collect();
    if !title.is_empty() {
        let Some(latest) = store.get_task(&original_task.id)? else {
            return Ok(());
        };
        if latest.title != fallback_title || latest.status != TaskStatus::Running {
            return Ok(());
        }
        store.rename_task(&original_task.id, &title)?;
    }
    Ok(())
}

async fn write_worker_output<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    output: WorkerOutput,
) -> Result<()> {
    let mut line = serde_json::to_vec(&output)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}

/// Single-host task supervisor. Work is deliberately serialized at first:
/// this avoids concurrent edits to the same checkout until workspace leases
/// and isolated worktrees are implemented.
pub struct RuntimeSupervisor {
    store: RuntimeStore,
}

impl RuntimeSupervisor {
    pub fn new(store: RuntimeStore) -> Self {
        Self { store }
    }

    pub async fn run(mut self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(error) = self.enqueue_due_schedules() {
                        tracing::error!(%error, "failed to enqueue due schedules");
                    }
                    let queued = match self.store.list_tasks(None, 500) {
                        Ok(tasks) => {
                            let mut eligible = None;
                            for task in tasks.into_iter().filter(|task| task.status == TaskStatus::Queued) {
                                match self.store.get_agent(&task.agent_id) {
                                    Ok(Some(agent)) if agent.status == AgentStatus::Stopped => {
                                        if let Err(error) = self.store.transition_task(
                                            &task.id,
                                            TaskStatus::Cancelled,
                                            None,
                                            None,
                                        ) {
                                            tracing::warn!(task_id = %task.id, %error, "failed to cancel queued work for a stopped teammate");
                                        }
                                    }
                                    Ok(Some(agent)) if agent.status != AgentStatus::Paused => {
                                        eligible = Some(task);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                            eligible
                        }
                        Err(error) => { tracing::error!(%error, "failed to inspect queued tasks"); None }
                    };
                    if let Some(task) = queued {
                        if let Err(error) = self.run_one(&task.id, &shutdown).await {
                            tracing::error!(task_id = %task.id, %error, "background task failed");
                            if let Ok(Some(current)) = self.store.get_task(&task.id) {
                                if matches!(current.status, TaskStatus::Queued | TaskStatus::Running | TaskStatus::WaitingForApproval | TaskStatus::WaitingForInput) {
                                    let _ = self.store.transition_task(&task.id, TaskStatus::Failed, Some(error.to_string()), None);
                                }
                            }
                            if let Ok(Some(agent)) = self.store.get_agent(&task.agent_id) {
                                if matches!(agent.status, AgentStatus::Active | AgentStatus::Waiting | AgentStatus::Recovering) {
                                    let _ = self.store.set_agent_status(&agent.id, AgentStatus::Idle);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn enqueue_due_schedules(&self) -> Result<()> {
        let timestamp = super::runtime::unix_now();
        for schedule in self.store.due_schedules(timestamp)? {
            self.store.enqueue_due_schedule(&schedule.id, timestamp)?;
        }
        Ok(())
    }

    async fn run_one(&mut self, task_id: &str, shutdown: &CancellationToken) -> Result<()> {
        let task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("task disappeared: {task_id}"))?;
        let agent = self
            .store
            .get_agent(&task.agent_id)?
            .ok_or_else(|| anyhow!("agent disappeared: {}", task.agent_id))?;
        if matches!(agent.status, AgentStatus::Paused | AgentStatus::Stopped) {
            return Ok(());
        }
        let workspace = task
            .workspace
            .clone()
            .or(agent.configuration.workspace.clone())
            .unwrap_or(std::env::current_dir()?.display().to_string());
        let workspace = PathBuf::from(workspace)
            .canonicalize()
            .with_context(|| "task workspace is unavailable")?;
        crate::runtime::validate_workspace(
            workspace
                .to_str()
                .ok_or_else(|| anyhow!("workspace path is not valid UTF-8"))?,
            &agent.configuration,
        )?;
        let _ = self
            .store
            .transition_task(task_id, TaskStatus::Running, None, None)?;
        self.store
            .set_agent_status(&agent.id, AgentStatus::Active)?;

        let executable = std::env::current_exe().context("locating luma-core executable")?;
        let mut child = tokio::process::Command::new(executable)
            .arg("task-worker")
            .arg(task_id)
            .current_dir(&workspace)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("starting isolated task worker")?;
        let mut child_stdin = Some(
            child
                .stdin
                .take()
                .ok_or_else(|| anyhow!("worker stdin unavailable"))?,
        );
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("worker stdout unavailable"))?;
        let mut reader = BufReader::new(child_stdout);
        let mut line = String::new();
        let mut terminal_status = TaskStatus::Failed;
        let mut failure: Option<String> = None;
        let mut cancelled = false;
        let mut pending_approval_id: Option<String> = None;
        let mut command_interval = tokio::time::interval(std::time::Duration::from_millis(150));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    if let Some(stdin) = child_stdin.as_mut() {
                        stdin.write_all(br#"{"type":"cancel"}"#).await.ok();
                        stdin.write_all(b"\n").await.ok();
                        stdin.flush().await.ok();
                    }
                    cancelled = true;
                    break;
                }
                _ = command_interval.tick(), if child_stdin.is_some() => {
                    let Some(stdin) = child_stdin.as_mut() else { continue; };
                    for command in self.store.pending_gateway_commands(task_id)? {
                        match command.command {
                            crate::runtime::GatewayCommand::Cancel => {
                                stdin.write_all(br#"{"type":"cancel"}"#).await?;
                                stdin.write_all(b"\n").await?;
                                stdin.flush().await?;
                                self.store.mark_gateway_command_processed(&command.id)?;
                                cancelled = true;
                                pending_approval_id = None;
                            }
                            crate::runtime::GatewayCommand::Confirm { approval_id, allowed }
                                if pending_approval_id.as_deref() == Some(approval_id.as_str()) =>
                            {
                                let payload = serde_json::json!({"type":"confirm","allowed":allowed});
                                stdin.write_all(serde_json::to_string(&payload)?.as_bytes()).await?;
                                stdin.write_all(b"\n").await?;
                                stdin.flush().await?;
                                self.store.mark_gateway_command_processed(&command.id)?;
                                pending_approval_id = None;
                                let _ = self.store.transition_task(task_id, TaskStatus::Running, None, None);
                                let _ = self.store.set_agent_status(&agent.id, AgentStatus::Active);
                            }
                            crate::runtime::GatewayCommand::Confirm { .. } => {}
                            crate::runtime::GatewayCommand::AnswerQuestion { question_id, answer } => {
                                let payload = serde_json::json!({"type":"answer_question","question_id":question_id,"answer":answer});
                                stdin.write_all(serde_json::to_string(&payload)?.as_bytes()).await?;
                                stdin.write_all(b"\n").await?;
                                stdin.flush().await?;
                                self.store.mark_gateway_command_processed(&command.id)?;
                                let _ = self.store.set_agent_status(&agent.id, AgentStatus::Active);
                            }
                        }
                    }
                }
                read = reader.read_line(&mut line) => {
                    if read? == 0 { break; }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                        if value.get("type").and_then(|v| v.as_str()) == Some("done") {
                            // Close the pipe so Tokio's non-cancellable stdin
                            // blocking read can finish and let the worker exit.
                            child_stdin.take();
                        } else if value.get("type").and_then(|v| v.as_str()) == Some("event") {
                            if let Ok(event) = serde_json::from_value::<AgentEvent>(value["data"].clone()) {
                                match &event {
                                    AgentEvent::Status { state } if *state == crate::event::AgentStatus::Waiting => {
                                        let _ = self.store.set_agent_status(&agent.id, AgentStatus::Waiting);
                                    }
                                    AgentEvent::Status { state } if *state == crate::event::AgentStatus::Thinking || *state == crate::event::AgentStatus::Doing || *state == crate::event::AgentStatus::Talking => {
                                        let _ = self.store.set_agent_status(&agent.id, AgentStatus::Active);
                                    }
                                    AgentEvent::Cancelled => { cancelled = true; terminal_status = TaskStatus::Cancelled; }
                                    AgentEvent::ConfirmationRequired { name, input } => {
                                        let _ = self.store.transition_task(task_id, TaskStatus::WaitingForApproval, None, None);
                                        let _ = self.store.set_agent_status(&agent.id, AgentStatus::Waiting);
                                        self.store.expire_pending_approvals(task_id)?;
                                        let approval = self.store.create_approval(&agent.id, task_id, name, input)?;
                                        pending_approval_id = Some(approval.id);
                                    }
                                    AgentEvent::QuestionRequired { question_id, prompt, choices } => {
                                        let checkpoint = serde_json::json!({"question_id":question_id,"prompt":prompt,"choices":choices});
                                        let _ = self.store.transition_task(task_id, TaskStatus::WaitingForInput, None, Some(checkpoint));
                                        let _ = self.store.set_agent_status(&agent.id, AgentStatus::Waiting);
                                    }
                                    AgentEvent::Error(message) => failure = Some(message.clone()),
                                    AgentEvent::Finished => {
                                        if !cancelled { terminal_status = if failure.is_some() { TaskStatus::Failed } else { TaskStatus::Completed }; }
                                    }
                                    _ => {}
                                }
                                let _ = self.store.append_event(Some(&agent.id), Some(task_id), "task.agent_event", serde_json::to_value(&event)?);
                            }
                        }
                    }
                    line.clear();
                }
            }
        }
        // Also close stdin when the gateway itself is shutting down.
        child_stdin.take();
        let status =
            match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
                Ok(status) => status?,
                Err(_) => {
                    child.start_kill().ok();
                    child.wait().await?
                }
            };
        if !status.success() && failure.is_none() && !cancelled {
            failure = Some(format!("task worker exited with {status}"));
        }
        if cancelled && terminal_status != TaskStatus::Completed {
            terminal_status = if shutdown.is_cancelled() {
                TaskStatus::Interrupted
            } else {
                TaskStatus::Cancelled
            };
        }
        let _ = self
            .store
            .transition_task(task_id, terminal_status, failure, None);
        if let Ok(Some(current_agent)) = self.store.get_agent(&agent.id) {
            if matches!(
                current_agent.status,
                AgentStatus::Active | AgentStatus::Waiting | AgentStatus::Recovering
            ) {
                let _ = self.store.set_agent_status(&agent.id, AgentStatus::Idle);
            }
        }
        Ok(())
    }
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::WorkerOutput;

    #[test]
    fn worker_done_message_has_a_stable_supervisor_protocol_tag() {
        let value = serde_json::to_value(WorkerOutput::Done).unwrap();
        assert_eq!(value, serde_json::json!({ "type": "done" }));
    }
}
