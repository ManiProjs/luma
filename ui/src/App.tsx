import { Box, Text, useApp, useInput, useStdout } from "ink";
import { userError } from "./errors";
import type { DOMElement } from "ink";
import { useEffect, useRef, useState } from "react";
import { readFile, realpath, stat } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";

import { useSession } from "./session";

import { Message } from "./components/Message";
import { Composer } from "./components/Composer";
import { Header } from "./components/Header";
import { ModelPicker } from "./components/ModelPicker";
import { theme } from "./theme";
import { useTerminalMouse } from "./hooks/useTerminalMouse";
import { useTerminalFullscreen } from "./hooks/useTerminalFullscreen";
import { formatChatPrompt, type ChatAttachment } from "../../shared/attachments";

export function App({ initialSetup = false }: { initialSetup?: boolean }) {
  const { exit } = useApp();
  const { state, send, openModelPicker, selectProvider, chooseModel, closeModelPicker, confirm, answerQuestion, cancel } = useSession(initialSetup);

  const [input, setInput] = useState("");
  const [expandedToolGroups, setExpandedToolGroups] = useState<Record<string, boolean>>({});
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentMode, setAttachmentMode] = useState(false);
  const [attachmentError, setAttachmentError] = useState("");
  const [fullScreen, setFullScreen] = useState(false);
  const [transcriptOffset, setTranscriptOffset] = useState(0);
  const rootElement = useRef<DOMElement>(null);
  const toolActivityTriggers = useRef<Array<{ id: string; element: DOMElement }>>([]);
  toolActivityTriggers.current = [];
  const attachmentTrigger = useRef<DOMElement>(null);
  const { stdout } = useStdout();
  const recentMessages = state.messages;
  const messageWindowSize = Math.max(5, (stdout.rows || 24) - 12);
  const messageEnd = transcriptOffset > 0 ? Math.max(0, recentMessages.length - transcriptOffset) : recentMessages.length;
  const visibleMessages = recentMessages.slice(Math.max(0, messageEnd - messageWindowSize), messageEnd);
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );

  useEffect(() => {
    if (!state.question) setInput("");
  }, [state.question]);

  useEffect(() => {
    if (recentMessages.length === 0) setTranscriptOffset(0);
  }, [recentMessages.length]);

  useInput((input, key) => {
    if ((key.ctrl || key.meta) && input.toLowerCase() === "c") {
      if (!state.modelPicker && !state.question && !state.confirmation
        && state.status !== "thinking" && state.status !== "working" && state.status !== "streaming" && state.status !== "waiting" && state.status !== "cancelling") {
        exit();
      }
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "f") {
      if (!state.modelPicker && !state.question && !state.confirmation && !attachmentMode) {
        setFullScreen((current) => !current);
      }
      return;
    }
    if ((key.meta || key.ctrl) && input.toLowerCase() === "o") {
      if (state.modelPicker) return;
      const latestGroup = [...recentMessages].reverse().find((message) => message.role === "tool");
      if (latestGroup) setExpandedToolGroups((current) => ({
        ...current,
        [latestGroup.id]: !(current[latestGroup.id] ?? false),
      }));
      return;
    }
    if (!state.modelPicker && key.end) { setTranscriptOffset(0); return; }
    if (!state.modelPicker && key.home) { setTranscriptOffset(Math.max(0, recentMessages.length - 1)); return; }
    if (!state.modelPicker && key.pageUp) { setTranscriptOffset((offset) => Math.min(Math.max(0, recentMessages.length - 1), offset + messageWindowSize)); return; }
    if (!state.modelPicker && key.pageDown) { setTranscriptOffset((offset) => Math.max(0, offset - messageWindowSize)); return; }
  });

  useTerminalMouse(rootElement, (event, contains) => {
    if (event.action === "wheel") {
      if (state.modelPicker || !contains(rootElement.current)) return;
      setTranscriptOffset((offset) => Math.max(0, Math.min(Math.max(0, recentMessages.length - 1), offset + (event.wheelDirection === "down" ? -3 : 3))));
      return;
    }
    if (event.action !== "press" || event.button !== 0) return;
    const toolTrigger = toolActivityTriggers.current.find(({ element }) => contains(element));
    if (toolTrigger) setExpandedToolGroups((current) => ({
      ...current,
      [toolTrigger.id]: !(current[toolTrigger.id] ?? false),
    }));
    else if (contains(attachmentTrigger.current)) {
      setAttachmentMode(true);
      setAttachmentError("");
      setInput("");
    }
  });

  useTerminalFullscreen(fullScreen);

  async function attachFile(pathInput: string) {
    try {
      const workspace = await realpath(process.cwd());
      const target = await realpath(resolve(workspace, pathInput));
      const relativePath = relative(workspace, target);
      if (relativePath === ".." || relativePath.startsWith(`..${sep}`) || isAbsolute(relativePath)) {
        throw new Error("Only files inside the current workspace can be attached.");
      }
      const metadata = await stat(target);
      if (!metadata.isFile()) throw new Error("Choose a file, not a directory.");
      if (metadata.size > 1024 * 1024) throw new Error("Each attached file must be 1 MiB or smaller.");
      const bytes = await readFile(target);
      if (bytes.includes(0)) throw new Error("Binary files cannot be attached as text.");
      const content = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      const next = [...attachments, { name: relativePath, content }];
      const totalBytes = next.reduce((total, item) => total + new TextEncoder().encode(item.content).byteLength, 0);
      if (next.length > 8 || totalBytes > 4 * 1024 * 1024) throw new Error("Attach up to 8 text files and 4 MiB of content.");
      setAttachments(next);
      setAttachmentError("");
    } catch (reason) {
      setAttachmentError(userError(reason));
    } finally {
      setAttachmentMode(false);
      setInput("");
    }
  }

  if (state.connection !== "connected") {
    return <Box flexDirection="column" paddingX={2} paddingY={1}>
      <Text color={theme.brand} bold>LUMA</Text>
      {state.connection === "connecting"
        ? <Text color={theme.muted}>Connecting...</Text>
        : <><Text color={theme.error}>Couldn't connect to Luma Core</Text><Text color={theme.muted}>{state.connectionError ?? "The core process failed to start."}</Text><Text color={theme.chrome}>Check the core installation and try launching Luma again. Ctrl+C to exit.</Text></>}
    </Box>;
  }

  return (
    <Box ref={rootElement} flexDirection="column" width="100%" paddingX={fullScreen ? 2 : 1} height={fullScreen && stdout.isTTY ? Math.max(1, stdout.rows - 1) : undefined}>
      <Header session={state.session} fullScreen={fullScreen} />

      {state.modelPicker ? <ModelPicker
        providers={state.modelPicker.providers}
        scope={state.modelPicker.scope}
        savedApiKeys={state.modelPicker.config?.provider_api_keys}
        models={state.modelPicker.models}
        stage={state.modelPicker.stage}
        loading={state.modelPicker.loading}
        error={state.modelPicker.error}
        onSelectProvider={selectProvider}
        onChooseModel={chooseModel}
        onClose={closeModelPicker}
      /> : null}

      <Box flexDirection="column" width="100%" flexGrow={fullScreen ? 1 : 0} minHeight={0} overflow={fullScreen ? "hidden" : "visible"}>
        {!hasConversation ? (
          <Box flexDirection="column" marginTop={2} marginBottom={1}>
            <Text color={theme.text} bold>What are we working on?</Text>
            <Text color={theme.muted}>Ask Luma to explore, explain, or change this codebase.</Text>
          </Box>
        ) : null}
        {transcriptOffset > 0 ? <Text color={theme.chrome}>↑ Viewing older messages · scroll down or press End for latest</Text> : null}
        {visibleMessages.map((message, index) => {
          if (message.role === "tool") {
            if (visibleMessages[index - 1]?.role === "tool") return null;
            const group = visibleMessages.slice(index).filter((item, offset, all) => offset === 0 || all.slice(0, offset).every((entry) => entry.role === "tool"))
              .filter((item) => item.role === "tool");
            const expanded = expandedToolGroups[message.id] ?? false;
            return <Box key={`tool-group-${message.id}`} flexDirection="column" marginTop={1}>
              <Box ref={(element) => { if (element) toolActivityTriggers.current.push({ id: message.id, element }); }}>
                <Text color={theme.chrome}>{expanded ? "⌄" : "›"} {group.length} tool{group.length === 1 ? "" : "s"} · Cmd/Ctrl+O for details</Text>
              </Box>
              {expanded ? group.map((tool) => <Message key={tool.id} message={tool} running={tool.content.startsWith(`${state.activeTool}:`)} />) : null}
            </Box>;
          }
          const isStreamingMessage = message.role === "assistant"
            && state.status === "streaming"
            && message.id === recentMessages[recentMessages.length - 1]?.id;
          return <Message key={message.id} message={message} streaming={isStreamingMessage} detailsExpanded={false} />;
        })}
      </Box>

      <Box width="100%" marginTop={1}>
      <Composer
        disabled={Boolean(state.modelPicker)}
        status={state.status}
        activeTool={state.activeTool}
        usage={state.usage}
        lastPromptTokens={state.lastPromptTokens}
        confirmation={state.confirmation}
        onConfirm={(allowed) => confirm(allowed)}
        question={state.question}
        onAnswer={answerQuestion}
        onCancelQuestion={cancel}
        onCancelRequest={cancel}
        value={input}
        onChange={setInput}
        onOpenModelPicker={openModelPicker}
        attachmentTriggerRef={attachmentTrigger}
        attachments={attachments}
        attachmentMode={attachmentMode}
        attachmentError={attachmentError}
        onCancelAttachment={() => { setAttachmentMode(false); setInput(""); }}
        onRemoveAttachment={(index) => setAttachments((items) => items.filter((_, itemIndex) => itemIndex !== index))}
        onSubmit={() => {
          if (attachmentMode) {
            void attachFile(input.trim());
            return;
          }
          if (input.trim().toLowerCase() === "/fullscreen") {
            setFullScreen((current) => !current);
            setInput("");
            return;
          }
          if (!input.trim() && attachments.length === 0) return;
          setTranscriptOffset(0);
          send(formatChatPrompt(input.trim() || "Please inspect the attached files.", attachments));
          setInput("");
          setAttachments([]);
        }}
      />
      </Box>
      {fullScreen ? <Text color={theme.chrome}>FULL SCREEN  ·  Ctrl+F to return  ·  scroll to browse history</Text> : null}
    </Box>
  );
}
