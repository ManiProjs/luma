//! UI-agnostic session layer.
//!
//! Everything a front end needs in order to present Luma lives here: the
//! transcript it renders, the commands it can send, and the events it
//! receives. No terminal, no Ratatui, no Ink — just data and routing.
//!
//! The Ratatui TUI was one client of this module; the Ink client will be
//! another, and the desktop JSONL server a third. They all speak the same
//! [`AgentCommand`] / [`AgentEvent`] pair.

use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::agent::Confirmation;
use crate::commands;
use crate::event::AgentEvent;

// ============================================================================
// Channels
// ============================================================================

pub type ConfirmationSender = Sender<Confirmation>;

// ============================================================================
// Commands (client -> core)
// ============================================================================

/// Everything a front end can ask the session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCommand {
    /// Submit a line of user input. May be a slash command.
    Submit(String),

    /// Answer a pending [`AgentEvent::ConfirmationRequired`].
    Confirm { allowed: bool },

    /// Cancel the request currently in flight. The session stays usable and
    /// the agent loop keeps running.
    Cancel,

    /// Restore the transcript to `index`, dropping everything after it, so
    /// the user can edit what they said and try a different approach.
    Rollback { index: usize },

    /// Expand or collapse the transcript entry at `index`. The session tracks
    /// which entries are expanded; the client decides how to render them.
    ToggleExpanded { index: usize },
}

// ============================================================================
// Transcript
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    Plan,
    System,
    Error,
}

/// One renderable entry in the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageLine {
    pub role: MessageRole,
    pub content: String,

    /// Set for tool entries. Lets a finished tool be updated in place
    /// instead of appending a second line.
    pub tool: Option<String>,

    /// Whether a tool is still running. Drives the client's elapsed timer.
    pub running: bool,

    /// How a tool entry ended. `None` for everything that is not a tool,
    /// and for a tool that is still running.
    pub succeeded: Option<bool>,

    /// Collapsed by default; the client expands long tool output on demand.
    pub expanded: bool,

    /// Wall-clock start, kept out of the transcript itself so a client can
    /// re-render a live timer without the session re-sending the line.
    started_at: Option<Instant>,
}

impl MessageLine {
    fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            tool: None,
            running: false,
            succeeded: None,
            expanded: false,
            started_at: None,
        }
    }

    fn tool(name: String, input: String) -> Self {
        Self {
            tool: Some(name),
            running: true,
            started_at: Some(Instant::now()),
            ..Self::new(MessageRole::Tool, input)
        }
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|started| started.elapsed())
    }

    fn view(&self) -> MessageView {
        MessageView::from(self)
    }
}

// ============================================================================
// Wire view
// ============================================================================

/// A [`MessageLine`] as it crosses the process boundary.
///
/// The live line carries an `Instant`, which cannot be serialized; the view
/// carries the elapsed milliseconds that were true when the snapshot was
/// taken instead.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MessageView {
    pub role: MessageRole,
    pub content: String,
    pub tool: Option<String>,
    pub running: bool,
    pub succeeded: Option<bool>,
    pub expanded: bool,
    pub elapsed_ms: Option<u64>,
}

impl From<&MessageLine> for MessageView {
    fn from(line: &MessageLine) -> Self {
        Self {
            role: line.role,
            content: line.content.clone(),
            tool: line.tool.clone(),
            running: line.running,
            succeeded: line.succeeded,
            expanded: line.expanded,
            elapsed_ms: line.elapsed().map(|elapsed| elapsed.as_millis() as u64),
        }
    }
}

/// A serializable snapshot of an entire session.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SessionView {
    pub info: SessionInfo,
    pub messages: Vec<MessageView>,
    pub confirmation: Option<PendingConfirmation>,
    pub busy: bool,
    pub fresh: bool,
    pub status: String,
    pub usage: UsageStats,
    pub elapsed_ms: Option<u64>,
}

// ============================================================================
// Pending confirmation
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingConfirmation {
    pub name: String,
    pub input: String,
}

// ============================================================================
// Session info
// ============================================================================

/// Token and cost totals for the session so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsageStats {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,

    /// Cost in millionths of a dollar, to keep it exactly representable.
    pub cost_usd_micros: u64,
}

