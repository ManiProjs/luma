use anyhow::Result;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use std::fs;

pub fn run() -> Result<()> {
    println!();

    println!(
        r#"
██╗     ██╗   ██╗███╗   ███╗ █████╗ 
██║     ██║   ██║████╗ ████║██╔══██╗
██║     ██║   ██║██╔████╔██║███████║
██║     ██║   ██║██║╚██╔╝██║██╔══██║
███████╗╚██████╔╝██║ ╚═╝ ██║██║  ██║
╚══════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝  ╚═╝

          Luma Setup Wizard
"#
    );

    let theme = ColorfulTheme::default();

    println!("Let's configure your local AI coding agent.\n");

    let providers = vec!["Ollama", "LM Studio", "Custom OpenAI-compatible API"];

    let provider_index = Select::with_theme(&theme)
        .with_prompt("Choose your model provider")
        .items(&providers)
        .default(0)
        .interact()?;

    let provider = providers[provider_index].to_string();

    let endpoint = match provider_index {
        0 => "http://localhost:11434/v1/chat/completions".to_string(),

        1 => "http://localhost:1234/v1/chat/completions".to_string(),

        _ => Input::with_theme(&theme)
            .with_prompt("API endpoint")
            .interact_text()?,
    };

    let model: String = Input::with_theme(&theme)
        .with_prompt("Model name")
        .default("qwen2.5-coder:3b".to_string())
        .interact_text()?;

    let planner_model: String = Input::with_theme(&theme)
        .with_prompt("Planner model")
        .default(model.clone())
        .interact_text()?;

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("luma");

    fs::create_dir_all(&config_dir)?;

    let config_file = config_dir.join("config.toml");

    let content = format!(
        r#"
[model]
provider = "{provider}"
endpoint = "{endpoint}"
name = "{model}"

[planner]
provider = "{provider}"
endpoint = "{endpoint}"
name = "{planner_model}"
"#
    );

    fs::write(&config_file, content.trim_start())?;

    println!();

    println!("✓ Configuration saved:");

    println!("  {}", config_file.display());

    println!();

    println!("Configured model:");

    println!("  {} ({})", provider, model);

    println!();

    println!("Run:");

    println!("  luma \"hello\"");

    println!();

    Ok(())
}
