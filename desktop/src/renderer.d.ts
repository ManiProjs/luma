import type { LumaAgentEvent } from "./preload";

declare global {
  interface Window {
    luma: {
      sendPrompt(text: string): Promise<{ ok: true }>;

      confirm(allowed: boolean): Promise<{ ok: true }>;

      cancel(): Promise<{ ok: true }>;

      status(): Promise<{
        connected: boolean;
      }>;

      onEvent(callback: (event: LumaAgentEvent) => void): () => void;
    };
  }
}

export {};
