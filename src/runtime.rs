//! Persistent runtime domain and storage for Code and Teammates.
//!
//! The runtime is deliberately independent of any UI transport. Clients
//! manipulate stable agent/task identities; execution workers are supervised
//! by the core process and are never owned by a client connection.

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

const SCHEMA_VERSION: i64 = 2;
pub const GLOBAL_AGENT_ID: &str = "agent_global";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn unix_now() -> i64 {
    now()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Code,
    Teammates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Created,
    Idle,
    Active,
    Waiting,
    Paused,
    Recovering,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    WaitingForApproval,
    WaitingForInput,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionProfile {
    /// Explicitly permitted tools. An empty list does not mean unrestricted.
    pub allowed_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub auto_approve_read_only: bool,
    pub require_approval_for_mutations: bool,
    pub allow_network: bool,
}

impl Default for PermissionProfile {
    fn default() -> Self {
        Self {
            allowed_tools: vec![
                "read_file".into(),
                "list_directory".into(),
                "search_files".into(),
                "git_status".into(),
                "git_diff".into(),
                "find_symbols".into(),
                "find_references".into(),
                "git_log".into(),
                "git_show".into(),
                "git_blame".into(),
                "git_branches".into(),
                "file_metadata".into(),
                "find_todos".into(),
                "count_lines".into(),
                "inspect_project".into(),
                "list_skills".into(),
                "load_skill".into(),
            ],
            denied_tools: Vec::new(),
            auto_approve_read_only: true,
            require_approval_for_mutations: true,
            allow_network: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    RequireApproval,
    Deny,
}

impl PermissionProfile {
    pub fn decide(&self, tool: &str, mutating: bool) -> PermissionDecision {
        let known_read_only = matches!(
            tool,
            "read_file"
                | "list_directory"
                | "search_files"
                | "git_status"
                | "git_diff"
                | "find_symbols"
                | "find_references"
                | "git_log"
                | "git_show"
                | "git_blame"
                | "git_branches"
                | "file_metadata"
                | "find_todos"
                | "count_lines"
                | "inspect_project"
                | "list_skills"
                | "load_skill"
                | "web_search"
                | "fetch_url"
        );
        self.decide_with_capabilities(
            tool,
            mutating,
            matches!(tool, "web_search" | "fetch_url" | "http_request"),
            known_read_only && !mutating,
        )
    }

    pub fn decide_with_capabilities(
        &self,
        tool: &str,
        mutating: bool,
        requires_network: bool,
        read_only: bool,
    ) -> PermissionDecision {
        if self.denied_tools.iter().any(|item| item == tool) {
            return PermissionDecision::Deny;
        }
        // Network policy is an absolute boundary, not an ordinary default.
        // An explicit tool allow must never override `allow_network = false`.
        if !self.allow_network && requires_network {
            return PermissionDecision::Deny;
        }
        // These two tools are confined to the current teammate's own memory
        // and its explicitly inherited shared scopes; they never access the
        // filesystem, shell, or network and should not interrupt every task
        // with an approval prompt.
        if matches!(tool, "save_memory" | "recall_memory") {
            return PermissionDecision::Allow;
        }
        if tool == "http_request" && mutating && self.require_approval_for_mutations {
            return PermissionDecision::RequireApproval;
        }
        if self.allowed_tools.iter().any(|item| item == tool) {
            return PermissionDecision::Allow;
        }
        if mutating && self.require_approval_for_mutations {
            return PermissionDecision::RequireApproval;
        }
        if !mutating && read_only && self.auto_approve_read_only {
            return PermissionDecision::Allow;
        }
        PermissionDecision::RequireApproval
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfiguration {
    pub system_prompt: String,
    /// Stable mascot choice shared by the desktop, dashboard and Ink clients.
    pub mascot: String,
    /// Opt in to the Global teammate's shared skills, memory and defaults.
    pub use_global_context: bool,
    /// Skills explicitly attached to this agent and loaded into each task context.
    pub skills: Vec<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub workspace: Option<String>,
    pub allowed_workspaces: Vec<String>,
    pub enabled_tools: Vec<String>,
    /// Explicit Teammates network preference. `None` keeps the personal-agent
    /// default (enabled) while preserving older stored configurations.
    pub network_access: Option<bool>,
    pub permissions: PermissionProfile,
    /// Current supervisor policy is one active task per core process until
    /// workspace leases/isolation are available. Keep this explicit so a
    /// larger configured value is never silently ignored.
    pub max_concurrent_tasks: usize,
    pub recover_interrupted_tasks: bool,
}

impl Default for AgentConfiguration {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            mascot: "auto".into(),
            use_global_context: true,
            skills: Vec::new(),
            provider: None,
            model: None,
            endpoint: None,
            workspace: None,
            allowed_workspaces: Vec::new(),
            enabled_tools: Vec::new(),
            network_access: None,
            permissions: PermissionProfile::default(),
            max_concurrent_tasks: 1,
            recover_interrupted_tasks: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub kind: AgentKind,
    pub status: AgentStatus,
    pub configuration: AgentConfiguration,
    pub configuration_revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub request: String,
    pub status: TaskStatus,
    pub workspace: Option<String>,
    pub checkpoint: Option<serde_json::Value>,
    pub error: Option<String>,
    pub attempt: u32,
    pub recover_on_restart: bool,
    pub schedule_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    User,
    Agent,
    Project,
    Conversation,
    Task,
}

impl MemoryScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::Project => "project",
            Self::Conversation => "conversation",
            Self::Task => "task",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub content: String,
    pub source: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    pub tool: String,
    pub arguments: String,
    pub status: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRecord {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub prompt: String,
    /// Unix timestamp for one-shot schedules.
    pub run_at: Option<i64>,
    /// Recurrence interval in seconds. `None` means one-shot.
    pub every_seconds: Option<u64>,
    pub enabled: bool,
    pub next_run_at: i64,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub sequence: i64,
    pub event_id: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GatewayCommand {
    Cancel,
    Confirm { approval_id: String, allowed: bool },
    AnswerQuestion { question_id: String, answer: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGatewayCommand {
    pub id: String,
    pub task_id: String,
    pub command: GatewayCommand,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct RuntimeStore {
    connection: Arc<Mutex<Connection>>,
}

impl RuntimeStore {
    pub fn open_default() -> Result<Self> {
        let directory = crate::config::config_dir();
        std::fs::create_dir_all(&directory)
            .with_context(|| format!("creating runtime data directory {}", directory.display()))?;
        Self::open(directory.join("runtime.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path.as_ref())
            .with_context(|| format!("opening runtime database {}", path.as_ref().display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path.as_ref(), std::fs::Permissions::from_mode(0o600))?;
        }
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.migrate()?;
        store.ensure_global_agent()?;
        Ok(store)
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        let connection = Connection::open_in_memory().unwrap();
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        store.migrate().unwrap();
        store.ensure_global_agent().unwrap();
        store
    }

    #[cfg(test)]
    pub(crate) fn open_in_memory_for_tests() -> Self {
        Self::in_memory()
    }

    fn ensure_global_agent(&self) -> Result<()> {
        if self.get_agent(GLOBAL_AGENT_ID)?.is_some() {
            return Ok(());
        }
        let timestamp = now();
        let mut configuration = AgentConfiguration {
            use_global_context: false,
            ..AgentConfiguration::default()
        };
        enable_teammate_network_tools(&mut configuration);
        let db = self.db()?;
        db.execute(
            "INSERT OR IGNORE INTO agents (id,name,description,kind,status,configuration,configuration_revision,created_at,updated_at) VALUES (?1,'Global','Shared defaults for teammates','teammates','idle',?2,1,?3,?3)",
            params![GLOBAL_AGENT_ID, serde_json::to_string(&configuration)?, timestamp],
        )?;
        drop(db);
        self.append_event(
            Some(GLOBAL_AGENT_ID),
            None,
            "agent.global_initialized",
            serde_json::json!({"id": GLOBAL_AGENT_ID}),
        )?;
        Ok(())
    }

    fn db(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow!("runtime database lock poisoned"))
    }

    fn migrate(&self) -> Result<()> {
        let db = self.db()?;
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            bail!(
                "runtime database schema {version} is newer than this Luma build supports ({SCHEMA_VERSION})"
            );
        }
        if version == 0 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE agents (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT,
                    kind TEXT NOT NULL, status TEXT NOT NULL, configuration TEXT NOT NULL,
                    configuration_revision INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                    title TEXT NOT NULL, request TEXT NOT NULL, status TEXT NOT NULL, workspace TEXT,
                    checkpoint TEXT, error TEXT, attempt INTEGER NOT NULL DEFAULT 0,
                    recover_on_restart INTEGER NOT NULL DEFAULT 0, schedule_id TEXT,
                    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );
                CREATE INDEX tasks_agent_updated ON tasks(agent_id, updated_at DESC);
                CREATE INDEX tasks_status_created ON tasks(status, created_at);
                CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY, scope TEXT NOT NULL, scope_id TEXT NOT NULL,
                    content TEXT NOT NULL, source TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );
                CREATE INDEX memory_scope_lookup ON memory_entries(scope, scope_id, updated_at DESC);
                CREATE TABLE schedules (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                    title TEXT NOT NULL, prompt TEXT NOT NULL, run_at INTEGER, every_seconds INTEGER,
                    enabled INTEGER NOT NULL, next_run_at INTEGER NOT NULL, last_run_at INTEGER, created_at INTEGER NOT NULL
                );
                CREATE INDEX schedules_due ON schedules(enabled, next_run_at);
                CREATE TABLE approvals (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
                    task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE, tool TEXT NOT NULL,
                    arguments TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
                    created_at INTEGER NOT NULL, resolved_at INTEGER
                );
                CREATE TABLE events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE,
                    agent_id TEXT, task_id TEXT, event_type TEXT NOT NULL,
                    payload TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                CREATE INDEX events_agent_sequence ON events(agent_id, sequence);
                PRAGMA user_version = 1;
                COMMIT;"
            )?;
        }
        if version < 2 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE gateway_commands (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    command TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    processed_at INTEGER
                );
                CREATE INDEX gateway_commands_pending ON gateway_commands(task_id, processed_at, created_at);
                PRAGMA user_version = 2;
                COMMIT;",
            )?;
        }
        Ok(())
    }

    pub fn create_agent(
        &self,
        name: &str,
        description: Option<String>,
        kind: AgentKind,
        mut configuration: AgentConfiguration,
    ) -> Result<AgentRecord> {
        let name = name.trim();
        if name.is_empty() || name.len() > 120 {
            bail!("agent name must contain between 1 and 120 characters");
        }
        if configuration.max_concurrent_tasks != 1 {
            bail!(
                "max_concurrent_tasks must be 1; the supervisor serializes tasks until workspace isolation is available"
            );
        }
        if kind == AgentKind::Teammates {
            enable_teammate_network_tools(&mut configuration);
        }
        let timestamp = now();
        let record = AgentRecord {
            id: new_id("agent"),
            name: name.into(),
            description,
            kind,
            status: AgentStatus::Idle,
            configuration,
            configuration_revision: 1,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let db = self.db()?;
        db.execute("INSERT INTO agents (id,name,description,kind,status,configuration,configuration_revision,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![record.id, record.name, record.description, serde_json::to_string(&record.kind)?, serde_json::to_string(&record.status)?, serde_json::to_string(&record.configuration)?, record.configuration_revision as i64, timestamp, timestamp])?;
        drop(db);
        self.append_event(
            Some(&record.id),
            None,
            "agent.created",
            serde_json::to_value(&record)?,
        )?;
        Ok(record)
    }

    pub fn get_agent(&self, id: &str) -> Result<Option<AgentRecord>> {
        let db = self.db()?;
        db.query_row("SELECT id,name,description,kind,status,configuration,configuration_revision,created_at,updated_at FROM agents WHERE id=?1", [id], |row| {
            let kind: String = row.get(3)?; let status: String = row.get(4)?; let configuration: String = row.get(5)?;
            Ok(AgentRecord { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, kind: from_json_or_plain_string(&kind).map_err(to_sql_error)?, status: from_json_or_plain_string(&status).map_err(to_sql_error)?, configuration: serde_json::from_str(&configuration).map_err(to_sql_error)?, configuration_revision: row.get::<_, i64>(6)? as u64, created_at: row.get(7)?, updated_at: row.get(8)? })
        }).optional().map_err(Into::into)
    }

    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let db = self.db()?;
        let mut statement = db.prepare("SELECT id,name,description,kind,status,configuration,configuration_revision,created_at,updated_at FROM agents ORDER BY created_at ASC")?;
        let rows = statement.query_map([], |row| {
            let kind: String = row.get(3)?;
            let status: String = row.get(4)?;
            let configuration: String = row.get(5)?;
            Ok(AgentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                kind: from_json_or_plain_string(&kind).map_err(to_sql_error)?,
                status: from_json_or_plain_string(&status).map_err(to_sql_error)?,
                configuration: serde_json::from_str(&configuration).map_err(to_sql_error)?,
                configuration_revision: row.get::<_, i64>(6)? as u64,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Delete an agent and its dependent tasks, approvals and schedules.
    /// Active work must be stopped first so a worker cannot outlive its identity.
    pub fn delete_agent(&self, id: &str) -> Result<bool> {
        if id == GLOBAL_AGENT_ID {
            bail!("the Global teammate is a protected system identity and cannot be deleted");
        }
        let db = self.db()?;
        let agent = db
            .query_row("SELECT name FROM agents WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        let Some(name) = agent else { return Ok(false) };
        let active: i64 = db.query_row(
            "SELECT COUNT(*) FROM tasks WHERE agent_id=?1 AND json_extract(status, '$') IN ('queued','running','waiting_for_approval','waiting_for_input','paused')",
            [id],
            |row| row.get(0),
        )?;
        if active > 0 {
            bail!("cannot delete {name} while it has active tasks; cancel or finish them first");
        }
        db.execute_batch("BEGIN IMMEDIATE;")?;
        let result = (|| -> Result<()> {
            db.execute(
                "DELETE FROM memory_entries WHERE scope='agent' AND scope_id=?1",
                [id],
            )?;
            db.execute("DELETE FROM agents WHERE id=?1", [id])?;
            Ok(())
        })();
        match result {
            Ok(()) => db.execute_batch("COMMIT;")?,
            Err(error) => {
                let _ = db.execute_batch("ROLLBACK;");
                return Err(error);
            }
        }
        drop(db);
        self.append_event(
            None,
            None,
            "agent.deleted",
            serde_json::json!({"id":id,"name":name}),
        )?;
        Ok(true)
    }

    pub fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        configuration: Option<AgentConfiguration>,
    ) -> Result<AgentRecord> {
        let mut agent = self
            .get_agent(id)?
            .ok_or_else(|| anyhow!("agent not found: {id}"))?;
        if let Some(name) = name {
            let name = name.trim();
            if name.is_empty() || name.len() > 120 {
                bail!("invalid agent name");
            }
            agent.name = name.into();
        }
        if let Some(description) = description {
            agent.description = Some(description);
        }
        if let Some(configuration) = configuration {
            if configuration.max_concurrent_tasks != 1 {
                bail!(
                    "max_concurrent_tasks must be 1; the supervisor serializes tasks until workspace isolation is available"
                );
            }
            agent.configuration = configuration;
            agent.configuration_revision += 1;
        }
        agent.updated_at = now();
        let db = self.db()?;
        db.execute("UPDATE agents SET name=?2,description=?3,configuration=?4,configuration_revision=?5,updated_at=?6 WHERE id=?1", params![id, agent.name, agent.description, serde_json::to_string(&agent.configuration)?, agent.configuration_revision as i64, agent.updated_at])?;
        drop(db);
        self.append_event(
            Some(id),
            None,
            "agent.updated",
            serde_json::to_value(&agent)?,
        )?;
        Ok(agent)
    }

    pub fn set_agent_status(&self, id: &str, status: AgentStatus) -> Result<()> {
        let timestamp = now();
        let mut db = self.db()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current_status: String = transaction
            .query_row("SELECT status FROM agents WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| anyhow!("agent not found: {id}"))?;
        let current_status: AgentStatus = from_json_or_plain_string(&current_status)?;
        let active_tasks: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM tasks WHERE agent_id=?1 AND json_extract(status, '$') IN ('running','waiting_for_approval','waiting_for_input')",
            [id],
            |row| row.get(0),
        )?;
        if current_status == AgentStatus::Stopped && status != AgentStatus::Stopped {
            bail!("a stopped agent cannot be resumed; create a new agent instead");
        }
        if status == AgentStatus::Idle && active_tasks > 0 {
            bail!("cancel or finish active tasks before setting this agent to idle");
        }
        if matches!(status, AgentStatus::Active | AgentStatus::Waiting) && active_tasks == 0 {
            bail!("an agent can only become active or waiting while it owns an active task");
        }
        if current_status == AgentStatus::Paused
            && matches!(status, AgentStatus::Active | AgentStatus::Waiting)
        {
            bail!("resume the agent before starting work");
        }
        if matches!(status, AgentStatus::Paused | AgentStatus::Stopped) && active_tasks > 0 {
            let task_ids = {
                let mut statement = transaction.prepare(
                    "SELECT id FROM tasks WHERE agent_id=?1 AND json_extract(status, '$') IN ('running','waiting_for_approval','waiting_for_input')",
                )?;
                let rows = statement.query_map([id], |row| row.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let command = serde_json::to_string(&GatewayCommand::Cancel)?;
            for task_id in task_ids {
                transaction.execute(
                    "INSERT INTO gateway_commands (id,task_id,command,created_at,processed_at) VALUES (?1,?2,?3,?4,NULL)",
                    params![new_id("gateway_cmd"), task_id, command, timestamp],
                )?;
            }
        }
        if status == AgentStatus::Stopped {
            transaction.execute("UPDATE schedules SET enabled=0 WHERE agent_id=?1", [id])?;
        }
        let changed = transaction.execute(
            "UPDATE agents SET status=?2,updated_at=?3 WHERE id=?1",
            params![id, serde_json::to_string(&status)?, timestamp],
        )?;
        if changed == 0 {
            bail!("agent not found: {id}");
        }
        transaction.commit()?;
        drop(db);
        self.append_event(
            Some(id),
            None,
            "agent.status_changed",
            serde_json::json!({"status":status,"updated_at":timestamp}),
        )?;
        Ok(())
    }

    pub fn create_task(
        &self,
        agent_id: &str,
        request: &str,
        title: Option<String>,
        workspace: Option<String>,
        recover_on_restart: bool,
        schedule_id: Option<String>,
    ) -> Result<TaskRecord> {
        let agent = self
            .get_agent(agent_id)?
            .ok_or_else(|| anyhow!("agent not found: {agent_id}"))?;
        if agent.status == AgentStatus::Stopped {
            bail!("cannot queue work for a stopped agent");
        }
        let request = request.trim();
        if request.is_empty() || request.len() > 100_000 {
            bail!("task request must contain between 1 and 100000 characters");
        }
        if let Some(path) = workspace.as_deref() {
            validate_workspace(path, &agent.configuration)?;
        }
        let timestamp = now();
        let record = TaskRecord {
            id: new_id("task"),
            agent_id: agent_id.into(),
            title: title.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
                request
                    .lines()
                    .next()
                    .unwrap_or("New task")
                    .chars()
                    .take(100)
                    .collect()
            }),
            request: request.into(),
            status: TaskStatus::Queued,
            workspace,
            checkpoint: None,
            error: None,
            attempt: 0,
            recover_on_restart,
            schedule_id,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let db = self.db()?;
        db.execute("INSERT INTO tasks (id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,0,?7,?8,?9,?9)", params![record.id, record.agent_id, record.title, record.request, serde_json::to_string(&record.status)?, record.workspace, recover_on_restart, record.schedule_id, timestamp])?;
        drop(db);
        self.append_event(
            Some(agent_id),
            Some(&record.id),
            "task.queued",
            serde_json::to_value(&record)?,
        )?;
        Ok(record)
    }

    pub fn get_task(&self, id: &str) -> Result<Option<TaskRecord>> {
        let db = self.db()?;
        db.query_row("SELECT id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at FROM tasks WHERE id=?1", [id], task_from_row).optional().map_err(Into::into)
    }

    pub fn rename_task(&self, id: &str, title: &str) -> Result<TaskRecord> {
        let title = title.trim();
        if title.is_empty() || title.len() > 120 {
            bail!("chat title must contain between 1 and 120 characters");
        }
        let task = self
            .get_task(id)?
            .ok_or_else(|| anyhow!("task not found: {id}"))?;
        let updated_at = now();
        let db = self.db()?;
        db.execute(
            "UPDATE tasks SET title=?2, updated_at=?3 WHERE id=?1",
            params![id, title, updated_at],
        )?;
        drop(db);
        self.get_task(&task.id)?
            .ok_or_else(|| anyhow!("task disappeared after rename: {id}"))
    }

    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let task = self
            .get_task(id)?
            .ok_or_else(|| anyhow!("task not found: {id}"))?;
        if matches!(
            task.status,
            TaskStatus::Queued
                | TaskStatus::Running
                | TaskStatus::WaitingForApproval
                | TaskStatus::WaitingForInput
                | TaskStatus::Paused
        ) {
            bail!("cannot delete an active chat; cancel its task first");
        }
        let mut db = self.db()?;
        let transaction = db.transaction()?;
        transaction.execute(
            "DELETE FROM memory_entries WHERE scope='task' AND scope_id=?1",
            [id],
        )?;
        let deleted = transaction.execute("DELETE FROM tasks WHERE id=?1", [id])? > 0;
        transaction.commit()?;
        drop(db);
        if deleted {
            self.append_event(
                Some(&task.agent_id),
                None,
                "task.deleted",
                serde_json::json!({"id": id}),
            )?;
        }
        Ok(deleted)
    }

    pub fn list_tasks(&self, agent_id: Option<&str>, limit: usize) -> Result<Vec<TaskRecord>> {
        let db = self.db()?;
        let limit = limit.clamp(1, 500) as i64;
        let mut statement = if agent_id.is_some() {
            db.prepare("SELECT id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at FROM tasks WHERE agent_id=?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2")?
        } else {
            db.prepare("SELECT id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at FROM tasks ORDER BY created_at DESC, rowid DESC LIMIT ?1")?
        };
        let rows = if let Some(agent_id) = agent_id {
            statement.query_map(params![agent_id, limit], task_from_row)?
        } else {
            statement.query_map([limit], task_from_row)?
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn transition_task(
        &self,
        id: &str,
        status: TaskStatus,
        error: Option<String>,
        checkpoint: Option<serde_json::Value>,
    ) -> Result<TaskRecord> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow!("task not found: {id}"))?;
        if !valid_task_transition(task.status, status) {
            bail!("invalid task transition: {:?} -> {:?}", task.status, status);
        }
        task.status = status;
        task.error = error;
        if checkpoint.is_some() {
            task.checkpoint = checkpoint;
        }
        if status == TaskStatus::Running {
            task.attempt = task.attempt.saturating_add(1);
        }
        task.updated_at = now();
        let db = self.db()?;
        db.execute("UPDATE tasks SET status=?2,error=?3,checkpoint=?4,attempt=?5,updated_at=?6 WHERE id=?1", params![id, serde_json::to_string(&task.status)?, task.error, task.checkpoint.as_ref().map(serde_json::to_string).transpose()?, task.attempt, task.updated_at])?;
        drop(db);
        let event = match status {
            TaskStatus::Running => "task.started",
            TaskStatus::WaitingForApproval => "task.waiting_for_approval",
            TaskStatus::WaitingForInput => "task.waiting_for_input",
            TaskStatus::Paused => "task.paused",
            TaskStatus::Completed => "task.completed",
            TaskStatus::Failed => "task.failed",
            TaskStatus::Cancelled => "task.cancelled",
            TaskStatus::Interrupted => "task.interrupted",
            TaskStatus::Queued => "task.queued",
        };
        self.append_event(
            Some(&task.agent_id),
            Some(id),
            event,
            serde_json::to_value(&task)?,
        )?;
        if matches!(
            status,
            TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::Interrupted
        ) {
            self.expire_pending_approvals(id)?;
            let db = self.db()?;
            db.execute("DELETE FROM gateway_commands WHERE task_id=?1", [id])?;
        }
        Ok(task)
    }

    pub fn recover_after_restart(&self) -> Result<Vec<TaskRecord>> {
        let running = self
            .list_tasks(None, 500)?
            .into_iter()
            .filter(|task| {
                task.status == TaskStatus::Running
                    || task.status == TaskStatus::WaitingForApproval
                    || task.status == TaskStatus::WaitingForInput
            })
            .collect::<Vec<_>>();
        let mut recovered = Vec::new();
        for task in running {
            let interrupted = self.transition_task(&task.id, TaskStatus::Interrupted, Some("Runtime restarted before this task reached a terminal state; external side effects must be reconciled before retry.".into()), None)?;
            self.expire_pending_approvals(&task.id)?;
            // The worker does not yet persist an idempotent action cursor, so
            // restarting from the original request could repeat external side
            // effects. Recovery is reconciliation, never automatic replay.
            recovered.push(interrupted);
        }
        for agent in self.list_agents()? {
            if matches!(
                agent.status,
                AgentStatus::Active | AgentStatus::Waiting | AgentStatus::Recovering
            ) {
                self.set_agent_status(&agent.id, AgentStatus::Idle)?;
            }
        }
        self.append_event(
            None,
            None,
            "runtime.recovered",
            serde_json::json!({"tasks_reconciled":recovered.len()}),
        )?;
        Ok(recovered)
    }

    pub fn append_event(
        &self,
        agent_id: Option<&str>,
        task_id: Option<&str>,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<RuntimeEvent> {
        let event_id = new_id("evt");
        let created_at = now();
        let db = self.db()?;
        db.execute("INSERT INTO events (event_id,agent_id,task_id,event_type,payload,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![event_id, agent_id, task_id, event_type, serde_json::to_string(&payload)?, created_at])?;
        let sequence = db.last_insert_rowid();
        Ok(RuntimeEvent {
            sequence,
            event_id,
            agent_id: agent_id.map(str::to_owned),
            task_id: task_id.map(str::to_owned),
            event_type: event_type.into(),
            payload,
            created_at,
        })
    }

    pub fn create_approval(
        &self,
        agent_id: &str,
        task_id: &str,
        tool: &str,
        arguments: &str,
    ) -> Result<ApprovalRecord> {
        let task = self
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("task not found: {task_id}"))?;
        if task.agent_id != agent_id {
            bail!("approval agent/task mismatch");
        }
        if task.status != TaskStatus::WaitingForApproval {
            bail!("approval can only be created for a task waiting for approval");
        }
        let record = ApprovalRecord {
            id: new_id("approval"),
            agent_id: agent_id.into(),
            task_id: task_id.into(),
            tool: tool.into(),
            arguments: arguments.into(),
            status: "pending".into(),
            created_at: now(),
            resolved_at: None,
        };
        let db = self.db()?;
        db.execute("INSERT INTO approvals (id,agent_id,task_id,tool,arguments,status,created_at,resolved_at) VALUES (?1,?2,?3,?4,?5,'pending',?6,NULL)", params![record.id, record.agent_id, record.task_id, record.tool, record.arguments, record.created_at])?;
        drop(db);
        self.append_event(
            Some(agent_id),
            Some(task_id),
            "approval.requested",
            serde_json::to_value(&record)?,
        )?;
        Ok(record)
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<ApprovalRecord>> {
        let db = self.db()?;
        let mut statement = db.prepare("SELECT id,agent_id,task_id,tool,arguments,status,created_at,resolved_at FROM approvals WHERE status='pending' ORDER BY created_at ASC")?;
        let rows = statement.query_map([], |row| {
            Ok(ApprovalRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                task_id: row.get(2)?,
                tool: row.get(3)?,
                arguments: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
                resolved_at: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn resolve_approval(&self, id: &str, allowed: bool) -> Result<ApprovalRecord> {
        let timestamp = now();
        let status = if allowed { "approved" } else { "denied" };
        let mut db = self.db()?;
        let transaction = db.transaction()?;
        let pending_task_id = transaction
            .query_row(
                "SELECT task_id FROM approvals WHERE id=?1 AND status='pending'",
                [id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| anyhow!("approval not found or already resolved: {id}"))?;
        let task_status: String = transaction.query_row(
            "SELECT status FROM tasks WHERE id=?1",
            [&pending_task_id],
            |row| row.get(0),
        )?;
        if task_status != serde_json::to_string(&TaskStatus::WaitingForApproval)? {
            bail!("the task is no longer waiting for this approval");
        }
        let changed = transaction.execute(
            "UPDATE approvals SET status=?2,resolved_at=?3 WHERE id=?1 AND status='pending' AND task_id=?4",
            params![id, status, timestamp, pending_task_id],
        )?;
        if changed == 0 {
            bail!("approval not found or already resolved: {id}");
        }
        let record = transaction.query_row("SELECT id,agent_id,task_id,tool,arguments,status,created_at,resolved_at FROM approvals WHERE id=?1", [id], |row| Ok(ApprovalRecord { id: row.get(0)?, agent_id: row.get(1)?, task_id: row.get(2)?, tool: row.get(3)?, arguments: row.get(4)?, status: row.get(5)?, created_at: row.get(6)?, resolved_at: row.get(7)? }))?;
        let command = GatewayCommand::Confirm {
            approval_id: record.id.clone(),
            allowed,
        };
        transaction.execute(
            "INSERT INTO gateway_commands (id,task_id,command,created_at,processed_at) VALUES (?1,?2,?3,?4,NULL)",
            params![new_id("gateway_cmd"), record.task_id, serde_json::to_string(&command)?, timestamp],
        )?;
        transaction.commit()?;
        drop(db);
        self.append_event(
            Some(&record.agent_id),
            Some(&record.task_id),
            "approval.resolved",
            serde_json::to_value(&record)?,
        )?;
        Ok(record)
    }

    pub fn enqueue_gateway_cancel(&self, task_id: &str) -> Result<()> {
        let task = self
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("task not found: {task_id}"))?;
        if !matches!(
            task.status,
            TaskStatus::Running | TaskStatus::WaitingForApproval | TaskStatus::WaitingForInput
        ) {
            bail!("only active tasks can receive a gateway cancellation");
        }
        let command = GatewayCommand::Cancel;
        let db = self.db()?;
        db.execute(
            "INSERT INTO gateway_commands (id,task_id,command,created_at,processed_at) VALUES (?1,?2,?3,?4,NULL)",
            params![new_id("gateway_cmd"), task_id, serde_json::to_string(&command)?, now()],
        )?;
        Ok(())
    }

    pub fn answer_task_question(
        &self,
        task_id: &str,
        question_id: &str,
        answer: &str,
    ) -> Result<TaskRecord> {
        let answer = answer.trim();
        if answer.is_empty() || answer.len() > 8_000 {
            bail!("answer must contain 1–8000 characters");
        }
        let timestamp = now();
        let mut db = self.db()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = transaction.query_row("SELECT id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at FROM tasks WHERE id=?1", [task_id], task_from_row)?;
        if task.status != TaskStatus::WaitingForInput {
            bail!("the task is not waiting for an answer");
        }
        let agent_status: String = transaction.query_row(
            "SELECT status FROM agents WHERE id=?1",
            [&task.agent_id],
            |row| row.get(0),
        )?;
        let agent_status: AgentStatus = from_json_or_plain_string(&agent_status)?;
        if matches!(agent_status, AgentStatus::Paused | AgentStatus::Stopped) {
            bail!("resume the teammate before answering its question");
        }
        let pending_id = task
            .checkpoint
            .as_ref()
            .and_then(|value| value.get("question_id"))
            .and_then(serde_json::Value::as_str);
        if pending_id != Some(question_id) {
            bail!("question is no longer pending");
        }
        let changed = transaction.execute("UPDATE tasks SET status=?2,checkpoint=NULL,error=NULL,updated_at=?3 WHERE id=?1 AND status=?4", params![task_id, serde_json::to_string(&TaskStatus::Running)?, timestamp, serde_json::to_string(&TaskStatus::WaitingForInput)?])?;
        if changed == 0 {
            bail!("question is no longer pending");
        }
        let command = GatewayCommand::AnswerQuestion {
            question_id: question_id.to_owned(),
            answer: answer.to_owned(),
        };
        transaction.execute("INSERT INTO gateway_commands (id,task_id,command,created_at,processed_at) VALUES (?1,?2,?3,?4,NULL)", params![new_id("gateway_cmd"), task_id, serde_json::to_string(&command)?, timestamp])?;
        transaction.commit()?;
        drop(db);
        self.append_event(
            Some(&task.agent_id),
            Some(task_id),
            "task.resumed_with_answer",
            serde_json::json!({"question_id": question_id}),
        )?;
        self.get_task(task_id)?
            .ok_or_else(|| anyhow!("task disappeared after answering question"))
    }

    pub fn pending_gateway_commands(&self, task_id: &str) -> Result<Vec<PendingGatewayCommand>> {
        let db = self.db()?;
        let mut statement = db.prepare(
            "SELECT id,task_id,command,created_at FROM gateway_commands WHERE task_id=?1 AND processed_at IS NULL ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([task_id], |row| {
            let command: String = row.get(2)?;
            Ok(PendingGatewayCommand {
                id: row.get(0)?,
                task_id: row.get(1)?,
                command: serde_json::from_str(&command).map_err(to_sql_error)?,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn mark_gateway_command_processed(&self, id: &str) -> Result<()> {
        let db = self.db()?;
        db.execute(
            "UPDATE gateway_commands SET processed_at=?2 WHERE id=?1 AND processed_at IS NULL",
            params![id, now()],
        )?;
        Ok(())
    }

    /// Mark approvals belonging to a worker that disappeared during restart.
    /// They must not remain actionable after the process that could receive
    /// their confirmation has gone away.
    pub fn expire_pending_approvals(&self, task_id: &str) -> Result<usize> {
        let timestamp = now();
        let db = self.db()?;
        let changed = db.execute(
            "UPDATE approvals SET status='expired',resolved_at=?2 WHERE task_id=?1 AND status='pending'",
            params![task_id, timestamp],
        )?;
        drop(db);
        if changed > 0 {
            self.append_event(
                None,
                Some(task_id),
                "approval.expired",
                serde_json::json!({"count": changed}),
            )?;
        }
        Ok(changed)
    }

    pub fn events_since(&self, after_sequence: i64, limit: usize) -> Result<Vec<RuntimeEvent>> {
        let db = self.db()?;
        let mut statement = db.prepare("SELECT sequence,event_id,agent_id,task_id,event_type,payload,created_at FROM events WHERE sequence>?1 ORDER BY sequence ASC LIMIT ?2")?;
        let rows = statement.query_map(
            params![after_sequence.max(0), limit.clamp(1, 1000) as i64],
            |row| {
                let payload: String = row.get(5)?;
                Ok(RuntimeEvent {
                    sequence: row.get(0)?,
                    event_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    task_id: row.get(3)?,
                    event_type: row.get(4)?,
                    payload: serde_json::from_str(&payload).map_err(to_sql_error)?,
                    created_at: row.get(6)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Return one task's persisted assistant text in event order. Unlike the
    /// general activity feed, chat reconstruction must not silently truncate
    /// a long-running conversation at an arbitrary global event limit.
    pub fn task_text_response(&self, task_id: &str) -> Result<String> {
        let db = self.db()?;
        let mut statement = db.prepare(
            "SELECT json_extract(payload, '$.data') FROM events WHERE task_id=?1 AND event_type='task.agent_event' AND json_extract(payload, '$.type')='TextDelta' ORDER BY sequence ASC",
        )?;
        let rows = statement.query_map([task_id], |row| row.get::<_, Option<String>>(0))?;
        let mut response = String::new();
        for delta in rows {
            if let Some(delta) = delta? {
                response.push_str(&delta);
            }
        }
        Ok(response)
    }

    /// Agent-scoped activity query. Filtering after a global LIMIT can hide
    /// an agent's events once other agents generate enough activity.
    pub fn events_for_agent(
        &self,
        agent_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<RuntimeEvent>> {
        let db = self.db()?;
        let mut statement = db.prepare(
            "SELECT sequence,event_id,agent_id,task_id,event_type,payload,created_at FROM events WHERE agent_id=?1 AND sequence>?2 ORDER BY sequence ASC LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![agent_id, after_sequence.max(0), limit.clamp(1, 1000) as i64],
            |row| {
                let payload: String = row.get(5)?;
                Ok(RuntimeEvent {
                    sequence: row.get(0)?,
                    event_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    task_id: row.get(3)?,
                    event_type: row.get(4)?,
                    payload: serde_json::from_str(&payload).map_err(to_sql_error)?,
                    created_at: row.get(6)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn add_memory(
        &self,
        scope: MemoryScope,
        scope_id: &str,
        content: &str,
        source: Option<String>,
    ) -> Result<MemoryEntry> {
        let content = content.trim();
        if scope_id.trim().is_empty() || content.is_empty() || content.len() > 100_000 {
            bail!("memory scope and content are required; content must be at most 100000 bytes");
        }
        if scope == MemoryScope::Agent && self.get_agent(scope_id)?.is_none() {
            bail!("memory agent does not exist");
        }
        let timestamp = now();
        let entry = MemoryEntry {
            id: new_id("mem"),
            scope,
            scope_id: scope_id.into(),
            content: content.into(),
            source,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let db = self.db()?;
        db.execute("INSERT INTO memory_entries (id,scope,scope_id,content,source,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)", params![entry.id, entry.scope.as_str(), entry.scope_id, entry.content, entry.source, timestamp])?;
        drop(db);
        self.append_event(
            (scope == MemoryScope::Agent).then_some(scope_id),
            None,
            "memory.created",
            serde_json::to_value(&entry)?,
        )?;
        Ok(entry)
    }

    pub fn list_memory(
        &self,
        scope: MemoryScope,
        scope_id: &str,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let db = self.db()?;
        let pattern = query.map(|q| format!("%{}%", q.replace('%', "\\%").replace('_', "\\_")));
        let mut statement = db.prepare("SELECT id,scope,scope_id,content,source,created_at,updated_at FROM memory_entries WHERE scope=?1 AND scope_id=?2 AND (?3 IS NULL OR content LIKE ?3 ESCAPE '\\') ORDER BY updated_at DESC LIMIT ?4")?;
        let rows = statement.query_map(
            params![
                scope.as_str(),
                scope_id,
                pattern,
                limit.clamp(1, 500) as i64
            ],
            |row| {
                let scope: String = row.get(1)?;
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    scope: serde_json::from_str(&format!("\"{scope}\"")).map_err(to_sql_error)?,
                    scope_id: row.get(2)?,
                    content: row.get(3)?,
                    source: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn delete_memory(&self, id: &str) -> Result<bool> {
        let db = self.db()?;
        Ok(db.execute("DELETE FROM memory_entries WHERE id=?1", [id])? > 0)
    }

    pub fn create_schedule(
        &self,
        agent_id: &str,
        title: &str,
        prompt: &str,
        run_at: Option<i64>,
        every_seconds: Option<u64>,
    ) -> Result<ScheduleRecord> {
        if self.get_agent(agent_id)?.is_none() {
            bail!("agent not found: {agent_id}");
        }
        if prompt.trim().is_empty() {
            bail!("scheduled prompt cannot be empty");
        }
        if every_seconds.is_some_and(|seconds| seconds < 60 || seconds > 31_536_000) {
            bail!("recurrence interval must be between 60 seconds and one year");
        }
        if run_at.is_none() && every_seconds.is_none() {
            bail!("a schedule needs a run_at time or recurrence interval");
        }
        let timestamp = now();
        let next_run_at =
            run_at.unwrap_or(timestamp.saturating_add(every_seconds.unwrap_or(60) as i64));
        let record = ScheduleRecord {
            id: new_id("sch"),
            agent_id: agent_id.into(),
            title: title.trim().into(),
            prompt: prompt.trim().into(),
            run_at,
            every_seconds,
            enabled: true,
            next_run_at,
            last_run_at: None,
            created_at: timestamp,
        };
        let db = self.db()?;
        db.execute("INSERT INTO schedules (id,agent_id,title,prompt,run_at,every_seconds,enabled,next_run_at,last_run_at,created_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,NULL,?8)", params![record.id, record.agent_id, record.title, record.prompt, record.run_at, record.every_seconds.map(|value| value as i64), record.next_run_at, timestamp])?;
        Ok(record)
    }

    pub fn list_schedules(&self, agent_id: Option<&str>) -> Result<Vec<ScheduleRecord>> {
        let db = self.db()?;
        let sql = if agent_id.is_some() {
            "SELECT id,agent_id,title,prompt,run_at,every_seconds,enabled,next_run_at,last_run_at,created_at FROM schedules WHERE agent_id=?1 ORDER BY next_run_at"
        } else {
            "SELECT id,agent_id,title,prompt,run_at,every_seconds,enabled,next_run_at,last_run_at,created_at FROM schedules ORDER BY next_run_at"
        };
        let mut statement = db.prepare(sql)?;
        let map = |row: &rusqlite::Row<'_>| {
            Ok(ScheduleRecord {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                title: row.get(2)?,
                prompt: row.get(3)?,
                run_at: row.get(4)?,
                every_seconds: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                enabled: row.get(6)?,
                next_run_at: row.get(7)?,
                last_run_at: row.get(8)?,
                created_at: row.get(9)?,
            })
        };
        let rows = if let Some(agent_id) = agent_id {
            statement.query_map([agent_id], map)?
        } else {
            statement.query_map([], map)?
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn due_schedules(&self, timestamp: i64) -> Result<Vec<ScheduleRecord>> {
        Ok(self
            .list_schedules(None)?
            .into_iter()
            .filter(|schedule| schedule.enabled && schedule.next_run_at <= timestamp)
            .collect())
    }

    /// Atomically claim a due occurrence and enqueue its task. A crash cannot
    /// leave the schedule unadvanced after the task was inserted (which would
    /// duplicate that occurrence on the next supervisor start).
    pub fn enqueue_due_schedule(&self, id: &str, timestamp: i64) -> Result<Option<TaskRecord>> {
        let schedule = self
            .list_schedules(None)?
            .into_iter()
            .find(|schedule| schedule.id == id)
            .ok_or_else(|| anyhow!("schedule not found: {id}"))?;
        if !schedule.enabled || schedule.next_run_at > timestamp {
            return Ok(None);
        }
        let agent = self
            .get_agent(&schedule.agent_id)?
            .ok_or_else(|| anyhow!("schedule agent not found: {}", schedule.agent_id))?;
        if agent.status == AgentStatus::Paused {
            return Ok(None);
        }

        let next_run_at = schedule
            .every_seconds
            .map(|interval| timestamp.saturating_add(interval as i64));
        let mut db = self.db()?;
        let transaction = db.transaction()?;
        let changed = transaction.execute(
            "UPDATE schedules SET enabled=?2,next_run_at=?3,last_run_at=?4 WHERE id=?1 AND enabled=1 AND next_run_at<=?4",
            params![id, next_run_at.is_some(), next_run_at.unwrap_or(timestamp), timestamp],
        )?;
        if changed == 0 {
            return Ok(None);
        }

        let task = if agent.status == AgentStatus::Stopped {
            None
        } else {
            let workspace = agent.configuration.workspace.clone();
            if let Some(path) = workspace.as_deref() {
                validate_workspace(path, &agent.configuration)?;
            }
            let task = TaskRecord {
                id: new_id("task"),
                agent_id: schedule.agent_id.clone(),
                title: if schedule.title.trim().is_empty() {
                    schedule
                        .prompt
                        .lines()
                        .next()
                        .unwrap_or("Scheduled task")
                        .chars()
                        .take(100)
                        .collect()
                } else {
                    schedule.title.clone()
                },
                request: schedule.prompt.clone(),
                status: TaskStatus::Queued,
                workspace,
                checkpoint: None,
                error: None,
                attempt: 0,
                recover_on_restart: false,
                schedule_id: Some(schedule.id.clone()),
                created_at: timestamp,
                updated_at: timestamp,
            };
            transaction.execute(
                "INSERT INTO tasks (id,agent_id,title,request,status,workspace,checkpoint,error,attempt,recover_on_restart,schedule_id,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,NULL,NULL,0,0,?7,?8,?8)",
                params![task.id, task.agent_id, task.title, task.request, serde_json::to_string(&task.status)?, task.workspace, task.schedule_id, timestamp],
            )?;
            let payload = serde_json::to_string(&task)?;
            transaction.execute(
                "INSERT INTO events (event_id,agent_id,task_id,event_type,payload,created_at) VALUES (?1,?2,?3,'task.queued',?4,?5)",
                params![new_id("evt"), task.agent_id, task.id, payload, timestamp],
            )?;
            Some(task)
        };

        transaction.commit()?;
        Ok(task)
    }

    pub fn delete_schedule(&self, id: &str) -> Result<bool> {
        let db = self.db()?;
        Ok(db.execute("DELETE FROM schedules WHERE id=?1", [id])? > 0)
    }

    pub fn set_schedule_enabled(&self, id: &str, enabled: bool) -> Result<ScheduleRecord> {
        let db = self.db()?;
        let changed = db.execute(
            "UPDATE schedules SET enabled=?2 WHERE id=?1",
            params![id, enabled as i64],
        )?;
        if changed == 0 {
            bail!("schedule not found: {id}");
        }
        drop(db);
        self.list_schedules(None)?
            .into_iter()
            .find(|schedule| schedule.id == id)
            .ok_or_else(|| anyhow!("schedule disappeared after updating: {id}"))
    }
}

fn enable_teammate_network_tools(configuration: &mut AgentConfiguration) {
    let enabled = configuration.network_access.unwrap_or(true);
    configuration.network_access = Some(enabled);
    configuration.permissions.allow_network = enabled;
    if enabled {
        for tool in ["web_search", "fetch_url"] {
            if !configuration
                .permissions
                .allowed_tools
                .iter()
                .any(|item| item == tool)
            {
                configuration
                    .permissions
                    .allowed_tools
                    .push(tool.to_owned());
            }
        }
    }
}

pub(crate) fn validate_workspace(path: &str, configuration: &AgentConfiguration) -> Result<()> {
    let canonical = PathBuf::from(path)
        .canonicalize()
        .with_context(|| format!("workspace does not exist: {path}"))?;
    if !canonical.is_dir() {
        bail!("workspace is not a directory");
    }
    let grants = if configuration.allowed_workspaces.is_empty() {
        // With no explicit grants, a task may use only the workspace the user
        // explicitly submitted. Falling back to the gateway process cwd makes
        // behavior depend on which UI launched the daemon (desktop, Ink, or a
        // shell in a nested directory), and can strand tasks in the queue.
        vec![
            configuration
                .workspace
                .clone()
                .unwrap_or_else(|| path.to_owned()),
        ]
    } else {
        configuration.allowed_workspaces.clone()
    };
    let allowed = grants
        .iter()
        .filter_map(|path| PathBuf::from(path).canonicalize().ok())
        .any(|root| canonical.starts_with(root));
    if !allowed {
        bail!("workspace is outside this agent's allowed workspace grants");
    }
    Ok(())
}

fn valid_task_transition(from: TaskStatus, to: TaskStatus) -> bool {
    use TaskStatus::*;
    if from == to {
        return true;
    }
    match from {
        // A queued task can fail before its worker starts (e.g. an unavailable
        // workspace or an unspawnable worker executable). Make that terminal
        // instead of retrying the same impossible task forever.
        Queued => matches!(to, Running | Failed | Cancelled),
        Running => matches!(
            to,
            WaitingForApproval
                | WaitingForInput
                | Paused
                | Completed
                | Failed
                | Cancelled
                | Interrupted
        ),
        WaitingForApproval | WaitingForInput => {
            matches!(to, Running | Paused | Cancelled | Interrupted)
        }
        Paused => matches!(to, Queued | Cancelled),
        Interrupted => matches!(to, Queued | Cancelled),
        Failed => matches!(to, Queued),
        Completed | Cancelled => false,
    }
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let status: String = row.get(4)?;
    let checkpoint: Option<String> = row.get(6)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        title: row.get(2)?,
        request: row.get(3)?,
        status: serde_json::from_str(&status).map_err(to_sql_error)?,
        workspace: row.get(5)?,
        checkpoint: checkpoint
            .map(|value| serde_json::from_str(&value).map_err(to_sql_error))
            .transpose()?,
        error: row.get(7)?,
        attempt: row.get(8)?,
        recover_on_restart: row.get(9)?,
        schedule_id: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn to_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

/// Accept the JSON-encoded enum representation used by current writes and the
/// unquoted TEXT representation used by the original Global seed. This keeps
/// databases created by that seed readable without destructive migration.
fn from_json_or_plain_string<T: DeserializeOwned>(value: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str(value)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(value.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_and_tasks_survive_store_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let agent;
        let task;
        {
            let store = RuntimeStore::open(&path).unwrap();
            agent = store
                .create_agent(
                    "Research",
                    None,
                    AgentKind::Teammates,
                    AgentConfiguration::default(),
                )
                .unwrap();
            task = store
                .create_task(&agent.id, "Review the project", None, None, false, None)
                .unwrap();
            store
                .transition_task(&task.id, TaskStatus::Running, None, None)
                .unwrap();
        }
        let store = RuntimeStore::open(&path).unwrap();
        assert_eq!(
            store.get_agent(&agent.id).unwrap().unwrap().name,
            "Research"
        );
        assert_eq!(
            store.get_task(&task.id).unwrap().unwrap().status,
            TaskStatus::Running
        );
    }

    #[test]
    fn teammate_chat_can_be_renamed_and_deleted_after_completion() {
        let directory = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(directory.path().join("runtime.sqlite3")).unwrap();
        let agent = store
            .create_agent(
                "Research",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Review the project", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Completed, None, None)
            .unwrap();
        let renamed = store.rename_task(&task.id, "Project review").unwrap();
        assert_eq!(renamed.title, "Project review");
        assert!(store.delete_task(&task.id).unwrap());
        assert!(store.get_task(&task.id).unwrap().is_none());
    }

    #[test]
    fn active_teammate_chat_cannot_be_deleted_or_renamed_to_an_empty_title() {
        let directory = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(directory.path().join("runtime.sqlite3")).unwrap();
        let agent = store
            .create_agent(
                "Research",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Review the project", None, None, false, None)
            .unwrap();
        assert!(store.delete_task(&task.id).is_err());
        assert!(store.rename_task(&task.id, "  ").is_err());
    }

    #[test]
    fn schema_v1_is_upgraded_with_the_gateway_command_queue() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE agents (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT,
                    kind TEXT NOT NULL, status TEXT NOT NULL, configuration TEXT NOT NULL,
                    configuration_revision INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );
                CREATE TABLE tasks (
                    id TEXT PRIMARY KEY, agent_id TEXT NOT NULL REFERENCES agents(id),
                    title TEXT NOT NULL, request TEXT NOT NULL, status TEXT NOT NULL, workspace TEXT,
                    checkpoint TEXT, error TEXT, attempt INTEGER NOT NULL DEFAULT 0,
                    recover_on_restart INTEGER NOT NULL DEFAULT 0, schedule_id TEXT,
                    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );
                CREATE TABLE events (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT, event_id TEXT NOT NULL UNIQUE,
                    agent_id TEXT, task_id TEXT, event_type TEXT NOT NULL,
                    payload TEXT NOT NULL, created_at INTEGER NOT NULL
                );
                PRAGMA user_version = 1;",
            )
            .unwrap();
        drop(legacy);

        let store = RuntimeStore::open(&path).unwrap();
        let db = store.db().unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let table_exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='gateway_commands')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 2);
        assert!(table_exists);
    }

    #[test]
    fn global_teammate_is_seeded_once_and_cannot_be_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let store = RuntimeStore::open(&path).unwrap();
        let global = store.get_agent(GLOBAL_AGENT_ID).unwrap().unwrap();
        assert_eq!(global.name, "Global");
        assert!(store.delete_agent(GLOBAL_AGENT_ID).is_err());
        store
            .set_agent_status(GLOBAL_AGENT_ID, AgentStatus::Paused)
            .unwrap();
        assert_eq!(
            store.get_agent(GLOBAL_AGENT_ID).unwrap().unwrap().status,
            AgentStatus::Paused
        );
        store
            .set_agent_status(GLOBAL_AGENT_ID, AgentStatus::Idle)
            .unwrap();
        drop(store);

        let reopened = RuntimeStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .list_agents()
                .unwrap()
                .iter()
                .filter(|agent| agent.id == GLOBAL_AGENT_ID)
                .count(),
            1
        );
    }

    #[test]
    fn task_chat_text_is_complete_and_agent_event_queries_are_isolated() {
        let store = RuntimeStore::in_memory();
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
        let task = store
            .create_task(&first.id, "Keep this conversation", None, None, false, None)
            .unwrap();

        for index in 0..1_100 {
            store
                .append_event(
                    Some(&first.id),
                    Some(&task.id),
                    "task.agent_event",
                    serde_json::json!({"type":"TextDelta","data":"x"}),
                )
                .unwrap();
            if index == 0 {
                store
                    .append_event(
                        Some(&second.id),
                        None,
                        "agent.activity",
                        serde_json::json!({"message":"unrelated"}),
                    )
                    .unwrap();
            }
        }

        assert_eq!(store.task_text_response(&task.id).unwrap().len(), 1_100);
        let events = store.events_for_agent(&first.id, 0, 1_000).unwrap();
        assert_eq!(events.len(), 1_000);
        assert!(
            events
                .iter()
                .all(|event| event.agent_id.as_deref() == Some(first.id.as_str()))
        );
    }

    #[test]
    fn legacy_agent_configuration_receives_safe_new_defaults() {
        let configuration: AgentConfiguration = serde_json::from_str(
            r#"{"system_prompt":"","skills":[],"provider":null,"model":null,"endpoint":null,"workspace":null,"allowed_workspaces":[],"enabled_tools":[],"permissions":{"allowed_tools":[],"denied_tools":[],"auto_approve_read_only":false,"require_approval_for_mutations":true,"allow_network":false},"max_concurrent_tasks":1,"recover_interrupted_tasks":false}"#,
        ).unwrap();
        assert_eq!(configuration.mascot, "auto");
        assert!(configuration.use_global_context);
    }

    #[test]
    fn deleting_agent_cascades_owned_runtime_data_and_preserves_audit_event() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Disposable",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do work", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Completed, None, None)
            .unwrap();
        store
            .add_memory(
                MemoryScope::Agent,
                &agent.id,
                "Remember this",
                Some("test".into()),
            )
            .unwrap();
        store
            .create_schedule(&agent.id, "Daily", "Do work", Some(1), None)
            .unwrap();

        assert!(store.delete_agent(&agent.id).unwrap());
        assert!(store.get_agent(&agent.id).unwrap().is_none());
        assert!(store.list_tasks(Some(&agent.id), 20).unwrap().is_empty());
        assert!(store.list_schedules(Some(&agent.id)).unwrap().is_empty());
        assert!(
            store
                .list_memory(MemoryScope::Agent, &agent.id, None, 20)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .events_since(0, 100)
                .unwrap()
                .iter()
                .any(|event| event.event_type == "agent.deleted")
        );
        assert!(!store.delete_agent(&agent.id).unwrap());
    }

    #[test]
    fn deleting_agent_with_queued_work_is_rejected() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Busy",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        store
            .create_task(&agent.id, "Still queued", None, None, false, None)
            .unwrap();
        assert!(store.delete_agent(&agent.id).is_err());
        assert!(store.get_agent(&agent.id).unwrap().is_some());
    }

    #[test]
    fn due_schedule_occurrence_is_claimed_and_enqueued_atomically() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Scheduler",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let schedule = store
            .create_schedule(&agent.id, "Daily", "Do the daily work", Some(1), None)
            .unwrap();

        let first = store.enqueue_due_schedule(&schedule.id, 2).unwrap();
        let second = store.enqueue_due_schedule(&schedule.id, 2).unwrap();

        assert!(first.is_some());
        assert!(second.is_none());
        let tasks = store.list_tasks(Some(&agent.id), 10).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].schedule_id.as_deref(), Some(schedule.id.as_str()));
        assert!(store.due_schedules(2).unwrap().is_empty());
    }

    #[test]
    fn invalid_task_transitions_are_rejected() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent("A", None, AgentKind::Code, AgentConfiguration::default())
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do work", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Completed, None, None)
            .unwrap();
        assert!(
            store
                .transition_task(&task.id, TaskStatus::Queued, None, None)
                .is_err()
        );
    }

    #[test]
    fn queued_task_can_be_marked_failed_when_worker_cannot_start() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "A",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do work", None, None, false, None)
            .unwrap();

        let failed = store
            .transition_task(
                &task.id,
                TaskStatus::Failed,
                Some("worker could not start".into()),
                None,
            )
            .unwrap();

        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("worker could not start"));
    }

    #[test]
    fn task_workspace_is_its_own_default_grant_when_agent_has_no_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("task-workspace");
        std::fs::create_dir(&workspace).unwrap();
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "A",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();

        let task = store
            .create_task(
                &agent.id,
                "Work in the explicitly selected folder",
                None,
                Some(workspace.to_string_lossy().into_owned()),
                false,
                None,
            )
            .unwrap();

        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(task.workspace.as_deref(), Some(workspace.to_str().unwrap()));
    }

    #[test]
    fn teammate_task_history_is_isolated_by_agent() {
        let store = RuntimeStore::in_memory();
        let first = store
            .create_agent(
                "Researcher",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let second = store
            .create_agent(
                "Reviewer",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let first_task = store
            .create_task(&first.id, "Research the project", None, None, false, None)
            .unwrap();
        let second_task = store
            .create_task(&second.id, "Review the project", None, None, false, None)
            .unwrap();

        let first_history = store.list_tasks(Some(&first.id), 50).unwrap();
        let second_history = store.list_tasks(Some(&second.id), 50).unwrap();

        assert_eq!(first_history.len(), 1);
        assert_eq!(first_history[0].id, first_task.id);
        assert_eq!(second_history.len(), 1);
        assert_eq!(second_history[0].id, second_task.id);
    }

    #[test]
    fn restart_recovery_never_requeues_uncertain_side_effects() {
        let store = RuntimeStore::in_memory();
        let mut configuration = AgentConfiguration::default();
        configuration.recover_interrupted_tasks = true;
        let agent = store
            .create_agent("A", None, AgentKind::Teammates, configuration)
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do work", None, None, true, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        let recovered = store.recover_after_restart().unwrap();
        assert_eq!(recovered[0].status, TaskStatus::Interrupted);
        assert!(
            recovered[0]
                .error
                .as_deref()
                .unwrap()
                .contains("reconciled before retry")
        );
        assert!(
            store
                .list_tasks(Some(&agent.id), 10)
                .unwrap()
                .iter()
                .all(|item| item.status != TaskStatus::Queued)
        );
    }

    #[test]
    fn terminal_task_expires_pending_approvals_and_prevents_late_resolution() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Approver",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do approved work", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::WaitingForApproval, None, None)
            .unwrap();
        let approval = store
            .create_approval(&agent.id, &task.id, "write_file", "file.txt")
            .unwrap();

        store
            .transition_task(&task.id, TaskStatus::Cancelled, None, None)
            .unwrap();

        assert!(store.list_pending_approvals().unwrap().is_empty());
        assert!(store.resolve_approval(&approval.id, true).is_err());
    }

    #[test]
    fn approval_resolution_requires_a_task_waiting_for_approval() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Approver",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Do approved work", None, None, false, None)
            .unwrap();
        assert!(
            store
                .create_approval(&agent.id, &task.id, "write_file", "file.txt")
                .is_err()
        );

        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::WaitingForApproval, None, None)
            .unwrap();
        let approval = store
            .create_approval(&agent.id, &task.id, "write_file", "file.txt")
            .unwrap();
        let resolved = store.resolve_approval(&approval.id, true).unwrap();
        assert_eq!(resolved.status, "approved");
        let commands = store.pending_gateway_commands(&task.id).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            GatewayCommand::Confirm {
                approval_id: approval.id.clone(),
                allowed: true,
            }
        );
        store
            .mark_gateway_command_processed(&commands[0].id)
            .unwrap();
        assert!(store.pending_gateway_commands(&task.id).unwrap().is_empty());
        assert!(store.resolve_approval(&approval.id, false).is_err());
    }

    #[test]
    fn teammate_question_answers_resume_the_waiting_worker_once() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Question asker",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Ask me something", None, None, false, None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .transition_task(
                &task.id,
                TaskStatus::WaitingForInput,
                None,
                Some(serde_json::json!({
                    "question_id": "question-1", "prompt": "Choose one", "choices": ["A", "B"]
                })),
            )
            .unwrap();

        assert!(
            store
                .answer_task_question(&task.id, "stale-question", "A")
                .is_err()
        );
        let resumed = store
            .answer_task_question(&task.id, "question-1", "B")
            .unwrap();
        assert_eq!(resumed.status, TaskStatus::Running);
        assert!(resumed.checkpoint.is_none());
        let commands = store.pending_gateway_commands(&task.id).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].command,
            GatewayCommand::AnswerQuestion {
                question_id: "question-1".into(),
                answer: "B".into(),
            }
        );
        assert!(
            store
                .answer_task_question(&task.id, "question-1", "A")
                .is_err()
        );
    }

    #[test]
    fn gateway_cancel_commands_survive_store_reopen_until_acknowledged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let (agent_id, task_id) = {
            let store = RuntimeStore::open(&path).unwrap();
            let agent = store
                .create_agent(
                    "Persistent worker",
                    None,
                    AgentKind::Teammates,
                    AgentConfiguration::default(),
                )
                .unwrap();
            let task = store
                .create_task(&agent.id, "Run until cancelled", None, None, false, None)
                .unwrap();
            store
                .transition_task(&task.id, TaskStatus::Running, None, None)
                .unwrap();
            store.enqueue_gateway_cancel(&task.id).unwrap();
            (agent.id, task.id)
        };

        let store = RuntimeStore::open(&path).unwrap();
        assert!(store.get_agent(&agent_id).unwrap().is_some());
        let commands = store.pending_gateway_commands(&task_id).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, GatewayCommand::Cancel);
        store
            .mark_gateway_command_processed(&commands[0].id)
            .unwrap();
        assert!(store.pending_gateway_commands(&task_id).unwrap().is_empty());
    }

    #[test]
    fn schedules_can_be_paused_and_resumed() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Scheduled teammate",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let schedule = store
            .create_schedule(&agent.id, "Hourly", "Check the workspace", None, Some(3600))
            .unwrap();

        let paused = store.set_schedule_enabled(&schedule.id, false).unwrap();
        assert!(!paused.enabled);
        assert!(store.due_schedules(i64::MAX).unwrap().is_empty());

        let resumed = store.set_schedule_enabled(&schedule.id, true).unwrap();
        assert!(resumed.enabled);
        assert_eq!(resumed.id, schedule.id);
    }

    #[test]
    fn pausing_or_stopping_an_active_agent_persists_gateway_cancellation() {
        let store = RuntimeStore::in_memory();
        let agent = store
            .create_agent(
                "Lifecycle",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        let task = store
            .create_task(&agent.id, "Run", None, None, false, None)
            .unwrap();
        store
            .create_schedule(&agent.id, "Daily", "Run the daily task", Some(1), None)
            .unwrap();
        store
            .transition_task(&task.id, TaskStatus::Running, None, None)
            .unwrap();
        store
            .set_agent_status(&agent.id, AgentStatus::Paused)
            .unwrap();
        let commands = store.pending_gateway_commands(&task.id).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, GatewayCommand::Cancel);
        store
            .mark_gateway_command_processed(&commands[0].id)
            .unwrap();
        assert!(
            store
                .set_agent_status(&agent.id, AgentStatus::Active)
                .is_err()
        );
        store
            .set_agent_status(&agent.id, AgentStatus::Stopped)
            .unwrap();
        assert!(
            store
                .list_schedules(Some(&agent.id))
                .unwrap()
                .iter()
                .all(|schedule| !schedule.enabled)
        );
        assert_eq!(
            store.pending_gateway_commands(&task.id).unwrap()[0].command,
            GatewayCommand::Cancel
        );

        store
            .transition_task(&task.id, TaskStatus::Completed, None, None)
            .unwrap();
        assert_eq!(
            store.get_agent(&agent.id).unwrap().unwrap().status,
            AgentStatus::Stopped
        );
        assert!(
            store
                .set_agent_status(&agent.id, AgentStatus::Idle)
                .is_err()
        );
    }

    #[test]
    fn memory_is_scoped_and_searchable() {
        let store = RuntimeStore::in_memory();
        store
            .add_memory(
                MemoryScope::User,
                "default",
                "Prefers concise answers",
                None,
            )
            .unwrap();
        store
            .add_memory(MemoryScope::Project, "/workspace", "Uses Rust", None)
            .unwrap();
        assert_eq!(
            store
                .list_memory(MemoryScope::User, "default", Some("concise"), 10)
                .unwrap()
                .len(),
            1
        );
        assert!(
            store
                .list_memory(MemoryScope::User, "default", Some("Rust"), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn new_teammates_receive_personal_agent_network_tools_by_default() {
        let store = RuntimeStore::in_memory();
        let teammate = store
            .create_agent(
                "Researcher",
                None,
                AgentKind::Teammates,
                AgentConfiguration::default(),
            )
            .unwrap();
        assert_eq!(teammate.configuration.network_access, Some(true));
        assert!(teammate.configuration.permissions.allow_network);
        assert!(
            teammate
                .configuration
                .permissions
                .allowed_tools
                .contains(&"web_search".to_owned())
        );
        assert!(
            teammate
                .configuration
                .permissions
                .allowed_tools
                .contains(&"fetch_url".to_owned())
        );
    }

    #[test]
    fn teammates_can_explicitly_disable_network_access() {
        let store = RuntimeStore::in_memory();
        let configuration = AgentConfiguration {
            network_access: Some(false),
            ..AgentConfiguration::default()
        };
        let teammate = store
            .create_agent("Offline", None, AgentKind::Teammates, configuration)
            .unwrap();
        assert_eq!(teammate.configuration.network_access, Some(false));
        assert!(!teammate.configuration.permissions.allow_network);
        assert_eq!(
            teammate
                .configuration
                .permissions
                .decide("web_search", false),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn permission_profiles_deny_before_allow() {
        let mut profile = PermissionProfile::default();
        profile.denied_tools.push("read_file".into());
        assert_eq!(profile.decide("read_file", false), PermissionDecision::Deny);
        assert_eq!(
            profile.decide_with_capabilities("save_memory", true, false, false),
            PermissionDecision::Allow
        );
        profile.denied_tools.push("save_memory".into());
        assert_eq!(
            profile.decide_with_capabilities("save_memory", true, false, false),
            PermissionDecision::Deny
        );
        assert_eq!(
            profile.decide("write_file", true),
            PermissionDecision::RequireApproval
        );
    }

    #[test]
    fn network_policy_cannot_be_overridden_by_an_explicit_tool_allow() {
        let mut profile = PermissionProfile::default();
        profile.allowed_tools.push("http_request".into());
        assert_eq!(
            profile.decide("http_request", false),
            PermissionDecision::Deny
        );

        profile.allow_network = true;
        assert_eq!(
            profile.decide("http_request", true),
            PermissionDecision::RequireApproval
        );
    }

    #[test]
    fn external_plugin_and_mcp_tools_require_network_permission_and_approval() {
        let mut profile = PermissionProfile::default();
        assert_eq!(
            profile.decide_with_capabilities("plugin__calendar__create", true, true, false),
            PermissionDecision::Deny
        );
        assert_eq!(
            profile.decide_with_capabilities("mcp__filesystem__write_file", true, true, false),
            PermissionDecision::Deny
        );

        profile.allow_network = true;
        assert_eq!(
            profile.decide_with_capabilities("plugin__calendar__create", true, true, false),
            PermissionDecision::RequireApproval
        );
        assert_eq!(
            profile.decide_with_capabilities("mcp__filesystem__write_file", true, true, false),
            PermissionDecision::RequireApproval
        );
    }
}
