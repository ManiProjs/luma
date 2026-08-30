use std::{path::Path, time::Instant};

use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;

use crate::{
    context::{Context, Message, MessageRole},
    event::AgentEvent,
    history::{History, HistoryMessage},
    model::{CompletionRequest, Model},
    planner::{PlanAction, PlannerTrait},
    router::{RoutedAction, ToolRouter},
    tools::ToolRegistry,
    workspace::language::{self, ProgrammingLanguage},
};

// ============================================================================
// Confirmation
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    Allow,
    Deny,
}

fn requires_confirmation(tool: &str) -> bool {
    matches!(tool, "write_file" | "patch_file" | "run_command")
}

// ============================================================================
// Workspace inspection
// ============================================================================

#[derive(Default, Debug)]
struct InspectionState {
    directory: bool,
    config: bool,
    readme: bool,
    source: bool,
    galaxy: bool,
}

// ============================================================================
// Planning state
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlanningState {
    Exploring,
    Implementing,
}

// ============================================================================
// Agent
// ============================================================================

pub struct Agent<M, P> {
    model: M,
    planner: P,
    context: Context,
    tools: ToolRegistry,
    history: History,

    inspected_files: Vec<String>,
    inspection: InspectionState,
    language: ProgrammingLanguage,
    planning_state: PlanningState,
}

