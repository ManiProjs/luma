use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::context::MessageRole;

pub type ModelStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

#[async_trait]
pub trait Model: Send + Sync {
    async fn stream(&self, request: CompletionRequest) -> Result<ModelStream>;
}

#[derive(Debug)]
pub struct CompletionRequest {
    pub messages: Vec<crate::context::Message>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ModelAction {
    #[serde(rename = "text")]
    Text { content: String },

    #[serde(rename = "tool_call")]
    ToolCall { name: String, input: String },
}

pub fn parse_json_response<T>(text: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let cleaned = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if let Ok(value) = serde_json::from_str(&cleaned) {
        return Ok(value);
    }

    if let (Some(start), Some(end)) = (cleaned.find('{'), cleaned.rfind('}')) {
        let json = &cleaned[start..=end];

        if let Ok(value) = serde_json::from_str(json) {
            return Ok(value);
        }
    }

    let fallback = serde_json::json!({
        "type": "text",
        "content": text
    });

    Ok(serde_json::from_value(fallback)?)
}

#[derive(Clone)]
pub struct OpenAICompatibleModel {
    client: Client,

    endpoint: String,

    model: String,
}

impl OpenAICompatibleModel {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.into(),
            model: model.into(),
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,

    messages: Vec<ApiMessage>,

    stream: bool,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,

    content: String,
}

fn parse_stream_chunk(chunk: &str) -> Vec<String> {
    let mut output = Vec::new();

    for line in chunk.lines() {
        let line = line.trim();

        if !line.starts_with("data:") {
            continue;
        }

        let data = line.trim_start_matches("data:").trim();

        if data == "[DONE]" {
            continue;
        }

        let json: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Ollama
        if let Some(text) = json["message"]["content"].as_str() {
            output.push(text.to_string());

            continue;
        }

        // OpenAI compatible
        if let Some(text) = json["choices"]
            .get(0)
            .and_then(|x| x["delta"]["content"].as_str())
        {
            output.push(text.to_string());
        }
    }

    output
}

#[async_trait]
impl Model for OpenAICompatibleModel {
    async fn stream(&self, request: CompletionRequest) -> Result<ModelStream> {
        let messages = request
            .messages
            .into_iter()
            .map(|message| ApiMessage {
                role: match message.role {
                    MessageRole::System => "system",

                    MessageRole::User => "user",

                    MessageRole::Assistant => "assistant",

                    MessageRole::Tool => "user",

                    // Important:
                    // Local models like Qwen/Llama understand workspace
                    // observations better as user messages.
                    MessageRole::Observation => "user",
                }
                .to_string(),

                content: message.content,
            })
            .collect();

        let body = ChatRequest {
            model: self.model.clone(),

            messages,

            stream: true,
        };

        let response = self.client.post(&self.endpoint).json(&body).send().await?;

        let stream = response
            .bytes_stream()
            .map(|chunk| {
                let bytes = chunk?;

                let raw = String::from_utf8_lossy(&bytes);

                Ok(parse_stream_chunk(&raw))
            })
            .filter_map(|result| async {
                match result {
                    Ok(chunks) => {
                        if chunks.is_empty() {
                            None
                        } else {
                            Some(Ok(chunks.join("")))
                        }
                    }

                    Err(err) => Some(Err(err)),
                }
            });

        Ok(Box::pin(stream))
    }
}
