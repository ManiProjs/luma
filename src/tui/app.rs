use crate::event::AgentEvent;

#[derive(Debug)]
pub struct App {
    pub messages: Vec<MessageLine>,
    pub input: TextBuffer,

    pub welcome_visible: bool,
    pub thinking: bool,
    pub current_tool: Option<ToolState>,

    pub scroll: usize,
    pub auto_scroll: bool,
    pub running: bool,

    pub logo_frame: usize,

    // Input history
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,

    // Slash command autocomplete
    pub suggestions: Vec<String>,
    pub selected_suggestion: usize,
}

#[derive(Debug)]
pub struct ToolState {
    pub name: String,
    pub input: String,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Success,
    Failed,
}

impl ToolStatus {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Running => "◇",
            Self::Success => "✓",
            Self::Failed => "✗",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Success => "DONE",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Debug)]
pub struct MessageLine {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Default)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub cursor_x: usize,
    pub cursor_y: usize,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    pub fn insert(&mut self, c: char) {
        if self.cursor_y >= self.lines.len() {
            self.lines.push(String::new());
            self.cursor_y = self.lines.len() - 1;
        }

        self.lines[self.cursor_y].insert(self.cursor_x, c);
        self.cursor_x += 1;
    }

    pub fn newline(&mut self) {
        let rest = self.lines[self.cursor_y].split_off(self.cursor_x);

        self.lines.insert(self.cursor_y + 1, rest);

        self.cursor_y += 1;
        self.cursor_x = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
            self.lines[self.cursor_y].remove(self.cursor_x);
            return;
        }

        if self.cursor_y > 0 {
            let current = self.lines.remove(self.cursor_y);

            self.cursor_y -= 1;
            self.cursor_x = self.lines[self.cursor_y].len();

            self.lines[self.cursor_y].push_str(&current);
        }
    }

    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.lines.push(String::new());

        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub fn set_content(&mut self, text: String) {
        self.lines = text.lines().map(str::to_string).collect();

        if self.lines.is_empty() {
            self.lines.push(String::new());
        }

        self.cursor_y = self.lines.len() - 1;
        self.cursor_x = self.lines[self.cursor_y].len();
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),

            input: TextBuffer::new(),

            welcome_visible: true,

            thinking: false,

            current_tool: None,

            scroll: 0,

            auto_scroll: true,

            running: true,

            logo_frame: 0,

            input_history: Vec::new(),

            history_index: None,

            suggestions: Vec::new(),

            selected_suggestion: 0,
        }
    }

    // ─────────────────────────────────────────────
    // Input / autocomplete
    // ─────────────────────────────────────────────

    pub fn update_suggestions(&mut self) {
        let current = self
            .input
            .lines
            .get(self.input.cursor_y)
            .cloned()
            .unwrap_or_default();

        self.suggestions = crate::commands::suggestions(&current);
        self.selected_suggestion = 0;
    }

    pub fn accept_suggestion(&mut self) {
        if let Some(command) = self.suggestions.get(self.selected_suggestion).cloned() {
            self.input.set_content(command);
        }

        self.suggestions.clear();
    }

    pub fn suggestion_up(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.selected_suggestion = self.selected_suggestion.saturating_sub(1);
    }

    pub fn suggestion_down(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        self.selected_suggestion = (self.selected_suggestion + 1) % self.suggestions.len();
    }

    // ─────────────────────────────────────────────
    // History
    // ─────────────────────────────────────────────

    pub fn add_history(&mut self, message: String) {
        if message.trim().is_empty() {
            return;
        }

        if self.input_history.last() != Some(&message) {
            self.input_history.push(message);
        }

        self.history_index = None;
    }

    pub fn history_up(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        let index = match self.history_index {
            Some(index) if index > 0 => index - 1,
            Some(_) => 0,
            None => self.input_history.len() - 1,
        };

        self.history_index = Some(index);

        self.input.set_content(self.input_history[index].clone());
    }

    pub fn history_down(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };

        if index + 1 >= self.input_history.len() {
            self.history_index = None;
            self.input.clear();
            return;
        }

        let index = index + 1;

        self.history_index = Some(index);

        self.input.set_content(self.input_history[index].clone());
    }

    // ─────────────────────────────────────────────
    // Sending messages
    // ─────────────────────────────────────────────

    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.content();

        if text.trim().is_empty() {
            return None;
        }

        self.messages.push(MessageLine {
            role: MessageRole::User,
            content: text.clone(),
        });

        self.add_history(text.clone());

        self.input.clear();

        self.suggestions.clear();

        self.welcome_visible = false;

        self.auto_scroll = true;

        Some(text)
    }

    // ─────────────────────────────────────────────
    // Agent events
    // ─────────────────────────────────────────────

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking => {
                self.thinking = true;
            }

            AgentEvent::Planning => {
                self.thinking = true;
            }

            AgentEvent::ToolStarted { name, input } => {
                self.thinking = false;

                let display_input = Self::format_tool_input(&name, &input);

                self.current_tool = Some(ToolState {
                    name,
                    input: display_input,
                    status: ToolStatus::Running,
                });
            }

            AgentEvent::ToolFinished { name, duration_ms } => {
                if let Some(tool) = self.current_tool.as_mut() {
                    if tool.name == name {
                        tool.status = ToolStatus::Success;
                    }
                }

                // Keep a compact activity record rather than polluting
                // the normal assistant conversation.
                self.messages.push(MessageLine {
                    role: MessageRole::System,
                    content: format!("✓ {} completed in {}ms", name, duration_ms),
                });

                self.current_tool = None;

                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }

            AgentEvent::TextDelta(text) => {
                self.thinking = false;

                if let Some(last) = self.messages.last_mut() {
                    if matches!(&last.role, MessageRole::Assistant) {
                        last.content.push_str(&text);
                        return;
                    }
                }

                self.messages.push(MessageLine {
                    role: MessageRole::Assistant,
                    content: text,
                });
            }

            AgentEvent::SystemMessage(text) => {
                self.messages.push(MessageLine {
                    role: MessageRole::System,
                    content: text,
                });

                self.thinking = false;
                self.current_tool = None;
            }

            AgentEvent::Finished => {
                self.thinking = false;
                self.current_tool = None;
            }

            AgentEvent::Error(error) => {
                self.messages.push(MessageLine {
                    role: MessageRole::System,
                    content: error,
                });

                self.thinking = false;

                if let Some(tool) = self.current_tool.as_mut() {
                    tool.status = ToolStatus::Failed;
                }

                self.current_tool = None;
            }

            AgentEvent::Debug(text) => {
                self.messages.push(MessageLine {
                    role: MessageRole::System,
                    content: text,
                });
            }
        }
    }

    fn format_tool_input(name: &str, input: &str) -> String {
        match name {
            "write_file" => input.lines().next().unwrap_or("?").to_string(),

            "read_file" => input.trim().to_string(),

            "list_directory" => ".".to_string(),

            "search_files" => input.trim().to_string(),

            "run_command" => input.trim().to_string(),

            _ => {
                let trimmed = input.trim();

                if trimmed.is_empty() {
                    "working".into()
                } else {
                    trimmed.to_string()
                }
            }
        }
    }

    // ─────────────────────────────────────────────
    // Scrolling
    // ─────────────────────────────────────────────

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        self.auto_scroll = false;
        self.scroll = self.scroll.saturating_add(3);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
    }

    // ─────────────────────────────────────────────
    // System messages
    // ─────────────────────────────────────────────

    pub fn add_system_message(&mut self, text: impl Into<String>) {
        self.messages.push(MessageLine {
            role: MessageRole::System,
            content: text.into(),
        });

        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }
}
