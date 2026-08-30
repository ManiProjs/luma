#[derive(Debug, Clone)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    // Tool,
    Observation,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Default)]
pub struct Context {
    messages: Vec<Message>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn add(&mut self, role: MessageRole, content: impl Into<String>) {
        self.messages.push(Message {
            role,
            content: content.into(),
        });
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
}
