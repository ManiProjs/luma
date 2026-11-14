//! End-to-end agent-loop tests with scripted react + mock model.
//!
//! Each test drives the real `Agent::run` through channels, in a temp
//! working directory, with auto-confirmation. No network, no real model,
//! no user filesystem.

use std::sync::Mutex;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    agent::{Agent, Confirmation, QuestionAnswer},
    event::AgentEvent,
    history::History,
    react::ReActAction,
    tools::{
        ToolRegistry,
        filesystem::{
            list_directory::ListDirectory, patch_file::PatchFile, read_file::ReadFile,
            search_files::SearchFiles, write_file::WriteFile,
        },
        git::{GitDiff, GitStatus},
        shell::RunCommand,
    },
};

use super::mock::MockModel;

struct FailingReAct;

#[async_trait::async_trait]
impl crate::react::ReActTrait for FailingReAct {
    async fn next_action(
        &self,
        _messages: Vec<crate::context::Message>,
        _cancel: CancellationToken,
    ) -> anyhow::Result<ReActAction> {
        anyhow::bail!("invalid ReAct response")
    }
}

fn action_json(
    action: ReActAction,
    final_responses: &mut impl Iterator<Item = String>,
) -> serde_json::Value {
    match action {
        ReActAction::Tool { name, input } => serde_json::json!({
            "type": "tool", "name": name, "input": input
        }),
        ReActAction::Answer { content } => serde_json::json!({
            "type": "answer",
            "content": final_responses.next().unwrap_or(content)
        }),
        ReActAction::Plan { content } => serde_json::json!({
            "type": "plan", "content": content
        }),
    }
}

/// Builds a fully-wired agent, runs one user input through it, and
/// returns every event the agent emitted.
async fn run_agent_once(
    react_actions: Vec<ReActAction>,
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
    tools.register(GitStatus);
    tools.register(GitDiff);

    let expects_observation = react_actions
        .iter()
        .any(|action| matches!(action, ReActAction::Tool { .. }));
    let mut final_responses = model_responses.into_iter();
    let responses = react_actions
        .into_iter()
        .map(|action| action_json(action, &mut final_responses).to_string())
        .collect();
    let model = MockModel::new(responses);
    let model_capture = model.clone();
    let react = crate::react::ReAct::new(model.clone(), &tools);

    let mut agent = Agent::new(model, react, tools, History::default(), String::new());

    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(100);
    let (input_tx, input_rx) = mpsc::channel::<String>(10);
    let (confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(16);
    let (_question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(4);
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
            .run(
                input_rx,
                event_tx,
                agent_cancel.clone(),
                crate::session::RequestCancellation::new(agent_cancel),
                confirmation_rx,
                question_rx,
            )
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

    if expects_observation {
        let requests = model_capture.requests();
        assert!(
            requests.iter().skip(1).any(|messages| messages
                .iter()
                .any(|message| { message.role == crate::context::MessageRole::Observation })),
            "the next ReAct turn must receive the previous tool observation"
        );
    }

    events
}

#[tokio::test]
async fn a_request_error_is_reported_without_stopping_the_agent_loop() {
    let tools = ToolRegistry::new();
    let model = MockModel::new(vec!["recovered".into()]);
    let react = crate::react::ReAct::new(model.clone(), &tools);
    let mut agent = Agent::new(model, react, tools, History::default(), String::new());
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (input_tx, input_rx) = mpsc::channel::<String>(4);
    let (_confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(4);
    let (_question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(4);
    let shutdown = CancellationToken::new();
    let request_cancel = crate::session::RequestCancellation::new(shutdown.clone());

    let handle = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                shutdown,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });

    input_tx.send("\0LUMA_RESTORE:{".into()).await.unwrap();
    input_tx.send("hello".into()).await.unwrap();

    let mut events = Vec::new();
    let mut finished = 0;
    while finished < 2 {
        let event = tokio::time::timeout(tokio::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("agent should continue after reporting a request error")
            .expect("agent event channel should remain open");
        if matches!(event, AgentEvent::Finished) {
            finished += 1;
        }
        events.push(event);
    }

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Error(_)))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::TextDelta(text) if text == "recovered"))
    );

    drop(input_tx);
    let _ = handle.await;
}

#[tokio::test]
async fn a_react_failure_falls_back_to_a_direct_answer() {
    let tools = ToolRegistry::new();
    let model = MockModel::new(vec!["Here is the recovered answer.".into()]);
    let mut agent = Agent::new(
        model,
        FailingReAct,
        tools,
        History::default(),
        String::new(),
    );
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (input_tx, input_rx) = mpsc::channel::<String>(4);
    let (_confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(4);
    let (_question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(4);
    let shutdown = CancellationToken::new();
    let request_cancel = crate::session::RequestCancellation::new(shutdown.clone());

    let handle = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                shutdown,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });
    input_tx.send("Explain this project.".into()).await.unwrap();

    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(tokio::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("fallback response should finish")
            .expect("agent event channel should remain open");
        let finished = matches!(event, AgentEvent::Finished);
        events.push(event);
        if finished {
            break;
        }
    }

    assert!(events.iter().any(|event| matches!(event, AgentEvent::SystemMessage(message) if message.contains("direct response"))));
    assert!(events.iter().any(|event| matches!(event, AgentEvent::TextDelta(text) if text == "Here is the recovered answer.")));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Error(_)))
    );

    drop(input_tx);
    let _ = handle.await;
}

