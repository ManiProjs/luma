import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // A warm, restrained palette. Meaningful roles get distinct colors instead
  // of painting every part of the interface with the brand accent.
  brand: "#D6A85F",
  user: "#79A6A1",
  tool: "#A18DBB",
  plan: "#D6A85F",

  // Neutral supporting tones; primary content remains the terminal foreground.
  muted: "gray",
  chrome: "gray",

  // Normal content stays terminal-native
  assistant: undefined,

  // State
  success: "#83AD8B",
  error: "#D97777",
  warning: "#D6A85F",

  // Compatibility
  danger: "#D97777",
  accent: "#D6A85F",
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
