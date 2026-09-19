use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agent::Confirmation;
use crate::event::AgentEvent;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DesktopMessage {
    Prompt { text: String },
    Cancel,
    Confirm { allowed: bool },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum DesktopEvent {
    Agent(AgentEvent),
    Ready,
    Error { message: String },
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
            // ------------------------------------------------
            // Messages coming from Electron
            // ------------------------------------------------

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
                                let confirmation = if allowed {
                                    Confirmation::Allow
                                } else {
                                    Confirmation::Deny
                                };

                                if confirmation_tx.send(confirmation).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }

                    Err(error) => {
                        write_event(
                            &mut stdout,
                            DesktopEvent::Error {
                                message: format!("Invalid desktop message: {error}"),
                            },
                        )
                        .await?;
                    }
                }

                line.clear();
            }

            // ------------------------------------------------
            // Agent -> Electron
            // ------------------------------------------------

            Some(event) = event_rx.recv() => {
                write_event(
                    &mut stdout,
                    DesktopEvent::Agent(event),
                )
                .await?;
            }

            // ------------------------------------------------
            // Cancellation
            // ------------------------------------------------

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
