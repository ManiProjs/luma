use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{theme::LumaTheme, tui::info::LumaInfo};

const LOGO: &[&str] = &[
    "██╗     ██╗   ██╗███╗   ███╗ █████╗ ",
    "██║     ██║   ██║████╗ ████║██╔══██╗",
    "██║     ██║   ██║██╔████╔██║███████║",
    "██║     ██║   ██║██║╚██╔╝██║██╔══██║",
    "███████╗╚██████╔╝██║ ╚═╝ ██║██║  ██║",
    "╚══════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝  ╚═╝",
];

pub struct WelcomeScreen<'a> {
    theme: &'a LumaTheme,

    info: &'a LumaInfo,
}

impl<'a> WelcomeScreen<'a> {
    pub fn new(theme: &'a LumaTheme, info: &'a LumaInfo) -> Self {
        Self { theme, info }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                " LUMA ",
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" | "),
            Span::raw(format!("{} ({})", self.info.provider, self.info.model)),
            Span::raw(" | "),
            Span::styled(
                self.info.status.clone(),
                Style::default().fg(self.theme.glow),
            ),
        ]));

        frame.render_widget(header, layout[0]);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(layout[1]);

        self.render_logo(frame, columns[0]);

        self.render_dashboard(frame, columns[1]);

        let footer = Paragraph::new("Type a message to begin • ↑ ↓ history • Ctrl+C interrupt")
            .block(Block::default().borders(Borders::ALL));

        frame.render_widget(footer, layout[2]);
    }

    fn render_logo(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        for row in LOGO {
            lines.push(Line::from(Span::styled(
                *row,
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        lines.push(Line::from(""));

        lines.push(Line::from("Local-first AI Coding Agent"));

        frame.render_widget(
            Paragraph::new(lines).block(Block::default().title(" Luma ").borders(Borders::ALL)),
            area,
        );
    }

    fn render_dashboard(&self, frame: &mut Frame, area: Rect) {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Min(5),
            ])
            .split(area);

        let model = vec![
            Line::from("AI Model"),
            Line::from(""),
            Line::from(format!("Provider: {}", self.info.provider)),
            Line::from(format!("Model: {}", self.info.model)),
        ];

        frame.render_widget(
            Paragraph::new(model).block(Block::default().title(" Model ").borders(Borders::ALL)),
            sections[0],
        );

        let workspace = vec![
            Line::from("Workspace"),
            Line::from(""),
            Line::from(format!(
                "Path: {}",
                self.info
                    .workspace
                    .clone()
                    .unwrap_or_else(|| "Unknown".into())
            )),
            Line::from(format!(
                "Language: {}",
                self.info
                    .language
                    .clone()
                    .unwrap_or_else(|| "Unknown".into())
            )),
            Line::from(format!("Files: {}", self.info.files_scanned)),
        ];

        frame.render_widget(
            Paragraph::new(workspace)
                .block(Block::default().title(" Workspace ").borders(Borders::ALL)),
            sections[1],
        );

        let mut tips = vec![Line::from("Capabilities"), Line::from("")];

        for tool in &self.info.tools {
            tips.push(Line::from(format!("✓ {}", tool)));
        }

        tips.push(Line::from(""));

        tips.push(Line::from("Tips"));

        for tip in &self.info.tips {
            tips.push(Line::from(format!("• {}", tip)));
        }

        frame.render_widget(
            Paragraph::new(tips).block(Block::default().title(" Dashboard ").borders(Borders::ALL)),
            sections[2],
        );
    }
}
