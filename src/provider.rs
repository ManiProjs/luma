use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// The wire protocol used by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderProtocol {
    OpenAiCompatible,
    Anthropic,
    Gemini,
}

/// Broad provider category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderCategory {
    Local,
    Cloud,
    Custom,
}

/// Complete description of a supported provider.
///
/// This is the canonical provider registry used by:
/// - CLI setup
/// - Desktop setup
/// - model discovery
/// - connection testing
/// - future agent runtime code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedProvider {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: ProviderCategory,
    pub protocol: ProviderProtocol,
    pub endpoint: String,
    pub requires_api_key: bool,
    pub supports_model_discovery: bool,
    pub api_key_env: Option<String>,
}

/// Model returned by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub context_window: Option<u64>,
}

/// Request used by the generic provider implementation.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub provider: SupportedProvider,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
}

/// Provider implementation.
#[async_trait]
pub trait Provider: Send + Sync {
    async fn list_models(&self, request: &ModelRequest) -> Result<Vec<SupportedModel>>;

    async fn test(&self, request: &ModelRequest, model: &str) -> Result<()>;
}

// ─────────────────────────────────────────────
// Registry
// ─────────────────────────────────────────────

struct ProviderDefinition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: ProviderCategory,
    protocol: ProviderProtocol,
    endpoint: &'static str,
    requires_api_key: bool,
    supports_model_discovery: bool,
    api_key_env: Option<&'static str>,
}

