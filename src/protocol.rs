//! The single wire protocol between the Luma core and any front end.
//!
//! One protocol serves all three clients:
//!
//! - `luma` in the terminal (today Ratatui, soon Ink)
//! - `luma dashboard` (the browser)
//! - `luma desktop-server` (the desktop shell, over stdio JSONL)
//!
//! Messages are newline-delimited JSON objects on a transport-agnostic
//! channel, so the same types work for stdio, a WebSocket, or an in-process
//! `mpsc`. Every message carries `type`, and every request carries a
//! `request_id` so a client can match a response to what it asked for.

use serde::{Deserialize, Serialize};

use crate::chats::{ChatArchive, ChatSummary};
use crate::config::Config;
use crate::event::AgentEvent;
use crate::provider::{SupportedModel, SupportedProvider};
use crate::session::AgentCommand;

// ============================================================================
// Client -> Core
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClientMessage {
    // -- Conversation --------------------------------------------------
    /// Submit a line of user input. Handled by the session, which may
    /// resolve it locally (`/help`) or forward it to the agent.
    Prompt {
        text: String,
    },

    /// Answer a pending tool confirmation.
    Confirm {
        allowed: bool,
    },

    /// Cancel the request in flight.
    Cancel,

    /// Drop everything after transcript index `index` so the user can
    /// re-send a corrected version of that turn.
    Rollback {
        index: usize,
    },

    /// Expand or collapse a transcript entry.
    ToggleExpanded {
        index: usize,
    },

    // -- Introspection -------------------------------------------------
    /// Ask for the full current session state. The reply is a [`ServerEvent::Session`].
    GetSession,

    // -- Setup ---------------------------------------------------------
    GetSetupConfig {
        request_id: String,
    },

    SaveSetupConfig {
        request_id: String,
        config: Config,
    },

    GetProviders {
        request_id: String,
    },

    GetModels {
        request_id: String,
        provider: SupportedProvider,
        api_key: Option<String>,
        endpoint: Option<String>,
    },

    TestProvider {
        request_id: String,
        provider: SupportedProvider,
        api_key: Option<String>,
        endpoint: Option<String>,
        model: String,
    },

    /// Switch the active agent model without restarting the core.
    SetModel {
        request_id: String,
        model: String,
    },

    /// Change the active model for this session without saving global config.
    SetSessionModel {
        request_id: String,
        model: crate::config::ModelConfig,
    },

    /// List archived conversations without loading the agent model.
    HistoryList {
        request_id: String,
    },

    /// Load one archived conversation by its stable id.
    HistoryShow {
        request_id: String,
        id: String,
    },

    /// Delete one archived conversation.
    HistoryDelete {
        request_id: String,
        id: String,
    },

    /// Permanently remove all saved conversation archives.
    HistoryPrune {
        request_id: String,
    },

    /// Save or replace the summary attached to an archived conversation.
    SummaryAdd {
        request_id: String,
        id: String,
        summary: String,
    },

    /// Start the browser dashboard within this core process.
    StartDashboard {
        request_id: String,
        host: String,
        port: u16,
    },

    // -- Persistent runtime ---------------------------------------------
    AgentsList {
        request_id: String,
    },
    AgentCreate {
        request_id: String,
        name: String,
        description: Option<String>,
        kind: crate::runtime::AgentKind,
        configuration: crate::runtime::AgentConfiguration,
    },
    AgentDelete {
        request_id: String,
        id: String,
    },
    AgentUpdate {
        request_id: String,
        id: String,
        name: Option<String>,
        description: Option<String>,
        configuration: Option<crate::runtime::AgentConfiguration>,
    },
    AgentSetStatus {
        request_id: String,
        id: String,
        status: crate::runtime::AgentStatus,
    },
    TasksList {
        request_id: String,
        agent_id: Option<String>,
        limit: Option<usize>,
    },
    TaskCreate {
        request_id: String,
        agent_id: String,
        request: String,
        title: Option<String>,
        workspace: Option<String>,
        recover_on_restart: bool,
    },
    TaskCancel {
        request_id: String,
        id: String,
    },
    TaskRetry {
        request_id: String,
        id: String,
    },
    ApprovalsList {
        request_id: String,
    },
    ApprovalResolve {
        request_id: String,
        id: String,
        allowed: bool,
    },
    MemoryList {
        request_id: String,
        scope: crate::runtime::MemoryScope,
        scope_id: String,
        query: Option<String>,
        limit: Option<usize>,
    },
    MemoryAdd {
        request_id: String,
        scope: crate::runtime::MemoryScope,
        scope_id: String,
        content: String,
        source: Option<String>,
    },
    MemoryDelete {
        request_id: String,
        id: String,
    },
    SchedulesList {
        request_id: String,
        agent_id: Option<String>,
    },
    ScheduleCreate {
        request_id: String,
        agent_id: String,
        title: String,
        prompt: String,
        run_at: Option<i64>,
        every_seconds: Option<u64>,
    },
    ScheduleDelete {
        request_id: String,
        id: String,
    },
    EventsSince {
        request_id: String,
        sequence: i64,
        limit: Option<usize>,
        agent_id: Option<String>,
    },
    RuntimeCapabilities {
        request_id: String,
    },
}

