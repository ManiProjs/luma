use anyhow::{Result, anyhow};
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::{
    config::ModelConfig,
    context::MessageRole,
    model::{CompletionRequest, Model, Usage},
};

pub struct GeminiModel {
    config: ModelConfig,
    usage: Arc<Mutex<Option<Usage>>>,
}

impl GeminiModel {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            usage: Arc::new(Mutex::new(None)),
        }
    }
}

fn parse_usage_event(event: &str) -> Option<Usage> {
    let data = event
        .lines()
        .find_map(|line| line.trim().strip_prefix("data:"))?
        .trim();
    let value: Value = serde_json::from_str(data).ok()?;
    let usage = &value["usageMetadata"];
    let prompt_tokens = usage["promptTokenCount"].as_u64().unwrap_or(0);
    let completion_tokens = usage["candidatesTokenCount"].as_u64().unwrap_or(0);
    let total_tokens = usage["totalTokenCount"]
        .as_u64()
        .unwrap_or(prompt_tokens + completion_tokens);
    (prompt_tokens > 0 || completion_tokens > 0 || total_tokens > 0).then_some(Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
    })
}

#[async_trait::async_trait]
impl Model for GeminiModel {
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        if let Ok(mut usage) = self.usage.lock() {
            *usage = None;
        }
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
            "{}/models/{}:streamGenerateContent?key={}&alt=sse",
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

        let usage_tracker = self.usage.clone();
        let stream = response
            .bytes_stream()
            .scan(String::new(), move |buffer, chunk| {
                let text = match chunk {
                    Ok(chunk) => String::from_utf8_lossy(&chunk).to_string(),
                    Err(error) => return futures_util::future::ready(Some(Err(error.into()))),
                };
                buffer.push_str(&text);
                while let Some(position) = buffer.find("\n\n") {
                    let event = buffer[..position].to_owned();
                    buffer.drain(..position + 2);
                    if let Some(usage) = parse_usage_event(&event)
                        && let Ok(mut guard) = usage_tracker.lock()
                    {
                        *guard = Some(usage);
                    }
                }
                futures_util::future::ready(Some(Ok(text)))
            });

        Ok(Box::pin(stream))
    }

    fn usage(&self) -> Option<Usage> {
        self.usage.lock().ok().and_then(|usage| usage.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_usage_event;

    #[test]
    fn parses_gemini_usage_metadata() {
        let usage = parse_usage_event(
            "data: {\"usageMetadata\":{\"promptTokenCount\":80,\"candidatesTokenCount\":25,\"totalTokenCount\":105}}",
        )
        .unwrap();
        assert_eq!(usage.prompt_tokens, 80);
        assert_eq!(usage.completion_tokens, 25);
        assert_eq!(usage.total_tokens, 105);
    }
}
