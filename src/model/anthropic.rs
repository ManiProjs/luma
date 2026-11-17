use anyhow::{Result, anyhow};
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

use crate::{
    config::ModelConfig,
    context::MessageRole,
    model::{CompletionRequest, Model, Usage},
};

pub struct AnthropicModel {
    config: ModelConfig,
    usage: Arc<Mutex<Option<Usage>>>,
}

impl AnthropicModel {
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
    let kind = value["type"].as_str()?;
    let usage = match kind {
        "message_start" => &value["message"]["usage"],
        "message_delta" => &value["usage"],
        _ => return None,
    };
    let prompt_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
    let completion_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
    Some(Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
    })
}

#[async_trait::async_trait]
impl Model for AnthropicModel {
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        if let Ok(mut usage) = self.usage.lock() {
            *usage = None;
        }
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
                    if let Some(update) = parse_usage_event(&event)
                        && let Ok(mut guard) = usage_tracker.lock()
                    {
                        let mut usage = guard.clone().unwrap_or_default();
                        if update.prompt_tokens > 0 {
                            usage.prompt_tokens = update.prompt_tokens;
                        }
                        if update.completion_tokens > 0 {
                            usage.completion_tokens = update.completion_tokens;
                        }
                        usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
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
    fn parses_anthropic_input_and_output_usage_events() {
        let start = parse_usage_event(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":120}}}",
        )
        .unwrap();
        let delta = parse_usage_event(
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":45}}",
        )
        .unwrap();
        assert_eq!(start.prompt_tokens, 120);
        assert_eq!(delta.completion_tokens, 45);
    }
}
