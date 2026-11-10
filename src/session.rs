//! UI-agnostic session layer.
//!
//! Everything a front end needs in order to present Luma lives here: the
//! transcript it renders, the commands it can send, and the events it
//! receives. No terminal, no Ratatui, no Ink — just data and routing.
//!
//! The Ratatui TUI was one client of this module; the Ink client will be
//! another, and the desktop JSONL server a third. They all speak the same
//! [`AgentCommand`] / [`AgentEvent`] pair.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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

/// A fresh cancellation token is used for each request. The parent token is
/// reserved for shutting down the whole agent process.
#[derive(Clone)]
pub struct RequestCancellation {
    shutdown: CancellationToken,
    current: Arc<Mutex<CancellationToken>>,
}

impl RequestCancellation {
    pub fn new(shutdown: CancellationToken) -> Self {
        Self {
            current: Arc::new(Mutex::new(shutdown.child_token())),
            shutdown,
        }
    }

    pub fn current(&self) -> CancellationToken {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn reset(&self) {
        *self
            .current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = self.shutdown.child_token();
    }

    pub fn cancel(&self) {
        self.current().cancel();
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

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
    Preview,
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
    pub chat_id: String,
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

    #[cfg(feature = "tui")]
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
    cancel: RequestCancellation,
    started_at: Option<Instant>,
    exit_requested: bool,

    /// Set by [`Session::handle_event`] and read once by [`Session::advance`].
    last_event: Option<AgentEvent>,
    pending: Option<Pending>,
    chat_id: String,
    summary: Option<String>,
    #[cfg(not(test))]
    chat_created_at: u64,
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
        cancel: RequestCancellation,
    ) -> Self {
        let session = Self {
            info,
            state: SessionState::default(),
            confirm_tx,
            cancel,
            started_at: None,
            exit_requested: false,
            last_event: None,
            pending: None,
            chat_id: crate::chats::new_id(),
            summary: None,
            #[cfg(not(test))]
            chat_created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        session
    }

    /// Whether a client has asked to exit — sent by `/quit`, or by Ctrl+C
    /// when nothing was running and the user confirmed.
    #[cfg(feature = "tui")]
    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    #[cfg(feature = "tui")]
    pub fn info(&self) -> &SessionInfo {
        &self.info
    }

    pub fn set_model(&mut self, provider: impl Into<String>, model: impl Into<String>) {
        self.info.provider = provider.into();
        self.info.model = model.into();
        self.push_system(format!(
            "Switched to {} · {}",
            self.info.provider, self.info.model
        ));
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn request_cancellation(&self) -> RequestCancellation {
        self.cancel.clone()
    }

    /// How long the current request has been running, if one is.
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|started| started.elapsed())
    }

    /// A serializable snapshot of the whole session, for a client that
    /// renders from state rather than by folding events.
    pub fn view(&self) -> SessionView {
        SessionView {
            chat_id: self.chat_id.clone(),
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

        if text == "/history" {
            let output = crate::chats::list()?
                .into_iter()
                .take(10)
                .map(|chat| format!("{}  {}  ({} messages)", chat.id, chat.title, chat.messages))
                .collect::<Vec<_>>()
                .join("\n");
            self.push_system(if output.is_empty() {
                "No saved chats yet.".into()
            } else {
                output
            });
            return Ok(None);
        }

        if text == "/resume" {
            if self.state.busy {
                self.push_system("Cancel the current request before resuming a task.");
                return Ok(None);
            }
            self.cancel.reset();
            let Some(checkpoint) = crate::tasks::load()? else {
                self.push_system("There is no saved task checkpoint for this workspace.");
                return Ok(None);
            };
            if checkpoint.status == crate::tasks::TaskStatus::Completed {
                self.push_system("The last saved task is already completed.");
                return Ok(None);
            }
            let workspace = std::env::current_dir()?.display().to_string();
            if checkpoint.workspace != workspace {
                self.push_system("The saved checkpoint belongs to a different workspace.");
                return Ok(None);
            }
            let snapshot = serde_json::to_string(&checkpoint)?;
            self.state.fresh = false;
            self.state.busy = true;
            self.state.status = BUSY_STATUS.into();
            self.started_at = Some(Instant::now());
            self.push_system(format!("Resuming task: {}", checkpoint.request));
            self.persist();
            return Ok(Some(format!("\0LUMA_RESUME:{snapshot}")));
        }
        if text == "/plan" {
            self.push_system(
                "Usage: /plan <task> — create and review a plan without implementing it.",
            );
            return Ok(None);
        }

        if let Some(id) = text.strip_prefix("/restore ") {
            if self.state.busy {
                self.push_system("Cancel the current request before restoring a chat.");
                return Ok(None);
            }
            let archive = match crate::chats::load(id.trim()) {
                Ok(archive) => archive,
                Err(error) => {
                    self.push_system(format!("Could not restore chat: {error:#}"));
                    return Ok(None);
                }
            };
            let replay = serde_json::to_string(&archive.messages)?;
            let messages = archive
                .messages
                .iter()
                .map(|view| MessageLine {
                    role: view.role,
                    content: view.content.clone(),
                    tool: view.tool.clone(),
                    running: false,
                    succeeded: view.succeeded,
                    expanded: view.expanded,
                    started_at: None,
                })
                .collect::<Vec<_>>();

            self.state.messages = messages;
            self.state.input_history = archive
                .messages
                .iter()
                .filter(|message| message.role == MessageRole::User)
                .map(|message| message.content.clone())
                .collect();
            self.state.fresh = self.state.messages.is_empty();
            self.state.busy = false;
            self.state.confirmation = None;
            self.state.status = DEFAULT_STATUS.into();
            self.started_at = None;
            self.cancel.reset();
            self.last_event = None;
            self.pending = None;
            self.chat_id = archive.id;
            self.summary = archive.summary;
            #[cfg(not(test))]
            {
                self.chat_created_at = archive.created_at;
            }
            return Ok(Some(format!("\0LUMA_RESTORE:{replay}")));
        }
        if text == "/restore" {
            self.push_system("Usage: /restore <chat-id>");
            return Ok(None);
        }

        if let Some(summary) = text.strip_prefix("/summary:add ") {
            crate::chats::add_summary(&self.chat_id, summary.to_owned())?;
            self.summary = Some(summary.trim().to_owned());
            self.push_system("Summary saved for this chat.");
            self.persist();
            return Ok(None);
        }

        match commands::Command::parse(&text) {
            Some(commands::Command::Quit) => {
                self.push_system("Goodbye.");
                self.exit_requested = true;
                self.cancel.shutdown();

                Ok(None)
            }

            Some(commands::Command::Clear) => {
                self.state.messages.clear();
                self.state.input_history.clear();
                self.state.fresh = true;
                self.state.status = DEFAULT_STATUS.into();
                self.chat_id = crate::chats::new_id();
                self.summary = None;
                #[cfg(not(test))]
                {
                    self.chat_created_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                }
                self.persist();

                Ok(None)
            }

            Some(commands::Command::Help) => {
                self.push_system(Self::help_text());

                Ok(None)
            }

            Some(commands::Command::Init) | Some(commands::Command::Unknown) | None => {
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

        self.cancel.reset();

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

        self.persist();

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

    /// Request cancellation and wait for the agent's terminal event before
    /// marking the session idle.
    pub fn cancel(&mut self) {
        if !self.state.busy {
            return;
        }
        self.cancel.cancel();
        self.state.confirmation = None;
        self.state.status = "Cancelling".into();
    }

    /// Drop everything after `index`, leaving `index` as the last entry so
    /// the user can re-send a corrected version of it.
    pub fn rollback(&mut self, index: usize) {
        if index >= self.state.messages.len() {
            return;
        }

        self.state.messages.truncate(index + 1);
        self.persist();

        if self.state.busy {
            // The in-flight request will never resolve into this transcript.
            self.cancel();
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
        let should_persist = !matches!(
            &event,
            AgentEvent::Thinking
                | AgentEvent::ThinkingContent(_)
                | AgentEvent::TextDelta(_)
                | AgentEvent::Usage { .. }
                | AgentEvent::Status { .. }
                | AgentEvent::SessionInfo { .. }
        );
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

            AgentEvent::ChangePreview { path, diff } => {
                self.push(MessageLine::new(
                    MessageRole::Preview,
                    format!("{path}\n{diff}"),
                ));
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
                ..
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
                self.state.status = "Error".into();
                self.finish();
            }

            AgentEvent::Cancelled => {
                self.state.confirmation = None;
                for message in &mut self.state.messages {
                    if message.role == MessageRole::Tool && message.running {
                        message.running = false;
                        message.started_at = None;
                    }
                }
                self.push(MessageLine::new(
                    MessageRole::System,
                    "Generation cancelled.".into(),
                ));
                self.finish();
                self.state.status = "Cancelled".into();
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

            AgentEvent::Status { state } => {
                self.state.status = match state {
                    crate::event::AgentStatus::Idle => {
                        self.state.busy = false;
                        self.started_at = None;
                        DEFAULT_STATUS.into()
                    }
                    crate::event::AgentStatus::Thinking => "Thinking".into(),
                    crate::event::AgentStatus::Doing => "Working".into(),
                    crate::event::AgentStatus::Talking => "Generating response".into(),
                    crate::event::AgentStatus::Waiting => "Waiting for confirmation".into(),
                    crate::event::AgentStatus::Error => {
                        self.state.busy = false;
                        "Error".into()
                    }
                };
                if matches!(
                    state,
                    crate::event::AgentStatus::Thinking
                        | crate::event::AgentStatus::Doing
                        | crate::event::AgentStatus::Talking
                        | crate::event::AgentStatus::Waiting
                ) {
                    self.state.busy = true;
                    self.started_at.get_or_insert_with(Instant::now);
                }
            }

            AgentEvent::SessionInfo { .. } => self.absorb(),
        }
        if should_persist {
            self.persist();
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
        if self.state.confirmation.is_none()
            && self.state.status != "Cancelled"
            && self.state.status != "Error"
        {
            self.state.status = DEFAULT_STATUS.into();
        }
    }

    fn push(&mut self, message: MessageLine) {
        self.state.messages.push(message);
    }

    fn push_system(&mut self, text: impl Into<String>) {
        self.push(MessageLine::new(MessageRole::System, text.into()));
        self.persist();
    }

    fn persist(&self) {
        #[cfg(test)]
        return;

        #[cfg(not(test))]
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let archive = crate::chats::ChatArchive {
                id: self.chat_id.clone(),
                created_at: self.chat_created_at,
                updated_at: now,
                info: self.info.clone(),
                messages: self.state.messages.iter().map(MessageLine::view).collect(),
                summary: self.summary.clone(),
            };
            if let Err(error) = crate::chats::save(&archive) {
                tracing::warn!(%error, "could not persist chat transcript");
            }
        }
    }

    fn help_text() -> String {
        [
            "Commands:",
            "  /help    show this message",
            "  /clear   clear the transcript",
            "  /history list recent saved chats",
            "  /resume  resume the interrupted task in this workspace",
            "  /plan <task> create an implementation plan for a task",
            "  /restore <chat-id> restore a saved chat as the active conversation",
            "  /summary:add <text> save a summary for this chat",
            "  /model   show the current model; /model <model-id> switches it",
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
                RequestCancellation::new(CancellationToken::new()),
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
        let previous_chat_id = session.chat_id.clone();

        assert_eq!(session.submit("   ".into()).unwrap(), None);
        assert_eq!(session.submit("/help".into()).unwrap(), None);
        assert_eq!(session.submit("/clear".into()).unwrap(), None);
        assert!(session.state().messages.is_empty());
        assert_ne!(session.chat_id, previous_chat_id);
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
    fn restore_replaces_transcript_and_keeps_archive_identity_and_summary() {
        let (mut session, _rx) = session();
        let id = crate::chats::new_id();
        let archive = crate::chats::ChatArchive {
            id: id.clone(),
            created_at: 123,
            updated_at: 456,
            info: SessionInfo::new("Archived provider", "old-model", "/old", vec![]),
            messages: vec![
                MessageView {
                    role: MessageRole::User,
                    content: "previous question".into(),
                    tool: None,
                    running: false,
                    succeeded: None,
                    expanded: false,
                    elapsed_ms: None,
                },
                MessageView {
                    role: MessageRole::Assistant,
                    content: "previous answer".into(),
                    tool: None,
                    running: false,
                    succeeded: None,
                    expanded: false,
                    elapsed_ms: None,
                },
            ],
            summary: Some("kept summary".into()),
        };
        crate::chats::save(&archive).unwrap();

        let replay = session.submit(format!("/restore {id}")).unwrap().unwrap();

        assert!(replay.starts_with("\0LUMA_RESTORE:"));
        assert_eq!(session.chat_id, id);
        assert_eq!(session.info.model, "qwen3");
        assert_eq!(session.state.messages.len(), 2);
        assert_eq!(session.state.messages[0].content, "previous question");
        assert_eq!(session.state.input_history, vec!["previous question"]);
        assert_eq!(session.summary.as_deref(), Some("kept summary"));
        assert!(!session.state.fresh);
        assert_eq!(
            session
                .submit("continue from here".into())
                .unwrap()
                .as_deref(),
            Some("continue from here")
        );
        assert_eq!(session.chat_id, archive.id);

        std::fs::remove_file(crate::chats::chats_dir().join(format!("{}.json", archive.id)))
            .unwrap();
    }

    #[test]
    fn failed_restore_does_not_replace_active_transcript() {
        let (mut session, _rx) = session();
        session.state.messages.push(MessageLine::new(
            MessageRole::User,
            "keep this active".into(),
        ));
        let before = session.state.messages.clone();

        assert!(
            session
                .submit("/restore missing-chat-id".into())
                .unwrap()
                .is_none()
        );
        assert_eq!(session.state.messages[0].content, before[0].content);
        assert!(
            session
                .state
                .messages
                .last()
                .unwrap()
                .content
                .contains("Could not restore chat")
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
            diff: None,
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
            diff: None,
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
            diff: None,
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
    fn cancel_waits_for_agent_acknowledgement_before_clearing_busy() {
        let (mut session, _rx) = session();

        session.submit("go".into()).unwrap();

        session.handle_event(AgentEvent::ConfirmationRequired {
            name: "write_file".into(),
            input: "out.txt".into(),
        });
        session.cancel();

        assert!(session.state().busy);
        assert!(session.state().confirmation.is_none());
        assert_eq!(session.state().status, "Cancelling");

        session.handle_event(AgentEvent::Cancelled);
        assert!(!session.state().busy);
        assert_eq!(session.state().status, "Cancelled");
    }

    #[test]
    fn a_cancelled_request_does_not_cancel_the_next_request() {
        let (mut session, _rx) = session();

        session.submit("first".into()).unwrap();
        let first = session.request_cancellation().current();
        session.cancel();
        assert!(first.is_cancelled());

        session.handle_event(AgentEvent::Cancelled);
        session.submit("second".into()).unwrap();
        let second = session.request_cancellation().current();

        assert!(!second.is_cancelled());
        assert_eq!(session.state().status, BUSY_STATUS);
    }

    #[test]
    fn status_events_are_forwarded_and_folded_into_the_session() {
        let (mut session, _rx) = session();
        session.submit("hello".into()).unwrap();

        let before = session.state().messages.len();
        session.handle_event(AgentEvent::Status {
            state: crate::event::AgentStatus::Talking,
        });

        assert_eq!(session.state().status, "Generating response");
        assert!(matches!(
            session.advance(before),
            Some(AgentEvent::Status { .. })
        ));
    }

    #[test]
    fn cancelled_event_finishes_the_request_without_reporting_an_error() {
        let (mut session, _rx) = session();
        session.submit("hello".into()).unwrap();
        session.handle_event(AgentEvent::ToolStarted {
            name: "run_command".into(),
            input: "cargo test".into(),
        });
        let before = session.state().messages.len();

        session.handle_event(AgentEvent::Cancelled);

        assert!(!session.state().busy);
        assert_eq!(session.state().status, "Cancelled");
        assert_eq!(
            session.state().messages.last().unwrap().content,
            "Generation cancelled."
        );
        assert!(!session.state().messages[1].running);
        assert!(matches!(
            session.advance(before),
            Some(AgentEvent::Cancelled)
        ));
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
        assert_eq!(usage.cost_usd_micros, 750_000);
    }

    #[test]
    fn changing_model_updates_session_info_and_transcript() {
        let (mut session, _rx) = session();

        session.set_model("anthropic", "claude-sonnet-4-5");

        assert_eq!(session.info.provider, "anthropic");
        assert_eq!(session.info.model, "claude-sonnet-4-5");
        assert_eq!(
            session.state().messages.last().unwrap().content,
            "Switched to anthropic · claude-sonnet-4-5"
        );
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
