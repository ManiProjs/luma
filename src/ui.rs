use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

use crate::event::AgentEvent;

pub struct Renderer {
    spinner: Option<ProgressBar>,
}

impl Renderer {
    pub fn new() -> Self {
        Self { spinner: None }
    }

    pub fn handle(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking => {
                let spinner = ProgressBar::new_spinner();

                spinner.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());

                spinner.set_message("Luma is thinking...");
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                self.spinner = Some(spinner);
            }

            AgentEvent::Planning => {
                println!("{} Planning next action...", "🧠".bright_blue());
            }

            AgentEvent::ToolStarted { name, input } => {
                if let Some(spinner) = &self.spinner {
                    spinner.finish_and_clear();
                }

                println!(
                    "\n{} {} {}",
                    "🔧".bright_green(),
                    name.bold(),
                    input.dimmed()
                );
            }

            AgentEvent::ToolFinished { name, duration_ms } => {
                println!(
                    "{} {} finished {}",
                    "✓".bright_green(),
                    name,
                    format!("({}ms)", duration_ms).dimmed()
                );
            }

            AgentEvent::TextDelta(text) => {
                print!("{}", text);
            }

            AgentEvent::Finished => {
                if let Some(spinner) = &self.spinner {
                    spinner.finish_and_clear();
                }

                println!("\n\n{} Finished", "✓".bright_green());
            }

            AgentEvent::Error(error) => {
                eprintln!("{} {}", "✗".bright_red(), error);
            }
        }
    }
}
