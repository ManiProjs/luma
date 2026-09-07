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
    Answer,
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
    Answer,

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
    descriptions: Vec<String>,
}

#[async_trait::async_trait]
impl<M> PlannerTrait for Planner<M>
where
    M: Model,
{
    async fn plan(&self, messages: Vec<Message>, cancel: CancellationToken) -> Result<PlanAction> {
        self.create_plan(messages, cancel).await
    }
}

impl<M> Planner<M>
where
    M: Model,
{
    pub fn new(model: M, tools: &ToolRegistry) -> Self {
        Self {
            model,
            tools: tools.names(),
            descriptions: tools.descriptions(),
        }
    }

    fn system_prompt(&self) -> String {
        let tools = if self.descriptions.is_empty() {
            "No tools are currently available.".to_owned()
        } else {
            self.descriptions.join("\n")
        };

        format!(
            r#"You are Luma's internal planner.

Your job is to decide the NEXT action required to satisfy the user's request.

Return EXACTLY ONE valid JSON object.
Never use Markdown.
Never explain your reasoning.
Never output conversational text.

AVAILABLE TOOLS:
{tools}

EXACT TOOL NAMES:
{tool_names}

A tool action:

{{
  "type": "tool",
  "name": "tool_name",
  "input": "tool input"
}}

Structured tool input:

{{
  "type": "tool",
  "name": "write_file",
  "input": {{
    "path": "file.txt",
    "content": "hello"
  }}
}}

Multiple independent tool actions:

{{
  "type": "multi",
  "actions": [
    {{
      "type": "tool",
      "name": "read_file",
      "input": "Cargo.toml"
    }},
    {{
      "type": "tool",
      "name": "read_file",
      "input": "src/main.rs"
    }}
  ]
}}

Only tool actions may appear inside "multi".
Nested multi actions are forbidden.

A plan:

{{
  "type": "plan",
  "content": "1. Inspect the relevant files. 2. Modify them. 3. Verify."
}}

Use "plan" only when enough information has already been gathered to propose a concrete implementation plan requiring approval.

An answer:

{{
  "type": "answer"
}}

Use "answer" when no workspace action is necessary.

WORKSPACE RULES:

- Never invent files, directories, symbols, dependencies, source code, command output, or test results.
- If information is missing, inspect the workspace.
- Use previous observations.
- Do not repeat an inspection unnecessarily.
- Existing files must be read before modification.
- Prefer patch_file for existing files.
- Use write_file for new files or complete replacements.
- Verify modifications with run_command when appropriate.

DECISION RULES:

User asks a normal conversational question:
→ answer

Required workspace information is missing:
→ inspect

Existing file must be changed and has not been inspected:
→ read_file

Existing file has already been inspected and must be changed:
→ patch_file

New file is required:
→ write_file

Several independent inspections are required:
→ multi

Implementation needs explicit approval:
→ plan

After an approved plan:
→ perform the required tool actions

EFFICIENCY:

Choose the smallest useful action.
Do not repeat observations.
Do not perform unnecessary commands.
Prefer independent reads in multi.

VALID TOP-LEVEL TYPES:

tool
multi
answer
plan

Return JSON only."#,
            tools = tools,
            tool_names = self.tools.join(", "),
        )
    }

    pub async fn create_plan(
        &self,
        messages: Vec<Message>,
        cancel: CancellationToken,
    ) -> Result<PlanAction> {
        let mut request_messages = Vec::with_capacity(messages.len() + 1);

        // The planner owns its system prompt.
        // Agent must NOT add another planner system prompt.
        request_messages.push(Message {
            role: MessageRole::System,
            content: self.system_prompt(),
        });

        request_messages.extend(messages);

        tracing::debug!(
            messages = request_messages.len(),
            "Planner requesting next action"
        );

        let mut stream = match self
            .model
            .stream(CompletionRequest {
                messages: request_messages,
            })
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                tracing::error!(
                    error = %error,
                    "Planner request failed"
                );

                return Err(error);
            }
        };

        let mut response = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(anyhow!("Planning interrupted."));
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    response.push_str(&chunk?);
                }
            }
        }

        tracing::debug!(response_bytes = response.len(), "Planner response received");

        let json = extract_json(&response)?;

        let planner_response: PlannerResponse =
            serde_json::from_value(json.clone()).map_err(|error| {
                anyhow!(
                    "Invalid planner response schema: {}\n\nPlanner JSON:\n{}",
                    error,
                    serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string())
                )
            })?;

        self.validate_response(&planner_response)?;

        Ok(convert_response(planner_response))
    }

    fn validate_response(&self, response: &PlannerResponse) -> Result<()> {
        match response {
            PlannerResponse::Tool { name, input } => {
                if name.trim().is_empty() {
                    return Err(anyhow!("Planner returned an empty tool name"));
                }

                if !self.tool_exists(name) {
                    return Err(anyhow!(
                        "Planner requested unknown tool: '{}'. Available tools: {}",
                        name,
                        self.tools.join(", ")
                    ));
                }

                if input.is_null() {
                    return Err(anyhow!("Planner returned null input for tool '{}'", name));
                }

                Ok(())
            }

            PlannerResponse::Multi { actions } => {
                if actions.is_empty() {
                    return Err(anyhow!("Planner returned an empty multi action"));
                }

                for action in actions {
                    match action {
                        PlannerResponse::Tool { .. } => {
                            self.validate_response(action)?;
                        }

                        PlannerResponse::Multi { .. } => {
                            return Err(anyhow!("Nested multi actions are not allowed"));
                        }

                        PlannerResponse::Answer => {
                            return Err(anyhow!("Answer actions are not allowed inside multi"));
                        }

                        PlannerResponse::Plan { .. } => {
                            return Err(anyhow!("Plan actions are not allowed inside multi"));
                        }
                    }
                }

                Ok(())
            }

            PlannerResponse::Answer => Ok(()),

            PlannerResponse::Plan { content } => {
                if content.trim().is_empty() {
                    return Err(anyhow!("Planner returned an empty plan"));
                }

                Ok(())
            }
        }
    }

    fn tool_exists(&self, name: &str) -> bool {
        self.tools.iter().any(|tool| tool == name)
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

        PlannerResponse::Answer => PlanAction::Answer,

        PlannerResponse::Plan { content } => PlanAction::Plan { content },
    }
}

fn serialize_input(input: Value) -> String {
    match input {
        Value::String(value) => value,
        value => value.to_string(),
    }
}

fn extract_json(text: &str) -> Result<Value> {
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if clean.is_empty() {
        return Err(anyhow!("Planner returned an empty response"));
    }

    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        return Ok(value);
    }

    let Some(start) = clean.find('{') else {
        return Err(anyhow!("Planner returned invalid JSON:\n{}", clean));
    };

    let Some(end) = clean.rfind('}') else {
        return Err(anyhow!("Planner returned invalid JSON:\n{}", clean));
    };

    if start > end {
        return Err(anyhow!("Planner returned invalid JSON:\n{}", clean));
    }

    let candidate = &clean[start..=end];

    serde_json::from_str::<Value>(candidate).map_err(|error| {
        anyhow!(
            "Planner returned invalid JSON: {}\n\nResponse:\n{}",
            error,
            clean
        )
    })
}
