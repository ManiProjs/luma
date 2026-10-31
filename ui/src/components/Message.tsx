import { Box, Text } from "ink";

import type { Message as MessageType } from "../state";
import { glyphs, theme } from "../theme";
import { Markdown } from "./Markdown";

type Props = {
  message: MessageType;
};

export function Message({ message }: Props) {
  if (message.role === "user") {
    return (
      <Box flexDirection="column" marginTop={1}>
        <Text color={theme.user} bold>{glyphs.user} YOU</Text>
        <Box marginLeft={2}>
          <Markdown content={message.content} />
        </Box>
      </Box>
    );
  }

  if (message.role === "assistant") {
    return (
      <Box flexDirection="column" marginTop={1}>
        <Text color={theme.brand} bold>{glyphs.assistant} LUMA</Text>
        <Box marginLeft={2}>
          <Markdown content={message.content} />
        </Box>
      </Box>
    );
  }

  if (message.role === "tool") {
    return (
      <Box marginLeft={2} marginTop={1}>
        <Text color={theme.tool}>{glyphs.tool} </Text>
        <Text>{message.content}</Text>
      </Box>
    );
  }

  if (message.role === "plan") {
    return (
      <Box marginTop={1}>
        <Text color={theme.plan}>{glyphs.plan} </Text>
        <Text>{message.content}</Text>
      </Box>
    );
  }

  if (message.role === "preview") {
    const [path, ...diff] = message.content.split("\n");
    return (
      <Box flexDirection="column" marginTop={1} borderStyle="round" borderColor={theme.warning} paddingX={1}>
        <Text color={theme.warning} bold>Proposed change · {path}</Text>
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
