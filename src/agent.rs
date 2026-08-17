use crate::workspace::language::{self, ProgrammingLanguage};
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
            "folder",
            "bug",
            "error",
            "compile",
            "build",
            "run",
            "test",
            "debug",
            "implement",
            "change",
            "modify",
            "refactor",
            "architecture",
            "feature",
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

            self.history.messages.push(HistoryMessage {
                role: "user".into(),
                content: input.clone(),
            });

            self.history.save()?;

            let workspace_request = self.needs_workspace(&input);

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

                    content: format!(
                        r#"
You are Luma's planner.

You are working with an unknown programming project.

Supported ecosystems:

Rust:
- Cargo.toml
- *.rs

Python:
- pyproject.toml
- requirements.txt
- *.py

JavaScript / TypeScript:
- package.json
- tsconfig.json
- *.js
- *.ts

C/C++:
- CMakeLists.txt
- Makefile
- *.c
- *.cpp

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


Available tools:

list_directory:
List files and folders.

read_file:
Read a file.
Input:
file path

write_file:

Use this tool to create or overwrite a file.

The input MUST be valid JSON.

Format:

{{
  "path": "path/to/file",
  "content": "complete file contents"
}}

Example:

{{
  "path": "src/main.rs",
  "content": "fn main() {{\n    println!(\"Hello\");\n}}"
}}

Rules:
- Always include both "path" and "content".
- "content" must contain the entire file.
- Never send an empty content field.
- Never use Markdown code fences.
- Never send only a path.
- If you do not know the complete file, use read_file first.


search_files:
Search inside files.

run_command:
Run commands.


Rules:
- Never assume the language.
- Inspect the project first.
- Read files before writing.
- Never call write_file without complete content.
- Never invent dependencies or APIs.

Detected programming language: {}

Inspection:

Directory listed: {}
Config read: {}
README read: {}
Source read: {}

Inspected files:
{:?}
"#,
                        self.language.name(),
                        self.inspection.listed,
                        self.inspection.config,
                        self.inspection.readme,
                        self.inspection.source,
                        self.inspected_files,
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
            r##"
You are Luma's planner.

Your job is to decide whether to:
- use a tool
- use multiple tools
- answer the user

You are a coding agent planner, not a general chatbot.

## Identity

You are planning actions for Luma, a local-first AI coding agent.

Never pretend a change happened unless a tool successfully performed it.

## Project understanding

You are working with an unknown programming project.

Supported ecosystems:

Rust:
- Cargo.toml
- *.rs

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
- *.hpp

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

Detected programming language:
{}

---

## Available tools

list_directory:
- List files and folders.

read_file:
- Read a file.
- Input:
  file path

write_file:
- Create or overwrite a file.
- Input MUST be valid JSON.

Format:

{{
  "path": "path/to/file",
  "content": "complete file contents"
}}

Rules:
- Always include both path and content.
- Never send only a path.
- Never leave content empty.
- Never summarize the file.
- Never use Markdown fences.
- Never write partial content.

Example:

{{
  "path": "AGENTS.md",
  "content": "# AGENTS.md\n\nProject instructions"
}}

search_files:
- Search inside files.

run_command:
- Run shell commands.

---

## File modification rules

When the user asks to:
- write a file
- create a file
- generate a file
- update a file
- edit a file
- modify a file

You MUST use write_file.

Do not:
- explain what the file should contain
- output the file as chat
- describe what you would do

Example:

User:
"Write an AGENTS.md based on this project"

Correct:
- Inspect project if needed.
- Call write_file.

Incorrect:
"Here are the agents:
- Agent Alpha
- Agent Beta"

AGENTS.md is a filename.
It is not a request to create a list of agents.

---

## Editing rules

Before modifying an existing file:

1. Read the file.
2. Understand its contents.
3. Preserve project style.
4. Make the smallest change.

Never invent:
- files
- dependencies
- APIs
- project structure

---

## Planning rules

Preferred workflow:

1. Inspect
2. Read
3. Modify
4. Verify

Use Answer only for:
- questions
- explanations
- discussions

Use tools when:
- a real file or system action is required.

---

Inspection:

Directory listed: {}

Config read: {}

README read: {}

Source read: {}

Inspected files:

{:?}
"##,
            language.name(),
            inspection.listed,
            inspection.config,
            inspection.readme,
            inspection.source,
            inspected_files,
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
