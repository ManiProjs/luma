import { Box, Text } from "ink";
import { useEffect, useState } from "react";

import { theme } from "../theme";
import type { Status } from "../state";

type Props = {
  status: Status;
  activeTool: string | null;
  toolCalls: number;
  turns: number;
};

const statusWords: Record<Status, string> = {
  idle: "ready",
  thinking: "thinking through it",
  working: "working on your code",
  streaming: "writing response",
  waiting: "waiting for your approval",
  done: "done",
  error: "stuck",
};

const frames = ["◐", "◓", "◑", "◒"];

export function PromptHeader({ status, activeTool, toolCalls, turns }: Props) {
  const [frame, setFrame] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [startedAt, setStartedAt] = useState<number | null>(null);

  useEffect(() => {
    if (status === "idle" || status === "done" || status === "error" || status === "waiting") {
      setFrame(0);
      return;
    }

    const interval = setInterval(() => {
      setFrame((current) => (current + 1) % frames.length);
    }, 120);

    return () => clearInterval(interval);
  }, [status]);

  useEffect(() => {
    const active = status === "thinking" || status === "working" || status === "streaming" || status === "waiting";
    if (active && startedAt === null) setStartedAt(Date.now());
    if (status === "idle" || status === "done" || status === "error") {
      setStartedAt(null);
      setElapsed(0);
      return;
    }
    if (!active || startedAt === null) return;

    const update = () => setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [status, startedAt]);

  const active = status === "thinking" || status === "working" || status === "streaming";

  return (
    <Box marginLeft={2} marginBottom={0} flexDirection="row" flexWrap="wrap" columnGap={1}>
      <Text color={status === "error" ? theme.error : status === "done" ? theme.success : theme.accent} bold>
        {active ? frames[frame] : status === "waiting" ? "◷" : status === "done" ? "✓" : status === "error" ? "!" : "●"}{" "}
      </Text>
      <Text color={active ? theme.accent : theme.muted}>{statusWords[status]}</Text>
      {activeTool ? <Text color={theme.tool}>· {activeTool}</Text> : null}
      {active || status === "waiting" ? <Text color={theme.muted}>· {elapsed}s</Text> : null}
      <Text color={theme.chrome}>· {turns} {turns === 1 ? "turn" : "turns"}</Text>
      <Text color={theme.chrome}>· {toolCalls} {toolCalls === 1 ? "tool call" : "tool calls"}</Text>
    </Box>
  );
}
