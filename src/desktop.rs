//! JSONL transport for the unified protocol.
//!
//! Reads [`ClientMessage`]s from stdin, writes [`ServerEvent`]s to stdout,
//! one JSON object per line. The desktop shell spawns `luma desktop-server`
//! and speaks this.
//!
//! This module is transport only. Every decision about what a message means
//! belongs to the core; the desktop shell is one client, not a privileged
//! one.

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::config;
use crate::event::AgentEvent;
use crate::model::DynamicModel;
use crate::protocol::{
    ClientMessage, ErrorCategory, PROTOCOL_VERSION, ServerEvent, decode, encode,
};
use crate::provider::{ModelRequest, provider_for, supported_providers};
use crate::session::{AgentCommand, Session};

/// How much input to buffer ahead of the reader.
///
/// The desktop shell may leave the process idle for a long time, so this is
/// sized generously rather than tuned for throughput.
const READ_BUFFER_BYTES: usize = 64 * 1024;

// ============================================================
// Server
// ============================================================

pub struct DesktopServer {
    session: Session,
    input_tx: mpsc::Sender<String>,
    cancel: CancellationToken,
    model: DynamicModel,
}

impl DesktopServer {
    pub fn new(
        session: Session,
        input_tx: mpsc::Sender<String>,
        cancel: CancellationToken,
        model: DynamicModel,
    ) -> Self {
        Self {
            session,
            input_tx,
            cancel,
            model,
        }
    }

    /// Run the read/write loop until stdin closes or the core is cancelled.
    pub async fn run(mut self, mut events: mpsc::Receiver<AgentEvent>) -> Result<()> {
        let stdout = tokio::io::stdout();

        let mut stdout = tokio::io::BufWriter::new(stdout);

        let mut stdin = BufReader::with_capacity(READ_BUFFER_BYTES, tokio::io::stdin());

        let mut line = String::new();

        write_event(
            &mut stdout,
            ServerEvent::Ready {
                version: PROTOCOL_VERSION,
            },
        )
        .await?;

        loop {
            tokio::select! {
                read = stdin.read_line(&mut line) => {
                    if read? == 0 {
                        break;
                    }

                    let trimmed = line.trim();

                    if !trimmed.is_empty() {
                        self.handle_line(trimmed, &mut stdout).await?;
                    }

                    line.clear();
                }

                Some(event) = events.recv() => {
                    // The session is the core's view of the conversation, so
                    // it folds every event first. The session then decides
                    // which events are worth putting on the wire; a client
                    // must never see an event the core did not fold in.
                    let before = self.session.state().messages.len();

                    self.session.handle_event(event.clone());

                    if let Some(event) = self.session.advance(before) {
                        write_event(&mut stdout, ServerEvent::Agent { event }).await?;
                    }
                }

                _ = self.cancel.cancelled() => break,
            }
        }

        Ok(())
    }

    async fn handle_line<W>(&mut self, line: &str, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let message = match decode(line) {
            Ok(message) => message,

            Err(error) => {
                // A malformed line must not kill the session: the client may
                // be mid-update, and the next line is probably fine.
                tracing::warn!(%error, "ignoring unparseable client message");

                return report_error(
                    stdout,
                    None,
                    format!("could not parse message: {error}"),
                    ErrorCategory::Fatal,
                )
                .await;
            }
        };

        match message {
            ClientMessage::GetSession => {
                write_event(
                    stdout,
                    ServerEvent::Session {
                        session: self.session.view(),
                    },
                )
                .await
            }

            ClientMessage::SetModel { request_id, model } => {
                self.change_model(request_id, model, stdout).await
            }

            message if message.request_id().is_some() => serve(message, stdout).await,

            // Everything left is a conversation command, which the session
            // owns; `into_command` only returns `None` for the arms above.
            message => {
                let command = message
                    .into_command()
                    .expect("every non-request client message is a session command");

                self.run_command(command, stdout).await
            }
        }
    }

    async fn change_model<W>(
        &mut self,
        request_id: String,
        name: String,
        stdout: &mut W,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let name = name.trim();
        if name.is_empty() {
            return report_error(
                stdout,
                Some(request_id),
                "model name cannot be empty".into(),
                ErrorCategory::Fatal,
            )
            .await;
        }

        let mut settings = match config::load() {
            Ok(config) => config,
            Err(error) => {
                return report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await;
            }
        };
        settings.model.name = name.to_string();

        let provider = crate::provider::provider_by_id(&settings.model.provider)
            .or_else(|| {
                settings
                    .providers
                    .iter()
                    .find(|provider| provider.id == settings.model.provider)
                    .cloned()
            })
            .or_else(|| {
                settings.model.provider.starts_with("custom-").then(|| {
                    crate::provider::SupportedProvider {
                        id: settings.model.provider.clone(),
                        name: "Custom provider".into(),
                        description: "Temporary custom OpenAI-compatible provider".into(),
                        category: crate::provider::ProviderCategory::Custom,
                        protocol: crate::provider::ProviderProtocol::OpenAiCompatible,
                        endpoint: settings.model.endpoint.clone(),
                        requires_api_key: settings.model.api_key.is_some(),
                        supports_model_discovery: true,
                        api_key_env: None,
                    }
                })
            });
        let Some(provider) = provider else {
            return report_error(
                stdout,
                Some(request_id),
                format!("Unknown configured provider: {}", settings.model.provider),
                ErrorCategory::Fatal,
            )
            .await;
        };
        let request = ModelRequest {
            provider,
            api_key: settings.model.api_key.clone(),
            endpoint: Some(settings.model.endpoint.clone()),
        };
        if let Err(error) = provider_for(&request.provider).test(&request, name).await {
            return report_error(
                stdout,
                Some(request_id),
                format!("Model test failed: {error}"),
                ErrorCategory::Transient,
            )
            .await;
        }

        if let Err(error) = config::save(&settings) {
            return report_error(
                stdout,
                Some(request_id),
                error.to_string(),
                ErrorCategory::Transient,
            )
            .await;
        }

        self.model.replace(&settings.model).await;
        self.session
            .set_model(settings.model.provider.clone(), settings.model.name.clone());
        write_event(
            stdout,
            ServerEvent::ModelChanged {
                request_id,
                provider: settings.model.provider,
                model: settings.model.name,
            },
        )
        .await?;
        write_event(
            stdout,
            ServerEvent::Session {
                session: self.session.view(),
            },
        )
        .await
    }

