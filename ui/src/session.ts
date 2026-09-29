import { useState } from "react";
import type { Message } from "./state";

export type SessionState = {
  messages: Message[];
  input: string;
  streaming: boolean;
};

export function useSession() {
  const [state, setState] = useState<SessionState>({
    messages: [],
    input: "",
    streaming: false,
  });

  async function send(input: string) {
    if (!input.trim()) return;

    const user: Message = {
      id: crypto.randomUUID(),
      role: "user",
      content: input,
      timestamp: Date.now(),
    };

    setState((s) => ({
      ...s,
      messages: [...s.messages, user],
      streaming: true,
    }));

    await new Promise((resolve) => setTimeout(resolve, 500));

    const assistant: Message = {
      id: crypto.randomUUID(),
      role: "assistant",
      content: "Hello from Luma.",
      timestamp: Date.now(),
    };

    setState((s) => ({
      ...s,
      messages: [...s.messages, assistant],
      streaming: false,
    }));
  }

  return {
    state,
    send,
  };
}
