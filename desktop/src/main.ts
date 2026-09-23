import { app, BrowserWindow, ipcMain } from "electron";

import path from "node:path";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

import readline from "node:readline";

import { randomUUID } from "node:crypto";

type PendingRequest = {
  resolve: (value: unknown) => void;
  reject: (reason?: unknown) => void;
};

let mainWindow: BrowserWindow | null = null;

let lumaProcess: ChildProcessWithoutNullStreams | null = null;

const pendingRequests = new Map<string, PendingRequest>();

function getLumaBinary(): string {
  return process.env.LUMA_BINARY || "luma";
}

function sendToLuma(message: unknown) {
  if (!lumaProcess || lumaProcess.killed) {
    throw new Error("Luma desktop server is not running.");
  }

  lumaProcess.stdin.write(`${JSON.stringify(message)}\n`);
}

function requestLuma(
  type: string,
  data: Record<string, unknown> = {},
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const requestId = randomUUID();

    pendingRequests.set(requestId, {
      resolve,
      reject,
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
      pendingRequests.delete(requestId);
      reject(error);
    }
  });
}

function startLuma() {
  if (lumaProcess) {
    return;
  }

  const binary = getLumaBinary();

  console.log(`[luma] Starting: ${binary} desktop-server`);

  lumaProcess = spawn(binary, ["desktop-server"], {
    cwd: process.cwd(),
    stdio: ["pipe", "pipe", "pipe"],
  });

  lumaProcess.on("error", (error) => {
    console.error(`[luma] Failed to start "${binary}":`, error);

    for (const pending of pendingRequests.values()) {
      pending.reject(error);
    }

    pendingRequests.clear();

    if (lumaProcess) {
      lumaProcess = null;
    }

    mainWindow?.webContents.send("luma:event", {
      type: "Error",
      data: {
        message: `Failed to start Luma: ${error.message}`,
      },
    });
  });

  const stdout = readline.createInterface({
    input: lumaProcess.stdout,
    crlfDelay: Infinity,
  });

  stdout.on("line", (line) => {
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

    const requestId = event?.data?.request_id;

    if (typeof requestId === "string" && pendingRequests.has(requestId)) {
      const pending = pendingRequests.get(requestId);

      pendingRequests.delete(requestId);

      if (event.type === "Error") {
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

  lumaProcess.stderr.on("data", (chunk) => {
    console.error("[luma]", chunk.toString());
  });

  lumaProcess.on("exit", (code, signal) => {
    console.log(`[luma] Exited: code=${code}, signal=${signal}`);

    for (const pending of pendingRequests.values()) {
      pending.reject(new Error("Luma desktop server exited."));
    }

    pendingRequests.clear();

    lumaProcess = null;

    mainWindow?.webContents.send("luma:event", {
      type: "Error",
      data: {
        message: "Luma desktop server exited.",
      },
    });
  });
}

function stopLuma() {
  if (!lumaProcess) {
    return;
  }

  for (const pending of pendingRequests.values()) {
    pending.reject(new Error("Luma desktop server stopped."));
  }

  pendingRequests.clear();

  lumaProcess.kill();

  lumaProcess = null;
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

app.whenReady().then(() => {
  startLuma();

  ipcMain.handle("luma:status", () => ({
    connected: lumaProcess !== null && !lumaProcess.killed,
  }));

  ipcMain.handle("luma:get-providers", async () => {
    const response = (await requestLuma("GetProviders")) as {
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
      const response = (await requestLuma("GetModels", {
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
    const response = (await requestLuma("GetSetupConfig")) as {
      config?: unknown;
    };

    return response.config ?? null;
  });

  ipcMain.handle("luma:save-setup-config", async (_event, config: unknown) => {
    await requestLuma("SaveSetupConfig", {
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
      await requestLuma("TestProvider", {
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
      type: "Prompt",

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
      type: "Confirm",

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
      type: "Cancel",

      data: {},
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
