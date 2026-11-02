import { Box, Text } from "ink";
import { useState } from "react";

import { useSession } from "./session";

import { Message } from "./components/Message";
import { Composer } from "./components/Composer";
import { Header } from "./components/Header";
import { Mascot } from "./components/Mascot";
import { Tips } from "./components/Tips";
import { ModelPicker } from "./components/ModelPicker";
import { theme } from "./theme";

export function App({ initialSetup = false }: { initialSetup?: boolean }) {
  const { state, send, openModelPicker, selectProvider, chooseModel, closeModelPicker, confirm } = useSession(initialSetup);

  const [input, setInput] = useState("");
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );

  return (
    <Box flexDirection="column" paddingX={1}>
      <Header session={state.session} />

      {state.modelPicker ? <ModelPicker
        providers={state.modelPicker.providers}
        savedApiKeys={state.modelPicker.config?.provider_api_keys}
        models={state.modelPicker.models}
        stage={state.modelPicker.stage}
        loading={state.modelPicker.loading}
        error={state.modelPicker.error}
        onSelectProvider={selectProvider}
        onChooseModel={chooseModel}
        onClose={closeModelPicker}
      /> : null}

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
        disabled={Boolean(state.modelPicker)}
        status={state.status}
        activeTool={state.activeTool}
        confirmation={state.confirmation}
        onConfirm={(allowed) => confirm(allowed)}
        toolCalls={state.messages.filter((message) => message.role === "tool").length}
        turns={state.messages.filter((message) => message.role === "user").length}
        value={input}
        onChange={setInput}
        onOpenModelPicker={openModelPicker}
        onSubmit={() => {
          send(input);
          setInput("");
        }}
      />
    </Box>
  );
}
