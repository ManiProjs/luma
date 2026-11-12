//! JSONL transport for the unified protocol.
//!
//! Reads [`ClientMessage`]s from stdin, writes [`ServerEvent`]s to stdout,
//! one JSON object per line. The desktop shell spawns `luma desktop-server`
//! and speaks this.
//!
//! This module is transport only. Every decision about what a message means
//! belongs to the core; the desktop shell is one client, not a privileged
//! one.

use anyhow::Result;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::chats;
use crate::config;
use crate::dashboard::{DashboardCommand, DashboardServer};
use crate::event::AgentEvent;
use crate::model::DynamicModel;
use crate::protocol::{
    ClientMessage, ErrorCategory, PROTOCOL_VERSION, ServerEvent, decode, encode,
};
use crate::provider::{ModelRequest, provider_for, supported_providers};
use crate::runtime::{RuntimeStore, TaskStatus};
use crate::session::{AgentCommand, Session};

/// How much input to buffer ahead of the reader.
///
/// The desktop shell may leave the process idle for a long time, so this is
/// sized generously rather than tuned for throughput.
const READ_BUFFER_BYTES: usize = 64 * 1024;

fn dashboard_assets_dir() -> Option<PathBuf> {
    std::env::var_os("LUMA_DASHBOARD_DIR").map(|directory| {
        let directory = PathBuf::from(directory);
        if directory.join("dist/index.html").is_file() {
            directory.join("dist")
        } else {
            directory
        }
    })
}

// ============================================================
// Server
// ============================================================

pub struct DesktopServer {
    session: Session,
    input_tx: mpsc::Sender<String>,
    cancel: CancellationToken,
    model: DynamicModel,
    confirmation_tx: mpsc::Sender<crate::agent::Confirmation>,
    dashboard: Option<std::sync::Arc<DashboardServer>>,
    runtime: RuntimeStore,
}

impl DesktopServer {
    pub fn new(
        session: Session,
        input_tx: mpsc::Sender<String>,
        cancel: CancellationToken,
        model: DynamicModel,
        confirmation_tx: mpsc::Sender<crate::agent::Confirmation>,
        runtime: RuntimeStore,
    ) -> Self {
        Self {
            session,
            input_tx,
            cancel,
            model,
            confirmation_tx,
            dashboard: None,
            runtime,
        }
    }

    /// Run the read/write loop until stdin closes or the core is cancelled.
    pub async fn run(mut self, mut events: mpsc::Receiver<AgentEvent>) -> Result<()> {
        let stdout = tokio::io::stdout();

        let mut stdout = tokio::io::BufWriter::new(stdout);

        let mut stdin = BufReader::with_capacity(READ_BUFFER_BYTES, tokio::io::stdin());

        let mut line = String::new();
        write_event(
            &mut stdout,
            ServerEvent::Ready {
                version: PROTOCOL_VERSION,
                features: vec![
                    "persistent_agents".into(),
                    "durable_tasks".into(),
                    "scoped_memory".into(),
                    "schedules".into(),
                    "event_replay".into(),
                    "permission_profiles".into(),
                    "restart_reconciliation".into(),
                    "persistent_gateway".into(),
                ],
            },
        )
        .await?;

        loop {
            tokio::select! {
                read = stdin.read_line(&mut line) => {
                    if read? == 0 {
                        break;
                    }

                    let trimmed = line.trim();

                    if !trimmed.is_empty() {
                        self.handle_line(trimmed, &mut stdout).await?;
                    }

                    line.clear();
                }

                Some(event) = events.recv() => {
                    if let Some(dashboard) = &self.dashboard {
                        let _ = dashboard.events.send(event.clone());
                    }
                    // The session is the core's view of the conversation, so
                    // it folds every event first. The session then decides
                    // which events are worth putting on the wire; a client
                    // must never see an event the core did not fold in.
                    let before = self.session.state().messages.len();

                    self.session.handle_event(event.clone());

                    if let Some(event) = self.session.advance(before) {
                        write_event(&mut stdout, ServerEvent::Agent { event }).await?;
                    }
                }

                _ = self.cancel.cancelled() => break,
            }
        }

        Ok(())
    }

