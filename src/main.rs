mod agent;
mod commands;
mod config;
mod context;
mod dashboard;
mod desktop;
mod event;
mod history;
mod logging;
mod model;
mod planner;
mod router;
mod theme;
mod tools;
mod tui;
mod workspace;

use anyhow::Result;
use clap::{Parser, Subcommand};

use agent::Agent;
use dashboard::{DashboardCommand, DashboardServer};
use event::AgentEvent;
use history::History;
use tokio_util::sync::CancellationToken;

use tools::{
    ToolRegistry,
    filesystem::{
        list_directory::ListDirectory, patch_file::PatchFile, read_file::ReadFile,
        search_files::SearchFiles, write_file::WriteFile,
    },
    shell::RunCommand,
};

use crate::{model::create_model, tui::info::LumaInfo};

use dialoguer::Confirm;

#[derive(Parser, Debug)]
#[command(name = "luma", version, about = "A lightweight AI coding agent")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(trailing_var_arg = true)]
    prompt: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Setup,

    Dashboard {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value_t = 3000)]
        port: u16,
    },

    DesktopServer,
}

fn confirm_setup() -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt("Luma is not configured yet. Run setup?")
        .default(true)
        .interact()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    // ========================================================
    // Logging
    // ========================================================

    let log_path = logging::init();

    tracing::info!("Luma starting");

    tracing::debug!(
        path = %log_path?.display(),
        "Logging initialized"
    );

    // ========================================================
    // CLI
    // ========================================================

    let args = Args::parse();

    // ========================================================
    // Setup
    // ========================================================

    if !config::exists() && !matches!(args.command, Some(Commands::Setup)) {
        if confirm_setup()? {
            config::setup::run().await?;
            return Ok(());
        }

        println!("Luma cannot start without configuration.");

        return Ok(());
    }

    if let Some(Commands::Setup) = args.command {
        config::setup::run().await?;
        return Ok(());
    }

    // ========================================================
    // Desktop server
    // ========================================================

    if matches!(args.command, Some(Commands::DesktopServer)) {
        return run_desktop_server().await;
    }

    // ========================================================
    // Mode
    // ========================================================

    let dashboard_requested = matches!(args.command, Some(Commands::Dashboard { .. }));

    let (dashboard_host, dashboard_port) = match args.command {
        Some(Commands::Dashboard { host, port }) => (host, port),

        _ => ("127.0.0.1".to_string(), 3000),
    };

    // ========================================================
    // Configuration
    // ========================================================

    let config = config::load()?;

    // ========================================================
    // Tools
    // ========================================================

    let mut tools = ToolRegistry::new();

    let model = create_model(&config.model);

    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);

    let tool_names = tools.names();

    // ========================================================
    // Planner
    // ========================================================

    let planner_model = create_model(&config.planner);

    let planner = planner::Planner::new(planner_model, &tools);

    // ========================================================
    // History / Workspace
    // ========================================================

    let history = History::load();

    workspace::bootstrap::WorkspaceBootstrap::initialize()?;

    let galaxy = workspace::bootstrap::WorkspaceBootstrap::load()?;

    // ========================================================
    // Agent
    // ========================================================

    let mut agent = Agent::new(model, planner, tools, history, galaxy);

    // ========================================================
    // Channels
    // ========================================================

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (confirmation_tx, confirmation_rx) = tokio::sync::mpsc::channel::<agent::Confirmation>(16);

    let cancel = CancellationToken::new();

    // ========================================================
    // Dashboard
    // ========================================================

    let (dashboard, mut dashboard_commands) = DashboardServer::new("dashboard", 256);

    // --------------------------------------------------------
    // Dashboard commands -> Agent
    // --------------------------------------------------------

    let dashboard_input_tx = input_tx.clone();

    let dashboard_confirmation_tx = confirmation_tx.clone();

    tokio::spawn(async move {
        while let Some(command) = dashboard_commands.recv().await {
            match command {
                DashboardCommand::Chat(text) => {
                    if dashboard_input_tx.send(text).await.is_err() {
                        break;
                    }
                }

                DashboardCommand::Confirm { allowed } => {
                    let confirmation = if allowed {
                        agent::Confirmation::Allow
                    } else {
                        agent::Confirmation::Deny
                    };

                    if dashboard_confirmation_tx.send(confirmation).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // ========================================================
    // Agent events -> Dashboard broadcast
    // ========================================================

    let dashboard_events = dashboard.events.clone();

    tokio::spawn(async move {
        let mut event_rx = event_rx;

        while let Some(event) = event_rx.recv().await {
            let _ = dashboard_events.send(event);
        }
    });

    // ========================================================
    // Dashboard server
    // ========================================================

    if dashboard_requested {
        let addr = format!("{}:{}", dashboard_host, dashboard_port);

        let dashboard_server = dashboard.clone();

        let value = addr.clone();

        tokio::spawn(async move {
            if let Err(error) = dashboard_server.run(&value).await {
                tracing::error!("Dashboard failed: {}", error);
            }
        });

        println!("Luma Dashboard: http://{}", addr);
    }

    // ========================================================
    // Agent
    // ========================================================

    tokio::spawn(async move {
        if let Err(error) = agent.run(input_rx, event_tx, cancel, confirmation_rx).await {
            tracing::error!("Agent stopped: {}", error);
        }
    });

    // ========================================================
    // CLI prompt
    // ========================================================

    let cli_prompt = if args.prompt.is_empty() {
        None
    } else {
        Some(args.prompt.join(" "))
    };

    if let Some(prompt) = cli_prompt {
        input_tx.send(prompt).await?;
    }

    // ========================================================
    // Dashboard mode
    // ========================================================

    if dashboard_requested {
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    // ========================================================
    // TUI mode
    // ========================================================

    let info = {
        let mut info = LumaInfo::new(
            config.model.provider.clone(),
            config.model.name.clone(),
            tool_names,
        );

        info.workspace = Some(std::env::current_dir()?.display().to_string());

        info
    };

    // --------------------------------------------------------
    // Subscribe to the dashboard event bus.
    //
    // The dashboard bridge receives AgentEvent from the
    // original mpsc channel and publishes every event into
    // dashboard.events.
    //
    // The TUI gets its own broadcast subscription here.
    // --------------------------------------------------------

    let mut tui_event_rx = dashboard.events.subscribe();

    // --------------------------------------------------------
    // Convert broadcast::Receiver<AgentEvent> into the
    // mpsc::Receiver<AgentEvent> expected by terminal::run().
    // --------------------------------------------------------

    let (tui_tx, tui_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    tokio::spawn(async move {
        loop {
            match tui_event_rx.recv().await {
                Ok(event) => {
                    if tui_tx.send(event).await.is_err() {
                        break;
                    }
                }

                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!("TUI event receiver lagged; skipped {} events", skipped);
                }

                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // --------------------------------------------------------
    // Run the actual TUI.
    // --------------------------------------------------------

    tui::terminal::run(
        tui_rx,
        input_tx,
        CancellationToken::new(),
        confirmation_tx,
        info,
    )
    .await?;

    Ok(())
}

// ============================================================
// Desktop server
// ============================================================

async fn run_desktop_server() -> Result<()> {
    // ========================================================
    // Configuration
    // ========================================================

    let config = config::load()?;

    // ========================================================
    // Tools
    // ========================================================

    let mut tools = ToolRegistry::new();

    let model = create_model(&config.model);

    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);

    // ========================================================
    // Planner
    // ========================================================

    let planner_model = create_model(&config.planner);

    let planner = planner::Planner::new(planner_model, &tools);

    // ========================================================
    // History / Workspace
    // ========================================================

    let history = History::load();

    workspace::bootstrap::WorkspaceBootstrap::initialize()?;

    let galaxy = workspace::bootstrap::WorkspaceBootstrap::load()?;

    // ========================================================
    // Agent
    // ========================================================

    let mut agent = Agent::new(model, planner, tools, history, galaxy);

    // ========================================================
    // Channels
    // ========================================================

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (confirmation_tx, confirmation_rx) = tokio::sync::mpsc::channel::<agent::Confirmation>(16);

    let cancel = CancellationToken::new();

    // ========================================================
    // Agent
    // ========================================================

    let agent_cancel = cancel.clone();

    tokio::spawn(async move {
        if let Err(error) = agent
            .run(input_rx, event_tx, agent_cancel, confirmation_rx)
            .await
        {
            tracing::error!("Desktop agent stopped: {}", error);
        }
    });

    // ========================================================
    // Desktop protocol
    // ========================================================

    desktop::run(event_rx, input_tx, confirmation_tx, cancel).await?;

    Ok(())
}
