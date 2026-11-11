import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // Neutral near-black surfaces; blue is reserved for interactive emphasis.
  brand: "#78AFFF",
  user: "#D0D0D0",
  tool: "#A3A3A3",
  plan: "#78AFFF",

  muted: "#858585",
  chrome: "#626262",
  codeBackground: "#1D1D1D",

  // Normal content stays terminal-native
  assistant: undefined,

  // State
  success: "#83B89A",
  error: "#D78282",
  warning: "#D3B477",

  // Compatibility
  danger: "#D78282",
  accent: "#78AFFF",
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
