import { Box, Text, measureElement, useInput, useStdin, useStdout } from "ink";
import type { DOMElement } from "ink";
import { useEffect, useRef, useState } from "react";
import { readFile, realpath, stat } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";

import { useSession } from "./session";

import { Message } from "./components/Message";
import { Composer } from "./components/Composer";
import { Header } from "./components/Header";
import { Mascot } from "./components/Mascot";
import { ModelPicker } from "./components/ModelPicker";
import { theme } from "./theme";
import { formatChatPrompt, type ChatAttachment } from "../../shared/attachments";

export function App({ initialSetup = false }: { initialSetup?: boolean }) {
  const { state, send, openModelPicker, selectProvider, chooseModel, closeModelPicker, confirm, answerQuestion, cancel } = useSession(initialSetup);

  const [input, setInput] = useState("");
  const [expandedToolGroups, setExpandedToolGroups] = useState<Record<string, boolean>>({});
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentMode, setAttachmentMode] = useState(false);
  const [attachmentError, setAttachmentError] = useState("");
  const rootElement = useRef<DOMElement>(null);
  const toolActivityTriggers = useRef<Array<{ id: string; element: DOMElement }>>([]);
  toolActivityTriggers.current = [];
  const attachmentTrigger = useRef<DOMElement>(null);
  const terminalOriginRow = useRef(0);
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const recentMessages = state.messages;
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );

  useEffect(() => {
    if (!state.question) setInput("");
  }, [state.question]);

  useInput((input, key) => {
    if ((key.meta || key.ctrl) && input.toLowerCase() === "o") {
      if (state.modelPicker) return;
      const latestGroup = [...recentMessages].reverse().find((message) => message.role === "tool");
      if (latestGroup) setExpandedToolGroups((current) => ({
        ...current,
        [latestGroup.id]: !(current[latestGroup.id] ?? false),
      }));
      return;
    }
    // Ink strips the leading ESC from unknown CSI sequences before dispatching
    // them to useInput handlers.
    const cursorReport = /^\u001B?\[(\d+);(\d+)R$/.exec(input);
    if (cursorReport) {
      const rootHeight = rootElement.current ? measureElement(rootElement.current).height : 0;
      terminalOriginRow.current = stdout.isTTY && rootHeight >= stdout.rows
        ? 0
        : Math.max(0, Number(cursorReport[1]) - rootHeight - 1);
      return;
    }

    const match = /^\u001B?\[<(\d+);(\d+);(\d+)([Mm])$/.exec(input);
    if (!match || match[4] !== "M" || Number(match[1]) !== 0) return;
    const x = Number(match[2]) - 1;
    const y = Number(match[3]) - 1 - terminalOriginRow.current;
    const hit = (element: DOMElement | null) => {
      if (!element) return false;
      let top = 0;
      let left = 0;
      for (let node: DOMElement | undefined = element; node; node = node.parentNode) {
        top += node.yogaNode?.getComputedTop() ?? 0;
        left += node.yogaNode?.getComputedLeft() ?? 0;
      }
      const { width, height } = measureElement(element);
      return x >= left && x < left + width && y >= top && y < top + height;
    };
    const toolTrigger = toolActivityTriggers.current.find(({ element }) => hit(element));
    if (toolTrigger) setExpandedToolGroups((current) => ({
      ...current,
      [toolTrigger.id]: !(current[toolTrigger.id] ?? false),
    }));
    else if (hit(attachmentTrigger.current)) {
      setAttachmentMode(true);
      setAttachmentError("");
      setInput("");
    }
  });

  useEffect(() => {
    if (!isRawModeSupported || !stdout.isTTY) return;
    stdout.write("\u001B[?1000h\u001B[?1006h\u001B[6n");
    return () => {
      stdout.write("\u001B[?1000l\u001B[?1006l");
    };
  }, [isRawModeSupported, stdout]);

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
      setAttachmentError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setAttachmentMode(false);
      setInput("");
    }
  }

  return (
    <Box ref={rootElement} flexDirection="column" paddingX={1}>
      <Header session={state.session} />

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
            </Box>
          </Box>
        ) : null}
        {recentMessages.map((message, index) => {
          if (message.role === "tool") {
            if (recentMessages[index - 1]?.role === "tool") return null;
            const group = recentMessages.slice(index).filter((item, offset, all) => offset === 0 || all.slice(0, offset).every((entry) => entry.role === "tool"))
              .filter((item) => item.role === "tool");
            const expanded = expandedToolGroups[message.id] ?? false;
            return <Box key={`tool-group-${message.id}`} flexDirection="column" marginTop={1}>
              <Box ref={(element) => { if (element) toolActivityTriggers.current.push({ id: message.id, element }); }}>
                <Text color={theme.muted} bold>{expanded ? "⌄" : "›"} {group.length} tool{group.length === 1 ? "" : "s"} ran · click or Cmd/Ctrl+O</Text>
              </Box>
              {expanded ? group.map((tool) => <Message key={tool.id} message={tool} running={tool.content.startsWith(`${state.activeTool}:`)} />) : null}
            </Box>;
          }
          return <Message key={message.id} message={message} detailsExpanded={false} />;
        })}
      </Box>

      <Composer
        disabled={Boolean(state.modelPicker)}
        status={state.status}
        activeTool={state.activeTool}
        confirmation={state.confirmation}
        onConfirm={(allowed) => confirm(allowed)}
        question={state.question}
        onAnswer={answerQuestion}
        onCancelQuestion={cancel}
        toolCalls={state.messages.filter((message) => message.role === "tool").length}
        turns={state.messages.filter((message) => message.role === "user").length}
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
          if (!input.trim() && attachments.length === 0) return;
          send(formatChatPrompt(input.trim() || "Please inspect the attached files.", attachments));
          setInput("");
          setAttachments([]);
        }}
      />
    </Box>
  );
}
