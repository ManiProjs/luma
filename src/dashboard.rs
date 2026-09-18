use crate::event::AgentEvent;

use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
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

#[derive(Debug, Clone)]
pub struct DashboardServer {
    pub events: broadcast::Sender<AgentEvent>,
    pub commands: mpsc::Sender<DashboardCommand>,
    pub dashboard_dir: PathBuf,
    pub workspace_dir: PathBuf,
}

#[derive(Debug)]
pub enum DashboardCommand {
    Chat(String),
    Confirm { allowed: bool },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum ClientMessage {
    Chat(String),
    Confirm { allowed: bool },

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
        dashboard_dir: impl Into<PathBuf>,
        capacity: usize,
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
            dashboard_dir: dashboard_dir.into(),
            workspace_dir,
        });

        (server, commands_rx)
    }

    pub async fn run(self: Arc<Self>, addr: &str) -> anyhow::Result<()> {
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
            .route("/api/terminal", post(api_terminal))
            .with_state(self.clone());

        let listener = tokio::net::TcpListener::bind(addr).await?;

        tracing::info!("Luma Dashboard listening on http://{}", addr);

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

// ============================================================================
// Static files
// ============================================================================

async fn index(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_file(
        state.dashboard_dir.join("index.html"),
        "text/html; charset=utf-8",
    )
    .await
}

async fn app_js(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_file(
        state.dashboard_dir.join("app.js"),
        "application/javascript; charset=utf-8",
    )
    .await
}

async fn style_css(State(state): State<Arc<DashboardServer>>) -> impl IntoResponse {
    serve_file(
        state.dashboard_dir.join("style.css"),
        "text/css; charset=utf-8",
    )
    .await
}

async fn serve_file(path: PathBuf, content_type: &'static str) -> impl IntoResponse {
    match fs::read(path).await {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, content_type)], bytes).into_response(),

        Err(error) => (
            StatusCode::NOT_FOUND,
            format!("Dashboard file not found: {error}"),
        )
            .into_response(),
    }
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

    if !file.exists() {
        return ServerMessage::Error {
            message: format!("File does not exist: {}", path),
        };
    }

    if !file.is_file() {
        return ServerMessage::Error {
            message: format!("Not a file: {}", path),
        };
    }

    let relative = relative_display(&state.workspace_dir, &file);

    let tracked = Command::new("git")
        .current_dir(&state.workspace_dir)
        .args(["ls-files", "--error-unmatch", "--", relative.as_str()])
        .output()
        .await;

    let is_tracked = tracked
        .map(|output| output.status.success())
        .unwrap_or(false);

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