#[tokio::test]
async fn ask_question_waits_for_matching_answer_and_records_observation() {
    let tools = crate::tools::default_registry();
    let model = MockModel::new(vec![
        r#"{"type":"tool","name":"ask_question","input":{"prompt":"Which target?","choices":["A","B"]}}"#.into(),
        r#"{"type":"answer","content":"Done with the selected target."}"#.into(),
    ]);
    let capture = model.clone();
    let react = crate::react::ReAct::new(model.clone(), &tools);
    let mut agent = Agent::new(model, react, tools, History::default(), String::new());
    agent.set_force_react(true);
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (input_tx, input_rx) = mpsc::channel::<String>(2);
    let (_confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(2);
    let (question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(4);
    let shutdown = CancellationToken::new();
    let request_cancel = crate::session::RequestCancellation::new(shutdown.clone());
    let worker = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                shutdown,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });
    // Ordinary Teammate messages must still dispatch interactive tools; this
    // must not depend on the Code-only workspace detector or /plan command.
    input_tx
        .send("Help me choose a target.".into())
        .await
        .unwrap();

    let question = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(AgentEvent::QuestionRequired {
                question_id,
                prompt,
                choices,
            }) = event_rx.recv().await
            {
                break (question_id, prompt, choices);
            }
        }
    })
    .await
    .expect("question event should arrive");
    assert_eq!(question.1, "Which target?");
    assert_eq!(question.2, vec!["A", "B"]);
    question_tx
        .send(QuestionAnswer {
            question_id: "stale-question".into(),
            answer: "wrong".into(),
        })
        .await
        .unwrap();
    question_tx
        .send(QuestionAnswer {
            question_id: question.0,
            answer: "Target B".into(),
        })
        .await
        .unwrap();

    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event, AgentEvent::Finished) {
            break;
        }
    }
    drop(input_tx);
    worker.await.unwrap().unwrap();
    let requests = capture.requests();
    assert!(
        requests
            .iter()
            .skip(1)
            .any(|messages| messages.iter().any(|message| message.role
                == crate::context::MessageRole::Observation
                && message.content.contains("Target B"))),
        "the answer must be returned to ReAct as an observation"
    );
}

#[tokio::test]
async fn ask_question_unblocks_when_request_is_cancelled() {
    let tools = crate::tools::default_registry();
    let model = MockModel::new(vec![
        r#"{"type":"tool","name":"ask_question","input":{"prompt":"Need input"}}"#.into(),
    ]);
    let react = crate::react::ReAct::new(model.clone(), &tools);
    let mut agent = Agent::new(model, react, tools, History::default(), String::new());
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (input_tx, input_rx) = mpsc::channel::<String>(2);
    let (_confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(2);
    let (_question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(2);
    let shutdown = CancellationToken::new();
    let request_cancel = crate::session::RequestCancellation::new(shutdown.clone());
    let cancel_handle = request_cancel.clone();
    let worker = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                shutdown,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });
    input_tx.send("/plan Need a decision".into()).await.unwrap();
    loop {
        if matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
                .await
                .unwrap(),
            Some(AgentEvent::QuestionRequired { .. })
        ) {
            break;
        }
    }
    cancel_handle.cancel();
    drop(input_tx);
    let mut saw_cancel = false;
    while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .unwrap()
    {
        if matches!(event, AgentEvent::Cancelled) {
            saw_cancel = true;
        }
        if matches!(event, AgentEvent::Finished) {
            break;
        }
    }
    worker.await.unwrap().unwrap();
    assert!(
        saw_cancel,
        "cancellation must release the pending question wait"
    );
}