    async fn handle_line<W>(&mut self, line: &str, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let message = match decode(line) {
            Ok(message) => message,

            Err(error) => {
                // A malformed line must not kill the session: the client may
                // be mid-update, and the next line is probably fine.
                tracing::warn!(%error, "ignoring unparseable client message");

                // Preserve request correlation even when this core build does
                // not understand a newer message variant. Without the ID the
                // desktop client cannot reject the matching pending request
                // and would misleadingly report a timeout instead.
                let request_id = serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("data")?
                            .get("request_id")?
                            .as_str()
                            .map(str::to_owned)
                    });

                return report_error(
                    stdout,
                    request_id,
                    format!("could not parse message: {error}"),
                    ErrorCategory::Fatal,
                )
                .await;
            }
        };

        match message {
            ClientMessage::GetSession => {
                write_event(
                    stdout,
                    ServerEvent::Session {
                        session: self.session.view(),
                    },
                )
                .await
            }

            ClientMessage::SetModel { request_id, model } => {
                self.change_model(request_id, model, stdout).await
            }

            ClientMessage::SetSessionModel { request_id, model } => {
                self.change_session_model(request_id, model, stdout).await
            }

            ClientMessage::StartDashboard {
                request_id,
                host,
                port,
            } => self.start_dashboard(request_id, host, port, stdout).await,

            message @ (ClientMessage::AgentsList { .. }
            | ClientMessage::AgentCreate { .. }
            | ClientMessage::AgentDelete { .. }
            | ClientMessage::AgentUpdate { .. }
            | ClientMessage::AgentSetStatus { .. }
            | ClientMessage::TasksList { .. }
            | ClientMessage::TaskCreate { .. }
            | ClientMessage::TaskCancel { .. }
            | ClientMessage::TaskRetry { .. }
            | ClientMessage::ApprovalsList { .. }
            | ClientMessage::ApprovalResolve { .. }
            | ClientMessage::MemoryList { .. }
            | ClientMessage::MemoryAdd { .. }
            | ClientMessage::MemoryDelete { .. }
            | ClientMessage::SchedulesList { .. }
            | ClientMessage::ScheduleCreate { .. }
            | ClientMessage::ScheduleDelete { .. }
            | ClientMessage::EventsSince { .. }
            | ClientMessage::RuntimeCapabilities { .. }) => {
                self.serve_runtime(message, stdout).await
            }

            message if message.request_id().is_some() => serve(message, stdout).await,

            // Everything left is a conversation command, which the session
            // owns; `into_command` only returns `None` for the arms above.
            message => {
                let command = message
                    .into_command()
                    .expect("every non-request client message is a session command");

                self.run_command(command, stdout).await
            }
        }
    }

    async fn serve_runtime<W>(&self, message: ClientMessage, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        if let ClientMessage::ApprovalResolve {
            request_id,
            id,
            allowed,
        } = &message
        {
            let approval = self
                .runtime
                .list_pending_approvals()?
                .into_iter()
                .find(|approval| approval.id == *id)
                .ok_or_else(|| anyhow::anyhow!("approval not found or already resolved: {id}"))?;
            let task = self
                .runtime
                .get_task(&approval.task_id)?
                .ok_or_else(|| anyhow::anyhow!("approval task no longer exists"))?;
            if task.status != TaskStatus::WaitingForApproval {
                anyhow::bail!("the task is no longer waiting for this approval");
            }
            let agent = self
                .runtime
                .get_agent(&approval.agent_id)?
                .ok_or_else(|| anyhow::anyhow!("approval agent no longer exists"))?;
            if matches!(
                agent.status,
                crate::runtime::AgentStatus::Paused | crate::runtime::AgentStatus::Stopped
            ) {
                anyhow::bail!("resume the agent before resolving its approval");
            }
            let approval = self.runtime.resolve_approval(id, *allowed)?;
            return write_event(
                stdout,
                ServerEvent::ApprovalResolved {
                    request_id: request_id.clone(),
                    approval,
                },
            )
            .await;
        }
        let request_id = message.request_id().map(str::to_owned);
        let result: Result<ServerEvent> = (|| {
            Ok(match message {
                ClientMessage::AgentsList { request_id } => ServerEvent::Agents {
                    request_id,
                    agents: self.runtime.list_agents()?,
                },
                ClientMessage::AgentCreate {
                    request_id,
                    name,
                    description,
                    kind,
                    configuration,
                } => ServerEvent::AgentSaved {
                    request_id,
                    agent: self
                        .runtime
                        .create_agent(&name, description, kind, configuration)?,
                },
                ClientMessage::AgentDelete { request_id, id } => ServerEvent::AgentDeleted {
                    request_id,
                    id: id.clone(),
                    deleted: self.runtime.delete_agent(&id)?,
                },
                ClientMessage::AgentUpdate {
                    request_id,
                    id,
                    name,
                    description,
                    configuration,
                } => ServerEvent::AgentSaved {
                    request_id,
                    agent: self
                        .runtime
                        .update_agent(&id, name, description, configuration)?,
                },
                ClientMessage::AgentSetStatus {
                    request_id,
                    id,
                    status,
                } => {
                    if !matches!(
                        status,
                        crate::runtime::AgentStatus::Idle
                            | crate::runtime::AgentStatus::Paused
                            | crate::runtime::AgentStatus::Stopped
                    ) {
                        anyhow::bail!("clients may only set an agent to idle, paused, or stopped");
                    }
                    self.runtime.set_agent_status(&id, status)?;
                    let agent = self
                        .runtime
                        .get_agent(&id)?
                        .ok_or_else(|| anyhow::anyhow!("agent not found: {id}"))?;
                    ServerEvent::AgentSaved { request_id, agent }
                }
                ClientMessage::TasksList {
                    request_id,
                    agent_id,
                    limit,
                } => ServerEvent::Tasks {
                    request_id,
                    tasks: self
                        .runtime
                        .list_tasks(agent_id.as_deref(), limit.unwrap_or(100))?,
                },
                ClientMessage::TaskCreate {
                    request_id,
                    agent_id,
                    request,
                    title,
                    workspace,
                    recover_on_restart,
                } => ServerEvent::TaskSaved {
                    request_id,
                    task: self.runtime.create_task(
                        &agent_id,
                        &request,
                        title,
                        workspace,
                        recover_on_restart,
                        None,
                    )?,
                },
                ClientMessage::TaskCancel { request_id, id } => {
                    let task = self
                        .runtime
                        .get_task(&id)?
                        .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;
                    if task.status == TaskStatus::Running
                        || task.status == TaskStatus::WaitingForApproval
                    {
                        self.runtime.enqueue_gateway_cancel(&id)?;
                        ServerEvent::TaskSaved { request_id, task }
                    } else if task.status == TaskStatus::Queued || task.status == TaskStatus::Paused
                    {
                        ServerEvent::TaskSaved {
                            request_id,
                            task: self.runtime.transition_task(
                                &id,
                                TaskStatus::Cancelled,
                                None,
                                None,
                            )?,
                        }
                    } else {
                        anyhow::bail!("task is already in a terminal state")
                    }
                }
                ClientMessage::TaskRetry { request_id, id } => {
                    let task = self
                        .runtime
                        .get_task(&id)?
                        .ok_or_else(|| anyhow::anyhow!("task not found: {id}"))?;
                    if task.status != TaskStatus::Failed && task.status != TaskStatus::Interrupted {
                        anyhow::bail!("only failed or interrupted tasks can be retried");
                    }
                    if self
                        .runtime
                        .get_agent(&task.agent_id)?
                        .is_none_or(|agent| agent.status == crate::runtime::AgentStatus::Stopped)
                    {
                        anyhow::bail!("cannot retry work for a stopped agent");
                    }
                    ServerEvent::TaskSaved {
                        request_id,
                        task: self
                            .runtime
                            .transition_task(&id, TaskStatus::Queued, None, None)?,
                    }
                }
                ClientMessage::TaskRename {
                    request_id,
                    id,
                    title,
                } => ServerEvent::TaskSaved {
                    request_id,
                    task: self.runtime.rename_task(&id, &title)?,
                },
                ClientMessage::TaskDelete { request_id, id } => ServerEvent::TaskDeleted {
                    request_id,
                    deleted: self.runtime.delete_task(&id)?,
                    id,
                },
                ClientMessage::PluginsList { request_id } => ServerEvent::Plugins {
                    request_id,
                    plugins: crate::plugins::PluginManager::default().list()?,
                },
                ClientMessage::PluginInstall { request_id, path } => ServerEvent::PluginSaved {
                    request_id,
                    plugin: crate::plugins::PluginManager::default()
                        .install(std::path::Path::new(&path))?,
                },
                ClientMessage::PluginSetEnabled {
                    request_id,
                    id,
                    enabled,
                } => ServerEvent::PluginSaved {
                    request_id,
                    plugin: crate::plugins::PluginManager::default().set_enabled(&id, enabled)?,
                },
                ClientMessage::PluginUninstall { request_id, id } => ServerEvent::PluginDeleted {
                    request_id,
                    deleted: crate::plugins::PluginManager::default().uninstall(&id)?,
                    id,
                },
                ClientMessage::McpList { request_id } => ServerEvent::McpServers {
                    request_id,
                    servers: crate::mcp::McpManager::default().list()?,
                },
                ClientMessage::McpAddStdio {
                    request_id,
                    id,
                    command,
                    args,
                    env,
                } => ServerEvent::McpSaved {
                    request_id,
                    server: crate::mcp::McpManager::default().add(id, command, args, env)?,
                },
                ClientMessage::McpAddHttp {
                    request_id,
                    id,
                    url,
                    headers,
                } => ServerEvent::McpSaved {
                    request_id,
                    server: crate::mcp::McpManager::default().add_http(id, url, headers)?,
                },
                ClientMessage::McpSetEnabled {
                    request_id,
                    id,
                    enabled,
                } => ServerEvent::McpSaved {
                    request_id,
                    server: crate::mcp::McpManager::default().set_enabled(&id, enabled)?,
                },
                ClientMessage::McpUpdate { request_id, server } => ServerEvent::McpSaved {
                    request_id,
                    server: crate::mcp::McpManager::default().update(server)?,
                },
                ClientMessage::McpRemove { request_id, id } => ServerEvent::McpDeleted {
                    request_id,
                    deleted: crate::mcp::McpManager::default().remove(&id)?,
                    id,
                },
                ClientMessage::ApprovalsList { request_id } => ServerEvent::Approvals {
                    request_id,
                    approvals: self.runtime.list_pending_approvals()?,
                },
                ClientMessage::MemoryList {
                    request_id,
                    scope,
                    scope_id,
                    query,
                    limit,
                } => ServerEvent::Memory {
                    request_id,
                    entries: self.runtime.list_memory(
                        scope,
                        &scope_id,
                        query.as_deref(),
                        limit.unwrap_or(100),
                    )?,
                },
                ClientMessage::MemoryAdd {
                    request_id,
                    scope,
                    scope_id,
                    content,
                    source,
                } => ServerEvent::MemorySaved {
                    request_id,
                    entry: self
                        .runtime
                        .add_memory(scope, &scope_id, &content, source)?,
                },
                ClientMessage::MemoryDelete { request_id, id } => ServerEvent::MemoryDeleted {
                    request_id,
                    deleted: self.runtime.delete_memory(&id)?,
                },
                ClientMessage::SchedulesList {
                    request_id,
                    agent_id,
                } => ServerEvent::Schedules {
                    request_id,
                    schedules: self.runtime.list_schedules(agent_id.as_deref())?,
                },
                ClientMessage::ScheduleCreate {
                    request_id,
                    agent_id,
                    title,
                    prompt,
                    run_at,
                    every_seconds,
                } => ServerEvent::ScheduleSaved {
                    request_id,
                    schedule: self.runtime.create_schedule(
                        &agent_id,
                        &title,
                        &prompt,
                        run_at,
                        every_seconds,
                    )?,
                },
                ClientMessage::ScheduleDelete { request_id, id } => ServerEvent::ScheduleDeleted {
                    request_id,
                    deleted: self.runtime.delete_schedule(&id)?,
                },
                ClientMessage::ScheduleSetEnabled {
                    request_id,
                    id,
                    enabled,
                } => ServerEvent::ScheduleSaved {
                    request_id,
                    schedule: self.runtime.set_schedule_enabled(&id, enabled)?,
                },
                ClientMessage::EventsSince {
                    request_id,
                    sequence,
                    limit,
                    agent_id,
                } => ServerEvent::RuntimeEvents {
                    request_id,
                    events: if let Some(agent_id) = agent_id.as_deref() {
                        self.runtime
                            .events_for_agent(agent_id, sequence, limit.unwrap_or(100))?
                    } else {
                        self.runtime.events_since(sequence, limit.unwrap_or(100))?
                    },
                },
                ClientMessage::RuntimeCapabilities { request_id } => {
                    ServerEvent::RuntimeCapabilities {
                        request_id,
                        features: vec![
                            "persistent_agents".into(),
                            "durable_tasks".into(),
                            "scoped_memory".into(),
                            "schedules".into(),
                            "event_replay".into(),
                            "permission_profiles".into(),
                            "restart_reconciliation".into(),
                            "persistent_gateway".into(),
                        ],
                        storage_schema: 2,
                    }
                }
                _ => anyhow::bail!("not a runtime request"),
            })
        })();
        match result {
            Ok(event) => write_event(stdout, event).await,
            Err(error) => {
                report_error(stdout, request_id, error.to_string(), ErrorCategory::Fatal).await
            }
        }
    }

    async fn change_model<W>(
        &mut self,
        request_id: String,
        name: String,
        stdout: &mut W,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let name = name.trim();
        if name.is_empty() {
            return report_error(
                stdout,
                Some(request_id),
                "model name cannot be empty".into(),
                ErrorCategory::Fatal,
            )
            .await;
        }

        let mut settings = match config::load() {
            Ok(config) => config,
            Err(error) => {
                return report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await;
            }
        };
        settings.model.name = name.to_string();

        let provider = crate::provider::provider_by_id(&settings.model.provider)
            .or_else(|| {
                settings
                    .providers
                    .iter()
                    .find(|provider| provider.id == settings.model.provider)
                    .cloned()
            })
            .or_else(|| {
                settings.model.provider.starts_with("custom-").then(|| {
                    crate::provider::SupportedProvider {
                        id: settings.model.provider.clone(),
                        name: "Custom provider".into(),
                        description: "Temporary custom OpenAI-compatible provider".into(),
                        category: crate::provider::ProviderCategory::Custom,
                        protocol: crate::provider::ProviderProtocol::OpenAiCompatible,
                        endpoint: settings.model.endpoint.clone(),
                        requires_api_key: settings.model.api_key.is_some(),
                        supports_model_discovery: true,
                        api_key_env: None,
                    }
                })
            });
        let Some(provider) = provider else {
            return report_error(
                stdout,
                Some(request_id),
                format!("Unknown configured provider: {}", settings.model.provider),
                ErrorCategory::Fatal,
            )
            .await;
        };
        let request = ModelRequest {
            provider,
            api_key: settings.model.api_key.clone(),
            endpoint: Some(settings.model.endpoint.clone()),
        };
        if let Err(error) = provider_for(&request.provider).test(&request, name).await {
            return report_error(
                stdout,
                Some(request_id),
                format!("Model test failed: {error}"),
                ErrorCategory::Transient,
            )
            .await;
        }

        if let Err(error) = config::save(&settings) {
            return report_error(
                stdout,
                Some(request_id),
                error.to_string(),
                ErrorCategory::Transient,
            )
            .await;
        }

        self.model.replace(&settings.model).await;
        self.session
            .set_model(settings.model.provider.clone(), settings.model.name.clone());
        write_event(
            stdout,
            ServerEvent::ModelChanged {
                request_id,
                provider: settings.model.provider,
                model: settings.model.name,
            },
        )
        .await?;
        write_event(
            stdout,
            ServerEvent::Session {
                session: self.session.view(),
            },
        )
        .await
    }

    async fn change_session_model<W>(
        &mut self,
        request_id: String,
        model: crate::config::ModelConfig,
        stdout: &mut W,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let provider = crate::provider::provider_by_id(&model.provider)
            .or_else(|| {
                config::load()
                    .ok()?
                    .providers
                    .into_iter()
                    .find(|provider| provider.id == model.provider)
            })
            .or_else(|| {
                model
                    .provider
                    .starts_with("custom-")
                    .then(|| crate::provider::SupportedProvider {
                        id: model.provider.clone(),
                        name: "Custom provider".into(),
                        description: "Custom OpenAI-compatible provider".into(),
                        category: crate::provider::ProviderCategory::Custom,
                        protocol: crate::provider::ProviderProtocol::OpenAiCompatible,
                        endpoint: model.endpoint.clone(),
                        requires_api_key: model.api_key.is_some(),
                        supports_model_discovery: true,
                        api_key_env: None,
                    })
            });
        let Some(provider) = provider else {
            return report_error(
                stdout,
                Some(request_id),
                format!("Unknown provider: {}", model.provider),
                ErrorCategory::Fatal,
            )
            .await;
        };
        if model.name.trim().is_empty() || model.endpoint.trim().is_empty() {
            return report_error(
                stdout,
                Some(request_id),
                "Provider endpoint and model name are required.".into(),
                ErrorCategory::Fatal,
            )
            .await;
        }
        let request = ModelRequest {
            provider,
            api_key: model.api_key.clone(),
            endpoint: Some(model.endpoint.clone()),
        };
        if let Err(error) = provider_for(&request.provider)
            .test(&request, &model.name)
            .await
        {
            return report_error(
                stdout,
                Some(request_id),
                format!("Model test failed: {error}"),
                ErrorCategory::Transient,
            )
            .await;
        }
        self.model.replace(&model).await;
        self.session
            .set_model(model.provider.clone(), model.name.clone());
        write_event(
            stdout,
            ServerEvent::ModelChanged {
                request_id,
                provider: model.provider,
                model: model.name,
            },
        )
        .await?;
        write_event(
            stdout,
            ServerEvent::Session {
                session: self.session.view(),
            },
        )
        .await
    }

    async fn start_dashboard<W>(
        &mut self,
        request_id: String,
        host: String,
        port: u16,
        stdout: &mut W,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        if self.dashboard.is_some() {
            return report_error(
                stdout,
                Some(request_id),
                "Dashboard is already running in this core process.".into(),
                ErrorCategory::Fatal,
            )
            .await;
        }

        let bind_address: std::net::IpAddr = match host.as_str() {
            "localhost" => std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            value => match value.parse() {
                Ok(address) => address,
                Err(_) => {
                    return report_error(
                        stdout,
                        Some(request_id),
                        "Dashboard host must be a loopback IP address.".into(),
                        ErrorCategory::Fatal,
                    )
                    .await;
                }
            },
        };
        if !bind_address.is_loopback() {
            return report_error(stdout, Some(request_id), "Unauthenticated dashboard access is restricted to loopback. Remote access requires authenticated transport.".into(), ErrorCategory::Fatal).await;
        }

        let (dashboard, mut dashboard_commands) =
            DashboardServer::new(dashboard_assets_dir(), 256, self.runtime.clone());
        let listener = match tokio::net::TcpListener::bind((bind_address, port)).await {
            Ok(listener) => listener,
            Err(error) => {
                return report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await;
            }
        };
        let address = listener.local_addr()?.to_string();

        let input_tx = self.input_tx.clone();
        let confirmation_tx = self.confirmation_tx.clone();
        let request_cancel = self.session.request_cancellation();
        tokio::spawn(async move {
            while let Some(command) = dashboard_commands.recv().await {
                match command {
                    DashboardCommand::Chat(text) => {
                        if input_tx.send(text).await.is_err() {
                            break;
                        }
                    }
                    DashboardCommand::Confirm { allowed } => {
                        let confirmation = if allowed {
                            crate::agent::Confirmation::Allow
                        } else {
                            crate::agent::Confirmation::Deny
                        };
                        if confirmation_tx.send(confirmation).await.is_err() {
                            break;
                        }
                    }
                    DashboardCommand::Cancel => request_cancel.cancel(),
                }
            }
        });

        self.dashboard = Some(dashboard.clone());
        tokio::spawn(async move {
            if let Err(error) = dashboard.run_listener(listener).await {
                tracing::error!(%error, "Dashboard server stopped");
            }
        });

        write_event(
            stdout,
            ServerEvent::DashboardStarted {
                request_id,
                address,
            },
        )
        .await
    }

    /// Dispatch a session command, reporting rejections back to the client
    /// instead of taking the process down.
    async fn run_command<W>(&mut self, command: AgentCommand, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let forwarded = match self.session.apply(command.clone()).await {
            // Anything the session resolves locally (`/help`, `/quit`) or
            // routes to the agent itself — a confirmation, a cancel — returns
            // `None`, and has already been acted on.
            Ok(forwarded) => forwarded,

            Err(error) => {
                tracing::warn!(%error, "client command rejected");

                return report_error(stdout, None, error.to_string(), ErrorCategory::Transient)
                    .await;
            }
        };

        match forwarded {
            Some(text) => {
                write_event(
                    stdout,
                    ServerEvent::Session {
                        session: self.session.view(),
                    },
                )
                .await?;
                self.forward(text, stdout).await
            }

            // Local commands (`/help`, `/clear`) and state-only commands do
            // not generate an agent event, so send the updated snapshot to
            // keep clients' transcript and status in sync.
            None => {
                write_event(
                    stdout,
                    ServerEvent::Session {
                        session: self.session.view(),
                    },
                )
                .await
            }
        }
    }

    /// Hand a prompt to the agent. The session has already recorded it in the
    /// transcript, so a failure here is a dead agent, not a bad request.
    async fn forward<W>(&self, text: String, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        match self.input_tx.send(text).await {
            Ok(()) => Ok(()),

            Err(_) => {
                report_error(
                    stdout,
                    None,
                    "the agent is no longer accepting input".to_string(),
                    ErrorCategory::Fatal,
                )
                .await
            }
        }
    }
}

