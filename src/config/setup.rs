use anyhow::{Result, anyhow};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

async fn fetch_models(endpoint: &str, api_key: Option<&String>) -> Result<Vec<String>> {
    let base = endpoint.replace("/chat/completions", "/models");

    let client = reqwest::Client::new();

    let mut request = client.get(base);

    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Provider does not support model discovery"));
    }

    let models: ModelsResponse = response.json().await?;

    Ok(models.data.into_iter().map(|m| m.id).collect())
}

pub async fn run() -> Result<()> {
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

    let providers = vec![
        "Ollama",
        "LM Studio",
        "llama.cpp Server",
        "vLLM",
        "LocalAI",
        "OpenAI",
        "OpenRouter",
        "Anthropic",
        "Google Gemini",
        "Groq",
        "Together AI",
        "Custom OpenAI-compatible API",
    ];

    let provider_index = Select::with_theme(&theme)
        .with_prompt("Choose your model provider")
        .items(&providers)
        .default(0)
        .interact()?;

    let provider = providers[provider_index].to_string();

    let endpoint = match provider_index {
        0 => "http://localhost:11434/v1/chat/completions".to_string(),

        1 => "http://localhost:1234/v1/chat/completions".to_string(),

        2 => "http://localhost:8080/v1/chat/completions".to_string(),

        3 => "http://localhost:8000/v1/chat/completions".to_string(),

        4 => "http://localhost:8080/v1/chat/completions".to_string(),

        5 => "https://api.openai.com/v1/chat/completions".to_string(),

        6 => "https://openrouter.ai/api/v1/chat/completions".to_string(),

        7 => "https://api.anthropic.com/v1/messages".to_string(),

        8 => "https://generativelanguage.googleapis.com/v1beta".to_string(),

        9 => "https://api.groq.com/openai/v1/chat/completions".to_string(),

        10 => "https://api.together.xyz/v1/chat/completions".to_string(),

        _ => Input::<String>::with_theme(&theme)
            .with_prompt("API endpoint")
            .interact_text()?,
    };

    let api_key = if matches!(provider_index, 5 | 6 | 7 | 8 | 9 | 10) {
        Some(
            Input::<String>::with_theme(&theme)
                .with_prompt("API key")
                .interact_text()?,
        )
    } else {
        None
    };

    println!("\nFetching available models...");

    let models = fetch_models(&endpoint, api_key.as_ref())
        .await
        .unwrap_or_default();

    let model = if models.is_empty() {
        Input::<String>::with_theme(&theme)
            .with_prompt("Model name")
            .interact_text()?
    } else {
        let index = Select::with_theme(&theme)
            .with_prompt("Choose model")
            .items(&models)
            .default(0)
            .interact()?;

        models[index].clone()
    };

    let planner_model = model.clone();

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| ".".into())
        .join("luma");

    fs::create_dir_all(&config_dir)?;

    let api_key_line = match api_key {
        Some(key) if !key.is_empty() => format!("api_key = \"{}\"", key),

        _ => String::new(),
    };

    let config = format!(
        r#"
[model]
provider = "{provider}"
endpoint = "{endpoint}"
name = "{model}"
{api_key_line}

[planner]
provider = "{provider}"
endpoint = "{endpoint}"
name = "{planner_model}"
{api_key_line}
"#
    );

    let path = config_dir.join("config.toml");

    fs::write(&path, config.trim_start())?;

    println!("\n✓ Configuration saved");
    println!("  {}", path.display());

    println!("\nUsing:");
    println!("  {} ({})", provider, model);

    Ok(())
}
