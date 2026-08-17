use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::{
    theme::LumaTheme,
    tui::{
        app::App,
        components::{chat::ChatView, input::InputBox, status::StatusBar, welcome::WelcomeScreen},
    },
};

pub fn draw(frame: &mut Frame, app: &App, theme: &LumaTheme) {
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
            vec![
                "list_directory".to_string(),
                "read_file".to_string(),
                "run_command".to_string(),
            ],
        );

        welcome.render(frame, layout[0]);
    } else {
        let chat = ChatView::new(theme, app);

        chat.render(frame, layout[0]);
    }

    /*
        Input area
    */

    let input = InputBox::new(theme, &app.input);

    input.render(frame, layout[1]);

    /*
        Bottom status
    */

    let status = StatusBar::new(theme, "local-model".to_string(), 3, false);

    status.render(frame, layout[2]);
}
