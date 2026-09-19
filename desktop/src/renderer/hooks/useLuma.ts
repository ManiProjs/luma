import { useCallback, useEffect, useRef, useState } from "react";

import type { LumaAgentEvent } from "../../preload";

export type AgentMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
};

export type AgentTool = {
  id: string;
  name: string;
  input: string;
  durationMs?: number;
  finished: boolean;
};

export type ConfirmationRequest = {
  name: string;
  input: string;
};

export function useLuma() {
  const [connected, setConnected] = useState(false);

  const [messages, setMessages] = useState<AgentMessage[]>([]);

  const [tools, setTools] = useState<AgentTool[]>([]);

  const [thinking, setThinking] = useState(false);

  const [error, setError] = useState<string | null>(null);

  const [confirmation, setConfirmation] = useState<ConfirmationRequest | null>(
    null,
  );

  const activeToolIds = useRef<string[]>([]);

  // ----------------------------------------------------------
  // Agent events
  // ----------------------------------------------------------

  const handleEvent = useCallback((event: LumaAgentEvent) => {
    switch (event.type) {
      case "Ready": {
        setConnected(true);
        setError(null);
        break;
      }

      case "Error": {
        setError(event.data.message);
        setThinking(false);
        break;
      }

      case "ProcessExited": {
        setConnected(false);
        setThinking(false);

        if (event.data.code !== 0 && event.data.code !== null) {
          setError(`Luma exited with code ${event.data.code}.`);
        }

        break;
      }

      case "Agent": {
        handleAgentEvent(event.data.type, event.data.data);

        break;
      }
    }
  }, []);

  const handleAgentEvent = useCallback((type: string, data: unknown) => {
    switch (type) {
      // ----------------------------------------------------
      // Thinking
      // ----------------------------------------------------

      case "Thinking": {
        setThinking(true);
        break;
      }

      // ----------------------------------------------------
      // Streaming text
      // ----------------------------------------------------

      case "TextDelta": {
        if (typeof data !== "string" || !data) {
          return;
        }

        setThinking(false);

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

        setThinking(false);

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

        setTools((current) => [
          ...current,
          {
            id,
            name: tool.name,
            input: tool.input,
            finished: false,
          },
        ]);

        setThinking(false);

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

        setTools((current) => {
          // Prefer the most recent unfinished
          // invocation of this tool.
          const index = [...current]
            .reverse()
            .findIndex((item) => item.name === tool.name && !item.finished);

          if (index === -1) {
            return current;
          }

          const realIndex = current.length - 1 - index;

          const item = current[realIndex];

          activeToolIds.current = activeToolIds.current.filter(
            (id) => id !== item.id,
          );

          return current.map((currentItem, currentIndex) =>
            currentIndex === realIndex
              ? {
                  ...currentItem,
                  finished: true,
                  durationMs: tool.duration_ms,
                }
              : currentItem,
          );
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

        setThinking(false);

        setConfirmation({
          name: data.name,
          input: data.input,
        });

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

    window.luma
      .status()
      .then((status) => {
        if (mounted) {
          setConnected(status.connected);
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
      unsubscribe();
    };
  }, [handleEvent]);

  // ----------------------------------------------------------
  // Send prompt
  // ----------------------------------------------------------

  const sendPrompt = useCallback(async (text: string) => {
    const trimmed = text.trim();

    if (!trimmed) {
      return;
    }

    setError(null);

    setMessages((current) => [
      ...current,
      {
        id: crypto.randomUUID(),
        role: "user",
        content: trimmed,
      },
    ]);

    try {
      await window.luma.sendPrompt(trimmed);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Failed to send prompt.",
      );
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

  return {
    connected,
    messages,
    tools,
    thinking,
    error,
    confirmation,
    sendPrompt,
    respondToConfirmation,
    cancel,
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
} {
  if (typeof data !== "object" || data === null) {
    return false;
  }

  const value = data as Record<string, unknown>;

  return (
    typeof value.name === "string" && typeof value.duration_ms === "number"
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
