use anyhow::{Result, anyhow};
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::json;

use crate::{
    config::ModelConfig,
    context::MessageRole,
    model::{CompletionRequest, Model},
};

pub struct GeminiModel {
    config: ModelConfig,
}

impl GeminiModel {
    pub fn new(config: ModelConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl Model for GeminiModel {
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        let client = reqwest::Client::new();

        let mut contents = Vec::new();

        for message in request.messages {
            let role = match message.role {
                MessageRole::User => "user",

                MessageRole::Assistant => "model",

                _ => continue,
            };

            contents.push(json!({
                "role": role,
                "parts": [
                    {
                        "text": message.content
                    }
                ]
            }));
        }

        let url = format!(
            "{}/models/{}:streamGenerateContent?key={}",
            self.config.endpoint,
            self.config.name,
            self.config
                .api_key
                .clone()
                .ok_or_else(|| anyhow!("Missing Gemini API key"))?
        );

        let body = json!({
            "contents": contents
        });

        let response = client.post(url).json(&body).send().await?;

        let stream = response.bytes_stream().map(|chunk| {
            let chunk = chunk?;

            Ok(String::from_utf8_lossy(&chunk).to_string())
        });

        Ok(Box::pin(stream))
    }
}