impl ClientMessage {
    /// The `request_id` this message expects a response for, if any.
    ///
    /// Fire-and-forget conversation messages have none.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::GetSession
            | Self::Prompt { .. }
            | Self::Confirm { .. }
            | Self::Cancel
            | Self::Rollback { .. }
            | Self::ToggleExpanded { .. } => None,

            Self::GetSetupConfig { request_id }
            | Self::SaveSetupConfig { request_id, .. }
            | Self::GetProviders { request_id }
            | Self::GetModels { request_id, .. }
            | Self::TestProvider { request_id, .. }
            | Self::SetModel { request_id, .. }
            | Self::SetSessionModel { request_id, .. }
            | Self::HistoryList { request_id }
            | Self::HistoryShow { request_id, .. }
            | Self::HistoryDelete { request_id, .. }
            | Self::HistoryPrune { request_id }
            | Self::SummaryAdd { request_id, .. }
            | Self::StartDashboard { request_id, .. }
            | Self::AgentsList { request_id }
            | Self::AgentCreate { request_id, .. }
            | Self::AgentDelete { request_id, .. }
            | Self::AgentUpdate { request_id, .. }
            | Self::AgentSetStatus { request_id, .. }
            | Self::TasksList { request_id, .. }
            | Self::TaskCreate { request_id, .. }
            | Self::TaskCancel { request_id, .. }
            | Self::TaskRetry { request_id, .. }
            | Self::ApprovalsList { request_id }
            | Self::ApprovalResolve { request_id, .. }
            | Self::MemoryList { request_id, .. }
            | Self::MemoryAdd { request_id, .. }
            | Self::MemoryDelete { request_id, .. }
            | Self::SchedulesList { request_id, .. }
            | Self::ScheduleCreate { request_id, .. }
            | Self::ScheduleDelete { request_id, .. }
            | Self::EventsSince { request_id, .. }
            | Self::RuntimeCapabilities { request_id } => Some(request_id),
        }
    }
}

impl From<AgentCommand> for ClientMessage {
    fn from(command: AgentCommand) -> Self {
        match command {
            AgentCommand::Submit(text) => Self::Prompt { text },
            AgentCommand::Confirm { allowed } => Self::Confirm { allowed },
            AgentCommand::Cancel => Self::Cancel,
            AgentCommand::Rollback { index } => Self::Rollback { index },
            AgentCommand::ToggleExpanded { index } => Self::ToggleExpanded { index },
        }
    }
}

impl ClientMessage {
    /// The session command this message carries, or `None` for setup and
    /// introspection messages the session does not handle.
    pub fn into_command(self) -> Option<AgentCommand> {
        match self {
            Self::Prompt { text } => Some(AgentCommand::Submit(text)),
            Self::Confirm { allowed } => Some(AgentCommand::Confirm { allowed }),
            Self::Cancel => Some(AgentCommand::Cancel),
            Self::Rollback { index } => Some(AgentCommand::Rollback { index }),
            Self::ToggleExpanded { index } => Some(AgentCommand::ToggleExpanded { index }),

            Self::GetSession
            | Self::GetSetupConfig { .. }
            | Self::SaveSetupConfig { .. }
            | Self::GetProviders { .. }
            | Self::GetModels { .. }
            | Self::TestProvider { .. }
            | Self::SetModel { .. }
            | Self::SetSessionModel { .. }
            | Self::HistoryList { .. }
            | Self::HistoryShow { .. }
            | Self::HistoryDelete { .. }
            | Self::HistoryPrune { .. }
            | Self::SummaryAdd { .. }
            | Self::StartDashboard { .. }
            | Self::AgentsList { .. }
            | Self::AgentCreate { .. }
            | Self::AgentDelete { .. }
            | Self::AgentUpdate { .. }
            | Self::AgentSetStatus { .. }
            | Self::TasksList { .. }
            | Self::TaskCreate { .. }
            | Self::TaskCancel { .. }
            | Self::TaskRetry { .. }
            | Self::ApprovalsList { .. }
            | Self::ApprovalResolve { .. }
            | Self::MemoryList { .. }
            | Self::MemoryAdd { .. }
            | Self::MemoryDelete { .. }
            | Self::SchedulesList { .. }
            | Self::ScheduleCreate { .. }
            | Self::ScheduleDelete { .. }
            | Self::EventsSince { .. }
            | Self::RuntimeCapabilities { .. } => None,
        }
    }
}

