//! Temporary adapter between the core [`Session`] and the Ratatui front end.
//!
//! This module exists only for the duration of the migration. It holds the
//! state that has not yet been pushed down into the core — the text buffer,
//! cursor, autocomplete, scroll position — and translates terminal keys into
//! [`AgentCommand`]s and core events into `App` updates.
//!
//! When the Ink client lands, this file is deleted along with `src/tui/` and
//! nothing in the core refers to it. The core must not grow a dependency on
//! anything here.

use std::time::{Duration, Instant};

use anyhow::Result;

use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use tokio::sync::mpsc::Sender;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::{
    event::AgentEvent,
    session::{AgentCommand, MessageRole, Session},
    tui::{app::App, terminal},
};

const POLL_INTERVAL: Duration = Duration::from_millis(30);
const EXIT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(3);

/// Front-end state that the core does not own.
struct Front {
    app: App,
    confirm_exit: bool,
    last_ctrl_c: Instant,
}

impl Front {
    fn new() -> Self {
        Self {
            app: App::new(),
            confirm_exit: false,
            last_ctrl_c: Instant::now(),
        }
    }
}

/// Run the terminal front end until the user exits.
pub async fn run(
    session: &mut Session,
    events: &mut broadcast::Receiver<AgentEvent>,
    input_tx: Sender<String>,
    cancel: CancellationToken,
) -> Result<()> {
    let mut front = Front::new();

    terminal::run(move |app| {
        sync_front(&mut front.app, session);
    })
    .await
    .inspect(|()| input_tx.send(String::new()).ok())?;

    Ok(())
}

/// Fold the core's view of the session into the front end's own state.
///
/// The two overlap on purpose during the migration: `App` still owns the
/// transcript until the Ink work lands, and this keeps it in step with the
/// core so that `App` is never the source of truth for a command the core
/// also handles.
fn sync_front(app: &mut App, session: &Session) {
    let state = session.state();

    app.thinking = state.busy;
    app.status = state.status.clone();
    app.confirmation = state.confirmation.clone();
    app.messages = state
        .messages
        .iter()
        .map(|message| crate::tui::app::MessageLine {
            role: to_role(message.role),
            content: message.content.clone(),
        })
        .collect();
}

fn to_role(role: MessageRole) -> crate::tui::app::MessageRole {
    match role {
        MessageRole::User => crate::tui::app::MessageRole::User,
        MessageRole::Assistant => crate::tui::app::MessageRole::Assistant,
        MessageRole::Tool => crate::tui::app::MessageRole::Tool,
        MessageRole::Plan => crate::tui::app::MessageRole::Plan,
        MessageRole::System => crate::tui::app::MessageRole::System,
        MessageRole::Error => crate::tui::app::MessageRole::Error,
    }
}

// ============================================================
// Event folding
// ============================================================

/// Drain the broadcast channel into the core session.
///
/// Returns whether anything arrived, so the caller can redraw.
pub fn drain_events(
    events: &mut broadcast::Receiver<AgentEvent>,
    session: &mut Session,
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

    received
}

// ============================================================
// Input
// ============================================================

/// Translate one terminal event into session commands.
///
/// Returns `true` when the user asked to exit.
pub async fn handle_event(
    session: &mut Session,
    front: &mut Front,
    event: &Event,
    input_tx: &Sender<String>,
    cancel: &CancellationToken,
) -> Result<bool> {
    match event {
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => front.app.scroll_up(),
                MouseEventKind::ScrollDown => front.app.scroll_down(),
                _ => {}
            }
        }

        Event::Key(key) => {
            return handle_key(session, front, key, input_tx, cancel).await;
        }

        _ => {}
    }

    Ok(false)
}

async fn handle_key(
    session: &mut Session,
    front: &mut Front,
    key: &event::KeyEvent,
    input_tx: &Sender<String>,
    cancel: &CancellationToken,
) -> Result<bool> {
    let app = &mut front.app;

    // --------------------------------------------------------
    // Ctrl+C: cancel a running request, then confirm exit
    // --------------------------------------------------------

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if session.state().busy {
            session.apply(AgentCommand::Cancel).await?;

            return Ok(false);
        }

        if !front.confirm_exit {
            front.confirm_exit = true;
            front.last_ctrl_c = Instant::now();

            return Ok(false);
        }

        return Ok(true);
    }

    if front.confirm_exit && front.last_ctrl_c.elapsed() >= EXIT_CONFIRM_TIMEOUT {
        front.confirm_exit = false;
    }

    // --------------------------------------------------------
    // A pending confirmation swallows every other key
    // --------------------------------------------------------

    if session.state().confirmation.is_some() {
        return match key.code {
            KeyCode::Enter => {
                session.apply(AgentCommand::Confirm { allowed: true }).await?;

                Ok(false)
            }

            KeyCode::Esc => {
                session.apply(AgentCommand::Confirm { allowed: false }).await?;

                Ok(false)
            }

            _ => Ok(false),
        };
    }

    // --------------------------------------------------------
    // Input
    // --------------------------------------------------------

    match key.code {
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

        KeyCode::Left => move_cursor_left(app),

        KeyCode::Right => move_cursor_right(app),

        KeyCode::Home => app.input.cursor_x = 0,

        KeyCode::End => move_cursor_end(app),

        KeyCode::PageUp => app.scroll_up(),

        KeyCode::PageDown => app.scroll_down(),

        KeyCode::Enter => {
            return handle_enter(session, front, key.modifiers, input_tx, cancel).await;
        }

        _ => {}
    }

    Ok(false)
}

async fn handle_enter(
    session: &mut Session,
    front: &mut Front,
    modifiers: KeyModifiers,
    input_tx: &Sender<String>,
    cancel: &CancellationToken,
) -> Result<bool> {
    let app = &mut front.app;

    // Autocomplete takes precedence over sending.
    if !app.suggestions.is_empty() {
        app.accept_suggestion();

        return Ok(false);
    }

    if modifiers.contains(KeyModifiers::SHIFT) {
        app.input.newline();
        app.update_suggestions();

        return Ok(false);
    }

    let Some(text) = app.submit_input() else {
        return Ok(false);
    };

    if let Some(forwarded) = session.apply(AgentCommand::Submit(text)).await? {
        input_tx.send(forwarded).await?;

        let _ = cancel;
    }

    // A locally handled command (`/quit`) never reaches the core, so the
    // front end has to notice it here.
    Ok(app.should_exit)
}

// ============================================================
// Cursor
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
