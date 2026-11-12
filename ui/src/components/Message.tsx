import { Box, Text } from "ink";

import type { Message as MessageType } from "../state";
import { glyphs, theme } from "../theme";
import { Markdown } from "./Markdown";
import { useSpinner } from "../hooks/useSpinner";

type Props = {
  message: MessageType;
  running?: boolean;
};

export function Message({ message, running = false, detailsExpanded = false }: Props & { detailsExpanded?: boolean }) {
  const spinner = useSpinner(message.role === "tool" && running);
  if (message.role === "user") {
    return (
      <Box flexDirection="column" marginTop={1}>
        <Text color={theme.brand} bold>YOU</Text>
        <Box marginLeft={2} marginTop={0}>
          <Markdown content={message.content} />
        </Box>
      </Box>
    );
  }

  if (message.role === "assistant") {
    return (
      <Box flexDirection="column" marginTop={1}>
        <Text color={theme.assistant ?? "white"} bold>LUMA</Text>
        <Box marginLeft={2} marginTop={0}>
          <Markdown content={message.content} />
        </Box>
      </Box>
    );
  }

  if (message.role === "tool") {
    const [heading, ...details] = message.content.split("\n");
    return (
      <Box marginLeft={2} marginTop={1} flexDirection="column">
        <Box flexDirection="row" alignItems="center">
          <Text color={running ? theme.brand : theme.chrome}>{glyphs.tool} </Text>
          <Box flexGrow={1} marginLeft={1}>
            <Text color={running ? "white" : theme.muted} bold={running}>
              {running ? spinner : ""} {heading}
            </Text>
          </Box>
        </Box>
        {details.length > 0 && (
          <Box marginLeft={3} marginTop={0}>
            {details.map((line, index) => (
              <Text
                key={`${index}-${line}`}
                color={line.startsWith("+") ? theme.success : line.startsWith("-") ? theme.error : theme.chrome}
              >
                {line}
              </Text>
            ))}
          </Box>
        )}
      </Box>
    );
  }

  if (message.role === "preview") {
    const [path, ...diff] = message.content.split("\n");
    return (
      <Box flexDirection="column" marginTop={1} marginLeft={2} paddingLeft={1} borderStyle="round" borderColor={theme.warning}>
        <Text color={theme.warning} bold>{"  "}PROPOSED CHANGE</Text>
        <Text color={theme.muted} italic>{"  "}{path}</Text>
        <Box marginTop={1} flexDirection="column" marginLeft={1}>
          {diff.map((line, index) => (
            <Text
              key={`${index}-${line}`}
              color={line.startsWith("+") ? theme.success : line.startsWith("-") ? theme.error : theme.muted}
            >
              {line}
            </Text>
          ))}
        </Box>
      </Box>
    );
  }

  if (message.role === "error") {
    const [, ...details] = message.content.split("\n");
    const detailText = details.length > 0 ? details.join("\n") : message.content;
    return (
      <Box marginTop={1} flexDirection="column">
        <Text color={theme.error}>{glyphs.error} Failed to run tool · Cmd/Ctrl+O for details</Text>
        {detailsExpanded && detailText ? <Box marginLeft={2}>
          <Text color={theme.muted}>{detailText}</Text>
        </Box> : null}
      </Box>
    );
  }

  return (
    <Box marginTop={1}>
      <Text color={theme.muted}>· </Text>
      <Text>{message.content}</Text>
    </Box>
  );
}
