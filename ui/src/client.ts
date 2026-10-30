import type { Subprocess } from "bun";

import { getDesktopServer } from "./server";

import type { ClientMessage, ServerEvent } from "./protocol";

type Listener = (event: ServerEvent) => void;

export class LumaClient {
  private process: Subprocess | null = null;

  private listeners = new Set<Listener>();

  connect() {
    this.process = getDesktopServer();

    this.readEvents();
  }

  subscribe(listener: Listener) {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  }

  send(message: ClientMessage) {
    const stdin = this.process?.stdin;

    if (!stdin || typeof stdin === "number") {
      throw new Error("luma-core stdin unavailable");
    }

    stdin.write(JSON.stringify(message) + "\n");

    stdin.flush();
  }

  private async readEvents() {
    const stdout = this.process?.stdout;

    if (!stdout || typeof stdout === "number") {
      return;
    }

    const reader = stdout.getReader();

    const decoder = new TextDecoder();

    let buffer = "";

    while (true) {
      const { value, done } = await reader.read();

      if (done) {
        break;
      }

      buffer += decoder.decode(value);

      const lines = buffer.split("\n");

      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (!line.trim()) {
          continue;
        }

        const event = JSON.parse(line) as ServerEvent;

        for (const listener of this.listeners) {
          listener(event);
        }
      }
    }
  }
}
