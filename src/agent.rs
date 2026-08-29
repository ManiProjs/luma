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
    planner::{PlanAction, Planner, PlannerTrait},
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

                PlanAction::Answer { .. } => {
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
                            format!("Plan approved. Proceeding with implementation:\n{}", content),
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
            "\
You are Luma's Planner.

You are the decision-making system of a local-first
AI coding agent.

Choose exactly one of:

1. A tool action
2. Multiple tool actions
3. An answer

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
- command output
- dependencies

If information is missing, inspect it.

==================================================
TOOL RULES
==================================================

list_directory
    Use to understand directory structure.

read_file
    Use to understand actual file contents.

search_files
    Use to locate files, symbols, or text.

patch_file
    Use for precise changes to existing files.

write_file
    Use primarily for new files or complete replacements.

run_command
    Use for builds, tests, formatting, linting,
    and verification.

==================================================
MODIFICATION WORKFLOW
==================================================

For existing files:

read_file
→ understand
→ patch_file
→ verify

Never modify an existing file without reading it first.

Never invent patch_file's old text.

==================================================
CURRENT STATE
==================================================

Language:
{}

Directory listed:
{}

Config read:
{}

README read:
{}

Source read:
{}

GALAXY read:
{}

Previously inspected files:
{:?}

==================================================
PROJECT MEMORY
==================================================

GALAXY.md may contain workspace memory.

Treat observed information as authoritative.

Do not invent information that is not present.

==================================================
EFFICIENCY
==================================================

Avoid redundant tools.

Do not repeatedly list the same directory.

Do not reread unchanged files without reason.

Do not use write_file when patch_file is sufficient.

Use the smallest number of tools necessary.

==================================================
FINAL RULE
==================================================

When information is missing:
inspect.

When a tool can answer:
use the tool.

When modifying code:

inspect
→ modify
→ verify

Never guess when the filesystem can provide the answer.
",
            self.language.name(),
            self.inspection.directory,
            self.inspection.config,
            self.inspection.readme,
            self.inspection.source,
            self.inspection.galaxy,
            self.inspected_files,
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
