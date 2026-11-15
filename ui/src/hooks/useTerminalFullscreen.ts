import { useEffect } from "react";
import { useStdout } from "ink";

/** Owns the terminal's alternate screen buffer for a full-screen Ink view. */
export function useTerminalFullscreen(enabled: boolean) {
  const { stdout } = useStdout();

  useEffect(() => {
    if (!enabled || !stdout.isTTY) return;
    stdout.write("\u001B[?1049h\u001B[?25l\u001B[2J\u001B[H");
    return () => {
      stdout.write("\u001B[?25h\u001B[?1049l");
    };
  }, [enabled, stdout]);
}
