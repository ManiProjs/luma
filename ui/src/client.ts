import type { Subprocess } from "bun";

import { getDesktopServer } from "./server";

import {
  assertProtocolVersion,
  encodeClientMessage,
  parseServerEvent,
  type ClientMessage,
  type ServerEvent,
} from "./protocol";

type Listener = (event: ServerEvent) => void;

export class LumaClient {
  private process: Subprocess | null = null;

  private listeners = new Set<Listener>();

  private ready = false;

  private readyPromise: Promise<void> | null = null;

  private resolveReady: (() => void) | null = null;

  private rejectReady: ((error: Error) => void) | null = null;

  connect(): Promise<void> {
    if (this.ready && this.process) return Promise.resolve();
    if (this.readyPromise) return this.readyPromise;

    const child = getDesktopServer();
    this.process = child;
    this.ready = false;
    this.readyPromise = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    void this.readyPromise.catch(() => undefined);

    void this.readEvents(child).catch((error) => this.fail(error));
    void child.exited.then((code) => {
      if (this.process === child) {
        this.fail(new Error(`Luma core exited${code === 0 ? "" : ` with code ${code}`}.`));
      }
    });
    return this.readyPromise;
  }

  subscribe(listener: Listener) {
    this.listeners.add(listener);

    return () => {
      this.listeners.delete(listener);
    };
  }

  send(message: ClientMessage) {
    const stdin = this.process?.stdin;

    if (!this.ready || !stdin || typeof stdin === "number") {
      throw new Error("Luma core is not ready yet.");
    }

    stdin.write(encodeClientMessage(message));

    stdin.flush();
  }

  disconnect() {
    const child = this.process;
    this.process = null;
    this.ready = false;
    this.rejectReady?.(new Error("Luma client disconnected."));
    this.resolveReady = null;
    this.rejectReady = null;
    this.readyPromise = null;
    child?.kill();
  }

  private fail(reason: unknown) {
    const error = reason instanceof Error ? reason : new Error(String(reason));
    const child = this.process;
    this.process = null;
    this.ready = false;
    this.rejectReady?.(error);
    this.resolveReady = null;
    this.rejectReady = null;
    this.readyPromise = null;
    child?.kill();
    const failure: ServerEvent = {
      type: "error",
      data: { request_id: null, message: error.message, category: "transient" },
    };
    this.emit(failure);
  }

  private emit(event: ServerEvent) {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (error) {
        // A broken UI subscriber must not interrupt delivery to other views
        // or turn a recoverable core event into an unhandled rejection.
        process.stderr.write(`Luma UI event listener failed: ${error instanceof Error ? error.message : String(error)}\n`);
      }
    }
  }

  private async readEvents(child: Subprocess) {
    const stdout = child.stdout;

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

      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split("\n");

      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (!line.trim()) {
          continue;
        }

        const event = parseServerEvent(line);
        if (event.type === "ready") assertProtocolVersion(event.data.version);

        if (event.type === "ready") {
          this.ready = true;
          this.resolveReady?.();
          this.resolveReady = null;
          this.rejectReady = null;
        }

        this.emit(event);
      }
    }

    buffer += decoder.decode();
    const trailing = buffer.trim();
    if (trailing) {
      const event = parseServerEvent(trailing);
      if (event.type === "ready") {
        assertProtocolVersion(event.data.version);
        this.ready = true;
        this.resolveReady?.();
        this.resolveReady = null;
        this.rejectReady = null;
      }
      this.emit(event);
    }
    if (this.process === child) throw new Error("The Luma core connection closed.");
  }
}
