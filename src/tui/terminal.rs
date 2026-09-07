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

const POLL_INTERVAL: Duration = Duration::from_millis(30);
const EXIT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn run(
    mut rx: Receiver<AgentEvent>,
    input_tx: Sender<String>,
    cancel: CancellationToken,
    confirmation_tx: Sender<Confirmation>,
    mut info: LumaInfo,
) -> Result<()> {
    let mut terminal = setup_terminal()?;

    let result = run_loop(
        &mut terminal,
        &mut rx,
        &input_tx,
        &cancel,
        &confirmation_tx,
        &mut info,
    )
    .await;

    restore_terminal(&mut terminal)?;

    result
}

// ============================================================
// Terminal lifecycle
// ============================================================

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }

    let backend = CrosstermBackend::new(stdout);

    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;

    Ok(())
}

// ============================================================
// Main loop
// ============================================================

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: &mut Receiver<AgentEvent>,
    input_tx: &Sender<String>,
    cancel: &CancellationToken,
    confirmation_tx: &Sender<Confirmation>,
    info: &mut LumaInfo,
) -> Result<()> {
    let mut app = App::new();
    let theme = LumaTheme::default();

    let mut confirm_exit = false;
    let mut last_ctrl_c = Instant::now();

    loop {
        draw(terminal, &app, &theme, info, confirm_exit)?;

        // ----------------------------------------------------
        // Agent events
        // ----------------------------------------------------

        if drain_agent_events(rx, &mut app, info) {
            draw(terminal, &app, &theme, info, confirm_exit)?;
        }

        // ----------------------------------------------------
        // Exit confirmation timeout
        // ----------------------------------------------------

        if confirm_exit && last_ctrl_c.elapsed() >= EXIT_CONFIRM_TIMEOUT {
            confirm_exit = false;
        }

        // ----------------------------------------------------
        // Input
        // ----------------------------------------------------

        if !event::poll(POLL_INTERVAL)? {
            continue;
        }

        let event = event::read()?;

        let should_exit = match event {
            Event::Mouse(mouse) => {
                handle_mouse(&mut app, mouse.kind);
                false
            }

            Event::Key(key) => {
                let mut key_ctx = KeyContext {
                    app: &mut app,
                    input_tx,
                    cancel,
                    confirmation_tx,
                    info,
                    confirm_exit: &mut confirm_exit,
                    last_ctrl_c: &mut last_ctrl_c,
                };

                handle_key(&mut key_ctx, key.code, key.modifiers).await?
            }

            _ => false,
        };

        if should_exit {
            break;
        }
    }

    Ok(())
}

// ============================================================
// Rendering
// ============================================================

fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &App,
    theme: &LumaTheme,
    info: &LumaInfo,
    confirm_exit: bool,
) -> Result<()> {
    terminal.draw(|frame| {
        ui::draw(frame, app, theme, info, confirm_exit);
    })?;

    Ok(())
}

// ============================================================
// Agent events
// ============================================================

fn drain_agent_events(rx: &mut Receiver<AgentEvent>, app: &mut App, info: &mut LumaInfo) -> bool {
    let mut received = false;

    while let Ok(event) = rx.try_recv() {
        received = true;

        update_info_status(info, &event);

        app.handle_event(event);

        if app.auto_scroll {
            app.scroll_to_bottom();
        }
    }

    received
}

fn update_info_status(info: &mut LumaInfo, event: &AgentEvent) {
    match event {
        AgentEvent::Thinking => {
            info.set_status("Thinking");
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

        AgentEvent::SystemMessage(_) => {
            info.set_status("Ready");
        }

        AgentEvent::Finished => {
            info.set_status("Ready");
        }

        AgentEvent::Error(_) => {
            info.set_status("Error");
        }
    }
}

// ============================================================
// Mouse
// ============================================================

fn handle_mouse(app: &mut App, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.scroll_up();
        }

        MouseEventKind::ScrollDown => {
            app.scroll_down();
        }

        _ => {}
    }
}

// ============================================================
// Keyboard
// ============================================================

struct KeyContext<'a> {
    app: &'a mut App,
    input_tx: &'a Sender<String>,
    cancel: &'a CancellationToken,
    confirmation_tx: &'a Sender<Confirmation>,
    info: &'a mut LumaInfo,
    confirm_exit: &'a mut bool,
    last_ctrl_c: &'a mut Instant,
}

async fn handle_key(
    ctx: &mut KeyContext<'_>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Result<bool> {
    // --------------------------------------------------------
    // Ctrl+C
    // --------------------------------------------------------

    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        return handle_ctrl_c(
            ctx.app,
            ctx.cancel,
            ctx.info,
            ctx.confirm_exit,
            ctx.last_ctrl_c,
        );
    }

    // --------------------------------------------------------
    // Confirmation mode
    // --------------------------------------------------------

    if ctx.app.confirmation_pending() {
        return handle_confirmation(ctx.app, code, ctx.confirmation_tx, ctx.info).await;
    }

    // --------------------------------------------------------
    // Normal input
    // --------------------------------------------------------

    match code {
        KeyCode::Char(c) => {
            ctx.app.input.insert(c);
            ctx.app.history_index = None;
            ctx.app.update_suggestions();
        }

        KeyCode::Backspace => {
            ctx.app.input.backspace();
            ctx.app.history_index = None;
            ctx.app.update_suggestions();
        }

        KeyCode::Tab => {
            if !ctx.app.suggestions.is_empty() {
                ctx.app.accept_suggestion();
            }
        }

        KeyCode::Up => {
            if ctx.app.suggestions.is_empty() {
                ctx.app.history_up();
            } else {
                ctx.app.suggestion_up();
            }
        }

        KeyCode::Down => {
            if ctx.app.suggestions.is_empty() {
                ctx.app.history_down();
            } else {
                ctx.app.suggestion_down();
            }
        }

        KeyCode::Enter => {
            return handle_enter(ctx.app, modifiers, ctx.input_tx, ctx.info).await;
        }

        KeyCode::Left => {
            move_cursor_left(ctx.app);
        }

        KeyCode::Right => {
            move_cursor_right(ctx.app);
        }

        KeyCode::Home => {
            ctx.app.input.cursor_x = 0;
        }

        KeyCode::End => {
            move_cursor_end(ctx.app);
        }

        KeyCode::PageUp => {
            ctx.app.scroll_up();
        }

        KeyCode::PageDown => {
            ctx.app.scroll_down();
        }

        _ => {}
    }

    Ok(false)
}

