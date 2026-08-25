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
    planner::{PlanAction, Planner},
    router::{RoutedAction, ToolRouter},
    tools::ToolRegistry,
    workspace::language::{self, ProgrammingLanguage},
};

// ============================================================
// Confirmation
// ============================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    Allow,
    Deny,
}

fn requires_confirmation(tool: &str) -> bool {
    matches!(tool, "write_file" | "patch_file" | "run_command")
}

// ============================================================
// Inspection
// ============================================================

#[derive(Default, Debug)]
struct InspectionState {
    listed: bool,
    config: bool,
    readme: bool,
    source: bool,
    galaxy: bool,
}

// ============================================================
// Agent
// ============================================================

pub struct Agent<M, P> {
    model: M,
    planner: P,
    context: Context,
    tools: ToolRegistry,
    inspected_files: Vec<String>,
    inspection: InspectionState,
    history: History,
    language: ProgrammingLanguage,
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
                    r#"
# Luma Workspace Memory

The following information comes from GALAXY.md.

Treat it as project memory.

---

{}

---
"#,
                    galaxy
                ),
            );
        }

        Self {
            model,
            planner,
            context,
            tools,
            inspected_files: Vec::new(),
            inspection: InspectionState::default(),
            history,
            language: ProgrammingLanguage::Unknown,
        }
    }

    // ========================================================
    // Main loop
    // ========================================================

    pub async fn run(
        &mut self,
        mut rx: Receiver<String>,
        tx: Sender<AgentEvent>,
        cancel: CancellationToken,
        mut confirmation_rx: Receiver<Confirmation>,
    ) -> Result<()> {
        while let Some(input) = rx.recv().await {
            if cancel.is_cancelled() {
                continue;
            }

            if input.trim() == "/init" {
                self.initialize_workspace(&tx, &cancel, &mut confirmation_rx)
                    .await?;

                self.send_finished(&tx).await?;
                continue;
            }

            self.begin_request(&input)?;

            match ToolRouter::route(&input) {
                RoutedAction::Tool { name, input } => {
                    self.run_direct_tool(&name, &input, &tx, &cancel, &mut confirmation_rx)
                        .await?;

                    self.send_finished(&tx).await?;
                    continue;
                }

                RoutedAction::Planner => {}
            }

            if !self.needs_workspace(&input) {
                self.run_conversation(&tx, &cancel).await?;

                self.send_finished(&tx).await?;
                continue;
            }

            self.run_agent_loop(&tx, &cancel, &mut confirmation_rx)
                .await?;

            self.send_finished(&tx).await?;
        }

        Ok(())
    }

    fn begin_request(&mut self, input: &str) -> Result<()> {
        self.inspection = InspectionState::default();
        self.inspected_files.clear();

        self.context.add(MessageRole::User, input.to_string());

        self.history.messages.push(HistoryMessage {
            role: "user".into(),
            content: input.to_string(),
        });

        self.history.save()?;

        Ok(())
    }

    async fn send_finished(&self, tx: &Sender<AgentEvent>) -> Result<()> {
        tx.send(AgentEvent::Finished).await?;
        Ok(())
    }

    // ========================================================
    // Conversation
    // ========================================================

    async fn run_conversation(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        match self.answer(tx, cancel).await {
            Ok(response) => {
                self.store_assistant_response(&response)?;
            }

            Err(error) => {
                tx.send(AgentEvent::Error(error.to_string())).await?;
            }
        }

        Ok(())
    }

    async fn answer(&self, tx: &Sender<AgentEvent>, cancel: &CancellationToken) -> Result<String> {
        let mut messages = vec![Message {
            role: MessageRole::System,
            content: r#"
You are Luma.

You are a local-first AI coding agent.

Be concise, technical, and practical.

Do not claim workspace facts without tool observations.

When the user asks about the workspace, use the available tools
rather than guessing.

Do not invent files, commands, project structure, or tool results.
"#
            .to_string(),
        }];

        messages.extend(self.context.messages().iter().cloned());

        let mut stream = self.model.stream(CompletionRequest { messages }).await?;

        let mut response = String::new();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tx.send(
                        AgentEvent::Error(
                            "Generation interrupted.".into()
                        )
                    )
                    .await?;

                    break;
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    let chunk = chunk?;

                    response.push_str(&chunk);

                    tx.send(
                        AgentEvent::TextDelta(chunk)
                    )
                    .await?;
                }
            }
        }

        Ok(response)
    }

    fn store_assistant_response(&mut self, response: &str) -> Result<()> {
        if response.trim().is_empty() {
            return Ok(());
        }

        self.context
            .add(MessageRole::Assistant, response.to_string());

        self.history.messages.push(HistoryMessage {
            role: "assistant".into(),
            content: response.to_string(),
        });

        self.history.save()?;

        Ok(())
    }

    // ========================================================
    // Agent loop
    // ========================================================

    async fn run_agent_loop(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        tx.send(AgentEvent::Thinking).await?;

        for step in 0..12 {
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
                    for action in actions {
                        if cancel.is_cancelled() {
                            return Ok(());
                        }

                        let PlanAction::Tool { name, input } = action else {
                            continue;
                        };

                        if let Err(error) = self
                            .execute_tool(&name, &input, tx, cancel, confirmation_rx)
                            .await
                        {
                            tx.send(AgentEvent::Error(format!("{} failed: {}", name, error)))
                                .await?;

                            break;
                        }
                    }
                }

                PlanAction::Answer { .. } => {
                    self.run_conversation(tx, cancel).await?;
                    return Ok(());
                }
            }

            if step == 11 {
                self.run_conversation(tx, cancel).await?;
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

    // ========================================================
    // Tool execution
    // ========================================================

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

        let display_input = Self::format_tool_display(name, input);

        // ----------------------------------------------------
        // Confirmation policy
        // ----------------------------------------------------

        if requires_confirmation(name) {
            let allowed = self
                .request_confirmation(name, &display_input, tx, cancel, confirmation_rx)
                .await?;

            if !allowed {
                return Ok(());
            }
        }

        // ----------------------------------------------------
        // Start tool
        // ----------------------------------------------------

        tx.send(AgentEvent::ToolStarted {
            name: name.to_string(),
            input: display_input,
        })
        .await?;

        if cancel.is_cancelled() {
            return Ok(());
        }

        let start = Instant::now();

        let result = match self.tools.execute(name, input.trim()) {
            Ok(result) => result,

            Err(error) => {
                tx.send(AgentEvent::Error(format!("{} failed: {}", name, error)))
                    .await?;

                return Ok(());
            }
        };

        if cancel.is_cancelled() {
            return Ok(());
        }

        self.update_inspection(name, input.trim());

        tx.send(AgentEvent::ToolFinished {
            name: name.to_string(),
            duration_ms: start.elapsed().as_millis(),
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
            name: name.to_string(),
            input: input.to_string(),
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
                    let path = json.get("path").and_then(|v| v.as_str()).unwrap_or("?");

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

    // ========================================================
    // Workspace detection
    // ========================================================

    fn needs_workspace(&self, input: &str) -> bool {
        let input = input.to_lowercase();

        const KEYWORDS: &[&str] = &[
            "project",
            "repository",
            "repo",
            "code",
            "file",
            "files",
            "folder",
            "folders",
            "directory",
            "directories",
            "dir",
            "tree",
            "structure",
            "layout",
            "workspace",
            "list",
            "show",
            "find",
            "search",
            "browse",
            "explore",
            "inspect",
            "look",
            "rust",
            "cargo",
            "crate",
            "rustc",
            "c++",
            "cpp",
            "cmake",
            "makefile",
            "clang",
            "gcc",
            "python",
            "pip",
            "django",
            "flask",
            "fastapi",
            "javascript",
            "typescript",
            "node",
            "npm",
            "pnpm",
            "yarn",
            "react",
            "vue",
            "svelte",
            "java",
            "kotlin",
            "gradle",
            "maven",
            "golang",
            "go.mod",
            "swift",
            "xcode",
            "dart",
            "flutter",
            "c#",
            "csharp",
            "dotnet",
        ];

        KEYWORDS.iter().any(|word| input.contains(word))
    }

    // ========================================================
    // Inspection
    // ========================================================

    fn update_inspection(&mut self, name: &str, input: &str) {
        match name {
            "list_directory" => {
                self.inspection.listed = true;
            }

            "read_file" => {
                self.inspect_file(input);
            }

            _ => {}
        }
    }

    fn inspect_file(&mut self, input: &str) {
        let file = input.to_lowercase();

        if !self.inspected_files.contains(&input.to_string()) {
            self.inspected_files.push(input.to_string());

            let detected = language::detect_from_file(input);

            if detected != ProgrammingLanguage::Unknown {
                self.language = detected;
            }
        }

        if file.ends_with("cargo.toml")
            || file.ends_with("package.json")
            || file.ends_with("pyproject.toml")
            || file.ends_with("requirements.txt")
        {
            self.inspection.config = true;
        }

        if file.ends_with("readme.md") {
            self.inspection.readme = true;
        }

        if file.contains("src/")
            || file.ends_with(".rs")
            || file.ends_with(".py")
            || file.ends_with(".js")
            || file.ends_with(".ts")
        {
            self.inspection.source = true;
        }

        if file.ends_with("galaxy.md") {
            self.inspection.galaxy = true;
        }
    }

    // ========================================================
    // Planner prompt
    // ========================================================

    fn planner_system_prompt(&self) -> String {
        format!(
            r#"
You are Luma's Planner.

You are the decision-making system of a local-first
AI coding agent.

Your job is to choose exactly one of:

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
"#,
            self.language.name(),
            self.inspection.listed,
            self.inspection.config,
            self.inspection.readme,
            self.inspection.source,
            self.inspection.galaxy,
            self.inspected_files,
        )
    }

    // ========================================================
    // Initialization
    // ========================================================

    async fn initialize_workspace(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
        confirmation_rx: &mut Receiver<Confirmation>,
    ) -> Result<()> {
        self.execute_tool("list_directory", ".", tx, cancel, confirmation_rx)
            .await?;

        let (project, language, files) = self.detect_project();

        let structure = self.directory_structure();

        let galaxy = format!(
            r#"# GALAXY.md

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
"#,
            project,
            language,
            if files.is_empty() {
                "No important files detected.".to_string()
            } else {
                files.join("\n")
            },
            structure,
        );

        self.execute_tool(
            "write_file",
            &serde_json::json!({
                "path": "GALAXY.md",
                "content": galaxy,
            })
            .to_string(),
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
        let mut project = "Unknown project".to_string();

        let mut language = "Unknown".to_string();

        let mut files = Vec::new();

        if Path::new("Cargo.toml").exists() {
            language = "Rust".into();
            files.push("Cargo.toml".into());

            if let Ok(content) = std::fs::read_to_string("Cargo.toml") {
                for line in content.lines() {
                    if let Some(name) = line.strip_prefix("name =") {
                        project = name.replace('"', "").trim().to_string();

                        break;
                    }
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
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry| entry.file_name().into_string().ok())
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "Unable to inspect.".into())
    }
}

// ============================================================
// Planner adapter
// ============================================================

#[async_trait::async_trait]
pub trait PlannerTrait: Send + Sync {
    async fn plan(&self, messages: Vec<Message>, cancel: CancellationToken) -> Result<PlanAction>;
}

#[async_trait::async_trait]
impl<M> PlannerTrait for Planner<M>
where
    M: Model + Send + Sync,
{
    async fn plan(&self, messages: Vec<Message>, cancel: CancellationToken) -> Result<PlanAction> {
        tokio::select! {
            _ = cancel.cancelled() => {
                anyhow::bail!(
                    "Generation interrupted."
                )
            }

            result = self.create_plan(messages) => {
                result
            }
        }
    }
}