// ============================================================
// Setup and introspection
// ============================================================

async fn serve<W>(message: ClientMessage, stdout: &mut W) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    match message {
        ClientMessage::GetSetupConfig { request_id } => match crate::config::load_or_default() {
            Ok(config) => {
                write_event(stdout, ServerEvent::SetupConfig { request_id, config }).await?;
            }

            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::SaveSetupConfig { request_id, config } => {
            match crate::config::save(&config) {
                Ok(()) => {
                    write_event(stdout, ServerEvent::SetupSaved { request_id }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::GetProviders { request_id } => {
            let mut providers = supported_providers();
            if let Ok(config) = crate::config::load_or_default() {
                for saved in config.providers {
                    if !providers.iter().any(|provider| provider.id == saved.id) {
                        providers.push(saved);
                    }
                }
            }
            write_event(
                stdout,
                ServerEvent::Providers {
                    request_id,
                    providers,
                },
            )
            .await?;
        }

        ClientMessage::GetModels {
            request_id,
            provider,
            api_key,
            endpoint,
        } => {
            let request = ModelRequest {
                provider: provider.clone(),
                api_key,
                endpoint,
            };

            match provider_for(&provider).list_models(&request).await {
                Ok(models) => {
                    write_event(stdout, ServerEvent::Models { request_id, models }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::TestProvider {
            request_id,
            provider,
            api_key,
            endpoint,
            model,
        } => {
            let request = ModelRequest {
                provider: provider.clone(),
                api_key,
                endpoint,
            };

            match provider_for(&provider).test(&request, &model).await {
                Ok(()) => {
                    write_event(stdout, ServerEvent::ProviderTested { request_id }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::SetModel { .. } => {
            // Handled by DesktopServer because it owns the live model handle.
        }

        ClientMessage::StartDashboard { .. } => {
            // Handled by DesktopServer because it owns the live dashboard task.
        }

        ClientMessage::HistoryList { request_id } => match chats::list() {
            Ok(chats) => {
                write_event(stdout, ServerEvent::HistoryList { request_id, chats }).await?;
            }
            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::HistoryShow { request_id, id } => match chats::load(&id) {
            Ok(chat) => {
                write_event(stdout, ServerEvent::HistoryChat { request_id, chat }).await?;
            }
            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::HistoryDelete { request_id, id } => match chats::delete(&id) {
            Ok(deleted) => {
                write_event(
                    stdout,
                    ServerEvent::HistoryDeleted {
                        request_id,
                        id,
                        deleted,
                    },
                )
                .await?;
            }
            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::HistoryPrune { request_id } => match chats::prune() {
            Ok(removed) => {
                write_event(
                    stdout,
                    ServerEvent::HistoryPruned {
                        request_id,
                        removed,
                    },
                )
                .await?;
            }
            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::SummaryAdd {
            request_id,
            id,
            summary,
        } => match chats::add_summary(&id, summary) {
            Ok(()) => {
                write_event(stdout, ServerEvent::SummarySaved { request_id, id }).await?;
            }
            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        other => {
            tracing::debug!(?other, "unhandled client message");
        }
    }

    Ok(())
}

// ============================================================
// Output
// ============================================================

async fn write_event<W>(writer: &mut W, event: ServerEvent) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let line = encode(&event)?;

    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

async fn report_error<W>(
    writer: &mut W,
    request_id: Option<String>,
    message: String,
    category: ErrorCategory,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    write_event(
        writer,
        ServerEvent::Error {
            request_id,
            message,
            category,
        },
    )
    .await
}
