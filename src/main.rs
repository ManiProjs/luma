mod agent;
mod chats;
mod commands;
mod config;
mod context;
mod dashboard;
mod desktop;
mod event;
mod gateway;
mod history;
mod integration_cli;
mod logging;
mod mcp;
mod model;
mod plugins;
mod protocol;
mod provider;
mod react;
mod runtime;
mod session;
mod tasks;
mod tools;
mod worker;
mod workspace;

#[cfg(feature = "tui")]
mod theme;

#[cfg(feature = "tui")]
mod tui;
use std::process::ExitCode;

use anyhow::Result;

use agent::Agent;
use event::AgentEvent;
use history::History;
use tokio_util::sync::CancellationToken;

use crate::{model::DynamicModel, runtime::PermissionProfile, session::SessionInfo};

use dialoguer::Confirm;

fn confirm_setup() -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt("Luma is not configured yet. Run setup?")
        .default(true)
        .interact()?)
}

fn code_permissions(tool_names: &[String]) -> PermissionProfile {
    PermissionProfile {
        // Installing and enabling a native integration is an explicit user
        // choice. Its individual tool calls still require approval.
        allow_network: tool_names
            .iter()
            .any(|name| name.starts_with("plugin__") || name.starts_with("mcp__")),
        ..PermissionProfile::default()
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            // The front end owns presentation, so a top-level failure has to
            // be legible in a plain terminal as well as in a log file.
            eprintln!("luma-core: {error:#}");

            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    // ========================================================
    // Logging
    // ========================================================

    let log_path = logging::init();

    tracing::info!("Luma starting");

    tracing::debug!(path = %log_path?.display(), "Logging initialized");

    // ========================================================
    // CLI
    // ========================================================

    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if integration_cli::handles(&args) {
        return integration_cli::run(&args);
    }

    // The desktop server receives JSONL over stdin. It must never run the
    // interactive setup prompt on that stream, or an unconfigured install
    // will wait forever instead of returning a protocol error.
    if args.as_slice() == ["desktop-server"] {
        return run_desktop_server().await;
    }

    if args.first().is_some_and(|argument| argument == "gateway") {
        return gateway::run_command(&args[1..]).await;
    }

    if args.len() == 2 && args[0] == "task-worker" {
        return worker::run_task_worker(&args[1]).await;
    }

    if !args.is_empty() {
        anyhow::bail!(
            "CLI commands are provided by the Ink app. Invoke `luma` instead of `luma-core`."
        );
    }

    // ========================================================
    // Setup
    // ========================================================

    if !config::exists() {
        if confirm_setup()? {
            config::setup::run().await?;

            return Ok(());
        }

        println!("Luma cannot start without configuration.");

        return Ok(());
    }

    // ========================================================
    // Configuration
    // ========================================================

    let config = config::load()?;

    // ========================================================
    // Tools
    // ========================================================

    let mut tools = tools::default_registry();
    plugins::register_enabled_tools(&mut tools)?;
    mcp::register_enabled_tools(&mut tools)?;

    let tool_names = tools.names();

    // ========================================================
    // ReAct model (shared by action selection and final responses)
    // ========================================================

    let model = DynamicModel::new(&config.model);
    let react = react::ReAct::new(model.clone(), &tools);

    // ========================================================
    // History / Workspace
    // ========================================================

    let history = History::load();

    workspace::bootstrap::WorkspaceBootstrap::initialize()?;

    let galaxy = workspace::bootstrap::WorkspaceBootstrap::load()?;

    // ========================================================
    // Agent
    // ========================================================

    let mut agent = Agent::new(model, react, tools, history, galaxy);
    agent.set_permission_profile(code_permissions(&tool_names));

    // ========================================================
    // Channels
    // ========================================================

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (confirmation_tx, confirmation_rx) = tokio::sync::mpsc::channel::<agent::Confirmation>(16);
    let (question_tx, question_rx) = tokio::sync::mpsc::channel::<agent::QuestionAnswer>(4);

    let cancel = CancellationToken::new();

    // The legacy Ratatui client consumes a broadcast stream. The dashboard
    // now runs through the same JSONL desktop server as the Ink client.
    let (ui_events, _) = tokio::sync::broadcast::channel::<AgentEvent>(256);
    let ui_events_for_bridge = ui_events.clone();
    tokio::spawn(async move {
        let mut event_rx = event_rx;
        while let Some(event) = event_rx.recv().await {
            let _ = ui_events_for_bridge.send(event);
        }
    });

    // ========================================================
    // Session
    // ========================================================

    let info = SessionInfo::new(
        config.model.provider.clone(),
        config.model.name.clone(),
        std::env::current_dir()?.display().to_string(),
        tool_names,
    );

    let request_cancel = session::RequestCancellation::new(cancel.clone());
    let session = session::Session::new(
        info,
        confirmation_tx,
        question_tx.clone(),
        request_cancel.clone(),
    );

    // ========================================================
    // Agent
    // ========================================================

    let agent_cancel = cancel.clone();

    tokio::spawn(async move {
        if let Err(error) = agent
            .run(
                input_rx,
                event_tx,
                agent_cancel,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
        {
            tracing::error!("Agent stopped: {}", error);
        }
    });

    // ========================================================
    // TUI mode
    // ========================================================

    #[cfg(feature = "tui")]
    {
        return run_tui(session, ui_events.subscribe(), input_tx, cancel).await;
    }

    // Without the Ratatui front end there is no terminal UI to fall back to.
    // The core is still reachable over JSONL, so point the user at it rather
    // than exiting silently.
    #[cfg(not(feature = "tui"))]
    {
        drop(session);
        drop(input_tx);

        anyhow::bail!(
            "this build has no terminal UI (built without the `tui` feature); \
             run `luma desktop-server` and connect a client over stdio JSONL"
        )
    }
}

#[cfg(feature = "tui")]
async fn run_tui(
    mut session: session::Session,
    mut events: tokio::sync::broadcast::Receiver<AgentEvent>,
    input_tx: tokio::sync::mpsc::Sender<String>,
    cancel: CancellationToken,
) -> Result<()> {
    tui::run(&mut session, &mut events, input_tx, cancel).await
}

// ============================================================
// Desktop server
// ============================================================

async fn run_desktop_server() -> Result<()> {
    // ========================================================
    // Configuration
    // ========================================================

    let config = config::load_or_default()?;

    // The gateway owns scheduled/background work independently from this
    // short-lived UI protocol process.
    gateway::ensure_running().await?;
    let runtime = runtime::RuntimeStore::open_default()?;

    // ========================================================
    // Tools
    // ========================================================

    let mut tools = tools::default_registry();
    plugins::register_enabled_tools(&mut tools)?;
    mcp::register_enabled_tools(&mut tools)?;

    let tool_names = tools.names();

    // ========================================================
    // ReAct
    // ========================================================

    let model = model::DynamicModel::new(&config.model);
    let react = react::ReAct::new(model.clone(), &tools);

    // ========================================================
    // History / Workspace
    // ========================================================

    let history = History::load();

    workspace::bootstrap::WorkspaceBootstrap::initialize()?;

    let galaxy = workspace::bootstrap::WorkspaceBootstrap::load()?;

    // ========================================================
    // Agent
    // ========================================================

    let mut agent = Agent::new(model.clone(), react, tools, history, galaxy);
    agent.set_permission_profile(code_permissions(&tool_names));

    // ========================================================
    // Channels
    // ========================================================

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (confirmation_tx, confirmation_rx) = tokio::sync::mpsc::channel::<agent::Confirmation>(16);
    let (question_tx, question_rx) = tokio::sync::mpsc::channel::<agent::QuestionAnswer>(4);

    let cancel = CancellationToken::new();

    let info = SessionInfo::new(
        config.model.provider.clone(),
        config.model.name.clone(),
        std::env::current_dir()?.display().to_string(),
        tool_names,
    );

    let request_cancel = session::RequestCancellation::new(cancel.clone());
    let session = session::Session::new(
        info,
        confirmation_tx.clone(),
        question_tx.clone(),
        request_cancel.clone(),
    );

    // ========================================================
    // Agent
    // ========================================================

    let agent_cancel = cancel.clone();

    tokio::spawn(async move {
        if let Err(error) = agent
            .run(
                input_rx,
                event_tx,
                agent_cancel,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
        {
            tracing::error!("Desktop agent stopped: {}", error);
        }
    });

    // ========================================================
    // Desktop protocol
    // ========================================================

    desktop::DesktopServer::new(session, input_tx, cancel, model, confirmation_tx, runtime)
        .run(event_rx)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::code_permissions;

    #[test]
    fn code_enables_network_policy_only_when_external_integrations_are_enabled() {
        assert!(!code_permissions(&["read_file".into()]).allow_network);
        assert!(code_permissions(&["plugin__calendar__create".into()]).allow_network);
        assert!(code_permissions(&["mcp__filesystem__read_file".into()]).allow_network);
    }
}
