import { Box, Text } from "ink";

import type { Message as MessageType } from "../state";
import { glyphs, theme } from "../theme";
import { Markdown } from "./Markdown";
import { useSpinner } from "../hooks/useSpinner";

type Props = {
  message: MessageType;
  running?: boolean;
};

export function Message({ message, running = false }: Props) {
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
        <Box flexDirection="row">
          <Text color={running ? theme.brand : theme.chrome}>{glyphs.tool} </Text>
          <Box flexGrow={1}>
            <Text color={theme.muted}>{running ? spinner : ""} {heading}</Text>
          </Box>
        </Box>
        {details.map((line, index) => <Text key={`${index}-${line}`} color={line.startsWith("+") ? theme.success : line.startsWith("-") ? theme.error : theme.muted}>{line}</Text>)}
      </Box>
    );
  }

  if (message.role === "plan") {
    return (
      <Box marginTop={1}>
        <Text color={theme.brand}>{glyphs.plan} </Text>
        <Text bold>{message.content}</Text>
      </Box>
    );
  }

  if (message.role === "preview") {
    const [path, ...diff] = message.content.split("\n");
    return (
      <Box flexDirection="column" marginTop={1} marginLeft={1} borderStyle="single" borderLeft borderColor={theme.chrome} paddingLeft={1}>
        <Text color={theme.warning} bold>PROPOSED CHANGE</Text>
        <Text color={theme.muted}>{path}</Text>
        {diff.map((line, index) => <Text key={`${index}-${line}`} color={line.startsWith("+") ? theme.success : line.startsWith("-") ? theme.error : theme.muted}>{line}</Text>)}
      </Box>
    );
  }

  if (message.role === "error") {
    return (
      <Box marginTop={1}>
        <Text color={theme.error}>{glyphs.error} </Text>
        <Text>{message.content}</Text>
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
