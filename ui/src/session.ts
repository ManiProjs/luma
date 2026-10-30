import { useEffect, useState } from "react";

import { startDesktopServer, stopDesktopServer } from "./server";

import { LumaClient } from "./client";

import { initialState, type UiState } from "./state";

import type { ServerEvent } from "./protocol";

const client = new LumaClient();

export function useSession() {
  const [state, setState] = useState<UiState>(initialState);

  function addMessage(
    role: "user" | "assistant" | "error" | "system",
    content: string,
  ) {
    setState((current) => ({
      ...current,

      messages: [
        ...current.messages,

        {
          id: crypto.randomUUID(),
          role,
          content,
          timestamp: Date.now(),
        },
      ],
    }));
  }

  function handleAgentEvent(event: any) {
    setState((current) => {
      const messages = [...current.messages];
      let status = current.status;

      switch (event.event?.type) {
        case "Thinking":
          status = "thinking";
          break;

        case "Status": {
          const state = String(event.event.data?.state ?? "").toLowerCase();
          status = state === "doing" ? "working"
            : state === "talking" ? "streaming"
              : state === "waiting" ? "waiting"
                : state === "error" ? "error"
                  : state === "idle" ? "idle" : "thinking";
          break;
        }

        case "TextDelta": {
          status = "streaming";
          const delta = event.event.data;

          const lastIndex = messages.length - 1;
          const last = messages[lastIndex];

          if (last && last.role === "assistant") {
            messages[lastIndex] = { ...last, content: last.content + delta };
          } else {
            messages.push({
              id: crypto.randomUUID(),
              role: "assistant",
              content: delta,
              timestamp: Date.now(),
            });
          }
          break;
        }

        case "PlanGenerated":
          messages.push({
            id: crypto.randomUUID(),
            role: "plan",
            content: event.event.data,
            timestamp: Date.now(),
          });
          status = "thinking";
          break;

        case "ToolStarted":
          messages.push({
            id: crypto.randomUUID(),
            role: "tool",
            content: `${event.event.data.name}: ${event.event.data.input}`,
            timestamp: Date.now(),
          });
          status = "working";
          break;

        case "ToolFinished": {
          const tool = [...messages].reverse().find((message) => message.role === "tool");
          if (tool) {
            const index = messages.findIndex((message) => message.id === tool.id);
            messages[index] = {
              ...tool,
              content: `${event.event.data.name} — ${event.event.data.duration_ms} ms${event.event.data.failed ? " (failed)" : ""}`,
            };
          }
          status = "thinking";
          break;
        }

        case "ConfirmationRequired":
          status = "waiting";
          break;

        case "SystemMessage":
          messages.push({
            id: crypto.randomUUID(),
            role: "system",
            content: event.event.data,
            timestamp: Date.now(),
          });
          break;

        case "Error":
          messages.push({
            id: crypto.randomUUID(),
            role: "error",
            content: event.event.data,
            timestamp: Date.now(),
          });
          status = "error";
          break;

        case "Finished":
          status = "done";
          break;
      }

      return {
        ...current,
        messages,
        status,
      };
    });
  }

  function handleEvent(event: ServerEvent) {
    switch (event.type) {
      case "ready":
        addMessage("system", `Connected to luma-core v${event.data.version}`);
        break;

      case "session":
        setState((current) => ({
          ...current,
          session: event.data.session.info,
          messages: event.data.session.messages.map((message) => ({
            id: crypto.randomUUID(),
            role: message.role,
            content: message.content,
            timestamp: Date.now(),
          })),
          status: event.data.session.busy ? "thinking" : "idle",
        }));
        break;

      case "agent":
        handleAgentEvent(event.data);
        break;

      case "error":
        addMessage("error", event.data.message);
        setState((current) => ({ ...current, status: "error" }));
        break;
    }
  }

  useEffect(() => {
    try {
      startDesktopServer();
      client.connect();
    } catch (error) {
      addMessage("error", String(error));

      return;
    }

    const unsubscribe = client.subscribe(handleEvent);
    client.send({ type: "get_session" });

    return () => {
      unsubscribe();
      stopDesktopServer();
    };
  }, []);

  return {
    state,

    send(text: string) {
      if (!text.trim()) {
        return;
      }

      addMessage("user", text);
      setState((current) => ({ ...current, status: "thinking" }));

      try {
        client.send({
          type: "prompt",
          data: { text },
        });
      } catch (error) {
        addMessage("error", String(error));
        setState((current) => ({ ...current, status: "error" }));
      }
    },
  };
}
