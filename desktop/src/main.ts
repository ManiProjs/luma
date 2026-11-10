import { app, BrowserWindow, dialog, ipcMain, Menu, type OpenDialogOptions } from "electron";

import path from "node:path";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

import readline from "node:readline";

import { randomUUID } from "node:crypto";
import {
  assertProtocolVersion,
  encodeClientMessage,
  isAppConfigInput,
  isSupportedProvider,
  parseServerEvent,
  type ClientMessage,
  type ServerEvent,
  type AgentConfiguration,
  type AgentKind,
  type AgentRecord,
  type ChatSummary,
  type ApprovalRecord,
  type MemoryEntry,
  type MemoryScope,
  type ScheduleRecord,
  type TaskRecord,
  type RuntimeEvent,
} from "../../shared/protocol";
import type { ChatAttachment } from "../../shared/attachments";

type PendingRequest = {
  expectedType: "setup_config" | "setup_saved" | "providers" | "models" | "provider_tested" | "model_changed"
    | "agents" | "agent_saved" | "agent_deleted" | "tasks" | "task_saved" | "approvals" | "approval_resolved"
    | "memory" | "memory_saved" | "memory_deleted" | "schedules" | "schedule_saved" | "schedule_deleted" | "runtime_events"
    | "history_list";
  resolve: (value: ServerEvent) => void;
  reject: (reason?: unknown) => void;
};

let mainWindow: BrowserWindow | null = null;
let workspaceRoot = path.resolve(process.env.LUMA_WORKSPACE || process.cwd());

let lumaProcess: ChildProcessWithoutNullStreams | null = null;
let respawnAttempts = 0;
let intentionalShutdown = false;
const MAX_RESPAWN_ATTEMPTS = 3;

const pendingRequests = new Map<string, PendingRequest>();

let isLumaReady = false;
let lumaReadyResolver: (() => void) | null = null;
let lumaReadyRejecter: ((error: Error) => void) | null = null;
let lumaReadyPromise = new Promise<void>((resolve, reject) => {
  lumaReadyResolver = resolve;
  lumaReadyRejecter = reject;
});
void lumaReadyPromise.catch(() => undefined);
let startupTimer: ReturnType<typeof setTimeout> | null = null;
let respawnTimer: ReturnType<typeof setTimeout> | null = null;

function getLumaBinary(): string {
  if (process.env.LUMA_BINARY) return process.env.LUMA_BINARY;

  if (app.isPackaged) {
    const packagedBinary = path.join(
      process.resourcesPath,
      process.platform === "win32" ? "luma-core.exe" : "luma-core",
    );
    if (existsSync(packagedBinary)) return packagedBinary;
  }

  if (!app.isPackaged) {
    const localBuilds = [
      path.resolve(__dirname, "../../../target/debug/luma-core"),
      path.resolve(__dirname, "../../../target/release/luma-core"),
    ];
    const localBinary = localBuilds.find((candidate) => existsSync(candidate));
    if (localBinary) return localBinary;
  }

  return "luma-core";
}

function resolveWorkspacePath(relativePath: string): string {
  if (typeof relativePath !== "string" || path.isAbsolute(relativePath)) {
    throw new Error("Invalid workspace path.");
  }
  const resolved = path.resolve(workspaceRoot, relativePath);
  if (resolved !== workspaceRoot && !resolved.startsWith(`${workspaceRoot}${path.sep}`)) {
    throw new Error("Workspace paths cannot escape the selected folder.");
  }
  return resolved;
}

async function assertInsideWorkspace(filePath: string): Promise<string> {
  const realPath = await fs.realpath(filePath);
  if (realPath !== workspaceRoot && !realPath.startsWith(`${workspaceRoot}${path.sep}`)) {
    throw new Error("Workspace symlinks cannot access files outside the selected folder.");
  }
  return realPath;
}

type GitChange = {
  path: string;
  indexStatus: string;
  worktreeStatus: string;
  kind: "added" | "modified" | "deleted" | "renamed" | "untracked";
};

function runGit(args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn("git", ["-C", workspaceRoot, ...args], { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => { stdout += chunk; });
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => { stderr += chunk; });
    child.once("error", reject);
    child.once("close", (code) => {
      if (code === 0) resolve(stdout);
      else reject(new Error(stderr.trim() || `git ${args[0]} exited with code ${code ?? "unknown"}.`));
    });
  });
}

