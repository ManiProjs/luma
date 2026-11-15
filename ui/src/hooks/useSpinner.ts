import { useStdout } from "ink";
import { useMotionFrame } from "./useMotionFrame";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

export function spinnerAtFrame(frame: number): string {
  return FRAMES[Math.floor(frame / 3) % FRAMES.length] ?? FRAMES[0]!;
}

/** A restrained terminal spinner that stays still in pipes, CI, and opt-out mode. */
export function useSpinner(active: boolean): string {
  const { stdout } = useStdout();
  const enabled = active
    && Boolean(stdout.isTTY)
    && process.env.LUMA_NO_ANIMATION !== "1"
    && process.env.CI !== "true";

  const frame = useMotionFrame(enabled);

  return enabled ? spinnerAtFrame(frame) : FRAMES[0]!;
}
