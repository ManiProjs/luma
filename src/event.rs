#[derive(Debug)]
pub enum AgentEvent {
    Thinking,

    Planning,

    ToolStarted { name: String, input: String },

    ToolFinished { name: String, duration_ms: u128 },

    TextDelta(String),

    SystemMessage(String),

    Finished,

    Error(String),

    Debug(String),
}
