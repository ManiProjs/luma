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

export type ThemeName = "graphite" | "ocean" | "moss";

type ThemePalette = Record<keyof typeof theme, Color | undefined>;

const palettes: Record<ThemeName, ThemePalette> = {
  graphite: { ...theme },
  ocean: {
    background: "#101820", surface: "#17232d", surfaceRaised: "#20313d", border: "#304653",
    text: "#e3edf2", secondary: "#b7cbd5", brand: "#83b7cf", user: "#d2e1e8", tool: "#91aab7", plan: "#83b7cf",
    muted: "#8ba3af", chrome: "#607985", codeBackground: "#192832", assistant: "#e3edf2",
    success: "#8ebfae", error: "#d98e91", warning: "#d6b77c", danger: "#d98e91", accent: "#83b7cf",
  },
  moss: {
    background: "#151914", surface: "#1d241d", surfaceRaised: "#283128", border: "#3b493a",
    text: "#e6e9df", secondary: "#c1c9b7", brand: "#a9bd91", user: "#d8decf", tool: "#a5ae9b", plan: "#a9bd91",
    muted: "#98a18d", chrome: "#6e7967", codeBackground: "#20281f", assistant: "#e6e9df",
    success: "#9cbd8d", error: "#d58f86", warning: "#d0b77e", danger: "#d58f86", accent: "#a9bd91",
  },
};

let activeTheme: ThemeName = "graphite";

export function getThemeName(): ThemeName {
  return activeTheme;
}

export function cycleTheme(): ThemeName {
  const names = Object.keys(palettes) as ThemeName[];
  activeTheme = names[(names.indexOf(activeTheme) + 1) % names.length] ?? "graphite";
  Object.assign(theme, palettes[activeTheme]);
  return activeTheme;
}

export function themeLabel(name: ThemeName): string {
  return name[0]!.toUpperCase() + name.slice(1);
}

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
