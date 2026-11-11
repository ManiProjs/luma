import { Box, Text, useStdout } from "ink";

import { glyphs, theme } from "../theme";
import type { UiSessionInfo } from "../state";
import { usePulseColor } from "../hooks/usePulseColor";

type Props = {
  session: UiSessionInfo;
};

export function Header({ session }: Props) {
  const { stdout } = useStdout();
  const brandColor = usePulseColor(true, theme.brand, "#B4D3FF");
  const workspace = session.workspace.replace(/\/$/, "").split("/").pop() || session.workspace;
  const ruleWidth = Math.max(1, Math.min((stdout.columns || 72) - 2, 96));

  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box flexDirection="row" flexWrap="wrap" columnGap={1}>
        <Text color={brandColor} bold>{glyphs.logo} LUMA</Text>
        <Text color={theme.chrome}>/</Text>
        <Text color="white" bold>{workspace}</Text>
        <Text color={theme.chrome}>·</Text>
        <Text color={theme.muted}>{session.model}</Text>
        {session.branch ? <Text color={theme.chrome}>· {session.branch}</Text> : null}
        {session.branch ? <Text color={session.dirty ? theme.warning : theme.success}>{session.dirty ? "modified" : "clean"}</Text> : null}
      </Box>
      <Text color={theme.chrome}>{session.provider}{session.contextSize ? `  ·  ${Math.round(session.contextSize / 1000)}k context` : ""}</Text>
      <Text color={theme.chrome}>{"─".repeat(ruleWidth)}</Text>
    </Box>
  );
}
