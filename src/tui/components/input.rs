use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{theme::LumaTheme, tui::app::TextBuffer};

pub struct InputBox<'a> {
    pub theme: &'a LumaTheme,

    pub buffer: &'a TextBuffer,

    pub focused: bool,
}

impl<'a> InputBox<'a> {
    pub fn new(theme: &'a LumaTheme, buffer: &'a TextBuffer) -> Self {
        Self {
            theme,
            buffer,
            focused: true,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        let content = self.buffer.lines.iter();

        for line in content {
            lines.push(Line::from(line.clone()));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Type your message...",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }

        let title = if self.focused { " Message " } else { " Input " };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));

        let paragraph = Paragraph::new(lines).block(block);

        frame.render_widget(paragraph, area);
    }
}
