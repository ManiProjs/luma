import type { TextProps } from "ink";

type Color = NonNullable<TextProps["color"]>;

export const theme = {
  // Quiet graphite and brass; color is reserved for identity and state.
  brand: "#B89562",
  user: "#C4BFB3",
  tool: "#A29D91",
  plan: "#B89562",

  muted: "#898579",
  chrome: "#5F5D54",
  codeBackground: "#1E201B",

  // Normal content stays terminal-native
  assistant: undefined,

  // State
  success: "#83A88D",
  error: "#CB8279",
  warning: "#C6A36B",

  // Compatibility
  danger: "#CB8279",
  accent: "#B89562",
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
