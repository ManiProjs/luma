use anyhow::{Result, anyhow};
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::json;

use crate::{
    config::ModelConfig,
    context::MessageRole,
    model::{CompletionRequest, Model},
};

pub struct AnthropicModel {
    config: ModelConfig,
}

impl AnthropicModel {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl Model for AnthropicModel {
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        let client = reqwest::Client::new();

        let mut messages = Vec::new();

        let mut system = None;

        for message in request.messages {
            match message.role {
                MessageRole::System => {
                    system = Some(message.content);
                }

                MessageRole::User => {
                    messages.push(json!({
                        "role": "user",
                        "content": message.content
                    }));
                }

                MessageRole::Assistant => {
                    messages.push(json!({
                        "role": "assistant",
                        "content": message.content
                    }));
                }

                _ => {}
            }
        }

        let body = json!({
            "model": self.config.name,
            "max_tokens": 4096,
            "stream": true,
            "system": system.unwrap_or_default(),
            "messages": messages
        });

        let response = client
            .post(&self.config.endpoint)
            .header(
                "x-api-key",
                self.config
                    .api_key
                    .clone()
                    .ok_or_else(|| anyhow!("Missing Anthropic API key"))?,
            )
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        let stream = response.bytes_stream().map(|chunk| {
            let chunk = chunk?;

            let text = String::from_utf8_lossy(&chunk).to_string();

            Ok(text)
        });

        Ok(Box::pin(stream))
    }
}
