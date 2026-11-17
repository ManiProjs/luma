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
import { CommandPalette } from "./components/CommandPalette";
import { CommandPanel } from "./components/CommandPanel";
import { theme, themeLabel } from "./theme";
import { useTerminalMouse } from "./hooks/useTerminalMouse";
import { useTerminalFullscreen } from "./hooks/useTerminalFullscreen";
import { useTerminalTheme } from "./hooks/useTerminalTheme";
import { formatChatPrompt, type ChatAttachment } from "../../shared/attachments";

export function App({ initialSetup = false }: { initialSetup?: boolean }) {
  const { exit } = useApp();
  const { state, send, openModelPicker, selectProvider, chooseModel, closeModelPicker, confirm, answerQuestion, cancel, runCommand, commandAction, dismissRuntimeNotice } = useSession(initialSetup);

  const [input, setInput] = useState("");
  const [expandedToolGroups, setExpandedToolGroups] = useState<Record<string, boolean>>({});
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentMode, setAttachmentMode] = useState(false);
  const [attachmentError, setAttachmentError] = useState("");
  const [fullScreen, setFullScreen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [commandPanel, setCommandPanel] = useState<"git" | "files" | "find_files" | "search" | "doctor" | "monitor" | "teammates" | "tasks" | "approvals" | "schedules" | "memory" | "history" | "history_show" | "history_delete" | "history_prune" | "workspace" | "status" | "version" | "tools" | "skills" | "plugins" | "mcp" | null>(null);
  const [transcriptOffset, setTranscriptOffset] = useState(0);
  const rootElement = useRef<DOMElement>(null);
  const toolActivityTriggers = useRef<Array<{ id: string; element: DOMElement }>>([]);
  toolActivityTriggers.current = [];
  const attachmentTrigger = useRef<DOMElement>(null);
  const { stdout } = useStdout();
  const selectedTheme = useTerminalTheme(!state.modelPicker && !commandPaletteOpen && !commandPanel && !state.question && !state.confirmation && !attachmentMode);
  const recentMessages = state.messages;
  const messageWindowSize = Math.max(5, (stdout.rows || 24) - 12);
  const messageEnd = transcriptOffset > 0 ? Math.max(0, recentMessages.length - transcriptOffset) : recentMessages.length;
  const visibleMessageStart = Math.max(0, messageEnd - messageWindowSize);
  const visibleMessages = recentMessages.slice(visibleMessageStart, messageEnd);
  const hasConversation = state.messages.some(
    (message) => message.role !== "system",
  );
  const copyLatestResponse = () => {
    const latest = [...state.messages].reverse().find((message) => message.role === "assistant");
    if (latest) process.stdout.write(`\u001b]52;c;${Buffer.from(latest.content, "utf8").toString("base64")}\u0007`);
  };

  useEffect(() => {
    if (!state.question) setInput("");
  }, [state.question]);

  useEffect(() => {
    if (recentMessages.length === 0) setTranscriptOffset(0);
  }, [recentMessages.length]);

  useInput((input, key) => {
    if (commandPaletteOpen) return;
    if (commandPanel) return;
    if (key.ctrl && input.toLowerCase() === "d" && state.runtimeNotices[0]) {
      dismissRuntimeNotice(state.runtimeNotices[0].event_id);
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "k") {
      setCommandPaletteOpen(true);
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "n") {
      if (!state.modelPicker && !state.question && !state.confirmation) send("/new");
      return;
    }
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
      let groupStart = recentMessages.length - 1;
      while (groupStart >= 0 && recentMessages[groupStart]?.role !== "tool") groupStart -= 1;
      while (groupStart > 0 && recentMessages[groupStart - 1]?.role === "tool") groupStart -= 1;
      const latestGroup = recentMessages[groupStart];
      if (latestGroup?.role === "tool") setExpandedToolGroups((current) => ({
        ...current,
        [latestGroup.id]: !(current[latestGroup.id] ?? false),
      }));
      return;
    }
    if (!state.modelPicker && key.end) { setTranscriptOffset(0); return; }
    if (!state.modelPicker && key.home) { setTranscriptOffset(Math.max(0, recentMessages.length - messageWindowSize)); return; }
    if (!state.modelPicker && key.pageUp) { setTranscriptOffset((offset) => Math.min(Math.max(0, recentMessages.length - messageWindowSize), offset + messageWindowSize)); return; }
    if (!state.modelPicker && key.pageDown) { setTranscriptOffset((offset) => Math.max(0, offset - messageWindowSize)); return; }
  });

  useTerminalMouse(rootElement, (event, contains) => {
    if (state.modelPicker) return;
    if (event.action === "wheel") {
      if (!contains(rootElement.current)) return;
      setTranscriptOffset((offset) => Math.max(0, Math.min(Math.max(0, recentMessages.length - messageWindowSize), offset + (event.wheelDirection === "down" ? -3 : 3))));
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
      setAttachmentMode(false);
      setInput("");
    } catch (reason) {
      setAttachmentError(userError(reason));
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

      {state.runtimeNotices.length > 0 ? <Box flexDirection="column" marginTop={1}>
        {state.runtimeNotices.map((notice) => {
          const payload = typeof notice.payload === "object" && notice.payload !== null
            ? notice.payload as Record<string, unknown>
            : null;
          const detail = typeof payload?.message === "string" ? payload.message
            : typeof payload?.title === "string" ? payload.title : notice.event_type;
          const agent = notice.agent_id ? ` · agent ${notice.agent_id.slice(0, 8)}` : "";
          return <Text key={notice.event_id} color={theme.secondary}>
            · {detail}{agent}  <Text color={theme.chrome}>[Ctrl+D dismiss oldest]</Text>
          </Text>;
        })}
      </Box> : null}

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

      {commandPaletteOpen ? <CommandPalette
        commands={state.commands}
        onClose={() => setCommandPaletteOpen(false)}
        onSelect={(command) => {
          setCommandPaletteOpen(false);
          if (command === "/model" || command === "/model session") openModelPicker(command === "/model session" ? "session" : "global");
          else if (command === "/commands") setCommandPaletteOpen(true);
          else if (command === "/fullscreen") setFullScreen((current) => !current);
          else if (command === "/quit") exit();
          else if (command === "/git") { setCommandPanel("git"); runCommand("git", "status"); }
          else if (command === "/files") { setCommandPanel("files"); runCommand("files"); }
          else if (command === "/find") setCommandPanel("find_files");
          else if (command === "/search") setCommandPanel("search");
          else if (command === "/doctor") { setCommandPanel("doctor"); runCommand("doctor"); }
          else if (command === "/monitor") { setCommandPanel("monitor"); runCommand("monitor"); }
          else if (command === "/teammates" || command === "/agents") { setCommandPanel("teammates"); runCommand("teammates"); }
          else if (command === "/tasks") { setCommandPanel("tasks"); runCommand("tasks"); }
          else if (command === "/approvals") { setCommandPanel("approvals"); runCommand("approvals"); }
          else if (command === "/schedules") { setCommandPanel("schedules"); runCommand("schedules"); }
          else if (command === "/memory") { setCommandPanel("memory"); runCommand("memory"); }
          else if (command === "/history" || command === "/history list") { setCommandPanel("history"); runCommand("history"); }
          else if (command === "/history show") setCommandPanel("history_show");
          else if (command === "/history delete") setCommandPanel("history_delete");
          else if (command === "/history prune") setCommandPanel("history_prune");
          else if (["/workspace", "/status", "/version", "/tools", "/skills", "/plugins", "/mcp"].includes(command)) {
            const id = command.slice(1) as "workspace" | "status" | "version" | "tools" | "skills" | "plugins" | "mcp";
            setCommandPanel(id);
            runCommand(id);
          }
          else if (command === "/restore") setInput("/restore ");
          else if (command === "/plan") setInput("/plan ");
          else if (command === "/rename") setInput("/rename ");
          else if (command === "/summary:add") setInput("/summary:add ");
          else if (command === "/copy") copyLatestResponse();
          else send(command);
        }}
      /> : null}

      {commandPanel ? <CommandPanel
        id={commandPanel}
        output={state.commandOutput && (state.commandOutput.id === commandPanel || ((commandPanel === "files" || commandPanel === "find_files") && state.commandOutput.id === "read_file")) ? state.commandOutput.content : null}
        loading={state.commandLoading}
        error={state.commandError}
        onClose={() => setCommandPanel(null)}
        onDismissNotice={() => {
          const oldest = state.runtimeNotices[0];
          if (oldest) dismissRuntimeNotice(oldest.event_id);
        }}
        onRun={(id, query) => {
          if (id === "history_delete" && query) send(`/history delete ${query}`);
          else if (id === "history_prune" && query?.trim() === "confirm") send("/history prune");
          else runCommand(id, query);
        }}
        onAction={commandAction}
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
            const absoluteIndex = visibleMessageStart + index;
            let groupStart = absoluteIndex;
            let groupEnd = absoluteIndex + 1;
            while (groupStart > 0 && recentMessages[groupStart - 1]?.role === "tool") groupStart -= 1;
            while (groupEnd < recentMessages.length && recentMessages[groupEnd]?.role === "tool") groupEnd += 1;
            const group = recentMessages.slice(groupStart, groupEnd);
            const groupId = group[0]?.id ?? message.id;
            const expanded = expandedToolGroups[groupId] ?? false;
            return <Box key={`tool-group-${message.id}`} flexDirection="column" marginTop={1}>
              <Box ref={(element) => { if (element) toolActivityTriggers.current.push({ id: groupId, element }); }}>
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
        disabled={Boolean(state.modelPicker) || commandPaletteOpen || Boolean(commandPanel)}
        commands={state.commands}
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
          if (input.trim().toLowerCase() === "/commands") {
            setCommandPaletteOpen(true);
            setInput("");
            return;
          }
          if (input.trim().toLowerCase() === "/copy") {
            copyLatestResponse();
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
      <Text color={theme.chrome}>Ctrl+T theme ({themeLabel(selectedTheme)})</Text>
    </Box>
  );
}
