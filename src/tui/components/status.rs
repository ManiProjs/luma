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

    pub provider: String,

    pub model: String,

    pub tool_count: usize,

    pub confirm_exit: bool,

    pub thinking: bool,
}

impl<'a> StatusBar<'a> {
    pub fn new(
        theme: &'a LumaTheme,
        provider: String,
        model: String,
        tool_count: usize,
        confirm_exit: bool,
        thinking: bool,
    ) -> Self {
        Self {
            theme,
            provider,
            model,
            tool_count,
            confirm_exit,
            thinking,
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
                Span::raw(" │ "),
                Span::raw(format!("{} ({})", self.provider, self.model)),
                Span::raw(" │ "),
                Span::raw(format!("{} tools", self.tool_count)),
                Span::raw(" │ Ctrl+C interrupt"),
            ]
        };

        frame.render_widget(Paragraph::new(Line::from(text)), area);
    }
}
