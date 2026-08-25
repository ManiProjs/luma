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
    pub thinking: bool,
}

impl<'a> InputBox<'a> {
    pub fn new(
        theme: &'a LumaTheme,
        buffer: &'a TextBuffer,
        suggestions: &'a [String],
        selected_suggestion: usize,
        thinking: bool,
    ) -> Self {
        Self {
            theme,
            buffer,
            suggestions,
            selected_suggestion,
            focused: true,
            thinking,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        self.render_input(&mut lines);

        if !self.suggestions.is_empty() {
            self.render_suggestions(&mut lines);
        }

        let border_style = if self.focused {
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.space)
        };

        let title = if self.thinking {
            " Luma is working "
        } else if self.focused {
            " Message "
        } else {
            " Input "
        };

        let title_style = if self.thinking {
            Style::default()
                .fg(self.theme.glow)
                .add_modifier(Modifier::BOLD)
        } else {
            border_style
        };

        let block = Block::default()
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(title, title_style),
            ]))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);

        frame.render_widget(paragraph, area);
    }

    fn render_input(&self, lines: &mut Vec<Line<'static>>) {
        if self.buffer.lines.is_empty() {
            self.render_placeholder(lines);
            return;
        }

        for (y, text) in self.buffer.lines.iter().enumerate() {
            if y == self.buffer.cursor_y {
                self.render_cursor_line(lines, text);
            } else {
                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::raw(text.clone()),
                ]));
            }
        }

        if self.buffer.lines.len() == 1 && self.buffer.lines[0].is_empty() {
            lines.clear();
            self.render_placeholder(lines);
        }
    }

    fn render_placeholder(&self, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(self.theme.accent)),
            Span::styled(
                "Ask Luma anything...",
                Style::default()
                    .fg(self.theme.space)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled("▌", Style::default().fg(self.theme.accent)),
        ]));
    }

    fn render_cursor_line(&self, lines: &mut Vec<Line<'static>>, text: &str) {
        let cursor = self.buffer.cursor_x.min(text.chars().count());

        let before: String = text.chars().take(cursor).collect();
        let after: String = text.chars().skip(cursor).collect();

        let mut spans = Vec::new();

        spans.push(Span::styled("│ ", Style::default().fg(self.theme.accent)));

        if !before.is_empty() {
            spans.push(Span::raw(before));
        }

        if let Some(cursor_char) = after.chars().next() {
            let rest: String = after.chars().skip(1).collect();

            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::REVERSED),
            ));

            if !rest.is_empty() {
                spans.push(Span::raw(rest));
            }
        } else {
            spans.push(Span::styled(
                " ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::REVERSED),
            ));
        }

        lines.push(Line::from(spans));
    }

    fn render_suggestions(&self, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(self.theme.space)),
            Span::styled(
                "COMMANDS",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        for (index, suggestion) in self.suggestions.iter().take(6).enumerate() {
            let selected = index == self.selected_suggestion;

            let prefix = if selected { "│ › " } else { "│   " };

            let prefix_style = if selected {
                Style::default().fg(self.theme.accent)
            } else {
                Style::default().fg(self.theme.space)
            };

            let command_style = if selected {
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.space)
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(suggestion.clone(), command_style),
            ]));
        }

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(self.theme.space)),
            Span::styled(
                "↑↓",
                Style::default()
                    .fg(self.theme.star)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" select  ", Style::default().fg(self.theme.space)),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(self.theme.star)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" complete  ", Style::default().fg(self.theme.space)),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(self.theme.star)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" run", Style::default().fg(self.theme.space)),
        ]));
    }
}
