import { Text, Box, useInput } from "ink";
import { glyphs, theme } from "../theme";

type Props = {
  value: string;
  onChange(value: string): void;
  onSubmit(): void;
};

export function Composer({ value, onChange, onSubmit }: Props) {
  useInput((char, key) => {
    if (key.return) {
      onSubmit();
      return;
    }

    if (key.backspace) {
      onChange(value.slice(0, -1));
      return;
    }

    onChange(value + char);
  });

  return (
    <Box marginTop={1}>
      <Text color={theme.accent} bold>
        {glyphs.input}{" "}
      </Text>

      <Text>{value}</Text>
    </Box>
  );
}