impl<M, P> Agent<M, P>
where
    M: Model,
    P: PlannerTrait,
{
    pub fn new(
        model: M,
        planner: P,
        tools: ToolRegistry,
        history: History,
        galaxy: String,
    ) -> Self {
        let mut context = Context::new();

        if !galaxy.trim().is_empty() {
            context.add(
                MessageRole::System,
                format!(
                    "\
# Luma Workspace Memory

The following information comes from GALAXY.md.

Treat it as project memory.

---

{}

---
",
                    galaxy
                ),
            );
        }

        Self {
            model,
            planner,
            context,
            tools,
            history,
            inspected_files: Vec::new(),
            inspection: InspectionState::default(),
            language: ProgrammingLanguage::Unknown,
            planning_state: PlanningState::Exploring,
        }
    }

    // ========================================================================
    // Main loop
    // ========================================================================

    pub async fn run(
        &mut self,
        mut input_rx: Receiver<String>,
        event_tx: Sender<AgentEvent>,
        cancel: CancellationToken,
        confirmation_rx: Receiver<Confirmation>,
    ) -> Result<()> {
        let mut confirmation_rx = confirmation_rx;

        while let Some(input) = input_rx.recv().await {
            if cancel.is_cancelled() {
                continue;
            }

            self.handle_input(input, &event_tx, &cancel, &mut confirmation_rx)
                .await?;
        }

        Ok(())
    }

    async fn handle_input(
        &mut self,
        input: String,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        let input = input.trim();

        if input.is_empty() {
            return Ok(());
        }

        if input == "/init" {
            self.initialize_workspace(tx, cancel, confirmation_rx)
                .await?;

            self.finish(tx).await?;
            return Ok(());
        }

        self.begin_request(input)?;

        match ToolRouter::route(input) {
            RoutedAction::Tool { name, input } => {
                self.run_direct_tool(&name, &input, tx, cancel, confirmation_rx)
                    .await?;

                self.finish(tx).await?;
                return Ok(());
            }

            RoutedAction::Planner => {}
        }

        if self.needs_workspace(input) {
            self.run_agent_loop(tx, cancel, confirmation_rx).await?;
        } else {
            self.run_conversation(tx, cancel).await?;
        }

        self.finish(tx).await
    }

    async fn finish(&self, tx: &Sender<AgentEvent>) -> Result<()> {
        tx.send(AgentEvent::Finished).await?;
        Ok(())
    }

    // ========================================================================
    // Request / history
    // ========================================================================

    fn begin_request(&mut self, input: &str) -> Result<()> {
        self.inspection = InspectionState::default();
        self.inspected_files.clear();
        self.planning_state = PlanningState::Exploring;

        self.context.add(MessageRole::User, input.to_owned());

        self.history.messages.push(HistoryMessage {
            role: "user".into(),
            content: input.to_owned(),
        });

        self.history.save()
    }

    fn store_assistant_response(&mut self, response: &str) -> Result<()> {
        if response.trim().is_empty() {
            return Ok(());
        }

        self.context
            .add(MessageRole::Assistant, response.to_owned());

        self.history.messages.push(HistoryMessage {
            role: "assistant".into(),
            content: response.to_owned(),
        });

        self.history.save()
    }

    // ========================================================================
    // Conversation
    // ========================================================================

    async fn run_conversation(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        match self.generate_answer(tx, cancel).await {
            Ok(response) => self.store_assistant_response(&response)?,

            Err(error) => {
                tx.send(AgentEvent::Error(error.to_string())).await?;
            }
        }

        Ok(())
    }

    async fn generate_answer(
        &self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let mut messages = vec![Message {
            role: MessageRole::System,
            content: self.answer_system_prompt(),
        }];

        messages.extend(self.context.messages().iter().cloned());

        let mut stream = self.model.stream(CompletionRequest { messages }).await?;

        let mut response = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tx.send(AgentEvent::Error(
                        "Generation interrupted.".into()
                    ))
                    .await?;

                    break;
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    let chunk = chunk?;

                    response.push_str(&chunk);

                    tx.send(AgentEvent::TextDelta(chunk))
                        .await?;
                }
            }
        }

        Ok(response)
    }

    fn answer_system_prompt(&self) -> String {
        "\
You are Luma.
You are running inside the Luma coding agent.
The underlying model is not the assistant's identity.

You are a local-first AI coding agent.

Be concise, technical, and practical.

Do not claim workspace facts without tool observations.

When the user asks about the workspace, use the available tools
rather than guessing.

Do not invent files, commands, project structure, dependencies,
or tool results.
"
        .into()
    }

    // ========================================================================
    // Agent loop
    // ========================================================================

    async fn run_agent_loop(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        tx.send(AgentEvent::Thinking).await?;

        const MAX_STEPS: usize = 12;

        for step in 0..MAX_STEPS {
            if cancel.is_cancelled() {
                tx.send(AgentEvent::Error("Interrupted.".into())).await?;

                return Ok(());
            }

            let plan = match self.create_plan(cancel).await {
                Ok(plan) => plan,

                Err(error) => {
                    tx.send(AgentEvent::Error(format!("Planner failed: {}", error)))
                        .await?;

                    self.run_conversation(tx, cancel).await?;
                    return Ok(());
                }
            };

            match plan {
                PlanAction::Tool { name, input } => {
                    self.execute_tool(&name, &input, tx, cancel, confirmation_rx)
                        .await?;
                }

                PlanAction::Multi { actions } => {
                    self.execute_actions(actions, tx, cancel, confirmation_rx)
                        .await?;
                }

                PlanAction::Answer => {
                    self.run_conversation(tx, cancel).await?;
                    return Ok(());
                }

                PlanAction::Plan { content } => {
                    tx.send(AgentEvent::PlanGenerated(content.clone())).await?;

                    let display = format!("Plan:\n{}", content);

                    let allowed = self
                        .request_confirmation("approve_plan", &display, tx, cancel, confirmation_rx)
                        .await?;

                    if allowed {
                        self.planning_state = PlanningState::Implementing;
                        self.context.add(
                            MessageRole::System,
                            format!(
                                "Plan approved. Proceeding with implementation:\n{}",
                                content
                            ),
                        );
                    } else {
                        self.planning_state = PlanningState::Exploring;
                        self.context.add(
                            MessageRole::System,
                            "Plan rejected. Rethink the approach and either explore more or propose a revised plan."
                                .to_string(),
                        );
                    }
                }
            }

            if step + 1 == MAX_STEPS {
                self.run_conversation(tx, cancel).await?;
            }
        }

        Ok(())
    }

    async fn execute_actions(
        &mut self,
        actions: Vec<PlanAction>,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        for action in actions {
            if cancel.is_cancelled() {
                return Ok(());
            }

            let PlanAction::Tool { name, input } = action else {
                continue;
            };

            if self.planning_state == PlanningState::Exploring
                && (name == "write_file" || name == "patch_file")
            {
                tx.send(AgentEvent::Error(format!(
                    "Cannot execute '{}' while in planning phase. Approve a plan first.",
                    name
                )))
                .await?;

                break;
            }

            if let Err(error) = self
                .execute_tool(&name, &input, tx, cancel, confirmation_rx)
                .await
            {
                tx.send(AgentEvent::Error(format!("{} failed: {}", name, error)))
                    .await?;

                break;
            }
        }

        Ok(())
    }

    async fn create_plan(&self, cancel: &CancellationToken) -> Result<PlanAction> {
        let mut messages = vec![Message {
            role: MessageRole::System,
            content: self.planner_system_prompt(),
        }];

        messages.extend(self.context.messages().iter().cloned());

        self.planner.plan(messages, cancel.clone()).await
    }

    // ========================================================================
    // Tool execution
    // ========================================================================

    async fn execute_tool(
        &mut self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        if cancel.is_cancelled() {
            return Ok(());
        }

        let display = Self::format_tool_display(name, input);

        if requires_confirmation(name) {
            let allowed = self
                .request_confirmation(name, &display, tx, cancel, confirmation_rx)
                .await?;

            if !allowed {
                return Ok(());
            }
        }

        tx.send(AgentEvent::ToolStarted {
            name: name.to_owned(),
            input: display,
        })
        .await?;

        if cancel.is_cancelled() {
            return Ok(());
        }

        let started = Instant::now();

        let result = self
            .tools
            .execute(name, input.trim())
            .map_err(|error| anyhow!("{} failed: {}", name, error))?;

        if cancel.is_cancelled() {
            return Ok(());
        }

        self.update_inspection(name, input.trim());

        tx.send(AgentEvent::ToolFinished {
            name: name.to_owned(),
            duration_ms: started.elapsed().as_millis(),
        })
        .await?;

        self.context.add(
            MessageRole::Observation,
            format!("Observation from `{}`:\n{}", name, result),
        );

        Ok(())
    }

    async fn request_confirmation(
        &self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<bool> {
        tx.send(AgentEvent::ConfirmationRequired {
            name: name.to_owned(),
            input: input.to_owned(),
        })
        .await?;

        let confirmation = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(false);
            }

            result = confirmation_rx.recv() => {
                result.ok_or_else(|| {
                    anyhow!("Confirmation channel closed")
                })?
            }
        };

        match confirmation {
            Confirmation::Allow => Ok(true),

            Confirmation::Deny => {
                tx.send(AgentEvent::SystemMessage(format!("Skipped {}.", name)))
                    .await?;

                Ok(false)
            }
        }
    }

    async fn run_direct_tool(
        &mut self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        if let Err(error) = self
            .execute_tool(name, input, tx, cancel, confirmation_rx)
            .await
        {
            tx.send(AgentEvent::Error(error.to_string())).await?;
        }

        Ok(())
    }

    fn format_tool_display(name: &str, input: &str) -> String {
        match name {
            "read_file" => {
                format!("read_file → {}", input)
            }

            "write_file" | "patch_file" => match serde_json::from_str::<serde_json::Value>(input) {
                Ok(json) => {
                    let path = json
                        .get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("?");

                    format!("{} → {}", name, path)
                }

                Err(_) => {
                    format!("{} → invalid JSON", name)
                }
            },

            "run_command" => {
                format!("run_command → {}", input)
            }

            _ => {
                format!("{} {}", name, input)
            }
        }
    }

    // ========================================================================
    // Workspace detection
    // ========================================================================

    fn needs_workspace(&self, input: &str) -> bool {
        let input = input.trim().to_lowercase();

        if input.is_empty() {
            return false;
        }

        // ------------------------------------------------------------------------
        // Explicit filesystem references
        // ------------------------------------------------------------------------

        if Self::contains_path_reference(&input) {
            return true;
        }

        // ------------------------------------------------------------------------
        // File operations
        // ------------------------------------------------------------------------

        const FILE_ACTIONS: &[&str] = &[
            "read", "open", "edit", "modify", "change", "update", "rewrite", "refactor", "rename",
            "move", "delete", "remove", "create", "write", "patch", "fix", "replace", "add",
            "remove",
        ];

        const FILE_TARGETS: &[&str] = &[
            "file",
            "files",
            "code",
            "source",
            "source code",
            "implementation",
            "function",
            "method",
            "class",
            "module",
            "struct",
            "enum",
            "trait",
            "variable",
            "config",
            "configuration",
            "script",
        ];

        if Self::contains_action_target(&input, FILE_ACTIONS, FILE_TARGETS) {
            return true;
        }

        // ------------------------------------------------------------------------
        // Workspace / repository inspection
        // ------------------------------------------------------------------------

        const WORKSPACE_TERMS: &[&str] = &[
            "workspace",
            "project",
            "repository",
            "repo",
            "codebase",
            "code base",
            "current project",
            "this project",
            "my project",
            "my code",
            "this code",
            "current code",
            "working directory",
            "directory",
            "folder",
            "file tree",
            "project tree",
            "project structure",
            "directory structure",
            "project layout",
            "codebase structure",
        ];

        if Self::contains_any(&input, WORKSPACE_TERMS) {
            return true;
        }

        // ------------------------------------------------------------------------
        // Inspection verbs
        // ------------------------------------------------------------------------

        const INSPECTION_PATTERNS: &[&str] = &[
            "show me the files",
            "show the files",
            "show files",
            "list the files",
            "list files",
            "list the directory",
            "list directory",
            "show the directory",
            "show directory",
            "what files",
            "which files",
            "find the file",
            "find files",
            "find where",
            "search the code",
            "search my code",
            "search the project",
            "search the repository",
            "look through the code",
            "look through my code",
            "look through the project",
            "browse the project",
            "explore the project",
            "inspect the project",
            "inspect the code",
            "inspect my code",
            "what's in this project",
            "what is in this project",
            "what's in the project",
            "what is in the project",
            "how is this project structured",
            "how is the project structured",
        ];

        if Self::contains_any(&input, INSPECTION_PATTERNS) {
            return true;
        }

        // ------------------------------------------------------------------------
        // Build / test / lint / debug requests
        //
        // These are workspace operations because the answer depends on actually
        // running commands or inspecting the project.
        // ------------------------------------------------------------------------

        const WORKSPACE_OPERATIONS: &[&str] = &[
            "build this",
            "build the project",
            "build my project",
            "compile this",
            "compile the project",
            "run the tests",
            "run tests",
            "test this",
            "test the project",
            "run the project",
            "run this project",
            "start the project",
            "launch the project",
            "lint the project",
            "format the project",
            "format this project",
            "check the project",
            "check my code",
            "debug this",
            "debug the project",
            "debug my code",
            "why does this fail",
            "why is this failing",
            "why doesn't this work",
            "why does this not work",
            "fix this error",
            "fix this bug",
            "fix the error",
            "fix the bug",
        ];

        if Self::contains_any(&input, WORKSPACE_OPERATIONS) {
            return true;
        }

        // ------------------------------------------------------------------------
        // Explicit development-tool commands
        // ------------------------------------------------------------------------

        const TOOL_COMMANDS: &[&str] = &[
            "cargo ",
            "cargo",
            "npm ",
            "npm",
            "pnpm ",
            "pnpm",
            "yarn ",
            "yarn",
            "bun ",
            "bun",
            "python ",
            "pytest ",
            "pip ",
            "poetry ",
            "uv ",
            "go ",
            "go test",
            "go build",
            "cmake ",
            "make ",
            "swift ",
            "xcodebuild ",
            "dotnet ",
            "gradle ",
            "mvn ",
        ];

        if Self::contains_any(&input, TOOL_COMMANDS) {
            return true;
        }

        false
    }

    fn contains_any(input: &str, terms: &[&str]) -> bool {
        terms.iter().any(|term| input.contains(term))
    }

    fn contains_action_target(input: &str, actions: &[&str], targets: &[&str]) -> bool {
        actions.iter().any(|action| {
            targets.iter().any(|target| {
                input.contains(&format!("{action} {target}"))
                    || input.contains(&format!("{action} the {target}"))
                    || input.contains(&format!("{action} my {target}"))
                    || input.contains(&format!("{action} this {target}"))
            })
        })
    }

    fn contains_path_reference(input: &str) -> bool {
        // Unix paths.
        if input.contains('/') {
            return true;
        }

        // Home-relative paths.
        if input.contains("~/") {
            return true;
        }

        // Common source/config extensions.
        const EXTENSIONS: &[&str] = &[
            ".rs", ".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".java", ".kt", ".swift", ".c",
            ".h", ".cpp", ".hpp", ".cc", ".zig", ".lua", ".rb", ".php", ".cs", ".fs", ".fsx",
            ".dart", ".vue", ".svelte", ".html", ".css", ".scss", ".json", ".toml", ".yaml",
            ".yml", ".xml", ".md", ".txt", ".lock",
        ];

        if EXTENSIONS.iter().any(|ext| input.contains(ext)) {
            return true;
        }

        false
    }

    // ========================================================================
    // Inspection
    // ========================================================================

    fn update_inspection(&mut self, name: &str, input: &str) {
        match name {
            "list_directory" => {
                self.inspection.directory = true;
            }

            "read_file" => {
                self.inspect_file(input);
            }

            _ => {}
        }
    }

    fn inspect_file(&mut self, input: &str) {
        let path = input.trim();
        let file = path.to_lowercase();

        if !self.inspected_files.iter().any(|item| item == path) {
            self.inspected_files.push(path.to_owned());

            let detected = language::detect_from_file(path);

            if detected != ProgrammingLanguage::Unknown {
                self.language = detected;
            }
        }

        if is_config_file(&file) {
            self.inspection.config = true;
        }

        if file.ends_with("readme.md") {
            self.inspection.readme = true;
        }

        if is_source_file(&file) {
            self.inspection.source = true;
        }

        if file.ends_with("galaxy.md") {
            self.inspection.galaxy = true;
        }
    }

    // ========================================================================
    // Planner prompt
    // ========================================================================

    fn planner_system_prompt(&self) -> String {
        format!(
            r#"
You are Luma's Planner.

You are NOT the assistant speaking to the user.
You are an internal decision-making component of Luma.

Your output is consumed directly by Luma.
Therefore your response MUST be valid JSON.
DO NOT output Markdown.
DO NOT output explanations.
DO NOT output conversational text.
DO NOT identify yourself as Qwen, an AI assistant, or anything other than Luma's Planner.

==================================================
YOUR JOB
==================================================

Decide the NEXT action Luma should take to accomplish the user's request.

You may return exactly ONE of these action types:

- tool
- multi
- answer
- plan

The top-level JSON object MUST contain a "type" field.

==================================================
AVAILABLE TOOLS
==================================================

{}

==================================================
JSON SCHEMA
==================================================

------------------------------
TOOL
------------------------------

Use this when exactly one tool should be executed.

Required fields:

{{
  "type": "tool",
  "name": "TOOL_NAME",
  "input": TOOL_INPUT
}}

Example:

{{
  "type": "tool",
  "name": "read_file",
  "input": "src/main.rs"
}}

For tools requiring structured input, "input" MUST be a JSON object:

{{
  "type": "tool",
  "name": "write_file",
  "input": {{
    "path": "/tmp/hello.txt",
    "content": "Hello, Luma!"
  }}
}}

IMPORTANT:

"type" MUST be exactly "tool".

"name" MUST be the tool name.

"input" MUST contain the tool's actual input.

NEVER return tool arguments directly at the top level.

WRONG:

{{
  "content": "Hello, Luma!",
  "path": "/tmp/hello.txt"
}}

WRONG:

{{
  "name": "write_file",
  "path": "/tmp/hello.txt",
  "content": "Hello, Luma!"
}}

CORRECT:

{{
  "type": "tool",
  "name": "write_file",
  "input": {{
    "path": "/tmp/hello.txt",
    "content": "Hello, Luma!"
  }}
}}

------------------------------
MULTI
------------------------------

Use this when multiple independent tool actions should be performed.

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

Every item in "actions" MUST itself be a valid planner action.

Do not put raw tool arguments inside "actions".

------------------------------
ANSWER
------------------------------

Use this when the user's request can be answered without workspace
inspection or tool execution.

{{
  "type": "answer",
  "content": "The answer..."
}}

For example, if the user says:

"Hello"

return something like:

{{
  "type": "answer",
  "content": "Hello!"
}}

Do NOT use a tool for ordinary conversation.

------------------------------
PLAN
------------------------------

Use this when you have explored enough of the workspace and should
propose an implementation plan before making modifications.

{{
  "type": "plan",
  "content": "1. Read the relevant file. 2. Modify it. 3. Run tests."
}}

A plan is NOT a tool action.

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

Do not repeat inspections that already provided the required information.

==================================================
MODIFICATION WORKFLOW
==================================================

For an existing file:

1. Read the file first.
2. Understand the relevant code.
3. Modify the file.
4. Verify the modification.

Never modify an existing file that has not been observed.

Never invent old file contents.

Prefer:

read_file
→ patch_file
→ run_command

Use write_file primarily when:

- creating a new file
- replacing an entire file is genuinely appropriate

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
    Make precise modifications to existing files.

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

Prefer independent reads in a "multi" action.

==================================================
DECISION RULES
==================================================

If the user asks a normal conversational question:

→ answer

If the user asks about the workspace but required information is unknown:

→ inspect

If the user asks to modify an existing file:

→ read it first

If the user asks to create a new file:

→ use write_file

If the user asks to modify an existing file after it has been inspected:

→ use patch_file

If code was modified:

→ verify with run_command when appropriate

If several independent files must be inspected:

→ use multi

If enough information has been gathered and a modification requires
explicit approval:

→ return plan

==================================================
CRITICAL OUTPUT RULE
==================================================

Your ENTIRE response MUST be exactly ONE valid JSON object.

The object MUST contain "type".

Valid top-level forms are ONLY:

{{
  "type": "tool",
  "name": "...",
  "input": ...
}}

or:

{{
  "type": "multi",
  "actions": [...]
}}

or:

{{
  "type": "answer",
  "content": "..."
}}

or:

{{
  "type": "plan",
  "content": "..."
}}

NEVER omit "type".

NEVER return raw tool arguments.

NEVER wrap JSON in Markdown code fences.

NEVER add text before or after the JSON.

Return JSON only.
"#,
            self.tools.descriptions().join("\n")
        )
    }

    // ========================================================================
    // Workspace initialization
    // ========================================================================

    async fn initialize_workspace(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        self.execute_tool("list_directory", ".", tx, cancel, confirmation_rx)
            .await?;

        let (project, language, important_files) = self.detect_project();
        let structure = self.directory_structure();

        let galaxy = format!(
            "\
# GALAXY.md

Generated by Luma.

## Project

{}

## Language

{}

## Important Files

{}

## Structure

{}

## Notes

This file is Luma's workspace memory.

Update it when major architecture changes happen.
",
            project,
            language,
            if important_files.is_empty() {
                "No important files detected.".to_owned()
            } else {
                important_files.join("\n")
            },
            structure,
        );

        let input = serde_json::json!({
            "path": "GALAXY.md",
            "content": galaxy,
        });

        self.execute_tool(
            "write_file",
            &input.to_string(),
            tx,
            cancel,
            confirmation_rx,
        )
        .await?;

        self.context.add(
            MessageRole::System,
            format!("Updated GALAXY.md workspace memory:\n\n{}", galaxy),
        );

        Ok(())
    }

    fn detect_project(&self) -> (String, String, Vec<String>) {
        let mut project = "Unknown project".to_owned();
        let mut language = "Unknown".to_owned();
        let mut files = Vec::new();

        if Path::new("Cargo.toml").exists() {
            language = "Rust".into();
            files.push("Cargo.toml".into());

            if let Ok(content) = std::fs::read_to_string("Cargo.toml") {
                for line in content.lines() {
                    let Some(name) = line.strip_prefix("name =") else {
                        continue;
                    };

                    project = name.replace('"', "").trim().to_owned();

                    break;
                }
            }
        }

        if Path::new("package.json").exists() {
            language = "JavaScript / TypeScript".into();
            files.push("package.json".into());
        }

        if Path::new("pyproject.toml").exists() || Path::new("requirements.txt").exists() {
            language = "Python".into();

            if Path::new("pyproject.toml").exists() {
                files.push("pyproject.toml".into());
            }

            if Path::new("requirements.txt").exists() {
                files.push("requirements.txt".into());
            }
        }

        (project, language, files)
    }

    fn directory_structure(&self) -> String {
        std::fs::read_dir(".")
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|_| "Unable to inspect.".into())
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_config_file(path: &str) -> bool {
    path.ends_with("cargo.toml")
        || path.ends_with("package.json")
        || path.ends_with("pyproject.toml")
        || path.ends_with("requirements.txt")
}

fn is_source_file(path: &str) -> bool {
    path.contains("src/")
        || path.ends_with(".rs")
        || path.ends_with(".py")
        || path.ends_with(".js")
        || path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".jsx")
        || path.ends_with(".go")
        || path.ends_with(".java")
        || path.ends_with(".kt")
        || path.ends_with(".swift")
        || path.ends_with(".c")
        || path.ends_with(".h")
        || path.ends_with(".cpp")
        || path.ends_with(".hpp")
}
