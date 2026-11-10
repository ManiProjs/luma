//! Deterministic test doubles for the agent loop.
//!
//! `ScriptedReAct` returns a fixed sequence of `ReActAction`s — no model
//! call, no JSON parsing. `MockModel` returns scripted text for the
//! answer-generation path. Both work with the generic `Agent<M, P>`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures_util::stream::{self, BoxStream};

use crate::{
    context::Message,
    model::{CompletionRequest, Model},
};

/// Returns scripted model responses for ReAct steps and conversational replies.
#[derive(Clone)]
pub struct MockModel {
    responses: Arc<Mutex<VecDeque<String>>>,
    requests: Arc<Mutex<Vec<Vec<Message>>>>,
}

impl MockModel {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn requests(&self) -> Vec<Vec<Message>> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Model for MockModel {
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        self.requests.lock().unwrap().push(request.messages);
        let text = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "done".to_string());
        Ok(Box::pin(stream::iter(vec![Ok(text)])))
    }
}
