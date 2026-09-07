//! Deterministic test doubles for the agent loop.
//!
//! `ScriptedPlanner` returns a fixed sequence of `PlanAction`s — no model
//! call, no JSON parsing. `MockModel` returns scripted text for the
//! answer-generation path. Both work with the generic `Agent<M, P>`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures_util::stream::{self, BoxStream};
use tokio_util::sync::CancellationToken;

use crate::{
    context::Message,
    model::{CompletionRequest, Model},
    planner::{PlanAction, PlannerTrait},
};

/// Returns scripted `PlanAction`s in order, then `Answer` when exhausted.
pub struct ScriptedPlanner {
    actions: Arc<Mutex<VecDeque<PlanAction>>>,
}

impl ScriptedPlanner {
    pub fn new(actions: Vec<PlanAction>) -> Self {
        Self {
            actions: Arc::new(Mutex::new(VecDeque::from(actions))),
        }
    }
}

#[async_trait::async_trait]
impl PlannerTrait for ScriptedPlanner {
    async fn plan(
        &self,
        _messages: Vec<Message>,
        _cancel: CancellationToken,
    ) -> Result<PlanAction> {
        let next = self
            .actions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(PlanAction::Answer);
        Ok(next)
    }
}

/// Returns scripted text chunks for the answer-generation path.
#[derive(Clone)]
pub struct MockModel {
    responses: Arc<Mutex<VecDeque<String>>>,
}

impl MockModel {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
        }
    }
}

#[async_trait::async_trait]
impl Model for MockModel {
    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<BoxStream<'static, Result<String>>> {
        let text = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "done".to_string());
        Ok(Box::pin(stream::iter(vec![Ok(text)])))
    }
}
