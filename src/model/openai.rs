use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tracing::{debug, error, info};

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
        let endpoint = normalize_endpoint(&endpoint.into());

        Self {
            client: Client::new(),
            endpoint,
            model: model.into(),
            api_key,
        }
    }
}

fn normalize_endpoint(endpoint: &str) -> String {
    let mut endpoint = endpoint.trim().trim_end_matches('/').to_string();

    if endpoint.ends_with("/chat/completions") {
        return endpoint;
    }

    if endpoint.ends_with("/models") {
        endpoint.truncate(endpoint.len() - "/models".len());
        endpoint = endpoint.trim_end_matches('/').to_string();
    }

    format!("{endpoint}/chat/completions")
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

fn parse_sse_event(event: &str) -> Option<String> {
    for line in event.lines() {
        let line = line.trim();

        if !line.starts_with("data:") {
            continue;
        }

        let data = line.trim_start_matches("data:").trim();

        if data.is_empty() || data == "[DONE]" {
            continue;
        }

        let json: Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(_) => continue,
        };

        // Standard OpenAI-compatible response:
        //
        // {
        //   "choices": [
        //     {
        //       "delta": {
        //         "content": "Hello"
        //       }
        //     }
        //   ]
        // }
        if let Some(text) = json["choices"]
            .get(0)
            .and_then(|choice| choice["delta"]["content"].as_str())
        {
            return Some(text.to_string());
        }

        // Some providers may return the complete message
        // in a streaming response.
        if let Some(text) = json["choices"]
            .get(0)
            .and_then(|choice| choice["message"]["content"].as_str())
        {
            return Some(text.to_string());
        }

        // Ollama's OpenAI-compatible streaming format.
        if let Some(text) = json["message"]["content"].as_str() {
            return Some(text.to_string());
        }

        // Some compatible APIs use:
        //
        // {
        //   "response": "Hello"
        // }
        if let Some(text) = json["response"].as_str() {
            return Some(text.to_string());
        }
    }

    None
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

                    // Observations are fed back to the model as user
                    // messages so the model can reason over tool results.
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

        let mut request = self.client.post(&self.endpoint).json(&body);

        if let Some(key) = &self.api_key {
            if !key.trim().is_empty() {
                request = request.bearer_auth(key);
            }
        }

        let response = request
            .send()
            .await
            .map_err(|error| anyhow!("Failed to connect to model provider: {error}"))?;

        let status = response.status();

        debug!(
            status = %status,
            "Model provider responded"
        );

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read response>".to_string());

            error!(
                status = %status,
                body = %body,
                "Model provider returned an error"
            );

            return Err(anyhow!("Model provider returned HTTP {status}\n\n{body}"));
        }

        info!(
            status = %status,
            model = %self.model,
            "Model request successful"
        );

        let byte_stream = response.bytes_stream();

        // SSE events are separated by a blank line.
        //
        // Network chunks are NOT guaranteed to line up with SSE events,
        // so we keep a buffer between chunks.
        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    match byte_stream.next().await {
                        Some(Ok(bytes)) => {
                            buffer.push_str(&String::from_utf8_lossy(&bytes));

                            let mut output = String::new();

                            // Process complete SSE events.
                            while let Some(position) = buffer.find("\n\n") {
                                let event = buffer[..position].to_string();

                                buffer.drain(..position + 2);

                                if let Some(text) = parse_sse_event(&event) {
                                    output.push_str(&text);
                                }
                            }

                            if !output.is_empty() {
                                return Some((Ok(output), (byte_stream, buffer)));
                            }

                            // We received only a partial event.
                            // Keep buffering.
                        }

                        Some(Err(error)) => {
                            return Some((Err(error.into()), (byte_stream, buffer)));
                        }

                        None => {
                            // Process a final event that may not have
                            // ended with \n\n.
                            let output = parse_sse_event(&buffer).unwrap_or_default();

                            if !output.is_empty() {
                                buffer.clear();

                                return Some((Ok(output), (byte_stream, buffer)));
                            }

                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}
