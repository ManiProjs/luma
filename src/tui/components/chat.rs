use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
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
        let mut lines = Vec::<Line>::new();

        for message in &self.app.messages {
            let (prefix, style) = match message.role {
                MessageRole::User => ("> ", Style::default().fg(self.theme.accent)),

                MessageRole::Assistant => ("Luma: ", Style::default().fg(self.theme.glow)),

                MessageRole::Tool => ("🔧 ", Style::default().fg(self.theme.space)),

                MessageRole::System => ("[!] ", Style::default().fg(self.theme.star)),
            };

            let mut first_line = true;

            for text in message.content.lines() {
                if first_line {
                    lines.push(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::raw(text.to_string()),
                    ]));

                    first_line = false;
                } else {
                    // continuation lines
                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::raw(text.to_string()),
                    ]));
                }
            }

            // spacing between messages

            lines.push(Line::from(""));
        }

        if self.app.thinking {
            lines.push(Line::from(Span::styled(
                "◐ Luma is thinking▌",
                Style::default().fg(self.theme.glow),
            )));
        }

        let chat = Paragraph::new(lines).wrap(Wrap { trim: false });

        frame.render_widget(chat, area);
    }
}
