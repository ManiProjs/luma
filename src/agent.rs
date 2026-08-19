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
    pub fn new(model: M, planner: P, tools: ToolRegistry, history: History) -> Self {
        Self {
            model,

            planner,

            context: Context::new(),

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
- Explain debugging reasoning.
- Ask questions only when necessary.

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
- Use the correct package manager/build system.
- Do not invent dependencies.
- Do not invent APIs.

Workspace rules:
- Workspace information comes only from tool observations.
- Never invent files, technologies, dependencies, or architecture.
- Never guess what code does without reading it.

If information is missing:
"Not enough workspace information."

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
- Keep conversation natural.

You are Luma.
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

    pub async fn run(
        &mut self,
        mut rx: Receiver<String>,
        tx: Sender<AgentEvent>,
        cancel: CancellationToken,
    ) -> Result<()> {
        while let Some(input) = rx.recv().await {
            if cancel.is_cancelled() {
                continue;
            }

            // Reset workspace inspection for each request
            self.inspection = InspectionState::default();
            self.inspected_files.clear();

            self.context.add(MessageRole::User, input.clone());

            if !self.inspection.listed {
                self.execute_tool("list_directory", ".", &tx, &cancel)
                    .await?;
            }

            self.history.messages.push(HistoryMessage {
                role: "user".into(),
                content: input.clone(),
            });

            self.history.save()?;

            /*
                Deterministic tool routing

                Handles obvious actions without asking the model.
                Example:
                "list directories"
                "show files"
                "show project structure"
            */

            match ToolRouter::route(&input) {
                RoutedAction::Tool { name, input } => {
                    tx.send(AgentEvent::Debug(format!("Router selected {}", name)))
                        .await?;

                    self.execute_tool(&name, &input, &tx, &cancel).await?;

                    continue;
                }

                RoutedAction::Planner => {}
            }

            let workspace_request = self.needs_workspace(&input);

            // Normal chat
            if !workspace_request {
                let response = self.answer(&tx, &cancel).await?;

                self.context.add(MessageRole::Assistant, response.clone());

                self.history.messages.push(HistoryMessage {
                    role: "assistant".into(),
                    content: response,
                });

                self.history.save()?;

                tx.send(AgentEvent::Finished).await?;

                continue;
            }

            tx.send(AgentEvent::Thinking).await?;

            let mut steps = 0;

            loop {
                if cancel.is_cancelled() {
                    tx.send(AgentEvent::Error("Interrupted.".into())).await?;

                    break;
                }

                steps += 1;

                // Safety limit
                if steps > 12 {
                    let response = self.answer(&tx, &cancel).await?;

                    self.context.add(MessageRole::Assistant, response.clone());

                    self.history.messages.push(HistoryMessage {
                        role: "assistant".into(),
                        content: response,
                    });

                    self.history.save()?;

                    break;
                }

                let mut messages = vec![Message {
                    role: MessageRole::System,

                    content: Self::planner_system_prompt(
                        &self.language,
                        &self.inspection,
                        &self.inspected_files,
                    ),
                }];

                messages.extend(self.context.messages().iter().cloned());

                let plan = self.planner.plan(messages).await?;

                match plan {
                    PlanAction::Tool { name, input } => {
                        self.execute_tool(&name, &input, &tx, &cancel).await?;
                    }

                    PlanAction::Multi { actions } => {
                        for action in actions {
                            if let PlanAction::Tool { name, input } = action {
                                self.execute_tool(&name, &input, &tx, &cancel).await?;
                            }
                        }
                    }

                    PlanAction::Answer { .. } => {
                        let response = self.answer(&tx, &cancel).await?;

                        self.context.add(MessageRole::Assistant, response.clone());

                        self.history.messages.push(HistoryMessage {
                            role: "assistant".into(),
                            content: response,
                        });

                        self.history.save()?;

                        break;
                    }
                }
            }

            tx.send(AgentEvent::Finished).await?;
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

Use immediately when the user asks:

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

Never modify a file you have not read.

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

--------------------------------------------------
write_file
--------------------------------------------------

Purpose:
Create or replace files.

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

--------------------------------------------------
run_command
--------------------------------------------------

Purpose:
Execute commands.

Use for:

- building
- testing
- formatting
- running applications

Never assume command output.

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
FINAL RULE
==================================================

You are not here to sound helpful.

You are here to operate an engineering agent.

When information is missing:
collect it.

When a tool can answer:
use it.

When a file must change:
inspect, modify, verify.

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
}

#[async_trait::async_trait]
pub trait PlannerTrait: Send + Sync {
    async fn plan(&self, messages: Vec<Message>) -> Result<PlanAction>;
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