// ============================================================================
// Core -> Client
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Handshake, sent once the core is ready to accept prompts.
    Ready {
        /// Protocol major version. Additive runtime operations are discovered
        /// through RuntimeCapabilities and do not require a major bump.
        version: u32,
        /// Optional capabilities keep older v3 clients compatible while
        /// allowing newer clients to discover additive runtime support.
        #[serde(default)]
        features: Vec<String>,
    },

    /// The transcript advanced. Clients that keep their own copy of the
    /// state fold this in; clients that render from a snapshot can ignore it.
    Agent {
        event: AgentEvent,
    },

    /// Full session state, in reply to `GetSession`.
    Session {
        session: crate::session::SessionView,
    },

    // -- Setup ---------------------------------------------------------
    Error {
        request_id: Option<String>,
        message: String,
        category: ErrorCategory,
    },

    SetupConfig {
        request_id: String,
        config: Config,
    },

    SetupSaved {
        request_id: String,
    },

    Providers {
        request_id: String,
        providers: Vec<SupportedProvider>,
    },

    Models {
        request_id: String,
        models: Vec<SupportedModel>,
    },

    ProviderTested {
        request_id: String,
    },

    ModelChanged {
        request_id: String,
        provider: String,
        model: String,
    },

    HistoryList {
        request_id: String,
        chats: Vec<ChatSummary>,
    },
    HistoryChat {
        request_id: String,
        chat: ChatArchive,
    },
    HistoryDeleted {
        request_id: String,
        id: String,
        deleted: bool,
    },
    HistoryPruned {
        request_id: String,
        removed: usize,
    },
    SummarySaved {
        request_id: String,
        id: String,
    },
    DashboardStarted {
        request_id: String,
        address: String,
    },
    Agents {
        request_id: String,
        agents: Vec<crate::runtime::AgentRecord>,
    },
    AgentSaved {
        request_id: String,
        agent: crate::runtime::AgentRecord,
    },
    AgentDeleted {
        request_id: String,
        id: String,
        deleted: bool,
    },
    Tasks {
        request_id: String,
        tasks: Vec<crate::runtime::TaskRecord>,
    },
    TaskSaved {
        request_id: String,
        task: crate::runtime::TaskRecord,
    },
    Approvals {
        request_id: String,
        approvals: Vec<crate::runtime::ApprovalRecord>,
    },
    ApprovalResolved {
        request_id: String,
        approval: crate::runtime::ApprovalRecord,
    },
    Memory {
        request_id: String,
        entries: Vec<crate::runtime::MemoryEntry>,
    },
    MemorySaved {
        request_id: String,
        entry: crate::runtime::MemoryEntry,
    },
    MemoryDeleted {
        request_id: String,
        deleted: bool,
    },
    Schedules {
        request_id: String,
        schedules: Vec<crate::runtime::ScheduleRecord>,
    },
    ScheduleSaved {
        request_id: String,
        schedule: crate::runtime::ScheduleRecord,
    },
    ScheduleDeleted {
        request_id: String,
        deleted: bool,
    },
    RuntimeEvents {
        request_id: String,
        events: Vec<crate::runtime::RuntimeEvent>,
    },
    RuntimeCapabilities {
        request_id: String,
        features: Vec<String>,
        storage_schema: u32,
    },
    RuntimeNotice {
        event: crate::runtime::RuntimeEvent,
    },
}

pub const PROTOCOL_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Worth retrying as-is: a network blip, a rate limit.
    Transient,

    /// The request will never succeed in its current form.
    Fatal,

    /// The core could not start at all.
    InitializationFailed,
}

// ============================================================================
// Framing
// ============================================================================

/// Serialize one message as a single line of JSON, newline-terminated.
pub fn encode(message: &impl Serialize) -> serde_json::Result<String> {
    let mut line = serde_json::to_string(message)?;

    line.push('\n');

    Ok(line)
}

