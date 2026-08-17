use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::LumaTheme;

pub struct StatusBar<'a> {
    pub theme: &'a LumaTheme,

    pub model: String,

    pub tool_count: usize,

    pub confirm_exit: bool,
}

impl<'a> StatusBar<'a> {
    pub fn new(theme: &'a LumaTheme, model: String, tool_count: usize, confirm_exit: bool) -> Self {
        Self {
            theme,
            model,
            tool_count,
            confirm_exit,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let text = if self.confirm_exit {
            vec![Span::styled(
                "Press Ctrl+C again to exit",
                Style::default().fg(self.theme.star),
            )]
        } else {
            vec![
                Span::styled("Luma", Style::default().fg(self.theme.accent)),
                Span::raw(" | "),
                Span::raw(self.model.clone()),
                Span::raw(" | Tools: "),
                Span::raw(self.tool_count.to_string()),
                Span::raw(" | Ctrl+C exit"),
            ]
        };

        frame.render_widget(Paragraph::new(Line::from(text)), area);
    }
}
