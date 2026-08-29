#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),

    Thinking,

    Planning,

    PlanGenerated(String),

    ToolStarted { name: String, input: String },

    ToolFinished { name: String, duration_ms: u128 },

    ConfirmationRequired { name: String, input: String },

    SystemMessage(String),

    Error(String),

    Finished,

    Debug(String),
}
