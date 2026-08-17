use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::theme::LumaTheme;

pub struct LogoRenderer<'a> {
    pub theme: &'a LumaTheme,
    pub frame: usize,
}

impl<'a> LogoRenderer<'a> {
    pub fn new(theme: &'a LumaTheme, frame: usize) -> Self {
        Self { theme, frame }
    }

    pub fn render(&self) -> Vec<Line<'static>> {
        LOGO.iter()
            .map(|row| {
                let mut spans = Vec::new();

                for (index, ch) in row.chars().enumerate() {
                    if ch == ' ' {
                        spans.push(Span::raw(" "));

                        continue;
                    }

                    let color = self.gradient_color(index);

                    spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
                }

                Line::from(spans)
            })
            .collect()
    }

    fn gradient_color(&self, position: usize) -> Color {
        let colors = [
            self.theme.star,
            self.theme.glow,
            self.theme.accent,
            self.theme.space,
        ];

        let index = (position + self.frame) % colors.len();

        colors[index]
    }
}

const LOGO: &[&str] = &[
    "██╗     ██╗   ██╗███╗   ███╗ █████╗ ",
    "██║     ██║   ██║████╗ ████║██╔══██╗",
    "██║     ██║   ██║██╔████╔██║███████║",
    "██║     ██║   ██║██║╚██╔╝██║██╔══██║",
    "███████╗╚██████╔╝██║ ╚═╝ ██║██║  ██║",
    "╚══════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝  ╚═╝",
];
