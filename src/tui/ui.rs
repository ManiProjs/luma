use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::{
    theme::LumaTheme,
    tui::{
        app::App,
        components::{chat::ChatView, input::InputBox, status::StatusBar, welcome::WelcomeScreen},
        info::ModelInfo,
    },
};

pub fn draw(
    frame: &mut Frame,
    app: &App,
    theme: &LumaTheme,
    model_info: &ModelInfo,
    tools: &[String],
    confirm_exit: bool,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .split(frame.area());

    /*
        Main content
    */

    if app.welcome_visible {
        let welcome = WelcomeScreen::new(
            theme,
            model_info.provider.clone(),
            model_info.model.clone(),
            tools.to_vec(),
        );

        welcome.render(frame, layout[0]);
    } else {
        let chat = ChatView::new(theme, app);

        chat.render(frame, layout[0]);
    }

    /*
        Input
    */

    let input = InputBox::new(theme, &app.input);

    input.render(frame, layout[1]);

    /*
        Status
    */

    let status = StatusBar::new(
        theme,
        model_info.provider.clone(),
        model_info.model.clone(),
        tools.len(),
        confirm_exit,
        app.thinking,
    );

    status.render(frame, layout[2]);
}
