import { Box, Text, measureElement, useInput, useStdin, useStdout } from "ink";
import type { DOMElement } from "ink";
import { useEffect, useRef, useState } from "react";

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
  const [toolsExpanded, setToolsExpanded] = useState(false);
  const rootElement = useRef<DOMElement>(null);
  const toolActivityTrigger = useRef<DOMElement>(null);
  const terminalOriginRow = useRef(0);
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const lastUserIndex = state.messages.map((message) => message.role).lastIndexOf("user");
  const toolMessages = state.messages.slice(lastUserIndex + 1).filter((message) => message.role === "tool");
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );

  useInput((input) => {
    const cursorReport = /^\u001B\[(\d+);(\d+)R$/.exec(input);
    if (cursorReport) {
      const rootHeight = rootElement.current ? measureElement(rootElement.current).height : 0;
      terminalOriginRow.current = stdout.isTTY && rootHeight >= stdout.rows
        ? 0
        : Math.max(0, Number(cursorReport[1]) - rootHeight - 1);
      return;
    }

    const match = /^\u001B\[<(\d+);(\d+);(\d+)([Mm])$/.exec(input);
    if (!match || match[4] !== "M" || Number(match[1]) !== 0) return;
    const element = toolActivityTrigger.current;
    if (!element) return;

    let top = 0;
    let left = 0;
    for (let node: DOMElement | undefined = element; node; node = node.parentNode) {
      top += node.yogaNode?.getComputedTop() ?? 0;
      left += node.yogaNode?.getComputedLeft() ?? 0;
    }
    const { width, height } = measureElement(element);
    const x = Number(match[2]) - 1;
    const y = Number(match[3]) - 1 - terminalOriginRow.current;
    if (x >= left && x < left + width && y >= top && y < top + height) {
      setToolsExpanded((expanded) => !expanded);
    }
  });

  useEffect(() => {
    if (!isRawModeSupported || !stdout.isTTY) return;
    stdout.write("\u001B[?1000h\u001B[?1006h\u001B[6n");
    return () => {
      stdout.write("\u001B[?1000l\u001B[?1006l");
    };
  }, [isRawModeSupported, stdout]);

  return (
    <Box ref={rootElement} flexDirection="column" paddingX={1}>
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
          <Box flexDirection="row" alignItems="flex-start" marginTop={2} marginBottom={1} gap={1}>
            <Box marginTop={0}>
              <Mascot compact />
            </Box>
            <Box flexDirection="column" flexGrow={1}>
              <Text color="white" bold>What are we working on?</Text>
              <Text color={theme.muted}>Ask Luma to explore, explain, or change this codebase.</Text>
              <Box marginTop={1} flexDirection="column" gap={0}>
                <Text color={theme.chrome}>A few ways to get started</Text>
                <Text><Text color={theme.brand}>›</Text> Find a bug or explain unfamiliar code</Text>
                <Text><Text color={theme.brand}>›</Text> Build a feature or refactor a module</Text>
                <Text><Text color={theme.brand}>›</Text> Run checks and investigate a failure</Text>
              </Box>
              <Tips />
            </Box>
          </Box>
        ) : null}
        {state.messages.filter((message) => message.role !== "tool").map((message) => (
          <Message
            key={message.id}
            message={message}
            running={message.role === "tool" && state.activeTool !== null && message.content.startsWith(`${state.activeTool}:`)}
          />
        ))}
        {toolMessages.length > 0 ? <Box flexDirection="column" marginTop={1}>
          <Box ref={toolActivityTrigger}>
            <Text color={theme.muted} bold>{toolsExpanded ? "⌄" : "›"} {toolMessages.length} tool{toolMessages.length === 1 ? "" : "s"} ran · click or Ctrl+O</Text>
          </Box>
          {toolsExpanded ? toolMessages.map((message) => <Message key={message.id} message={message} running={message.content.startsWith(`${state.activeTool}:`)} />) : null}
        </Box> : null}
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
        onToggleToolActivity={() => setToolsExpanded((value) => !value)}
        onSubmit={() => {
          send(input);
          setInput("");
        }}
      />
    </Box>
  );
}