#[tokio::test]
async fn ask_question_falls_back_to_a_follow_up_task_when_interaction_is_unavailable() {
    let tools = crate::tools::teammate_registry();
    let model = MockModel::new(vec![
        r#"{"type":"tool","name":"ask_question","input":{"prompt":"Which target?"}}"#.into(),
        r#"{"type":"answer","content":"Which target should I optimize for? Please reply in your next message."}"#.into(),
    ]);
    let capture = model.clone();
    let react = crate::react::ReAct::new(model.clone(), &tools);
    let mut agent = Agent::new(model, react, tools, History::default(), String::new());
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (input_tx, input_rx) = mpsc::channel::<String>(2);
    let (_confirmation_tx, confirmation_rx) = mpsc::channel::<Confirmation>(2);
    let (_question_tx, question_rx) = mpsc::channel::<QuestionAnswer>(1);
    drop(_question_tx);
    let shutdown = CancellationToken::new();
    let request_cancel = crate::session::RequestCancellation::new(shutdown.clone());
    let worker = tokio::spawn(async move {
        agent
            .run(
                input_rx,
                event_tx,
                shutdown,
                request_cancel,
                confirmation_rx,
                question_rx,
            )
            .await
    });
    input_tx
        .send("/plan Choose an optimization target.".into())
        .await
        .unwrap();

    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("fallback question should finish")
            .expect("agent event channel should remain open");
        let finished = matches!(event, AgentEvent::Finished);
        events.push(event);
        if finished {
            break;
        }
    }
    drop(input_tx);
    worker.await.unwrap().unwrap();

    assert!(events.iter().any(|event| matches!(event, AgentEvent::TextDelta(text) if text.contains("Which target should I optimize"))), "events: {events:?}");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::Error(_)))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::QuestionRequired { .. }))
    );
    assert!(
        capture
            .requests()
            .iter()
            .skip(1)
            .any(|messages| messages.iter().any(|message| {
                message.role == crate::context::MessageRole::Observation
                    && message
                        .content
                        .contains("wait for their reply in the next task")
            }))
    );
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
    struct RestoreCurrentDir(std::path::PathBuf);
    impl Drop for RestoreCurrentDir {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }
    let _restore = RestoreCurrentDir(original);
    f(dir.path().to_path_buf()).await;
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
// The react requests a file read; the tool executes; the observation
// reaches the next react step (react's second action sees it); the
// agent then answers.

#[tokio::test]
async fn test_a_inspect() {
    in_temp_dir(|dir| async move {
        std::fs::write(dir.join("hello.txt"), "hello from test").unwrap();

        let events = run_agent_once(
            vec![
                ReActAction::Tool {
                    name: "read_file".into(),
                    input: "hello.txt".into(),
                },
                ReActAction::Answer {
                    content: "done".into(),
                },
            ],
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

#[tokio::test]
async fn plan_only_request_does_not_execute_tools() {
    in_temp_dir(|_dir| async move {
        let events = run_agent_once(
            vec![ReActAction::Plan {
                content: "Inspect the module, implement the feature, run tests.".into(),
            }],
            vec![],
            "/plan add a feature to this project",
            vec![Confirmation::Allow],
        )
        .await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::PlanGenerated(_)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::ToolStarted { .. }))
        );
        assert!(matches!(events.last(), Some(AgentEvent::Finished)));
    })
    .await;
}

// ── Test B: modify ──────────────────────────────────────────────────────────
//
// The react inspects a file, then requests a patch. The patch requires
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
                ReActAction::Plan {
                    content: "Inspect code.rs, patch the call, verify.".into(),
                },
                ReActAction::Tool {
                    name: "read_file".into(),
                    input: "code.rs".into(),
                },
                ReActAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                ReActAction::Answer { content: "done".into() },
            ],
            vec!["Patched.".into()],
            "edit the file code.rs",
            vec![Confirmation::Allow, Confirmation::Allow],
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
        let preview_index = events
            .iter()
            .position(|event| matches!(event, AgentEvent::ChangePreview { .. }))
            .expect("file changes must be previewed");
        let confirmation_index = events
            .iter()
            .position(|event| matches!(event, AgentEvent::ConfirmationRequired { name, .. } if name == "patch_file"))
            .expect("file changes must require approval");
        assert!(preview_index < confirmation_index);
        let tool_index = events
            .iter()
            .position(|event| matches!(event, AgentEvent::ToolStarted { name, .. } if name == "patch_file"))
            .expect("approved patch should start");
        assert!(confirmation_index < tool_index);
    })
    .await;
}

// ── Test C: verification failure ────────────────────────────────────────────
//
// The agent modifies a file, runs a failing command, and the failure
// (exit code + stderr) becomes an observation the next react step sees.

#[tokio::test]
async fn test_c_verification_failure() {
    in_temp_dir(|_dir| async move {
        let events = run_agent_once(
            vec![
                ReActAction::Tool {
                    name: "run_command".into(),
                    input: "echo 'error: expected identifier' >&2; exit 1".into(),
                },
                ReActAction::Answer {
                    content: "done".into(),
                },
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
                ReActAction::Tool {
                    name: "run_command".into(),
                    input: "echo ok".into(),
                },
                ReActAction::Answer {
                    content: "done".into(),
                },
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
                ReActAction::Plan {
                    content: "Inspect the missing file, then patch it.".into(),
                },
                ReActAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                ReActAction::Answer {
                    content: "done".into(),
                },
            ],
            vec!["Could not patch.".into()],
            "patch the file missing.rs",
            vec![Confirmation::Allow],
        )
        .await;

        let errs = errors(&events);
        assert!(
            errs.iter()
                .any(|e| e.contains("Failed to run tool") && e.contains("patch_file")),
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
                ReActAction::Plan {
                    content: "Inspect keep.rs and patch the requested text.".into(),
                },
                ReActAction::Tool {
                    name: "read_file".into(),
                    input: "keep.rs".into(),
                },
                ReActAction::Tool {
                    name: "patch_file".into(),
                    input: patch_input,
                },
                ReActAction::Answer {
                    content: "done".into(),
                },
            ],
            vec!["Skipped.".into()],
            "edit the file keep.rs",
            vec![Confirmation::Allow, Confirmation::Deny],
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
