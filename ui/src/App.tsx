import { Box, Text } from "ink";
import { useState } from "react";

import { useSession } from "./session";

import { Message } from "./components/Message";
import { Composer } from "./components/Composer";
import { Header } from "./components/Header";
import { Mascot } from "./components/Mascot";
import { Tips } from "./components/Tips";
import { theme } from "./theme";

export function App() {
  const { state, send } = useSession();

  const [input, setInput] = useState("");
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );

  return (
    <Box flexDirection="column" paddingX={1}>
      <Header session={state.session} />

      <Box flexDirection="column">
        {!hasConversation ? (
          <Box flexDirection="row" alignItems="flex-start" marginTop={1} marginBottom={1} gap={2}>
            <Mascot />
            <Box flexDirection="column" flexGrow={1}>
              <Text color="white" bold>Hi, I'm Luma.</Text>
              <Text>I'm here to help you understand and change your code.</Text>
              <Box marginTop={1} flexDirection="column">
                <Text color="gray">Try asking me to:</Text>
                <Text><Text color={theme.accent}>›</Text> Find a bug or explain unfamiliar code</Text>
                <Text><Text color={theme.accent}>›</Text> Implement a feature or refactor</Text>
                <Text><Text color={theme.accent}>›</Text> Run checks and investigate failures</Text>
              </Box>
              <Tips />
            </Box>
          </Box>
        ) : null}
        {state.messages.map((message) => (
          <Message key={message.id} message={message} />
        ))}
      </Box>

      <Composer
        status={state.status}
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
