import { useEffect, useState } from "react";
import { useStdout } from "ink";

const FRAMES = ["◐", "◓", "◑", "◒"];

/** A restrained terminal spinner that stays still in pipes, CI, and opt-out mode. */
export function useSpinner(active: boolean): string {
  const { stdout } = useStdout();
  const [frame, setFrame] = useState(0);
  const enabled = active
    && Boolean(stdout.isTTY)
    && process.env.LUMA_NO_ANIMATION !== "1"
    && process.env.CI !== "true";

  useEffect(() => {
    if (!enabled) {
      setFrame(0);
      return;
    }

    const interval = setInterval(() => {
      setFrame((current) => (current + 1) % FRAMES.length);
    }, 180);

    return () => clearInterval(interval);
  }, [enabled]);

  return FRAMES[enabled ? frame : 0] ?? FRAMES[0]!;
}
