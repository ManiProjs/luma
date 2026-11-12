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
pub enum ReActAction {
    Tool { name: String, input: String },
    Answer { content: String },
    Plan { content: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ReActResponse {
    #[serde(rename = "tool")]
    Tool { name: String, input: Value },

    #[serde(rename = "answer")]
    Answer { content: String },

    #[serde(rename = "plan")]
    Plan { content: String },
}

#[async_trait::async_trait]
pub trait ReActTrait: Send + Sync {
    async fn next_action(
        &self,
        messages: Vec<Message>,
        cancel: CancellationToken,
    ) -> Result<ReActAction>;
}

pub struct ReAct<M> {
    model: M,
    tools: Vec<String>,
    tool_metadata: Vec<crate::tools::ToolMetadata>,
}

#[async_trait::async_trait]
impl<M> ReActTrait for ReAct<M>
where
    M: Model,
{
    async fn next_action(
        &self,
        messages: Vec<Message>,
        cancel: CancellationToken,
    ) -> Result<ReActAction> {
        self.next_step(messages, cancel).await
    }
}

impl<M> ReAct<M>
where
    M: Model,
{
    pub fn new(model: M, tools: &ToolRegistry) -> Self {
        Self {
            model,
            tools: tools.names(),
            tool_metadata: tools.metadata(),
        }
    }

    fn system_prompt(&self) -> String {
        let tools = if self.tool_metadata.is_empty() {
            "No tools are currently available.".to_owned()
        } else {
            self.tool_metadata
                .iter()
                .map(|tool| {
                    format!(
                        "- {}\n  Description: {}\n  Model guide: {}",
                        tool.name, tool.description, tool.guide
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            r#"You are Luma, a ReAct coding agent.

Work in a ReAct cycle: use the conversation and previous tool observations,
choose one useful action, then continue after observing its result. When the
task is complete, return the complete user-facing answer.

Return EXACTLY ONE valid JSON object for this step.
Never use Markdown.
Do not reveal private chain-of-thought.

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

A plan:

{{
  "type": "plan",
  "content": "1. Inspect the relevant files. 2. Modify them. 3. Verify."
}}

Use "plan" only when enough information has already been gathered to propose a concrete implementation plan requiring approval.

An answer:

{{
  "type": "answer",
  "content": "The final response for the user."
}}

Use "answer" when the task is complete. Include the final response in content.

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

Implementation needs explicit approval:
→ plan

If a required preference, decision, or missing detail cannot be safely inferred:
→ call ask_question with JSON input {{"prompt":"...","choices":["..."]}}.
Use choices only when a small set of clear options helps; otherwise omit choices.
Do not guess or ask questions that are not necessary to proceed.

After an approved plan:
→ perform the required tool actions

EFFICIENCY:

Choose the smallest useful action.
Do not repeat observations.
Do not perform unnecessary commands.
VALID TOP-LEVEL TYPES:

tool
answer
plan

Return JSON only."#,
            tools = tools,
            tool_names = self.tools.join(", "),
        )
    }

    pub async fn next_step(
        &self,
        messages: Vec<Message>,
        cancel: CancellationToken,
    ) -> Result<ReActAction> {
        let mut request_messages = Vec::with_capacity(messages.len() + 1);

        // ReAct owns its system prompt; the agent supplies the live transcript.
        request_messages.push(Message {
            role: MessageRole::System,
            content: self.system_prompt(),
        });

        request_messages.extend(messages);

        tracing::debug!(messages = request_messages.len(), "ReAct step started");

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
                    "ReAct step failed"
                );

                return Err(error);
            }
        };

        let mut response = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(anyhow!("ReAct step interrupted."));
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    response.push_str(&chunk?);
                }
            }
        }

        tracing::debug!(response_bytes = response.len(), "ReAct response received");

        let json = extract_json(&response)?;

        let react_response: ReActResponse =
            serde_json::from_value(json.clone()).map_err(|error| {
                anyhow!(
                    "Invalid ReAct response schema: {}\n\nResponse JSON:\n{}",
                    error,
                    serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string())
                )
            })?;

        self.validate_response(&react_response)?;

        Ok(convert_response(react_response))
    }

    fn validate_response(&self, response: &ReActResponse) -> Result<()> {
        match response {
            ReActResponse::Tool { name, input } => {
                if name.trim().is_empty() {
                    return Err(anyhow!("ReAct returned an empty tool name"));
                }

                if !self.tool_exists(name) {
                    return Err(anyhow!(
                        "ReAct requested unknown tool: '{}'. Available tools: {}",
                        name,
                        self.tools.join(", ")
                    ));
                }

                if input.is_null() {
                    return Err(anyhow!("ReAct returned null input for tool '{}'", name));
                }

                Ok(())
            }

            ReActResponse::Answer { content } => {
                if content.trim().is_empty() {
                    return Err(anyhow!("ReAct returned an empty final answer"));
                }
                Ok(())
            }

            ReActResponse::Plan { content } => {
                if content.trim().is_empty() {
                    return Err(anyhow!("ReAct returned an empty plan"));
                }

                Ok(())
            }
        }
    }

    fn tool_exists(&self, name: &str) -> bool {
        self.tools.iter().any(|tool| tool == name)
    }
}

