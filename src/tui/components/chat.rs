use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    theme::LumaTheme,
    tui::{
        app::{App, MessageLine, MessageRole, ToolStatus},
        markdown::MarkdownRenderer,
    },
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
        let animation = self.app.logo_frame % 4;

        let spinner = match animation {
            0 => "◐",
            1 => "◓",
            2 => "◑",
            _ => "◒",
        };

        self.render_chat(frame, area, spinner, animation);
    }

    // ============================================================
    // Chat
    // ============================================================

    fn render_chat(&self, frame: &mut Frame, area: Rect, spinner: &str, animation: usize) {
        let mut lines: Vec<Line<'static>> = Vec::new();

        // ========================================================
        // Empty state
        // ========================================================

        if self.app.messages.is_empty()
            && !self.app.thinking
            && !self.app.confirmation_pending()
            && self.app.current_tool.is_none()
        {
            self.render_empty_state(frame, area);
            return;
        }

        // ========================================================
        // Messages
        // ========================================================

        for message in &self.app.messages {
            match message.role {
                MessageRole::Tool => {
                    self.render_tool_message(&mut lines, message);
                }

                _ => {
                    self.render_message(&mut lines, message, animation);
                }
            }

            lines.push(Line::from(""));
        }

        // ========================================================
        // Active tool
        // ========================================================

        if let Some(tool) = &self.app.current_tool {
            self.render_active_tool(&mut lines, tool, spinner);

            lines.push(Line::from(""));
        }

        // ========================================================
        // Confirmation
        // ========================================================

        if let Some(confirmation) = &self.app.confirmation {
            self.render_confirmation(&mut lines, confirmation);

            lines.push(Line::from(""));
        }

        // ========================================================
        // Thinking
        // ========================================================

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

        // ========================================================
        // Scrolling
        // ========================================================

        let inner_width = area.width.saturating_sub(2);
        let viewport_height = area.height.saturating_sub(2);

        let content_height = Self::visual_height(&lines, inner_width);

        let max_scroll = content_height.saturating_sub(viewport_height as usize);

        let scroll = if self.app.auto_scroll {
            max_scroll
        } else {
            self.app.scroll.min(max_scroll)
        };

        // ========================================================
        // Container
        // ========================================================

        let title_status = if self.app.confirmation_pending() {
            " CONFIRMATION "
        } else if self.app.thinking {
            " ACTIVE "
        } else {
            " CHAT "
        };

        let border_style = if self.app.confirmation_pending() {
            Style::default().fg(self.theme.accent)
        } else if self.app.thinking {
            Style::default().fg(self.theme.glow)
        } else {
            Style::default().fg(self.theme.space)
        };

        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(
                    " CHAT ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(title_status, Style::default().fg(self.theme.space)),
            ]))
            .title_bottom(Line::from(vec![
                Span::styled(
                    format!(" {} messages ", self.app.messages.len()),
                    Style::default().fg(self.theme.space),
                ),
                Span::styled(
                    if self.app.confirmation_pending() {
                        " • action required "
                    } else if self.app.thinking {
                        " • processing "
                    } else {
                        " • ready "
                    },
                    if self.app.confirmation_pending() {
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.space)
                    },
                ),
            ]))
            .borders(Borders::ALL)
            .border_style(border_style);

        let chat = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0));

        frame.render_widget(chat, area);
    }

    // ============================================================
    // Empty state
    // ============================================================

    fn render_empty_state(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(
                    " LUMA ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" • READY ", Style::default().fg(self.theme.space)),
            ]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.space));

        let inner = block.inner(area);

        frame.render_widget(block, area);

        if inner.height < 8 {
            return;
        }

        let center_y = inner.y + inner.height / 2;

        let logo = Line::from(vec![
            Span::styled(
                "◆ ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "LUMA",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ◆",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let subtitle = Line::from(Span::styled(
            "Your coding agent is ready.",
            Style::default().fg(self.theme.space),
        ));

        let hint = Line::from(vec![
            Span::styled("Type a message ", Style::default().fg(self.theme.space)),
            Span::styled(
                "and press Enter",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(vec![logo, Line::from(""), subtitle, hint]).alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y: center_y.saturating_sub(2),
                width: inner.width,
                height: 6,
            },
        );
    }

    // ============================================================
    // Active tool
    // ============================================================

    fn render_active_tool(
        &self,
        lines: &mut Vec<Line<'static>>,
        tool: &crate::tui::app::ToolState,
        spinner: &str,
    ) {
        let status_style = match tool.status {
            ToolStatus::Running => Style::default()
                .fg(self.theme.glow)
                .add_modifier(Modifier::BOLD),

            ToolStatus::Success => Style::default()
                .fg(self.theme.glow)
                .add_modifier(Modifier::BOLD),

            ToolStatus::Failed => Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        };

        lines.push(Line::from(vec![
            Span::styled("╭─ ", Style::default().fg(self.theme.space)),
            Span::styled("TOOL ", status_style),
            Span::styled(tool.status.label(), Style::default().fg(self.theme.space)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(self.theme.space)),
            Span::styled(format!("{} ", spinner), status_style),
            Span::styled(tool.name.clone(), status_style),
        ]));

        if !tool.input.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("│   ", Style::default().fg(self.theme.space)),
                Span::styled(tool.input.clone(), Style::default().fg(self.theme.space)),
            ]));
        }

        lines.push(Line::from(Span::styled(
            "╰────────────────────────────────────────",
            Style::default().fg(self.theme.space),
        )));
    }

    // ============================================================
    // Confirmation
    // ============================================================

    fn render_confirmation(
        &self,
        lines: &mut Vec<Line<'static>>,
        confirmation: &crate::tui::app::PendingConfirmation,
    ) {
        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        lines.push(Line::from(vec![
            Span::styled("╭─ ", muted),
            Span::styled("⚠ CONFIRMATION REQUIRED", accent),
            Span::styled(" ─────────────────────", muted),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│ ", muted),
            Span::styled("Tool   ", accent),
            Span::styled(
                confirmation.name.clone(),
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│ ", muted),
            Span::styled("Input  ", accent),
            Span::styled(confirmation.input.clone(), muted),
        ]));

        lines.push(Line::from(Span::styled("│", muted)));

        lines.push(Line::from(vec![
            Span::styled("│ ", muted),
            Span::styled(
                "Allow this operation?",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│ ", muted),
            Span::styled(
                "[Y]",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Allow    ", muted),
            Span::styled(
                "[N]",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Deny", muted),
        ]));

        lines.push(Line::from(Span::styled(
            "╰────────────────────────────────────────",
            muted,
        )));
    }

    // ============================================================
    // Tool messages
    // ============================================================

    fn render_tool_message(&self, lines: &mut Vec<Line<'static>>, message: &MessageLine) {
        let content = message.content.as_str();

        let (icon, style) = if content.starts_with("✓ ") {
            (
                "✓",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            )
        } else if content.starts_with("✗ ") {
            (
                "✗",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                "◇",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        };

        let mut parts = content.splitn(2, ' ');

        let first = parts.next().unwrap_or("tool");
        let rest = parts.next().unwrap_or("");

        let tool_name = if first == "✓" || first == "✗" {
            rest.split_whitespace().next().unwrap_or("tool")
        } else {
            first
        };

        let display_input = if first == "✓" || first == "✗" {
            rest.strip_prefix(tool_name).unwrap_or("").trim()
        } else {
            rest
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", icon), style),
            Span::styled(tool_name.to_string(), style),
            Span::styled(
                if display_input.is_empty() {
                    String::new()
                } else {
                    format!("  {}", display_input)
                },
                Style::default().fg(self.theme.space),
            ),
        ]));
    }

    // ============================================================
    // Normal messages
    // ============================================================

    fn render_message(
        &self,
        lines: &mut Vec<Line<'static>>,
        message: &MessageLine,
        animation: usize,
    ) {
        let (title, title_style) = match message.role {
            MessageRole::User => (
                " YOU ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),

            MessageRole::Assistant => (
                " LUMA ",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
            ),

            MessageRole::System => (
                " SYSTEM ",
                Style::default()
                    .fg(self.theme.star)
                    .add_modifier(Modifier::BOLD),
            ),

            MessageRole::Plan => (
                " PLAN ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),

            MessageRole::Error => (
                " ERROR ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),

            MessageRole::Tool => (
                " TOOL ",
                Style::default()
                    .fg(self.theme.space)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        // Header
        lines.push(Line::from(vec![
            Span::styled("╭─", Style::default().fg(self.theme.space)),
            Span::styled(title, title_style),
            Span::styled(
                "────────────────────────",
                Style::default().fg(self.theme.space),
            ),
        ]));

        // ========================================================
        // Markdown
        // ========================================================

        let renderer = MarkdownRenderer::new(self.theme);

        for line in renderer.render(&message.content) {
            let mut spans = Vec::with_capacity(line.spans.len() + 1);

            spans.push(Span::styled("│ ", Style::default().fg(self.theme.space)));

            spans.extend(line.spans);

            lines.push(Line::from(spans));
        }

        // Streaming cursor
        if message.role == MessageRole::Assistant && self.app.thinking {
            let cursor = if animation.is_multiple_of(2) {
                "▌"
            } else {
                " "
            };

            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(self.theme.space)),
                Span::styled(cursor, Style::default().fg(self.theme.glow)),
            ]));
        }

        // Footer
        lines.push(Line::from(Span::styled(
            "╰────────────────────────",
            Style::default().fg(self.theme.space),
        )));
    }

    // ============================================================
    // Visual height
    // ============================================================

    fn visual_height(lines: &[Line<'static>], width: u16) -> usize {
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
