mod agent;
mod config;
mod context;
mod event;
mod model;
mod planner;
mod theme;
mod tools;
mod tui;
mod workspace;

use clap::Parser;

use agent::Agent;
use event::AgentEvent;
use model::OpenAICompatibleModel;

use tools::{
    ToolRegistry,
    filesystem::{ListDirectory, ReadFile},
    shell::RunCommand,
};

#[derive(Parser, Debug)]
#[command(name = "luma", version, about = "A lightweight AI coding agent")]
struct Args {
    /// Optional initial prompt
    #[arg()]
    prompt: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = config::Config::load()?;

    let model =
        OpenAICompatibleModel::new(config.model.endpoint.clone(), config.model.name.clone());

    let mut tools = ToolRegistry::new();

    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);

    let planner_model =
        OpenAICompatibleModel::new(config.planner.endpoint.clone(), config.planner.name.clone());

    let planner = planner::Planner::new(planner_model, &tools);

    let mut agent = Agent::new(model, planner, tools);

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(100);

    let (input_tx, input_rx) = tokio::sync::mpsc::channel::<String>(100);

    tokio::spawn(async move {
        if let Err(error) = agent.run(input_rx, event_tx.clone()).await {
            let _ = event_tx.send(AgentEvent::Error(error.to_string())).await;
        }
    });

    // Send CLI argument as first message
    if !args.prompt.is_empty() {
        input_tx.send(args.prompt.join(" ")).await?;
    }

    tui::terminal::run(event_rx, input_tx).await?;

    Ok(())
}
