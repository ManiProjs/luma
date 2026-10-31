import { spawn, type Subprocess } from "bun";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

let serverProcess: Subprocess | null = null;

function getBinary() {
  // An explicit override is useful for packaged installs and test builds.
  if (process.env.LUMA_BINARY) return process.env.LUMA_BINARY;

  // During development, prefer the binary built from this checkout. Falling
  // straight through to PATH can silently launch an older installed core
  // whose JSON protocol does not know about newer UI messages (e.g. set_model).
  const localBuilds = [
    resolve(import.meta.dir, "../../target/debug/luma-core"),
    resolve(import.meta.dir, "../../target/release/luma-core"),
  ];
  return localBuilds.find((path) => existsSync(path)) ?? "luma-core";
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
