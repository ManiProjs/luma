use anyhow::{Result, anyhow};
use dialoguer::{Input, Password, Select, theme::ColorfulTheme};
use reqwest::Client;
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

#[derive(Debug, Clone, Copy)]
enum ProviderKind {
    Ollama,
    LmStudio,
    LlamaCpp,
    Vllm,
    LocalAi,
    OpenAi,
    OpenRouter,
    Anthropic,
    Gemini,
    Groq,
    Together,
    CustomOpenAi,
}

impl ProviderKind {
    fn name(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::LmStudio => "LM Studio",
            Self::LlamaCpp => "llama.cpp Server",
            Self::Vllm => "vLLM",
            Self::LocalAi => "LocalAI",
            Self::OpenAi => "OpenAI",
            Self::OpenRouter => "OpenRouter",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Google Gemini",
            Self::Groq => "Groq",
            Self::Together => "Together AI",
            Self::CustomOpenAi => "Custom OpenAI-compatible API",
        }
    }

    fn all() -> &'static [Self] {
        &[
            Self::Ollama,
            Self::LmStudio,
            Self::LlamaCpp,
            Self::Vllm,
            Self::LocalAi,
            Self::OpenAi,
            Self::OpenRouter,
            Self::Anthropic,
            Self::Gemini,
            Self::Groq,
            Self::Together,
            Self::CustomOpenAi,
        ]
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::Ollama => "http://localhost:11434/v1",
            Self::LmStudio => "http://localhost:1234/v1",
            Self::LlamaCpp => "http://localhost:8080/v1",
            Self::Vllm => "http://localhost:8000/v1",
            Self::LocalAi => "http://localhost:8080/v1",
            Self::OpenAi => "https://api.openai.com/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::Together => "https://api.together.xyz/v1",
            Self::CustomOpenAi => "",
        }
    }

    fn is_openai_compatible(self) -> bool {
        matches!(
            self,
            Self::Ollama
                | Self::LmStudio
                | Self::LlamaCpp
                | Self::Vllm
                | Self::LocalAi
                | Self::OpenAi
                | Self::OpenRouter
                | Self::Groq
                | Self::Together
                | Self::CustomOpenAi
        )
    }

    fn needs_api_key(self) -> bool {
        matches!(
            self,
            Self::OpenAi
                | Self::OpenRouter
                | Self::Anthropic
                | Self::Gemini
                | Self::Groq
                | Self::Together
                | Self::CustomOpenAi
        )
    }

    fn supports_model_discovery(self) -> bool {
        self.is_openai_compatible()
    }
}

/// Normalize a provider base URL.
///
/// Examples:
///
/// https://logfare.ai/v1
///     -> https://logfare.ai/v1
///
/// https://logfare.ai/v1/
///     -> https://logfare.ai/v1
///
/// https://logfare.ai/v1/chat/completions
///     -> https://logfare.ai/v1
///
/// https://logfare.ai/v1/models
///     -> https://logfare.ai/v1
fn normalize_base_url(input: &str) -> Result<String> {
    let mut url = input.trim().trim_end_matches('/').to_string();

    if url.is_empty() {
        return Err(anyhow!("API base URL cannot be empty"));
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("API base URL must start with http:// or https://"));
    }

    for suffix in ["/chat/completions", "/models"] {
        if url.ends_with(suffix) {
            url.truncate(url.len() - suffix.len());
            break;
        }
    }

    Ok(url.trim_end_matches('/').to_string())
}

fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

async fn fetch_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    let url = models_url(base_url);

    let client = Client::new();

    let mut request = client.get(&url);

    if let Some(key) = api_key {
        if !key.trim().is_empty() {
            request = request.bearer_auth(key);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| anyhow!("Could not connect to provider: {e}"))?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read response>".to_string());

        return Err(anyhow!(
            "Model discovery failed with HTTP {status}\n\n{body}"
        ));
    }

    let models: ModelsResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Invalid /models response: {e}"))?;

    Ok(models.data.into_iter().map(|model| model.id).collect())
}

async fn test_openai_compatible_endpoint(
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
) -> Result<()> {
    let endpoint = chat_completions_url(base_url);

    let client = Client::new();

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": "Reply with exactly: Luma connection successful."
            }
        ],
        "max_tokens": 32,
        "temperature": 0,
        "stream": false
    });

    let mut request = client.post(&endpoint).json(&body);

    if let Some(key) = api_key {
        if !key.trim().is_empty() {
            request = request.bearer_auth(key);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| anyhow!("Could not connect to provider: {e}"))?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read response>".to_string());

        return Err(anyhow!("Provider returned HTTP {status}\n\n{body}"));
    }

    Ok(())
}

async fn test_provider(
    provider: ProviderKind,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
) -> Result<()> {
    match provider {
        ProviderKind::Ollama
        | ProviderKind::LmStudio
        | ProviderKind::LlamaCpp
        | ProviderKind::Vllm
        | ProviderKind::LocalAi
        | ProviderKind::OpenAi
        | ProviderKind::OpenRouter
        | ProviderKind::Groq
        | ProviderKind::Together
        | ProviderKind::CustomOpenAi => {
            test_openai_compatible_endpoint(base_url, api_key, model).await
        }

        ProviderKind::Anthropic => Err(anyhow!(
            "Anthropic support is not implemented yet. \
             Luma needs the native Anthropic Messages API."
        )),

        ProviderKind::Gemini => Err(anyhow!(
            "Google Gemini support is not implemented yet. \
             Luma needs the native Gemini API."
        )),
    }
}