/// Parse one line into a [`ClientMessage`].
pub fn decode(line: &str) -> Result<ClientMessage, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentCommand;

    #[test]
    fn every_agent_command_has_a_wire_form() {
        let commands = [
            AgentCommand::Submit("hi".into()),
            AgentCommand::Confirm { allowed: true },
            AgentCommand::Cancel,
            AgentCommand::Rollback { index: 2 },
            AgentCommand::ToggleExpanded { index: 0 },
        ];

        for command in commands {
            let message = ClientMessage::from(command.clone());
            let line = encode(&message).unwrap();

            assert!(!line.trim_end().contains('\n'), "must encode to one line");

            let decoded: ClientMessage = decode(line.trim_end()).unwrap();

            assert_eq!(decoded.into_command(), Some(command));
        }
    }

    #[test]
    fn request_ids_survive_a_round_trip() {
        let message = ClientMessage::GetProviders {
            request_id: "abc-123".into(),
        };

        let line = encode(&message).unwrap();
        assert!(line.contains(r#""type":"get_providers""#), "got {line}");
        let decoded: ClientMessage = decode(line.trim_end()).unwrap();

        assert_eq!(decoded.request_id(), Some("abc-123"));
    }

    #[test]
    fn conversation_messages_carry_no_request_id() {
        assert!(ClientMessage::Cancel.request_id().is_none());
        assert!(ClientMessage::GetSession.request_id().is_none());
    }

    #[test]
    fn model_switch_requests_keep_the_request_id() {
        let message = ClientMessage::SetModel {
            request_id: "switch-1".into(),
            model: "gpt-4.1".into(),
        };
        let line = encode(&message).unwrap();
        let decoded = decode(line.trim_end()).unwrap();

        assert_eq!(decoded.request_id(), Some("switch-1"));
        assert!(matches!(decoded, ClientMessage::SetModel { model, .. } if model == "gpt-4.1"));
    }

    #[test]
    fn session_model_switch_carries_full_config_without_losing_request_id() {
        let message = ClientMessage::SetSessionModel {
            request_id: "session-switch-1".into(),
            model: crate::config::ModelConfig {
                provider: "openai".into(),
                endpoint: "https://api.openai.com/v1".into(),
                name: "gpt-4.1".into(),
                api_key: Some("secret".into()),
            },
        };
        let line = encode(&message).unwrap();
        assert!(line.contains(r#""type":"set_session_model""#));
        let decoded = decode(line.trim_end()).unwrap();
        assert_eq!(decoded.request_id(), Some("session-switch-1"));
        assert!(
            matches!(decoded, ClientMessage::SetSessionModel { model, .. } if model.name == "gpt-4.1")
        );
    }

    #[test]
    fn archive_and_dashboard_requests_keep_their_ids() {
        let requests = [
            ClientMessage::HistoryList {
                request_id: "history-list".into(),
            },
            ClientMessage::HistoryShow {
                request_id: "history-show".into(),
                id: "chat-1".into(),
            },
            ClientMessage::HistoryDelete {
                request_id: "history-delete".into(),
                id: "chat-1".into(),
            },
            ClientMessage::HistoryPrune {
                request_id: "history-prune".into(),
            },
            ClientMessage::SummaryAdd {
                request_id: "summary-add".into(),
                id: "chat-1".into(),
                summary: "summary".into(),
            },
            ClientMessage::StartDashboard {
                request_id: "dashboard".into(),
                host: "127.0.0.1".into(),
                port: 3000,
            },
        ];

        for request in requests {
            let expected = request.request_id().unwrap().to_owned();
            let encoded = encode(&request).unwrap();
            let decoded = decode(encoded.trim_end()).unwrap();
            assert_eq!(decoded.request_id(), Some(expected.as_str()));
        }
    }

    #[test]
    fn agent_delete_requests_keep_their_request_id() {
        let message = ClientMessage::AgentDelete {
            request_id: "delete-agent-1".into(),
            id: "agent_123".into(),
        };
        let decoded = decode(encode(&message).unwrap().trim_end()).unwrap();
        assert_eq!(decoded.request_id(), Some("delete-agent-1"));
        assert!(matches!(decoded, ClientMessage::AgentDelete { id, .. } if id == "agent_123"));
    }

    #[test]
    fn server_events_use_snake_case_tags() {
        let line = encode(&ServerEvent::Ready {
            version: PROTOCOL_VERSION,
            features: vec!["persistent_agents".into()],
        })
        .unwrap();

        assert!(line.contains(r#""type":"ready""#), "got {line}");
    }
}
