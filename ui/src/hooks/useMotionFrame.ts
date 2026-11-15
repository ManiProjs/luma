import { useEffect, useState } from "react";
import { useStdout } from "ink";

/** A lightweight frame clock for terminal-native motion. */
export function useMotionFrame(active: boolean, frameDuration = 1000 / 30): number {
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
    const startedAt = performance.now();
    const interval = setInterval(() => {
      setFrame(Math.floor((performance.now() - startedAt) / frameDuration));
    }, frameDuration);
    return () => clearInterval(interval);
  }, [enabled, frameDuration]);

  return enabled ? frame : 0;
}
