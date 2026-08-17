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

    pub suggestions: &'a [String],

    pub selected_suggestion: usize,

    pub focused: bool,
}

impl<'a> InputBox<'a> {
    pub fn new(
        theme: &'a LumaTheme,
        buffer: &'a TextBuffer,
        suggestions: &'a [String],
        selected_suggestion: usize,
    ) -> Self {
        Self {
            theme,
            buffer,
            suggestions,
            selected_suggestion,
            focused: true,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        for line in &self.buffer.lines {
            if !line.is_empty() || self.suggestions.is_empty() {
                lines.push(Line::from(line.clone()));
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "Type your message...",
                Style::default().add_modifier(Modifier::DIM),
            )));
        }

        if !self.suggestions.is_empty() {
            lines.push(Line::from(""));

            lines.push(Line::from(Span::styled(
                "Commands",
                Style::default().fg(self.theme.accent),
            )));

            for (index, suggestion) in self.suggestions.iter().enumerate() {
                let selected = index == self.selected_suggestion;

                let prefix = if selected { "› " } else { "  " };

                let style = if selected {
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                lines.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(suggestion.clone(), style),
                ]));
            }
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
