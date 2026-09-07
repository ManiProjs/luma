use std::time::Duration;

use anyhow::{Result, anyhow};

use super::Tool;

/// Hard ceiling on a single command so a hanging process cannot stall
/// the agent forever.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum bytes of stdout/stderr kept in the observation. Long output
/// is truncated with a marker so the agent knows it was cut short.
const MAX_OUTPUT_BYTES: usize = 20_000;

pub struct RunCommand;

impl Tool for RunCommand {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Runs a shell command and returns its exit code, stdout, and stderr. \
         Use it to build, test, lint, and verify changes. Commands fail with \
         a non-zero exit code when they fail; always read stderr in that case."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let command = input.to_owned();

        let (tx, rx) = std::sync::mpsc::channel();

        // Tool::execute is synchronous, so run the process on a dedicated
        // thread. A dedicated thread (rather than the tokio pool) keeps
        // blocking I/O off the runtime's workers.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();

            let result = match rt {
                Ok(runtime) => match runtime.block_on(async {
                    tokio::time::timeout(COMMAND_TIMEOUT, run_process(command)).await
                }) {
                    Ok(output) => Ok(output),
                    Err(_) => {
                        Ok("ERROR: command timed out after 120s and was terminated.".to_string())
                    }
                },
                Err(error) => Err(anyhow!(error)),
            };

            let _ = tx.send(result);
        });

        rx.recv()
            .map_err(|_| anyhow!("run_command worker thread panicked"))?
    }
}

async fn run_process(command: String) -> String {
    tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map(|output| {
            format_output(
                &command,
                &output.stdout,
                &output.stderr,
                output.status.code(),
            )
        })
        .unwrap_or_else(|error| {
            format!("ERROR: failed to spawn command: {error}\ncommand: {command}")
        })
}

fn format_output(command: &str, stdout: &[u8], stderr: &[u8], exit_code: Option<i32>) -> String {
    let stdout_text = truncate_output(stdout);
    let stderr_text = truncate_output(stderr);

    let mut result = String::new();

    match exit_code {
        Some(0) => result.push_str("exit code: 0 (success)\n"),
        Some(code) => result.push_str(&format!(
            "exit code: {code} (FAILED — the command reported an error)\n"
        )),
        None => result.push_str("exit code: unknown (process terminated by signal)\n"),
    }

    result.push_str(&format!("\nstdout:\n{stdout_text}"));
    result.push_str(&format!("\nstderr:\n{stderr_text}"));
    result.push_str(&format!("\ncommand: {command}"));

    result
}

fn truncate_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);

    if text.len() <= MAX_OUTPUT_BYTES {
        return text.into_owned();
    }

    // Split at a character boundary near the cap, keeping head and tail
    // so error summaries at either end survive.
    let mut head_end = MAX_OUTPUT_BYTES / 2;
    while head_end > 0 && !text.is_char_boundary(head_end) {
        head_end -= 1;
    }

    let mut tail_start = text.len() - (MAX_OUTPUT_BYTES - head_end);
    while tail_start < text.len() && !text.is_char_boundary(tail_start) {
        tail_start += 1;
    }

    format!(
        "{}\n... [output truncated: {} of {} bytes shown] ...\n{}",
        &text[..head_end],
        MAX_OUTPUT_BYTES,
        text.len(),
        &text[tail_start..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_success() {
        let out = RunCommand.execute("echo hello").unwrap();
        assert!(out.contains("exit code: 0"));
        assert!(out.contains("hello"));
    }

    #[test]
    fn reports_failure_with_stderr() {
        let out = RunCommand.execute("echo 'boom' >&2; exit 3").unwrap();
        assert!(out.contains("exit code: 3"));
        assert!(out.contains("FAILED"));
        assert!(out.contains("boom"));
    }
}
