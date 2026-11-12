import { useCallback, useEffect, useRef, useState } from "react";

import type { LumaAgentEvent } from "../../preload";

import { announce } from "../../utils/accessibility";

export type AgentMessage = {
  id: string;
  role: "user" | "assistant" | "error" | "tools";
  content: string;
  tools?: AgentTool[];
};

export type AgentTool = {
  id: string;
  name: string;
  input: string;
  durationMs?: number;
  finished: boolean;
  failed?: boolean;
  diff?: string;
};

export type ConfirmationRequest = {
  name: string;
  input: string;
};

export function useLuma() {
  const [connected, setConnected] = useState(false);
  const [sessionId, setSessionId] = useState<string | null>(null);

  const [messages, setMessages] = useState<AgentMessage[]>([]);

  const [thinking, setThinking] = useState(false);

  const [error, setError] = useState<string | null>(null);

  const [confirmation, setConfirmation] = useState<ConfirmationRequest | null>(
    null,
  );

  const activeToolIds = useRef<string[]>([]);
  const pendingToolDiff = useRef<string | undefined>(undefined);
  const toolGroupOpen = useRef(false);
  const mountedRef = useRef(false);

  const requestSession = useCallback(async () => {
    try {
      await window.luma.getSession();
    } catch (error) {
      if (mountedRef.current) {
        setError(error instanceof Error ? error.message : "Failed to load the current session.");
      }
    }
  }, []);

  // ----------------------------------------------------------
  // Agent events
  // ----------------------------------------------------------

  const handleEvent = useCallback((event: LumaAgentEvent) => {
    switch (event.type) {
      case "ready": {
        setConnected(true);
        setError(null);
        void requestSession();
        break;
      }

      case "error": {
        setError(event.data.message);
        setThinking(false);
        if (event.data.category === "initialization_failed") setConnected(false);
        announce(`Error: ${event.data.message}`, 'assertive');
        break;
      }

      case "agent": {
        const agentEvent = event.data.event;
        handleAgentEvent(
          agentEvent.type,
          "data" in agentEvent ? agentEvent.data : undefined,
        );

        break;
      }

      case "session": {
        const snapshot = event.data.session;
        setSessionId(snapshot.chat_id);
        setMessages(snapshot.messages.reduce<AgentMessage[]>((items, message, index) => {
          if (message.role === "tool") {
            const tool: AgentTool = {
              id: `session-tool-${index}`,
              name: message.tool ?? "Tool",
              input: message.content,
              finished: !message.running,
              durationMs: message.elapsed_ms ?? undefined,
            };
            const last = items[items.length - 1];
            if (last?.role === "tools") last.tools = [...(last.tools ?? []), tool];
            else items.push({ id: `session-tools-${index}`, role: "tools", content: "", tools: [tool] });
            return items;
          }
          items.push({
            id: `session-${index}-${message.role}`,
            role: message.role === "user" ? "user" : message.role === "error" ? "error" : "assistant",
            content: message.content,
          });
          return items;
        }, []));
        setThinking(snapshot.busy && !snapshot.question);
        setConfirmation(snapshot.confirmation);
        setError(null);
        break;
      }
    }
  }, [requestSession]);

  const handleAgentEvent = useCallback((type: string, data: unknown) => {
    switch (type) {
      // ----------------------------------------------------
      // Thinking
      // ----------------------------------------------------

      case "Thinking": {
        toolGroupOpen.current = false;
        setThinking(true);
        announce("Luma is thinking...");
        break;
      }

      // ----------------------------------------------------
      // Streaming text
      // ----------------------------------------------------

      case "TextDelta": {
        if (typeof data !== "string" || !data) {
          return;
        }
        toolGroupOpen.current = false;

        setMessages((current) => {
          const last = current[current.length - 1];

          if (last?.role === "assistant") {
            return [
              ...current.slice(0, -1),
              {
                ...last,
                content: last.content + data,
              },
            ];
          }

          return [
            ...current,
            {
              id: crypto.randomUUID(),
              role: "assistant",
              content: data,
            },
          ];
        });

        break;
      }

      // ----------------------------------------------------
      // Plan
      // ----------------------------------------------------

      case "PlanGenerated": {
        if (typeof data !== "string" || !data) {
          return;
        }
        toolGroupOpen.current = false;

        setMessages((current) => [
          ...current,
          {
            id: crypto.randomUUID(),
            role: "assistant",
            content: data,
          },
        ]);

        break;
      }

      case "ChangePreview": {
        if (typeof data === "object" && data !== null && "diff" in data && typeof data.diff === "string") {
          pendingToolDiff.current = data.diff;
        }
        break;
      }

      // ----------------------------------------------------
      // Tool started
      // ----------------------------------------------------

      case "ToolStarted": {
        const tool = isToolStartedData(data)
          ? data
          : {
              name: "Unknown tool",
              input: "",
            };

        const id = crypto.randomUUID();

        activeToolIds.current.push(id);

        const nextTool: AgentTool = {
          id,
          name: tool.name,
          input: tool.input,
          finished: false,
          diff: tool.name === "patch_file" ? pendingToolDiff.current : undefined,
        };
        setMessages((current) => {
          const last = current[current.length - 1];
          if (toolGroupOpen.current && last?.role === "tools") {
            return [...current.slice(0, -1), { ...last, tools: [...(last.tools ?? []), nextTool] }];
          }
          return [...current, { id: crypto.randomUUID(), role: "tools", content: "", tools: [nextTool] }];
        });
        toolGroupOpen.current = true;
        pendingToolDiff.current = undefined;

        announce(`Running tool: ${tool.name}`);

        break;
      }

      // ----------------------------------------------------
      // Tool finished
      // ----------------------------------------------------

      case "ToolFinished": {
        const tool = isToolFinishedData(data) ? data : null;

        if (!tool) {
          return;
        }

        announce(`Tool ${tool.name} completed in ${tool.duration_ms}ms`);

        setMessages((current) => {
          for (let messageIndex = current.length - 1; messageIndex >= 0; messageIndex -= 1) {
            const message = current[messageIndex];
            if (message.role !== "tools" || !message.tools) continue;
            const toolIndex = [...message.tools].reverse().findIndex((item) => item.name === tool.name && !item.finished);
            if (toolIndex < 0) continue;
            const realIndex = message.tools.length - 1 - toolIndex;
            const previous = message.tools[realIndex];
            const nextTools = message.tools.map((item, index) => index === realIndex ? {
              ...item,
              finished: true,
              failed: tool.failed,
              durationMs: tool.duration_ms,
              diff: typeof tool.diff === "string" ? tool.diff : item.diff,
            } : item);
            activeToolIds.current = activeToolIds.current.filter((id) => id !== previous.id);
            return current.map((item, index) => index === messageIndex ? { ...message, tools: nextTools } : item);
          }
          return current;
        });

        break;
      }

      // ----------------------------------------------------
      // Confirmation
      // ----------------------------------------------------

      case "ConfirmationRequired": {
        if (!isConfirmationData(data)) {
          return;
        }

        setConfirmation({
          name: data.name,
          input: data.input,
        });

        break;
      }

      case "QuestionRequired": {
        setThinking(false);
        break;
      }

      // ----------------------------------------------------
      // System messages
      // ----------------------------------------------------

      case "SystemMessage": {
        if (typeof data !== "string" || !data) {
          return;
        }

        setMessages((current) => [
          ...current,
          {
            id: crypto.randomUUID(),
            role: "assistant",
            content: data,
          },
        ]);

        break;
      }

      // ----------------------------------------------------
      // Finished
      // ----------------------------------------------------

      case "Finished": {
        setThinking(false);
        setConfirmation(null);
        pendingToolDiff.current = undefined;
        break;
      }

      case "Cancelled": {
        setThinking(false);
        setConfirmation(null);
        activeToolIds.current = [];
        setMessages((current) => current.map((message) => message.role === "tools"
          ? { ...message, tools: message.tools?.map((tool) => ({ ...tool, finished: true })) }
          : message));
        setMessages((current) => [...current, {
          id: crypto.randomUUID(),
          role: "assistant",
          content: "Generation cancelled.",
        }]);
        break;
      }

      case "Status": {
        if (typeof data === "object" && data !== null && "state" in data) {
          const state = String((data as { state: unknown }).state);
          setThinking(["thinking", "doing", "talking", "waiting"].includes(state));
        }
        break;
      }

      case "Error": {
        const message = typeof data === "string" ? data : "The agent encountered an error.";
        setError(null);
        setMessages((current) => [...current, {
          id: crypto.randomUUID(),
          role: "error",
          content: message,
        }]);
        setThinking(false);
        setConfirmation(null);
        announce("Failed to run tool. Press Command+O to inspect the error details.", "assertive");
        break;
      }

      default:
        break;
    }
  }, []);

  // ----------------------------------------------------------
  // Subscribe to Electron
  // ----------------------------------------------------------

  useEffect(() => {
    let mounted = true;
    mountedRef.current = true;

    window.luma
      .getStatus()
      .then((status) => {
        if (mounted) {
          setConnected(status.connected);
          if (status.connected) void requestSession();
        }
      })
      .catch(() => {
        if (mounted) {
          setConnected(false);
        }
      });

    const unsubscribe = window.luma.onEvent((event) => {
      if (mounted) {
        handleEvent(event);
      }
    });

    return () => {
      mounted = false;
      mountedRef.current = false;
      unsubscribe();
    };
  }, [handleEvent, requestSession]);

  // ----------------------------------------------------------
  // Send prompt
  // ----------------------------------------------------------

  const resetSession = useCallback((nextMessages: AgentMessage[] = []) => {
    activeToolIds.current = [];
    setMessages(nextMessages);
    setThinking(false);
    setError(null);
    setConfirmation(null);
  }, []);

  const sendPrompt = useCallback(async (text: string) => {
    const trimmed = text.trim();

    if (!trimmed) {
      return;
    }

    setError(null);
    activeToolIds.current = [];
    pendingToolDiff.current = undefined;

    setMessages((current) => [
      ...current,
      {
        id: crypto.randomUUID(),
        role: "user",
        content: trimmed,
      },
    ]);

    try {
      await window.luma.prompt(trimmed);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Failed to send prompt.",
      );
    }
  }, []);

  // Local slash commands are handled by the core session and return their
  // result as a SystemMessage. Do not optimistically add the command itself
  // as a user chat message: commands such as /status must not create archives.
  const runCommand = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setError(null);
    try {
      await window.luma.prompt(trimmed);
      await window.luma.getSession();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Failed to run command.");
    }
  }, []);

  // ----------------------------------------------------------
  // Confirmation
  // ----------------------------------------------------------

  const respondToConfirmation = useCallback(async (allowed: boolean) => {
    try {
      setConfirmation(null);

      await window.luma.confirm(allowed);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Failed to send confirmation.",
      );
    }
  }, []);

  // ----------------------------------------------------------
  // Cancel
  // ----------------------------------------------------------

  const cancel = useCallback(async () => {
    try {
      await window.luma.cancel();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Failed to cancel.");
    }
  }, []);

  const tools = messages.flatMap((message) => message.role === "tools" ? message.tools ?? [] : []);

  return {
    connected,
    sessionId,
    messages,
    tools,
    thinking,
    error,
    confirmation,
    sendPrompt,
    runCommand,
    respondToConfirmation,
    cancel,
    resetSession,
  };
}

// ============================================================
// Type guards
// ============================================================

function isToolStartedData(data: unknown): data is {
  name: string;
  input: string;
} {
  if (typeof data !== "object" || data === null) {
    return false;
  }

  const value = data as Record<string, unknown>;

  return typeof value.name === "string" && typeof value.input === "string";
}

function isToolFinishedData(data: unknown): data is {
  name: string;
  duration_ms: number;
  failed?: boolean;
  diff?: string | null;
} {
  if (typeof data !== "object" || data === null) {
    return false;
  }

  const value = data as Record<string, unknown>;

  return (
    typeof value.name === "string" && typeof value.duration_ms === "number"
    && (value.failed === undefined || typeof value.failed === "boolean")
  );
}

function isConfirmationData(data: unknown): data is {
  name: string;
  input: string;
} {
  if (typeof data !== "object" || data === null) {
    return false;
  }

  const value = data as Record<string, unknown>;

  return typeof value.name === "string" && typeof value.input === "string";
}
