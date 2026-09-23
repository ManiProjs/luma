use anyhow::{Context, Result, anyhow};
use dialoguer::{Confirm, Input, Password, Select, theme::ColorfulTheme};
use reqwest::Client;
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

use super::{Config, ModelConfig, config_path, save};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderProtocol {
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderCategory {
    Local,
    Cloud,
    Custom,
}

impl ProviderCategory {
    fn name(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Cloud => "Cloud",
            Self::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProviderDefinition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: ProviderCategory,
    protocol: ProviderProtocol,
    default_base_url: &'static str,
    requires_api_key: bool,
    supports_discovery: bool,
    api_key_env: Option<&'static str>,
}

const PROVIDERS: &[ProviderDefinition] = &[
    // ─────────────────────────────────────────────
    // Local
    // ─────────────────────────────────────────────
    ProviderDefinition {
        id: "ollama",
        name: "Ollama",
        description: "Local models managed by Ollama.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:11434/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "lm-studio",
        name: "LM Studio",
        description: "Local models served by LM Studio.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:1234/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "llama-cpp",
        name: "llama.cpp",
        description: "Models served by llama.cpp.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:8080/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "vllm",
        name: "vLLM",
        description: "Local or self-hosted vLLM server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:8000/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "localai",
        name: "LocalAI",
        description: "Local OpenAI-compatible inference server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:8080/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "text-generation-webui",
        name: "Text Generation WebUI",
        description: "Local inference through an OpenAI-compatible server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "http://localhost:5000/v1",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
    // ─────────────────────────────────────────────
    // Cloud
    // ─────────────────────────────────────────────
    ProviderDefinition {
        id: "openai",
        name: "OpenAI",
        description: "OpenAI models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.openai.com/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("OPENAI_API_KEY"),
    },
    ProviderDefinition {
        id: "openrouter",
        name: "OpenRouter",
        description: "Access models from many providers through one API.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://openrouter.ai/api/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("OPENROUTER_API_KEY"),
    },
    ProviderDefinition {
        id: "groq",
        name: "Groq",
        description: "High-speed inference through Groq.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.groq.com/openai/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("GROQ_API_KEY"),
    },
    ProviderDefinition {
        id: "together",
        name: "Together AI",
        description: "Hosted open models through Together AI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.together.xyz/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("TOGETHER_API_KEY"),
    },
    ProviderDefinition {
        id: "fireworks",
        name: "Fireworks AI",
        description: "Hosted inference through Fireworks AI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.fireworks.ai/inference/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("FIREWORKS_API_KEY"),
    },
    ProviderDefinition {
        id: "cerebras",
        name: "Cerebras",
        description: "Fast inference through Cerebras.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.cerebras.ai/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("CEREBRAS_API_KEY"),
    },
    ProviderDefinition {
        id: "deepseek",
        name: "DeepSeek",
        description: "DeepSeek models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.deepseek.com/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("DEEPSEEK_API_KEY"),
    },
    ProviderDefinition {
        id: "xai",
        name: "xAI",
        description: "Grok models through xAI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.x.ai/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("XAI_API_KEY"),
    },
    ProviderDefinition {
        id: "mistral",
        name: "Mistral AI",
        description: "Mistral models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.mistral.ai/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("MISTRAL_API_KEY"),
    },
    ProviderDefinition {
        id: "perplexity",
        name: "Perplexity",
        description: "Perplexity API models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.perplexity.ai",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("PERPLEXITY_API_KEY"),
    },
    ProviderDefinition {
        id: "cohere",
        name: "Cohere",
        description: "Cohere models through their compatible API.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://api.cohere.com/compatibility/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("COHERE_API_KEY"),
    },
    ProviderDefinition {
        id: "huggingface",
        name: "Hugging Face",
        description: "Models available through Hugging Face inference.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://router.huggingface.co/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("HF_TOKEN"),
    },
    ProviderDefinition {
        id: "nvidia",
        name: "NVIDIA NIM",
        description: "NVIDIA-hosted and self-hosted inference endpoints.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://integrate.api.nvidia.com/v1",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("NVIDIA_API_KEY"),
    },
    ProviderDefinition {
        id: "github-models",
        name: "GitHub Models",
        description: "Models available through GitHub's AI platform.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "https://models.inference.ai.azure.com",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("GITHUB_TOKEN"),
    },
    ProviderDefinition {
        id: "anthropic",
        name: "Anthropic",
        description: "Claude models through Anthropic.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::Anthropic,
        default_base_url: "https://api.anthropic.com",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("ANTHROPIC_API_KEY"),
    },
    ProviderDefinition {
        id: "gemini",
        name: "Google Gemini",
        description: "Gemini models through Google's API.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::Gemini,
        default_base_url: "https://generativelanguage.googleapis.com/v1beta",
        requires_api_key: true,
        supports_discovery: true,
        api_key_env: Some("GEMINI_API_KEY"),
    },
    // ─────────────────────────────────────────────
    // Custom
    // ─────────────────────────────────────────────
    ProviderDefinition {
        id: "custom-openai",
        name: "Custom OpenAI-compatible API",
        description: "Any server implementing the OpenAI API format.",
        category: ProviderCategory::Custom,
        protocol: ProviderProtocol::OpenAiCompatible,
        default_base_url: "",
        requires_api_key: false,
        supports_discovery: true,
        api_key_env: None,
    },
];

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

fn normalize_base_url(input: &str) -> Result<String> {
    let mut url = input.trim().trim_end_matches('/').to_string();

    if url.is_empty() {
        return Err(anyhow!("API base URL cannot be empty"));
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("API base URL must start with http:// or https://"));
    }

    for suffix in ["/chat/completions", "/completions", "/models"] {
        if url.ends_with(suffix) {
            url.truncate(url.len() - suffix.len());
            break;
        }
    }

    Ok(url.trim_end_matches('/').to_string())
}

fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

fn chat_completions_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

async fn fetch_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    let url = models_url(base_url);

    let client = Client::new();

    let mut request = client.get(&url);

    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|error| anyhow!("Could not connect to provider: {error}"))?;

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

    let response: ModelsResponse = response
        .json()
        .await
        .map_err(|error| anyhow!("Invalid /models response: {error}"))?;

    Ok(response.data.into_iter().map(|model| model.id).collect())
}