pub fn convert_response(response: ReActResponse) -> ReActAction {
    match response {
        ReActResponse::Tool { name, input } => ReActAction::Tool {
            name,
            input: serialize_input(input),
        },

        ReActResponse::Answer { content } => ReActAction::Answer { content },

        ReActResponse::Plan { content } => ReActAction::Plan { content },
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
        return Err(anyhow!("ReAct returned an empty response"));
    }

    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        return Ok(value);
    }

    let Some(start) = clean.find('{') else {
        return Err(anyhow!("ReAct returned invalid JSON:\n{}", clean));
    };

    let Some(end) = clean.rfind('}') else {
        return Err(anyhow!("ReAct returned invalid JSON:\n{}", clean));
    };

    if start > end {
        return Err(anyhow!("ReAct returned invalid JSON:\n{}", clean));
    }

    let candidate = &clean[start..=end];

    serde_json::from_str::<Value>(candidate).map_err(|error| {
        anyhow!(
            "ReAct returned invalid JSON: {}\n\nResponse:\n{}",
            error,
            clean
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelStream;

    // ── extract_json ────────────────────────────────────────────────────────

    #[test]
    fn extracts_clean_tool_json() {
        let value =
            extract_json(r#"{"type":"tool","name":"read_file","input":"src/main.rs"}"#).unwrap();
        assert_eq!(value["type"], "tool");
        assert_eq!(value["name"], "read_file");
    }

    #[test]
    fn extracts_answer_json() {
        let value = extract_json(r#"{"type":"answer","content":"done"}"#).unwrap();
        assert_eq!(value["type"], "answer");
    }

    #[test]
    fn strips_markdown_code_fence() {
        let value =
            extract_json("```json\n{\"type\":\"answer\",\"content\":\"done\"}\n```").unwrap();
        assert_eq!(value["type"], "answer");
    }

    #[test]
    fn extracts_json_surrounded_by_text() {
        let value = extract_json(
            "Here is my decision:\n{\"type\":\"tool\",\"name\":\"read_file\",\"input\":\"a.rs\"}\nDone.",
        )
        .unwrap();
        assert_eq!(value["name"], "read_file");
    }

    #[test]
    fn rejects_empty_response() {
        assert!(extract_json("   ").is_err());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(extract_json("{not valid json at all").is_err());
    }

    #[test]
    fn rejects_text_without_json() {
        assert!(extract_json("I have no idea what to do").is_err());
    }

    // ── validate_response ───────────────────────────────────────────────────

    fn react_with_tools(names: &[&str]) -> ReAct<MockModel> {
        let mut registry = ToolRegistry::new();
        for name in names {
            registry.register(DummyTool {
                name: name.to_string(),
            });
        }
        ReAct::new(MockModel, &registry)
    }

    struct MockModel;

    #[async_trait::async_trait]
    impl Model for MockModel {
        async fn stream(&self, _request: CompletionRequest) -> Result<ModelStream> {
            unimplemented!("not needed for validation tests")
        }
    }

    struct DummyTool {
        name: String,
    }

    impl crate::tools::Tool for DummyTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "dummy"
        }
        fn execute(&self, _input: &str) -> Result<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn validates_known_tool() {
        let react = react_with_tools(&["read_file", "write_file"]);
        let response = ReActResponse::Tool {
            name: "read_file".into(),
            input: Value::String("src/main.rs".into()),
        };
        assert!(react.validate_response(&response).is_ok());
    }

    #[test]
    fn rejects_unknown_tool() {
        let react = react_with_tools(&["read_file"]);
        let response = ReActResponse::Tool {
            name: "delete_everything".into(),
            input: Value::String(".".into()),
        };
        let error = react.validate_response(&response).unwrap_err();
        assert!(error.to_string().contains("unknown tool"));
    }

    #[test]
    fn system_prompt_includes_name_description_and_model_guide() {
        let react = react_with_tools(&["read_file"]);
        let prompt = react.system_prompt();
        assert!(prompt.contains("- read_file"));
        assert!(prompt.contains("Description:"));
        assert!(prompt.contains("Model guide:"));
    }

    #[test]
    fn rejects_null_tool_input() {
        let react = react_with_tools(&["read_file"]);
        let response = ReActResponse::Tool {
            name: "read_file".into(),
            input: Value::Null,
        };
        assert!(react.validate_response(&response).is_err());
    }

    #[test]
    fn rejects_empty_plan() {
        let react = react_with_tools(&[]);
        let response = ReActResponse::Plan {
            content: "   ".into(),
        };
        assert!(react.validate_response(&response).is_err());
    }

    // ── full response parsing (JSON → validated ReActAction) ─────────────────

    #[test]
    fn parses_full_tool_response() {
        let json =
            extract_json(r#"{"type":"tool","name":"read_file","input":"src/lib.rs"}"#).unwrap();
        let response: ReActResponse = serde_json::from_value(json).unwrap();
        match convert_response(response) {
            ReActAction::Tool { name, input } => {
                assert_eq!(name, "read_file");
                assert_eq!(input, "src/lib.rs");
            }
            _ => panic!("expected Tool action"),
        }
    }

    #[test]
    fn preserves_final_answer_content() {
        let json =
            extract_json(r#"{"type":"answer","content":"The change is complete."}"#).unwrap();
        let response: ReActResponse = serde_json::from_value(json).unwrap();
        match convert_response(response) {
            ReActAction::Answer { content } => {
                assert_eq!(content, "The change is complete.");
            }
            _ => panic!("expected final answer"),
        }
    }

    #[test]
    fn parses_structured_tool_input() {
        let json = extract_json(
            r#"{"type":"tool","name":"write_file","input":{"path":"a.txt","content":"hi"}}"#,
        )
        .unwrap();
        let response: ReActResponse = serde_json::from_value(json).unwrap();
        match convert_response(response) {
            ReActAction::Tool { name, input } => {
                assert_eq!(name, "write_file");
                let parsed: Value = serde_json::from_str(&input).unwrap();
                assert_eq!(parsed["path"], "a.txt");
            }
            _ => panic!("expected Tool action"),
        }
    }
}
