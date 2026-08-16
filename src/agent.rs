use anyhow::Result;
use futures_util::StreamExt;

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

    async fn answer(&self, tx: &tokio::sync::mpsc::Sender<AgentEvent>) -> Result<String> {
        let mut messages = vec![Message {
            role: MessageRole::System,
            content: r#"
You are Luma.

You summarize a codebase using ONLY workspace observations.

Rules:

- Never guess.
- Never invent features.
- Never invent technologies.
- Never infer a project type from filenames.
- Every claim must come from workspace observations.

Explain:

1. Project purpose
2. Technologies
3. Structure
4. Features

If information is missing:
say:
"Not enough workspace information."

Return plain text only.
"#
            .to_string(),
        }];

        messages.extend(self.context.messages().iter().cloned());

        messages.push(Message {
            role: MessageRole::User,
            content: r#"
Using the workspace observations above, answer the original user question.

Do not ask for files.
Do not say you cannot access the project.
Do not invent information.
"#
            .to_string(),
        });

        let mut stream = self.model.stream(CompletionRequest { messages }).await?;

        let mut response = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;

            response.push_str(&chunk);

            tx.send(AgentEvent::TextDelta(chunk)).await?;
        }

        Ok(response)
    }

    pub async fn run(
        &mut self,
        input: String,
        tx: tokio::sync::mpsc::Sender<AgentEvent>,
    ) -> Result<()> {
        self.context.add(MessageRole::User, input);

        let mut steps = 0;

        tx.send(AgentEvent::Thinking).await?;

        loop {
            steps += 1;

            if steps > 12 {
                let response = self.answer(&tx).await?;

                self.context.add(MessageRole::Assistant, response);

                break;
            }

            let mut messages = vec![Message {
                role: MessageRole::System,
                content: format!(
                    r#"
You are Luma's planner.

Current inspection state:

Directory listed: {}
Project config read: {}
README read: {}
Source code read: {}

Rules:

- If config is missing, inspect configuration.
- If source is missing, inspect source.
- Do not answer until:
  - configuration exists
  - at least one source file was inspected

Current inspected files:
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
                    self.execute_tool(&name, &input, &tx).await?;
                }

                PlanAction::Multi { actions } => {
                    for action in actions {
                        if let PlanAction::Tool { name, input } = action {
                            self.execute_tool(&name, &input, &tx).await?;
                        }
                    }
                }

                PlanAction::Answer { .. } => {
                    // Safety gate.
                    // Do not trust the planner.

                    if !self.inspection.config {
                        self.execute_tool("read_file", "Cargo.toml", &tx).await?;

                        continue;
                    }

                    if !self.inspection.source {
                        self.execute_tool("read_file", "src/main.rs", &tx).await?;

                        continue;
                    }

                    let response = self.answer(&tx).await?;

                    self.context.add(MessageRole::Assistant, response);

                    break;
                }
            }
        }

        tx.send(AgentEvent::Finished).await?;

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
        tx: &tokio::sync::mpsc::Sender<AgentEvent>,
    ) -> Result<()> {
        let key = format!("{}:{}", name, input);

        if self.inspected_files.contains(&key) {
            return Ok(());
        }

        self.inspected_files.push(key);

        tx.send(AgentEvent::ToolStarted {
            name: name.to_string(),
        })
        .await?;

        let result = self.tools.execute(name, input)?;

        self.update_inspection(name, input);

        tx.send(AgentEvent::ToolFinished {
            name: name.to_string(),
            result: result.clone(),
        })
        .await?;

        self.context.add(
            MessageRole::User,
            format!(
                r#"
WORKSPACE OBSERVATION

Tool:
{}

Result:
{}

This is verified workspace information.
"#,
                name, result
            ),
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