async fn test_openai_compatible_endpoint(
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
) -> Result<()> {
    let endpoint = chat_completions_url(base_url);

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

    let client = Client::new();

    let mut request = client.post(&endpoint).json(&body);

    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|error| anyhow!("Could not connect to provider: {error}"))?;

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

async fn test_anthropic(base_url: &str, api_key: &str, model: &str) -> Result<()> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "model": model,
        "max_tokens": 32,
        "messages": [
            {
                "role": "user",
                "content": "Reply with exactly: Luma connection successful."
            }
        ]
    });

    let response = Client::new()
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|error| anyhow!("Could not connect to Anthropic: {error}"))?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read response>".to_string());

        return Err(anyhow!("Anthropic returned HTTP {status}\n\n{body}"));
    }

    Ok(())
}

async fn test_gemini(base_url: &str, api_key: &str, model: &str) -> Result<()> {
    let url = format!(
        "{}/models/{}:generateContent?key={}",
        base_url.trim_end_matches('/'),
        model,
        api_key
    );

    let body = serde_json::json!({
        "contents": [
            {
                "parts": [
                    {
                        "text": "Reply with exactly: Luma connection successful."
                    }
                ]
            }
        ]
    });

    let response = Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|error| anyhow!("Could not connect to Google Gemini: {error}"))?;

    let status = response.status();

    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read response>".to_string());

        return Err(anyhow!("Gemini returned HTTP {status}\n\n{body}"));
    }

    Ok(())
}

