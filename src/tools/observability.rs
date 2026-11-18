//! Read-only summaries of persisted agent and task execution metrics.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::runtime::{RuntimeEvent, RuntimeStore};

use super::Tool;

const DEFAULT_TASK_LIMIT: usize = 25;
const MAX_TASK_LIMIT: usize = 100;
const EVENT_LIMIT_PER_AGENT: usize = 1_000;

#[derive(Default)]
pub struct ExecutionMetrics {
    store: Option<RuntimeStore>,
}

#[derive(Default)]
pub struct AgentEvaluation {
    store: Option<RuntimeStore>,
}

impl ExecutionMetrics {
    /// Use an explicitly supplied store (also useful for in-memory tests).
    #[cfg(test)]
    pub fn new(store: RuntimeStore) -> Self {
        Self { store: Some(store) }
    }
}

impl AgentEvaluation {
    #[cfg(test)]
    pub fn new(store: RuntimeStore) -> Self {
        Self { store: Some(store) }
    }
}

#[derive(Debug, Deserialize)]
struct MetricsInput {
    agent_id: Option<String>,
    task_limit: Option<usize>,
}

#[derive(Default, Clone)]
struct Counters {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
    tools_started: u64,
    tools_finished: u64,
    tool_duration_ms: u128,
    tool_failures: u64,
    task_started_at: Option<i64>,
    task_finished_at: Option<i64>,
}

impl Counters {
    fn observe(&mut self, event: &RuntimeEvent) {
        match event.event_type.as_str() {
            "task.started" => {
                self.task_started_at.get_or_insert(event.created_at);
            }
            "task.completed" | "task.failed" | "task.cancelled" | "task.interrupted" => {
                self.task_finished_at = Some(event.created_at);
            }
            _ => {}
        }
        let Some(kind) = event.payload.get("type").and_then(Value::as_str) else {
            return;
        };
        let data = &event.payload["data"];
        match kind {
            "Usage" => {
                self.prompt_tokens += data["prompt_tokens"].as_u64().unwrap_or(0);
                self.completion_tokens += data["completion_tokens"].as_u64().unwrap_or(0);
                self.total_tokens += data["total_tokens"].as_u64().unwrap_or(0);
                self.cost_usd += data["cost_usd"].as_f64().unwrap_or(0.0);
            }
            "ToolStarted" => self.tools_started += 1,
            "ToolFinished" => {
                self.tools_finished += 1;
                self.tool_duration_ms += data["duration_ms"].as_u64().unwrap_or(0) as u128;
                if data["failed"].as_bool().unwrap_or(false) {
                    self.tool_failures += 1;
                }
            }
            _ => {}
        }
    }

    fn into_json(self) -> Value {
        json!({
            "prompt_tokens": self.prompt_tokens,
            "completion_tokens": self.completion_tokens,
            "total_tokens": self.total_tokens,
            "cost_usd": self.cost_usd,
            "tools_started": self.tools_started,
            "tools_finished": self.tools_finished,
            "tool_duration_ms": self.tool_duration_ms,
            "tool_failures": self.tool_failures,
        })
    }
}

impl Tool for ExecutionMetrics {
    fn name(&self) -> &str {
        "execution_metrics"
    }

    fn description(&self) -> &str {
        "Read persisted execution observability metrics: token usage and cost, tool durations/failures, and task status/duration, grouped by agent and task. Input JSON: {\"agent_id\":\"optional agent ID\",\"task_limit\":25} (task_limit defaults to 25 and is capped at 100). Reads up to the latest 1000 persisted events per agent."
    }

