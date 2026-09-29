import { Box, Text } from "ink";
import { glyphs, theme } from "../theme";
import type { ToolEvent } from "../state";

type Props = {
  tool: ToolEvent;
};

export function Tool({ tool }: Props) {
  const icon =
    tool.status === "success"
      ? glyphs.success
      : tool.status === "failed"
        ? glyphs.error
        : glyphs.running;

  const color =
    tool.status === "success"
      ? theme.success
      : tool.status === "failed"
        ? theme.danger
        : theme.accent;

  return (
    <Box flexDirection="column">
      <Text color={color}>
        {glyphs.tool} {tool.name} {icon}
      </Text>

      {tool.output && (
        <Text color={theme.muted}>
          {"  "}
          {tool.output}
        </Text>
      )}
    </Box>
  );
}
