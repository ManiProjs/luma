use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use serde_json::Value;

use crate::{
    context::{Message, MessageRole},
    model::{CompletionRequest, Model},
    tools::ToolRegistry,
};

#[derive(Debug, Clone)]
pub enum PlanAction {
    Tool { name: String, input: String },

    Answer { content: String },

    Multi { actions: Vec<PlanAction> },
}

#[async_trait::async_trait]
pub trait PlannerTrait: Send + Sync {
    async fn plan(&self, messages: Vec<Message>) -> Result<PlanAction>;
}

pub struct Planner<M> {
    model: M,
    tools: Vec<String>,
}

impl<M> Planner<M>
where
    M: Model,
{
    pub fn new(model: M, tools: &ToolRegistry) -> Self {
        Self {
            model,
            tools: tools.descriptions(),
        }
    }

    fn prompt(&self) -> String {
        format!(
            r#"
You are Luma Planner.

Your job is to decide the next action.

You MUST return JSON only.

AVAILABLE TOOLS:

{}

ACTION FORMAT:

Tool:

{{
  "type": "tool",
  "name": "tool_name",
  "input": "tool input"
}}

For tools requiring structured input:

{{
  "type": "tool",
  "name": "write_file",
  "input": {{
    "path": "file.txt",
    "content": "hello"
  }}
}}

Multiple actions:

{{
  "type": "multi",
  "actions": [
    {{
      "type": "tool",
      "name": "read_file",
      "input": "README.md"
    }}
  ]
}}

Final answer:

{{
  "type": "answer",
  "content": "ready"
}}

RULES:

- Never invent project information.
- Inspect files before explaining projects.
- Use tools when information is missing.
- Use previous tool results.
- Do not repeat completed inspections.
- Return JSON only.

"#,
            self.tools.join("\n")
        )
    }

    pub async fn create_plan(&self, messages: Vec<Message>) -> Result<PlanAction> {
        let mut request_messages = vec![Message {
            role: MessageRole::System,
            content: self.prompt(),
        }];

        request_messages.extend(messages);

        let request = CompletionRequest {
            messages: request_messages,
        };

        let mut stream = self.model.stream(request).await?;

        let mut response = String::new();

        while let Some(chunk) = stream.next().await {
            response.push_str(&chunk?);
        }

        let json = extract_json(&response)?;

        Ok(parse_plan(json))
    }
}

#[async_trait::async_trait]
impl<M> PlannerTrait for Planner<M>
where
    M: Model + Send + Sync,
{
    async fn plan(&self, messages: Vec<Message>) -> Result<PlanAction> {
        self.create_plan(messages).await
    }
}

fn parse_plan(value: Value) -> PlanAction {
    match value["type"].as_str() {
        Some("tool") => {
            let name = value["name"].as_str().unwrap_or("").to_string();

            if name.is_empty() {
                return PlanAction::Answer {
                    content: "Invalid tool call".into(),
                };
            }

            let input = value["input"].to_string();

            PlanAction::Tool { name, input }
        }

        Some("multi") => {
            let actions = value["actions"]
                .as_array()
                .map(|items| items.iter().cloned().map(parse_plan).collect())
                .unwrap_or_default();

            PlanAction::Multi { actions }
        }

        Some("answer") => PlanAction::Answer {
            content: value["content"].as_str().unwrap_or("ready").to_string(),
        },

        _ => PlanAction::Answer {
            content: "Invalid planner response".into(),
        },
    }
}

fn extract_json(text: &str) -> Result<Value> {
    let clean = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if let Ok(value) = serde_json::from_str::<Value>(&clean) {
        return Ok(value);
    }

    if let (Some(start), Some(end)) = (clean.find('{'), clean.rfind('}')) {
        if let Ok(value) = serde_json::from_str::<Value>(&clean[start..=end]) {
            return Ok(value);
        }
    }

    Err(anyhow!("Planner returned invalid JSON:\n{}", clean))
}
