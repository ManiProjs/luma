pub mod anthropic;
pub mod gemini;
pub mod openai;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::BoxStream;

use crate::{config::ModelConfig, context::Message};

pub use openai::OpenAICompatibleModel;

pub type ModelStream = BoxStream<'static, Result<String>>;

pub struct CompletionRequest {
    pub messages: Vec<Message>,
}

#[async_trait]
pub trait Model: Send + Sync {
    async fn stream(&self, request: CompletionRequest) -> Result<ModelStream>;
}

pub fn create_model(config: &ModelConfig) -> Box<dyn Model> {
    match config.provider.as_str() {
        "Anthropic" => Box::new(anthropic::AnthropicModel::new(config.clone())),

        "Google Gemini" => Box::new(gemini::GeminiModel::new(config.clone())),

        _ => Box::new(OpenAICompatibleModel::new(
            config.endpoint.clone(),
            config.name.clone(),
            config.api_key.clone(),
        )),
    }
}

#[async_trait]
impl<T> Model for Box<T>
where
    T: Model + ?Sized,
{
    async fn stream(&self, request: CompletionRequest) -> Result<ModelStream> {
        (**self).stream(request).await
    }
}
