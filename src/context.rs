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

const MAX_CONTEXT_MESSAGES: usize = 80;
const MAX_CONTEXT_CHARS: usize = 64_000;
const MAX_SYSTEM_CONTEXT_CHARS: usize = MAX_CONTEXT_CHARS / 2;
const MAX_MESSAGE_CHARS: usize = 24_000;
const COMPACTION_MARKER: &str =
    "Earlier conversation was compacted to keep the active context bounded.";
const SYSTEM_TRUNCATION_MARKER: &str = "\n[system context truncated by Luma]";

impl Context {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn add(&mut self, role: MessageRole, content: impl Into<String>) {
        let content = bounded_message(&content.into());
        if role == MessageRole::System {
            let used = self
                .messages
                .iter()
                .filter(|message| message.role == MessageRole::System)
                .map(|message| message.content.chars().count())
                .sum::<usize>();
            let remaining = MAX_SYSTEM_CONTEXT_CHARS.saturating_sub(used);
            if remaining == 0 {
                return;
            }
            if let Some(system) = self
                .messages
                .iter_mut()
                .find(|message| message.role == MessageRole::System)
            {
                if remaining <= 2 {
                    return;
                }
                system.content.push_str("\n\n");
                system
                    .content
                    .push_str(&bounded_system_message(&content, remaining - 2));
                self.compact();
                return;
            }
            self.messages.push(Message {
                role,
                content: bounded_system_message(&content, remaining),
            });
            self.compact();
            return;
        }
        self.messages.push(Message { role, content });
        self.compact();
    }

    /// Replace conversation history while retaining every live system instruction.
    pub fn restore(&mut self, messages: impl IntoIterator<Item = Message>) {
        let system_prompts = self
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::System)
            .cloned()
            .collect::<Vec<_>>();
        self.messages.clear();
        self.messages.extend(system_prompts);
        self.messages.extend(
            messages
                .into_iter()
                .filter(|message| message.role != MessageRole::System)
                .map(|message| Message {
                    role: message.role,
                    content: bounded_message(&message.content),
                }),
        );
        self.compact();
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn usage(&self) -> (usize, usize, usize, bool) {
        (
            self.messages.len(),
            self.total_chars(),
            MAX_CONTEXT_CHARS,
            self.messages
                .iter()
                .any(|message| message.content == COMPACTION_MARKER),
        )
    }

    /// Retain every system instruction and the newest conversational messages.
    /// The marker tells the model that older turns existed but were omitted.
    fn compact(&mut self) {
        let marker_chars = COMPACTION_MARKER.len();
        let already_compacted = self
            .messages
            .iter()
            .any(|message| message.content == COMPACTION_MARKER);
        let mut compacted = already_compacted;
        while self.messages.len() > MAX_CONTEXT_MESSAGES || self.total_chars() > MAX_CONTEXT_CHARS {
            let Some(index) = self.messages.iter().position(|message| {
                message.role != MessageRole::System && message.content != COMPACTION_MARKER
            }) else {
                break;
            };
            self.messages.remove(index);
            compacted = true;
        }
        if compacted
            && !self
                .messages
                .iter()
                .any(|message| message.content == COMPACTION_MARKER)
        {
            let insertion = self
                .messages
                .iter()
                .rposition(|message| message.role == MessageRole::System)
                .map_or(0, |index| index + 1);
            self.messages.insert(
                insertion,
                Message {
                    role: MessageRole::Observation,
                    content: COMPACTION_MARKER.into(),
                },
            );
        }
        // Reserve room for the marker while preserving the newest actual turn.
        if compacted && !already_compacted {
            while self.messages.len() >= MAX_CONTEXT_MESSAGES
                || self.total_chars() + marker_chars > MAX_CONTEXT_CHARS
            {
                let Some(index) = self.messages.iter().position(|message| {
                    message.role != MessageRole::System && message.content != COMPACTION_MARKER
                }) else {
                    break;
                };
                self.messages.remove(index);
            }
        }
        // The marker itself must not push the context over the configured limit.
        while self.messages.len() > MAX_CONTEXT_MESSAGES || self.total_chars() > MAX_CONTEXT_CHARS {
            let Some(index) = self.messages.iter().position(|message| {
                message.role != MessageRole::System && message.content != COMPACTION_MARKER
            }) else {
                break;
            };
            self.messages.remove(index);
        }
    }

