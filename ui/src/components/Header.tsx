import { Box, Text, useStdout } from "ink";

import { theme } from "../theme";
import type { UiSessionInfo } from "../state";

type Props = {
  session: UiSessionInfo;
  fullScreen?: boolean;
};

export function Header({ session, fullScreen = false }: Props) {
  const { stdout } = useStdout();
  const workspace = session.workspace.replace(/\/$/, "").split("/").pop() || session.workspace;
  const width = stdout.columns || 80;

  return <Box flexDirection="column" marginBottom={1}>
    <Box flexDirection="row" alignItems="center" justifyContent="space-between">
      <Box flexDirection="row" alignItems="center" columnGap={1}>
        <Text color={theme.text} bold>LUMA</Text>
        <Text color={theme.chrome}>/</Text>
        <Text color={theme.secondary}>{workspace}</Text>
      </Box>
      <Box flexDirection="row" alignItems="center" columnGap={1}>
        {session.branch ? <Text color={theme.muted}>{session.branch}{session.dirty ? " *" : ""}</Text> : null}
        <Text color={theme.chrome}>·</Text>
        <Text color={theme.muted}>{session.model}</Text>
        {fullScreen ? <Text color={theme.brand}>  FULLSCREEN</Text> : null}
      </Box>
    </Box>
    <Text color={theme.border}>{"─".repeat(Math.max(0, width - (fullScreen ? 4 : 2)))}</Text>
  </Box>;
}
