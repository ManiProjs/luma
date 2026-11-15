import { useStdout } from "ink";
import { useMotionFrame } from "./useMotionFrame";

export function pulseColorAtFrame(frame: number, base: string, bright: string): string {
  // Smoothly breathe between the two terminal colors over ~1.25 seconds.
  const wave = (1 - Math.cos(((frame % 38) / 38) * Math.PI * 2)) / 2;
  const from = base.match(/[\da-f]{2}/gi)?.map((part) => Number.parseInt(part, 16));
  const to = bright.match(/[\da-f]{2}/gi)?.map((part) => Number.parseInt(part, 16));
  if (!from || !to || from.length < 3 || to.length < 3) return wave > 0.5 ? bright : base;
  const channels = from.slice(0, 3).map((value, index) => Math.round(value + ((to[index] ?? value) - value) * wave));
  return `#${channels.map((value) => value.toString(16).padStart(2, "0")).join("")}`;
}

/** A subtle two-tone pulse for active status text. */
export function usePulseColor(active: boolean, base: string, bright: string): string {
  const { stdout } = useStdout();
  const enabled = active
    && Boolean(stdout.isTTY)
    && process.env.LUMA_NO_ANIMATION !== "1"
    && process.env.CI !== "true";

  const frame = useMotionFrame(enabled);

  if (!enabled) return base;
  return pulseColorAtFrame(frame, base, bright);
}