const PROVIDERS: &[ProviderDefinition] = &[
    // ─────────────────────────────────────────
    // Local
    // ─────────────────────────────────────────
    ProviderDefinition {
        id: "ollama",
        name: "Ollama",
        description: "Local models managed by Ollama.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:11434/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "lm-studio",
        name: "LM Studio",
        description: "Local models served by LM Studio.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:1234/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "llama-cpp",
        name: "llama.cpp",
        description: "Models served by llama.cpp.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:8080/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "vllm",
        name: "vLLM",
        description: "Local or self-hosted vLLM server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:8000/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "localai",
        name: "LocalAI",
        description: "Local OpenAI-compatible inference server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:8080/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    ProviderDefinition {
        id: "text-generation-webui",
        name: "Text Generation WebUI",
        description: "Local inference through an OpenAI-compatible server.",
        category: ProviderCategory::Local,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "http://localhost:5000/v1",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
    // ─────────────────────────────────────────
    // Cloud
    // ─────────────────────────────────────────
    ProviderDefinition {
        id: "openai",
        name: "OpenAI",
        description: "OpenAI models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.openai.com/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("OPENAI_API_KEY"),
    },
    ProviderDefinition {
        id: "openrouter",
        name: "OpenRouter",
        description: "Access models from many providers through one API.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://openrouter.ai/api/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("OPENROUTER_API_KEY"),
    },
    ProviderDefinition {
        id: "groq",
        name: "Groq",
        description: "High-speed inference through Groq.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.groq.com/openai/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("GROQ_API_KEY"),
    },
    ProviderDefinition {
        id: "together",
        name: "Together AI",
        description: "Hosted open models through Together AI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.together.xyz/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("TOGETHER_API_KEY"),
    },
    ProviderDefinition {
        id: "fireworks",
        name: "Fireworks AI",
        description: "Hosted inference through Fireworks AI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.fireworks.ai/inference/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("FIREWORKS_API_KEY"),
    },
    ProviderDefinition {
        id: "cerebras",
        name: "Cerebras",
        description: "Fast inference through Cerebras.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.cerebras.ai/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("CEREBRAS_API_KEY"),
    },
    ProviderDefinition {
        id: "deepseek",
        name: "DeepSeek",
        description: "DeepSeek models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.deepseek.com/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("DEEPSEEK_API_KEY"),
    },
    ProviderDefinition {
        id: "xai",
        name: "xAI",
        description: "Grok models through xAI.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.x.ai/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("XAI_API_KEY"),
    },
    ProviderDefinition {
        id: "mistral",
        name: "Mistral AI",
        description: "Mistral models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.mistral.ai/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("MISTRAL_API_KEY"),
    },
    ProviderDefinition {
        id: "perplexity",
        name: "Perplexity",
        description: "Perplexity API models.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.perplexity.ai",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("PERPLEXITY_API_KEY"),
    },
    ProviderDefinition {
        id: "cohere",
        name: "Cohere",
        description: "Cohere models through the compatible API.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://api.cohere.com/compatibility/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("COHERE_API_KEY"),
    },
    ProviderDefinition {
        id: "huggingface",
        name: "Hugging Face",
        description: "Models through Hugging Face inference.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://router.huggingface.co/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("HF_TOKEN"),
    },
    ProviderDefinition {
        id: "nvidia",
        name: "NVIDIA NIM",
        description: "NVIDIA-hosted and self-hosted inference.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://integrate.api.nvidia.com/v1",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("NVIDIA_API_KEY"),
    },
    ProviderDefinition {
        id: "github-models",
        name: "GitHub Models",
        description: "Models available through GitHub.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "https://models.inference.ai.azure.com",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("GITHUB_TOKEN"),
    },
    ProviderDefinition {
        id: "anthropic",
        name: "Anthropic",
        description: "Claude models through Anthropic.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::Anthropic,
        endpoint: "https://api.anthropic.com",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("ANTHROPIC_API_KEY"),
    },
    ProviderDefinition {
        id: "gemini",
        name: "Google Gemini",
        description: "Gemini models through Google.",
        category: ProviderCategory::Cloud,
        protocol: ProviderProtocol::Gemini,
        endpoint: "https://generativelanguage.googleapis.com/v1beta",
        requires_api_key: true,
        supports_model_discovery: true,
        api_key_env: Some("GEMINI_API_KEY"),
    },
    // ─────────────────────────────────────────
    // Custom
    // ─────────────────────────────────────────
    ProviderDefinition {
        id: "custom-openai",
        name: "Custom OpenAI-compatible API",
        description: "Any server implementing the OpenAI API format.",
        category: ProviderCategory::Custom,
        protocol: ProviderProtocol::OpenAiCompatible,
        endpoint: "",
        requires_api_key: false,
        supports_model_discovery: true,
        api_key_env: None,
    },
];

/// Return every supported provider.
pub fn supported_providers() -> Vec<SupportedProvider> {
    PROVIDERS
        .iter()
        .map(|provider| SupportedProvider {
            id: provider.id.to_string(),
            name: provider.name.to_string(),
            description: provider.description.to_string(),
            category: provider.category,
            protocol: provider.protocol,
            endpoint: provider.endpoint.to_string(),
            requires_api_key: provider.requires_api_key,
            supports_model_discovery: provider.supports_model_discovery,
            api_key_env: provider.api_key_env.map(str::to_string),
        })
        .collect()
}

/// Find a provider by its stable ID.
pub fn provider_by_id(id: &str) -> Option<SupportedProvider> {
    PROVIDERS
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| SupportedProvider {
            id: provider.id.to_string(),
            name: provider.name.to_string(),
            description: provider.description.to_string(),
            category: provider.category,
            protocol: provider.protocol,
            endpoint: provider.endpoint.to_string(),
            requires_api_key: provider.requires_api_key,
            supports_model_discovery: provider.supports_model_discovery,
            api_key_env: provider.api_key_env.map(str::to_string),
        })
}

/// Find the runtime implementation for a provider.
pub fn provider_for(provider: &SupportedProvider) -> Box<dyn Provider> {
    match provider.protocol {
        ProviderProtocol::OpenAiCompatible => Box::new(OpenAiCompatibleProvider),

        ProviderProtocol::Anthropic => Box::new(AnthropicProvider),

        ProviderProtocol::Gemini => Box::new(GeminiProvider),
    }
}

// ─────────────────────────────────────────────
// OpenAI-compatible provider
// ─────────────────────────────────────────────

struct OpenAiCompatibleProvider;

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,

    #[serde(default)]
    owned_by: Option<String>,
}

