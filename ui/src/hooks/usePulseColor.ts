import { useEffect, useState } from "react";
import { useStdout } from "ink";

/** A subtle two-tone pulse for active status text. */
export function usePulseColor(active: boolean, base: string, bright: string): string {
  const { stdout } = useStdout();
  const [brightened, setBrightened] = useState(false);
  const enabled = active
    && Boolean(stdout.isTTY)
    && process.env.LUMA_NO_ANIMATION !== "1"
    && process.env.CI !== "true";

  useEffect(() => {
    if (!enabled) {
      setBrightened(false);
      return;
    }

    const interval = setInterval(() => {
      setBrightened((current) => !current);
    }, 620);

    return () => clearInterval(interval);
  }, [enabled]);

  return enabled && brightened ? bright : base;
}