fn escape_toml_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn api_key_toml(api_key: Option<&str>) -> String {
    match api_key {
        Some(key) if !key.trim().is_empty() => {
            format!("api_key = \"{}\"", escape_toml_string(key))
        }

        _ => String::new(),
    }
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

    // --------------------------------------------------
    // Provider
    // --------------------------------------------------

    let providers = ProviderKind::all();

    let provider_names: Vec<&str> = providers.iter().map(|p| p.name()).collect();

    let provider_index = Select::with_theme(&theme)
        .with_prompt("Choose your model provider")
        .items(&provider_names)
        .default(0)
        .interact()?;

    let provider = providers[provider_index];

    println!("\nProvider: {}", provider.name());

    // --------------------------------------------------
    // Base URL
    // --------------------------------------------------

    let raw_base_url = if matches!(provider, ProviderKind::CustomOpenAi) {
        Input::<String>::with_theme(&theme)
            .with_prompt("API base URL")
            .validate_with(|input: &String| normalize_base_url(input).map(|_| ()))
            .interact_text()?
    } else {
        Input::<String>::with_theme(&theme)
            .with_prompt("API base URL")
            .default(provider.default_base_url().to_string())
            .validate_with(|input: &String| normalize_base_url(input).map(|_| ()))
            .interact_text()?
    };

    let base_url = normalize_base_url(&raw_base_url)?;

    println!("  Base URL: {}", base_url);

    if provider.is_openai_compatible() {
        println!("  Chat endpoint: {}", chat_completions_url(&base_url));
    }

    // --------------------------------------------------
    // API key
    // --------------------------------------------------

    let api_key = if provider.needs_api_key() {
        let key = Password::with_theme(&theme)
            .with_prompt("API key (leave empty if not required)")
            .allow_empty_password(true)
            .interact()?;

        if key.trim().is_empty() {
            None
        } else {
            Some(key)
        }
    } else {
        None
    };

    // --------------------------------------------------
    // Model discovery
    // --------------------------------------------------

    let mut models = Vec::new();

    if provider.supports_model_discovery() {
        println!("\nFetching available models...");

        match fetch_models(&base_url, api_key.as_deref()).await {
            Ok(found_models) if !found_models.is_empty() => {
                models = found_models;

                println!("✓ Found {} model(s)", models.len());
            }

            Ok(_) => {
                println!("⚠ Provider returned no models.");
            }

            Err(error) => {
                println!("⚠ Automatic model discovery unavailable:");
                println!("  {error}");
            }
        }
    }

    // --------------------------------------------------
    // Model
    // --------------------------------------------------

    let model = if models.is_empty() {
        Input::<String>::with_theme(&theme)
            .with_prompt("Model name")
            .validate_with(|input: &String| {
                if input.trim().is_empty() {
                    Err("Model name cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()?
    } else {
        let model_index = Select::with_theme(&theme)
            .with_prompt("Choose model")
            .items(&models)
            .default(0)
            .interact()?;

        models[model_index].clone()
    };

    // --------------------------------------------------
    // Connection test
    // --------------------------------------------------

    println!("\nTesting provider connection...");

    match test_provider(provider, &base_url, api_key.as_deref(), &model).await {
        Ok(()) => {
            println!("✓ Connection successful");
        }

        Err(error) => {
            println!("✗ Connection failed");
            println!();
            println!("{error}");
            println!();

            let choices = ["Save configuration anyway", "Cancel setup"];

            let choice = Select::with_theme(&theme)
                .with_prompt("What would you like to do?")
                .items(choices)
                .default(0)
                .interact()?;

            match choice {
                0 => {
                    println!("⚠ Saving configuration without a successful test.");
                }

                1 => {
                    println!("Setup cancelled.");
                    return Ok(());
                }

                _ => unreachable!(),
            }
        }
    }

    // --------------------------------------------------
    // Configuration directory
    // --------------------------------------------------

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| ".".into())
        .join("luma");

    fs::create_dir_all(&config_dir)?;

    let path = config_dir.join("config.toml");

    // --------------------------------------------------
    // TOML values
    // --------------------------------------------------

    let provider_name = escape_toml_string(provider.name());
    let base_url = escape_toml_string(&base_url);
    let model = escape_toml_string(&model);

    let api_key_line = api_key_toml(api_key.as_deref());

    // --------------------------------------------------
    // Configuration
    // --------------------------------------------------

    let config = format!(
        r#"[model]
provider = "{provider_name}"
endpoint = "{base_url}"
name = "{model}"
{api_key_line}

[planner]
provider = "{provider_name}"
endpoint = "{base_url}"
name = "{model}"
{api_key_line}
"#
    );

    fs::write(&path, config)?;

    // --------------------------------------------------
    // Done
    // --------------------------------------------------

    println!("\n✓ Configuration saved");
    println!("  {}", path.display());

    println!("\nUsing:");
    println!("  Provider: {}", provider.name());
    println!("  Model:    {}", model);
    println!("  Base URL: {}", base_url);

    if provider.is_openai_compatible() {
        println!("  Chat:     {}", chat_completions_url(&base_url));
    }

    Ok(())
}
