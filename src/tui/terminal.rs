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
        app::{App, MessageRole},
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

        // --------------------------------------------------------
        // Agent events
        // --------------------------------------------------------

        let mut received_event = false;

        while let Ok(agent_event) = rx.try_recv() {
            received_event = true;

            // Update the status bar separately from App's event handling.
            match &agent_event {
                AgentEvent::Thinking => {
                    info.set_status("Thinking");
                }

                AgentEvent::Planning => {
                    info.set_status("Planning");
                }

                AgentEvent::PlanGenerated(_) => {
                    info.set_status("Thinking");
                }

                AgentEvent::ToolStarted { name, .. } => {
                    info.set_status(format!("Running {}", name));
                }

                AgentEvent::ToolFinished { .. } => {
                    info.set_status("Thinking");
                }

                AgentEvent::ConfirmationRequired { .. } => {
                    info.set_status("Confirmation required");
                }

                AgentEvent::TextDelta(_) => {
                    info.set_status("Generating");
                }

                AgentEvent::Finished => {
                    info.set_status("Ready");
                }

                AgentEvent::Error(_) => {
                    info.set_status("Error");
                }

                AgentEvent::SystemMessage(_) => {
                    info.set_status("Ready");
                }

                AgentEvent::Debug(_) => {
                    // Keep the existing status unchanged.
                }
            }

            // App owns the actual conversation state.
            app.handle_event(agent_event);

            if app.auto_scroll {
                app.scroll_to_bottom();
            }
        }

        if received_event {
            terminal.draw(|frame| {
                ui::draw(frame, &app, &theme, &info, confirm_exit);
            })?;
        }

        // --------------------------------------------------------
        // Exit confirmation timeout
        // --------------------------------------------------------

        if confirm_exit && last_ctrl_c.elapsed() > Duration::from_secs(3) {
            confirm_exit = false;
        }

        // --------------------------------------------------------
        // Keyboard / mouse input
        // --------------------------------------------------------

        if event::poll(Duration::from_millis(30))? {
            match event::read()? {
                // ------------------------------------------------
                // Mouse
                // ------------------------------------------------
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        app.scroll_up();
                    }

                    MouseEventKind::ScrollDown => {
                        app.scroll_down();
                    }

                    _ => {}
                },

                // ------------------------------------------------
                // Keyboard
                // ------------------------------------------------
                Event::Key(key) => {
                    // ------------------------------------------------
                    // Ctrl+C
                    // ------------------------------------------------

                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        // If the agent is currently working, cancel it.
                        if app.thinking {
                            cancel.cancel();

                            app.messages.push(crate::tui::app::MessageLine {
                                role: MessageRole::System,
                                content: "Generation interrupted.".into(),
                            });

                            app.thinking = false;
                            app.current_tool = None;

                            info.set_status("Ready");

                            continue;
                        }

                        // First Ctrl+C arms exit confirmation.
                        if !confirm_exit {
                            confirm_exit = true;
                            last_ctrl_c = Instant::now();

                            continue;
                        }

                        // Second Ctrl+C exits.
                        break;
                    }

                    // ------------------------------------------------
                    // Tool confirmation
                    // ------------------------------------------------

                    if app.confirmation_pending() {
                        match key.code {
                            KeyCode::Enter => {
                                confirmation_tx.send(Confirmation::Allow).await?;

                                app.clear_confirmation();
                                app.thinking = true;

                                info.set_status("Thinking");

                                continue;
                            }

                            KeyCode::Esc => {
                                confirmation_tx.send(Confirmation::Deny).await?;

                                app.clear_confirmation();
                                app.thinking = true;

                                info.set_status("Thinking");

                                continue;
                            }

                            _ => {
                                // Ignore normal input while confirmation
                                // is waiting for a decision.
                                continue;
                            }
                        }
                    }

                    // ------------------------------------------------
                    // Normal input
                    // ------------------------------------------------

                    match key.code {
                        // --------------------------------------------
                        // Character input
                        // --------------------------------------------
                        KeyCode::Char(c) => {
                            app.input.insert(c);
                            app.history_index = None;
                            app.update_suggestions();
                        }

                        // --------------------------------------------
                        // Backspace
                        // --------------------------------------------
                        KeyCode::Backspace => {
                            app.input.backspace();
                            app.history_index = None;
                            app.update_suggestions();
                        }

                        // --------------------------------------------
                        // Tab
                        // --------------------------------------------
                        KeyCode::Tab => {
                            if !app.suggestions.is_empty() {
                                app.accept_suggestion();
                            }
                        }

                        // --------------------------------------------
                        // Up
                        // --------------------------------------------
                        KeyCode::Up => {
                            if !app.suggestions.is_empty() {
                                app.suggestion_up();
                            } else {
                                app.history_up();
                            }
                        }

                        // --------------------------------------------
                        // Down
                        // --------------------------------------------
                        KeyCode::Down => {
                            if !app.suggestions.is_empty() {
                                app.suggestion_down();
                            } else {
                                app.history_down();
                            }
                        }

                        // --------------------------------------------
                        // Enter
                        // --------------------------------------------
                        KeyCode::Enter => {
                            // Accept autocomplete first.
                            if !app.suggestions.is_empty() {
                                app.accept_suggestion();
                                continue;
                            }

                            // Shift+Enter inserts a newline.
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                app.input.newline();
                                app.update_suggestions();
                                continue;
                            }

                            let Some(message) = app.submit_input() else {
                                continue;
                            };

                            // ----------------------------------------
                            // Slash commands
                            // ----------------------------------------

                            if let Some(command) = Command::parse(&message) {
                                match command {
                                    Command::Help => {
                                        app.messages.push(crate::tui::app::MessageLine {
                                            role: MessageRole::System,
                                            content: concat!(
                                                "Commands:\n\n",
                                                "/help\n",
                                                "/clear\n",
                                                "/quit\n",
                                                "/init"
                                            )
                                            .into(),
                                        });

                                        if app.auto_scroll {
                                            app.scroll_to_bottom();
                                        }
                                    }

                                    Command::Clear => {
                                        app.messages.clear();
                                        app.current_tool = None;
                                        app.confirmation = None;
                                        app.thinking = false;
                                        app.scroll = 0;
                                        app.auto_scroll = true;
                                        app.welcome_visible = true;

                                        info.set_status("Ready");
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

                                        app.messages.push(crate::tui::app::MessageLine {
                                            role: MessageRole::User,
                                            content: prompt.clone(),
                                        });

                                        app.thinking = true;
                                        info.set_status("Thinking");

                                        input_tx.send(prompt).await?;
                                    }

                                    Command::Unknown(name) => {
                                        app.messages.push(crate::tui::app::MessageLine {
                                            role: MessageRole::System,
                                            content: format!("Unknown command: /{}", name),
                                        });

                                        if app.auto_scroll {
                                            app.scroll_to_bottom();
                                        }
                                    }
                                }

                                continue;
                            }

                            // ----------------------------------------
                            // Normal agent prompt
                            // ----------------------------------------

                            app.thinking = true;
                            info.set_status("Thinking");

                            input_tx.send(message).await?;
                        }

                        // --------------------------------------------
                        // Left
                        // --------------------------------------------
                        KeyCode::Left => {
                            if app.input.cursor_x > 0 {
                                app.input.cursor_x -= 1;
                            }
                        }

                        // --------------------------------------------
                        // Right
                        // --------------------------------------------
                        KeyCode::Right => {
                            let len = app.input.lines[app.input.cursor_y].len();

                            if app.input.cursor_x < len {
                                app.input.cursor_x += 1;
                            }
                        }

                        // --------------------------------------------
                        // Home
                        // --------------------------------------------
                        KeyCode::Home => {
                            app.input.cursor_x = 0;
                        }

                        // --------------------------------------------
                        // End
                        // --------------------------------------------
                        KeyCode::End => {
                            app.input.cursor_x = app.input.lines[app.input.cursor_y].len();
                        }

                        // --------------------------------------------
                        // Page Up
                        // --------------------------------------------
                        KeyCode::PageUp => {
                            app.scroll_up();
                        }

                        // --------------------------------------------
                        // Page Down
                        // --------------------------------------------
                        KeyCode::PageDown => {
                            app.scroll_down();
                        }

                        _ => {}
                    }
                }

                _ => {}
            }
        }
    }

    // ------------------------------------------------------------
    // Restore terminal
    // ------------------------------------------------------------

    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;

    Ok(())
}
