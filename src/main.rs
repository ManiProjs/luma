mod agent;
mod config;
mod context;
mod event;
mod model;
mod planner;
mod tools;
mod ui;
mod workspace;

use std::io::Write;

use clap::Parser;

use agent::Agent;
use event::AgentEvent;
use model::OpenAICompatibleModel;
use ui::Renderer;

use tools::{
    ToolRegistry,
    filesystem::{ListDirectory, ReadFile},
    shell::RunCommand,
};

#[derive(Parser, Debug)]
#[command(name = "luma", version, about = "A lightweight AI coding agent")]
struct Args {
    /// Prompt for Luma
    #[arg(required = true)]
    prompt: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!(
        r#"
✨ Luma
A lightweight local AI coding agent
"#
    );

    let input = args.prompt.join(" ");

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

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    tokio::spawn(async move {
        if let Err(error) = agent.run(input, tx.clone()).await {
            let _ = tx.send(AgentEvent::Error(error.to_string())).await;
        }
    });

    let mut renderer = Renderer::new();

    while let Some(event) = rx.recv().await {
        renderer.handle(event);

        std::io::stdout().flush().unwrap();
    }

    Ok(())
}