    /// Dispatch a session command, reporting rejections back to the client
    /// instead of taking the process down.
    async fn run_command<W>(&mut self, command: AgentCommand, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let forwarded = match self.session.apply(command.clone()).await {
            // Anything the session resolves locally (`/help`, `/quit`) or
            // routes to the agent itself — a confirmation, a cancel — returns
            // `None`, and has already been acted on.
            Ok(forwarded) => forwarded,

            Err(error) => {
                tracing::warn!(%error, "client command rejected");

                return report_error(stdout, None, error.to_string(), ErrorCategory::Transient)
                    .await;
            }
        };

        match forwarded {
            Some(text) => self.forward(text, stdout).await,

            // Local commands (`/help`, `/clear`) and state-only commands do
            // not generate an agent event, so send the updated snapshot to
            // keep clients' transcript and status in sync.
            None => {
                write_event(
                    stdout,
                    ServerEvent::Session {
                        session: self.session.view(),
                    },
                )
                .await
            }
        }
    }

    /// Hand a prompt to the agent. The session has already recorded it in the
    /// transcript, so a failure here is a dead agent, not a bad request.
    async fn forward<W>(&self, text: String, stdout: &mut W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        match self.input_tx.send(text).await {
            Ok(()) => Ok(()),

            Err(_) => {
                report_error(
                    stdout,
                    None,
                    "the agent is no longer accepting input".to_string(),
                    ErrorCategory::Fatal,
                )
                .await
            }
        }
    }
}

// ============================================================
// Setup and introspection
// ============================================================

async fn serve<W>(message: ClientMessage, stdout: &mut W) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    match message {
        ClientMessage::GetSetupConfig { request_id } => match crate::config::load_or_default() {
            Ok(config) => {
                write_event(stdout, ServerEvent::SetupConfig { request_id, config }).await?;
            }

            Err(error) => {
                report_error(
                    stdout,
                    Some(request_id),
                    error.to_string(),
                    ErrorCategory::Transient,
                )
                .await?;
            }
        },

        ClientMessage::SaveSetupConfig { request_id, config } => {
            match crate::config::save(&config) {
                Ok(()) => {
                    write_event(stdout, ServerEvent::SetupSaved { request_id }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::GetProviders { request_id } => {
            let mut providers = supported_providers();
            if let Ok(config) = crate::config::load_or_default() {
                for saved in config.providers {
                    if !providers.iter().any(|provider| provider.id == saved.id) {
                        providers.push(saved);
                    }
                }
            }
            write_event(
                stdout,
                ServerEvent::Providers {
                    request_id,
                    providers,
                },
            )
            .await?;
        }

        ClientMessage::GetModels {
            request_id,
            provider,
            api_key,
            endpoint,
        } => {
            let request = ModelRequest {
                provider: provider.clone(),
                api_key,
                endpoint,
            };

            match provider_for(&provider).list_models(&request).await {
                Ok(models) => {
                    write_event(stdout, ServerEvent::Models { request_id, models }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::TestProvider {
            request_id,
            provider,
            api_key,
            endpoint,
            model,
        } => {
            let request = ModelRequest {
                provider: provider.clone(),
                api_key,
                endpoint,
            };

            match provider_for(&provider).test(&request, &model).await {
                Ok(()) => {
                    write_event(stdout, ServerEvent::ProviderTested { request_id }).await?;
                }

                Err(error) => {
                    report_error(
                        stdout,
                        Some(request_id),
                        error.to_string(),
                        ErrorCategory::Transient,
                    )
                    .await?;
                }
            }
        }

        ClientMessage::SetModel { .. } => {
            // Handled by DesktopServer because it owns the live model handle.
        }

        other => {
            tracing::debug!(?other, "unhandled client message");
        }
    }

    Ok(())
}

// ============================================================
// Output
// ============================================================

async fn write_event<W>(writer: &mut W, event: ServerEvent) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let line = encode(&event)?;

    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

async fn report_error<W>(
    writer: &mut W,
    request_id: Option<String>,
    message: String,
    category: ErrorCategory,
) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    write_event(
        writer,
        ServerEvent::Error {
            request_id,
            message,
            category,
        },
    )
    .await
}
