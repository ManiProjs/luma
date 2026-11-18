//! Isolated execution process for durable runtime tasks.
//!
//! Each task runs in its own `luma-core task-worker` process. This gives it a
//! real workspace working directory and lets the supervisor terminate shell
//! subprocess trees without changing the server's global current directory.

use anyhow::{Context, Result, anyhow, bail};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
    task::JoinSet,
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
    if let Some(ceiling) = task.permission_ceiling.as_ref() {
        permissions = permissions.intersect(ceiling);
    }
    let workspace_id = std::env::current_dir()?
        .canonicalize()?
        .display()
        .to_string();
    if agent_record.kind == crate::runtime::AgentKind::Teammates {
        registry.register(tools::code_intelligence::FindSymbols::scoped(
            std::path::PathBuf::from(&workspace_id),
        ));
        registry.register(tools::code_intelligence::FindReferences::scoped(
            std::path::PathBuf::from(&workspace_id),
        ));
        registry.register(tools::code_intelligence::AnalyzeCodebase::scoped(
            std::path::PathBuf::from(&workspace_id),
        ));
        registry.register(tools::memory::SaveMemory::new(
            store.clone(),
            agent_record.id.clone(),
        ));
        registry.register(tools::memory::RecallMemory::new(
            store.clone(),
            agent_record.id.clone(),
            workspace_id.clone(),
            inherit_global,
        ));
    }
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
        for name in registry.names().into_iter().chain(
            [
                "list_teammates",
                "create_team",
                "list_teams",
                "add_team_member",
                "add_task_dependency",
                "create_monitor",
                "delegate_task",
                "get_delegated_task",
                "list_delegated_tasks",
            ]
            .into_iter()
            .map(str::to_owned),
        ) {
            if !enabled_tools.iter().any(|enabled| enabled == &name)
                && !matches!(
                    name.as_str(),
                    "ask_question"
                        | "set_titlebar_status"
                        | "recall_memory"
                        | "save_memory"
                        | "list_teammates"
                        | "create_team"
                        | "list_teams"
                        | "add_team_member"
                        | "add_task_dependency"
                        | "create_monitor"
                        | "delegate_task"
                        | "get_delegated_task"
                        | "list_delegated_tasks"
                )
            {
                permissions.denied_tools.push(name);
            }
        }
        permissions.denied_tools.sort();
        permissions.denied_tools.dedup();
    }
    if agent_record.kind == crate::runtime::AgentKind::Teammates {
        registry.register(tools::delegation::ListTeammates::new(store.clone()));
        registry.register(tools::delegation::CreateTeam::new(store.clone()));
        registry.register(tools::delegation::ListTeams::new(store.clone()));
        registry.register(tools::delegation::AddTeamMember::new(store.clone()));
        registry.register(tools::delegation::AddTaskDependency::new(
            store.clone(),
            task.id.clone(),
            agent_record.id.clone(),
            task.conversation_id.clone(),
        ));
        registry.register(tools::delegation::CreateMonitor::new(
            store.clone(),
            agent_record.id.clone(),
        ));
        registry.register(tools::delegation::DelegateTask::new(
            store.clone(),
            task.id.clone(),
            agent_record.id.clone(),
            permissions.clone(),
        ));
        registry.register(tools::delegation::GetDelegatedTask::new(
            store.clone(),
            agent_record.id.clone(),
            task.conversation_id.clone(),
        ));
        registry.register(tools::delegation::ListDelegatedTasks::new(
            store.clone(),
            agent_record.id.clone(),
            task.conversation_id.clone(),
        ));
    }
    if !enabled_tools.is_empty() {
        // The runtime only exposes explicitly configured tools. Read-only
        // policy is enforced again inside Agent before invocation.
        // Collaboration is a core Teammates capability rather than an
        // optional connector. Keep its tools available even when the user
        // explicitly selects a smaller tool set; delegate_task still passes
        // through the ordinary mutation approval policy above.
        let mut retained = enabled_tools.to_vec();
        retained.extend([
            "list_teammates".to_owned(),
            "create_team".to_owned(),
            "list_teams".to_owned(),
            "add_team_member".to_owned(),
            "add_task_dependency".to_owned(),
            "create_monitor".to_owned(),
            "delegate_task".to_owned(),
            "get_delegated_task".to_owned(),
            "list_delegated_tasks".to_owned(),
        ]);
        registry.retain(&retained);
    }
    let model = DynamicModel::new(&model_config);
    let title_model = model.clone();
    let react = if agent_record.kind == crate::runtime::AgentKind::Teammates {
        ReAct::new_teammate(model.clone(), &registry)
    } else {
        ReAct::new(model.clone(), &registry)
    };
    let galaxy_path = std::env::current_dir()?.join("GALAXY.md");
    let galaxy = std::fs::read_to_string(&galaxy_path).unwrap_or_default();
    let mut history = History::default();
    let mut previous_tasks = store.list_tasks(Some(&agent_record.id), 500)?;
    previous_tasks.retain(|previous| {
        previous.id != task.id
            && previous.conversation_id == task.conversation_id
            && previous.schedule_id.is_none()
    });
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
    agent.set_force_react(agent_record.kind == crate::runtime::AgentKind::Teammates);
    agent.set_permission_profile(permissions);
    agent.set_workspace_root(&std::env::current_dir()?)?;
    if let Some(plan) = store.get_task_plan(&task.id)? {
        agent.add_system_context(format!(
            "# Persisted task plan\n\nApproval status: {}. Follow this plan if approved. If it is not approved, treat it only as a proposal and request approval before implementing it.\n\n{}",
            if plan.approved { "approved" } else { "not approved" },
            plan.plan
        ));
    }
    if let Some(recovery) = task
        .checkpoint
        .as_ref()
        .and_then(|checkpoint| checkpoint.get("action_recovery"))
    {
        agent.add_system_context(format!(
            "# Required task recovery\n\n\
             The user explicitly acknowledged the prior tool-action journal before this retry.\n\
             Treat the journal below as execution history. Inspect the current workspace and\n\
             external state before taking action. Do not repeat actions recorded as finished.\n\
             For actions recorded as failed or uncertain, verify whether their intended effect\n\
             occurred before deciding whether another attempt is necessary. Never assume an\n\
             uncertain action did not happen. Continue from the current state and explain any\n\
             discrepancy you discover. If omitted_older_actions is greater than zero, this\n\
             journal is incomplete: inspect the workspace and every relevant external system\n\
             broadly, and ask the user before repeating any operation whose outcome cannot be\n\
             established.\n\n{}",
            serde_json::to_string_pretty(recovery)?
        ));
    }
    if inherit_global && let Some(global_configuration) = global_configuration {
        if !global_configuration.system_prompt.trim().is_empty() {
            agent.add_system_context(global_configuration.system_prompt.clone());
        }
        for skill_name in &global_configuration.skills {
            let skill = tools::skills::load_agent_skill(&std::env::current_dir()?, skill_name)
                .with_context(|| format!("loading global skill '{skill_name}'"))?;
            agent.add_system_context(skill);
        }
    }
    if !agent_record.configuration.system_prompt.trim().is_empty() {
        agent.add_system_context(agent_record.configuration.system_prompt.clone());
    }
    if let Some(specialization) = agent_record
        .configuration
        .specialization
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let guidance = specialization_guidance(specialization);
        agent.add_system_context(format!(
            "# Teammate specialization\n\nYour assigned role is **{specialization}**. Apply the methods and review standards of this specialty while respecting the task scope and your existing permissions.\n\n{guidance}"
        ));
    }
    if agent_record.kind == crate::runtime::AgentKind::Teammates {
        let affiliations = store.teams_for_agent(&agent_record.id)?;
        if !affiliations.is_empty() {
            let roles = affiliations
                .iter()
                .map(|(team, role)| {
                    format!(
                        "- {} — {} ({})",
                        team.name,
                        role,
                        team.description.as_deref().unwrap_or("No team description")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            agent.add_system_context(format!(
                "# Team memberships\n\nYou are a member of these persistent Teams. Coordinate work toward their shared purposes and honor your assigned role in each group:\n{roles}"
            ));
        }
    }
    if agent_record.kind == crate::runtime::AgentKind::Teammates {
        agent.add_system_context(
            "# Long-term memory\n\n\
             You have persistent memory across tasks. Before relying on past preferences, decisions,\n\
             or project facts that are not present in this task, use recall_memory. When the user\n\
             shares a stable preference, an important project decision, a reusable lesson, or other\n\
             information that will materially help future tasks, save it with save_memory. Do this\n\
             selectively: do not save temporary task details, speculative conclusions, secrets,\n\
             credentials, or sensitive personal information. If a saved fact becomes outdated, save\n\
             the corrected fact and make the change clear. Never claim to remember something unless\n\
             it was saved or is present in the current conversation."
                .to_owned(),
        );
        agent.add_system_context(
            "# Teammate collaboration\n\n\
             Discover specialists with list_teammates. Create persistent groups with create_team,\n\
             add Teammates with add_team_member, and inspect teams with list_teams. Delegate\n\
             independent subtasks with delegate_task, then use add_task_dependency when one task\n\
             must wait for another. Include necessary context and clear deliverables; never delegate\n\
             secrets or work requiring broader permissions. Independent workspaces can execute in\n\
             parallel, while each checkout is protected by an exclusive workspace lease. When all\n\
             children finish, Luma automatically queues a continuation with their results. Review\n\
             those results before claiming delegated work is complete."
                .to_owned(),
        );
        agent.add_system_context(
            "# Rich responses\n\n\
             You can combine normal prose with rich visual blocks in your final answer. When a\n\
             visual genuinely improves the answer, emit a fenced `luma` block containing exactly\n\
             one JSON object. Supported blocks:\n\
             - image: {\"type\":\"image\",\"url\":\"https://...\",\"alt\":\"...\",\"caption\":\"...\"}\n\
             - gallery: {\"type\":\"gallery\",\"images\":[{\"url\":\"https://...\",\"alt\":\"...\"}]}\n\
             - table: {\"type\":\"table\",\"title\":\"...\",\"columns\":[\"A\",\"B\"],\"rows\":[[\"x\",2]]}\n\
             - stat: {\"type\":\"stat\",\"label\":\"...\",\"value\":42,\"detail\":\"...\"}\n\
             - chart: {\"type\":\"chart\",\"title\":\"...\",\"labels\":[\"A\",\"B\"],\"values\":[4,7],\"value_suffix\":\"%\"}\n\
             - callout: {\"type\":\"callout\",\"title\":\"...\",\"text\":\"...\",\"tone\":\"info|success|warning\"}\n\
             - link: {\"type\":\"link\",\"label\":\"...\",\"url\":\"https://...\",\"description\":\"...\"}\n\
             Example syntax: an opening triple-backtick fence tagged `luma`, one JSON object,\n\
             then a closing triple-backtick fence. Use only real,\n\
             verified image URLs and factual data; never invent sources, metrics, or links.\n\
             Prefer Markdown tables for simple comparisons. Keep rich blocks selective and useful;\n\
             do not use them decoratively or for information better expressed in a sentence."
                .to_owned(),
        );
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
    let mut relevant_memories = Vec::new();
    for (scope, scope_id, label) in memory_scopes {
        for entry in store.list_memory(scope, &scope_id, None, 500)? {
            let score = crate::tools::memory::relevance_score(&task.request, &entry.content);
            if score > 0 {
                relevant_memories.push((score, entry.updated_at, label, entry));
            }
        }
    }
    relevant_memories
        .sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let mut seen_memory = std::collections::HashSet::new();
    for (_, _, label, entry) in relevant_memories
        .into_iter()
        .filter(|(_, _, _, entry)| seen_memory.insert(entry.content.to_lowercase()))
        .take(12)
    {
        agent.add_system_context(format!(
            "Relevant {label} ({}): {}",
            entry.id, entry.content
        ));
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

/// Single-host task supervisor. Independent workspaces can run concurrently;
/// a canonical workspace lease prevents two workers from editing one checkout
/// at the same time. Each Teammate also controls its own concurrency ceiling.
#[derive(Clone)]
pub struct RuntimeSupervisor {
    store: RuntimeStore,
}

impl RuntimeSupervisor {
    pub fn new(store: RuntimeStore) -> Self {
        Self { store }
    }

    pub async fn run(self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut workers: JoinSet<(String, String, String, Result<()>)> = JoinSet::new();
        let mut active_workspaces = HashSet::new();
        let mut active_agents: HashMap<String, usize> = HashMap::new();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                completed = workers.join_next(), if !workers.is_empty() => {
                    if let Some(completed) = completed {
                        match completed {
                            Ok((task_id, agent_id, workspace, result)) => {
                                active_workspaces.remove(&workspace);
                                if let Some(count) = active_agents.get_mut(&agent_id) {
                                    *count = count.saturating_sub(1);
                                    if *count == 0 {
                                        active_agents.remove(&agent_id);
                                        if let Ok(Some(agent)) = self.store.get_agent(&agent_id)
                                            && matches!(agent.status, AgentStatus::Active | AgentStatus::Waiting | AgentStatus::Recovering) {
                                                let _ = self.store.set_agent_status(&agent_id, AgentStatus::Idle);
                                            }
                                    } else if let Ok(Some(agent)) = self.store.get_agent(&agent_id)
                                        && agent.status == AgentStatus::Waiting {
                                            let _ = self.store.set_agent_status(&agent_id, AgentStatus::Active);
                                    }
                                }
                                if let Err(error) = result {
                                    tracing::error!(task_id = %task_id, %error, "background task failed");
                                    if let Err(journal_error) = self.store.mark_unfinished_task_actions_uncertain(&task_id) {
                                        tracing::warn!(task_id = %task_id, %journal_error, "failed to mark unfinished tool actions uncertain");
                                    }
                                    if let Ok(Some(current)) = self.store.get_task(&task_id)
                                        && matches!(current.status, TaskStatus::Queued | TaskStatus::Running | TaskStatus::WaitingForApproval | TaskStatus::WaitingForInput) {
                                            let _ = self.store.transition_task(&task_id, TaskStatus::Failed, Some(error.to_string()), None);
                                        }
                                }
                                if let Err(error) = self.resume_after_terminal(&task_id) {
                                    tracing::warn!(task_id = %task_id, %error, "failed to resume Teammate after delegated work");
                                }
                            }
                            Err(error) => tracing::error!(%error, "task supervisor worker panicked"),
                        }
                    }
                }
                _ = interval.tick() => {
                    if let Err(error) = self.enqueue_due_schedules() {
                        tracing::error!(%error, "failed to enqueue due schedules");
                    }
                    if workers.len() >= 8 {
                        continue;
                    }
                    let queued = match self.store.list_queued_tasks() {
                        Ok(tasks) => tasks,
                        Err(error) => { tracing::error!(%error, "failed to inspect queued tasks"); Vec::new() }
                    };
                    for task in queued {
                        if workers.len() >= 8 { break; }
                        let Ok(Some(agent)) = self.store.get_agent(&task.agent_id) else { continue; };
                        if agent.status == AgentStatus::Stopped {
                            if let Err(error) = self.store.transition_task(&task.id, TaskStatus::Cancelled, None, None) {
                                tracing::warn!(task_id = %task.id, %error, "failed to cancel queued work for a stopped teammate");
                            } else if let Err(error) = self.resume_after_terminal(&task.id) {
                                tracing::warn!(task_id = %task.id, %error, "failed to resume parent after cancelling stopped Teammate work");
                            }
                            continue;
                        }
                        if agent.status == AgentStatus::Paused { continue; }
                        match self.store.task_dependencies_ready(&task.id) {
                            Ok((true, _)) => {}
                            Ok((false, Some(reason))) => {
                                if let Err(error) = self.store.transition_task(&task.id, TaskStatus::Failed, Some(reason), None) {
                                    tracing::warn!(task_id = %task.id, %error, "failed to stop task with an unsuccessful dependency");
                                } else if let Err(error) = self.resume_after_terminal(&task.id) {
                                    tracing::warn!(task_id = %task.id, %error, "failed to resume parent after dependency failure");
                                }
                                continue;
                            }
                            Ok((false, None)) => continue,
                            Err(error) => {
                                tracing::warn!(task_id = %task.id, %error, "could not inspect task dependencies");
                                continue;
                            }
                        }
                        let agent_limit = agent.configuration.max_concurrent_tasks.clamp(1, 8);
                        if active_agents.get(&agent.id).copied().unwrap_or(0) >= agent_limit { continue; }
                        let workspace_path = task.workspace.as_deref()
                            .or(agent.configuration.workspace.as_deref())
                            .map(PathBuf::from)
                            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                        let workspace = workspace_path.canonicalize().unwrap_or(workspace_path).to_string_lossy().into_owned();
                        if active_workspaces.contains(&workspace) { continue; }
                        let claimed = match self.store.claim_queued_task(&task.id) {
                            Ok(Some(task)) => task,
                            Ok(None) => continue,
                            Err(error) => {
                                tracing::warn!(task_id = %task.id, %error, "could not claim queued task");
                                continue;
                            }
                        };
                        active_workspaces.insert(workspace.clone());
                        *active_agents.entry(agent.id.clone()).or_default() += 1;
                        let supervisor = self.clone();
                        let task_id = claimed.id.clone();
                        let agent_id = agent.id.clone();
                        let worker_workspace = workspace.clone();
                        let worker_shutdown = shutdown.clone();
                        workers.spawn(async move {
                            let result = supervisor.run_one(&task_id, &worker_shutdown).await;
                            (task_id, agent_id, worker_workspace, result)
                        });
                        }
                }
            }
        }
        while let Some(completed) = workers.join_next().await {
            match completed {
                Ok((task_id, _, _, result)) => {
                    if let Err(error) = result {
                        tracing::warn!(task_id = %task_id, %error, "task stopped during gateway shutdown");
                        if let Err(journal_error) =
                            self.store.mark_unfinished_task_actions_uncertain(&task_id)
                        {
                            tracing::warn!(task_id = %task_id, %journal_error, "failed to mark unfinished tool actions uncertain during shutdown");
                        }
                        if let Ok(Some(current)) = self.store.get_task(&task_id)
                            && matches!(
                                current.status,
                                TaskStatus::Queued
                                    | TaskStatus::Running
                                    | TaskStatus::WaitingForApproval
                                    | TaskStatus::WaitingForInput
                            )
                            && let Err(transition_error) = self.store.transition_task(
                                &task_id,
                                TaskStatus::Failed,
                                Some(error.to_string()),
                                None,
                            )
                        {
                            tracing::warn!(task_id = %task_id, %transition_error, "failed to persist task failure during shutdown");
                        }
                    }
                    if let Err(error) = self.resume_after_terminal(&task_id) {
                        tracing::warn!(task_id = %task_id, %error, "failed to resume parent after task shutdown");
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "task supervisor worker panicked during shutdown")
                }
            }
        }
    }

    fn enqueue_due_schedules(&self) -> Result<()> {
        let timestamp = super::runtime::unix_now();
        for schedule in self.store.due_schedules(timestamp)? {
            self.store.enqueue_due_schedule(&schedule.id, timestamp)?;
        }
        self.store.dispatch_event_triggers(timestamp)?;
        Ok(())
    }

    fn resume_after_terminal(&self, task_id: &str) -> Result<()> {
        let Some(task) = self.store.get_task(task_id)? else {
            return Ok(());
        };
        self.store.resume_parent_after_children(task_id)?;
        if let Some(parent_task_id) = task.parent_task_id.as_deref() {
            self.store.resume_parent_after_children(parent_task_id)?;
        }
        Ok(())
    }

    async fn run_one(&self, task_id: &str, shutdown: &CancellationToken) -> Result<()> {
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
        if task.status != TaskStatus::Running {
            bail!(
                "task must be atomically claimed before worker launch (current status: {:?})",
                task.status
            );
        }
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
        let mut pending_approval_tool: Option<String> = None;
        let mut active_actions: HashMap<String, Vec<String>> = HashMap::new();
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
                                if allowed && pending_approval_tool.as_deref() == Some("approve_plan") {
                                    self.store.approve_task_plan(task_id)?;
                                }
                                pending_approval_tool = None;
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
                        } else if value.get("type").and_then(|v| v.as_str()) == Some("event")
                            && let Ok(event) = serde_json::from_value::<AgentEvent>(value["data"].clone()) {
                                match &event {
                                    AgentEvent::PlanGenerated(plan) => {
                                        self.store.save_task_plan(task_id, plan)?;
                                    }
                                    AgentEvent::ToolStarted { name, input } => {
                                        let action = self.store.start_task_action(task_id, name, input)?;
                                        active_actions.entry(name.clone()).or_default().push(action.id);
                                    }
                                    AgentEvent::ToolProgress { name, output, progress_percent } => {
                                        if let Some(action_id) = active_actions.get(name).and_then(|items| items.last()) {
                                            let summary = progress_percent
                                                .map(|percent| format!("[{percent}%] {output}"))
                                                .unwrap_or_else(|| output.clone());
                                            self.store.update_task_action_progress(action_id, &summary)?;
                                        }
                                    }
                                    AgentEvent::ToolFinished { name, duration_ms, failed, diff } => {
                                        if let Some(action_id) = active_actions.get_mut(name).and_then(Vec::pop) {
                                            let summary = diff.clone().unwrap_or_else(|| {
                                                format!("Tool {} in {duration_ms} ms", if *failed { "failed" } else { "finished" })
                                            });
                                            self.store.finish_task_action(&action_id, Some(&summary), *failed)?;
                                        }
                                    }
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
                                        pending_approval_tool = Some(name.clone());
                                    }
                                    AgentEvent::QuestionRequired { question_id, prompt, choices } => {
                                        let checkpoint = serde_json::json!({"question_id":question_id,"prompt":prompt,"choices":choices});
                                        let _ = self.store.transition_task(task_id, TaskStatus::WaitingForInput, None, Some(checkpoint));
                                        let _ = self.store.set_agent_status(&agent.id, AgentStatus::Waiting);
                                    }
                                    AgentEvent::Error(message) => failure = Some(message.clone()),
                                    AgentEvent::Finished
                                        if !cancelled => { terminal_status = if failure.is_some() { TaskStatus::Failed } else { TaskStatus::Completed }; }
                                    _ => {}
                                }
                                if !matches!(event, AgentEvent::PlanGenerated(_)) {
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
        self.store.mark_unfinished_task_actions_uncertain(task_id)?;
        let _ = self
            .store
            .transition_task(task_id, terminal_status, failure, None);
        Ok(())
    }
}

fn specialization_guidance(role: &str) -> &'static str {
    match role.to_ascii_lowercase().as_str() {
        "software architect" => {
            "Define clear boundaries and interfaces, trace data flow, compare viable designs, and account for migration, failure modes, and long-term maintenance before recommending structural changes."
        }
        "frontend developer" => {
            "Trace the existing component and state patterns, build accessible responsive interactions, preserve established visual language, and verify loading, empty, error, and keyboard states."
        }
        "backend developer" => {
            "Follow the service and persistence boundaries, validate inputs, make state changes transactional where needed, handle failures explicitly, and preserve API compatibility."
        }
        "security reviewer" => {
            "Threat-model trust boundaries and attacker-controlled inputs, inspect authorization and secret handling, verify dependency and execution risks, and report concrete exploit paths with practical mitigations."
        }
        "qa engineer" => {
            "Derive tests from user-visible behavior and failure cases, reproduce defects with minimal fixtures, cover regressions at the narrowest useful level, and report exact commands and outcomes."
        }
        "devops engineer" => {
            "Inspect deployment and runtime assumptions, make operations observable and repeatable, account for rollback and resource limits, and validate configuration across supported environments."
        }
        "code reviewer" => {
            "Prioritize correctness, regressions, security, and maintainability; trace changed behavior through callers; cite concrete locations; and omit stylistic findings without user impact."
        }
        "technical writer" => {
            "Write task-focused documentation with accurate prerequisites, runnable examples, expected results, and recovery guidance; verify every command and keep terminology consistent with the product."
        }
        "research analyst" => {
            "Separate verified facts from assumptions, compare primary sources, record dates and uncertainty, and connect evidence directly to the decision the task needs."
        }
        "data analyst" => {
            "Validate data shape and quality, state assumptions, choose methods appropriate to the question, check edge cases, and make conclusions traceable to the underlying data."
        }
        "product strategist" => {
            "Clarify the user problem and target outcome, map workflows and trade-offs, prioritize by impact and effort, and define observable success criteria before proposing scope."
        }
        "performance engineer" => {
            "Measure before changing code, identify the dominant bottleneck, distinguish CPU, memory, I/O, and rendering costs, and verify improvements with repeatable benchmarks and regression checks."
        }
        _ => {
            "Apply the relevant professional methods for this specialty. Make assumptions explicit, inspect existing project conventions, produce evidence for recommendations, and stay within the task scope and granted permissions."
        }
    }
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::{WorkerOutput, specialization_guidance};

    #[test]
    fn worker_done_message_has_a_stable_supervisor_protocol_tag() {
        let value = serde_json::to_value(WorkerOutput::Done).unwrap();
        assert_eq!(value, serde_json::json!({ "type": "done" }));
    }

    #[test]
    fn specialization_presets_have_distinct_guidance_and_custom_roles_fall_back() {
        let roles = [
            "Software Architect",
            "Frontend Developer",
            "Backend Developer",
            "Security Reviewer",
            "QA Engineer",
            "DevOps Engineer",
            "Code Reviewer",
            "Technical Writer",
            "Research Analyst",
            "Data Analyst",
            "Product Strategist",
            "Performance Engineer",
        ];
        let guidance = roles
            .iter()
            .map(|role| specialization_guidance(role))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(guidance.len(), roles.len());
        assert!(specialization_guidance("Custom specialist").contains("professional methods"));
    }
}