async function getGitChanges(): Promise<{ branch: string; files: GitChange[] }> {
  const branch = (await runGit(["branch", "--show-current"])).trim() || "HEAD";
  const status = await runGit(["status", "--porcelain=v1", "--untracked-files=all"]);
  const files = status.split(/\r?\n/).filter(Boolean).map((line): GitChange => {
    const indexStatus = line[0] ?? " ";
    const worktreeStatus = line[1] ?? " ";
    const rawPath = line.slice(3);
    const renamed = rawPath.includes(" -> ");
    const filePath = renamed ? rawPath.slice(rawPath.lastIndexOf(" -> ") + 4) : rawPath;
    const code = indexStatus === "?" ? "?" : indexStatus !== " " ? indexStatus : worktreeStatus;
    const kind: GitChange["kind"] = code === "?" ? "untracked" : code === "A" ? "added" : code === "D" ? "deleted" : code === "R" || renamed ? "renamed" : "modified";
    return { path: filePath, indexStatus, worktreeStatus, kind };
  });
  return { branch, files };
}

async function getGitDiff(relativePath: string): Promise<string> {
  const target = resolveWorkspacePath(relativePath);
  const stat = await fs.lstat(target).catch(() => null);
  if (!stat) {
    // A deleted tracked file still has a useful diff; Git resolves it by path.
    return runGit(["diff", "HEAD", "--no-ext-diff", "--unified=3", "--", relativePath]);
  }
  const tracked = await runGit(["ls-files", "--error-unmatch", "--", relativePath]).then(() => true, () => false);
  if (!tracked) {
    if (!stat.isFile()) return "Untracked directory. Select a file inside it to inspect its contents.";
    if (stat.size > 2 * 1024 * 1024) throw new Error("Untracked files larger than 2 MiB are not previewed.");
    const bytes = await fs.readFile(await assertInsideWorkspace(target));
    if (bytes.includes(0)) throw new Error("Binary files do not have a text diff preview.");
    const content = bytes.toString("utf8");
    return `New file: ${relativePath}\n\n${content.split("\n").map((line) => `+${line}`).join("\n")}`;
  }
  return runGit(["diff", "HEAD", "--no-ext-diff", "--unified=3", "--", relativePath]);
}

async function readWorkspaceTree(directory: string, relative = "", depth = 0): Promise<unknown[]> {
  if (depth > 8) return [];
  const ignored = new Set([".git", "node_modules", "target", ".next", "dist", "build", ".venv"]);
  const visibleHiddenDirectories = new Set([".github", ".agents", ".vscode", ".husky", ".devcontainer"]);
  const entries = await fs.readdir(directory, { withFileTypes: true });
  entries.sort((a, b) => Number(b.isDirectory()) - Number(a.isDirectory()) || a.name.localeCompare(b.name));
  const nodes: unknown[] = [];
  for (const entry of entries.slice(0, 2000)) {
    if (entry.name === ".DS_Store" || ignored.has(entry.name)) continue;
    if (entry.isDirectory() && entry.name.startsWith(".") && !visibleHiddenDirectories.has(entry.name)) continue;
    if (entry.isSymbolicLink()) continue;
    const relativePath = relative ? path.posix.join(relative, entry.name) : entry.name;
    if (entry.isDirectory()) {
      nodes.push({ type: "folder", name: entry.name, path: relativePath, children: await readWorkspaceTree(path.join(directory, entry.name), relativePath, depth + 1) });
    } else if (entry.isFile()) {
      nodes.push({ type: "file", name: entry.name, path: relativePath });
    }
  }
  return nodes;
}

function sendToLuma(message: ClientMessage) {
  if (!lumaProcess || lumaProcess.killed || !isLumaReady || !lumaProcess.stdin.writable) {
    throw new Error("Luma core is not ready. Wait for it to finish starting, then retry.");
  }

  lumaProcess.stdin.write(encodeClientMessage(message));
}

