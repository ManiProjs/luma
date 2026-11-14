use crate::event::AgentEvent;

use axum::{
    Json, Router,
    extract::{
        Path as AxumPath, Query, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{
        StatusCode, Uri,
        header::{HOST, ORIGIN},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};

use crate::{
    chats,
    runtime::{AgentConfiguration, AgentKind, AgentStatus, MemoryScope, RuntimeStore, TaskStatus},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs,
    process::Command,
    sync::{broadcast, mpsc},
    time::timeout,
};

const TERMINAL_TIMEOUT: Duration = Duration::from_secs(120);

const MAX_TREE_ENTRIES: usize = 2000;
const MAX_TREE_DEPTH: usize = 8;

#[derive(Clone)]
pub struct DashboardServer {
    pub events: broadcast::Sender<AgentEvent>,
    pub commands: mpsc::Sender<DashboardCommand>,
    pub dashboard_dir: Option<PathBuf>,
    pub workspace_dir: PathBuf,
    pub runtime: RuntimeStore,
}

#[derive(Debug)]
pub enum DashboardCommand {
    Chat(String),
    Confirm { allowed: bool },
    Cancel,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum ClientMessage {
    Chat(String),
    Confirm { allowed: bool },
    Cancel,

    OpenFile { path: String },

    SaveFile { path: String, content: String },

    CreateFile { path: String, content: String },

    CreateDirectory { path: String },

    Delete { path: String },

    Rename { from: String, to: String },

    GetTree,

    GetDiff { path: String },

    Terminal { command: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
enum ServerMessage {
    Event(AgentEvent),

    FileOpened { path: String, content: String },

    FileSaved { path: String },

    FileCreated { path: String },

    DirectoryCreated { path: String },

    Deleted { path: String },

    Renamed { from: String, to: String },

    DirectoryTree { entries: Vec<FileEntry> },

    Diff { path: String, diff: String },

    TerminalOutput { command: String, output: String },

    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Directory,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

impl DashboardServer {
    pub fn new(
        dashboard_dir: Option<PathBuf>,
        capacity: usize,
        runtime: RuntimeStore,
    ) -> (Arc<Self>, mpsc::Receiver<DashboardCommand>) {
        let (events, _) = broadcast::channel(capacity);
        let (commands, commands_rx) = mpsc::channel(64);

        let workspace_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from("."));

        let server = Arc::new(Self {
            events,
            commands,
            dashboard_dir,
            workspace_dir,
            runtime,
        });

        (server, commands_rx)
    }

    pub async fn run_listener(
        self: Arc<Self>,
        listener: tokio::net::TcpListener,
    ) -> anyhow::Result<()> {
        let app = Router::new()
            .route("/", get(index))
            .route("/assets/app.js", get(app_js))
            .route("/assets/style.css", get(style_css))
            .route("/ws", get(websocket))
            .route("/api/tree", get(api_tree))
            .route("/api/file", post(api_file))
            .route("/api/save", post(api_save))
            .route("/api/create-file", post(api_create_file))
            .route("/api/create-directory", post(api_create_directory))
            .route("/api/delete", post(api_delete))
            .route("/api/rename", post(api_rename))
            .route("/api/diff", post(api_diff))
            .route("/api/git-status", get(api_git_status))
            .route("/api/history", get(api_history))
            .route("/api/history/{id}", get(api_history_chat))
            .route("/api/agents", get(api_agents).post(api_create_agent))
            .route(
                "/api/agents/{id}",
                delete(api_delete_agent).put(api_update_agent),
            )
            .route("/api/agents/{id}/status", post(api_set_agent_status))
            .route("/api/tasks", get(api_tasks).post(api_create_task))
            .route("/api/tasks/{id}/cancel", post(api_cancel_task))
            .route("/api/tasks/{id}/answer", post(api_answer_task_question))
            .route("/api/tasks/{id}/retry", post(api_retry_task))
            .route("/api/approvals", get(api_approvals))
            .route("/api/approvals/{id}", post(api_resolve_approval))
            .route("/api/memory", get(api_memory).post(api_add_memory))
            .route("/api/memory/{id}", delete(api_delete_memory))
            .route(
                "/api/schedules",
                get(api_schedules).post(api_create_schedule),
            )
            .route("/api/schedules/{id}", delete(api_delete_schedule))
            .route("/api/events", get(api_events))
            .route("/api/terminal", post(api_terminal))
            .layer(middleware::from_fn(restrict_browser_origins))
            .with_state(self.clone());

        tracing::info!(
            "Luma Dashboard listening on http://{}",
            listener.local_addr()?
        );

        axum::serve(listener, app).await?;

        Ok(())
    }

    // pub fn emit(&self, event: AgentEvent) {
    //     let _ = self.events.send(event);
    // }

    fn resolve_workspace_path(&self, relative: &str) -> anyhow::Result<PathBuf> {
        resolve_workspace_path(&self.workspace_dir, relative)
    }
}

/// The dashboard is intentionally loopback-only until authenticated remote
/// access exists. Loopback binding alone does not stop DNS-rebinding or
/// cross-origin browser requests, so validate both Host and Origin here.
async fn restrict_browser_origins(request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok());
    let Some(host) = host.filter(|host| is_loopback_authority(host)) else {
        return StatusCode::FORBIDDEN.into_response();
    };

    if let Some(origin) = request.headers().get(ORIGIN) {
        if !origin
            .to_str()
            .is_ok_and(|origin| is_same_origin_authority(origin, host))
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    next.run(request).await
}

fn is_loopback_authority(value: &str) -> bool {
    let Ok(authority) = value.parse::<axum::http::uri::Authority>() else {
        return false;
    };
    let host = authority
        .host()
        .trim_start_matches('[')
        .trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn is_same_origin_authority(origin: &str, host: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some("http" | "https"))
        && uri
            .authority()
            .is_some_and(|authority| authority.as_str().eq_ignore_ascii_case(host))
}

// ============================================================================
// Static files
// ============================================================================

async fn index(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_asset(
        state.dashboard_dir.as_deref(),
        "index.html",
        include_bytes!("../dashboard/dist/index.html"),
        "text/html; charset=utf-8",
    )
    .await
}

async fn app_js(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_asset(
        state.dashboard_dir.as_deref(),
        "assets/app.js",
        include_bytes!("../dashboard/dist/assets/app.js"),
        "application/javascript; charset=utf-8",
    )
    .await
}

async fn style_css(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_asset(
        state.dashboard_dir.as_deref(),
        "assets/style.css",
        include_bytes!("../dashboard/dist/assets/style.css"),
        "text/css; charset=utf-8",
    )
    .await
}

async fn serve_asset(
    override_dir: Option<&Path>,
    filename: &str,
    embedded: &'static [u8],
    content_type: &'static str,
) -> axum::response::Response {
    let bytes = if let Some(directory) = override_dir {
        match fs::read(directory.join(filename)).await {
            Ok(bytes) => bytes,
            Err(error) => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("Dashboard override asset {filename} not found: {error}"),
                )
                    .into_response();
            }
        }
    } else {
        embedded.to_vec()
    };

    ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response()
}

// ============================================================================
// WebSocket
// ============================================================================

async fn websocket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DashboardServer>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<DashboardServer>) {
    let mut events = state.events.subscribe();
    let commands = state.commands.clone();

    loop {
        tokio::select! {
            event = events.recv() => {
                let event = match event {
                    Ok(event) => event,

                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        tracing::warn!(
                            "Dashboard client lagged by {} events",
                            count
                        );
                        continue;
                    }

                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                };

                let message = ServerMessage::Event(event);

                if send_message(&mut socket, &message).await.is_err() {
                    break;
                }
            }

            result = socket.recv() => {
                let Some(result) = result else {
                    break;
                };

                let message = match result {
                    Ok(message) => message,

                    Err(error) => {
                        tracing::debug!(
                            "Dashboard WebSocket error: {}",
                            error
                        );
                        break;
                    }
                };

                let Message::Text(text) = message else {
                    continue;
                };

                let client_message =
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(message) => message,

                        Err(error) => {
                            let _ = send_message(
                                &mut socket,
                                &ServerMessage::Error {
                                    message: format!(
                                        "Invalid dashboard message: {}",
                                        error
                                    ),
                                },
                            )
                            .await;

                            continue;
                        }
                    };

                match client_message {
                    ClientMessage::Chat(text) => {
                        if commands
                            .send(DashboardCommand::Chat(text))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }

                    ClientMessage::Confirm { allowed } => {
                        if commands
                            .send(DashboardCommand::Confirm { allowed })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }

                    ClientMessage::Cancel => {
                        if commands.send(DashboardCommand::Cancel).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::OpenFile { path } => {
                        let response = open_file(&state, &path).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::SaveFile { path, content } => {
                        let response = save_file(&state, &path, &content).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::CreateFile { path, content } => {
                        let response =
                            create_file(&state, &path, &content).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::CreateDirectory { path } => {
                        let response =
                            create_directory(&state, &path).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::Delete { path } => {
                        let response = delete_path(&state, &path).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::Rename { from, to } => {
                        let response =
                            rename_path(&state, &from, &to).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::GetTree => {
                        let response = tree_message(&state).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::GetDiff { path } => {
                        let response = get_diff(&state, &path).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }

                    ClientMessage::Terminal { command } => {
                        let response =
                            terminal_command(&state, &command).await;

                        if send_message(&mut socket, &response).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn send_message(socket: &mut WebSocket, message: &ServerMessage) -> anyhow::Result<()> {
    let json = serde_json::to_string(message)?;

    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(Into::into)
}

// ============================================================================
// Workspace API
// ============================================================================

async fn api_tree(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    match build_tree(&state.workspace_dir).await {
        Ok(entries) => Json(entries).into_response(),

        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct PathRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct SaveRequest {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct CreateDirectoryRequest {
    path: String,
}

#[derive(Debug, Deserialize)]
struct RenameRequest {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
struct TerminalRequest {
    command: String,
}

async fn api_file(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<PathRequest>,
) -> impl IntoResponse {
    match open_file(&state, &request.path).await {
        ServerMessage::FileOpened { path, content } => Json(serde_json::json!({
            "path": path,
            "content": content
        }))
        .into_response(),

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_save(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<SaveRequest>,
) -> impl IntoResponse {
    match save_file(&state, &request.path, &request.content).await {
        ServerMessage::FileSaved { path } => {
            Json(serde_json::json!({ "path": path })).into_response()
        }

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_create_file(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<SaveRequest>,
) -> impl IntoResponse {
    match create_file(&state, &request.path, &request.content).await {
        ServerMessage::FileCreated { path } => {
            Json(serde_json::json!({ "path": path })).into_response()
        }

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_create_directory(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<CreateDirectoryRequest>,
) -> impl IntoResponse {
    match create_directory(&state, &request.path).await {
        ServerMessage::DirectoryCreated { path } => {
            Json(serde_json::json!({ "path": path })).into_response()
        }

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_delete(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<PathRequest>,
) -> impl IntoResponse {
    match delete_path(&state, &request.path).await {
        ServerMessage::Deleted { path } => {
            Json(serde_json::json!({ "path": path })).into_response()
        }

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_rename(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<RenameRequest>,
) -> impl IntoResponse {
    match rename_path(&state, &request.from, &request.to).await {
        ServerMessage::Renamed { from, to } => Json(serde_json::json!({
            "from": from,
            "to": to
        }))
        .into_response(),

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_diff(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<PathRequest>,
) -> impl IntoResponse {
    match get_diff(&state, &request.path).await {
        ServerMessage::Diff { path, diff } => Json(serde_json::json!({
            "path": path,
            "diff": diff
        }))
        .into_response(),

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

async fn api_git_status(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    let branch = Command::new("git")
        .current_dir(&state.workspace_dir)
        .args(["branch", "--show-current"])
        .output()
        .await;
    let status = Command::new("git")
        .current_dir(&state.workspace_dir)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .await;
    match (branch, status) {
        (Ok(branch), Ok(status)) if branch.status.success() && status.status.success() => {
            let files = String::from_utf8_lossy(&status.stdout)
                .lines()
                .filter(|line| line.len() >= 4)
                .map(|line| {
                    let index = line.chars().next().unwrap_or(' ');
                    let worktree = line.chars().nth(1).unwrap_or(' ');
                    let raw_path = line.get(3..).unwrap_or_default();
                    let path = raw_path.rsplit_once(" -> ").map(|(_, path)| path).unwrap_or(raw_path);
                    let code = if index == '?' { '?' } else if index != ' ' { index } else { worktree };
                    let kind = match code {
                        '?' => "untracked",
                        'A' => "added",
                        'D' => "deleted",
                        'R' => "renamed",
                        _ => "modified",
                    };
                    serde_json::json!({ "path": path, "index_status": index.to_string(), "worktree_status": worktree.to_string(), "kind": kind })
                })
                .collect::<Vec<_>>();
            Json(serde_json::json!({ "branch": String::from_utf8_lossy(&branch.stdout).trim(), "files": files })).into_response()
        }
        (Ok(branch), Ok(status)) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: String::from_utf8_lossy(if !branch.status.success() {
                    &branch.stderr
                } else {
                    &status.stderr
                })
                .trim()
                .to_owned(),
            }),
        )
            .into_response(),
        (Err(error), _) | (_, Err(error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("Failed to inspect Git status: {error}"),
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    name: String,
    description: Option<String>,
    #[serde(default = "default_teammates_kind")]
    kind: AgentKind,
    configuration: Option<AgentConfiguration>,
}

#[derive(Debug, Deserialize)]
struct SetAgentStatusRequest {
    status: AgentStatus,
}

#[derive(Debug, Deserialize)]
struct UpdateAgentRequest {
    name: Option<String>,
    description: Option<String>,
    configuration: Option<AgentConfiguration>,
}

fn default_teammates_kind() -> AgentKind {
    AgentKind::Teammates
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    agent_id: String,
    request: String,
    title: Option<String>,
    workspace: Option<String>,
    #[serde(default)]
    recover_on_restart: bool,
}

#[derive(Debug, Deserialize)]
struct MemoryQuery {
    scope: MemoryScope,
    scope_id: String,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddMemoryRequest {
    scope: MemoryScope,
    scope_id: String,
    content: String,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateScheduleRequest {
    agent_id: String,
    title: String,
    prompt: String,
    run_at: Option<i64>,
    every_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ResolveApprovalRequest {
    allowed: bool,
}

#[derive(Debug, Deserialize)]
struct AnswerTaskQuestionRequest {
    question_id: String,
    answer: String,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    after_sequence: Option<i64>,
    agent_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TasksQuery {
    agent_id: Option<String>,
    limit: Option<usize>,
}

async fn api_agents(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    match state.runtime.list_agents() {
        Ok(agents) => Json(agents).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_history() -> impl IntoResponse {
    match chats::list() {
        Ok(chats) => Json(chats).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_history_chat(AxumPath(id): AxumPath<String>) -> impl IntoResponse {
    match chats::load(&id) {
        Ok(chat) => Json(chat).into_response(),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_create_agent(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    let workspace = state.workspace_dir.display().to_string();
    let mut configuration = request.configuration.unwrap_or_default();
    if configuration.workspace.is_none() {
        configuration.workspace = Some(workspace.clone());
    }
    if configuration.allowed_workspaces.is_empty() {
        configuration.allowed_workspaces.push(workspace);
    }
    match state.runtime.create_agent(
        &request.name,
        request.description,
        request.kind,
        configuration,
    ) {
        Ok(agent) => (StatusCode::CREATED, Json(agent)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_delete_agent(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runtime.delete_agent(&id) {
        Ok(true) => Json(serde_json::json!({"id": id, "deleted": true})).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Teammate not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_update_agent(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<UpdateAgentRequest>,
) -> impl IntoResponse {
    match state.runtime.update_agent(
        &id,
        request.name,
        request.description,
        request.configuration,
    ) {
        Ok(agent) => Json(agent).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_set_agent_status(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<SetAgentStatusRequest>,
) -> impl IntoResponse {
    if !matches!(
        request.status,
        AgentStatus::Idle | AgentStatus::Paused | AgentStatus::Stopped
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "Clients may only set an agent to idle, paused, or stopped.".into(),
            }),
        )
            .into_response();
    }
    if let Err(error) = state.runtime.set_agent_status(&id, request.status) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response();
    }
    match state.runtime.get_agent(&id) {
        Ok(Some(agent)) => Json(agent).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Agent not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_tasks(
    State(state): State<Arc<DashboardServer>>,
    Query(query): Query<TasksQuery>,
) -> impl IntoResponse {
    match state
        .runtime
        .list_tasks(query.agent_id.as_deref(), query.limit.unwrap_or(500))
    {
        Ok(tasks) => Json(tasks).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_create_task(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    let workspace = request
        .workspace
        .or_else(|| Some(state.workspace_dir.display().to_string()));
    match state.runtime.create_task(
        &request.agent_id,
        &request.request,
        request.title,
        workspace,
        request.recover_on_restart,
        None,
    ) {
        Ok(task) => (StatusCode::CREATED, Json(task)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_cancel_task(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runtime.get_task(&id) {
        Ok(Some(task))
            if task.status == TaskStatus::Queued || task.status == TaskStatus::Paused =>
        {
            match state
                .runtime
                .transition_task(&id, TaskStatus::Cancelled, None, None)
            {
                Ok(task) => Json(task).into_response(),
                Err(error) => (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        error: error.to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(Some(task))
            if task.status == TaskStatus::Running
                || task.status == TaskStatus::WaitingForApproval
                || task.status == TaskStatus::WaitingForInput =>
        {
            if let Err(error) = state.runtime.enqueue_gateway_cancel(&id) {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError {
                        error: error.to_string(),
                    }),
                )
                    .into_response();
            }
            Json(task).into_response()
        }
        Ok(Some(_)) => (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "Task is already terminal.".into(),
            }),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Task not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_answer_task_question(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<AnswerTaskQuestionRequest>,
) -> impl IntoResponse {
    match state
        .runtime
        .answer_task_question(&id, &request.question_id, &request.answer)
    {
        Ok(task) => Json(task).into_response(),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_retry_task(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runtime.get_task(&id) {
        Ok(Some(task))
            if (task.status == TaskStatus::Failed || task.status == TaskStatus::Interrupted)
                && state
                    .runtime
                    .get_agent(&task.agent_id)
                    .ok()
                    .flatten()
                    .is_some_and(|agent| agent.status != AgentStatus::Stopped) =>
        {
            match state
                .runtime
                .transition_task(&id, TaskStatus::Queued, None, None)
            {
                Ok(task) => Json(task).into_response(),
                Err(error) => (
                    StatusCode::BAD_REQUEST,
                    Json(ApiError {
                        error: error.to_string(),
                    }),
                )
                    .into_response(),
            }
        }
        Ok(Some(_)) => (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "Only failed or interrupted tasks can be retried.".into(),
            }),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Task not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_approvals(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    match state.runtime.list_pending_approvals() {
        Ok(approvals) => Json(approvals).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_resolve_approval(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<ResolveApprovalRequest>,
) -> impl IntoResponse {
    let approval = match state.runtime.list_pending_approvals() {
        Ok(items) => match items.into_iter().find(|item| item.id == id) {
            Some(approval) => approval,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Approval not found or already resolved.".into(),
                    }),
                )
                    .into_response();
            }
        },
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    };
    match state.runtime.get_task(&approval.task_id) {
        Ok(Some(task)) if task.status == TaskStatus::WaitingForApproval => {}
        Ok(_) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "The task is no longer waiting for this approval.".into(),
                }),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    }
    match state.runtime.get_agent(&approval.agent_id) {
        Ok(Some(agent)) if !matches!(agent.status, AgentStatus::Paused | AgentStatus::Stopped) => {}
        Ok(_) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "Resume the agent before resolving its approval.".into(),
                }),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    }
    let approval = match state.runtime.resolve_approval(&id, request.allowed) {
        Ok(approval) => approval,
        Err(error) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    };
    Json(approval).into_response()
}

async fn api_memory(
    State(state): State<Arc<DashboardServer>>,
    Query(query): Query<MemoryQuery>,
) -> impl IntoResponse {
    match state
        .runtime
        .list_memory(query.scope, &query.scope_id, query.query.as_deref(), 200)
    {
        Ok(entries) => Json(entries).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_add_memory(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<AddMemoryRequest>,
) -> impl IntoResponse {
    match state.runtime.add_memory(
        request.scope,
        &request.scope_id,
        &request.content,
        request.source,
    ) {
        Ok(entry) => (StatusCode::CREATED, Json(entry)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_delete_memory(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runtime.delete_memory(&id) {
        Ok(true) => Json(serde_json::json!({ "deleted": true })).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory entry not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_schedules(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    match state.runtime.list_schedules(None) {
        Ok(schedules) => Json(schedules).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_create_schedule(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<CreateScheduleRequest>,
) -> impl IntoResponse {
    match state.runtime.create_schedule(
        &request.agent_id,
        &request.title,
        &request.prompt,
        request.run_at,
        request.every_seconds,
    ) {
        Ok(schedule) => (StatusCode::CREATED, Json(schedule)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_delete_schedule(
    State(state): State<Arc<DashboardServer>>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    match state.runtime.delete_schedule(&id) {
        Ok(true) => Json(serde_json::json!({ "deleted": true })).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Schedule not found.".into(),
            }),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_events(
    State(state): State<Arc<DashboardServer>>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    let result = if let Some(agent_id) = query.agent_id.as_deref() {
        state.runtime.events_for_agent(
            agent_id,
            query.after_sequence.unwrap_or(0),
            query.limit.unwrap_or(1000),
        )
    } else {
        state.runtime.events_since(
            query.after_sequence.unwrap_or(0),
            query.limit.unwrap_or(1000),
        )
    };
    match result {
        Ok(events) => Json(events).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn api_terminal(
    State(state): State<Arc<DashboardServer>>,
    Json(request): Json<TerminalRequest>,
) -> impl IntoResponse {
    match terminal_command(&state, &request.command).await {
        ServerMessage::TerminalOutput { command, output } => Json(serde_json::json!({
            "command": command,
            "output": output
        }))
        .into_response(),

        ServerMessage::Error { message } => {
            (StatusCode::BAD_REQUEST, Json(ApiError { error: message })).into_response()
        }

        _ => unreachable!(),
    }
}

// ============================================================================
// Workspace operations
// ============================================================================

async fn open_file(state: &DashboardServer, path: &str) -> ServerMessage {
    let path = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    let relative = relative_display(&state.workspace_dir, &path);

    match fs::read_to_string(&path).await {
        Ok(content) => ServerMessage::FileOpened {
            path: relative,
            content,
        },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to read {}: {}", relative, error),
        },
    }
}

async fn save_file(state: &DashboardServer, path: &str, content: &str) -> ServerMessage {
    let path = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    if !path.exists() {
        return ServerMessage::Error {
            message: format!(
                "File does not exist: {}",
                relative_display(&state.workspace_dir, &path)
            ),
        };
    }

    if !path.is_file() {
        return ServerMessage::Error {
            message: format!(
                "Not a file: {}",
                relative_display(&state.workspace_dir, &path)
            ),
        };
    }

    let relative = relative_display(&state.workspace_dir, &path);

    match fs::write(&path, content).await {
        Ok(()) => ServerMessage::FileSaved { path: relative },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to save {}: {}", relative, error),
        },
    }
}

async fn create_file(state: &DashboardServer, path: &str, content: &str) -> ServerMessage {
    let path = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    if path.exists() {
        return ServerMessage::Error {
            message: format!(
                "Path already exists: {}",
                relative_display(&state.workspace_dir, &path)
            ),
        };
    }

    let Some(parent) = path.parent() else {
        return ServerMessage::Error {
            message: "Unable to determine parent directory.".into(),
        };
    };

    if let Err(error) = fs::create_dir_all(parent).await {
        return ServerMessage::Error {
            message: format!("Failed to create parent directory: {}", error),
        };
    }

    let relative = relative_display(&state.workspace_dir, &path);

    match fs::write(&path, content).await {
        Ok(()) => ServerMessage::FileCreated { path: relative },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to create {}: {}", relative, error),
        },
    }
}

async fn create_directory(state: &DashboardServer, path: &str) -> ServerMessage {
    let path = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    if path.exists() {
        return ServerMessage::Error {
            message: format!(
                "Path already exists: {}",
                relative_display(&state.workspace_dir, &path)
            ),
        };
    }

    let relative = relative_display(&state.workspace_dir, &path);

    match fs::create_dir_all(&path).await {
        Ok(()) => ServerMessage::DirectoryCreated { path: relative },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to create {}: {}", relative, error),
        },
    }
}

async fn delete_path(state: &DashboardServer, path: &str) -> ServerMessage {
    let path = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    if path == state.workspace_dir {
        return ServerMessage::Error {
            message: "Refusing to delete the workspace root.".into(),
        };
    }

    let relative = relative_display(&state.workspace_dir, &path);

    if !path.exists() {
        return ServerMessage::Error {
            message: format!("Path does not exist: {}", relative),
        };
    }

    let result = if path.is_dir() {
        fs::remove_dir_all(&path).await
    } else {
        fs::remove_file(&path).await
    };

    match result {
        Ok(()) => ServerMessage::Deleted { path: relative },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to delete {}: {}", relative, error),
        },
    }
}

async fn rename_path(state: &DashboardServer, from: &str, to: &str) -> ServerMessage {
    let from_path = match state.resolve_workspace_path(from) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    let to_path = match state.resolve_workspace_path(to) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    if !from_path.exists() {
        return ServerMessage::Error {
            message: format!("Source does not exist: {}", from),
        };
    }

    if to_path.exists() {
        return ServerMessage::Error {
            message: format!("Destination already exists: {}", to),
        };
    }

    if let Some(parent) = to_path.parent() {
        if let Err(error) = fs::create_dir_all(parent).await {
            return ServerMessage::Error {
                message: format!("Failed to create destination directory: {}", error),
            };
        }
    }

    match fs::rename(&from_path, &to_path).await {
        Ok(()) => ServerMessage::Renamed {
            from: from.to_owned(),
            to: to.to_owned(),
        },

        Err(error) => ServerMessage::Error {
            message: format!("Failed to rename {} to {}: {}", from, to, error),
        },
    }
}

// ============================================================================
// Tree
// ============================================================================

async fn tree_message(state: &DashboardServer) -> ServerMessage {
    match build_tree(&state.workspace_dir).await {
        Ok(entries) => ServerMessage::DirectoryTree { entries },

        Err(error) => ServerMessage::Error {
            message: error.to_string(),
        },
    }
}

async fn build_tree(root: &Path) -> anyhow::Result<Vec<FileEntry>> {
    let root = root.to_path_buf();

    let entries = tokio::task::spawn_blocking(move || {
        let mut output = Vec::new();

        walk_tree(&root, &root, 0, &mut output)?;

        output.sort_by(|a, b| {
            let a_dir = matches!(a.kind, FileKind::Directory);
            let b_dir = matches!(b.kind, FileKind::Directory);

            b_dir.cmp(&a_dir).then_with(|| a.path.cmp(&b.path))
        });

        Ok::<_, anyhow::Error>(output)
    })
    .await??;

    Ok(entries)
}

fn walk_tree(
    root: &Path,
    directory: &Path,
    depth: usize,
    output: &mut Vec<FileEntry>,
) -> anyhow::Result<()> {
    if depth > MAX_TREE_DEPTH || output.len() >= MAX_TREE_ENTRIES {
        return Ok(());
    }

    let mut entries = std::fs::read_dir(directory)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        if output.len() >= MAX_TREE_ENTRIES {
            break;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        if should_skip_tree_entry(&name) {
            continue;
        }

        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        let kind = if path.is_dir() {
            FileKind::Directory
        } else {
            FileKind::File
        };

        output.push(FileEntry {
            path: relative,
            name,
            kind: kind.clone(),
        });

        if matches!(kind, FileKind::Directory) {
            walk_tree(root, &path, depth + 1, output)?;
        }
    }

    Ok(())
}

fn should_skip_tree_entry(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | "dist"
            | "build"
            | "out"
            | "__pycache__"
            | ".venv"
            | "venv"
            | "env"
            | ".idea"
            | ".vscode"
    )
}

// ============================================================================
// Diff
// ============================================================================

async fn get_diff(state: &DashboardServer, path: &str) -> ServerMessage {
    let file = match state.resolve_workspace_path(path) {
        Ok(path) => path,

        Err(error) => {
            return ServerMessage::Error {
                message: error.to_string(),
            };
        }
    };

    let relative = relative_display(&state.workspace_dir, &file);

    let tracked = Command::new("git")
        .current_dir(&state.workspace_dir)
        .args(["ls-files", "--error-unmatch", "--", relative.as_str()])
        .output()
        .await;

    let is_tracked = tracked
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !file.exists() && !is_tracked {
        return ServerMessage::Error {
            message: format!("File does not exist: {}", path),
        };
    }

    if file.exists() && !file.is_file() {
        return ServerMessage::Error {
            message: format!("Not a file: {}", path),
        };
    }

    let output = if is_tracked {
        Command::new("git")
            .current_dir(&state.workspace_dir)
            .args(["diff", "HEAD", "--", relative.as_str()])
            .output()
            .await
    } else {
        let null_path = if cfg!(windows) { "NUL" } else { "/dev/null" };

        Command::new("git")
            .current_dir(&state.workspace_dir)
            .args(["diff", "--no-index", "--", null_path, relative.as_str()])
            .output()
            .await
    };

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);

            ServerMessage::Diff {
                path: relative,
                diff: stdout.into_owned(),
            }
        }

        Err(error) => ServerMessage::Error {
            message: format!("Failed to calculate diff: {}", error),
        },
    }
}

// ============================================================================
// Terminal
// ============================================================================

async fn terminal_command(state: &DashboardServer, command: &str) -> ServerMessage {
    let command = command.trim();

    if command.is_empty() {
        return ServerMessage::Error {
            message: "Terminal command cannot be empty.".into(),
        };
    }

    let child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&state.workspace_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();

    let child = match child {
        Ok(child) => child,

        Err(error) => {
            return ServerMessage::Error {
                message: format!("Failed to start terminal command: {}", error),
            };
        }
    };

    let result = timeout(TERMINAL_TIMEOUT, child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = String::new();

            result.push_str(&format!(
                "exit code: {}\n\n",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".into())
            ));

            if !stdout.is_empty() {
                result.push_str("stdout:\n");
                result.push_str(&stdout);
            }

            if !stderr.is_empty() {
                if !stdout.is_empty() {
                    result.push('\n');
                }

                result.push_str("stderr:\n");
                result.push_str(&stderr);
            }

            ServerMessage::TerminalOutput {
                command: command.to_owned(),
                output: result,
            }
        }

        Ok(Err(error)) => ServerMessage::Error {
            message: format!("Terminal command failed: {}", error),
        },

        Err(_) => ServerMessage::Error {
            message: format!(
                "Terminal command timed out after {} seconds.",
                TERMINAL_TIMEOUT.as_secs()
            ),
        },
    }
}

// ============================================================================
// Path security
// ============================================================================

fn resolve_workspace_path(workspace: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let relative = relative.trim();

    if relative.is_empty() {
        return Ok(workspace.to_path_buf());
    }

    let path = Path::new(relative);

    if path.is_absolute() {
        anyhow::bail!("Absolute paths are not allowed.");
    }

    for component in path.components() {
        match component {
            Component::ParentDir => {
                anyhow::bail!("Paths cannot escape the workspace.");
            }

            Component::Prefix(_) | Component::RootDir => {
                anyhow::bail!("Invalid workspace path.");
            }

            _ => {}
        }
    }

    Ok(workspace.join(path))
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{ClientMessage, is_loopback_authority, is_same_origin_authority, serve_asset};
    use axum::{body::to_bytes, http::StatusCode};

    #[tokio::test]
    async fn serves_embedded_dashboard_assets_without_a_filesystem_directory() {
        let assets = [
            (
                "index.html",
                include_bytes!("../dashboard/dist/index.html").as_slice(),
                "text/html; charset=utf-8",
            ),
            (
                "assets/app.js",
                include_bytes!("../dashboard/dist/assets/app.js").as_slice(),
                "application/javascript; charset=utf-8",
            ),
            (
                "assets/style.css",
                include_bytes!("../dashboard/dist/assets/style.css").as_slice(),
                "text/css; charset=utf-8",
            ),
        ];

        for (filename, embedded, content_type) in assets {
            let response = serve_asset(None, filename, embedded, content_type).await;

            assert_eq!(response.status(), StatusCode::OK, "{filename}");
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_TYPE],
                content_type,
                "{filename}"
            );
            assert!(
                !to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn dashboard_cancel_message_is_accepted() {
        let message = serde_json::from_str::<ClientMessage>(r#"{"type":"Cancel"}"#)
            .expect("Cancel should be a valid dashboard message");
        assert!(matches!(message, ClientMessage::Cancel));
    }

    #[test]
    fn dashboard_rejects_non_loopback_hosts_and_cross_origin_requests() {
        assert!(is_loopback_authority("127.0.0.1:3000"));
        assert!(is_loopback_authority("localhost:3000"));
        assert!(is_loopback_authority("[::1]:3000"));
        assert!(!is_loopback_authority("luma.example:3000"));
        assert!(!is_loopback_authority("192.0.2.10:3000"));

        assert!(is_same_origin_authority(
            "http://localhost:3000",
            "localhost:3000"
        ));
        assert!(!is_same_origin_authority(
            "https://evil.example",
            "localhost:3000"
        ));
        assert!(!is_same_origin_authority("null", "localhost:3000"));
    }
}
