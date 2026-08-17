use std::{
    io,
    time::{Duration, Instant},
};

use anyhow::Result;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend};

use tokio::sync::mpsc::{Receiver, Sender};

use crate::{
    event::AgentEvent,
    theme::LumaTheme,
    tui::{
        app::App,
        components::{chat::ChatView, input::InputBox, status::StatusBar, welcome::WelcomeScreen},
    },
};

pub async fn run(mut rx: Receiver<AgentEvent>, input_tx: Sender<String>) -> Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let theme = LumaTheme::default();

    let mut confirm_exit = false;

    let mut last_ctrl_c = Instant::now();

    loop {
        terminal.draw(|frame| {
            let size = frame.area();

            let chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([
                    ratatui::layout::Constraint::Min(5),
                    ratatui::layout::Constraint::Length(6),
                    ratatui::layout::Constraint::Length(1),
                ])
                .split(size);

            if app.welcome_visible {
                let welcome = WelcomeScreen::new(
                    &theme,
                    vec![
                        "list_directory".into(),
                        "read_file".into(),
                        "run_command".into(),
                    ],
                );

                welcome.render(frame, chunks[0]);
            } else {
                let chat = ChatView::new(&theme, &app);

                chat.render(frame, chunks[0]);
            }

            let input = InputBox::new(&theme, &app.input);

            input.render(frame, chunks[1]);

            let status = StatusBar::new(&theme, "local-model".into(), 3, confirm_exit);

            status.render(frame, chunks[2]);
        })?;

        while let Ok(event) = rx.try_recv() {
            app.handle_event(event);
        }

        if confirm_exit && last_ctrl_c.elapsed() > Duration::from_secs(3) {
            confirm_exit = false;
        }

        if event::poll(Duration::from_millis(30))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    if confirm_exit {
                        break;
                    }

                    confirm_exit = true;

                    last_ctrl_c = Instant::now();

                    continue;
                }

                match key.code {
                    KeyCode::Char(c) => {
                        app.input.insert(c);
                    }

                    KeyCode::Backspace => {
                        app.input.backspace();
                    }

                    KeyCode::Enter => {
                        if key.modifiers.contains(KeyModifiers::SHIFT) {
                            app.input.newline();
                        } else if let Some(msg) = app.submit_input() {
                            input_tx.send(msg).await?;
                        }
                    }

                    KeyCode::Left => {
                        if app.input.cursor_x > 0 {
                            app.input.cursor_x -= 1;
                        }
                    }

                    KeyCode::Right => {
                        let len = app.input.lines[app.input.cursor_y].len();

                        if app.input.cursor_x < len {
                            app.input.cursor_x += 1;
                        }
                    }

                    KeyCode::Up => {
                        if app.input.cursor_y > 0 {
                            app.input.cursor_y -= 1;
                        }
                    }

                    KeyCode::Down => {
                        if app.input.cursor_y + 1 < app.input.lines.len() {
                            app.input.cursor_y += 1;
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}
