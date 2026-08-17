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
}

#[derive(Debug)]
pub struct ToolState {
    pub name: String,
    pub input: String,
}

#[derive(Debug)]
pub struct MessageLine {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug)]
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

            logo_frame: 0,

            running: true,
        }
    }

    pub fn submit_input(&mut self) -> Option<String> {
        let text = self.input.content();

        if text.trim().is_empty() {
            return None;
        }

        self.messages.push(MessageLine {
            role: MessageRole::User,

            content: text.clone(),
        });

        self.input.clear();

        self.welcome_visible = false;

        Some(text)
    }

    pub fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking => {
                self.thinking = true;
            }

            AgentEvent::Planning => {
                self.thinking = true;
            }

            AgentEvent::ToolStarted { name, input } => {
                self.current_tool = Some(ToolState { name, input });
            }

            AgentEvent::ToolFinished { name, duration_ms } => {
                self.messages.push(MessageLine {
                    role: MessageRole::Tool,

                    content: format!("{} finished ({}ms)", name, duration_ms),
                });

                self.current_tool = None;
            }

            AgentEvent::TextDelta(text) => {
                self.auto_scroll = true;

                self.thinking = false;

                if let Some(last) = self.messages.last_mut() {
                    if matches!(last.role, MessageRole::Assistant) {
                        last.content.push_str(&text);

                        return;
                    }
                }

                self.messages.push(MessageLine {
                    role: MessageRole::Assistant,

                    content: text,
                });

                if self.auto_scroll {
                    self.scroll_to_bottom();
                }
            }

            AgentEvent::Finished => {
                self.thinking = false;
            }

            AgentEvent::Error(error) => {
                self.messages.push(MessageLine {
                    role: MessageRole::System,

                    content: error,
                });
            }
        }
    }

    pub fn scroll_down(&mut self) {
        self.auto_scroll = false;

        self.scroll = self.scroll.saturating_add(3);
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;

        self.scroll = self.scroll.saturating_sub(3);
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.messages.len() > 10 {
            self.scroll = self.messages.len() - 10;
        } else {
            self.scroll = 0;
        }
    }
}
