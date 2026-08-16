use anyhow::Result;
use std::process::Command;

use super::Tool;

pub struct RunCommand;

impl Tool for RunCommand {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Runs a shell command"
    }

    fn execute(&self, input: &str) -> Result<String> {
        let output = Command::new("sh").arg("-c").arg(input).output()?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
