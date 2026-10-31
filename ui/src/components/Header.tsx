import { Box, Text } from "ink";

import { glyphs, theme } from "../theme";
import type { UiSessionInfo } from "../state";

type Props = {
  session: UiSessionInfo;
};

export function Header({ session }: Props) {
  const workspace = session.workspace.replace(/\/$/, "").split("/").pop() || session.workspace;

  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box>
        <Text color={theme.brand} bold>{glyphs.logo} LUMA</Text>
        <Text color={theme.chrome}>  /  </Text>
        <Text bold>{workspace}</Text>
        <Text color={theme.muted}>  ·  {session.provider}</Text>
      </Box>

      <Box marginLeft={2}>
        <Text color={theme.muted}>{session.model}</Text>
        {session.contextSize ? <Text color={theme.chrome}>  ·  {Math.round(session.contextSize / 1000)}k context</Text> : null}
        {session.branch ? <Text color={theme.chrome}>  ·  {session.branch}</Text> : null}
        {session.branch ? <Text color={session.dirty ? theme.warning : theme.success}> {session.dirty ? "● modified" : "● clean"}</Text> : null}
      </Box>

      <Text color={theme.chrome}>{glyphs.divider.repeat(32)}</Text>
    </Box>
  );
}
