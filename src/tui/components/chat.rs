use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    theme::LumaTheme,
    tui::app::{App, MessageRole},
};

pub struct ChatView<'a> {
    pub theme: &'a LumaTheme,
    pub app: &'a App,
}

impl<'a> ChatView<'a> {
    pub fn new(theme: &'a LumaTheme, app: &'a App) -> Self {
        Self { theme, app }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        let animation = self.app.logo_frame % 4;

        let spinner = match animation {
            0 => "◐",
            1 => "◓",
            2 => "◑",
            _ => "◒",
        };

        for message in &self.app.messages {
            let (title, title_style) = match &message.role {
                MessageRole::User => (
                    " You ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),

                MessageRole::Assistant => (
                    " Luma ",
                    Style::default()
                        .fg(self.theme.glow)
                        .add_modifier(Modifier::BOLD),
                ),

                MessageRole::Tool => (
                    " Tool ",
                    Style::default()
                        .fg(self.theme.space)
                        .add_modifier(Modifier::BOLD),
                ),

                MessageRole::System => (
                    " System ",
                    Style::default()
                        .fg(self.theme.star)
                        .add_modifier(Modifier::BOLD),
                ),
            };

            // Message header.
            lines.push(Line::from(vec![
                Span::styled("╭─", Style::default().fg(self.theme.space)),
                Span::styled(title, title_style),
                Span::styled(
                    "────────────────────────",
                    Style::default().fg(self.theme.space),
                ),
            ]));

            let content_lines: Vec<&str> = message.content.lines().collect();

            if content_lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    "│",
                    Style::default().fg(self.theme.space),
                )));
            } else {
                for text in content_lines {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(self.theme.space)),
                        Span::raw(text.to_string()),
                    ]));
                }
            }

            // Animated cursor while Luma is actively generating.
            if matches!(&message.role, MessageRole::Assistant) && self.app.thinking {
                let cursor = if animation % 2 == 0 { "▌" } else { " " };

                lines.push(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::styled(cursor, Style::default().fg(self.theme.glow)),
                ]));
            }

            lines.push(Line::from(Span::styled(
                "╰────────────────────────",
                Style::default().fg(self.theme.space),
            )));

            lines.push(Line::from(""));
        }

        // Active tool indicator.
        if let Some(tool) = &self.app.current_tool {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", spinner),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    &tool.name,
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(&tool.input, Style::default().fg(self.theme.space)),
            ]));

            lines.push(Line::from(""));
        }

        // Thinking indicator.
        if self.app.thinking {
            let thinking_text = match animation {
                0 => "Luma is thinking",
                1 => "Luma is thinking.",
                2 => "Luma is thinking..",
                _ => "Luma is thinking...",
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", spinner),
                    Style::default()
                        .fg(self.theme.glow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    thinking_text,
                    Style::default()
                        .fg(self.theme.glow)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        let inner_width = area.width.saturating_sub(2);
        let viewport_height = area.height.saturating_sub(2);

        let content_height = Self::visual_height(&lines, inner_width);

        let max_scroll = content_height.saturating_sub(viewport_height as usize);

        let scroll = if self.app.auto_scroll {
            max_scroll
        } else {
            self.app.scroll.min(max_scroll)
        };

        let border_char = match animation {
            0 => "─",
            1 => "━",
            2 => "─",
            _ => "━",
        };

        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(
                    " LUMA ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if self.app.thinking {
                        " • ACTIVE "
                    } else {
                        " • READY "
                    },
                    Style::default().fg(self.theme.space),
                ),
            ]))
            .title_bottom(Line::from(Span::styled(
                format!(" {} messages ", self.app.messages.len()),
                Style::default().fg(self.theme.space),
            )))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.app.thinking {
                self.theme.glow
            } else {
                self.theme.space
            }));

        let chat = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0));

        frame.render_widget(chat, area);

        // Small animated line at the bottom when generating.
        if self.app.thinking && area.width > 4 {
            let x = area.x + 2;
            let y = area.y + area.height.saturating_sub(1);

            let width = area.width.saturating_sub(4);

            let position = if width > 0 {
                (self.app.logo_frame as u16) % width
            } else {
                0
            };

            let mut animation_line = String::new();

            for i in 0..width {
                if i == position {
                    animation_line.push('●');
                } else {
                    animation_line.push(border_char.chars().next().unwrap());
                }
            }

            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    animation_line,
                    Style::default().fg(self.theme.glow),
                ))),
                Rect {
                    x,
                    y,
                    width,
                    height: 1,
                },
            );
        }
    }

    fn visual_height(lines: &[Line], width: u16) -> usize {
        if width == 0 {
            return lines.len();
        }

        let width = width as usize;

        lines
            .iter()
            .map(|line| {
                let text_width: usize = line
                    .spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum();

                if text_width == 0 {
                    1
                } else {
                    text_width.div_ceil(width)
                }
            })
            .sum()
    }
}
