use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent::Confirmation;
use crate::config::Config;
use crate::event::AgentEvent;
use crate::provider::{
    ModelRequest, SupportedModel, SupportedProvider, provider_for, supported_providers,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DesktopMessage {
    Prompt {
        text: String,
    },

    Cancel,

    Confirm {
        allowed: bool,
    },

    GetSetupConfig {
        request_id: String,
    },

    SaveSetupConfig {
        request_id: String,
        config: Config,
    },

    GetProviders {
        request_id: String,
    },

    GetModels {
        request_id: String,
        provider: SupportedProvider,
        api_key: Option<String>,
        endpoint: Option<String>,
    },

    TestProvider {
        request_id: String,
        provider: SupportedProvider,
        api_key: Option<String>,
        endpoint: Option<String>,
        model: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum DesktopEvent {
    Agent(AgentEvent),

    Ready,

    Error {
        request_id: Option<String>,
        message: String,
    },

    SetupConfig {
        request_id: String,
        config: Config,
    },

    SetupSaved {
        request_id: String,
    },

    Providers {
        request_id: String,
        providers: Vec<SupportedProvider>,
    },

    Models {
        request_id: String,
        models: Vec<SupportedModel>,
    },

    ProviderTested {
        request_id: String,
    },
}

pub async fn run(
    mut event_rx: mpsc::Receiver<AgentEvent>,
    input_tx: mpsc::Sender<String>,
    confirmation_tx: mpsc::Sender<Confirmation>,
    cancel: CancellationToken,
) -> Result<()> {
    let stdout = tokio::io::stdout();

    let mut stdout = tokio::io::BufWriter::new(stdout);

    let mut stdin = BufReader::new(tokio::io::stdin());

    let mut line = String::new();

    write_event(&mut stdout, DesktopEvent::Ready).await?;

    loop {
        tokio::select! {
            result = stdin.read_line(&mut line) => {
                let bytes = result?;

                if bytes == 0 {
                    break;
                }

                let input = line.trim();

                if input.is_empty() {
                    line.clear();
                    continue;
                }

                match serde_json::from_str::<DesktopMessage>(input) {
                    Ok(message) => {
                        match message {
                            DesktopMessage::Prompt { text } => {
                                if input_tx.send(text).await.is_err() {
                                    break;
                                }
                            }

                            DesktopMessage::Cancel => {
                                cancel.cancel();
                            }

                            DesktopMessage::Confirm { allowed } => {
                                let confirmation =
                                    if allowed {
                                        Confirmation::Allow
                                    } else {
                                        Confirmation::Deny
                                    };

                                if confirmation_tx
                                    .send(confirmation)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }

                            DesktopMessage::GetSetupConfig {
                                request_id,
                            } => {
                                match crate::config::load_or_default() {
                                    Ok(config) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::SetupConfig {
                                                request_id,
                                                config,
                                            },
                                        )
                                        .await?;
                                    }

                                    Err(error) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::Error {
                                                request_id: Some(request_id),
                                                message: error.to_string(),
                                            },
                                        )
                                        .await?;
                                    }
                                }
                            }

                            DesktopMessage::SaveSetupConfig {
                                request_id,
                                config,
                            } => {
                                match crate::config::save(&config) {
                                    Ok(()) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::SetupSaved {
                                                request_id,
                                            },
                                        )
                                        .await?;
                                    }

                                    Err(error) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::Error {
                                                request_id: Some(request_id),
                                                message: error.to_string(),
                                            },
                                        )
                                        .await?;
                                    }
                                }
                            }

                            DesktopMessage::GetProviders {
                                request_id,
                            } => {
                                let providers = supported_providers();

                                write_event(
                                    &mut stdout,
                                    DesktopEvent::Providers {
                                        request_id,
                                        providers,
                                    },
                                )
                                .await?;
                            }

                            DesktopMessage::GetModels {
                                request_id,
                                provider,
                                api_key,
                                endpoint,
                            } => {
                                let provider_impl = provider_for(&provider);

                                let request = ModelRequest {
                                    provider,
                                    api_key,
                                    endpoint,
                                };

                                match provider_impl
                                    .list_models(&request)
                                    .await
                                {
                                    Ok(models) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::Models {
                                                request_id,
                                                models,
                                            },
                                        )
                                        .await?;
                                    }

                                    Err(error) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::Error {
                                                request_id: Some(request_id),
                                                message: error.to_string(),
                                            },
                                        )
                                        .await?;
                                    }
                                }
                            }

                            DesktopMessage::TestProvider {
                                request_id,
                                provider,
                                api_key,
                                endpoint,
                                model,
                            } => {
                                let provider_impl = provider_for(&provider);

                                let request = ModelRequest {
                                    provider,
                                    api_key,
                                    endpoint,
                                };

                                match provider_impl
                                    .test(&request, &model)
                                    .await
                                {
                                    Ok(()) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::ProviderTested {
                                                request_id,
                                            },
                                        )
                                        .await?;
                                    }

                                    Err(error) => {
                                        write_event(
                                            &mut stdout,
                                            DesktopEvent::Error {
                                                request_id: Some(request_id),
                                                message: error.to_string(),
                                            },
                                        )
                                        .await?;
                                    }
                                }
                            }
                        }
                    }

                    Err(error) => {
                        write_event(
                            &mut stdout,
                            DesktopEvent::Error {
                                request_id: None,
                                message: format!(
                                    "Invalid desktop message: {error}"
                                ),
                            },
                        )
                        .await?;
                    }
                }

                line.clear();
            }

            Some(event) = event_rx.recv() => {
                write_event(
                    &mut stdout,
                    DesktopEvent::Agent(event),
                )
                .await?;
            }

            _ = cancel.cancelled() => {
                break;
            }
        }
    }

    Ok(())
}

async fn write_event<W>(writer: &mut W, event: DesktopEvent) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let json = serde_json::to_string(&event)?;

    writer.write_all(json.as_bytes()).await?;

    writer.write_all(b"\n").await?;

    writer.flush().await?;

    Ok(())
}
