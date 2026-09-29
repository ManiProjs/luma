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

use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::{
    event::AgentEvent,
    session::{AgentCommand, MessageRole, Session},
    theme::LumaTheme,
    tui::{
        app::{App, MessageLine},
        info::LumaInfo,
        ui,
    },
};

const POLL_INTERVAL: Duration = Duration::from_millis(30);
const EXIT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn run(
    session: &mut Session,
    events: &mut broadcast::Receiver<AgentEvent>,
    input_tx: Sender<String>,
    cancel: CancellationToken,
) -> Result<()> {
    let mut terminal = setup_terminal()?;

    let mut app = App::new();

    let mut info = LumaInfo::new(session.info());

    let result = run_loop(
        &mut terminal,
        session,
        events,
        &mut app,
        &mut info,
        &input_tx,
    )
    .await;

    restore_terminal(&mut terminal)?;

    // The loop only stops on an explicit exit, but a `/quit` or a cancelled
    // request anywhere else still has to tear the agent down.
    cancel.cancel();

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
    session: &mut Session,
    events: &mut broadcast::Receiver<AgentEvent>,
    app: &mut App,
    info: &mut LumaInfo,
    input_tx: &Sender<String>,
) -> Result<()> {
    let theme = LumaTheme::default();

    let mut confirm_exit = false;
    let mut last_ctrl_c = Instant::now();

    loop {
        draw(terminal, app, &theme, info, confirm_exit)?;

        // ----------------------------------------------------
        // Agent events
        // ----------------------------------------------------

        if drain_agent_events(events, session, app, info) {
            draw(terminal, app, &theme, info, confirm_exit)?;
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

        // Ctrl+C twice always exits, even if the session never saw a command.
        if session.exit_requested() {
            break;
        }

        let should_exit = match &event {
            Event::Mouse(mouse) => {
                handle_mouse(session, app, info, mouse.kind);
                false
            }

            Event::Key(key) => {
                handle_key(
                    session,
                    app,
                    info,
                    key.code,
                    key.modifiers,
                    input_tx,
                    &mut confirm_exit,
                    &mut last_ctrl_c,
                )
                .await?
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

/// Fold incoming agent events into the core session, then mirror the core's
/// state into the front end's `App`.
///
/// `App` still owns the transcript it draws; the session owns the transcript
/// it reasons about. They are the same list, and this is the one place the
/// copy is refreshed.
fn drain_agent_events(
    events: &mut broadcast::Receiver<AgentEvent>,
    session: &mut Session,
    app: &mut App,
    info: &mut LumaInfo,
) -> bool {
    let mut received = false;

    loop {
        match events.try_recv() {
            Ok(event) => {
                session.handle_event(event);
                received = true;
            }

            Err(broadcast::error::TryRecvError::Empty) => break,

            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "terminal event receiver lagged");
                received = true;
            }

            Err(broadcast::error::TryRecvError::Closed) => break,
        }
    }

    if received {
        sync_from_session(session, app, info);
    }

    received
}

/// Copy the core's state into the front end.
fn sync_from_session(session: &Session, app: &mut App, info: &mut LumaInfo) {
    let state = session.state();

    app.thinking = state.busy;
    app.status = state.status.clone();
    app.welcome_visible = state.fresh;
    app.confirmation =
        state
            .confirmation
            .clone()
            .map(|confirmation| crate::tui::app::PendingConfirmation {
                name: confirmation.name,
                input: confirmation.input,
            });

    app.messages = state
        .messages
        .iter()
        .map(|message| MessageLine {
            role: match message.role {
                MessageRole::User => crate::tui::app::MessageRole::User,
                MessageRole::Assistant => crate::tui::app::MessageRole::Assistant,
                MessageRole::Tool => crate::tui::app::MessageRole::Tool,
                MessageRole::Plan => crate::tui::app::MessageRole::Plan,
                MessageRole::System => crate::tui::app::MessageRole::System,
                MessageRole::Error => crate::tui::app::MessageRole::Error,
            },
            content: message.content.clone(),
        })
        .collect();

    // The tool panel tracks the most recent tool entry, running or finished,
    // but only for as long as the turn is alive — once the agent reports
    // `Finished` the transcript is the only place the tool is shown.
    app.current_tool = match state.busy {
        true => state
            .messages
            .iter()
            .rev()
            .find(|message| message.tool.is_some())
            .map(|message| crate::tui::app::ToolState {
                name: message.tool.clone().unwrap_or_default(),
                input: message.content.clone(),
                status: match (message.running, message.succeeded) {
                    (_, Some(true)) => crate::tui::app::ToolStatus::Success,
                    (_, Some(false)) => crate::tui::app::ToolStatus::Failed,
                    _ => crate::tui::app::ToolStatus::Running,
                },
            }),

        false => None,
    };

    info.sync(session.info(), state);
}

// ============================================================
// Mouse
// ============================================================

fn handle_mouse(session: &Session, app: &mut App, info: &mut LumaInfo, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => {
            app.scroll_up();
        }

        MouseEventKind::ScrollDown => {
            app.scroll_down();
        }

        // Scrolling is a purely local view change, but it clears the
        // session's echo of the status line the header reads.
        _ => {
            info.sync(session.info(), session.state());
        }
    }
}

// ============================================================
// Keyboard
// ============================================================

#[allow(clippy::too_many_arguments)]
async fn handle_key(
    session: &mut Session,
    app: &mut App,
    info: &mut LumaInfo,
    code: KeyCode,
    modifiers: KeyModifiers,
    input_tx: &Sender<String>,
    confirm_exit: &mut bool,
    last_ctrl_c: &mut Instant,
) -> Result<bool> {
    // --------------------------------------------------------
    // Ctrl+C: cancel a running request, otherwise arm the exit
    // --------------------------------------------------------

    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        if session.state().busy {
            session.apply(AgentCommand::Cancel).await?;

            sync_from_session(session, app, info);

            return Ok(false);
        }

        if !*confirm_exit {
            *confirm_exit = true;
            *last_ctrl_c = Instant::now();

            return Ok(false);
        }

        return Ok(true);
    }

    // --------------------------------------------------------
    // A pending confirmation swallows every other key
    // --------------------------------------------------------

    if session.state().confirmation.is_some() {
        return match code {
            KeyCode::Enter => {
                session
                    .apply(AgentCommand::Confirm { allowed: true })
                    .await?;

                Ok(false)
            }

            KeyCode::Esc => {
                session
                    .apply(AgentCommand::Confirm { allowed: false })
                    .await?;

                Ok(false)
            }

            _ => Ok(false),
        };
    }

    // --------------------------------------------------------
    // Normal input
    // --------------------------------------------------------

    match code {
        KeyCode::Char(c) => {
            app.input.insert(c);
            app.history_index = None;
            app.update_suggestions();
        }

        KeyCode::Backspace => {
            app.input.backspace();
            app.history_index = None;
            app.update_suggestions();
        }

        KeyCode::Tab => {
            if !app.suggestions.is_empty() {
                app.accept_suggestion();
            }
        }

        KeyCode::Up => {
            if app.suggestions.is_empty() {
                app.history_up();
            } else {
                app.suggestion_up();
            }
        }

        KeyCode::Down => {
            if app.suggestions.is_empty() {
                app.history_down();
            } else {
                app.suggestion_down();
            }
        }

        KeyCode::Left => {
            move_cursor_left(app);
        }

        KeyCode::Right => {
            move_cursor_right(app);
        }

        KeyCode::Home => {
            app.input.cursor_x = 0;
        }

        KeyCode::End => {
            move_cursor_end(app);
        }

        KeyCode::PageUp => {
            app.scroll_up();
        }

        KeyCode::PageDown => {
            app.scroll_down();
        }

        KeyCode::Enter => {
            return handle_enter(session, app, info, modifiers, input_tx).await;
        }

        _ => {}
    }

    Ok(false)
}

// ============================================================
// Enter
// ============================================================

async fn handle_enter(
    session: &mut Session,
    app: &mut App,
    info: &mut LumaInfo,
    modifiers: KeyModifiers,
    input_tx: &Sender<String>,
) -> Result<bool> {
    // --------------------------------------------------------
    // Autocomplete takes precedence over sending
    // --------------------------------------------------------

    if !app.suggestions.is_empty() {
        app.accept_suggestion();

        return Ok(false);
    }

    // --------------------------------------------------------
    // Shift+Enter is a newline
    // --------------------------------------------------------

    if modifiers.contains(KeyModifiers::SHIFT) {
        app.input.newline();
        app.update_suggestions();

        return Ok(false);
    }

    // --------------------------------------------------------
    // Submit
    // --------------------------------------------------------

    let Some(text) = app.submit_input() else {
        return Ok(false);
    };

    // The session owns slash-command handling now: it resolves `/help`,
    // `/clear` and `/quit` locally, and forwards `/init` to the agent as a
    // real request.
    match session.apply(AgentCommand::Submit(text)).await? {
        Some(forwarded) => input_tx.send(forwarded).await?,

        None => sync_from_session(session, app, info),
    }

    Ok(session.exit_requested())
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
