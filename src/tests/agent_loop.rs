//! End-to-end agent-loop tests with scripted planner + mock model.
//!
//! Each test drives the real `Agent::run` through channels, in a temp
//! working directory, with auto-confirmation. No network, no real model,
//! no user filesystem.

use std::sync::Mutex;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    agent::{Agent, Confirmation},
    event::AgentEvent,
    history::History,
    planner::PlanAction,
    tools::{
        ToolRegistry,
        filesystem::{
            list_directory::ListDirectory, patch_file::PatchFile, read_file::ReadFile,
            search_files::SearchFiles, write_file::WriteFile,
        },
        shell::RunCommand,
    },
};

use super::mock::{MockModel, ScriptedPlanner};

/// Builds a fully-wired agent, runs one user input through it, and
/// returns every event the agent emitted.
async fn run_agent_once(
    planner_actions: Vec<PlanAction>,
    model_responses: Vec<String>,
    user_input: &str,
    confirmations: Vec<Confirmation>,
) -> Vec<AgentEvent> {
    let mut tools = ToolRegistry::new();
    tools.register(ReadFile);
    tools.register(ListDirectory);
    tools.register(RunCommand);
    tools.register(SearchFiles);
    tools.register(WriteFile);
    tools.register(PatchFile);

    let planner = ScriptedPlanner::new(planner_actions);
    let model = MockModel::new(model_responses);

    let mut agent = Agent::new(model, planner, tools, History::default(), String::new());

    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(100);
    let (input_tx, input_rx) = mpsc::channel::<String>(10);
    let (confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(16);
    let cancel = CancellationToken::new();

    // Feed queued confirmations (auto-answer any confirmation request).
    tokio::spawn(async move {
        for confirmation in confirmations {
            // Wait until the agent actually asks, then answer.
            if confirmation_tx.send(confirmation).await.is_err() {
                break;
            }
        }
        // Keep the sender alive so the channel never closes prematurely.
        std::future::pending::<()>().await;
    });

    let agent_cancel = cancel.clone();
    let handle = tokio::spawn(async move {
        agent
            .run(input_rx, event_tx, agent_cancel, confirmation_rx)
            .await
    });

    input_tx.send(user_input.to_string()).await.unwrap();

    // Collect events until Finished (or a safety timeout).
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        match tokio::time::timeout_at(deadline, event_rx.recv()).await {
            Ok(Some(event)) => {
                let is_finished = matches!(event, AgentEvent::Finished);
                events.push(event);
                if is_finished {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => panic!("agent did not finish within 10s"),
        }
    }

    drop(input_tx);
    cancel.cancel();
    let _ = handle.await;

    events
}

/// Serializes tests that change the process working directory.
/// `std::env::set_current_dir` is process-global, so cwd-dependent tests
/// must not run concurrently with each other.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// Runs the closure inside a temp working directory, restoring the
/// original cwd afterwards. Required because the filesystem tools are
/// cwd-relative.
async fn in_temp_dir<F, Fut>(f: F)
where
    F: FnOnce(std::path::PathBuf) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    f(dir.path().to_path_buf()).await;
    std::env::set_current_dir(original).unwrap();
}

fn tool_names(events: &[AgentEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolStarted { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn errors(events: &[AgentEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::Error(msg) => Some(msg.clone()),
            _ => None,
        })
        .collect()
}

// ── Test A: inspect ─────────────────────────────────────────────────────────
//
// The planner requests a file read; the tool executes; the observation
// reaches the next planner step (planner's second action sees it); the
// agent then answers.

#[tokio::test]
async fn test_a_inspect() {
    in_temp_dir(|dir| async move {
        std::fs::write(dir.join("hello.txt"), "hello from test").unwrap();

        // The agent's deterministic-first-action shortcut reads hello.txt
        // before the planner runs, so the planner's first action is the
        // second tool call overall.
        let events = run_agent_once(
            vec![PlanAction::Answer],
            vec!["The file says hello.".into()],
            "read the file hello.txt in this project",
            vec![],
        )
        .await;

        assert!(tool_names(&events).contains(&"read_file".to_string()));
        assert!(errors(&events).is_empty(), "errors: {:?}", errors(&events));
        assert!(matches!(events.last(), Some(AgentEvent::Finished)));
    })
    .await;
}

// ── Test B: modify ──────────────────────────────────────────────────────────
//
// The planner inspects a file, then requests a patch. The patch requires
// confirmation, which the test auto-allows. The patch must execute and the
// file on disk must change.

#[tokio::test]
async fn test_b_modify() {
    in_temp_dir(|dir| async move {
        std::fs::write(dir.join("code.rs"), "fn main() { old(); }").unwrap();

        let patch_input = serde_json::json!({
            "path": "code.rs",
            "old": "old();",
            "new": "new();"
        })
        .to_string();

        let events = run_agent_once(
            vec![
                PlanAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                PlanAction::Answer,
            ],
            vec!["Patched.".into()],
            "edit the file code.rs",
            vec![Confirmation::Allow],
        )
        .await;

        // code.rs is read once (deterministic first action), then patched.
        assert_eq!(tool_names(&events), vec!["read_file", "patch_file"]);

        let content = std::fs::read_to_string(dir.join("code.rs")).unwrap();
        assert!(
            content.contains("new();"),
            "file was not patched: {content}"
        );
        assert!(errors(&events).is_empty(), "errors: {:?}", errors(&events));
    })
    .await;
}

// ── Test C: verification failure ────────────────────────────────────────────
//
// The agent modifies a file, runs a failing command, and the failure
// (exit code + stderr) becomes an observation the next planner step sees.

#[tokio::test]
async fn test_c_verification_failure() {
    in_temp_dir(|_dir| async move {
        let events = run_agent_once(
            vec![
                PlanAction::Tool {
                    name: "run_command".into(),
                    input: "echo 'error: expected identifier' >&2; exit 1".into(),
                },
                PlanAction::Answer,
            ],
            vec!["The build failed with a syntax error.".into()],
            "build this project",
            vec![Confirmation::Allow],
        )
        .await;

        assert_eq!(tool_names(&events), vec!["run_command"]);
        assert!(errors(&events).is_empty(), "errors: {:?}", errors(&events));
        // The agent completed the loop after seeing the failure — it did
        // not hang or fabricate success.
        assert!(matches!(events.last(), Some(AgentEvent::Finished)));
    })
    .await;
}

// ── Test D: successful verification ─────────────────────────────────────────
//
// The agent runs a passing command and reaches a normal completion state.

#[tokio::test]
async fn test_d_successful_verification() {
    in_temp_dir(|_dir| async move {
        let events = run_agent_once(
            vec![
                PlanAction::Tool {
                    name: "run_command".into(),
                    input: "echo ok".into(),
                },
                PlanAction::Answer,
            ],
            vec!["Build succeeded.".into()],
            "run the tests for this project",
            vec![Confirmation::Allow],
        )
        .await;

        assert_eq!(tool_names(&events), vec!["run_command"]);
        assert!(errors(&events).is_empty(), "errors: {:?}", errors(&events));
        assert!(matches!(events.last(), Some(AgentEvent::Finished)));
    })
    .await;
}

// ── Test E: error path ──────────────────────────────────────────────────────
//
// A tool failure (patch on a missing file) produces an error event; the
// agent does not continue blindly — the loop surfaces the failure.

#[tokio::test]
async fn test_e_tool_failure_propagates() {
    in_temp_dir(|_dir| async move {
        let patch_input = serde_json::json!({
            "path": "missing.rs",
            "old": "a",
            "new": "b"
        })
        .to_string();

        let events = run_agent_once(
            vec![
                PlanAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                PlanAction::Answer,
            ],
            vec!["Could not patch.".into()],
            "patch the file missing.rs",
            vec![Confirmation::Allow],
        )
        .await;

        let errs = errors(&events);
        assert!(
            errs.iter().any(|e| e.contains("patch_file failed")),
            "expected a patch_file failure, events: {events:?}"
        );
    })
    .await;
}

// ── Confirmation denial ─────────────────────────────────────────────────────
//
// When the user denies a destructive action, the tool must NOT execute.

#[tokio::test]
async fn test_confirmation_denied_skips_tool() {
    in_temp_dir(|dir| async move {
        std::fs::write(dir.join("keep.rs"), "original").unwrap();

        let patch_input = serde_json::json!({
            "path": "keep.rs",
            "old": "original",
            "new": "changed"
        })
        .to_string();

        let events = run_agent_once(
            vec![
                PlanAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                PlanAction::Answer,
            ],
            vec!["Skipped.".into()],
            "edit the file keep.rs",
            vec![Confirmation::Deny],
        )
        .await;

        // patch_file never started (confirmation denied before execution).
        assert!(
            !tool_names(&events).contains(&"patch_file".to_string()),
            "patch_file executed despite denial"
        );

        let content = std::fs::read_to_string(dir.join("keep.rs")).unwrap();
        assert_eq!(content, "original", "file changed despite denial");
    })
    .await;
}
