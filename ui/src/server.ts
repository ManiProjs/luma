import { spawn, type Subprocess } from "bun";

let serverProcess: Subprocess | null = null;

function getBinary() {
  return process.env.LUMA_BINARY ?? "luma-core";
}

export function startDesktopServer() {
  if (serverProcess) {
    return serverProcess;
  }

  const binary = getBinary();

  try {
    serverProcess = spawn([binary, "desktop-server"], {
      stdin: "pipe",
      stdout: "pipe",
      // Keep startup failures and diagnostic output visible, and avoid
      // blocking the child on an stderr pipe nobody reads.
      stderr: "inherit",
    });

    return serverProcess;
  } catch {
    throw new Error(
      [
        "ERROR ! Can't find `luma-core` executable",
        "If it isn't in PATH, pass it to LUMA_BINARY environment variable",
      ].join("\n"),
    );
  }
}

export function getDesktopServer() {
  if (!serverProcess) {
    throw new Error("Desktop server is not running");
  }

  return serverProcess;
}

export function stopDesktopServer() {
  serverProcess?.kill();
  serverProcess = null;
}
