mod agent;
mod config;
mod context;
mod event;
mod history;
mod model;
mod planner;
mod theme;
mod tools;
mod tui;
mod workspace;

use clap::{Parser, Subcommand};

use agent::Agent;
use event::AgentEvent;
use model::OpenAICompatibleModel;

use tools::{
    ToolRegistry,
    filesystem::{list_directory::ListDirectory, read_file::ReadFile, search_files::SearchFiles},
    shell::RunCommand,
};

use history::History;

use std::io::{self, Write};

use tokio_util::sync::CancellationToken;

use crate::{
    tools::{Tool, filesystem::write_file::WriteFile},
    tui::info::ModelInfo,
};

#[derive(Parser, Debug)]
#[command(name = "luma", version, about = "A lightweight AI coding agent")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Normal prompt
    #[arg(trailing_var_arg = true)]
    prompt: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configure Luma
    Setup,
}

fn confirm_setup() -> anyhow::Result<bool> {
    print!("\nLuma is not configured yet.\n\nRun setup? [Y/n] ");

    io::stdout().flush()?;

    let mut input = String::new();

    io::stdin().read_line(&mut input)?;

    let answer = input.trim().to_lowercase();

    Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if !config::exists() && !matches!(args.command, Some(Commands::Setup)) {
        if confirm_setup()? {
            config::setup::run().await?;

            return Ok(());
        } else {
            println!("Luma cannot start without configuration.");

            return Ok(());
        }
    }

    match args.command {
        Some(Commands::Setup) => {
            config::setup::run().await?;

            return Ok(());
        }

        None => {}
    }

    let cli_prompt = if args.prompt.is_empty() {
        None
    } else {
        Some(args.prompt.join(" "))
    };

    let config = config::load()?;

    let model =
        OpenAICompatibleModel::new(config.model.endpoint.clone(), config.model.name.clone());

    let mut tools = ToolRegistry::new();

    tools.register(ReadFile);

    tools.register(ListDirectory);

    tools.register(RunCommand);

    tools.register(SearchFiles);

    tools.register(WriteFile);

    let tool_names = tools.names();

    let planner_model =
        OpenAICompatibleModel::new(config.planner.endpoint.clone(), config.planner.name.clone());

    let planner = planner::Planner::new(planner_model, &tools);

    let history = History::load();

    let mut agent = Agent::new(model, planner, tools, history);

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    let cancel: CancellationToken = CancellationToken::new();

    let agent_cancel: CancellationToken = cancel.clone();

    tokio::spawn(async move {
        if let Err(error) = agent.run(input_rx, event_tx.clone(), cancel.clone()).await {
            let _ = event_tx.send(AgentEvent::Error(error.to_string())).await;
        }
    });
    // Send CLI prompt as first message

    if let Some(prompt) = cli_prompt {
        input_tx.send(prompt).await?;
    }

    tui::terminal::run(
        event_rx,
        input_tx,
        agent_cancel,
        ModelInfo {
            provider: config.model.provider.clone(),
            model: config.model.name.clone(),
        },
        tool_names,
    )
    .await?;

    Ok(())
}
