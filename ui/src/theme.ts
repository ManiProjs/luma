import type { TextProps } from 'ink';

type Color = NonNullable<TextProps['color']>;

export const theme = {
  brand: 'yellowBright',
  text: undefined,
  muted: 'cyan',
  accent: 'greenBright',

  highlight: 'whiteBright',

  success: 'greenBright',
  warning: 'yellowBright',
  danger: 'redBright',

  chrome: 'cyan',

  user: 'greenBright',
  assistant: 'whiteBright',
  tool: 'magenta',
  plan: 'yellowBright',
} satisfies Record<string, Color | undefined>;

export const glyphs = {
  logo: '✦',
  user: '›',
  assistant: '✦',

  tool: '▸',
  plan: '◆',

  success: '✓',
  error: '✗',
  running: '·',

  divider: '─',
  bullet: '•',
  input: '›',
} as const;

export const layout = {
  paddingX: 1,
  maxWidthSlack: 2,
} as const;
