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
            | Self::TestProvider { request_id, .. } => Some(request_id),
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
            | Self::TestProvider { .. } => None,
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
        /// Protocol version. A client that does not recognise this should
        /// refuse to start rather than guess.
        version: u32,
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
}

pub const PROTOCOL_VERSION: u32 = 1;

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
        let decoded: ClientMessage = decode(line.trim_end()).unwrap();

        assert_eq!(decoded.request_id(), Some("abc-123"));
    }

    #[test]
    fn conversation_messages_carry_no_request_id() {
        assert!(ClientMessage::Cancel.request_id().is_none());
        assert!(ClientMessage::GetSession.request_id().is_none());
    }

    #[test]
    fn server_events_use_snake_case_tags() {
        let line = encode(&ServerEvent::Ready {
            version: PROTOCOL_VERSION,
        })
        .unwrap();

        assert!(line.contains(r#""type":"ready""#), "got {line}");
    }
}
