use crate::{
    router::{RoutedAction, ToolRouter},
    workspace::language::{self, ProgrammingLanguage},
};
use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;

use crate::{
    context::{Context, Message, MessageRole},
    event::AgentEvent,
    history::{History, HistoryMessage},
    model::{CompletionRequest, Model},
    planner::{PlanAction, Planner},
    tools::ToolRegistry,
};

#[derive(Default)]
struct InspectionState {
    listed: bool,
    config: bool,
    readme: bool,
    source: bool,
    galaxy: bool,
}

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

    fn needs_workspace(&self, input: &str) -> bool {
        let input = input.to_lowercase();

        let keywords = [
            // General
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
            // Rust
            "rust",
            "cargo",
            "crate",
            "rustc",
            // C / C++
            "c++",
            "cpp",
            "cmake",
            "makefile",
            "clang",
            "gcc",
            // Python
            "python",
            "pip",
            "django",
            "flask",
            "fastapi",
            // JavaScript / TypeScript
            "javascript",
            "typescript",
            "node",
            "npm",
            "pnpm",
            "yarn",
            "react",
            "vue",
            "svelte",
            // Java / Kotlin
            "java",
            "kotlin",
            "gradle",
            "maven",
            // Go
            "go",
            "golang",
            "go.mod",
            // Swift
            "swift",
            "xcode",
            // Dart
            "dart",
            "flutter",
            // .NET
            "c#",
            "csharp",
            "dotnet",
        ];

        keywords.iter().any(|word| input.contains(word))
    }

    async fn answer(&self, tx: &Sender<AgentEvent>, cancel: &CancellationToken) -> Result<String> {
        let mut messages = vec![Message {
            role: MessageRole::System,
            content: r#"
You are Luma.

You are a local-first AI coding agent.

Identity:
- Your name is Luma.
- Never claim to be ChatGPT.
- Never mention OpenAI.
- Never mention training data or knowledge cutoffs.

Personality:
- Be concise, technical, and helpful.
- Prefer practical solutions.
- Explain debugging reasoning when useful.
- Ask questions only when absolutely necessary.
- Prefer taking action over explaining what could be done.

Programming abilities:

You are a multi-language programming assistant.

You understand:

Systems:
- Rust
- C
- C++
- Zig
- Go

Web:
- JavaScript
- TypeScript
- HTML
- CSS
- React
- Vue
- Svelte

Backend:
- Python
- Java
- Kotlin
- C#
- Ruby
- PHP

Mobile:
- Swift
- SwiftUI
- Kotlin Android
- Dart / Flutter

Data:
- SQL
- R
- Julia

Rules:
- Detect the programming language from the workspace.
- Never assume Rust.
- Use the project's existing conventions.
- Use the correct package manager and build system.
- Do not invent dependencies.
- Do not invent APIs.

Workspace rules:
- Workspace information comes only from tool observations.
- Never invent files, technologies, dependencies, or architecture.
- Never guess what code does without reading it.

When the user asks to initialize a workspace:
- You are an execution agent, not a chat assistant.
- Prefer tools over explanations.
- Do not describe inspected files unless asked.
- Complete the task first, then give a short completion message.

If information is missing:
- Say that the workspace information is insufficient.
- Do not fabricate an answer.

Tool rules:
- Inspect before modifying.
- Read files before writing.
- Never call write_file without complete content.
- Always include the target file path.
- Verify changes when possible.

Response style:
- Use Markdown when useful.
- Use code blocks for code.
- Explain important tradeoffs.
- Keep responses concise.
- Do not repeat the user's request.
- Do not add unnecessary introductions.

You are Luma.
"#
            .to_string(),
        }];

        messages.extend(self.context.messages().iter().cloned());

        // ------------------------------------------------------------
        // Start model request
        // ------------------------------------------------------------

        let mut stream = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(String::new());
            }

            result = self.model.stream(
                CompletionRequest { messages }
            ) => {
                result?
            }
        };

        // ------------------------------------------------------------
        // Stream response
        // ------------------------------------------------------------

        let mut response = String::new();

        loop {
            tokio::select! {
                // Cancellation is checked while the model is streaming.
                _ = cancel.cancelled() => {
                    let _ = tx.send(
                        AgentEvent::Error(
                            "Generation interrupted.".into()
                        )
                    ).await;

                    return Ok(response);
                }

                chunk = stream.next() => {
                    let Some(chunk) = chunk else {
                        break;
                    };

                    let chunk = chunk?;

                    if chunk.is_empty() {
                        continue;
                    }

                    response.push_str(&chunk);

                    // The TUI may have gone away.
                    //
                    // This should NOT kill the agent task.
                    if tx.send(
                        AgentEvent::TextDelta(chunk)
                    ).await.is_err() {
                        return Ok(response);
                    }
                }
            }
        }

        Ok(response)
    }

    pub async fn run(
        &mut self,
        mut rx: Receiver<String>,
        tx: Sender<AgentEvent>,
        session_cancel: CancellationToken,
    ) -> Result<()> {
        while let Some(input) = tokio::select! {
            _ = session_cancel.cancelled() => {
                None
            }

            input = rx.recv() => {
                input
            }
        } {
            if input.trim().is_empty() {
                continue;
            }

            // Each request gets its own cancellation token.
            let cancel = session_cancel.child_token();

            if input.trim() == "/init" {
                match self.initialize_workspace(&tx, &cancel).await {
                    Ok(()) => {
                        let _ = tx
                            .send(AgentEvent::SystemMessage("Workspace initialized.".into()))
                            .await;

                        let _ = tx.send(AgentEvent::Finished).await;
                    }

                    Err(error) => {
                        let _ = tx
                            .send(AgentEvent::Error(format!(
                                "Workspace initialization failed: {}",
                                error
                            )))
                            .await;
                    }
                }

                continue;
            }

            if cancel.is_cancelled() {
                continue;
            }

            self.context.add(MessageRole::User, input.clone());

            // --------------------------------------------------------
            // Initial workspace listing
            // --------------------------------------------------------

            if !self.inspection.listed {
                if let Err(error) = self.execute_tool("list_directory", ".", &tx, &cancel).await {
                    let _ = tx
                        .send(AgentEvent::Error(format!(
                            "Workspace inspection failed: {}",
                            error
                        )))
                        .await;

                    continue;
                }
            }

            // --------------------------------------------------------
            // History
            // --------------------------------------------------------

            self.history.messages.push(HistoryMessage {
                role: "user".into(),
                content: input.clone(),
            });

            if let Err(error) = self.history.save() {
                let _ = tx
                    .send(AgentEvent::Error(format!(
                        "Failed to save history: {}",
                        error
                    )))
                    .await;
            }

            // --------------------------------------------------------
            // Deterministic tool routing
            // --------------------------------------------------------

            match ToolRouter::route(&input) {
                RoutedAction::Tool { name, input } => {
                    let _ = tx
                        .send(AgentEvent::Debug(format!("Router selected {}", name)))
                        .await;

                    if let Err(error) = self.execute_tool(&name, &input, &tx, &cancel).await {
                        let _ = tx
                            .send(AgentEvent::Error(format!("Tool failed: {}", error)))
                            .await;
                    }

                    let _ = tx.send(AgentEvent::Finished).await;

                    continue;
                }

                RoutedAction::Planner => {}
            }

            // --------------------------------------------------------
            // Normal chat
            // --------------------------------------------------------

            let workspace_request = self.needs_workspace(&input);

            if !workspace_request {
                match self.answer(&tx, &cancel).await {
                    Ok(response) => {
                        if !response.is_empty() {
                            self.context.add(MessageRole::Assistant, response.clone());

                            self.history.messages.push(HistoryMessage {
                                role: "assistant".into(),
                                content: response,
                            });

                            if let Err(error) = self.history.save() {
                                let _ = tx
                                    .send(AgentEvent::Error(format!(
                                        "Failed to save history: {}",
                                        error
                                    )))
                                    .await;
                            }
                        }
                    }

                    Err(error) => {
                        if cancel.is_cancelled() {
                            let _ = tx
                                .send(AgentEvent::Error("Generation interrupted.".into()))
                                .await;
                        } else {
                            let _ = tx
                                .send(AgentEvent::Error(format!("Model failed: {}", error)))
                                .await;
                        }
                    }
                }

                let _ = tx.send(AgentEvent::Finished).await;

                continue;
            }

            // --------------------------------------------------------
            // Planner / workspace agent
            // --------------------------------------------------------

            let _ = tx.send(AgentEvent::Thinking).await;

            let mut steps = 0;

            loop {
                if cancel.is_cancelled() {
                    let _ = tx
                        .send(AgentEvent::Error("Generation interrupted.".into()))
                        .await;

                    break;
                }

                steps += 1;

                // Safety limit
                if steps > 12 {
                    match self.answer(&tx, &cancel).await {
                        Ok(response) => {
                            if !response.is_empty() {
                                self.context.add(MessageRole::Assistant, response.clone());

                                self.history.messages.push(HistoryMessage {
                                    role: "assistant".into(),
                                    content: response,
                                });

                                let _ = self.history.save();
                            }
                        }

                        Err(error) => {
                            let _ = tx
                                .send(AgentEvent::Error(format!("Model failed: {}", error)))
                                .await;
                        }
                    }

                    break;
                }

                // ----------------------------------------------------
                // Build planner context
                // ----------------------------------------------------

                let mut messages = vec![Message {
                    role: MessageRole::System,
                    content: Self::planner_system_prompt(
                        &self.language,
                        &self.inspection,
                        &self.inspected_files,
                    ),
                }];

                messages.extend(self.context.messages().iter().cloned());

                // ----------------------------------------------------
                // Planner
                // ----------------------------------------------------

                let plan = tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = tx
                            .send(AgentEvent::Error(
                                "Generation interrupted.".into(),
                            ))
                            .await;

                        break;
                    }

                    result = self.planner.plan(
                        messages,
                        cancel.clone(),
                    ) => {
                        result
                    }
                };

                let plan = match plan {
                    Ok(plan) => plan,

                    Err(error) => {
                        if cancel.is_cancelled() {
                            let _ = tx
                                .send(AgentEvent::Error("Generation interrupted.".into()))
                                .await;
                        } else {
                            let _ = tx
                                .send(AgentEvent::Error(format!("Planner failed: {}", error)))
                                .await;
                        }

                        // IMPORTANT:
                        //
                        // Do NOT return Err(error).
                        //
                        // This request failed, but Luma stays alive.
                        break;
                    }
                };

                // ----------------------------------------------------
                // Execute plan
                // ----------------------------------------------------

                match plan {
                    PlanAction::Tool { name, input } => {
                        if let Err(error) = self.execute_tool(&name, &input, &tx, &cancel).await {
                            let _ = tx
                                .send(AgentEvent::Error(format!("{} failed: {}", name, error)))
                                .await;

                            break;
                        }
                    }

                    PlanAction::Multi { actions } => {
                        for action in actions {
                            if cancel.is_cancelled() {
                                break;
                            }

                            if let PlanAction::Tool { name, input } = action {
                                if let Err(error) =
                                    self.execute_tool(&name, &input, &tx, &cancel).await
                                {
                                    let _ = tx
                                        .send(AgentEvent::Error(format!(
                                            "{} failed: {}",
                                            name, error
                                        )))
                                        .await;

                                    break;
                                }
                            }
                        }
                    }

                    PlanAction::Answer { .. } => {
                        match self.answer(&tx, &cancel).await {
                            Ok(response) => {
                                if !response.is_empty() {
                                    self.context.add(MessageRole::Assistant, response.clone());

                                    self.history.messages.push(HistoryMessage {
                                        role: "assistant".into(),
                                        content: response,
                                    });

                                    let _ = self.history.save();
                                }
                            }

                            Err(error) => {
                                if cancel.is_cancelled() {
                                    let _ = tx
                                        .send(AgentEvent::Error("Generation interrupted.".into()))
                                        .await;
                                } else {
                                    let _ = tx
                                        .send(AgentEvent::Error(format!("Model failed: {}", error)))
                                        .await;
                                }
                            }
                        }

                        break;
                    }
                }
            }

            let _ = tx.send(AgentEvent::Finished).await;
        }

        Ok(())
    }

    fn update_inspection(&mut self, name: &str, input: &str) {
        if name == "list_directory" {
            self.inspection.listed = true;

            return;
        }

        if name != "read_file" {
            return;
        }

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

    fn planner_system_prompt(
        language: &ProgrammingLanguage,
        inspection: &InspectionState,
        inspected_files: &[String],
    ) -> String {
        format!(
            r#"
You are Luma's Planner.

You are not a chatbot.
You are the decision-making system of a local-first AI coding agent.

Your job is to decide the next action:
- use a tool
- use multiple tools
- answer the user

You control an agent that can inspect and modify a real workspace.

==================================================
CORE IDENTITY
==================================================

You are operating inside a user's project.

The filesystem is unknown.

You MUST NOT:
- invent files
- invent folders
- invent project structure
- assume technologies
- claim something exists without observation

The only truth about the workspace comes from tool results.

If you need information, gather it.

==================================================
GENERAL AGENT BEHAVIOR
==================================================

Think like a senior software engineer.

Workflow:

1. Understand the user's intent.
2. Determine what information is required.
3. Inspect the workspace if needed.
4. Read relevant files.
5. Modify only when enough information exists.
6. Verify changes when possible.

Never skip inspection when the task depends on the project.

==================================================
TOOL USAGE RULES
==================================================

You have tools.

Tools are not optional suggestions.

Use them whenever they provide missing information.

--------------------------------------------------
list_directory
--------------------------------------------------

Purpose:
Understand the workspace structure.

Use when the user asks:

- "what files are here?"
- "list files"
- "list directories"
- "show folders"
- "show project structure"
- "explore the project"
- "where is X?"
- "what does this project contain?"

Do not answer these questions from memory.

Call:

list_directory(".")

first unless a specific directory is requested.

Do NOT repeatedly call list_directory when the workspace has
already been inspected and the existing observation is sufficient.

--------------------------------------------------
read_file
--------------------------------------------------

Purpose:
Understand file contents.

Use when:

- analyzing code
- debugging
- explaining implementation
- modifying existing files
- understanding configuration
- verifying a change

Never modify an existing file you have not read.

Prefer reading only the relevant file or relevant section.

--------------------------------------------------
search_files
--------------------------------------------------

Purpose:
Find code or text.

Use when:

- looking for symbols
- finding usages
- locating errors
- finding TODOs
- finding configuration
- locating a file whose path is unknown

Prefer search_files over repeatedly listing directories when
you are looking for a specific symbol, filename, or piece of text.

--------------------------------------------------
patch_file
--------------------------------------------------

Purpose:
Make a precise modification to an existing file.

Use patch_file for modifications to existing files.

Before using patch_file:

- The target file MUST have been read.
- You MUST know the exact file path.
- You MUST know the exact existing text to replace.
- The "old" text must come from the actual file contents.
- The "new" text must be complete and intentional.

patch_file performs an exact replacement.

Rules:

- Never invent the "old" text.
- Never use placeholders.
- Never use approximate text.
- Never patch a file that has not been inspected.
- Never assume whitespace or formatting.
- If "old" matches zero times, read the file again.
- If "old" matches more than once, make the match more specific.
- Never blindly retry the same failed patch.
- Prefer small, focused patches.
- After patching, verify the result when practical.

Example:

patch_file(
    path="src/main.rs",
    old="let value = old_value;",
    new="let value = new_value;",
)

For multiple independent changes, prefer multiple precise patches
rather than replacing an entire file.

--------------------------------------------------
write_file
--------------------------------------------------

Purpose:
Create a new file or replace an entire file.

Prefer patch_file when modifying an existing file.

Use write_file primarily when:

- creating a new file
- generating a completely new file
- replacing an entire file is explicitly necessary

Before using write_file:

You MUST know:

- exact path
- complete content

Never send:

- partial files
- patches
- explanations
- placeholders

The content must be a complete valid file.

Do NOT use write_file simply because it is easier than constructing
a precise patch.

--------------------------------------------------
run_command
--------------------------------------------------

Purpose:
Execute commands.

Use for:

- building
- testing
- formatting
- linting
- running applications
- verifying changes

Never assume command output.

After making important code changes, prefer running the project's
appropriate verification command when practical.

Examples:

Rust:
cargo check
cargo test
cargo fmt -- --check
cargo clippy

Python:
pytest
python -m compileall

JavaScript / TypeScript:
npm test
npm run build
npx tsc --noEmit

Go:
go test ./...
go vet ./...

Use the project's existing package manager and conventions.

==================================================
MODIFICATION WORKFLOW
==================================================

When modifying existing code, follow:

1. Locate the target file.
2. Read the target file.
3. Understand the surrounding implementation.
4. Choose the smallest safe modification.
5. Use patch_file.
6. Inspect or verify the resulting change.
7. Run an appropriate check when possible.
8. Report the result.

Do NOT jump directly from:

"user wants a change"

to:

"write_file"

unless creating a new file.

The preferred flow is:

read_file
    ↓
patch_file
    ↓
read_file / run_command
    ↓
answer

==================================================
PROJECT DETECTION
==================================================

Supported ecosystems:

Rust:
- Cargo.toml
- src/*.rs

Python:
- pyproject.toml
- requirements.txt
- *.py

JavaScript / TypeScript:
- package.json
- tsconfig.json
- *.js
- *.ts
- *.tsx

C/C++:
- CMakeLists.txt
- Makefile
- *.c
- *.cpp
- *.h

Go:
- go.mod
- *.go

Java/Kotlin:
- pom.xml
- build.gradle
- *.java
- *.kt

Swift:
- Package.swift
- *.swift

Flutter:
- pubspec.yaml
- *.dart

Detected language:
{}

==================================================
DECISION RULES
==================================================

If the user asks a question:

- Answer directly if no workspace information is needed.

If the user asks about the workspace:

- Use tools.

If the user requests a change:

- Inspect first.
- Then modify.

If the user reports an error:

- Read relevant files.
- Search for related code.
- Do not guess.

If the target file is already known but its contents are not:

- read_file first.

If the target file has already been read:

- Do not unnecessarily list the workspace again.
- Use the existing observation.

If a precise modification is required:

- Prefer patch_file.

If creating a new file:

- Use write_file.

If an existing file must be completely regenerated:

- Use write_file only when necessary.

If unsure:

- Gather information.

==================================================
NEVER DO THIS
==================================================

Wrong:

User:
"List project files"

Assistant:
"The project probably contains src and Cargo.toml."

Correct:

Tool:
list_directory(".")

---

Wrong:

User:
"Fix this Rust file"

Assistant:
"I would change the borrow checker issue by..."

Correct:

read_file("file.rs")
then decide.

---

Wrong:

User:
"Change this function"

Assistant:
write_file("file.rs", entire guessed file)

Correct:

read_file("file.rs")
then:

patch_file(
    path="file.rs",
    old="exact existing code",
    new="exact replacement",
)

---

Wrong:

patch_file(
    path="src/agent.rs",
    old="probably this code",
    new="..."
)

Correct:

Read the file first and use text that actually exists.

---

Wrong:

A patch fails.

Assistant:
retry the same patch repeatedly.

Correct:

Read the file again, determine why the match failed,
construct a more precise patch, then retry.

---

Wrong:

User:
"Create a config"

Assistant:
"Here is a config example."

Correct:

Use write_file if a real file is requested.

==================================================
CURRENT INSPECTION STATE
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

Previously inspected files:
{:?}

==================================================
WORKSPACE MEMORY
==================================================

A GALAXY.md file may exist.

Rules:

- Read GALAXY.md before exploring the project when it exists.
- Treat it as project memory.
- Do not invent information that is not present in GALAXY.md.
- Update it after major architecture changes when appropriate.

==================================================
EFFICIENCY
==================================================

Do not use tools merely because they are available.

Every tool call should answer a question or perform a necessary action.

Avoid redundant calls.

In particular:

- Do not repeatedly call list_directory.
- Do not reread unchanged files without a reason.
- Do not use search_files when the exact file is already known.
- Do not use write_file when patch_file is sufficient.
- Do not run expensive commands unless they provide useful verification.

Prefer the smallest number of tool calls that produces a reliable result.

==================================================
FINAL RULE
==================================================

You are not here to sound helpful.

You are here to operate an engineering agent.

When information is missing:
collect it.

When a tool can answer:
use it.

When a file must change:

inspect
→ patch
→ verify

Prefer precise, minimal, reversible changes.

Never guess when the filesystem can tell you the answer.

"#,
            language.name(),
            language.name(),
            inspection.listed,
            inspection.config,
            inspection.readme,
            inspection.source,
            inspected_files
        )
    }

    async fn execute_tool(
        &mut self,
        name: &str,
        input: &str,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        if cancel.is_cancelled() {
            return Ok(());
        }

        let start = std::time::Instant::now();

        let display_input = match name {
            "read_file" => {
                format!("read_file → {}", input)
            }

            "write_file" => match serde_json::from_str::<serde_json::Value>(input) {
                Ok(json) => {
                    let path = json.get("path").and_then(|v| v.as_str()).unwrap_or("?");

                    format!("write_file → {}", path)
                }

                Err(_) => "write_file → invalid JSON".into(),
            },

            _ => {
                format!("{} {}", name, input)
            }
        };

        tx.send(AgentEvent::ToolStarted {
            name: name.to_string(),
            input: display_input,
        })
        .await?;

        if cancel.is_cancelled() {
            return Ok(());
        }

        let tool_input = if name == "write_file" {
            input.trim().to_string()
        } else {
            input.to_string()
        };

        if name == "write_file" {
            println!("WRITE FILE INPUT:\n{}", tool_input);
        }

        let result = match self.tools.execute(name, &tool_input) {
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

        self.update_inspection(name, &tool_input);

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

    async fn initialize_workspace(
        &mut self,
        tx: &Sender<AgentEvent>,
        cancel: &CancellationToken,
    ) -> Result<()> {
        // Inspect workspace first
        self.execute_tool("list_directory", ".", tx, cancel).await?;

        let mut project = "Unknown project".to_string();
        let mut language = "Unknown".to_string();

        let mut files = Vec::new();

        if std::path::Path::new("Cargo.toml").exists() {
            language = "Rust".into();
            files.push("Cargo.toml");

            if let Ok(content) = std::fs::read_to_string("Cargo.toml") {
                for line in content.lines() {
                    if line.starts_with("name =") {
                        project = line
                            .replace("name =", "")
                            .replace('"', "")
                            .trim()
                            .to_string();

                        break;
                    }
                }
            }
        }

        if std::path::Path::new("package.json").exists() {
            language = "JavaScript / TypeScript".into();
            files.push("package.json");
        }

        if std::path::Path::new("pyproject.toml").exists()
            || std::path::Path::new("requirements.txt").exists()
        {
            language = "Python".into();

            if std::path::Path::new("pyproject.toml").exists() {
                files.push("pyproject.toml");
            }

            if std::path::Path::new("requirements.txt").exists() {
                files.push("requirements.txt");
            }
        }

        let structure = std::fs::read_dir(".")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "Unable to inspect.".into());

        let galaxy = format!(
            r##"# GALAXY.md

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
"##,
            project,
            language,
            if files.is_empty() {
                "No important files detected.".into()
            } else {
                files.join("\n")
            },
            structure,
        );

        self.execute_tool(
            "write_file",
            &serde_json::json!({
                "path": "GALAXY.md",
                "content": galaxy
            })
            .to_string(),
            tx,
            cancel,
        )
        .await?;

        // Load new memory immediately
        self.context.add(
            MessageRole::System,
            format!("Updated GALAXY.md workspace memory:\n\n{}", galaxy),
        );

        Ok(())
    }
}

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
                anyhow::bail!("Generation interrupted.")
            }

            result = self.create_plan(messages) => {
                result
            }
        }
    }
}
