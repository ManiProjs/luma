import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import readline from "node:readline";

declare const MAIN_WINDOW_VITE_DEV_SERVER_URL: string;
declare const MAIN_WINDOW_VITE_NAME: string;

let mainWindow: BrowserWindow | null = null;
let lumaProcess: ChildProcessWithoutNullStreams | null = null;
let lumaReady = false;

const createWindow = () => {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1100,
    minHeight: 700,
    title: "Luma",
    titleBarStyle: "hiddenInset",
    backgroundColor: "#09090b",

    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (MAIN_WINDOW_VITE_DEV_SERVER_URL) {
    mainWindow.loadURL(MAIN_WINDOW_VITE_DEV_SERVER_URL);
  } else {
    mainWindow.loadFile(
      path.join(__dirname, `../renderer/${MAIN_WINDOW_VITE_NAME}/index.html`),
    );
  }

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
};

// ============================================================
// Luma process
// ============================================================

const getLumaBinary = () => {
  return process.env.LUMA_BINARY || "luma";
};

const sendToRenderer = (event: unknown) => {
  if (!mainWindow) {
    return;
  }

  mainWindow.webContents.send("luma:event", event);
};

const startLuma = () => {
  if (lumaProcess) {
    return;
  }

  const binary = getLumaBinary();

  console.log(`[Luma] Starting ${binary} desktop-server`);

  lumaProcess = spawn(binary, ["desktop-server"], {
    cwd: process.cwd(),

    env: {
      ...process.env,
      LUMA_DESKTOP: "1",
    },

    stdio: ["pipe", "pipe", "pipe"],
  });

  // ----------------------------------------------------------
  // stdout
  // ----------------------------------------------------------

  const stdout = readline.createInterface({
    input: lumaProcess.stdout,
    crlfDelay: Infinity,
  });

  stdout.on("line", (line) => {
    const trimmed = line.trim();

    if (!trimmed) {
      return;
    }

    console.log(`[Luma] ${trimmed}`);

    try {
      const event = JSON.parse(trimmed);

      if (event.type === "Ready") {
        lumaReady = true;

        console.log("[Luma] Desktop server ready");
      }

      sendToRenderer(event);
    } catch (error) {
      console.error("[Luma] Invalid JSON from desktop server:", trimmed, error);

      sendToRenderer({
        type: "Error",
        data: {
          message: "Luma returned invalid JSON.",
        },
      });
    }
  });

  // ----------------------------------------------------------
  // stderr
  // ----------------------------------------------------------

  const stderr = readline.createInterface({
    input: lumaProcess.stderr,
    crlfDelay: Infinity,
  });

  stderr.on("line", (line) => {
    if (!line.trim()) {
      return;
    }

    console.error(`[Luma] ${line}`);
  });

  // ----------------------------------------------------------
  // process errors
  // ----------------------------------------------------------

  lumaProcess.on("error", (error) => {
    console.error("[Luma] Failed to start:", error);

    lumaReady = false;

    sendToRenderer({
      type: "Error",
      data: {
        message: `Failed to start Luma: ${error.message}`,
      },
    });

    lumaProcess = null;
  });

  // ----------------------------------------------------------
  // process exit
  // ----------------------------------------------------------

  lumaProcess.on("exit", (code, signal) => {
    console.log(`[Luma] Process exited: code=${code} signal=${signal}`);

    lumaReady = false;
    lumaProcess = null;

    sendToRenderer({
      type: "ProcessExited",
      data: {
        code,
        signal,
      },
    });
  });
};

// ============================================================
// Send message to Rust
// ============================================================

const sendToLuma = (message: unknown) => {
  if (!lumaProcess) {
    throw new Error("Luma process is not running.");
  }

  if (!lumaReady) {
    throw new Error("Luma is not ready yet.");
  }

  const json = JSON.stringify(message) + "\n";

  lumaProcess.stdin.write(json);
};

// ============================================================
// Stop Luma
// ============================================================

const stopLuma = () => {
  if (!lumaProcess) {
    return;
  }

  console.log("[Luma] Stopping desktop server");

  lumaReady = false;

  try {
    lumaProcess.stdin.end();
  } catch {
    // Process may already be closed.
  }

  const processToKill = lumaProcess;

  lumaProcess = null;

  setTimeout(() => {
    if (!processToKill.killed) {
      processToKill.kill();
    }
  }, 1000);
};

// ============================================================
// IPC
// ============================================================

const registerIPC = () => {
  // ----------------------------------------------------------
  // Status
  // ----------------------------------------------------------

  ipcMain.handle("luma:status", () => {
    return {
      connected: lumaProcess !== null && lumaReady,
    };
  });

  // ----------------------------------------------------------
  // Prompt
  // ----------------------------------------------------------

  ipcMain.handle("luma:prompt", (_event, text: unknown) => {
    if (typeof text !== "string") {
      throw new Error("Prompt must be a string.");
    }

    const trimmed = text.trim();

    if (!trimmed) {
      throw new Error("Prompt cannot be empty.");
    }

    sendToLuma({
      type: "Prompt",
      data: {
        text: trimmed,
      },
    });

    return {
      ok: true,
    };
  });

  // ----------------------------------------------------------
  // Confirmation
  // ----------------------------------------------------------

  ipcMain.handle("luma:confirm", (_event, allowed: unknown) => {
    sendToLuma({
      type: "Confirm",
      data: {
        allowed: Boolean(allowed),
      },
    });

    return {
      ok: true,
    };
  });

  // ----------------------------------------------------------
  // Cancel
  // ----------------------------------------------------------

  ipcMain.handle("luma:cancel", () => {
    sendToLuma({
      type: "Cancel",
    });

    return {
      ok: true,
    };
  });
};

// ============================================================
// Application lifecycle
// ============================================================

app.whenReady().then(() => {
  registerIPC();
  createWindow();
  startLuma();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on("before-quit", () => {
  stopLuma();
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});
