use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::{
    theme::LumaTheme,
    tui::{
        app::App,
        components::{chat::ChatView, input::InputBox, status::StatusBar, welcome::WelcomeScreen},
        info::LumaInfo,
    },
};

pub fn draw(frame: &mut Frame, app: &App, theme: &LumaTheme, info: &LumaInfo, confirm_exit: bool) {
    let input_height = if app.suggestions.is_empty() {
        3
    } else {
        (4 + app.suggestions.len() as u16).min(10)
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(input_height),
            Constraint::Length(1),
        ])
        .split(frame.area());

    if app.welcome_visible {
        let welcome = WelcomeScreen::new(theme, info);

        welcome.render(frame, layout[0]);
    } else {
        let chat = ChatView::new(theme, app);

        chat.render(frame, layout[0]);
    }

    let input = InputBox::new(theme, &app.input, &app.suggestions, app.selected_suggestion);

    input.render(frame, layout[1]);

    let status = StatusBar::new(
        theme,
        info.provider.clone(),
        info.model.clone(),
        info.tools.len(),
        confirm_exit,
        app.thinking,
    );

    status.render(frame, layout[2]);
}
