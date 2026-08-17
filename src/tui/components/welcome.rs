use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::LumaTheme;

const LOGO: &[&str] = &[
    "██╗     ██╗   ██╗███╗   ███╗ █████╗ ",
    "██║     ██║   ██║████╗ ████║██╔══██╗",
    "██║     ██║   ██║██╔████╔██║███████║",
    "██║     ██║   ██║██║╚██╔╝██║██╔══██║",
    "███████╗╚██████╔╝██║ ╚═╝ ██║██║  ██║",
    "╚══════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝  ╚═╝",
];

pub struct WelcomeScreen<'a> {
    pub theme: &'a LumaTheme,

    pub tools: Vec<String>,
}

impl<'a> WelcomeScreen<'a> {
    pub fn new(theme: &'a LumaTheme, tools: Vec<String>) -> Self {
        Self { theme, tools }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        // Top border
        lines.push(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(self.theme.accent),
        )));

        lines.push(Line::from(""));

        // Logo
        for (i, row) in LOGO.iter().enumerate() {
            let color = match i {
                0 => self.theme.star,

                1 => self.theme.glow,

                2 => Color::Yellow,

                3 => self.theme.accent,

                _ => self.theme.space,
            };

            lines.push(Line::from(Span::styled(
                row.to_string(),
                Style::default().fg(color),
            )));
        }

        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "Local-first AI Coding Agent",
            Style::default().fg(self.theme.glow),
        )));

        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "Available tools:",
            Style::default().fg(self.theme.accent),
        )));

        if self.tools.is_empty() {
            lines.push(Line::from("  No tools loaded"));
        } else {
            for tool in &self.tools {
                lines.push(Line::from(vec![
                    Span::styled("  • ", Style::default().fg(self.theme.star)),
                    Span::raw(tool.clone()),
                ]));
            }
        }

        lines.push(Line::from(""));

        lines.push(Line::from("Type a message to begin..."));

        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(self.theme.accent),
        )));

        frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
    }
}