#[async_trait]
impl Provider for OpenAiCompatibleProvider {
    async fn list_models(&self, request: &ModelRequest) -> Result<Vec<SupportedModel>> {
        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = models_url(endpoint);

        let client = Client::new();

        let mut http_request = client.get(url);

        if let Some(key) = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
        {
            http_request = http_request.bearer_auth(key);
        }

        let response = http_request.send().await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();

            return Err(anyhow!("Provider returned HTTP {status}\n\n{body}"));
        }

        let models: OpenAiModelsResponse = response.json().await?;

        Ok(models
            .data
            .into_iter()
            .map(|model| SupportedModel {
                id: model.id.clone(),
                name: model.id,
                description: model.owned_by,
                context_window: None,
            })
            .collect())
    }

    async fn test(&self, request: &ModelRequest, model: &str) -> Result<()> {
        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = chat_completions_url(endpoint);

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content":
                        "Reply with exactly: Luma connection successful."
                }
            ],
            "max_tokens": 32,
            "temperature": 0,
            "stream": false
        });

        let client = Client::new();

        let mut http_request = client.post(url).json(&body);

        if let Some(key) = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
        {
            http_request = http_request.bearer_auth(key);
        }

        let response = http_request.send().await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();

            return Err(anyhow!("Provider returned HTTP {status}\n\n{body}"));
        }

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Anthropic
// ─────────────────────────────────────────────

struct AnthropicProvider;

#[derive(Debug, Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Debug, Deserialize)]
struct AnthropicModel {
    id: String,
    display_name: Option<String>,
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn list_models(&self, request: &ModelRequest) -> Result<Vec<SupportedModel>> {
        let key = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| anyhow!("Anthropic API key is required."))?;

        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));

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

        Ok(response
            .data
            .into_iter()
            .map(|model| SupportedModel {
                id: model.id.clone(),
                name: model.display_name.unwrap_or(model.id),
                description: None,
                context_window: None,
            })
            .collect())
    }

    async fn test(&self, request: &ModelRequest, model: &str) -> Result<()> {
        let key = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| anyhow!("Anthropic API key is required."))?;

        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = format!("{}/v1/messages", endpoint.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": model,
            "max_tokens": 32,
            "messages": [
                {
                    "role": "user",
                    "content":
                        "Reply with exactly: Luma connection successful."
                }
            ]
        });

        let response = Client::new()
            .post(url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();

            let body = response.text().await.unwrap_or_default();

            return Err(anyhow!("Anthropic returned HTTP {status}\n\n{body}"));
        }

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Gemini
// ─────────────────────────────────────────────

struct GeminiProvider;

#[derive(Debug, Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Debug, Deserialize)]
struct GeminiModel {
    name: String,

    #[serde(default)]
    display_name: Option<String>,

    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn list_models(&self, request: &ModelRequest) -> Result<Vec<SupportedModel>> {
        let key = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| anyhow!("Google Gemini API key is required."))?;

        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = format!("{}/models?key={}", endpoint.trim_end_matches('/'), key);

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
                let id = model
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(&model.name)
                    .to_string();

                SupportedModel {
                    id: id.clone(),
                    name: model.display_name.unwrap_or(id),
                    description: None,
                    context_window: None,
                }
            })
            .collect())
    }

    async fn test(&self, request: &ModelRequest, model: &str) -> Result<()> {
        let key = request
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| anyhow!("Google Gemini API key is required."))?;

        let endpoint = request
            .endpoint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&request.provider.endpoint);

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            endpoint.trim_end_matches('/'),
            model,
            key
        );

        let body = serde_json::json!({
            "contents": [
                {
                    "parts": [
                        {
                            "text":
                                "Reply with exactly: Luma connection successful."
                        }
                    ]
                }
            ]
        });

        let response = Client::new().post(url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();

            let body = response.text().await.unwrap_or_default();

            return Err(anyhow!("Gemini returned HTTP {status}\n\n{body}"));
        }

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn models_url(endpoint: &str) -> String {
    format!("{}/models", endpoint.trim_end_matches('/'))
}

fn chat_completions_url(endpoint: &str) -> String {
    format!("{}/chat/completions", endpoint.trim_end_matches('/'))
}
