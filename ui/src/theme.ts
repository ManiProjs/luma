import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // Quiet terminal palette: neutral chrome, readable content, one cool accent.
  background: "#111214",
  surface: "#191a1d",
  surfaceRaised: "#222429",
  border: "#303238",
  text: "#e7e8ea",
  secondary: "#b9bbc1",
  brand: "#9aabc4",
  user: "#d4d5d8",
  tool: "#9da1aa",
  plan: "#9aabc4",

  muted: "#92959d",
  chrome: "#696d76",
  codeBackground: "#1c1e22",

  assistant: "#e7e8ea",

  // State
  success: "#8ebc9a",
  error: "#d88989",
  warning: "#d6b77c",

  // Compatibility
  danger: "#d88989",
  accent: "#9aabc4",
} satisfies Record<string, Color | undefined>;

export const glyphs = {
  logo: "◆",

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