// ============================================================
// Ctrl+C
// ============================================================

fn handle_ctrl_c(
    app: &mut App,
    cancel: &CancellationToken,
    info: &mut LumaInfo,
    confirm_exit: &mut bool,
    last_ctrl_c: &mut Instant,
) -> Result<bool> {
    if app.thinking || app.current_tool.is_some() {
        cancel.cancel();

        app.messages.push(MessageLine {
            role: MessageRole::System,
            content: "Generation interrupted.".into(),
        });

        app.thinking = false;
        app.current_tool = None;
        app.confirmation = None;

        info.set_status("Ready");

        return Ok(false);
    }

    if !*confirm_exit {
        *confirm_exit = true;
        *last_ctrl_c = Instant::now();

        return Ok(false);
    }

    Ok(true)
}

// ============================================================
// Confirmation
// ============================================================

async fn handle_confirmation(
    app: &mut App,
    code: KeyCode,
    confirmation_tx: &Sender<Confirmation>,
    info: &mut LumaInfo,
) -> Result<bool> {
    match code {
        KeyCode::Enter => {
            confirmation_tx.send(Confirmation::Allow).await?;

            app.clear_confirmation();
            app.thinking = true;

            info.set_status("Thinking");
        }

        KeyCode::Esc => {
            confirmation_tx.send(Confirmation::Deny).await?;

            app.clear_confirmation();
            app.thinking = true;

            info.set_status("Thinking");
        }

        _ => {}
    }

    Ok(false)
}

// ============================================================
// Enter
// ============================================================

async fn handle_enter(
    app: &mut App,
    modifiers: KeyModifiers,
    input_tx: &Sender<String>,
    info: &mut LumaInfo,
) -> Result<bool> {
    // --------------------------------------------------------
    // Autocomplete
    // --------------------------------------------------------

    if !app.suggestions.is_empty() {
        app.accept_suggestion();
        return Ok(false);
    }

    // --------------------------------------------------------
    // Shift+Enter
    // --------------------------------------------------------

    if modifiers.contains(KeyModifiers::SHIFT) {
        app.input.newline();
        app.update_suggestions();

        return Ok(false);
    }

    // --------------------------------------------------------
    // Submit
    // --------------------------------------------------------

    let Some(message) = app.submit_input() else {
        return Ok(false);
    };

    // --------------------------------------------------------
    // Slash commands
    // --------------------------------------------------------

    if let Some(command) = Command::parse(&message) {
        return handle_command(app, command, input_tx, info).await;
    }

    // --------------------------------------------------------
    // Normal prompt
    // --------------------------------------------------------

    app.thinking = true;
    info.set_status("Thinking");

    input_tx.send(message).await?;

    Ok(false)
}

// ============================================================
// Commands
// ============================================================

async fn handle_command(
    app: &mut App,
    command: Command,
    input_tx: &Sender<String>,
    info: &mut LumaInfo,
) -> Result<bool> {
    match command {
        Command::Help => {
            app.messages.push(MessageLine {
                role: MessageRole::System,
                content: concat!("Commands:\n\n", "/help\n", "/clear\n", "/quit\n", "/init",)
                    .into(),
            });

            app.scroll_to_bottom();
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
            return Ok(true);
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

            app.welcome_visible = false;
            app.thinking = true;
            info.set_status("Thinking");

            input_tx.send(prompt).await?;
        }

        Command::Unknown(name) => {
            app.messages.push(MessageLine {
                role: MessageRole::System,
                content: format!("Unknown command: /{}", name),
            });

            app.scroll_to_bottom();
        }
    }

    Ok(false)
}

// ============================================================
// Cursor movement
// ============================================================

fn move_cursor_left(app: &mut App) {
    if app.input.cursor_x == 0 {
        return;
    }

    let line = &app.input.lines[app.input.cursor_y];

    let mut cursor = app.input.cursor_x.min(line.len());

    cursor -= 1;

    while cursor > 0 && !line.is_char_boundary(cursor) {
        cursor -= 1;
    }

    app.input.cursor_x = cursor;
}

fn move_cursor_right(app: &mut App) {
    let line = &app.input.lines[app.input.cursor_y];

    let cursor = app.input.cursor_x.min(line.len());

    if cursor >= line.len() {
        return;
    }

    let mut next = cursor + 1;

    while next < line.len() && !line.is_char_boundary(next) {
        next += 1;
    }

    app.input.cursor_x = next;
}

fn move_cursor_end(app: &mut App) {
    app.input.cursor_x = app.input.lines[app.input.cursor_y].len();
}