impl UsageStats {
    pub fn add(&mut self, prompt: u64, completion: u64, total: u64, cost_usd: f64) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_tokens += total;
        self.cost_usd_micros += (cost_usd * 1_000_000.0).round() as u64;
    }

    pub fn cost_usd(&self) -> f64 {
        self.cost_usd_micros as f64 / 1_000_000.0
    }
}

/// What a client needs to render its header and status line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionInfo {
    pub provider: String,
    pub model: String,
    pub workspace: String,
    pub tools: Vec<String>,
}

impl SessionInfo {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        workspace: impl Into<String>,
        tools: Vec<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            workspace: workspace.into(),
            tools,
        }
    }
}

// ============================================================================
// Session state
// ============================================================================

/// The full client-visible state.
///
/// Not serializable on purpose: `MessageLine` carries an `Instant`, and
/// [`SessionView`] is the projection that crosses a process boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionState {
    pub messages: Vec<MessageLine>,
    pub confirmation: Option<PendingConfirmation>,

    /// True from the moment a prompt is submitted until the agent reports
    /// `Finished`.
    pub busy: bool,

    /// True until the first real turn, so a client can show its own splash
    /// instead of an empty transcript.
    pub fresh: bool,

    pub status: String,
    pub usage: UsageStats,
    pub input_history: Vec<String>,
}

const DEFAULT_STATUS: &str = "Ready";
const BUSY_STATUS: &str = "Working";

impl Default for SessionState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            confirmation: None,
            busy: false,
            fresh: true,
            status: DEFAULT_STATUS.into(),
            usage: UsageStats::default(),
            input_history: Vec::new(),
        }
    }
}

// ============================================================================
// Session
// ============================================================================

/// Owns the transcript and routes commands and events between the agent and
/// a front end.
pub struct Session {
    info: SessionInfo,
    state: SessionState,
    confirm_tx: ConfirmationSender,
    cancel: CancellationToken,
    started_at: Option<Instant>,
    exit_requested: bool,

    /// Set by [`Session::handle_event`] and read once by [`Session::advance`].
    last_event: Option<AgentEvent>,
    pending: Option<Pending>,
}

/// What the last fold did, as far as the protocol is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// The event is already part of the protocol; forward it verbatim.
    Emit,

    /// The delta was folded into the trailing assistant line.
    Append,

    /// The core consumed the event and it never reaches a client.
    Absorb,
}

