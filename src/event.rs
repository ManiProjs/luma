#[derive(Debug, Clone)]
pub enum AgentEvent {
    Thinking,

    TextDelta(String),

    ToolStarted { name: String },

    ToolFinished { name: String, result: String },

    Finished,

    Error(String),
}
