use std::{
    io,
    time::{Duration, Instant},
};

use anyhow::Result;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{Terminal, backend::CrosstermBackend};

use tokio::sync::mpsc::{Receiver, Sender};

use tokio_util::sync::CancellationToken;

use crate::{
    event::AgentEvent,
    theme::LumaTheme,
    tui::{
        app::{App, MessageLine, MessageRole},
        info::LumaInfo,
        ui,
    },
};

pub async fn run(
    mut rx: Receiver<AgentEvent>,
    input_tx: Sender<String>,
    cancel: CancellationToken,
    mut info: LumaInfo,
) -> Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let theme = LumaTheme::default();

    let mut confirm_exit = false;

    let mut last_ctrl_c = Instant::now();

    loop {
        terminal.draw(|frame| {
            ui::draw(frame, &app, &theme, &info, confirm_exit);
        })?;

        while let Ok(agent_event) = rx.try_recv() {
            match &agent_event {
                AgentEvent::Thinking => {
                    info.set_status("Thinking");
                }

                AgentEvent::ToolStarted { name, .. } => {
                    info.set_status(format!("Running {}", name));
                }

                AgentEvent::Finished => {
                    info.set_status("Ready");
                }

                AgentEvent::Error(_) => {
                    info.set_status("Error");
                }

                _ => {}
            }

            app.handle_event(agent_event);

            if app.auto_scroll {
                app.scroll_to_bottom();
            }
        }

        if confirm_exit && last_ctrl_c.elapsed() > Duration::from_secs(3) {
            confirm_exit = false;
        }

        if event::poll(Duration::from_millis(30))? {
            match event::read()? {
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        app.scroll_up();
                    }

                    MouseEventKind::ScrollDown => {
                        app.scroll_down();
                    }

                    _ => {}
                },

                Event::Key(key) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        if app.thinking {
                            cancel.cancel();

                            app.messages.push(MessageLine {
                                role: MessageRole::System,

                                content: "Generation interrupted.".into(),
                            });

                            app.thinking = false;

                            continue;
                        }

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
                            } else if let Some(message) = app.submit_input() {
                                input_tx.send(message).await?;
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
                            app.history_up();
                        }

                        KeyCode::Down => {
                            app.history_down();
                        }

                        _ => {}
                    }
                }

                _ => {}
            }
        }
    }

    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;

    Ok(())
}
