import { Box, Text, useInput } from "ink";
import { useEffect, useState } from "react";

import { glyphs, theme } from "../theme";
import { PromptHeader } from "./PromptHeader";

import type { Status } from "../state";

type Props = {
  status: Status;
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
};

export function Composer({
  status,
  value,
  onChange,
  onSubmit,
}: Props) {
  const [cursor, setCursor] = useState(value.length);

  // Keep the insertion point in sync when the parent clears the prompt after submit.
  useEffect(() => {
    if (value.length === 0) setCursor(0);
  }, [value]);

  useInput((input, key) => {
    if (key.return) {
      onSubmit();
      return;
    }

    // Most macOS terminals send DEL (\x7f) for the physical Backspace key.
    // Ink exposes that byte as `delete`, while Ctrl+H is `backspace`.
    // Treat both as backward deletion; the forward Delete key is less common
    // and otherwise makes Backspace appear completely broken at end-of-line.
    if (key.backspace || key.delete) {
      if (cursor > 0) {
        onChange(value.slice(0, cursor - 1) + value.slice(cursor));
        setCursor(cursor - 1);
      }
      return;
    }

    if (key.leftArrow) {
      setCursor((position) => Math.max(0, position - 1));
      return;
    }

    if (key.rightArrow) {
      setCursor((position) => Math.min(value.length, position + 1));
      return;
    }

    if (key.home) {
      setCursor(0);
      return;
    }

    if (key.end) {
      setCursor(value.length);
      return;
    }

    if (!key.ctrl && !key.meta && input) {
      onChange(value.slice(0, cursor) + input + value.slice(cursor));
      setCursor(cursor + input.length);
    }
  });

  const beforeCursor = value.slice(0, cursor);
  const afterCursor = value.slice(cursor);

  return (
    <Box flexDirection="column" marginTop={1}>
      <PromptHeader status={status} />

      <Box>
        <Text color={theme.accent} bold>
          {glyphs.input}{" "}
        </Text>

        <Text>
          {beforeCursor}
          <Text color={theme.accent} bold>▌</Text>
          {afterCursor}
        </Text>
      </Box>
    </Box>
  );
}
