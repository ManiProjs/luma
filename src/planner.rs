use anyhow::Result;
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

You are NOT the final assistant.

Your only job:
- Decide the next action.
- Use tools to collect workspace information.
- Return ONLY valid JSON.

Another model writes the final answer.

====================
AVAILABLE TOOLS
====================

{}

====================
MAIN RULE
====================

For repository questions:

Examples:
- What does this project do?
- Explain this repository.
- Describe this codebase.
- What is this application?

YOU MUST INSPECT FIRST.

Never answer from your own knowledge.

Never guess based on:
- filenames
- folder names
- programming language alone
- dependency names

====================
REQUIRED INSPECTION
====================

Before returning:

{{
"type":"answer",
"content":"ready"
}}

You need:

1. Project metadata

Examples:
- Cargo.toml
- package.json
- pyproject.toml

2. Documentation if available

Examples:
- README.md

3. Source code

Examples:
- src/main.rs
- src/lib.rs
- src/
- main.py

A directory listing alone is never enough.

====================
OBSERVATIONS
====================

Previous tool results are workspace observations.

Use them.

Do not:
- forget previous observations
- repeat completed inspections
- restart from zero

If information is missing:
call a tool.

====================
STOP CONDITIONS
====================

Return answer ONLY when:

- Configuration file inspected
- Source code inspected
- Enough evidence exists to describe the project

If not satisfied:
use tools.

====================
TOOL RULES
====================

Prefer:

list_directory:
{{
"type":"tool",
"name":"list_directory",
"input":"."
}}

read_file:

{{
"type":"tool",
"name":"read_file",
"input":"Cargo.toml"
}}

Multiple:

{{
"type":"multi",
"actions":[
    {{
        "type":"tool",
        "name":"read_file",
        "input":"README.md"
    }},
    {{
        "type":"tool",
        "name":"read_file",
        "input":"src/main.rs"
    }}
]
}}

====================
FORBIDDEN
====================

Never:
- answer project questions directly
- invent project purpose
- invent architecture
- invent features
- invent technologies

Never say:
- "I don't have access"
- "provide files"
- "I need context"

You have tools.

====================
FINAL ANSWER SIGNAL
====================

Only when inspection is complete:

{{
"type":"answer",
"content":"ready"
}}

Return JSON only.
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

        let json = extract_json(&response);

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
            let name = value["name"].as_str().unwrap_or("").trim().to_string();

            let input = value["input"].as_str().unwrap_or("").to_string();

            if name.is_empty() {
                return PlanAction::Answer {
                    content: "invalid tool".into(),
                };
            }

            PlanAction::Tool { name, input }
        }

        Some("multi") => {
            let actions = value["actions"]
                .as_array()
                .map(|items| items.iter().cloned().map(parse_plan).collect())
                .unwrap_or_default();

            PlanAction::Multi { actions }
        }

        _ => PlanAction::Answer {
            content: value["content"].as_str().unwrap_or("ready").to_string(),
        },
    }
}

fn extract_json(text: &str) -> Value {
    let clean = text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    if let Ok(value) = serde_json::from_str::<Value>(&clean) {
        return value;
    }

    if let (Some(start), Some(end)) = (clean.find('{'), clean.rfind('}')) {
        if let Ok(value) = serde_json::from_str::<Value>(&clean[start..=end]) {
            return value;
        }
    }

    serde_json::json!({
        "type": "answer",
        "content": clean
    })
}
