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
    agent::Confirmation,
    commands::Command,
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
    confirmation_tx: Sender<Confirmation>,
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

        let mut received_event = false;

        while let Ok(agent_event) = rx.try_recv() {
            received_event = true;

            match &agent_event {
                AgentEvent::Thinking => {
                    info.set_status("Thinking");
                }

                AgentEvent::ToolStarted { name, .. } => {
                    info.set_status(format!("Running {}", name));
                }

                AgentEvent::ConfirmationRequired { name, input } => {
                    info.set_status("Confirmation required");

                    app.messages.push(MessageLine {
                        role: MessageRole::System,
                        content: format!(
                            "Tool `{}` wants to run:\n\n{}\n\n\
                             Press Enter to allow or Esc to deny.",
                            name, input
                        ),
                    });

                    app.thinking = false;
                }

                AgentEvent::ToolFinished { .. } => {
                    info.set_status("Thinking");
                }

                AgentEvent::Finished => {
                    info.set_status("Ready");
                    app.thinking = false;
                }

                AgentEvent::Error(error) => {
                    info.set_status("Error");

                    app.messages.push(MessageLine {
                        role: MessageRole::System,
                        content: error.clone(),
                    });

                    app.thinking = false;
                }

                AgentEvent::SystemMessage(message) => {
                    info.set_status("Ready");

                    app.messages.push(MessageLine {
                        role: MessageRole::System,
                        content: message.clone(),
                    });
                }

                _ => {}
            }

            app.handle_event(agent_event);

            if app.auto_scroll {
                app.scroll_to_bottom();
            }
        }

        /*
         * Confirmation responses are sent by the TUI only when the
         * user explicitly chooses Allow/Deny.
         *
         * The receiver exists here so the channel remains part of
         * the terminal session, but the actual confirmation request
         * is represented by AgentEvent::ConfirmationRequired.
         */
        if received_event {
            terminal.draw(|frame| {
                ui::draw(frame, &app, &theme, &info, confirm_exit);
            })?;
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
                    /*
                     * Ctrl+C
                     *
                     * First press while the agent is working:
                     * cancel the current generation.
                     *
                     * Otherwise:
                     * first Ctrl+C arms exit confirmation.
                     * second Ctrl+C exits.
                     */
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

                    /*
                     * If the last event was ConfirmationRequired,
                     * Enter/Esc can be used to answer it.
                     */
                    if matches!(
                        app.messages.last(),
                        Some(MessageLine {
                            role: MessageRole::System,
                            content,
                        }) if content.contains("Press Enter to allow")
                            && content.contains("Esc to deny")
                    ) {
                        match key.code {
                            KeyCode::Enter => {
                                confirmation_tx.send(Confirmation::Allow).await?;

                                app.messages.push(MessageLine {
                                    role: MessageRole::System,
                                    content: "Allowed.".into(),
                                });

                                app.thinking = true;

                                continue;
                            }

                            KeyCode::Esc => {
                                confirmation_tx.send(Confirmation::Deny).await?;

                                app.messages.push(MessageLine {
                                    role: MessageRole::System,
                                    content: "Denied.".into(),
                                });

                                app.thinking = true;

                                continue;
                            }

                            _ => {}
                        }
                    }

                    match key.code {
                        KeyCode::Char(c) => {
                            app.input.insert(c);
                            app.update_suggestions();
                        }

                        KeyCode::Backspace => {
                            app.input.backspace();
                            app.update_suggestions();
                        }

                        KeyCode::Tab => {
                            app.accept_suggestion();
                        }

                        KeyCode::Up => {
                            if !app.suggestions.is_empty() {
                                app.suggestion_up();
                            } else {
                                app.history_up();
                            }
                        }

                        KeyCode::Down => {
                            if !app.suggestions.is_empty() {
                                app.suggestion_down();
                            } else {
                                app.history_down();
                            }
                        }

                        KeyCode::Enter => {
                            if !app.suggestions.is_empty() {
                                app.accept_suggestion();
                                continue;
                            }

                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                app.input.newline();
                                app.update_suggestions();
                                continue;
                            }

                            if let Some(message) = app.submit_input() {
                                if let Some(command) = Command::parse(&message) {
                                    match command {
                                        Command::Help => {
                                            app.messages.push(MessageLine {
                                                role: MessageRole::System,
                                                content: "Commands:\n\n/help\n/clear\n/quit".into(),
                                            });
                                        }

                                        Command::Clear => {
                                            app.messages.clear();
                                            app.welcome_visible = true;
                                        }

                                        Command::Quit => {
                                            break;
                                        }

                                        Command::Init => {
                                            let prompt = r#"Initialize this workspace.

Tasks:
1. Inspect the project files using available tools.
2. Detect the programming language and framework.
3. Create or update GALAXY.md with:
   - Project name
   - Language
   - Framework
   - Important files
   - Project structure
   - Notes for future sessions
4. Do not explain files.
5. Do not summarize code.
6. Use tools whenever possible.
7. After finishing, reply exactly:

Workspace initialized."#
                                                .to_string();

                                            app.messages.push(MessageLine {
                                                role: MessageRole::User,
                                                content: prompt.clone(),
                                            });

                                            input_tx.send(prompt).await?;
                                        }

                                        Command::Unknown(name) => {
                                            app.messages.push(MessageLine {
                                                role: MessageRole::System,
                                                content: format!("Unknown command: /{}", name),
                                            });
                                        }
                                    }
                                } else {
                                    app.thinking = true;

                                    input_tx.send(message).await?;
                                }
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