async function requestLuma(
  expectedType: PendingRequest["expectedType"],
  createMessage: (requestId: string) => ClientMessage,
): Promise<ServerEvent> {
  if (!isLumaReady) {
    await lumaReadyPromise;
  }
  if (!isLumaReady) throw new Error("Luma core stopped before the request could be sent.");

  return new Promise((resolve, reject) => {
    const requestId = randomUUID();
    const TIMEOUT_MS = 10000; // 10 seconds

    const timeout = setTimeout(() => {
      if (pendingRequests.has(requestId)) {
        pendingRequests.delete(requestId);
        reject(new Error(`Luma request timed out: ${expectedType}`));
      }
    }, TIMEOUT_MS);

    pendingRequests.set(requestId, {
      expectedType,
      resolve: (value) => {
        clearTimeout(timeout);
        resolve(value);
      },
      reject: (reason) => {
        clearTimeout(timeout);
        reject(reason);
      },
    });

    try {
      sendToLuma(createMessage(requestId));
    } catch (error) {
      clearTimeout(timeout);
      pendingRequests.delete(requestId);
      reject(error);
    }
  });
}

function startLuma() {
  if (lumaProcess) {
    return;
  }

  intentionalShutdown = false;
  isLumaReady = false;
  lumaReadyPromise = new Promise<void>((resolve, reject) => {
    lumaReadyResolver = resolve;
    lumaReadyRejecter = reject;
  });
  void lumaReadyPromise.catch(() => undefined);

  const binary = getLumaBinary();

  console.log(`[luma] Starting: ${binary} desktop-server`);

  let child: ChildProcessWithoutNullStreams;
  try {
    child = spawn(binary, ["desktop-server"], {
      cwd: workspaceRoot,
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (error) {
    handleLumaProcessFailure(error instanceof Error ? error : new Error(String(error)));
    return;
  }
  lumaProcess = child;

  startupTimer = setTimeout(() => {
    if (lumaProcess === child && !isLumaReady) {
      handleLumaProcessFailure(new Error("Luma core did not complete its startup handshake."));
      child.kill();
    }
  }, 15000);

  child.on("error", (error) => {
    if (lumaProcess !== child) return;
    handleLumaProcessFailure(error);
  });

  child.stdin.on("error", (error) => {
    if (lumaProcess !== child) return;
    handleLumaProcessFailure(error);
  });

  const stdout = readline.createInterface({
    input: child.stdout,
    crlfDelay: Infinity,
  });

  stdout.on("line", (line) => {
    if (lumaProcess !== child) return;
    if (!line.trim()) {
      return;
    }

    let event: ServerEvent;
    try {
      event = parseServerEvent(line);
    } catch (error) {
      handleLumaProcessFailure(
        new Error(`Invalid message from Luma core: ${error instanceof Error ? error.message : String(error)}`),
      );
      child.kill();
      return;
    }

    if (event.type === "ready") {
      try {
        assertProtocolVersion(event.data.version);
      } catch (error) {
        handleLumaProcessFailure(error instanceof Error ? error : new Error(String(error)));
        child.kill();
        return;
      }
      if (startupTimer) clearTimeout(startupTimer);
      startupTimer = null;
      isLumaReady = true;
      respawnAttempts = 0;
      lumaReadyResolver?.();
      lumaReadyResolver = null;
      lumaReadyRejecter = null;
      mainWindow?.webContents.send("luma:event", event);
      return;
    }

    const requestId = "data" in event
      && typeof event.data === "object"
      && event.data !== null
      && "request_id" in event.data
      ? event.data.request_id
      : null;

    if (typeof requestId === "string" && pendingRequests.has(requestId)) {
      const pending = pendingRequests.get(requestId);

      pendingRequests.delete(requestId);

      if (event.type === "error") {
        pending?.reject(
          new Error(event.data?.message ?? "Luma request failed."),
        );
      } else if (event.type !== pending?.expectedType) {
        pending?.reject(new Error(`Unexpected response to request: ${event.type}.`));
      } else {
        pending?.resolve(event);
      }

      return;
    }

    mainWindow?.webContents.send("luma:event", event);
  });

  child.stderr.on("data", (chunk) => {
    if (lumaProcess !== child) return;
    console.error("[luma]", chunk.toString());
  });

  child.on("exit", (code, signal) => {
    if (lumaProcess !== child) return;
    if (intentionalShutdown) {
      return;
    }

    handleLumaProcessFailure(`Luma exited with code=${code}, signal=${signal}`);
  });
}

function stopLuma() {
  intentionalShutdown = true;
  if (respawnTimer) clearTimeout(respawnTimer);
  respawnTimer = null;
  if (!lumaProcess) {
    return;
  }

  if (startupTimer) clearTimeout(startupTimer);
  startupTimer = null;
  lumaReadyRejecter?.(new Error("Luma core stopped before becoming ready."));
  lumaReadyRejecter = null;

  for (const pending of pendingRequests.values()) {
    pending.reject(new Error("Luma desktop server stopped."));
  }

  pendingRequests.clear();

  const child = lumaProcess;
  lumaProcess = null;
  isLumaReady = false;
  child.kill();
}

function handleLumaProcessFailure(error: Error | string) {
  if (intentionalShutdown) {
    return;
  }

  console.error("[luma] Process failure:", error);

  if (startupTimer) clearTimeout(startupTimer);
  startupTimer = null;
  isLumaReady = false;
  lumaReadyRejecter?.(typeof error === "string" ? new Error(error) : error);
  lumaReadyRejecter = null;

  for (const pending of pendingRequests.values()) {
    pending.reject(
      typeof error === "string" ? new Error(error) : error,
    );
  }

  pendingRequests.clear();
  const failedProcess = lumaProcess;
  lumaProcess = null;
  failedProcess?.kill();

  mainWindow?.webContents.send("luma:event", {
    type: "error",
    data: {
      message:
        typeof error === "string" ? error : `Luma error: ${error.message}`,
      request_id: null,
      category: "initialization_failed",
    },
  });

  if (respawnAttempts < MAX_RESPAWN_ATTEMPTS) {
    respawnAttempts++;
    console.log(
      `[luma] Respawning (attempt ${respawnAttempts}/${MAX_RESPAWN_ATTEMPTS})...`,
    );
    respawnTimer = setTimeout(() => {
      respawnTimer = null;
      if (!intentionalShutdown) startLuma();
    }, 1000 * respawnAttempts);
  }
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 820,

    minWidth: 900,
    minHeight: 600,

    title: "Luma",

    titleBarStyle: "hiddenInset",
    transparent: true,
    backgroundColor: "#00000000",
    // Let the OS compositor draw the material behind transparent renderer
    // surfaces instead of trying to imitate acrylic with CSS blur alone.
    ...(process.platform === "darwin"
      ? { vibrancy: "sidebar" as const, visualEffectState: "followWindow" as const }
      : process.platform === "win32"
        ? { backgroundMaterial: "acrylic" as const }
        : {}),

    webPreferences: {
      preload: path.join(__dirname, "preload.js"),

      contextIsolation: true,

      nodeIntegration: false,
    },
  });

  if (process.env.NODE_ENV === "development") {
    void mainWindow.loadURL("http://localhost:5173");
  } else {
    void mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  }

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

function openSettings() {
  mainWindow?.webContents.send("luma:open-settings");
}

function createApplicationMenu() {
  const isMac = process.platform === "darwin";

  const template: Electron.MenuItemConstructorOptions[] = [
    ...(isMac
      ? [
          {
            label: app.name,
            submenu: [
              { role: "about" as const },
              { type: "separator" as const },
              {
                label: "Settings…",
                accelerator: "CommandOrControl+,",
                click: () => openSettings(),
              },
              { type: "separator" as const },
              { role: "services" as const },
              { type: "separator" as const },
              { role: "hide" as const },
              { role: "hideOthers" as const },
              { role: "unhide" as const },
              { type: "separator" as const },
              { role: "quit" as const },
            ],
          },
        ]
      : []),
    {
      label: "File",
      submenu: [
        ...(isMac
          ? []
          : [
              {
                label: "Settings…",
                accelerator: "CommandOrControl+,",
                click: () => openSettings(),
              },
              { type: "separator" as const },
            ]),
        isMac ? { role: "close" as const } : { role: "quit" as const },
      ],
    },
    {
      label: "Edit",
      submenu: [
        { role: "undo" },
        { role: "redo" },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { role: "selectAll" },
      ],
    },
    {
      label: "View",
      submenu: [
        { role: "reload" },
        { role: "toggleDevTools" },
        { type: "separator" },
        { role: "resetZoom" },
        { role: "zoomIn" },
        { role: "zoomOut" },
        { type: "separator" },
        { role: "togglefullscreen" },
      ],
    },
    {
      label: "Window",
      submenu: [{ role: "minimize" }, { role: "zoom" }],
    },
  ];

  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

app.whenReady().then(async () => {
  const workspaceSettings = path.join(app.getPath("userData"), "workspace.json");
  let hasSavedWorkspace = false;
  try {
    const saved = JSON.parse(await fs.readFile(workspaceSettings, "utf8")) as { path?: unknown };
    if (typeof saved.path === "string") {
      workspaceRoot = saved.path;
      hasSavedWorkspace = true;
    }
  } catch {
    // First launch or an unavailable previous workspace: use the launch directory.
  }
  if (!process.env.LUMA_WORKSPACE && !hasSavedWorkspace) {
    const parent = path.resolve(process.cwd(), "..");
    try {
      await fs.access(path.join(parent, "Cargo.toml"));
      if (path.basename(process.cwd()) === "desktop") workspaceRoot = parent;
    } catch {
      // The launch directory is not a Luma repository; keep it as the workspace.
    }
  }
  try {
    workspaceRoot = await fs.realpath(workspaceRoot);
  } catch {
    workspaceRoot = await fs.realpath(process.cwd());
  }
  startLuma();
  createApplicationMenu();

  ipcMain.handle("luma:status", () => ({
    connected: isLumaReady,
  }));

  ipcMain.handle("luma:get-session", () => {
    sendToLuma({ type: "get_session" });
    return { ok: true };
  });

  ipcMain.handle("luma:get-history", async (): Promise<ChatSummary[]> => {
    const response = await requestLuma("history_list", (request_id) => ({ type: "history_list", data: { request_id } }));
    if (response.type !== "history_list") throw new Error("Unexpected response to history_list.");
    return response.data.chats;
  });

  ipcMain.handle("luma:restore-chat", (_event, id: string) => {
    sendToLuma({ type: "prompt", data: { text: `/restore ${id}` } });
    sendToLuma({ type: "get_session" });
    return { ok: true };
  });

  ipcMain.handle("luma:clear-session", () => {
    sendToLuma({ type: "prompt", data: { text: "/clear" } });
    sendToLuma({ type: "get_session" });
    return { ok: true };
  });

  ipcMain.handle("luma:get-workspace", () => ({ path: workspaceRoot }));
  ipcMain.handle("luma:choose-workspace", async () => {
    const result = await dialog.showOpenDialog(mainWindow!, {
      title: "Choose workspace folder",
      properties: ["openDirectory", "createDirectory"],
    });
    if (result.canceled || !result.filePaths[0]) return { path: workspaceRoot, canceled: true };
    workspaceRoot = await fs.realpath(result.filePaths[0]);
    await fs.mkdir(app.getPath("userData"), { recursive: true });
    await fs.writeFile(path.join(app.getPath("userData"), "workspace.json"), JSON.stringify({ path: workspaceRoot }), "utf8");
    stopLuma();
    startLuma();
    return { path: workspaceRoot, canceled: false };
  });
  ipcMain.handle("luma:list-workspace-files", async () => readWorkspaceTree(workspaceRoot));
  ipcMain.handle("luma:get-git-changes", async () => getGitChanges());
  ipcMain.handle("luma:get-git-diff", async (_event, relativePath: string) => {
    if (typeof relativePath !== "string" || !relativePath.trim()) throw new Error("A file path is required.");
    return getGitDiff(relativePath);
  });
  ipcMain.handle("luma:read-workspace-file", async (_event, relativePath: string) => {
    const target = await assertInsideWorkspace(resolveWorkspacePath(relativePath));
    const stat = await fs.stat(target);
    if (!stat.isFile() || stat.size > 2 * 1024 * 1024) throw new Error("Only files up to 2 MiB can be opened in the editor.");
    const contents = await fs.readFile(target);
    if (contents.includes(0)) throw new Error("Binary files cannot be opened in the text editor.");
    return contents.toString("utf8");
  });
  ipcMain.handle("luma:choose-chat-attachments", async (): Promise<ChatAttachment[]> => {
    const options: OpenDialogOptions = {
      title: "Attach files to chat",
      defaultPath: workspaceRoot,
      properties: ["openFile", "multiSelections"],
    };
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, options)
      : await dialog.showOpenDialog(options);
    if (result.canceled || result.filePaths.length === 0) return [];
    if (result.filePaths.length > 8) throw new Error("Attach at most 8 files at a time.");

    let totalBytes = 0;
    const attachments: ChatAttachment[] = [];
    for (const selectedPath of result.filePaths) {
      const realPath = await fs.realpath(selectedPath);
      const stat = await fs.stat(realPath);
      if (!stat.isFile()) throw new Error("Only files can be attached.");
      if (stat.size > 1024 * 1024) throw new Error("Each attached file must be 1 MiB or smaller.");
      totalBytes += stat.size;
      if (totalBytes > 4 * 1024 * 1024) throw new Error("The total attachment size cannot exceed 4 MiB.");

      const bytes = await fs.readFile(realPath);
      if (bytes.includes(0)) throw new Error(`${path.basename(realPath)} is binary and cannot be attached as text.`);
      let content: string;
      try {
        content = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      } catch {
        throw new Error(`${path.basename(realPath)} is not valid UTF-8 text.`);
      }
      const relative = path.relative(workspaceRoot, realPath);
      const name = relative && !relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative)
        ? relative
        : path.basename(realPath);
      attachments.push({ name, content });
    }
    return attachments;
  });
  ipcMain.handle("luma:write-workspace-file", async (_event, relativePath: string, content: string) => {
    const target = await assertInsideWorkspace(resolveWorkspacePath(relativePath));
    const stat = await fs.stat(target);
    if (!stat.isFile()) throw new Error("The selected path is not a file.");
    await fs.writeFile(target, content, "utf8");
    return { ok: true };
  });

  ipcMain.handle("luma:get-agents", async (): Promise<AgentRecord[]> => {
    const response = await requestLuma("agents", (request_id) => ({ type: "agents_list", data: { request_id } }));
    if (response.type !== "agents") throw new Error("Unexpected response to agents_list.");
    return response.data.agents;
  });

  ipcMain.handle("luma:create-agent", async (_event, input: { name: string; description?: string | null; kind: AgentKind; configuration?: AgentConfiguration }) => {
    const configuration = input.configuration ?? {
      system_prompt: "", mascot: "auto", use_global_context: true, provider: null, model: null, endpoint: null, workspace: workspaceRoot,
      skills: [],
      allowed_workspaces: [workspaceRoot], enabled_tools: [],
      permissions: { allowed_tools: ["read_file", "list_directory", "search_files", "git_status", "git_diff", "find_symbols", "find_references", "git_log", "git_show", "git_blame", "git_branches", "file_metadata", "find_todos", "count_lines", "inspect_project", "list_skills", "load_skill"], denied_tools: [], auto_approve_read_only: true, require_approval_for_mutations: true, allow_network: false },
      max_concurrent_tasks: 1, recover_interrupted_tasks: false,
    };
    const response = await requestLuma("agent_saved", (request_id) => ({ type: "agent_create", data: {
      request_id, name: input.name, description: input.description ?? null, kind: input.kind, configuration,
    } }));
    if (response.type !== "agent_saved") throw new Error("Unexpected response to agent_create.");
    return response.data.agent;
  });

  ipcMain.handle("luma:delete-agent", async (_event, id: string) => {
    const response = await requestLuma("agent_deleted", (request_id) => ({ type: "agent_delete", data: { request_id, id } }));
    if (response.type !== "agent_deleted") throw new Error("Unexpected response to agent_delete.");
    return response.data.deleted;
  });

  ipcMain.handle("luma:update-agent", async (_event, input: { id: string; name?: string | null; description?: string | null; configuration?: AgentConfiguration | null }) => {
    const response = await requestLuma("agent_saved", (request_id) => ({ type: "agent_update", data: {
      request_id, id: input.id, name: input.name ?? null, description: input.description ?? null, configuration: input.configuration ?? null,
    } }));
    if (response.type !== "agent_saved") throw new Error("Unexpected response to agent_update.");
    return response.data.agent;
  });

  ipcMain.handle("luma:set-agent-status", async (_event, id: string, status: "idle" | "paused" | "stopped") => {
    const response = await requestLuma("agent_saved", (request_id) => ({ type: "agent_set_status", data: { request_id, id, status } }));
    if (response.type !== "agent_saved") throw new Error("Unexpected response to agent_set_status.");
    return response.data.agent;
  });

  ipcMain.handle("luma:get-tasks", async (_event, agentId?: string): Promise<TaskRecord[]> => {
    const response = await requestLuma("tasks", (request_id) => ({ type: "tasks_list", data: { request_id, agent_id: agentId ?? null, limit: 500 } }));
    if (response.type !== "tasks") throw new Error("Unexpected response to tasks_list.");
    return response.data.tasks;
  });

  ipcMain.handle("luma:get-runtime-events", async (_event, agentId: string): Promise<RuntimeEvent[]> => {
    const response = await requestLuma("runtime_events", (request_id) => ({ type: "events_since", data: { request_id, sequence: 0, limit: 1000, agent_id: agentId } }));
    if (response.type !== "runtime_events") throw new Error("Unexpected response to events_since.");
    return response.data.events;
  });

  ipcMain.handle("luma:create-task", async (_event, input: { agentId: string; request: string; workspace?: string }) => {
    const response = await requestLuma("task_saved", (request_id) => ({ type: "task_create", data: {
      request_id, agent_id: input.agentId, request: input.request, title: null,
      workspace: input.workspace ?? workspaceRoot, recover_on_restart: false,
    } }));
    if (response.type !== "task_saved") throw new Error("Unexpected response to task_create.");
    return response.data.task;
  });

  ipcMain.handle("luma:cancel-task", async (_event, id: string) => {
    const response = await requestLuma("task_saved", (request_id) => ({ type: "task_cancel", data: { request_id, id } }));
    if (response.type !== "task_saved") throw new Error("Unexpected response to task_cancel.");
    return response.data.task;
  });

  ipcMain.handle("luma:retry-task", async (_event, id: string) => {
    const response = await requestLuma("task_saved", (request_id) => ({ type: "task_retry", data: { request_id, id } }));
    if (response.type !== "task_saved") throw new Error("Unexpected response to task_retry.");
    return response.data.task;
  });

  ipcMain.handle("luma:get-approvals", async (): Promise<ApprovalRecord[]> => {
    const response = await requestLuma("approvals", (request_id) => ({ type: "approvals_list", data: { request_id } }));
    if (response.type !== "approvals") throw new Error("Unexpected response to approvals_list.");
    return response.data.approvals;
  });

  ipcMain.handle("luma:resolve-approval", async (_event, id: string, allowed: boolean) => {
    const response = await requestLuma("approval_resolved", (request_id) => ({ type: "approval_resolve", data: { request_id, id, allowed } }));
    if (response.type !== "approval_resolved") throw new Error("Unexpected response to approval_resolve.");
    return response.data.approval;
  });

  ipcMain.handle("luma:get-memory", async (_event, scope: MemoryScope, scopeId: string): Promise<MemoryEntry[]> => {
    const response = await requestLuma("memory", (request_id) => ({ type: "memory_list", data: { request_id, scope, scope_id: scopeId, query: null, limit: 200 } }));
    if (response.type !== "memory") throw new Error("Unexpected response to memory_list.");
    return response.data.entries;
  });

  ipcMain.handle("luma:add-memory", async (_event, scope: MemoryScope, scopeId: string, content: string) => {
    const response = await requestLuma("memory_saved", (request_id) => ({ type: "memory_add", data: { request_id, scope, scope_id: scopeId, content, source: "desktop" } }));
    if (response.type !== "memory_saved") throw new Error("Unexpected response to memory_add.");
    return response.data.entry;
  });

  ipcMain.handle("luma:delete-memory", async (_event, id: string) => {
    const response = await requestLuma("memory_deleted", (request_id) => ({ type: "memory_delete", data: { request_id, id } }));
    if (response.type !== "memory_deleted") throw new Error("Unexpected response to memory_delete.");
    return response.data.deleted;
  });

  ipcMain.handle("luma:get-schedules", async (_event, agentId?: string): Promise<ScheduleRecord[]> => {
    const response = await requestLuma("schedules", (request_id) => ({ type: "schedules_list", data: { request_id, agent_id: agentId ?? null } }));
    if (response.type !== "schedules") throw new Error("Unexpected response to schedules_list.");
    return response.data.schedules;
  });

  ipcMain.handle("luma:create-schedule", async (_event, input: { agentId: string; title: string; prompt: string; everySeconds: number }) => {
    const response = await requestLuma("schedule_saved", (request_id) => ({ type: "schedule_create", data: {
      request_id, agent_id: input.agentId, title: input.title, prompt: input.prompt, run_at: null, every_seconds: input.everySeconds,
    } }));
    if (response.type !== "schedule_saved") throw new Error("Unexpected response to schedule_create.");
    return response.data.schedule;
  });

  ipcMain.handle("luma:delete-schedule", async (_event, id: string) => {
    const response = await requestLuma("schedule_deleted", (request_id) => ({ type: "schedule_delete", data: { request_id, id } }));
    if (response.type !== "schedule_deleted") throw new Error("Unexpected response to schedule_delete.");
    return response.data.deleted;
  });

  ipcMain.handle("luma:get-providers", async () => {
    const response = await requestLuma("providers", (request_id) => ({
      type: "get_providers", data: { request_id },
    }));
    if (response.type !== "providers") throw new Error("Unexpected response to get_providers.");
    return response.data.providers;
  });

  ipcMain.handle(
    "luma:get-models",
    async (
      _event,
      provider: unknown,
      options?: {
        apiKey?: string;
        endpoint?: string;
      },
    ) => {
      if (!isSupportedProvider(provider)) throw new Error("Invalid provider payload.");
      const response = await requestLuma("models", (request_id) => ({
        type: "get_models",
        data: { request_id, provider, api_key: options?.apiKey ?? null, endpoint: options?.endpoint ?? null },
      }));
      if (response.type !== "models") throw new Error("Unexpected response to get_models.");
      return response.data.models;
    },
  );

  ipcMain.handle("luma:get-setup-config", async () => {
    const response = await requestLuma("setup_config", (request_id) => ({
      type: "get_setup_config", data: { request_id },
    }));
    if (response.type !== "setup_config") throw new Error("Unexpected response to get_setup_config.");
    return response.data.config;
  });

  ipcMain.handle("luma:save-setup-config", async (_event, config: unknown) => {
    if (!isAppConfigInput(config)) throw new Error("Invalid setup configuration payload.");
    const response = await requestLuma("setup_saved", (request_id) => ({
      type: "save_setup_config", data: { request_id, config },
    }));
    if (response.type !== "setup_saved") throw new Error("Unexpected response to save_setup_config.");

    const changed = await requestLuma("model_changed", (request_id) => ({
      type: "set_model", data: { request_id, model: config.model.name },
    }));
    if (changed.type !== "model_changed") throw new Error("Unexpected response to set_model.");

    return {
      ok: true,
    };
  });

  ipcMain.handle(
    "luma:test-provider",
    async (
      _event,
      provider: unknown,
      options: {
        apiKey?: string;
        endpoint?: string;
        model: string;
      },
    ) => {
      if (!isSupportedProvider(provider)) throw new Error("Invalid provider payload.");
      const response = await requestLuma("provider_tested", (request_id) => ({
        type: "test_provider",
        data: { request_id, provider, api_key: options.apiKey ?? null, endpoint: options.endpoint ?? null, model: options.model },
      }));
      if (response.type !== "provider_tested") throw new Error("Unexpected response to test_provider.");

      return {
        ok: true,
      };
    },
  );

  ipcMain.handle("luma:prompt", async (_event, text: string) => {
    sendToLuma({
      type: "prompt",

      data: {
        text,
      },
    });

    return {
      ok: true,
    };
  });

  ipcMain.handle("luma:confirm", async (_event, allowed: boolean) => {
    sendToLuma({
      type: "confirm",

      data: {
        allowed,
      },
    });

    return {
      ok: true,
    };
  });

  ipcMain.handle("luma:cancel", async () => {
    sendToLuma({
      type: "cancel",
    });

    return {
      ok: true,
    };
  });

  createWindow();
});

app.on("window-all-closed", () => {
  stopLuma();

  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", () => {
  stopLuma();
});
