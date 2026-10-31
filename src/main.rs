mod agent;
mod chats;
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
mod protocol;
mod provider;
mod router;
mod session;
mod tasks;
mod tools;
mod workspace;

#[cfg(feature = "tui")]
mod theme;

#[cfg(feature = "tui")]
mod tui;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use agent::Agent;
use dashboard::{DashboardCommand, DashboardServer};
use event::AgentEvent;
use history::History;
use tokio_util::sync::CancellationToken;

use tools::{
    ToolRegistry,
    code_intelligence::{FindReferences, FindSymbols},
    developer::{
        CountLines, FileMetadata, FindTodos, GitBlame, GitBranches, GitLog, GitShow, InspectProject,
    },
    filesystem::{
        list_directory::ListDirectory, patch_file::PatchFile, read_file::ReadFile,
        search_files::SearchFiles, write_file::WriteFile,
    },
    git::{GitDiff, GitStatus},
    shell::RunCommand,
    skills::{ListSkills, LoadSkill},
};

use crate::{model::create_model, session::SessionInfo};

use dialoguer::Confirm;

#[derive(Parser, Debug)]
#[command(
    name = "luma-core",
    version,
    about = "The Luma core: agent, tools, and the JSONL protocol server"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(trailing_var_arg = true)]
    prompt: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Setup,

    /// Browse saved conversations.
    History {
        #[command(subcommand)]
        command: Option<HistoryCommands>,
    },

    /// Manage saved conversation summaries.
    Summary {
        #[command(subcommand)]
        command: SummaryCommands,
    },

    Dashboard {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        #[arg(long, default_value_t = 3000)]
        port: u16,
    },

    DesktopServer,
}

#[derive(Subcommand, Debug)]
enum HistoryCommands {
    List,
    Show { id: String },
}

#[derive(Subcommand, Debug)]
enum SummaryCommands {
    Add { id: String, summary: Vec<String> },
}

fn confirm_setup() -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt("Luma is not configured yet. Run setup?")
        .default(true)
        .interact()?)
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

    let args = Args::parse();

    // Archive commands are standalone and must work without model setup.
    match args.command.as_ref() {
        Some(Commands::History { command }) => {
            match command.as_ref().unwrap_or(&HistoryCommands::List) {
                HistoryCommands::List => {
                    let chats = chats::list()?;
                    if chats.is_empty() {
                        println!("No saved chats yet.");
                    }
                    for chat in chats {
                        println!("{}  {}  ({} messages)", chat.id, chat.title, chat.messages);
                        if let Some(summary) = chat.summary {
                            println!("  Summary: {summary}");
                        }
                    }
                }
                HistoryCommands::Show { id } => {
                    let chat = chats::load(id)?;
                    println!(
                        "Chat {} · {} · {}",
                        chat.id, chat.info.model, chat.info.workspace
                    );
                    if let Some(summary) = chat.summary {
                        println!("\nSummary\n{summary}");
                    }
                    for message in chat.messages {
                        println!("\n[{:?}]\n{}", message.role, message.content);
                    }
                }
            }
            return Ok(());
        }
        Some(Commands::Summary {
            command: SummaryCommands::Add { id, summary },
        }) => {
            chats::add_summary(id, summary.join(" "))?;
            println!("Summary saved for chat {id}.");
            return Ok(());
        }
        _ => {}
    }

    // The desktop server receives JSONL over stdin. It must never run the
    // interactive setup prompt on that stream, or an unconfigured install
    // will wait forever instead of returning a protocol error.
    if matches!(args.command, Some(Commands::DesktopServer)) {
        return run_desktop_server().await;
    }

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

    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);
    tools.register(GitStatus);
    tools.register(GitDiff);
    tools.register(FindSymbols);
    tools.register(FindReferences);
    tools.register(GitLog);
    tools.register(GitShow);
    tools.register(GitBlame);
    tools.register(GitBranches);
    tools.register(FileMetadata);
    tools.register(FindTodos);
    tools.register(CountLines);
    tools.register(InspectProject);
    tools.register(ListSkills);
    tools.register(LoadSkill);

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

    let model = create_model(&config.model);

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
    // Session
    // ========================================================

    let info = SessionInfo::new(
        config.model.provider.clone(),
        config.model.name.clone(),
        std::env::current_dir()?.display().to_string(),
        tool_names,
    );

    let mut session = session::Session::new(info, confirmation_tx, cancel.clone());

    // ========================================================
    // Agent
    // ========================================================

    let agent_cancel = cancel.clone();

    tokio::spawn(async move {
        if let Err(error) = agent
            .run(input_rx, event_tx, agent_cancel, confirmation_rx)
            .await
        {
            tracing::error!("Agent stopped: {}", error);
        }
    });

    // --------------------------------------------------------
    // One-shot prompt from the command line.
    // --------------------------------------------------------

    if !args.prompt.is_empty() {
        let prompt = args.prompt.join(" ");

        if let Some(forwarded) = session.submit(prompt)? {
            input_tx.send(forwarded).await?;
        }
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

    #[cfg(feature = "tui")]
    {
        return run_tui(session, dashboard.events.subscribe(), input_tx, cancel).await;
    }

    // Without the Ratatui front end there is no terminal UI to fall back to.
    // The core is still reachable over JSONL, so point the user at it rather
    // than exiting silently.
    #[cfg(not(feature = "tui"))]
    {
        drop(session);

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

    let config = config::load()?;

    // ========================================================
    // Tools
    // ========================================================

    let mut tools = ToolRegistry::new();

    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);
    tools.register(GitStatus);
    tools.register(GitDiff);
    tools.register(FindSymbols);
    tools.register(FindReferences);
    tools.register(GitLog);
    tools.register(GitShow);
    tools.register(GitBlame);
    tools.register(GitBranches);
    tools.register(FileMetadata);
    tools.register(FindTodos);
    tools.register(CountLines);
    tools.register(InspectProject);
    tools.register(ListSkills);
    tools.register(LoadSkill);

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

    let model = model::DynamicModel::new(&config.model);

    let mut agent = Agent::new(model.clone(), planner, tools, history, galaxy);

    // ========================================================
    // Channels
    // ========================================================

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (confirmation_tx, confirmation_rx) = tokio::sync::mpsc::channel::<agent::Confirmation>(16);

    let cancel = CancellationToken::new();

    let info = SessionInfo::new(
        config.model.provider.clone(),
        config.model.name.clone(),
        std::env::current_dir()?.display().to_string(),
        tool_names,
    );

    let session = session::Session::new(info, confirmation_tx, cancel.clone());

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

    desktop::DesktopServer::new(session, input_tx, cancel, model)
        .run(event_rx)
        .await?;

    Ok(())
}
