use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AgentEvent {
    Status {
        state: AgentStatus,
    },

    TextDelta(String),

    Thinking,

    PlanGenerated(String),

    ToolStarted {
        name: String,
        input: String,
    },

    ToolFinished {
        name: String,
        duration_ms: u128,
        failed: bool,
    },

    ConfirmationRequired {
        name: String,
        input: String,
    },

    SystemMessage(String),

    Error(String),

    Usage {
        prompt_tokens: u64,
        completion_tokens: u64,
        total_tokens: u64,
        cost_usd: f64,
    },

    Finished,

    /// Detailed tool progress with full output (for Ink retrieval)
    ToolProgress {
        name: String,
        output: String,
        progress_percent: Option<u8>,
    },

    /// Session metadata for UI header
    SessionInfo {
        workspace: String,
        model: String,
        provider: String,
        tool_count: usize,
        messages_count: usize,
    },

    /// Thinking content shown in expanded state
    ThinkingContent(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Doing,
    Talking,
    Waiting,
    Error,
}