async fn test_provider(
    provider: ProviderDefinition,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
) -> Result<()> {
    match provider.protocol {
        ProviderProtocol::OpenAiCompatible => {
            test_openai_compatible_endpoint(base_url, api_key, model).await
        }

        ProviderProtocol::Anthropic => {
            let key = api_key.ok_or_else(|| anyhow!("Anthropic API key is required."))?;

            test_anthropic(base_url, key, model).await
        }

        ProviderProtocol::Gemini => {
            let key = api_key.ok_or_else(|| anyhow!("Google Gemini API key is required."))?;

            test_gemini(base_url, key, model).await
        }
    }
}

async fn discover_models(
    provider: ProviderDefinition,
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>> {
    match provider.protocol {
        ProviderProtocol::OpenAiCompatible => fetch_models(base_url, api_key).await,

        ProviderProtocol::Anthropic => fetch_anthropic_models(base_url, api_key).await,

        ProviderProtocol::Gemini => fetch_gemini_models(base_url, api_key).await,
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
}

async fn fetch_anthropic_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    let key = api_key.ok_or_else(|| anyhow!("Anthropic API key is required."))?;

    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));

    let response = Client::new()
        .get(url)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        return Err(anyhow!("Anthropic returned HTTP {status}\n\n{body}"));
    }

    let response: AnthropicModelsResponse = response.json().await?;

    Ok(response.data.into_iter().map(|model| model.id).collect())
}

#[derive(Debug, Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Debug, Deserialize)]
struct GeminiModel {
    name: String,

    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

async fn fetch_gemini_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    let key = api_key.ok_or_else(|| anyhow!("Google Gemini API key is required."))?;

    let url = format!("{}/models?key={}", base_url.trim_end_matches('/'), key);

    let response = Client::new().get(url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        return Err(anyhow!("Gemini returned HTTP {status}\n\n{body}"));
    }

    let response: GeminiModelsResponse = response.json().await?;

    Ok(response
        .models
        .into_iter()
        .filter(|model| {
            model
                .supported_generation_methods
                .iter()
                .any(|method| method == "generateContent")
        })
        .map(|model| {
            model
                .name
                .strip_prefix("models/")
                .unwrap_or(&model.name)
                .to_string()
        })
        .collect())
}