    fn total_chars(&self) -> usize {
        self.messages
            .iter()
            .map(|message| message.content.chars().count())
            .sum()
    }
}

fn bounded_message(content: &str) -> String {
    let mut chars = content.chars();
    let output: String = chars.by_ref().take(MAX_MESSAGE_CHARS).collect();
    if chars.next().is_some() {
        format!("{output}\n[message truncated by Luma]")
    } else {
        output
    }
}

fn bounded_system_message(content: &str, max_chars: usize) -> String {
    let mut chars = content.chars();
    let output: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let marker_len = SYSTEM_TRUNCATION_MARKER.chars().count();
        if max_chars > marker_len {
            let mut bounded: String = output.chars().take(max_chars - marker_len).collect();
            bounded.push_str(SYSTEM_TRUNCATION_MARKER);
            bounded
        } else {
            output
        }
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMPACTION_MARKER, Context, MAX_CONTEXT_CHARS, MAX_CONTEXT_MESSAGES, MAX_MESSAGE_CHARS,
        MAX_SYSTEM_CONTEXT_CHARS, MessageRole, SYSTEM_TRUNCATION_MARKER,
    };

    #[test]
    fn restoring_history_keeps_all_live_system_instructions() {
        let mut context = Context::new();
        context.add(MessageRole::System, "workspace memory");
        context.add(MessageRole::System, "specialist role");
        context.add(MessageRole::User, "old active chat");
        context.restore([super::Message {
            role: MessageRole::User,
            content: "archived chat".into(),
        }]);

        let messages = context.messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].content.contains("workspace memory"));
        assert!(messages[0].content.contains("specialist role"));
        assert_eq!(messages[1].content, "archived chat");
    }

    #[test]
    fn system_context_has_a_separate_aggregate_budget() {
        let mut context = Context::new();
        for index in 0..8 {
            context.add(
                MessageRole::System,
                format!("{index}:{}", "x".repeat(20_000)),
            );
        }
        let system_chars = context
            .messages()
            .iter()
            .filter(|message| message.role == MessageRole::System)
            .map(|message| message.content.chars().count())
            .sum::<usize>();
        assert!(system_chars <= MAX_SYSTEM_CONTEXT_CHARS);
        assert!(context.usage().1 <= MAX_CONTEXT_CHARS);
        assert_eq!(
            context
                .messages()
                .iter()
                .filter(|message| message.role == MessageRole::System)
                .count(),
            1
        );
        assert!(
            context
                .messages()
                .iter()
                .any(|message| { message.content.ends_with(SYSTEM_TRUNCATION_MARKER) })
        );
    }

    #[test]
    fn compaction_keeps_system_prompts_and_newest_messages() {
        let mut context = Context::new();
        context.add(MessageRole::System, "workspace memory");
        for index in 0..(MAX_CONTEXT_MESSAGES + 10) {
            context.add(MessageRole::User, format!("turn {index}"));
        }

        let messages = context.messages();
        assert!(
            messages
                .iter()
                .any(|message| message.content == "workspace memory")
        );
        assert!(
            messages
                .iter()
                .any(|message| message.content == COMPACTION_MARKER)
        );
        assert!(messages.iter().any(|message| message.content == "turn 89"));
        assert!(!messages.iter().any(|message| message.content == "turn 0"));
        assert!(messages.len() <= MAX_CONTEXT_MESSAGES);
        assert!(context.total_chars() <= MAX_CONTEXT_CHARS);
    }

    #[test]
    fn individual_messages_are_truncated_without_splitting_utf8() {
        let mut context = Context::new();
        context.add(MessageRole::User, "界".repeat(MAX_MESSAGE_CHARS + 4));
        assert!(
            context.messages()[0]
                .content
                .starts_with(&"界".repeat(MAX_MESSAGE_CHARS))
        );
        assert!(
            context.messages()[0]
                .content
                .ends_with("[message truncated by Luma]")
        );
    }
}
