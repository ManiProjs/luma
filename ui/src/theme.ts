import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // One accent throughout the interface: a clear, slightly softened blue.
  brand: "#4A90E2",
  user: "#4A90E2",
  tool: "#4A90E2",
  plan: "#4A90E2",

  // Neutral supporting tones; primary content remains the terminal foreground.
  muted: "gray",
  chrome: "gray",

  // Normal content stays terminal-native
  assistant: undefined,

  // State
  success: "#4A90E2",
  error: "#4A90E2",
  warning: "#4A90E2",

  // Compatibility
  danger: "#4A90E2",
  accent: "#4A90E2",
} satisfies Record<string, Color | undefined>;

export const glyphs = {
  logo: "◈",

  user: "›",
  assistant: "✦",

  tool: "▸",
  plan: "◆",

  success: "✓",
  error: "✗",
  running: "·",

  arrow: "→",
  bullet: "•",
  divider: "─",

  input: "›",
} as const;
