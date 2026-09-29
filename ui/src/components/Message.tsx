import { Text, Box } from "ink";
import { glyphs, theme } from "../theme";
import type { Message as MessageType } from "../state";

type Props = {
  message: MessageType;
};

export function Message({ message }: Props) {
  const config = {
    user: {
      glyph: glyphs.user,
      color: theme.user,
    },
    assistant: {
      glyph: glyphs.assistant,
      color: theme.assistant,
    },
    tool: {
      glyph: glyphs.tool,
      color: theme.tool,
    },
    plan: {
      glyph: glyphs.plan,
      color: theme.plan,
    },
    system: {
      glyph: "·",
      color: theme.muted,
    },
    error: {
      glyph: glyphs.error,
      color: theme.danger,
    },
  }[message.role];

  return (
    <Box>
      <Text color={config.color} bold>
        {config.glyph}{" "}
      </Text>

      <Text color={theme.text}>{message.content}</Text>
    </Box>
  );
}
