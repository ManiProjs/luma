#[derive(Debug, Clone)]
pub enum AgentEvent {
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
}
