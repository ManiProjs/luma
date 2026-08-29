use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    theme::LumaTheme,
    tui::app::{App, MessageLine, MessageRole, ToolStatus},
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
        let mut lines: Vec<Line<'static>> = Vec::new();

        let animation = self.app.logo_frame % 4;

        let spinner = match animation {
            0 => "◐",
            1 => "◓",
            2 => "◑",
            _ => "◒",
        };

        // ─────────────────────────────────────────
        // Messages
        // ─────────────────────────────────────────

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

        // ─────────────────────────────────────────
        // Confirmation request
        // ─────────────────────────────────────────

        if let Some(confirmation) = &self.app.confirmation {
            lines.push(Line::from(vec![
                Span::styled("╭─", Style::default().fg(self.theme.accent)),
                Span::styled(
                    " CONFIRMATION REQUIRED ",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "────────────────────────",
                    Style::default().fg(self.theme.space),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    "│ Tool: ",
                    Style::default()
                        .fg(self.theme.space)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    confirmation.name.clone(),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    "│ Input: ",
                    Style::default()
                        .fg(self.theme.space)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    confirmation.input.clone(),
                    Style::default().fg(self.theme.star),
                ),
            ]));

            lines.push(Line::from("│"));

            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(self.theme.space)),
                Span::styled(
                    "[Enter]",
                    Style::default()
                        .fg(self.theme.glow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Allow    ", Style::default().fg(self.theme.star)),
                Span::styled(
                    "[esc]",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Deny", Style::default().fg(self.theme.star)),
            ]));

            lines.push(Line::from(Span::styled(
                "╰────────────────────────────────",
                Style::default().fg(self.theme.accent),
            )));

            lines.push(Line::from(""));
        }

        // ─────────────────────────────────────────
        // Active tool
        // ─────────────────────────────────────────

        if let Some(tool) = &self.app.current_tool {
            let status = tool.status;

            let status_style = match status {
                ToolStatus::Running => Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),

                ToolStatus::Success => Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),

                ToolStatus::Failed => Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", spinner), status_style),
                Span::styled(tool.name.clone(), status_style),
                Span::raw(" "),
                Span::styled(tool.input.clone(), Style::default().fg(self.theme.space)),
            ]));

            lines.push(Line::from(""));
        }

        // ─────────────────────────────────────────
        // Thinking indicator
        // ─────────────────────────────────────────

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

        // ─────────────────────────────────────────
        // Scrolling
        // ─────────────────────────────────────────

        let inner_width = area.width.saturating_sub(2);
        let viewport_height = area.height.saturating_sub(2);

        let content_height = Self::visual_height(&lines, inner_width);

        let max_scroll = content_height.saturating_sub(viewport_height as usize);

        let scroll = if self.app.auto_scroll {
            max_scroll
        } else {
            self.app.scroll.min(max_scroll)
        };

        // ─────────────────────────────────────────
        // Container
        // ─────────────────────────────────────────

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
            .scroll((scroll.min(u16::MAX as usize) as u16, 0));

        frame.render_widget(chat, area);

        // ─────────────────────────────────────────
        // Generation animation
        // ─────────────────────────────────────────

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
                    animation_line.push(border_char.chars().next().unwrap_or('─'));
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
        let (title, title_style, body_style) = match message.role {
            MessageRole::User => (
                " You ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
            ),

            MessageRole::Assistant => (
                " Luma ",
                Style::default()
                    .fg(self.theme.glow)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
            ),

            MessageRole::System => (
                " System ",
                Style::default()
                    .fg(self.theme.star)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
            ),

            MessageRole::Plan => (
                " Plan ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
            ),

            MessageRole::Error => (
                " Error ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
            ),

            MessageRole::Tool => (
                " Tool ",
                Style::default()
                    .fg(self.theme.space)
                    .add_modifier(Modifier::BOLD),
                Style::default(),
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

        // Body
        self.render_markdown_with_base(lines, &message.content, body_style);

        // Streaming cursor
        if message.role == MessageRole::Assistant && self.app.thinking {
            let cursor = if animation % 2 == 0 { "▌" } else { " " };

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
    // Markdown renderer
    // ============================================================

    fn render_markdown(&self, output: &mut Vec<Line<'static>>, markdown: &str) {
        self.render_markdown_with_base(output, markdown, Style::default());
    }

    fn render_markdown_with_base(
        &self,
        output: &mut Vec<Line<'static>>,
        markdown: &str,
        base_style: Style,
    ) {
        let mut in_code_block = false;

        for raw_line in markdown.lines() {
            let line = raw_line.trim_end();

            // ─────────────────────────────────────
            // Fenced code block
            // ─────────────────────────────────────

            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;

                if in_code_block {
                    output.push(Line::from(Span::styled(
                        "│ ┌────────────────────────────────",
                        self.theme.code_block_border_style(),
                    )));
                } else {
                    output.push(Line::from(Span::styled(
                        "│ └────────────────────────────────",
                        self.theme.code_block_border_style(),
                    )));
                }

                continue;
            }

            if in_code_block {
                output.push(Line::from(vec![
                    Span::styled("│ │ ", self.theme.code_block_border_style()),
                    Span::styled(line.to_string(), self.theme.code_style()),
                ]));

                continue;
            }

            // ─────────────────────────────────────
            // Empty line
            // ─────────────────────────────────────

            if line.trim().is_empty() {
                output.push(Line::from(Span::styled(
                    "│",
                    Style::default().fg(self.theme.space),
                )));

                continue;
            }

            // ─────────────────────────────────────
            // Headings
            // ─────────────────────────────────────

            if let Some(heading) = line.strip_prefix("### ") {
                output.push(self.markdown_line(
                    "│ ",
                    heading,
                    self.theme.heading_style().add_modifier(Modifier::BOLD),
                ));

                continue;
            }

            if let Some(heading) = line.strip_prefix("## ") {
                output.push(self.markdown_line(
                    "│ ",
                    heading,
                    self.theme.heading_style().add_modifier(Modifier::BOLD),
                ));

                continue;
            }

            if let Some(heading) = line.strip_prefix("# ") {
                output.push(self.markdown_line(
                    "│ ",
                    heading,
                    self.theme.heading_style().add_modifier(Modifier::BOLD),
                ));

                continue;
            }

            // ─────────────────────────────────────
            // Blockquote
            // ─────────────────────────────────────

            if let Some(quote) = line.strip_prefix("> ") {
                let quote_style = self.theme.quote_style();

                let mut spans = vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::styled("│ ", quote_style),
                ];

                spans.extend(self.inline_markdown(quote, quote_style));

                output.push(Line::from(spans));

                continue;
            }

            // ─────────────────────────────────────
            // Unordered lists
            // ─────────────────────────────────────

            if let Some(item) = line.strip_prefix("- ") {
                let mut spans = vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::styled(
                        "• ",
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];

                spans.extend(self.inline_markdown(item, base_style));

                output.push(Line::from(spans));

                continue;
            }

            if let Some(item) = line.strip_prefix("* ") {
                let mut spans = vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::styled(
                        "• ",
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];

                spans.extend(self.inline_markdown(item, base_style));

                output.push(Line::from(spans));

                continue;
            }

            // ─────────────────────────────────────
            // Ordered lists
            // ─────────────────────────────────────

            if let Some((number, item)) = Self::ordered_list(line) {
                let mut spans = vec![
                    Span::styled("│ ", Style::default().fg(self.theme.space)),
                    Span::styled(
                        format!("{} ", number),
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                ];

                spans.extend(self.inline_markdown(item, base_style));

                output.push(Line::from(spans));

                continue;
            }

            // ─────────────────────────────────────
            // Horizontal rule
            // ─────────────────────────────────────

            if Self::is_horizontal_rule(line) {
                output.push(Line::from(Span::styled(
                    "│ ────────────────────────────────",
                    Style::default().fg(self.theme.space),
                )));

                continue;
            }

            // ─────────────────────────────────────
            // Normal paragraph
            // ─────────────────────────────────────

            let mut spans = vec![Span::styled("│ ", Style::default().fg(self.theme.space))];

            spans.extend(self.inline_markdown(line, base_style));

            output.push(Line::from(spans));
        }
    }

    // ============================================================
    // Inline Markdown
    // ============================================================

    fn inline_markdown(&self, text: &str, base_style: Style) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut current = String::new();

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // ─────────────────────────────────────
            // Inline code
            // ─────────────────────────────────────

            if chars[i] == '`' {
                if !current.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut current), base_style));
                }

                i += 1;

                let start = i;

                while i < chars.len() && chars[i] != '`' {
                    i += 1;
                }

                let code: String = chars[start..i].iter().collect();

                spans.push(Span::styled(code, self.theme.code_style()));

                if i < chars.len() {
                    i += 1;
                }

                continue;
            }

            // ─────────────────────────────────────
            // Bold
            // ─────────────────────────────────────

            if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
                if !current.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut current), base_style));
                }

                i += 2;

                let start = i;

                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                    i += 1;
                }

                let bold: String = chars[start..i].iter().collect();

                spans.push(Span::styled(bold, base_style.add_modifier(Modifier::BOLD)));

                if i + 1 < chars.len() {
                    i += 2;
                }

                continue;
            }

            // ─────────────────────────────────────
            // Italic
            // ─────────────────────────────────────

            if chars[i] == '*' && (i + 1 >= chars.len() || chars[i + 1] != '*') {
                if !current.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut current), base_style));
                }

                i += 1;

                let start = i;

                while i < chars.len() && chars[i] != '*' {
                    i += 1;
                }

                let italic: String = chars[start..i].iter().collect();

                spans.push(Span::styled(
                    italic,
                    base_style.add_modifier(Modifier::ITALIC),
                ));

                if i < chars.len() {
                    i += 1;
                }

                continue;
            }

            // ─────────────────────────────────────
            // Strikethrough
            // ─────────────────────────────────────

            if i + 1 < chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
                if !current.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut current), base_style));
                }

                i += 2;

                let start = i;

                while i + 1 < chars.len() && !(chars[i] == '~' && chars[i + 1] == '~') {
                    i += 1;
                }

                let strike: String = chars[start..i].iter().collect();

                spans.push(Span::styled(
                    strike,
                    base_style.add_modifier(Modifier::CROSSED_OUT),
                ));

                if i + 1 < chars.len() {
                    i += 2;
                }

                continue;
            }

            current.push(chars[i]);
            i += 1;
        }

        if !current.is_empty() {
            spans.push(Span::styled(current, base_style));
        }

        spans
    }

    // ============================================================
    // Helpers
    // ============================================================

    fn markdown_line(&self, prefix: &str, text: &str, style: Style) -> Line<'static> {
        let mut spans = vec![Span::styled(
            prefix.to_owned(),
            Style::default().fg(self.theme.space),
        )];

        spans.extend(self.inline_markdown(text, style));

        Line::from(spans)
    }

    fn ordered_list(line: &str) -> Option<(u32, &str)> {
        let mut end = 0;

        for (index, c) in line.char_indices() {
            if c.is_ascii_digit() {
                end = index + c.len_utf8();
            } else {
                break;
            }
        }

        if end == 0 {
            return None;
        }

        let rest = &line[end..];

        let rest = rest.strip_prefix(". ")?;

        let number = line[..end].parse().ok()?;

        Some((number, rest))
    }

    fn is_horizontal_rule(line: &str) -> bool {
        let trimmed = line.trim();

        if trimmed.len() < 3 {
            return false;
        }

        trimmed.chars().all(|c| c == '-' || c == '*' || c == '_')
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