fn read_env_api_key(provider: ProviderDefinition) -> Option<String> {
    let variable = provider.api_key_env?;

    env::var(variable)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn request_api_key(theme: &ColorfulTheme, provider: ProviderDefinition) -> Result<Option<String>> {
    if let Some(key) = read_env_api_key(provider) {
        let use_key = Confirm::with_theme(theme)
            .with_prompt(format!(
                "Use {} from {}?",
                provider.name,
                provider.api_key_env.unwrap()
            ))
            .default(true)
            .interact()?;

        if use_key {
            return Ok(Some(key));
        }
    }

    if !provider.requires_api_key {
        let wants_key = Confirm::with_theme(theme)
            .with_prompt("Use an API key?")
            .default(false)
            .interact()?;

        if !wants_key {
            return Ok(None);
        }
    }

    let key = Password::with_theme(theme)
        .with_prompt("API key")
        .allow_empty_password(!provider.requires_api_key)
        .interact()?;

    if key.trim().is_empty() {
        if provider.requires_api_key {
            return Err(anyhow!("{} requires an API key.", provider.name));
        }

        return Ok(None);
    }

    Ok(Some(key))
}

fn select_provider(theme: &ColorfulTheme) -> Result<ProviderDefinition> {
    let mut entries = Vec::new();

    for category in [
        ProviderCategory::Local,
        ProviderCategory::Cloud,
        ProviderCategory::Custom,
    ] {
        entries.push(format!("── {} ──", category.name()));

        for provider in PROVIDERS
            .iter()
            .filter(|provider| provider.category == category)
        {
            entries.push(format!("  {} — {}", provider.name, provider.description));
        }
    }

    let index = Select::with_theme(theme)
        .with_prompt("Choose your model provider")
        .items(&entries)
        .default(1)
        .interact()?;

    let selected = entries[index].trim();

    if selected.starts_with('─') {
        return select_provider(theme);
    }

    PROVIDERS
        .iter()
        .find(|provider| selected.starts_with(provider.name))
        .copied()
        .ok_or_else(|| anyhow!("Invalid provider selection"))
}

fn request_base_url(theme: &ColorfulTheme, provider: ProviderDefinition) -> Result<String> {
    let input = if provider.default_base_url.is_empty() {
        Input::<String>::with_theme(theme)
            .with_prompt("API base URL")
            .interact_text()?
    } else {
        Input::<String>::with_theme(theme)
            .with_prompt("API base URL")
            .default(provider.default_base_url.to_string())
            .interact_text()?
    };

    normalize_base_url(&input)
}

fn select_model(theme: &ColorfulTheme, models: &[String], prompt: &str) -> Result<String> {
    if models.is_empty() {
        return Input::<String>::with_theme(theme)
            .with_prompt(prompt)
            .validate_with(|input: &String| {
                if input.trim().is_empty() {
                    Err("Model name cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()
            .map(|value| value.trim().to_string())
            .map_err(Into::into);
    }

    let mut options = models.to_vec();
    options.push("Enter model manually".into());

    let index = Select::with_theme(theme)
        .with_prompt(prompt)
        .items(&options)
        .default(0)
        .interact()?;

    if index == models.len() {
        Input::<String>::with_theme(theme)
            .with_prompt("Model name")
            .validate_with(|input: &String| {
                if input.trim().is_empty() {
                    Err("Model name cannot be empty")
                } else {
                    Ok(())
                }
            })
            .interact_text()
            .map(|value| value.trim().to_string())
            .map_err(Into::into)
    } else {
        Ok(models[index].clone())
    }
}

async fn choose_model(
    theme: &ColorfulTheme,
    provider: ProviderDefinition,
    base_url: &str,
    api_key: Option<&str>,
    label: &str,
) -> Result<String> {
    println!();
    println!("Fetching models for {label}...");

    let mut models = Vec::new();

    match discover_models(provider, base_url, api_key).await {
        Ok(found) if !found.is_empty() => {
            println!("✓ Found {} model(s)", found.len());

            models = found;
        }

        Ok(_) => {
            println!("⚠ Provider returned no models.");
        }

        Err(error) => {
            println!("⚠ Automatic discovery failed:");
            println!("  {error}");
        }
    }

    select_model(theme, &models, label)
}

fn backup_existing_config() -> Result<Option<PathBuf>> {
    let path = config_path();

    if !path.is_file() {
        return Ok(None);
    }

    let backup = path.with_extension("toml.backup");

    fs::copy(&path, &backup).with_context(|| format!("Failed to back up {}", path.display()))?;

    Ok(Some(backup))
}

fn print_summary(
    provider: ProviderDefinition,
    endpoint: &str,
    model: &str,
    planner: &str,
    has_api_key: bool,
) {
    println!();
    println!("╭──────────────────────────────────────────────╮");
    println!("│              Luma Configuration              │");
    println!("╰──────────────────────────────────────────────╯");
    println!();
    println!("  Provider   {}", provider.name);
    println!("  Endpoint   {}", endpoint);
    println!("  Model      {}", model);
    println!("  Planner    {}", planner);
    println!(
        "  API key    {}",
        if has_api_key {
            "configured"
        } else {
            "not configured"
        }
    );
    println!();
    println!("  Workspace  current directory at runtime");
    println!("  Workspace is NOT saved to config.");
    println!();
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

    println!("Configure Luma's model providers and persistent settings.");

    println!("Your workspace is determined at runtime and is never saved here.");

    // --------------------------------------------------
    // Provider
    // --------------------------------------------------

    let provider = select_provider(&theme)?;

    println!();
    println!("Provider: {}", provider.name);
    println!("  {}", provider.description);

    // --------------------------------------------------
    // Endpoint
    // --------------------------------------------------

    let base_url = request_base_url(&theme, provider)?;

    println!("  Endpoint: {base_url}");

    // --------------------------------------------------
    // API key
    // --------------------------------------------------

    let api_key = request_api_key(&theme, provider)?;

    // --------------------------------------------------
    // Agent model
    // --------------------------------------------------

    let model = choose_model(
        &theme,
        provider,
        &base_url,
        api_key.as_deref(),
        "Choose agent model",
    )
    .await?;

    // --------------------------------------------------
    // Planner model
    // --------------------------------------------------

    println!();
    println!("Luma can use a separate model for planning.");

    let separate_planner = Confirm::with_theme(&theme)
        .with_prompt("Use a separate planner model?")
        .default(false)
        .interact()?;

    let planner = if separate_planner {
        choose_model(
            &theme,
            provider,
            &base_url,
            api_key.as_deref(),
            "Choose planner model",
        )
        .await?
    } else {
        model.clone()
    };

    // --------------------------------------------------
    // Connection test
    // --------------------------------------------------

    println!();
    println!("Testing Luma connection...");

    let test_result = test_provider(provider, &base_url, api_key.as_deref(), &model).await;

    match test_result {
        Ok(()) => {
            println!("✓ Connection successful.");
        }

        Err(error) => {
            println!("✗ Connection test failed.");
            println!();
            println!("{error}");
            println!();

            let continue_anyway = Confirm::with_theme(&theme)
                .with_prompt("Save this configuration anyway?")
                .default(false)
                .interact()?;

            if !continue_anyway {
                println!();
                println!("Setup cancelled.");
                return Ok(());
            }

            println!("⚠ Saving without a successful connection test.");
        }
    }

    // --------------------------------------------------
    // Summary
    // --------------------------------------------------

    print_summary(provider, &base_url, &model, &planner, api_key.is_some());

    let save_config = Confirm::with_theme(&theme)
        .with_prompt("Save this configuration?")
        .default(true)
        .interact()?;

    if !save_config {
        println!();
        println!("Setup cancelled.");
        return Ok(());
    }

    // --------------------------------------------------
    // Existing configuration
    // --------------------------------------------------

    if config_path().is_file() {
        println!();
        println!("A Luma configuration already exists:");
        println!("  {}", config_path().display());

        let overwrite = Confirm::with_theme(&theme)
            .with_prompt("Replace the existing configuration?")
            .default(false)
            .interact()?;

        if !overwrite {
            println!();
            println!("Setup cancelled.");
            return Ok(());
        }
    }

    // --------------------------------------------------
    // Build configuration
    // --------------------------------------------------

    let config = Config {
        model: ModelConfig {
            provider: provider.id.to_string(),
            endpoint: base_url.clone(),
            name: model.clone(),
            api_key: api_key.clone(),
        },

        planner: ModelConfig {
            provider: provider.id.to_string(),
            endpoint: base_url.clone(),
            name: planner.clone(),
            api_key: api_key.clone(),
        },
    };

    // --------------------------------------------------
    // Backup
    // --------------------------------------------------

    let backup = backup_existing_config()?;

    // --------------------------------------------------
    // Save
    // --------------------------------------------------

    save(&config)?;

    println!();
    println!("╭──────────────────────────────────────────────╮");
    println!("│              ✓ Luma is ready                │");
    println!("╰──────────────────────────────────────────────╯");
    println!();
    println!("  Provider   {}", provider.name);
    println!("  Model      {}", model);
    println!("  Planner    {}", planner);
    println!("  Endpoint   {}", base_url);
    println!();
    println!("  Config     {}", config_path().display());

    if let Some(backup) = backup {
        println!("  Backup     {}", backup.display());
    }

    println!();
    println!("  Workspace  current directory");
    println!("  Status     workspace is runtime-only");
    println!();

    Ok(())
}
