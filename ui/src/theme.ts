import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // Match desktop Luma: warm neutrals, with color reserved for state.
  brand: "#C49A5A",
  user: "#B8B5AD",
  tool: "#A7A39B",
  plan: "#C49A5A",

  muted: "#89867F",
  chrome: "#625F59",

  // Normal content stays terminal-native
  assistant: undefined,

  // State
  success: "#4ADE80",
  error: "#F87171",
  warning: "#FBBF24",

  // Compatibility
  danger: "#F87171",
  accent: "#C49A5A",
} satisfies Record<string, Color | undefined>;

export const glyphs = {
  logo: "◈",

  user: "›",
  assistant: "●",

  tool: "↳",
  plan: "◆",

  success: "✓",
  error: "✗",
  running: "·",

  arrow: "→",
  bullet: "•",
  divider: "─",

  input: "›",
} as const;
