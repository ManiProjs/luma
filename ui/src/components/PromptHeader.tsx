import { Box, Text } from "ink";
import { useEffect, useState } from "react";

import { theme } from "../theme";
import type { Status } from "../state";

type Props = {
  status: Status;
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

export function PromptHeader({ status }: Props) {
  const [frame, setFrame] = useState(0);

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

  const active = status === "thinking" || status === "working" || status === "streaming";

  return (
    <Box marginLeft={2} marginBottom={0}>
      <Text color={status === "error" ? theme.error : status === "done" ? theme.success : theme.accent} bold>
        {active ? frames[frame] : status === "waiting" ? "◷" : status === "done" ? "✓" : status === "error" ? "!" : "●"}{" "}
      </Text>
      <Text color={active ? theme.accent : theme.muted}>{statusWords[status]}</Text>
    </Box>
  );
}
