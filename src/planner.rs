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
        let tools = self
            .tools
            .iter()
            .map(|tool| format!("- {}", tool))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"
You are Luma's planning engine.

You control a real coding agent.

Your ONLY job is to choose the next action.

You MUST output exactly one valid JSON object.
Do not output markdown.
Do not output explanations.
Do not output reasoning.

==================================================
AVAILABLE TOOLS
==================================================

{}

==================================================
TOOL DECISION RULES
==================================================

Use a tool whenever the user asks you to perform an operation
that requires access to the workspace.

Examples:

User: "read README.md"
→ use read_file

User: "show me the project files"
→ use list_directory

User: "find where Foo is defined"
→ use search_files

User: "modify src/main.rs"
→ use read_file first if the file has not already been read

User: "patch /tmp/hello.txt"
→ use read_file first if the file has not already been read

User: "create config.toml"
→ use write_file

User: "run cargo check"
→ use run_command

User: "replace X with Y in a file"
→ use read_file first if the current contents are unknown,
then use patch_file

==================================================
CRITICAL RULE
==================================================

You DO have access to the tools listed above.

NEVER say:

"I do not have access to tools."

NEVER answer a workspace operation with normal text.

If a tool can perform the requested operation,
SELECT THAT TOOL.

==================================================
READ BEFORE MODIFY
==================================================

Never modify an existing file unless its current contents
are known.

For an unknown existing file:

FIRST:
read_file

THEN:
patch_file

For a new file:

write_file may be used directly.

==================================================
ANSWER
==================================================

Use "answer" ONLY when no tool is required.

Examples:

User: "What is Rust?"
→ answer

User: "Explain ownership."
→ answer

User: "Patch /tmp/hello.txt"
→ NOT answer

==================================================
JSON FORMAT
==================================================

Tool:

{{
  "type": "tool",
  "name": "read_file",
  "input": "path/to/file"
}}

Structured tool input:

{{
  "type": "tool",
  "name": "patch_file",
  "input": {{
    "path": "path/to/file",
    "old": "exact existing text",
    "new": "replacement text"
  }}
}}

Answer:

{{
  "type": "answer",
  "content": "response"
}}

Multi:

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

==================================================
FINAL RULE
==================================================

Choose an action.

Do not describe what you would do.

Do not explain the action.

Return JSON only.
"#,
            tools
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

        parse_plan(json)
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

fn parse_plan(value: Value) -> Result<PlanAction> {
    let action_type = value["type"]
        .as_str()
        .ok_or_else(|| anyhow!("Planner response missing `type`"))?;

    match action_type {
        "tool" => {
            let name = value["name"]
                .as_str()
                .ok_or_else(|| anyhow!("Tool action missing `name`"))?
                .trim();

            if name.is_empty() {
                return Err(anyhow!("Planner returned an empty tool name"));
            }

            let input = match &value["input"] {
                Value::String(value) => value.clone(),

                Value::Object(_) | Value::Array(_) => value["input"].to_string(),

                Value::Null => String::new(),

                other => other.to_string(),
            };

            Ok(PlanAction::Tool {
                name: name.to_string(),
                input,
            })
        }

        "multi" => {
            let items = value["actions"]
                .as_array()
                .ok_or_else(|| anyhow!("Multi action missing `actions`"))?;

            if items.is_empty() {
                return Err(anyhow!("Planner returned an empty action list"));
            }

            let mut actions = Vec::with_capacity(items.len());

            for item in items {
                actions.push(parse_plan(item.clone())?);
            }

            Ok(PlanAction::Multi { actions })
        }

        "answer" => {
            let content = value["content"].as_str().unwrap_or("").to_string();

            Ok(PlanAction::Answer { content })
        }

        other => Err(anyhow!("Unknown planner action type: {}", other)),
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

    let start = clean
        .find('{')
        .ok_or_else(|| anyhow!("Planner returned no JSON object:\n{}", clean))?;

    let end = clean
        .rfind('}')
        .ok_or_else(|| anyhow!("Planner returned incomplete JSON:\n{}", clean))?;

    if start > end {
        return Err(anyhow!("Invalid planner JSON:\n{}", clean));
    }

    let json = &clean[start..=end];

    serde_json::from_str::<Value>(json)
        .map_err(|error| anyhow!("Planner returned invalid JSON: {}\n{}", error, clean))
}
