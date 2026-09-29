import { Box, Text } from "ink";
import { useSession } from "./session";
import { Message } from "./components/Message";
import { Composer } from "./components/Composer";
import { glyphs, theme } from "./theme";
import { useState } from "react";

export function App() {
  const { state, send } = useSession();
  const [input, setInput] = useState("");

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text color={theme.brand} bold>
        {glyphs.logo} Luma
      </Text>

      <Text color={theme.muted}>local · model · workspace</Text>

      <Box flexDirection="column" marginTop={1}>
        {state.messages.map((message) => (
          <Message key={message.id} message={message} />
        ))}
      </Box>

      <Composer
        value={input}
        onChange={setInput}
        onSubmit={() => {
          send(input);
          setInput("");
        }}
      />
    </Box>
  );
}