    fn model_guide(&self) -> &str {
        "Use to inspect historical execution cost, token consumption, tool reliability/latency, and task outcomes. cost_usd is an estimate using Luma's default token rates, not authoritative provider billing; local and custom OpenAI-compatible endpoints may use different pricing. Metrics reflect persisted events (up to the latest 1000 per agent) and task records; they do not include in-memory activity that has not been persisted yet."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: MetricsInput =
            serde_json::from_str(input).context("execution_metrics: invalid JSON input")?;
        let agent_id = input
            .agent_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let task_limit = input
            .task_limit
            .unwrap_or(DEFAULT_TASK_LIMIT)
            .clamp(1, MAX_TASK_LIMIT);
        let opened;
        let store = if let Some(store) = self.store.as_ref() {
            store
        } else {
            opened = RuntimeStore::open_default().context("opening persisted runtime store")?;
            &opened
        };

        let agents = store.list_agents()?;
        let agents = agents
            .into_iter()
            .filter(|agent| agent_id.is_none_or(|id| agent.id == id))
            .collect::<Vec<_>>();
        let tasks = store.list_tasks(agent_id, task_limit)?;
        let mut agent_metrics = Vec::with_capacity(agents.len());
        let mut task_counters = BTreeMap::<String, Counters>::new();

        for agent in &agents {
            let events = store.recent_events_for_agent(&agent.id, EVENT_LIMIT_PER_AGENT)?;
            let mut counters = Counters::default();
            for event in &events {
                counters.observe(event);
                if let Some(task_id) = event.task_id.as_deref() {
                    task_counters
                        .entry(task_id.to_owned())
                        .or_default()
                        .observe(event);
                }
            }
            agent_metrics.push(json!({
                "agent_id": agent.id,
                "name": agent.name,
                "kind": agent.kind,
                "status": agent.status,
                "provider": agent.configuration.provider,
                "model": agent.configuration.model,
                "metrics": counters.into_json(),
                "events_analyzed": events.len(),
                "event_window_full": events.len() == EVENT_LIMIT_PER_AGENT,
            }));
        }

        let task_metrics = tasks
            .into_iter()
            .map(|task| {
                let metrics = task_counters.remove(&task.id).unwrap_or_default();
                let terminal = matches!(
                    task.status,
                    crate::runtime::TaskStatus::Completed
                        | crate::runtime::TaskStatus::Failed
                        | crate::runtime::TaskStatus::Cancelled
                        | crate::runtime::TaskStatus::Interrupted
                );
                let ended_at = metrics.task_finished_at.unwrap_or_else(|| {
                    if terminal {
                        task.updated_at
                    } else {
                        crate::runtime::unix_now()
                    }
                });
                let duration_ms = ended_at.saturating_sub(task.created_at).max(0) as u64 * 1_000;
                let execution_duration_ms = metrics
                    .task_started_at
                    .map(|started_at| ended_at.saturating_sub(started_at).max(0) as u64 * 1_000);
                json!({
                    "task_id": task.id,
                    "agent_id": task.agent_id,
                    "title": task.title,
                    "status": task.status,
                    "duration_ms": duration_ms,
                    "execution_duration_ms": execution_duration_ms,
                    "created_at": task.created_at,
                    "updated_at": task.updated_at,
                    "metrics": metrics.into_json(),
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::to_string(&json!({
            "agents": agent_metrics,
            "tasks": task_metrics,
            "limits": {
                "task_limit": task_limit,
                "events_per_agent": EVENT_LIMIT_PER_AGENT,
            }
        }))?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

impl Tool for AgentEvaluation {
    fn name(&self) -> &str {
        "evaluate_agent"
    }

    fn description(&self) -> &str {
        "Build an operational scorecard from persisted task outcomes and tool events. Input JSON: {\"agent_id\":\"optional agent ID\",\"task_limit\":25}. Reports completion rate, tool success rate, failures, interruptions, token use, reliability score, and per-agent provider/model comparisons. It does not judge answer quality."
    }

    fn model_guide(&self) -> &str {
        "Use evaluate_agent to assess an agent's operational reliability before changing its model, tools or instructions. Treat small samples as inconclusive. The score measures persisted task completion and tool-call success only; it is not a benchmark of reasoning or answer correctness."
    }

    fn execute(&self, input: &str) -> Result<String> {
        let input: MetricsInput =
            serde_json::from_str(input).context("evaluate_agent: invalid JSON input")?;
        let agent_id = input
            .agent_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let task_limit = input
            .task_limit
            .unwrap_or(DEFAULT_TASK_LIMIT)
            .clamp(1, MAX_TASK_LIMIT);
        let opened;
        let store = if let Some(store) = self.store.as_ref() {
            store
        } else {
            opened = RuntimeStore::open_default().context("opening persisted runtime store")?;
            &opened
        };
        let metrics_input = serde_json::json!({ "agent_id": agent_id, "task_limit": task_limit });
        let metrics = ExecutionMetrics {
            store: Some(store.clone()),
        }
        .execute(&metrics_input.to_string())?;
        let metrics: Value = serde_json::from_str(&metrics)?;
        let tasks = metrics["tasks"].as_array().cloned().unwrap_or_default();
        let agents = metrics["agents"].as_array().cloned().unwrap_or_default();
        let task_count = tasks.len();
        let completed = tasks
            .iter()
            .filter(|task| task["status"] == "completed")
            .count();
        let failed = tasks
            .iter()
            .filter(|task| task["status"] == "failed")
            .count();
        let interrupted = tasks
            .iter()
            .filter(|task| task["status"] == "interrupted")
            .count();
        let cancelled = tasks
            .iter()
            .filter(|task| task["status"] == "cancelled")
            .count();
        let terminal = completed + failed + interrupted + cancelled;
        let tool_started: u64 = agents
            .iter()
            .map(|agent| agent["metrics"]["tools_started"].as_u64().unwrap_or(0))
            .sum();
        let tool_finished: u64 = agents
            .iter()
            .map(|agent| agent["metrics"]["tools_finished"].as_u64().unwrap_or(0))
            .sum();
        let tool_failures: u64 = agents
            .iter()
            .map(|agent| agent["metrics"]["tool_failures"].as_u64().unwrap_or(0))
            .sum();
        let tokens: u64 = agents
            .iter()
            .map(|agent| agent["metrics"]["total_tokens"].as_u64().unwrap_or(0))
            .sum();
        let completion_rate = (terminal > 0).then_some(completed as f64 / terminal as f64);
        let tool_success_rate = (tool_finished > 0)
            .then_some((tool_finished.saturating_sub(tool_failures)) as f64 / tool_finished as f64);
        let score = match (completion_rate, tool_success_rate) {
            (Some(tasks), Some(tools)) => {
                Some(((tasks * 0.65 + tools * 0.35) * 100.0).round() as u8)
            }
            (Some(tasks), None) => Some((tasks * 100.0).round() as u8),
            (None, Some(tools)) => Some((tools * 100.0).round() as u8),
            (None, None) => None,
        };
        let health = match score {
            Some(score) if terminal >= 10 && score >= 90 => "reliable",
            Some(score) if terminal >= 5 && score < 70 => "needs_attention",
            Some(_) => "developing",
            None => "insufficient_data",
        };
        let model_comparison = agents.iter().map(|agent| {
            let id = agent["agent_id"].as_str().unwrap_or_default();
            let agent_tasks = tasks.iter().filter(|task| task["agent_id"] == id).collect::<Vec<_>>();
            let task_completed = agent_tasks.iter().filter(|task| task["status"] == "completed").count();
            let task_failed = agent_tasks.iter().filter(|task| task["status"] == "failed").count();
            let task_interrupted = agent_tasks.iter().filter(|task| task["status"] == "interrupted").count();
            let agent_terminal = agent_tasks.iter().filter(|task| ["completed", "failed", "cancelled", "interrupted"].contains(&task["status"].as_str().unwrap_or_default())).count();
            let metrics = &agent["metrics"];
            let finished = metrics["tools_finished"].as_u64().unwrap_or(0);
            let failures = metrics["tool_failures"].as_u64().unwrap_or(0);
            json!({
                "agent_id": id,
                "name": agent["name"],
                "provider": agent["provider"],
                "model": agent["model"],
                "tasks_sampled": agent_tasks.len(),
                "completed": task_completed,
                "failed": task_failed,
                "interrupted": task_interrupted,
                "completion_rate": (agent_terminal > 0).then_some(task_completed as f64 / agent_terminal as f64),
                "tool_success_rate": (finished > 0).then_some(finished.saturating_sub(failures) as f64 / finished as f64),
                "total_tokens": metrics["total_tokens"],
            })
        }).collect::<Vec<_>>();
        Ok(serde_json::to_string_pretty(&json!({
            "agent_id": agent_id,
            "health": health,
            "reliability_score": score,
            "sample": { "tasks": task_count, "terminal_tasks": terminal, "tool_calls_started": tool_started, "tool_calls_finished": tool_finished },
            "task_outcomes": { "completed": completed, "failed": failed, "interrupted": interrupted, "cancelled": cancelled, "completion_rate": completion_rate },
            "tool_reliability": { "successful": tool_finished.saturating_sub(tool_failures), "failed": tool_failures, "success_rate": tool_success_rate },
            "total_tokens": tokens,
            "model_comparison": model_comparison,
            "limits": metrics["limits"],
            "interpretation": "Operational reliability only. This score does not measure answer correctness, code quality, or benchmark performance. Small samples are inconclusive."
        }))?)
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{AgentConfiguration, AgentKind, TaskStatus};

    fn append_agent_event(store: &RuntimeStore, agent_id: &str, task_id: &str, event: Value) {
        store
            .append_event(Some(agent_id), Some(task_id), "task.agent_event", event)
            .unwrap();
    }

    #[test]
    fn aggregates_usage_tool_failures_durations_and_task_status() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agent = store
            .create_agent(
                "Metrics agent",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Run a measured task", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({
                "type":"Usage", "data":{"prompt_tokens":100,"completion_tokens":40,"total_tokens":140,"cost_usd":0.0125}
            }),
        );
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({
                "type":"ToolStarted", "data":{"name":"read_file","input":"{}"}
            }),
        );
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({
                "type":"ToolFinished", "data":{"name":"read_file","duration_ms":23,"failed":false,"diff":null}
            }),
        );
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({
                "type":"ToolStarted", "data":{"name":"run_command","input":"{}"}
            }),
        );
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({
                "type":"ToolFinished", "data":{"name":"run_command","duration_ms":71,"failed":true,"diff":null}
            }),
        );
        store
            .transition_task(
                &task.id,
                TaskStatus::Failed,
                Some("expected test failure".into()),
                None,
            )
            .unwrap();

        let output = ExecutionMetrics::new(store.clone()).execute("{}").unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        let agent_metrics = &value["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["agent_id"] == agent.id)
            .unwrap()["metrics"];
        assert_eq!(agent_metrics["prompt_tokens"], 100);
        assert_eq!(agent_metrics["completion_tokens"], 40);
        assert_eq!(agent_metrics["total_tokens"], 140);
        assert_eq!(agent_metrics["cost_usd"], 0.0125);
        assert_eq!(agent_metrics["tools_started"], 2);
        assert_eq!(agent_metrics["tools_finished"], 2);
        assert_eq!(agent_metrics["tool_duration_ms"], 94);
        assert_eq!(agent_metrics["tool_failures"], 1);

        let task_metrics = &value["tasks"][0];
        assert_eq!(task_metrics["task_id"], task.id);
        assert_eq!(task_metrics["status"], "failed");
        assert!(task_metrics["duration_ms"].as_u64().is_some());
        assert!(task_metrics["execution_duration_ms"].as_u64().is_some());
        assert_eq!(task_metrics["metrics"]["total_tokens"], 140);
        assert_eq!(task_metrics["metrics"]["tool_failures"], 1);
    }

    #[test]
    fn filters_agent_and_bounds_task_limit() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let first = store
            .create_agent(
                "First",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let second = store
            .create_agent(
                "Second",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let first_task = store
            .create_task(&first.id, "First task", None, None, false, None)
            .unwrap();
        store
            .create_task(&first.id, "Second task", None, None, false, None)
            .unwrap();
        store
            .create_task(&second.id, "Other agent task", None, None, false, None)
            .unwrap();

        let input = json!({"agent_id":first.id,"task_limit":999}).to_string();
        let value: Value =
            serde_json::from_str(&ExecutionMetrics::new(store).execute(&input).unwrap()).unwrap();
        assert_eq!(value["agents"].as_array().unwrap().len(), 1);
        assert_eq!(value["agents"][0]["agent_id"], first.id);
        assert_eq!(value["tasks"].as_array().unwrap().len(), 2);
        assert_eq!(value["limits"]["task_limit"], MAX_TASK_LIMIT);
        assert!(
            value["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task["task_id"] == first_task.id)
        );
    }

    #[test]
    fn evaluation_scorecard_reports_task_and_tool_reliability() {
        let store = RuntimeStore::open_in_memory_for_tests();
        let agent = store
            .create_agent(
                "Evaluator",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Successful task", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({"type":"ToolStarted","data":{"name":"read_file","input":"{}"}}),
        );
        append_agent_event(
            &store,
            &agent.id,
            &task.id,
            json!({"type":"ToolFinished","data":{"name":"read_file","duration_ms":12,"failed":false}}),
        );
        store
            .transition_task(&task.id, TaskStatus::Completed, None, None)
            .unwrap();
        let output = AgentEvaluation::new(store)
            .execute(&json!({"agent_id":agent.id.clone()}).to_string())
            .unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["health"], "developing");
        assert_eq!(value["reliability_score"], 100);
        assert_eq!(value["task_outcomes"]["completion_rate"], 1.0);
        assert_eq!(value["tool_reliability"]["success_rate"], 1.0);
        assert_eq!(value["model_comparison"][0]["agent_id"], agent.id);
        assert_eq!(value["model_comparison"][0]["tasks_sampled"], 1);
    }
}
