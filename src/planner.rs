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

    // Actual registered tool names.
    tools: Vec<String>,

    // Human-readable descriptions used in the prompt.
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

    // ========================================================================
    // System prompt
    // ========================================================================

    fn system_prompt(&self) -> String {
        let tools = if self.descriptions.is_empty() {
            "No tools are currently available.".to_owned()
        } else {
            self.descriptions.join("\n")
        };

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

{tools}

IMPORTANT:
The exact tool names are:

{tool_names}

Use these names exactly.

==================================================
ACTION FORMAT
==================================================

Tool action:

{{
  "type": "tool",
  "name": "tool_name",
  "input": "tool input"
}}

For tools requiring structured input, input MUST be a JSON object:

{{
  "type": "tool",
  "name": "write_file",
  "input": {{
    "path": "file.txt",
    "content": "hello"
  }}
}}

Multiple tool actions:

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

IMPORTANT:

"multi" may contain ONLY tool actions.

Do not put another "multi", "answer", or "plan" inside "multi".

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
DECISION PROCESS
==================================================

Think about the user's request and the observations already available.

If required information is missing:
inspect it.

If the task is purely conversational:
answer it.

If implementation requires multiple independent inspections:
use "multi".

If the implementation approach is known and should be presented
for approval:
return "plan".

After a plan is approved, continue with tool actions.

When modifying code:

inspect
→ modify
→ verify

Never guess when the workspace can provide the answer.

==================================================
FINAL RULE
==================================================

Return JSON only.
"#,
            tools = tools,
            tool_names = self.tools.join(", "),
        )
    }

    // ========================================================================
    // Planning
    // ========================================================================

    pub async fn create_plan(
        &self,
        messages: Vec<Message>,
        cancel: CancellationToken,
    ) -> Result<PlanAction> {
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

    // ========================================================================
    // Validation
    // ========================================================================

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

                        PlannerResponse::Answer { .. } => {
                            return Err(anyhow!("Answer actions are not allowed inside multi"));
                        }

                        PlannerResponse::Plan { .. } => {
                            return Err(anyhow!("Plan actions are not allowed inside multi"));
                        }
                    }
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
        self.tools.iter().any(|tool| tool == name)
    }
}

// ============================================================================
// Response conversion
// ============================================================================

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

// ============================================================================
// Input serialization
// ============================================================================

fn serialize_input(input: Value) -> String {
    match input {
        Value::String(value) => value,
        value => value.to_string(),
    }
}

// ============================================================================
// JSON extraction
// ============================================================================

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

    // Try the entire response first.
    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        return Ok(value);
    }

    // Fall back to the first JSON object.
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
