import { Box, Text, useStdout } from "ink";
import { useEffect, useState } from "react";

const faces = ["●ᴗ●", "◉‿◉", "•⩊•", "●ᴥ●", "◕‿◕", "◉ᴗ◉"];
const colors = ["#9298ed", "#67c9ae", "#ed9b7d", "#e5bd70", "#b49be8", "#6ebbd5"];

function mascotIndex(seed: string) {
  let hash = 0;
  for (const character of seed) hash = Math.imul(hash ^ character.charCodeAt(0), 16777619);
  return (hash >>> 0) % faces.length;
}

/** Luma's small, expressive sprite family, stable for each agent identity. */
export function Mascot({ seed = "luma", compact = false }: { seed?: string; compact?: boolean }) {
  const index = mascotIndex(seed);
  const { stdout } = useStdout();
  const [blink, setBlink] = useState(false);

  useEffect(() => {
    if (compact || !stdout.isTTY || process.env.LUMA_NO_ANIMATION === "1" || process.env.CI === "true") return;
    const closeEyes = setTimeout(() => setBlink(true), 650);
    const openEyes = setTimeout(() => setBlink(false), 790);
    return () => {
      clearTimeout(closeEyes);
      clearTimeout(openEyes);
    };
  }, [compact, stdout]);

  const face = blink ? "—ᴗ—" : faces[index];
  if (compact) return <Text color={colors[index]} bold>{face}</Text>;

  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color={colors[index]} bold>{"     ╭─╮"}</Text>
      <Text color={colors[index]} bold>{"   ╭─┴─┴─╮"}</Text>
      <Text color={colors[index]} bold>{`  ╭┤ ${face} ├╮`}</Text>
      <Text color={colors[index]} bold>{"  ╰┤  ᴗ  ├╯"}</Text>
      <Text color={colors[index]} bold>{"    ╰┬─┬╯"}</Text>
      <Text color={colors[index]} bold>{"     ╵ ╵"}</Text>
    </Box>
  );
}
