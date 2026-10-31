import { marked, type Token, type Tokens } from "marked";
import { Box, Text } from "ink";
import type { ReactNode } from "react";

import { theme } from "../theme";

type InlineToken = Token;

function children(tokens: Token[] | undefined, key: string): ReactNode[] {
  return inline(tokens ?? [], key);
}

function inline(tokens: InlineToken[], keyPrefix: string): ReactNode[] {
  return tokens.map((token, index) => {
    const key = `${keyPrefix}-${index}`;
    switch (token.type) {
      case "strong":
        return <Text key={key} bold>{children(token.tokens, key)}</Text>;
      case "em":
        return <Text key={key} italic>{children(token.tokens, key)}</Text>;
      case "del":
        return <Text key={key} strikethrough>{children(token.tokens, key)}</Text>;
      case "codespan":
        return <Text key={key} color={theme.accent} backgroundColor="#292929"> {token.text} </Text>;
      case "br":
        return <Text key={key}>{"\n"}</Text>;
      case "link":
        return <Text key={key} color={theme.user} underline>{children(token.tokens, key)} <Text color={theme.muted}>({token.href})</Text></Text>;
      case "image":
        return <Text key={key} color={theme.muted}>[image: {token.text}]</Text>;
      case "escape":
      case "text":
        return <Text key={key}>{"tokens" in token && token.tokens ? inline(token.tokens, key) : token.text}</Text>;
      case "html":
        return <Text key={key}>{token.text}</Text>;
      default:
        return <Text key={key}>{"text" in token ? String(token.text) : ""}</Text>;
    }
  });
}

function blocks(tokens: Token[], keyPrefix = "md"): ReactNode[] {
  return tokens.map((token, index) => {
    const key = `${keyPrefix}-${index}`;
    switch (token.type) {
      case "space":
        return null;
      case "heading":
        return <Box key={key} marginTop={index === 0 ? 0 : 1}><Text bold color={token.depth <= 2 ? theme.brand : undefined}>{children(token.tokens, key)}</Text></Box>;
      case "paragraph":
        return <Box key={key}><Text>{children(token.tokens, key)}</Text></Box>;
      case "text":
        return <Box key={key}><Text>{"tokens" in token && token.tokens ? inline(token.tokens, key) : token.text}</Text></Box>;
      case "code":
        return <Box key={key} flexDirection="column" marginY={1} paddingX={1} borderStyle="round" borderColor={theme.chrome}>
          {token.lang ? <Text color={theme.muted} dimColor>{token.lang}</Text> : null}
          <Text>{token.text}</Text>
        </Box>;
      case "blockquote":
        return <Box key={key} flexDirection="column" marginLeft={1} paddingLeft={1} borderStyle="single" borderLeft borderColor={theme.chrome}>
          {blocks(token.tokens ?? [], key)}
        </Box>;
      case "list":
        const list = token as Tokens.List;
        return <Box key={key} flexDirection="column" marginY={1}>
          {list.items.map((item, itemIndex) => <Box key={`${key}-${itemIndex}`} flexDirection="row">
            <Text color={theme.muted}>{item.task ? (item.checked ? "☑ " : "☐ ") : list.ordered ? `${Number(list.start || 1) + itemIndex}. ` : "• "}</Text>
            <Box flexGrow={1} flexDirection="column">{blocks(item.tokens, `${key}-${itemIndex}`)}</Box>
          </Box>)}
        </Box>;
      case "hr":
        return <Box key={key} marginY={1}><Text color={theme.chrome}>────────────────────────────────</Text></Box>;
      case "table":
        const table = token as Tokens.Table;
        return <Box key={key} flexDirection="column" marginY={1}>
          {[table.header, ...table.rows].map((row, rowIndex) => <Text key={`${key}-row-${rowIndex}`}>
            {row.map((cell, cellIndex) => <Text key={`${key}-${rowIndex}-${cellIndex}`}>
              {cellIndex ? "  │  " : ""}<Text bold={rowIndex === 0}>{inline(cell.tokens, `${key}-${rowIndex}-${cellIndex}`)}</Text>
            </Text>)}
          </Text>)}
        </Box>;
      case "html":
        return <Box key={key}><Text>{token.text}</Text></Box>;
      default:
        return null;
    }
  });
}

export function Markdown({ content }: { content: string }) {
  const tokens = marked.lexer(content, { gfm: true, breaks: true });
  return <Box flexDirection="column">{blocks(tokens)}</Box>;
}
