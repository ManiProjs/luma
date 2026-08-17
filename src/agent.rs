use anyhow::Result;
use futures_util::StreamExt;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;

use crate::{
    context::{Context, Message, MessageRole},
    event::AgentEvent,
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
}

impl<M, P> Agent<M, P>
where
    M: Model,
    P: PlannerTrait,
{
    pub fn new(model: M, planner: P, tools: ToolRegistry) -> Self {
        Self {
            model,

            planner,

            context: Context::new(),

            tools,

            inspected_files: Vec::new(),

            inspection: InspectionState::default(),
        }
    }

    fn needs_workspace(&self, input: &str) -> bool {
        let input = input.to_lowercase();

        let keywords = [
            "project",
            "repository",
            "repo",
            "code",
            "file",
            "folder",
            "bug",
            "error",
            "compile",
            "cargo",
            "rust",
            "function",
            "struct",
            "architecture",
            "feature",
            "explain",
            "how does",
            "what does",
            "read",
            "analyze",
            "debug",
            "implement",
            "change",
            "modify",
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

Coding behavior:
- Help users understand, debug, and improve software.
- Prefer safe changes.
- Explain changes.
- Never pretend you modified files.

Workspace rules:
- Workspace information comes only from observations.
- Never invent files, technologies, dependencies, or architecture.
- Never guess without evidence.

If information is missing:
"Not enough workspace information."

Response style:
- Use Markdown when useful.
- Use code blocks for code.
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
                            "Generation interrupted."
                                .into()
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

            self.context.add(MessageRole::User, input.clone());

            let workspace_request = self.needs_workspace(&input);

            if !workspace_request {
                let response = self.answer(&tx, &cancel).await?;

                self.context.add(MessageRole::Assistant, response);

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

                    self.context.add(MessageRole::Assistant, response);

                    break;
                }

                let mut messages = vec![Message {
                    role: MessageRole::System,

                    content: format!(
                        r#"
You are Luma's planner.

Available tools:

read_file:
- Reads a file.
- Input: file path.

write_file:
- Writes a complete file.
- Input format:
  First line: file path
  Remaining lines: complete file contents.

Rules:
- Always use read_file before write_file.
- Never call write_file without complete content.
- Never send only a file path to write_file.

Inspection state:

Directory listed: {}
Config read: {}
README read: {}
Source read: {}

Inspected files:
{:?}
"#,
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
                        if !self.inspection.config {
                            self.execute_tool("read_file", "Cargo.toml", &tx, &cancel)
                                .await?;

                            continue;
                        }

                        if !self.inspection.source {
                            self.execute_tool("read_file", "src/main.rs", &tx, &cancel)
                                .await?;

                            continue;
                        }

                        let response = self.answer(&tx, &cancel).await?;

                        self.context.add(MessageRole::Assistant, response);

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

        let display_input = if name == "read_file" || name == "write_file" {
            format!("{} → {}", name, input)
        } else {
            format!("{} {}", name, input)
        };

        tx.send(AgentEvent::ToolStarted {
            name: name.to_string(),
            input: display_input,
        })
        .await?;

        let result = self.tools.execute(name, input)?;

        self.update_inspection(name, input);

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
