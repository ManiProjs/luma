use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

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

    Plan { content: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum PlannerResponse {
    #[serde(rename = "tool")]
    Tool { name: String, input: Value },

    #[serde(rename = "multi")]
    Multi { actions: Vec<PlannerResponse> },

    #[serde(rename = "answer")]
    Answer { content: String },

    #[serde(rename = "plan")]
    Plan { content: String },
}

#[async_trait::async_trait]
pub trait PlannerTrait: Send + Sync {
    async fn plan(&self, messages: Vec<Message>, cancel: CancellationToken) -> Result<PlanAction>;
}

pub struct Planner<M> {
    model: M,
    tools: Vec<String>,
}

#[async_trait::async_trait]
impl<M> PlannerTrait for Planner<M>
where
    M: Model,
{
    async fn plan(&self, messages: Vec<Message>, _cancel: CancellationToken) -> Result<PlanAction> {
        self.create_plan(messages).await
    }
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

    fn system_prompt(&self) -> String {
        format!(
            r#"
You are Luma's Planner.

You are the decision-making system of a local-first AI coding agent.

Your job is to decide the NEXT action required to accomplish the user's
request.

You MUST return valid JSON only.

==================================================
AVAILABLE TOOLS
==================================================

{}

==================================================
ACTION FORMAT
==================================================

Tool action:

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
    }},
    {{
      "type": "tool",
      "name": "read_file",
      "input": "Cargo.toml"
    }}
  ]
}}

Concise implementation plan:

{{
  "type": "plan",
  "content": "1. Read the relevant files. 2. Apply the change. 3. Verify."
}}

Use a plan only after exploring enough to know what to do.

Final answer:

{{
  "type": "answer",
  "content": "The answer..."
}}

==================================================
WORKSPACE RULES
==================================================

The filesystem is unknown unless a tool has observed it.

Never invent:

- files
- folders
- symbols
- project structure
- configuration
- dependencies
- command output
- source code
- test results

If information is missing, inspect it.

Use previous tool observations.

Do not repeat inspections that have already provided the
information you need.

==================================================
MODIFICATION RULES
==================================================

For an existing file:

1. Read it first.
2. Understand the relevant code.
3. Modify it.
4. Verify the modification.

Never invent the old contents of a file.

Use patch_file for precise modifications.

Use write_file primarily for new files or complete replacements.

==================================================
TOOL SELECTION
==================================================

list_directory
    Understand directory structure.

read_file
    Read actual file contents.

search_files
    Locate files, symbols, or text.

patch_file
    Make precise changes to existing files.

write_file
    Create a new file or replace a complete file.

run_command
    Build, test, format, lint, or otherwise verify the project.

==================================================
EFFICIENCY
==================================================

Choose the smallest number of actions necessary.

Do not repeatedly list the same directory.

Do not reread unchanged files without a reason.

Do not use write_file when patch_file is sufficient.

Do not run commands that cannot contribute to the task.

==================================================
FINAL RULE
==================================================

When information is missing:
inspect.

When a tool can answer the question:
use the tool.

When modifying code:

inspect
→ modify
→ verify

Never guess when the workspace can provide the answer.

Return JSON only.
"#,
            self.tools.join("\n")
        )
    }

    pub async fn create_plan(&self, messages: Vec<Message>) -> Result<PlanAction> {
        let mut request_messages = Vec::with_capacity(messages.len() + 1);

        request_messages.push(Message {
            role: MessageRole::System,
            content: self.system_prompt(),
        });

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

        let planner_response: PlannerResponse = serde_json::from_value(json)
            .map_err(|error| anyhow!("Invalid planner response schema: {}", error))?;

        self.validate_response(&planner_response)?;

        Ok(convert_response(planner_response))
    }

    fn validate_response(&self, response: &PlannerResponse) -> Result<()> {
        match response {
            PlannerResponse::Tool { name, .. } => {
                if name.trim().is_empty() {
                    return Err(anyhow!("Planner returned an empty tool name"));
                }

                if !self.tool_exists(name) {
                    return Err(anyhow!("Planner requested unknown tool: {}", name));
                }

                Ok(())
            }

            PlannerResponse::Multi { actions } => {
                if actions.is_empty() {
                    return Err(anyhow!("Planner returned an empty multi action"));
                }

                for action in actions {
                    self.validate_response(action)?;
                }

                Ok(())
            }

            PlannerResponse::Answer { content } => {
                if content.trim().is_empty() {
                    return Err(anyhow!("Planner returned an empty answer"));
                }

                Ok(())
            }

            PlannerResponse::Plan { content } => {
                if content.trim().is_empty() {
                    return Err(anyhow!("Planner returned an empty plan"));
                }

                Ok(())
            }
        }
    }

    fn tool_exists(&self, name: &str) -> bool {
        self.tools.iter().any(|description| {
            tool_name_from_description(description).is_some_and(|tool| tool == name)
        })
    }
}

fn convert_response(response: PlannerResponse) -> PlanAction {
    match response {
        PlannerResponse::Tool { name, input } => PlanAction::Tool {
            name,
            input: serialize_input(input),
        },

        PlannerResponse::Multi { actions } => PlanAction::Multi {
            actions: actions.into_iter().map(convert_response).collect(),
        },

        PlannerResponse::Answer { content } => PlanAction::Answer { content },

        PlannerResponse::Plan { content } => PlanAction::Plan { content },
    }
}

fn serialize_input(input: Value) -> String {
    match input {
        Value::String(value) => value,

        value => value.to_string(),
    }
}

fn tool_name_from_description(description: &str) -> Option<&str> {
    description
        .split_whitespace()
        .next()
        .filter(|name| !name.is_empty())
}

fn extract_json(text: &str) -> Result<Value> {
    let clean = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if clean.is_empty() {
        return Err(anyhow!("Planner returned an empty response"));
    }

    if let Ok(value) = serde_json::from_str::<Value>(&clean) {
        return Ok(value);
    }

    let start = clean.find('{');
    let end = clean.rfind('}');

    if let (Some(start), Some(end)) = (start, end) {
        if start <= end {
            if let Ok(value) = serde_json::from_str::<Value>(&clean[start..=end]) {
                return Ok(value);
            }
        }
    }

    Err(anyhow!("Planner returned invalid JSON:\n{}", clean))
}
