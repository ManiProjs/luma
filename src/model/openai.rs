use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

use crate::{
    context::MessageRole,
    model::{CompletionRequest, Model, ModelStream},
};

#[derive(Clone)]
pub struct OpenAICompatibleModel {
    client: Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAICompatibleModel {
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
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
            Ok(value) => value,
            Err(_) => continue,
        };

        // Ollama OpenAI-compatible endpoint.
        if let Some(text) = json["message"]["content"].as_str() {
            output.push(text.to_string());
            continue;
        }

        // Standard OpenAI-compatible SSE.
        if let Some(text) = json["choices"]
            .get(0)
            .and_then(|choice| choice["delta"]["content"].as_str())
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

        let request = self.client.post(&self.endpoint).json(&body);

        let request = if let Some(key) = &self.api_key {
            request.bearer_auth(key)
        } else {
            request
        };

        let response = request.send().await?;

        let stream = response
            .bytes_stream()
            .map(|chunk| {
                let bytes = chunk?;
                let raw = String::from_utf8_lossy(&bytes);

                Ok(parse_stream_chunk(&raw))
            })
            .filter_map(|result| async {
                match result {
                    Ok(chunks) if !chunks.is_empty() => Some(Ok(chunks.join(""))),
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                }
            });

        Ok(Box::pin(stream))
    }
}