impl Session {
    pub fn new(
        info: SessionInfo,
        confirm_tx: ConfirmationSender,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            info,
            state: SessionState::default(),
            confirm_tx,
            cancel,
            started_at: None,
            exit_requested: false,
            last_event: None,
            pending: None,
        }
    }

    /// Whether a client has asked to exit — sent by `/quit`, or by Ctrl+C
    /// when nothing was running and the user confirmed.
    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// How long the current request has been running, if one is.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|started| started.elapsed())
    }

    /// A serializable snapshot of the whole session, for a client that
    /// renders from state rather than by folding events.
    pub fn view(&self) -> SessionView {
        SessionView {
            info: self.info.clone(),
            messages: self.state.messages.iter().map(MessageLine::view).collect(),
            confirmation: self.state.confirmation.clone(),
            busy: self.state.busy,
            fresh: self.state.fresh,
            status: self.state.status.clone(),
            usage: self.state.usage,
            elapsed_ms: self.elapsed().map(|elapsed| elapsed.as_millis() as u64),
        }
    }

    // --------------------------------------------------------------------
    // Commands
    // --------------------------------------------------------------------

    /// Apply a command, returning the text to forward to the agent if any.
    ///
    /// Returning the text rather than sending it keeps the session usable
    /// from a test or a synchronous client with no agent in the loop.
    pub async fn apply(&mut self, command: AgentCommand) -> Result<Option<String>> {
        match command {
            AgentCommand::Submit(text) => self.submit(text),

            AgentCommand::Confirm { allowed } => {
                self.confirm(allowed).await?;

                Ok(None)
            }

            AgentCommand::Cancel => {
                self.cancel();

                Ok(None)
            }

            AgentCommand::Rollback { index } => {
                self.rollback(index);

                Ok(None)
            }

            AgentCommand::ToggleExpanded { index } => {
                if let Some(message) = self.state.messages.get_mut(index) {
                    message.expanded = !message.expanded;
                }

                Ok(None)
            }
        }
    }

    /// Submit a line of user input.
    ///
    /// Local slash commands are handled here and return `Ok(None)`. `/init`
    /// is forwarded to the agent because it inspects the workspace.
    pub fn submit(&mut self, text: String) -> Result<Option<String>> {
        let text = text.trim().to_string();

        if text.is_empty() {
            return Ok(None);
        }

        match commands::Command::parse(&text) {
            Some(commands::Command::Quit) => {
                self.push_system("Goodbye.");
                self.exit_requested = true;
                self.cancel.cancel();

                Ok(None)
            }

            Some(commands::Command::Clear) => {
                self.state.messages.clear();
                self.state.input_history.clear();
                self.state.fresh = true;
                self.state.status = DEFAULT_STATUS.into();

                Ok(None)
            }

            Some(commands::Command::Help) => {
                self.push_system(Self::help_text());

                Ok(None)
            }

            Some(commands::Command::Init) | Some(commands::Command::Unknown(_)) | None => {
                self.submit_prompt(text)
            }
        }
    }

    fn submit_prompt(&mut self, text: String) -> Result<Option<String>> {
        if self.state.busy {
            // The agent is mid-request. The client is expected to cancel
            // first; queueing here would silently reorder the transcript.
            return Err(anyhow::anyhow!(
                "the agent is busy — cancel the current request before sending another"
            ));
        }

        self.state.fresh = false;

        self.state
            .messages
            .push(MessageLine::new(MessageRole::User, text.clone()));

        if self.state.input_history.last() != Some(&text) {
            self.state.input_history.push(text.clone());
        }

        self.state.busy = true;
        self.state.status = BUSY_STATUS.into();
        self.started_at = Some(Instant::now());

        Ok(Some(text))
    }

    /// Answer a pending confirmation.
    pub async fn confirm(&mut self, allowed: bool) -> Result<()> {
        if self.state.confirmation.is_none() {
            return Err(anyhow::anyhow!("no confirmation is pending"));
        }

        let confirmation = if allowed {
            Confirmation::Allow
        } else {
            Confirmation::Deny
        };

        self.confirm_tx
            .send(confirmation)
            .await
            .map_err(|_| anyhow::anyhow!("the agent is no longer listening for a confirmation"))?;

        self.state.confirmation = None;
        self.state.status = BUSY_STATUS.into();

        Ok(())
    }

    /// Cancel the request in flight and return the session to idle.
    pub fn cancel(&mut self) {
        self.cancel.cancel();

        self.started_at = None;
        self.state.busy = false;
        self.state.confirmation = None;
        self.state.status = "Cancelled".into();
    }

    /// Drop everything after `index`, leaving `index` as the last entry so
    /// the user can re-send a corrected version of it.
    pub fn rollback(&mut self, index: usize) {
        if index >= self.state.messages.len() {
            return;
        }

        self.state.messages.truncate(index + 1);

        if self.state.busy {
            // The in-flight request will never resolve into this transcript.
            self.cancel.cancel();
        }

        tracing::debug!(index, "session rolled back");
    }

    // --------------------------------------------------------------------
    // Events
    // --------------------------------------------------------------------

    /// Decide whether an event the session has just folded is worth sending to
    /// a client.
    ///
    /// `before` is the transcript length recorded prior to the fold, which is
    /// what lets an append to an in-flight assistant line be told apart from a
    /// new line. The returned value is what the client receives; `None` means
    /// the event was consumed by the core and is not part of the protocol.
    pub fn advance(&self, before: usize) -> Option<AgentEvent> {
        // An assistant line that was already the last entry grew, so this is a
        // delta rather than a new message.
        let appended = self.state.messages.len() == before
            && self
                .state
                .messages
                .last()
                .is_some_and(|message| message.role == MessageRole::Assistant);

        match self.pending {
            // The client is told what actually happened, so the original event
            // goes out verbatim. Re-deriving it from the folded state would
            // turn a delta into the whole accumulated line.
            Some(Pending::Emit) => self.last_event.clone(),

            // An in-flight assistant line only grows by deltas; if it was
            // replaced instead, the fold changed shape and the event is stale.
            Some(Pending::Append) if appended => self.last_event.clone(),

            _ => None,
        }
    }

    /// Fold an agent event into the session state.
    pub fn handle_event(&mut self, event: AgentEvent) {
        self.pending = Some(Pending::Emit);
        self.last_event = Some(event.clone());

        match event {
            AgentEvent::Thinking => {
                if !self.state.busy {
                    self.state.busy = true;
                    self.started_at.get_or_insert_with(Instant::now);
                }

                self.state.status = "Thinking".into();
            }

            // The core folds the content but does not put it on the wire. A
            // client shows progress from the tool events instead.
            AgentEvent::ThinkingContent(_) => self.absorb(),

            AgentEvent::PlanGenerated(content) => {
                self.push(MessageLine::new(MessageRole::Plan, content));
                self.state.status = BUSY_STATUS.into();
            }

            AgentEvent::ToolStarted { name, input } => {
                self.state.messages.push(MessageLine::tool(name, input));
                self.state.status = BUSY_STATUS.into();
            }

            // Nothing emits this yet. If it is ever wired up in the agent it
            // belongs on the wire, so it is forwarded rather than absorbed.
            AgentEvent::ToolProgress { .. } => {}

            AgentEvent::ToolFinished {
                name,
                duration_ms,
                failed,
            } => {
                if let Some(message) = self
                    .state
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.tool.as_deref() == Some(name.as_str()))
                {
                    message.content = format!("{name} — {duration_ms} ms");
                    message.running = false;
                    message.succeeded = Some(!failed);
                    message.started_at = None;
                }
            }

            AgentEvent::ConfirmationRequired { name, input } => {
                self.state.confirmation = Some(PendingConfirmation { name, input });
                self.state.status = "Waiting for confirmation".into();
            }

            AgentEvent::TextDelta(text) => {
                if let Some(last) = self.state.messages.last_mut()
                    && last.role == MessageRole::Assistant
                {
                    // Appending to the trailing line is a delta, not a new
                    // message, and the client must be told which it is.
                    self.pending = Some(Pending::Append);

                    last.content.push_str(&text);
                } else {
                    self.push(MessageLine::new(MessageRole::Assistant, text));
                }

                self.state.busy = true;
                self.started_at.get_or_insert_with(Instant::now);
            }

            AgentEvent::SystemMessage(text) => {
                self.push(MessageLine::new(MessageRole::System, text));
            }

            AgentEvent::Error(error) => {
                self.push(MessageLine::new(MessageRole::Error, error));
                self.finish();
            }

            // Usage is already in the snapshot, and it is sent again on
            // `Finished`; repeating it per call would be noise.
            AgentEvent::Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
                cost_usd,
            } => {
                self.state
                    .usage
                    .add(prompt_tokens, completion_tokens, total_tokens, cost_usd);

                self.absorb();
            }

            AgentEvent::Finished => self.finish(),

            // A display hint for a front end that polls status. The core
            // derives its own status, so this carries no information.
            AgentEvent::Status { .. } => self.absorb(),

            AgentEvent::SessionInfo { .. } => self.absorb(),
        }
    }

    // --------------------------------------------------------------------
    // Internals
    // --------------------------------------------------------------------

    /// Mark the event just folded as core-only: a client reads this from the
    /// session state instead of from the event stream.
    fn absorb(&mut self) {
        self.pending = Some(Pending::Absorb);
        self.last_event = None;
    }

    fn finish(&mut self) {
        self.started_at = None;
        self.state.busy = false;

        // A confirmation is not cleared here: the agent may report `Finished`
        // while still blocked waiting for the user, and erasing the prompt
        // would leave the tool call hanging with no way to answer it.
        if self.state.confirmation.is_none() {
            self.state.status = DEFAULT_STATUS.into();
        }
    }

    fn push(&mut self, message: MessageLine) {
        self.state.messages.push(message);
    }

    fn push_system(&mut self, text: impl Into<String>) {
        self.push(MessageLine::new(MessageRole::System, text.into()));
    }

    fn help_text() -> String {
        [
            "Commands:",
            "  /help    show this message",
            "  /clear   clear the transcript",
            "  /init    inspect the workspace and write GALAXY.md",
            "  /quit    exit",
        ]
        .join("\n")
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (Session, tokio::sync::mpsc::Receiver<Confirmation>) {
        let (tx, rx) = tokio::sync::mpsc::channel(4);

        (
            Session::new(
                SessionInfo::new("Ollama", "qwen3", "/tmp", vec!["read_file".into()]),
                tx,
                CancellationToken::new(),
            ),
            rx,
        )
    }

    #[test]
    fn submit_appends_user_line_and_returns_prompt() {
        let (mut session, _rx) = session();

        let forwarded = session.submit("  hello  ".into()).unwrap();

        assert_eq!(forwarded.as_deref(), Some("hello"));
        assert_eq!(session.state().messages.len(), 1);
        assert_eq!(session.state().messages[0].role, MessageRole::User);
        assert!(session.state().busy);
    }

    #[test]
    fn empty_and_local_commands_are_not_forwarded() {
        let (mut session, _rx) = session();

        assert_eq!(session.submit("   ".into()).unwrap(), None);
        assert_eq!(session.submit("/help".into()).unwrap(), None);
        assert_eq!(session.submit("/clear".into()).unwrap(), None);
        assert!(session.state().messages.is_empty());
    }

    #[test]
    fn init_is_forwarded_to_the_agent() {
        let (mut session, _rx) = session();

        assert_eq!(
            session.submit("/init".into()).unwrap().as_deref(),
            Some("/init")
        );
    }

    #[test]
    fn submitting_while_busy_is_rejected() {
        let (mut session, _rx) = session();

        session.submit("first".into()).unwrap();

        assert!(session.submit("second".into()).is_err());
        assert_eq!(session.state().messages.len(), 1);
    }

    #[test]
    fn text_deltas_append_to_the_trailing_assistant_line() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::TextDelta("Hel".into()));
        session.handle_event(AgentEvent::TextDelta("lo".into()));
        session.handle_event(AgentEvent::TextDelta(" there".into()));

        let messages = &session.state().messages;

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello there");
    }

    #[test]
    fn tool_finish_updates_the_existing_line() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::ToolStarted {
            name: "read_file".into(),
            input: "src/lib.rs".into(),
        });
        session.handle_event(AgentEvent::ToolFinished {
            name: "read_file".into(),
            duration_ms: 12,
            failed: false,
        });

        let messages = &session.state().messages;

        assert_eq!(messages.len(), 1);
        assert!(!messages[0].running);
        assert_eq!(messages[0].succeeded, Some(true));
        assert_eq!(messages[0].content, "read_file — 12 ms");
    }

    #[test]
    fn a_failed_tool_is_recorded_as_such() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::ToolStarted {
            name: "read_file".into(),
            input: "missing.txt".into(),
        });
        session.handle_event(AgentEvent::ToolFinished {
            name: "read_file".into(),
            duration_ms: 3,
            failed: true,
        });

        let messages = &session.state().messages;

        assert!(!messages[0].running);
        assert_eq!(messages[0].succeeded, Some(false));
    }

    #[test]
    fn the_view_omits_the_instant_but_keeps_the_outcome() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::ToolStarted {
            name: "read_file".into(),
            input: "src/lib.rs".into(),
        });

        let running = session.state().messages[0].view();

        assert!(running.running);
        assert_eq!(running.succeeded, None);
        assert!(running.elapsed_ms.is_some());

        session.handle_event(AgentEvent::ToolFinished {
            name: "read_file".into(),
            duration_ms: 7,
            failed: false,
        });

        let finished = session.state().messages[0].view();

        assert!(!finished.running);
        assert_eq!(finished.succeeded, Some(true));
        assert_eq!(finished.elapsed_ms, None);
    }

    #[test]
    fn advance_forwards_the_delta_and_not_the_whole_line() {
        let (mut session, _rx) = session();

        // A client appends what it receives, so a repeated accumulated line
        // would show "HelloHello".
        for delta in ["Hel", "lo"] {
            let before = session.state().messages.len();

            session.handle_event(AgentEvent::TextDelta(delta.into()));

            match session.advance(before) {
                Some(AgentEvent::TextDelta(sent)) => assert_eq!(sent, delta),

                other => panic!("expected the delta on the wire, got {other:?}"),
            }
        }

        assert_eq!(session.state().messages[0].content, "Hello");
    }

    #[test]
    fn advance_withholds_what_the_core_absorbed() {
        let (mut session, _rx) = session();

        let before = session.state().messages.len();

        session.handle_event(AgentEvent::Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost_usd: 0.25,
        });

        assert!(session.advance(before).is_none());
        // Absorbed from the wire, not from the state.
        assert_eq!(session.state().usage.total_tokens, 15);

        let before = session.state().messages.len();

        session.handle_event(AgentEvent::SystemMessage("hi".into()));

        assert!(matches!(
            session.advance(before),
            Some(AgentEvent::SystemMessage(_))
        ));
    }

    #[test]
    fn finished_preserves_a_pending_confirmation() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::ConfirmationRequired {
            name: "run_command".into(),
            input: "rm -rf /".into(),
        });
        session.handle_event(AgentEvent::Finished);

        assert!(session.state().confirmation.is_some());
        assert!(!session.state().busy);

        session.handle_event(AgentEvent::ConfirmationRequired {
            name: "run_command".into(),
            input: "rm -rf /".into(),
        });
        session.handle_event(AgentEvent::Finished);
        session.handle_event(AgentEvent::Error("boom".into()));

        assert!(session.state().confirmation.is_some());
    }

    #[tokio::test]
    async fn confirm_sends_the_answer_to_the_agent() {
        let (mut session, mut rx) = session();

        session.handle_event(AgentEvent::ConfirmationRequired {
            name: "write_file".into(),
            input: "out.txt".into(),
        });

        session.confirm(true).await.unwrap();

        assert_eq!(rx.recv().await, Some(Confirmation::Allow));
        assert!(session.state().confirmation.is_none());
        assert!(session.confirm(false).await.is_err());
    }

    #[test]
    fn cancel_clears_busy_and_confirmation() {
        let (mut session, _rx) = session();

        session.submit("go".into()).unwrap();

        session.handle_event(AgentEvent::ConfirmationRequired {
            name: "write_file".into(),
            input: "out.txt".into(),
        });
        session.cancel();

        assert!(!session.state().busy);
        assert!(session.state().confirmation.is_none());
        assert_eq!(session.state().status, "Cancelled");
    }

    #[test]
    fn rollback_truncates_after_the_index() {
        let (mut session, _rx) = session();

        session.submit("one".into()).unwrap();
        session.handle_event(AgentEvent::TextDelta("answer".into()));
        session.handle_event(AgentEvent::Finished);
        session.submit("two".into()).unwrap();

        session.rollback(0);

        assert_eq!(session.state().messages.len(), 1);
        assert_eq!(session.state().messages[0].content, "one");
    }

    #[test]
    fn usage_accumulates_in_micros() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost_usd: 0.5,
        });
        session.handle_event(AgentEvent::Usage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
            cost_usd: 0.25,
        });

        let usage = session.state().usage;

        assert_eq!(usage.prompt_tokens, 11);
        assert_eq!(usage.total_tokens, 17);
        assert!((usage.cost_usd() - 0.75).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn toggling_expansion_only_touches_the_target_line() {
        let (mut session, _rx) = session();

        session.handle_event(AgentEvent::ToolStarted {
            name: "read_file".into(),
            input: "a.rs".into(),
        });
        session.handle_event(AgentEvent::ToolStarted {
            name: "read_file".into(),
            input: "b.rs".into(),
        });

        session
            .apply(AgentCommand::ToggleExpanded { index: 0 })
            .await
            .unwrap();

        assert!(session.state().messages[0].expanded);
        assert!(!session.state().messages[1].expanded);
    }
}
