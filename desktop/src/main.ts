import { app, BrowserWindow, dialog, ipcMain, Menu, shell, type OpenDialogOptions } from "electron";

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
  type ModelConfig,
  type PluginEntry,
  type McpServerConfig,
} from "../../shared/protocol";
import type { ChatAttachment } from "../../shared/attachments";

type PendingRequest = {
  expectedType: "setup_config" | "setup_saved" | "providers" | "models" | "provider_tested" | "model_changed"
    | "agents" | "agent_saved" | "agent_deleted" | "tasks" | "task_saved" | "task_deleted" | "approvals" | "approval_resolved"
    | "memory" | "memory_saved" | "memory_deleted" | "schedules" | "schedule_saved" | "schedule_deleted" | "runtime_events"
    | "plugins" | "plugin_saved" | "plugin_deleted" | "mcp_servers" | "mcp_saved" | "mcp_deleted"
    | "history_list";
  resolve: (value: ServerEvent) => void;
  reject: (reason?: unknown) => void;
};

let mainWindow: BrowserWindow | null = null;
let workspaceRoot = path.resolve(process.env.LUMA_WORKSPACE || process.cwd());
let defaultWorkspaceRoot = workspaceRoot;

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

    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : process.platform === "win32" ? "hidden" : "default",
    ...(process.platform === "darwin" ? { trafficLightPosition: { x: 18, y: 20 } } : {}),
    transparent: process.platform !== "linux",
    backgroundColor: process.platform === "linux" ? "#101010" : "#00000000",
    // Let the OS compositor draw the material behind transparent renderer
    // surfaces instead of trying to imitate acrylic with CSS blur alone.
    ...(process.platform === "darwin"
      ? { vibrancy: "sidebar" as const, visualEffectState: "followWindow" as const }
      : process.platform === "win32"
        ? {
            backgroundMaterial: "acrylic" as const,
            titleBarOverlay: { color: "#171717", symbolColor: "#efede7", height: 48 },
          }
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
        {
          label: "New Chat",
          accelerator: "CommandOrControl+N",
          click: () => mainWindow?.webContents.send("luma:new-chat"),
        },
        {
          label: "Open Workspace…",
          accelerator: "CommandOrControl+Shift+O",
          click: () => mainWindow?.webContents.send("luma:open-workspace"),
        },
        { type: "separator" as const },
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
        {
          label: "Toggle Sidebar",
          accelerator: "CommandOrControl+B",
          click: () => mainWindow?.webContents.send("luma:toggle-sidebar"),
        },
        { type: "separator" },
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
  defaultWorkspaceRoot = workspaceRoot;
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

  ipcMain.handle("luma:quit", () => {
    app.quit();
    return { ok: true };
  });

  ipcMain.handle("luma:get-workspace", () => ({ path: workspaceRoot }));
  ipcMain.handle("luma:get-default-workspace", () => ({ path: defaultWorkspaceRoot }));
  ipcMain.handle("luma:choose-project-workspace", async () => {
    const options: OpenDialogOptions = {
      title: "Choose Project folder",
      defaultPath: workspaceRoot,
      properties: ["openDirectory", "createDirectory"],
    };
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, options)
      : await dialog.showOpenDialog(options);
    if (result.canceled || !result.filePaths[0]) return { path: null, canceled: true };
    const selectedPath = await fs.realpath(result.filePaths[0]);
    const stat = await fs.stat(selectedPath);
    if (!stat.isDirectory()) throw new Error("A Project workspace must be a folder.");
    return { path: selectedPath, canceled: false };
  });
  ipcMain.handle("luma:set-workspace", async (_event, requestedPath: string) => {
    if (typeof requestedPath !== "string" || !requestedPath.trim()) {
      throw new Error("A workspace folder is required.");
    }
    const selectedPath = await fs.realpath(requestedPath);
    const stat = await fs.stat(selectedPath);
    if (!stat.isDirectory()) throw new Error("The workspace must be a folder.");
    if (selectedPath === workspaceRoot) return { path: workspaceRoot };
    workspaceRoot = selectedPath;
    stopLuma();
    startLuma();
    await lumaReadyPromise;
    mainWindow?.webContents.send("luma:workspace-changed", { path: workspaceRoot });
    return { path: workspaceRoot };
  });
  ipcMain.handle("luma:choose-workspace", async () => {
    const result = await dialog.showOpenDialog(mainWindow!, {
      title: "Choose workspace folder",
      properties: ["openDirectory", "createDirectory"],
    });
    if (result.canceled || !result.filePaths[0]) return { path: workspaceRoot, canceled: true };
    workspaceRoot = await fs.realpath(result.filePaths[0]);
    defaultWorkspaceRoot = workspaceRoot;
    await fs.mkdir(app.getPath("userData"), { recursive: true });
    await fs.writeFile(path.join(app.getPath("userData"), "workspace.json"), JSON.stringify({ path: workspaceRoot }), "utf8");
    stopLuma();
    startLuma();
    await lumaReadyPromise;
    mainWindow?.webContents.send("luma:workspace-changed", { path: workspaceRoot });
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

  ipcMain.handle("luma:rename-task", async (_event, id: string, title: string) => {
    const response = await requestLuma("task_saved", (request_id) => ({ type: "task_rename", data: { request_id, id, title } }));
    if (response.type !== "task_saved") throw new Error("Unexpected response to task_rename.");
    return response.data.task;
  });

  ipcMain.handle("luma:delete-task", async (_event, id: string) => {
    const response = await requestLuma("task_deleted", (request_id) => ({ type: "task_delete", data: { request_id, id } }));
    if (response.type !== "task_deleted") throw new Error("Unexpected response to task_delete.");
    return response.data.deleted;
  });

  ipcMain.handle("luma:get-skills", async () => {
    const root = path.join(app.getPath("appData"), "luma", "skills");
    await fs.mkdir(root, { recursive: true });
    const entries = await fs.readdir(root, { withFileTypes: true });
    return Promise.all(entries.filter((entry) => entry.isDirectory()).map(async (entry) => {
      const skillPath = path.join(root, entry.name, "SKILL.md");
      try { return { name: entry.name, content: await fs.readFile(skillPath, "utf8") }; }
      catch { return { name: entry.name, content: "" }; }
    }));
  });

  ipcMain.handle("luma:create-skill", async (_event, name: string, description: string, instructions: string) => {
    if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(name)) throw new Error("Skill names must use lowercase letters, numbers, and hyphens.");
    if (!description.trim() || !instructions.trim()) throw new Error("A description and instructions are required.");
    const root = path.join(app.getPath("appData"), "luma", "skills");
    const directory = path.join(root, name);
    await fs.mkdir(root, { recursive: true });
    await fs.mkdir(directory);
    const content = `---\nname: ${name}\ndescription: ${JSON.stringify(description.trim().replace(/[\r\n]+/g, " "))}\n---\n\n${instructions.trim()}\n`;
    if (new TextEncoder().encode(content).byteLength > 256 * 1024) throw new Error("A skill must be 256 KiB or smaller.");
    await fs.writeFile(path.join(directory, "SKILL.md"), content, { encoding: "utf8", flag: "wx" });
    return { name, content };
  });

  ipcMain.handle("luma:save-skill", async (_event, name: string, content: string) => {
    if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(name)) throw new Error("Invalid skill name.");
    if (typeof content !== "string" || !content.trim()) throw new Error("Skill content cannot be empty.");
    if (new TextEncoder().encode(content).byteLength > 256 * 1024) throw new Error("A skill must be 256 KiB or smaller.");
    const directory = path.join(app.getPath("appData"), "luma", "skills", name);
    const target = path.join(directory, "SKILL.md");
    await fs.access(directory);
    await fs.writeFile(target, content.endsWith("\n") ? content : `${content}\n`, "utf8");
    return { name, content };
  });

  ipcMain.handle("luma:delete-skill", async (_event, name: string) => {
    if (!/^[a-z0-9][a-z0-9-]{0,63}$/.test(name)) throw new Error("Invalid skill name.");
    const directory = path.join(app.getPath("appData"), "luma", "skills", name);
    await fs.rm(directory, { recursive: true, force: false });
    return { deleted: true };
  });

  ipcMain.handle("luma:open-extension-folder", async (_event, kind: "skills" | "plugins" | "mcp") => {
    const folder = path.join(app.getPath("appData"), "luma", kind === "mcp" ? "mcp" : kind);
    await fs.mkdir(folder, { recursive: true });
    const error = await shell.openPath(folder);
    if (error) throw new Error(error);
    return { ok: true };
  });

  ipcMain.handle("luma:choose-plugin-package", async () => {
    const result = await dialog.showOpenDialog(mainWindow!, { properties: ["openDirectory"] });
    return result.canceled ? null : result.filePaths[0] ?? null;
  });
  ipcMain.handle("luma:get-plugins", async (): Promise<PluginEntry[]> => {
    const response = await requestLuma("plugins", (request_id) => ({ type: "plugins_list", data: { request_id } }));
    if (response.type !== "plugins") throw new Error("Unexpected response to plugins_list.");
    return response.data.plugins;
  });
  ipcMain.handle("luma:install-plugin", async (_event, pluginPath: string) => {
    const response = await requestLuma("plugin_saved", (request_id) => ({ type: "plugin_install", data: { request_id, path: pluginPath } }));
    if (response.type !== "plugin_saved") throw new Error("Unexpected response to plugin_install.");
    return response.data.plugin;
  });
  ipcMain.handle("luma:set-plugin-enabled", async (_event, id: string, enabled: boolean) => {
    const response = await requestLuma("plugin_saved", (request_id) => ({ type: "plugin_set_enabled", data: { request_id, id, enabled } }));
    if (response.type !== "plugin_saved") throw new Error("Unexpected response to plugin_set_enabled.");
    return response.data.plugin;
  });
  ipcMain.handle("luma:uninstall-plugin", async (_event, id: string) => {
    const response = await requestLuma("plugin_deleted", (request_id) => ({ type: "plugin_uninstall", data: { request_id, id } }));
    if (response.type !== "plugin_deleted") throw new Error("Unexpected response to plugin_uninstall.");
    return response.data.deleted;
  });
  ipcMain.handle("luma:get-mcp-servers", async (): Promise<McpServerConfig[]> => {
    const response = await requestLuma("mcp_servers", (request_id) => ({ type: "mcp_list", data: { request_id } }));
    if (response.type !== "mcp_servers") throw new Error("Unexpected response to mcp_list.");
    return response.data.servers;
  });
  ipcMain.handle("luma:add-mcp-stdio", async (_event, input: { id: string; command: string; args: string[]; env: Record<string, string> }) => {
    const response = await requestLuma("mcp_saved", (request_id) => ({ type: "mcp_add_stdio", data: { request_id, ...input } }));
    if (response.type !== "mcp_saved") throw new Error("Unexpected response to mcp_add_stdio.");
    return response.data.server;
  });
  ipcMain.handle("luma:add-mcp-http", async (_event, input: { id: string; url: string; headers: Record<string, string> }) => {
    const response = await requestLuma("mcp_saved", (request_id) => ({ type: "mcp_add_http", data: { request_id, ...input } }));
    if (response.type !== "mcp_saved") throw new Error("Unexpected response to mcp_add_http.");
    return response.data.server;
  });
  ipcMain.handle("luma:set-mcp-enabled", async (_event, id: string, enabled: boolean) => {
    const response = await requestLuma("mcp_saved", (request_id) => ({ type: "mcp_set_enabled", data: { request_id, id, enabled } }));
    if (response.type !== "mcp_saved") throw new Error("Unexpected response to mcp_set_enabled.");
    return response.data.server;
  });
  ipcMain.handle("luma:update-mcp", async (_event, server: McpServerConfig) => {
    const response = await requestLuma("mcp_saved", (request_id) => ({ type: "mcp_update", data: { request_id, server } }));
    if (response.type !== "mcp_saved") throw new Error("Unexpected response to mcp_update.");
    return response.data.server;
  });
  ipcMain.handle("luma:remove-mcp", async (_event, id: string) => {
    const response = await requestLuma("mcp_deleted", (request_id) => ({ type: "mcp_remove", data: { request_id, id } }));
    if (response.type !== "mcp_deleted") throw new Error("Unexpected response to mcp_remove.");
    return response.data.deleted;
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

  ipcMain.handle("luma:set-schedule-enabled", async (_event, id: string, enabled: boolean) => {
    const response = await requestLuma("schedule_saved", (request_id) => ({ type: "schedule_set_enabled", data: { request_id, id, enabled } }));
    if (response.type !== "schedule_saved") throw new Error("Unexpected response to schedule_set_enabled.");
    return response.data.schedule;
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

  ipcMain.handle("luma:set-session-model", async (_event, model: unknown) => {
    if (typeof model !== "object" || model === null
      || !("provider" in model) || typeof model.provider !== "string"
      || !("endpoint" in model) || typeof model.endpoint !== "string"
      || !("name" in model) || typeof model.name !== "string"
      || !("api_key" in model) || !(model.api_key === null || typeof model.api_key === "string")) {
      throw new Error("Invalid session model configuration.");
    }
    const sessionModel: ModelConfig = {
      provider: model.provider as string,
      endpoint: model.endpoint as string,
      name: model.name as string,
      api_key: model.api_key as string | null,
    };
    const changed = await requestLuma("model_changed", (request_id) => ({
      type: "set_session_model", data: { request_id, model: sessionModel },
    }));
    if (changed.type !== "model_changed") throw new Error("Unexpected response to set_session_model.");
    return { ok: true };
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

  ipcMain.handle("luma:answer-question", async (_event, questionId: string, answer: string) => {
    if (typeof questionId !== "string" || !questionId.trim() || typeof answer !== "string" || !answer.trim()) {
      throw new Error("A question ID and non-empty answer are required.");
    }
    sendToLuma({ type: "answer_question", data: { question_id: questionId, answer } });
    return { ok: true };
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
