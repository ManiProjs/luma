#[derive(Debug)]
pub struct App {
    pub messages: Vec<MessageLine>,
    pub input: TextBuffer,

    pub welcome_visible: bool,
    pub thinking: bool,
    pub current_tool: Option<ToolState>,

    pub more_mode: bool,

    pub status: String,

    pub scroll: usize,
    pub auto_scroll: bool,

    pub logo_frame: usize,

    // Input history
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,

    // Slash command autocomplete
    pub suggestions: Vec<String>,
    pub selected_suggestion: usize,

    // Pending tool confirmation
    pub confirmation: Option<PendingConfirmation>,
}

#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub name: String,
    pub input: String,
}

#[derive(Debug, Clone)]
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
    // pub fn symbol(self) -> &'static str {
    //     match self {
    //         Self::Running => "◇",
    //         Self::Success => "✓",
    //         Self::Failed => "✗",
    //     }
    // }

    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Success => "DONE",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Debug, Clone)]
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
    Error,
    Plan,
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

        let line = &mut self.lines[self.cursor_y];

        let cursor = self.cursor_x.min(line.len());

        // Keep the cursor on a UTF-8 character boundary.
        let cursor = if line.is_char_boundary(cursor) {
            cursor
        } else {
            let mut pos = cursor;

            while pos > 0 && !line.is_char_boundary(pos) {
                pos -= 1;
            }

            pos
        };

        line.insert(cursor, c);
        self.cursor_x = cursor + c.len_utf8();
    }

    pub fn newline(&mut self) {
        if self.cursor_y >= self.lines.len() {
            self.lines.push(String::new());
            self.cursor_y = self.lines.len() - 1;
            self.cursor_x = 0;
            return;
        }

        let line_len = self.lines[self.cursor_y].len();

        let mut cursor = self.cursor_x.min(line_len);

        while cursor > 0 && !self.lines[self.cursor_y].is_char_boundary(cursor) {
            cursor -= 1;
        }

        let rest = self.lines[self.cursor_y].split_off(cursor);

        self.lines.insert(self.cursor_y + 1, rest);

        self.cursor_y += 1;
        self.cursor_x = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_y >= self.lines.len() {
            return;
        }

        if self.cursor_x > 0 {
            let line = &mut self.lines[self.cursor_y];

            let mut previous = self.cursor_x.min(line.len());

            // Move to the beginning of the previous UTF-8 character.
            previous -= 1;

            while previous > 0 && !line.is_char_boundary(previous) {
                previous -= 1;
            }

            if line.is_char_boundary(previous) {
                line.drain(previous..self.cursor_x);
                self.cursor_x = previous;
            }

            return;
        }

        // Join with the previous line.
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

impl Default for App {
    fn default() -> Self {
        Self::new()
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

            more_mode: false,

            status: "Ready".into(),

            confirmation: None,

            scroll: 0,
            auto_scroll: true,

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
        self.selected_suggestion = 0;
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
        self.selected_suggestion = 0;

        self.welcome_visible = false;
        self.auto_scroll = true;

        Some(text)
    }

    // ─────────────────────────────────────────────
    // Confirmation
    // ─────────────────────────────────────────────

    pub fn confirmation_pending(&self) -> bool {
        self.confirmation.is_some()
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

    // ─────────────────────────────────────────────
    // System messages
    // ─────────────────────────────────────────────

    // pub fn add_system_message(&mut self, text: impl Into<String>) {
    //     self.messages.push(MessageLine {
    //         role: MessageRole::System,
    //         content: text.into(),
    //     });

    //     if self.auto_scroll {
    //         self.scroll_to_bottom();
    //     }
    // }
}
