import { Box, Text } from "ink";
import { useEffect, useState } from "react";

import { theme } from "../theme";
import type { Status } from "../state";
import { useSpinner } from "../hooks/useSpinner";
import { usePulseColor } from "../hooks/usePulseColor";

type Props = {
  status: Status;
  activeTool: string | null;
  toolCalls: number;
  turns: number;
};

const statusWords: Record<Status, string> = {
  idle: "Ready",
  thinking: "Thinking",
  working: "Working",
  streaming: "Responding",
  waiting: "Approval needed",
  done: "Completed",
  error: "Needs attention",
};

export function PromptHeader({ status, activeTool, toolCalls, turns }: Props) {
  const [elapsed, setElapsed] = useState(0);
  const [startedAt, setStartedAt] = useState<number | null>(null);

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
  const spinner = useSpinner(active && !activeTool);
  const statusColor = usePulseColor(
    active || status === "waiting",
    status === "waiting" ? theme.warning : theme.accent,
    "#F0D18D",
  );

  return (
    <Box marginBottom={0} flexDirection="row" flexWrap="wrap" columnGap={1}>
      <Text color={status === "error" ? theme.error : status === "done" ? theme.success : status === "waiting" ? theme.warning : theme.muted} bold>
        {active && !activeTool ? spinner : status === "waiting" ? "◷" : status === "done" ? "✓" : status === "error" ? "!" : "●"}{" "}
      </Text>
      <Text color={active || status === "waiting" ? statusColor : theme.muted} bold={active || status === "waiting"}>{statusWords[status]}</Text>
      {activeTool ? <Text color={theme.tool}>· {activeTool}</Text> : null}
      {active || status === "waiting" ? <Text color={theme.muted}>· {elapsed}s</Text> : null}
      <Text color={theme.chrome}>{turns} {turns === 1 ? "turn" : "turns"}</Text>
      <Text color={theme.chrome}>{toolCalls} {toolCalls === 1 ? "tool" : "tools"}</Text>
    </Box>
  );
}
