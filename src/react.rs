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
    persona: AgentPersona,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentPersona {
    Code,
    Teammate,
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
            persona: AgentPersona::Code,
        }
    }

    /// Construct the action selector for a user-serving personal assistant.
    pub fn new_teammate(model: M, tools: &ToolRegistry) -> Self {
        Self {
            model,
            tools: tools.names(),
            tool_metadata: tools.metadata(),
            persona: AgentPersona::Teammate,
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

        let persona = match self.persona {
            AgentPersona::Code => {
                "You are Luma Code, an AI coding agent. Help the user understand, build, debug, and maintain software. Prioritize correctness, inspect before editing, and verify changes."
            }
            AgentPersona::Teammate => {
                "You are a Luma Teammate: a capable personal assistant and member of a user-serving workforce. Your job is to help the user with their life, work, and responsibilities—not to assume every task is software development. You can research, reason, plan, organize, draft and revise writing, analyze information, coordinate work, track ongoing responsibilities, and collaborate with other teammates when those capabilities are available. Do not write or modify code, inspect repositories, or perform software-engineering work unless the user explicitly asks for it. If a request is ambiguous, choose the useful non-coding interpretation or ask one focused question. Act in the user's interests, be honest about what you have and have not done, and seek approval for consequential external actions."
            }
        };
        let persona_rules = match self.persona {
            AgentPersona::Code => "",
            AgentPersona::Teammate => {
                "\nTEAMMATE TASK BOUNDARIES:\n\n\
                 The workspace and implementation rules below are relevant only when the user\n\
                 explicitly asks you to do software-engineering work. For ordinary personal or\n\
                 professional requests, do not inspect the current directory, browse repositories,\n\
                 or use coding tools merely because they are available. Do not reinterpret general\n\
                 words such as 'work', 'project', 'fix', or 'organize' as permission to inspect or\n\
                 modify source code. If the user explicitly asks you to debug, write, change, or\n\
                 review code, you may use the relevant coding tools for that request.\n"
            }
        };

        format!(
            r#"{persona}

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
Never print the ask_question input JSON as an answer or explanation. It must be
sent as the input of a tool action so Luma can render the interactive question.

After an approved plan:
→ perform the required tool actions

{persona_rules}

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

        for attempt in 0..2 {
            let mut stream = match self
                .model
                .stream(CompletionRequest {
                    messages: request_messages.clone(),
                })
                .await
            {
                Ok(stream) => stream,
                Err(error) => {
                    tracing::error!(error = %error, attempt, "ReAct step failed");
                    return Err(error);
                }
            };

            let mut response = String::new();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => return Err(anyhow!("ReAct step interrupted.")),
                    chunk = stream.next() => {
                        let Some(chunk) = chunk else { break; };
                        response.push_str(&chunk?);
                    }
                }
            }

            tracing::debug!(
                response_bytes = response.len(),
                attempt,
                "ReAct response received"
            );
            let parsed = extract_json(&response).and_then(|json| {
                let react_response: ReActResponse = serde_json::from_value(json)
                    .map_err(|error| anyhow!("Invalid ReAct response schema: {error}"))?;
                self.validate_response(&react_response)?;
                Ok(react_response)
            });

            match parsed {
                Ok(action) => return Ok(convert_response(action)),
                Err(error) if attempt == 0 => {
                    tracing::warn!(error = %error, "ReAct response did not satisfy the action protocol; retrying once");
                    request_messages.push(Message {
                        role: MessageRole::User,
                        content: "Your previous response did not follow the required action protocol. Try again. Return exactly one JSON object with type tool, answer, or plan. Do not include prose, XML, Markdown, or provider-specific tool markup. If the request is conversational, return an answer object.".into(),
                    });
                }
                Err(error) => return Err(error),
            }
        }

        Err(anyhow!("ReAct could not produce a valid action"))
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

    // Some OpenAI-compatible reasoning models emit their native XML-style
    // function-call envelope even though the ReAct contract asks for JSON.
    // Normalize that envelope at the boundary so tool calls (especially
    // ask_question) still enter the regular, validated interaction loop.
    if let Some(tool_call) = extract_xml_tool_call(clean) {
        return Ok(tool_call);
    }

    if let Ok(value) = serde_json::from_str::<Value>(clean) {
        return Ok(value);
    }

    // Scan balanced top-level objects instead of slicing from the first `{`
    // to the last `}`. Models sometimes include braces in prose or emit a
    // short preamble before their actual action object.
    let bytes = clean.as_bytes();
    let mut start = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut last_error = None;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' if depth > 0 => in_string = true,
            b'{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(begin) = start {
                        if let Ok(value) = serde_json::from_slice::<Value>(&bytes[begin..=index]) {
                            return Ok(value);
                        } else if let Err(error) =
                            serde_json::from_slice::<Value>(&bytes[begin..=index])
                        {
                            last_error = Some(error);
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    Err(anyhow!(
        "ReAct returned invalid JSON{}:\n{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default(),
        clean
    ))
}

/// Detect model-internal action payloads in fallback text, including JSON
/// preceded by a natural-language preamble and provider XML envelopes.
pub(crate) fn is_internal_action_response(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("<invoke")
        || lower.contains("</invoke>")
        || lower.contains("<ernie:tool_call")
        || lower.contains("</ernie:tool_call>")
        || lower.contains("<tool_call")
        || lower.contains("<function_call")
    {
        return true;
    }

    extract_json(text)
        .ok()
        .and_then(|value| value.get("type").and_then(Value::as_str).map(str::to_owned))
        .is_some_and(|kind| matches!(kind.as_str(), "tool" | "plan" | "answer"))
}

fn extract_xml_tool_call(text: &str) -> Option<Value> {
    let invoke_start = text.find("<invoke ")?;
    let invoke_end = text[invoke_start..].find('>')? + invoke_start;
    let opening = &text[invoke_start..=invoke_end];
    let name_start = opening.find("name=")? + 5;
    let quote = opening.as_bytes().get(name_start).copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let name_value_start = name_start + 1;
    let name_end = opening[name_value_start..].find(quote as char)? + name_value_start;
    let name = &opening[name_value_start..name_end];
    if name.trim().is_empty() {
        return None;
    }

    let body_end = text[invoke_end + 1..].find("</invoke>")? + invoke_end + 1;
    let body = &text[invoke_end + 1..body_end];
    let mut input = serde_json::Map::new();
    let mut remaining = body;
    while let Some(parameter_start) = remaining.find("<parameter ") {
        remaining = &remaining[parameter_start + 11..];
        let tag_end = remaining.find('>')?;
        let tag = &remaining[..tag_end];
        let key_start = tag.find("name=")? + 5;
        let key_quote = tag.as_bytes().get(key_start).copied()?;
        if key_quote != b'"' && key_quote != b'\'' {
            return None;
        }
        let key_value_start = key_start + 1;
        let key_end = tag[key_value_start..].find(key_quote as char)? + key_value_start;
        let key = xml_unescape(&tag[key_value_start..key_end]);
        let value_start = tag_end + 1;
        let close = remaining[value_start..].find("</parameter>")? + value_start;
        let raw_value = xml_unescape(remaining[value_start..close].trim());
        let value = serde_json::from_str::<Value>(&raw_value).unwrap_or(Value::String(raw_value));
        input.insert(key, value);
        remaining = &remaining[close + "</parameter>".len()..];
    }
    Some(serde_json::json!({"type":"tool", "name":name, "input":input}))
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelStream;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

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
    fn parses_ernie_xml_tool_call_envelope() {
        let value = extract_json(
            "</ernie:tool_call><invoke name=\"list_directory\"><parameter name=\"depth\">2</parameter><parameter name=\"path\">.</parameter></invoke></ernie:tool_call>",
        ).unwrap();
        assert_eq!(value["type"], "tool");
        assert_eq!(value["name"], "list_directory");
        assert_eq!(value["input"]["depth"], 2);
        assert_eq!(value["input"]["path"], ".");
    }

    #[test]
    fn finds_balanced_json_without_merging_braces_from_prose() {
        let value = extract_json(
            "I considered {an incomplete example}. Actual action: {\"type\":\"answer\",\"content\":\"done\"}",
        ).unwrap();
        assert_eq!(value["type"], "answer");
        assert_eq!(value["content"], "done");
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

    #[test]
    fn detects_protocol_json_even_when_preceded_by_prose() {
        assert!(is_internal_action_response(
            "The action is: {\"type\":\"tool\",\"name\":\"ask_question\",\"input\":{}}"
        ));
        assert!(!is_internal_action_response("A normal answer."));
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

    #[derive(Clone)]
    struct SequenceModel {
        responses: Arc<Mutex<VecDeque<String>>>,
        requests: Arc<Mutex<Vec<Vec<Message>>>>,
    }

    #[async_trait::async_trait]
    impl Model for SequenceModel {
        async fn stream(&self, request: CompletionRequest) -> Result<ModelStream> {
            self.requests.lock().unwrap().push(request.messages);
            let response = self.responses.lock().unwrap().pop_front().unwrap();
            Ok(Box::pin(futures_util::stream::iter([Ok(response)])))
        }
    }

    #[tokio::test]
    async fn retries_once_when_model_ignores_the_json_action_protocol() {
        let model = SequenceModel {
            responses: Arc::new(Mutex::new(VecDeque::from([
                "I need to ask the user a question.".into(),
                r#"{"type":"tool","name":"ask_question","input":{"prompt":"Which option?"}}"#
                    .into(),
            ]))),
            requests: Arc::new(Mutex::new(Vec::new())),
        };
        let requests = model.requests.clone();
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool {
            name: "ask_question".into(),
        });
        let react = ReAct::new(model, &registry);
        let action = react
            .next_action(Vec::new(), CancellationToken::new())
            .await
            .unwrap();
        assert!(matches!(action, ReActAction::Tool { name, .. } if name == "ask_question"));
        let captured = requests.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert!(
            captured[1]
                .last()
                .unwrap()
                .content
                .contains("did not follow the required action protocol")
        );
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
        assert!(prompt.contains("Luma Code"));
    }

    #[test]
    fn teammate_prompt_prioritizes_user_work_and_disables_unsolicited_coding() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool {
            name: "web_search".into(),
        });
        let react = ReAct::new_teammate(MockModel, &registry);
        let prompt = react.system_prompt();
        assert!(prompt.contains("user-serving workforce"));
        assert!(prompt.contains("research, reason, plan, organize"));
        assert!(prompt.contains("unless the user explicitly asks for it"));
        assert!(prompt.contains("collaborate with other teammates"));
        assert!(prompt.contains("do not inspect the current directory"));
        assert!(prompt.contains("Do not reinterpret general"));
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
