import { app, BrowserWindow, dialog, ipcMain, Menu } from "electron";

import path from "node:path";
import fs from "node:fs/promises";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

import readline from "node:readline";

import { randomUUID } from "node:crypto";

type PendingRequest = {
  resolve: (value: unknown) => void;
  reject: (reason?: unknown) => void;
};

type LumaRequestType =
  | "get_providers"
  | "get_models"
  | "get_setup_config"
  | "save_setup_config"
  | "test_provider";

let mainWindow: BrowserWindow | null = null;
let workspaceRoot = path.resolve(process.env.LUMA_WORKSPACE || process.cwd());

let lumaProcess: ChildProcessWithoutNullStreams | null = null;
let respawnAttempts = 0;
let intentionalShutdown = false;
const MAX_RESPAWN_ATTEMPTS = 3;

const pendingRequests = new Map<string, PendingRequest>();

let isLumaReady = false;
let lumaReadyResolver: (() => void) | null = null;
let lumaReadyPromise = new Promise<void>((resolve) => {
  lumaReadyResolver = resolve;
});

function getLumaBinary(): string {
  return process.env.LUMA_BINARY || "luma";
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

function sendToLuma(message: unknown) {
  if (!lumaProcess || lumaProcess.killed) {
    throw new Error("Luma desktop server is not running.");
  }

  lumaProcess.stdin.write(`${JSON.stringify(message)}\n`);
}

async function requestLuma(
  type: LumaRequestType,
  data: Record<string, unknown> = {},
): Promise<unknown> {
  if (!isLumaReady) {
    await lumaReadyPromise;
  }

  return new Promise((resolve, reject) => {
    const requestId = randomUUID();
    const TIMEOUT_MS = 10000; // 10 seconds

    const timeout = setTimeout(() => {
      if (pendingRequests.has(requestId)) {
        pendingRequests.delete(requestId);
        reject(new Error(`Luma request timed out: ${type}`));
      }
    }, TIMEOUT_MS);

    pendingRequests.set(requestId, {
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
      sendToLuma({
        type,
        data: {
          ...data,
          request_id: requestId,
        },
      });
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
  lumaReadyPromise = new Promise<void>((resolve) => {
    lumaReadyResolver = resolve;
  });

  const binary = getLumaBinary();

  console.log(`[luma] Starting: ${binary} desktop-server`);

  const child = spawn(binary, ["desktop-server"], {
    cwd: workspaceRoot,
    stdio: ["pipe", "pipe", "pipe"],
  });
  lumaProcess = child;

  child.on("error", (error) => {
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

    let event: any;

    try {
      event = JSON.parse(line);
    } catch {
      console.error("[luma] Invalid JSON:", line);

      return;
    }

    if (event.type === "ready") {
      isLumaReady = true;
      respawnAttempts = 0;
      lumaReadyResolver?.();
      return;
    }

    const requestId = event?.data?.request_id;

    if (typeof requestId === "string" && pendingRequests.has(requestId)) {
      const pending = pendingRequests.get(requestId);

      pendingRequests.delete(requestId);

      if (event.type === "error") {
        pending?.reject(
          new Error(event.data?.message ?? "Luma request failed."),
        );
      } else {
        pending?.resolve(event.data);
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

    if (code !== 0) {
      handleLumaProcessFailure(
        `Luma exited with code=${code}, signal=${signal}`,
      );
      return;
    }

    lumaProcess = null;
  });
}

function stopLuma() {
  if (!lumaProcess) {
    return;
  }

  intentionalShutdown = true;

  for (const pending of pendingRequests.values()) {
    pending.reject(new Error("Luma desktop server stopped."));
  }

  pendingRequests.clear();

  lumaProcess.kill();

  lumaProcess = null;
}

function handleLumaProcessFailure(error: Error | string) {
  if (intentionalShutdown) {
    return;
  }

  console.error("[luma] Process failure:", error);

  for (const pending of pendingRequests.values()) {
    pending.reject(
      typeof error === "string" ? new Error(error) : error,
    );
  }

  pendingRequests.clear();
  lumaProcess = null;

  mainWindow?.webContents.send("luma:event", {
    type: "error",
    data: {
      message:
        typeof error === "string" ? error : `Luma error: ${error.message}`,
      category: "transient",
    },
  });

  if (respawnAttempts < MAX_RESPAWN_ATTEMPTS) {
    respawnAttempts++;
    console.log(
      `[luma] Respawning (attempt ${respawnAttempts}/${MAX_RESPAWN_ATTEMPTS})...`,
    );
    setTimeout(startLuma, 1000 * respawnAttempts);
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
    connected: lumaProcess !== null && !lumaProcess.killed,
  }));

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
  ipcMain.handle("luma:read-workspace-file", async (_event, relativePath: string) => {
    const target = await assertInsideWorkspace(resolveWorkspacePath(relativePath));
    const stat = await fs.stat(target);
    if (!stat.isFile() || stat.size > 2 * 1024 * 1024) throw new Error("Only files up to 2 MiB can be opened in the editor.");
    const contents = await fs.readFile(target);
    if (contents.includes(0)) throw new Error("Binary files cannot be opened in the text editor.");
    return contents.toString("utf8");
  });
  ipcMain.handle("luma:write-workspace-file", async (_event, relativePath: string, content: string) => {
    const target = await assertInsideWorkspace(resolveWorkspacePath(relativePath));
    const stat = await fs.stat(target);
    if (!stat.isFile()) throw new Error("The selected path is not a file.");
    await fs.writeFile(target, content, "utf8");
    return { ok: true };
  });

  ipcMain.handle("luma:get-providers", async () => {
    const response = (await requestLuma("get_providers")) as {
      providers?: unknown;
    };

    return Array.isArray(response.providers) ? response.providers : [];
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
      const response = (await requestLuma("get_models", {
        provider,

        api_key: options?.apiKey ?? null,

        endpoint: options?.endpoint ?? null,
      })) as {
        models?: unknown;
      };

      return Array.isArray(response.models) ? response.models : [];
    },
  );

  ipcMain.handle("luma:get-setup-config", async () => {
    const response = (await requestLuma("get_setup_config")) as {
      config?: unknown;
    };

    return response.config ?? null;
  });

  ipcMain.handle("luma:save-setup-config", async (_event, config: unknown) => {
    await requestLuma("save_setup_config", {
      config,
    });

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
      await requestLuma("test_provider", {
        provider,

        api_key: options.apiKey ?? null,

        endpoint: options.endpoint ?? null,

        model: options.model,
      });

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
