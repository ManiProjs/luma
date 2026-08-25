use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{theme::LumaTheme, tui::info::LumaInfo};

pub struct WelcomeScreen<'a> {
    pub theme: &'a LumaTheme,
    pub info: &'a LumaInfo,
}

impl<'a> WelcomeScreen<'a> {
    pub fn new(theme: &'a LumaTheme, info: &'a LumaInfo) -> Self {
        Self { theme, info }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let area = area.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });

        if area.width < 60 || area.height < 10 {
            self.render_compact(frame, area);
            return;
        }

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(1),
                Constraint::Min(6),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_header(frame, layout[0]);
        self.render_separator(frame, layout[1]);
        self.render_body(frame, layout[2]);
        self.render_footer(frame, layout[3]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = Style::default()
            .fg(self.theme.glow)
            .add_modifier(Modifier::BOLD);

        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        let workspace = self
            .info
            .workspace
            .as_deref()
            .unwrap_or("Current directory");

        let lines = vec![
            Line::from(vec![Span::styled("LUMA", title)]),
            Line::from(vec![
                Span::styled("Local-first AI coding agent", muted),
                Span::styled("                         ", muted),
                Span::styled("● ", accent),
                Span::styled("Ready", accent),
            ]),
            Line::from(vec![
                Span::styled(&self.info.model, muted),
                Span::styled(" · ", muted),
                Span::styled(&self.info.provider, muted),
                Span::styled("                         ", muted),
                Span::styled(workspace, muted),
            ]),
        ];

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_separator(&self, frame: &mut Frame, area: Rect) {
        let line = "─".repeat(area.width as usize);

        frame.render_widget(
            Paragraph::new(Span::styled(line, Style::default().fg(self.theme.space))),
            area,
        );
    }

    fn render_body(&self, frame: &mut Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);

        self.render_commands(frame, columns[0]);
        self.render_capabilities(frame, columns[1]);
    }

    fn render_commands(&self, frame: &mut Frame, area: Rect) {
        let heading = Style::default()
            .fg(self.theme.glow)
            .add_modifier(Modifier::BOLD);

        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        let lines = vec![
            Line::from(Span::styled("Get started", heading)),
            Line::from(""),
            Line::from(vec![
                Span::styled("/init", accent),
                Span::styled("  initialize workspace", muted),
            ]),
            Line::from(vec![
                Span::styled("/help", accent),
                Span::styled("  show commands", muted),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Ask Luma to inspect, edit, debug or build.",
                muted,
            )),
        ];

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_capabilities(&self, frame: &mut Frame, area: Rect) {
        let heading = Style::default()
            .fg(self.theme.glow)
            .add_modifier(Modifier::BOLD);

        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        let mut lines = vec![Line::from(vec![
            Span::styled("Capabilities", heading),
            Span::styled(format!(" · {}", self.info.tools.len()), muted),
        ])];

        lines.push(Line::from(""));

        for chunk in self.info.tools.chunks(2) {
            let mut spans = Vec::new();

            for (index, tool) in chunk.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled("    ", muted));
                }

                spans.push(Span::styled("✓ ", accent));
                spans.push(Span::styled(tool, muted));
            }

            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        let lines = Line::from(vec![
            Span::styled("↑↓", accent),
            Span::styled(" history  ", muted),
            Span::styled("Tab", accent),
            Span::styled(" autocomplete  ", muted),
            Span::styled("Ctrl+C", accent),
            Span::styled(" interrupt", muted),
        ]);

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_compact(&self, frame: &mut Frame, area: Rect) {
        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);

        let glow = Style::default()
            .fg(self.theme.glow)
            .add_modifier(Modifier::BOLD);

        let muted = Style::default().fg(self.theme.space);

        let lines = vec![
            Line::from(vec![
                Span::styled("LUMA", glow),
                Span::styled("  ● ", muted),
                Span::styled("Ready", accent),
            ]),
            Line::from(vec![
                Span::styled(&self.info.model, muted),
                Span::styled(" · ", muted),
                Span::styled(&self.info.provider, muted),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("/init", accent),
                Span::styled(" initialize   ", muted),
                Span::styled("/help", accent),
                Span::styled(" commands", muted),
            ]),
        ];

        frame.render_widget(Paragraph::new(lines), area);
    }
}
