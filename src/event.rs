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

    /// Proposed file contents, emitted before the corresponding approval request.
    ChangePreview {
        path: String,
        diff: String,
    },

    ToolStarted {
        name: String,
        input: String,
    },

    ToolFinished {
        name: String,
        duration_ms: u128,
        failed: bool,
        #[serde(default)]
        diff: Option<String>,
    },

    ConfirmationRequired {
        name: String,
        input: String,
    },

    /// Ask the user for information before the agent continues its ReAct loop.
    QuestionRequired {
        question_id: String,
        prompt: String,
        #[serde(default)]
        choices: Vec<String>,
    },

    SystemMessage(String),

    Error(String),

    /// The active request was cancelled by the user.
    Cancelled,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Doing,
    Talking,
    Waiting,
    Error,
}

#[cfg(test)]
mod tests {
    use super::AgentEvent;

    #[test]
    fn tool_finished_serializes_the_actual_patch_diff() {
        let event = AgentEvent::ToolFinished {
            name: "patch_file".into(),
            duration_ms: 8,
            failed: false,
            diff: Some("--- a/file.rs\n+++ b/file.rs\n-old\n+new".into()),
        };

        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["type"], "ToolFinished");
        assert_eq!(
            value["data"]["diff"],
            "--- a/file.rs\n+++ b/file.rs\n-old\n+new"
        );
    }
}
