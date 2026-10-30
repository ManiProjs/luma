import { Box, Text } from "ink";

import { theme } from "../theme";

/** Luma's original mascot: a tiny pixel-star built from solid block glyphs. */
export function Mascot() {
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color={theme.accent} bold>{"    ▄   ▄"}</Text>
      <Text color={theme.accent} bold>{"  ▄███████▄"}</Text>
      <Text color={theme.accent} bold>{" ███ ▪ ▪ ███"}</Text>
      <Text color={theme.accent} bold>{" ███  ▿  ███"}</Text>
      <Text color={theme.accent} bold>{"  ▀███████▀"}</Text>
      <Text color={theme.accent} bold>{"    ▀   ▀"}</Text>
    </Box>
  );
}
