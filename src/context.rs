#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Replace conversation history while retaining the workspace system prompt.
    pub fn restore(&mut self, messages: impl IntoIterator<Item = Message>) {
        let workspace_prompt = self
            .messages
            .iter()
            .find(|message| message.role == MessageRole::System)
            .cloned();
        self.messages.clear();
        if let Some(prompt) = workspace_prompt {
            self.messages.push(prompt);
        }
        self.messages.extend(messages);
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, MessageRole};

    #[test]
    fn restoring_history_keeps_only_the_live_workspace_prompt() {
        let mut context = Context::new();
        context.add(MessageRole::System, "workspace memory");
        context.add(MessageRole::User, "old active chat");
        context.restore([super::Message {
            role: MessageRole::User,
            content: "archived chat".into(),
        }]);

        let messages = context.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "workspace memory");
        assert_eq!(messages[1].content, "archived chat");
    }
}
