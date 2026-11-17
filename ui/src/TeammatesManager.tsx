import { useEffect, useRef, useState } from "react";
import { userError } from "./errors";
import { Box, Text, useApp, useInput, useStdout } from "ink";
import type { DOMElement } from "ink";
import { randomUUID } from "node:crypto";
import { LumaClient } from "./client";
import { startDesktopServer, stopDesktopServer } from "./server";
import type { AgentConfiguration, AgentRecord, ClientMessage, RuntimeEvent, ServerEvent, TaskRecord } from "./protocol";
import { Markdown } from "./components/Markdown";
import { theme } from "./theme";
import { useMotionFrame } from "./hooks/useMotionFrame";
import { useTerminalMouse } from "./hooks/useTerminalMouse";
import { useTerminalFullscreen } from "./hooks/useTerminalFullscreen";

type RequestEvent = Extract<ServerEvent, { data: { request_id: string } }>;
type View = "list" | "settings" | "chat";
type SettingField = "personality" | "workspace" | "skills" | "mascot";
type ChatLine = { id: string; role: "user" | "assistant" | "status" | "tool"; content: string };
type PendingQuestion = { taskId: string; questionId: string; prompt: string; choices: string[] };

function TaskActivity({ taskId }: { taskId: string }) {
  const frame = useMotionFrame(true);
  const spinner = ["◐", "◓", "◑", "◒"][Math.floor(frame / 3) % 4];
  return <Text color={theme.brand}>{spinner} Task running · {taskId.slice(0, 8)} · Ctrl+C cancels this task.</Text>;
}

function request<T extends RequestEvent["type"]>(
  client: LumaClient,
  message: ClientMessage,
  expectedType: T,
  requestId: string,
): Promise<Extract<RequestEvent, { type: T }>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { unsubscribe(); reject(new Error(`Timed out waiting for ${expectedType}.`)); }, 15_000);
    const unsubscribe = client.subscribe((event) => {
      if (event.type === "error" && (event.data.request_id === requestId || event.data.request_id === null)) {
        clearTimeout(timer); unsubscribe(); reject(new Error(event.data.message)); return;
      }
      if (event.type === expectedType && "request_id" in event.data && event.data.request_id === requestId) {
        clearTimeout(timer); unsubscribe(); resolve(event as Extract<RequestEvent, { type: T }>);
      }
    });
    try { client.send(message); }
    catch (error) { clearTimeout(timer); unsubscribe(); reject(error); }
  });
}

export function TeammatesManager() {
  const { exit } = useApp();
  const { stdout } = useStdout();
  const [client] = useState(() => new LumaClient());
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [selected, setSelected] = useState(0);
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState("");
  const [view, setView] = useState<View>("list");
  const [editing, setEditing] = useState<SettingField | null>(null);
  const [editValue, setEditValue] = useState("");
  const [chatInput, setChatInput] = useState("");
  const [chatCursor, setChatCursor] = useState(0);
  const [chatOffset, setChatOffset] = useState(0);
  const [fullScreen, setFullScreen] = useState(false);
  const [chatLines, setChatLines] = useState<ChatLine[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const [pendingQuestion, setPendingQuestion] = useState<PendingQuestion | null>(null);
  const sequence = useRef(0);
  const polling = useRef(false);
  const rootElement = useRef<DOMElement>(null);
  const agentRowElements = useRef<Array<DOMElement | null>>([]);
  const chatAction = useRef<DOMElement>(null);
  const settingsAction = useRef<DOMElement>(null);
  const chatWindowSize = Math.max(5, (stdout.rows || 24) - 12);
  const chatEnd = chatOffset > 0 ? Math.max(0, chatLines.length - chatOffset) : chatLines.length;
  const visibleChatLines = chatLines.slice(Math.max(0, chatEnd - chatWindowSize), chatEnd);
  const wideLayout = (stdout.columns || 80) >= 92;
  useTerminalFullscreen(fullScreen);

  useTerminalMouse(rootElement, (event, contains) => {
    if (event.action === "wheel" && view === "chat" && contains(rootElement.current)) {
      setChatOffset((offset) => Math.max(0, Math.min(Math.max(0, chatLines.length - chatWindowSize), offset + (event.wheelDirection === "down" ? -3 : 3))));
      return;
    }
    if (event.action === "wheel" && view === "list" && contains(rootElement.current)) {
      setSelected((index) => Math.max(0, Math.min(agents.length - 1, index + (event.wheelDirection === "down" ? 1 : -1) * 3)));
      return;
    }
    if (event.action !== "press" || event.button !== 0 || busy) return;
    if (view === "list") {
      const row = agentRowElements.current.findIndex((element) => contains(element));
      if (row >= 0) setSelected(row);
      else if (contains(chatAction.current) && selectedAgent) { setChatLines([]); setView("chat"); }
      else if (contains(settingsAction.current) && selectedAgent) setView("settings");
    }
  });

  const selectedAgent = agents[selected];

  async function refresh() {
    const requestId = randomUUID();
    const response = await request(client, { type: "agents_list", data: { request_id: requestId } }, "agents", requestId);
    setAgents(response.data.agents);
    setSelected((index) => Math.min(index, Math.max(0, response.data.agents.length - 1)));
  }

  useEffect(() => {
    let active = true;
    startDesktopServer();
    void client.connect().then(async () => {
      if (!active) return;
      setConnected(true);
      await refresh();
    }).catch((reason) => { if (active) setError(userError(reason)); });
    return () => { active = false; client.disconnect(); stopDesktopServer(); };
  }, [client]);

  async function deleteSelected() {
    const agent = selectedAgent;
    if (!agent || agent.id === "agent_global") return;
    setBusy(true); setError("");
    try {
      const requestId = randomUUID();
      const response = await request(client, { type: "agent_delete", data: { request_id: requestId, id: agent.id } }, "agent_deleted", requestId);
      if (!response.data.deleted) throw new Error("Teammate no longer exists.");
      setConfirmDelete(false);
      setView("list");
      await refresh();
    } catch (reason) { setError(userError(reason)); }
    finally { setBusy(false); }
  }

  async function saveSetting() {
    if (!selectedAgent || !editing) return;
    setBusy(true); setError("");
    try {
      const configuration: AgentConfiguration = { ...selectedAgent.configuration };
      if (editing === "personality") configuration.system_prompt = editValue;
      if (editing === "workspace") {
        const workspace = editValue.trim() || null;
        configuration.workspace = workspace;
        configuration.allowed_workspaces = workspace ? [workspace] : [];
      }
      if (editing === "skills") configuration.skills = [...new Set(editValue.split(",").map((item) => item.trim()).filter(Boolean))];
      if (editing === "mascot") configuration.mascot = editValue.trim() || "auto";
      const requestId = randomUUID();
      const response = await request(client, {
        type: "agent_update",
        data: { request_id: requestId, id: selectedAgent.id, name: null, description: null, configuration },
      }, "agent_saved", requestId);
      setAgents((items) => items.map((agent) => agent.id === response.data.agent.id ? response.data.agent : agent));
      setEditing(null);
      setEditValue("");
    } catch (reason) { setError(userError(reason)); }
    finally { setBusy(false); }
  }

  async function toggleGlobalContext() {
    if (!selectedAgent) return;
    setBusy(true); setError("");
    try {
      const configuration = { ...selectedAgent.configuration, use_global_context: !selectedAgent.configuration.use_global_context };
      const requestId = randomUUID();
      const response = await request(client, { type: "agent_update", data: { request_id: requestId, id: selectedAgent.id, name: null, description: null, configuration } }, "agent_saved", requestId);
      setAgents((items) => items.map((agent) => agent.id === response.data.agent.id ? response.data.agent : agent));
    } catch (reason) { setError(userError(reason)); }
    finally { setBusy(false); }
  }

  async function toggleNetworkAccess() {
    if (!selectedAgent) return;
    setBusy(true); setError("");
    try {
      const enabled = !(selectedAgent.configuration.network_access ?? true);
      const configuration = {
        ...selectedAgent.configuration,
        network_access: enabled,
        permissions: { ...selectedAgent.configuration.permissions, allow_network: enabled },
      };
      const requestId = randomUUID();
      const response = await request(client, { type: "agent_update", data: { request_id: requestId, id: selectedAgent.id, name: null, description: null, configuration } }, "agent_saved", requestId);
      setAgents((items) => items.map((agent) => agent.id === response.data.agent.id ? response.data.agent : agent));
    } catch (reason) { setError(userError(reason)); }
    finally { setBusy(false); }
  }

  async function sendChat() {
    const agent = selectedAgent;
    const prompt = chatInput.trim();
    if (!agent || !prompt || busy || (activeTaskId && pendingQuestion?.taskId !== activeTaskId)) return;
    setChatOffset(0);
    if (pendingQuestion && activeTaskId === pendingQuestion.taskId) {
      setBusy(true); setError("");
      try {
        const requestId = randomUUID();
        await request(client, {
          type: "task_answer_question",
          data: { request_id: requestId, id: pendingQuestion.taskId, question_id: pendingQuestion.questionId, answer: prompt },
        }, "task_saved", requestId);
        setChatLines((lines) => [...lines, { id: randomUUID(), role: "user", content: prompt }, { id: randomUUID(), role: "status", content: "Answer sent." }]);
        setPendingQuestion(null);
        setChatInput("");
        setChatCursor(0);
      } catch (reason) {
        setError(userError(reason));
      } finally { setBusy(false); }
      return;
    }
    setChatLines((lines) => [...lines, { id: randomUUID(), role: "user", content: prompt }, { id: `pending-${Date.now()}`, role: "assistant", content: "Thinking…" }]);
    setBusy(true); setError("");
    try {
      const requestId = randomUUID();
      const response = await request(client, {
        type: "task_create",
        data: { request_id: requestId, agent_id: agent.id, request: prompt, title: null, workspace: agent.configuration.workspace || process.cwd(), recover_on_restart: false },
      }, "task_saved", requestId);
      setActiveTaskId(response.data.task.id);
      setChatInput("");
      setChatCursor(0);
    } catch (reason) {
      setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "assistant", content: `Could not send message: ${userError(reason)}` }]);
    } finally { setBusy(false); }
  }

  async function cancelActiveTask() {
    const taskId = activeTaskId;
    if (!taskId || busy) return;
    setBusy(true); setError("");
    try {
      const requestId = randomUUID();
      const response = await request(client, {
        type: "task_cancel",
        data: { request_id: requestId, id: taskId },
      }, "task_saved", requestId);
      if (response.data.task.status === "cancelled") setActiveTaskId(null);
      setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "status", content: response.data.task.status === "cancelled" ? "Task cancelled." : "Cancellation requested…" }]);
    } catch (reason) {
      setError(userError(reason));
    } finally { setBusy(false); }
  }

  useEffect(() => {
    if (view !== "chat" || !selectedAgent) return;
    let active = true;
    const agentId = selectedAgent.id;
    setHistoryLoading(true);
    setChatLines([]);
    setChatInput("");
    setChatCursor(0);
    setActiveTaskId(null);
    setPendingQuestion(null);
    setError("");

    void (async () => {
      const tasksRequestId = randomUUID();
      const tasksResponse = await request(client, {
        type: "tasks_list",
        data: { request_id: tasksRequestId, agent_id: agentId, limit: 500 },
      }, "tasks", tasksRequestId);
      const tasks = [...tasksResponse.data.tasks]
        .filter((task) => task.schedule_id === null)
        .sort((left, right) => left.created_at - right.created_at) as TaskRecord[];

      const events: RuntimeEvent[] = [];
      let cursor = 0;
      while (true) {
        const eventsRequestId = randomUUID();
        const response = await request(client, {
          type: "events_since",
          data: { request_id: eventsRequestId, sequence: cursor, limit: 1000, agent_id: agentId },
        }, "runtime_events", eventsRequestId);
        events.push(...response.data.events);
        const last = response.data.events.at(-1);
        if (!last || response.data.events.length < 1000) break;
        cursor = last.sequence;
      }
      if (!active) return;
      sequence.current = events.at(-1)?.sequence ?? 0;

      const byTask = new Map<string, RuntimeEvent[]>();
      for (const event of events) {
        if (!event.task_id) continue;
        const entries = byTask.get(event.task_id) ?? [];
        entries.push(event);
        byTask.set(event.task_id, entries);
      }

      const restored: ChatLine[] = [];
      for (const task of tasks) {
        restored.push({ id: `${task.id}-user`, role: "user", content: task.request });
        for (const event of byTask.get(task.id) ?? []) {
          if (event.event_type !== "task.agent_event") continue;
          const payload = event.payload as { type?: string; data?: unknown };
          if (payload.type === "TextDelta" && typeof payload.data === "string") {
            const last = restored[restored.length - 1];
            if (last?.role === "assistant") restored[restored.length - 1] = { ...last, content: last.content + payload.data };
            else restored.push({ id: event.event_id, role: "assistant", content: payload.data });
          } else if (payload.type === "ToolStarted" && typeof payload.data === "object" && payload.data !== null && "name" in payload.data) {
            const tool = payload.data as { name: string; input?: string };
            restored.push({ id: event.event_id, role: "tool", content: `${tool.name}: ${tool.input ?? ""}` });
          } else if (payload.type === "ToolFinished" && typeof payload.data === "object" && payload.data !== null && "name" in payload.data) {
            const tool = payload.data as { name: string; duration_ms?: number; failed?: boolean };
            for (let index = restored.length - 1; index >= 0; index -= 1) {
              const line = restored[index];
              if (line?.role === "tool" && line.content.startsWith(`${tool.name}:`)) {
                restored[index] = { ...line, content: `${line.content}  ·  ${tool.duration_ms ?? 0} ms${tool.failed ? " · failed" : " · done"}` };
                break;
              }
            }
          } else if ((payload.type === "SystemMessage" || payload.type === "Error") && typeof payload.data === "string") {
            restored.push({ id: event.event_id, role: "status", content: payload.data });
          } else if (payload.type === "QuestionRequired" && typeof payload.data === "object" && payload.data !== null) {
            const question = payload.data as { question_id?: string; prompt?: string; choices?: string[] };
            if (typeof question.question_id === "string" && typeof question.prompt === "string" && Array.isArray(question.choices)) {
              restored.push({ id: event.event_id, role: "status", content: `Question: ${question.prompt}${question.choices.length ? ` (Options: ${question.choices.join(" · ")})` : ""}` });
              if (task.status === "waiting_for_input") {
                setPendingQuestion({ taskId: task.id, questionId: question.question_id, prompt: question.prompt, choices: question.choices.filter((choice): choice is string => typeof choice === "string") });
              }
            }
          }
        }
        if (task.error && !["completed", "cancelled"].includes(task.status)) {
          restored.push({ id: `${task.id}-error`, role: "status", content: `${task.status.replaceAll("_", " ")}: ${task.error}` });
        }
      }
      setChatLines(restored);
      const activeTask = [...tasks].reverse().find((task) => ["queued", "running", "waiting_for_approval", "waiting_for_input"].includes(task.status));
      setActiveTaskId(activeTask?.id ?? null);
      if (!activeTask || activeTask.status !== "waiting_for_input") setPendingQuestion(null);
    })().catch((reason) => {
      if (active) setError(userError(reason));
    }).finally(() => {
      if (active) setHistoryLoading(false);
    });

    return () => { active = false; };
  }, [client, selectedAgent?.id, view]);

  useEffect(() => {
    if (!activeTaskId || !selectedAgent) return;
    let active = true;
    const poll = async () => {
      if (polling.current) return;
      polling.current = true;
      try {
        const requestId = randomUUID();
        const response = await request(client, {
          type: "events_since",
          data: { request_id: requestId, sequence: sequence.current, limit: 100, agent_id: selectedAgent.id },
        }, "runtime_events", requestId);
        if (!active) return;
        for (const event of response.data.events) {
          sequence.current = Math.max(sequence.current, event.sequence);
          if (event.task_id !== activeTaskId) continue;
          if (event.event_type === "task.agent_event") {
            const agentEvent = event.payload as { type?: string; data?: unknown };
            if (agentEvent.type === "TextDelta" && typeof agentEvent.data === "string") {
              setChatLines((lines) => {
                const next = [...lines];
                const last = next[next.length - 1];
                if (last?.role === "assistant") next[next.length - 1] = { ...last, content: last.content === "Thinking…" ? agentEvent.data as string : last.content + (agentEvent.data as string) };
                else next.push({ id: randomUUID(), role: "assistant", content: agentEvent.data as string });
                return next;
              });
            } else if (agentEvent.type === "ToolStarted" && typeof agentEvent.data === "object" && agentEvent.data !== null && "name" in agentEvent.data) {
              const tool = agentEvent.data as { name: string; input?: string };
              setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "tool", content: `${tool.name}: ${tool.input ?? ""}` }]);
            } else if (agentEvent.type === "ToolFinished" && typeof agentEvent.data === "object" && agentEvent.data !== null && "name" in agentEvent.data) {
              const tool = agentEvent.data as { name: string; duration_ms?: number; failed?: boolean };
              setChatLines((lines) => {
                const next = [...lines];
                let index = -1;
                for (let cursor = next.length - 1; cursor >= 0; cursor -= 1) {
                  const candidate = next[cursor];
                  if (candidate?.role === "tool" && candidate.content.startsWith(`${tool.name}:`)) { index = cursor; break; }
                }
                const target = index >= 0 ? next[index] : undefined;
                if (target) next[index] = { ...target, content: `${target.content}  ·  ${tool.duration_ms ?? 0} ms${tool.failed ? " · failed" : " · done"}` };
                return next;
              });
            } else if (agentEvent.type === "SystemMessage" && typeof agentEvent.data === "string") {
              setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "status", content: agentEvent.data as string }]);
            } else if (agentEvent.type === "Error" && typeof agentEvent.data === "string") {
              setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "status", content: `Task note: ${agentEvent.data as string}` }]);
            } else if (agentEvent.type === "QuestionRequired" && typeof agentEvent.data === "object" && agentEvent.data !== null) {
              const question = agentEvent.data as { question_id?: string; prompt?: string; choices?: string[] };
              if (typeof question.question_id === "string" && typeof question.prompt === "string" && Array.isArray(question.choices)) {
                const choices = question.choices.filter((choice): choice is string => typeof choice === "string");
                setPendingQuestion({ taskId: activeTaskId, questionId: question.question_id, prompt: question.prompt, choices });
                setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "status", content: `Question: ${question.prompt}${choices.length ? ` (Options: ${choices.join(" · ")})` : ""}` }]);
              }
            }
          }
          if (["task.completed", "task.failed", "task.cancelled", "task.interrupted"].includes(event.event_type)) {
            setChatLines((lines) => lines.filter((line) => line.content !== "Thinking…"));
            if (event.event_type !== "task.completed") {
              const task = event.payload as { error?: string | null };
              if (task.error) setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "assistant", content: `**${event.event_type.replace("task.", "").replaceAll("_", " ")}:** ${task.error}` }]);
            }
            setActiveTaskId(null);
            setPendingQuestion(null);
          }
        }
      } catch (reason) {
        if (active) setError(userError(reason));
      } finally {
        polling.current = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 900);
    return () => { active = false; clearInterval(timer); };
  }, [activeTaskId, client, selectedAgent]);

  useInput((input, key) => {
    if (busy) return;
    if ((key.ctrl || key.meta) && input.toLowerCase() === "f") {
      setFullScreen((current) => !current);
      return;
    }
    if ((key.ctrl || key.meta) && input.toLowerCase() === "c") {
      if (activeTaskId) void cancelActiveTask();
      else exit();
      return;
    }
    if (confirmDelete) {
      if (input.toLowerCase() === "y") void deleteSelected();
      else if (input.toLowerCase() === "n" || key.escape) setConfirmDelete(false);
      return;
    }
    if (editing) {
      if (key.escape) { setEditing(null); setEditValue(""); }
      else if (key.return) void saveSetting();
      else if (key.backspace || key.delete) setEditValue((value) => value.slice(0, -1));
      else if (input && !key.ctrl && !key.meta) setEditValue((value) => value + input);
      return;
    }
    if (view === "chat") {
      if ((key.ctrl || key.meta) && input.toLowerCase() === "c" && activeTaskId) { void cancelActiveTask(); return; }
      if (key.escape) { setView("list"); return; }
      if (key.pageUp) { setChatOffset((offset) => Math.min(Math.max(0, chatLines.length - chatWindowSize), offset + chatWindowSize)); return; }
      if (key.pageDown) { setChatOffset((offset) => Math.max(0, offset - chatWindowSize)); return; }
      if (key.end) { if (chatInput.length > 0) setChatCursor(chatInput.length); else setChatOffset(0); return; }
      if (key.home) { if (chatInput.length > 0) setChatCursor(0); else setChatOffset(Math.max(0, chatLines.length - chatWindowSize)); return; }
      if (key.return) { void sendChat(); return; }
      if (key.backspace || key.delete) {
        if (chatCursor > 0) {
          setChatInput((value) => value.slice(0, chatCursor - 1) + value.slice(chatCursor));
          setChatCursor(chatCursor - 1);
        }
      }
      else if (key.leftArrow) setChatCursor((cursor) => Math.max(0, cursor - 1));
      else if (key.rightArrow) setChatCursor((cursor) => Math.min(chatInput.length, cursor + 1));
      else if (input && !key.ctrl && !key.meta) {
        setChatInput((value) => value.slice(0, chatCursor) + input + value.slice(chatCursor));
        setChatCursor(chatCursor + input.length);
      }
      return;
    }
    if (view === "settings") {
      if (key.escape || input.toLowerCase() === "q") { setView("list"); return; }
      if (!selectedAgent) return;
      const fields: Record<string, SettingField> = { "1": "personality", "2": "workspace", "3": "skills", "4": "mascot" };
      const field = fields[input];
      if (field) {
        setEditing(field);
        setEditValue(field === "personality" ? selectedAgent.configuration.system_prompt : field === "workspace" ? selectedAgent.configuration.workspace ?? "" : field === "skills" ? selectedAgent.configuration.skills.join(", ") : selectedAgent.configuration.mascot);
      } else if (input.toLowerCase() === "g") void toggleGlobalContext();
      else if (input === "5") void toggleNetworkAccess();
      return;
    }
    if (input === "q" || key.escape) { exit(); return; }
    if (key.upArrow) setSelected((index) => Math.max(0, index - 1));
    else if (key.downArrow) setSelected((index) => Math.min(agents.length - 1, index + 1));
    else if (input.toLowerCase() === "d" && selectedAgent && selectedAgent.id !== "agent_global") setConfirmDelete(true);
    else if (input.toLowerCase() === "r") void refresh().catch((reason) => setError(userError(reason)));
    else if (input.toLowerCase() === "e" && selectedAgent) setView("settings");
    else if (input.toLowerCase() === "c" && selectedAgent) { setChatLines([]); setView("chat"); }
  });

  if (!connected) {
    return <Box flexDirection="column" paddingX={2} paddingY={1}>
      <Text color={theme.brand} bold>LUMA / TEAMMATES</Text>
      {error
        ? <><Text color={theme.error}>Couldn't connect to Luma Core</Text><Text color={theme.muted}>{error}</Text><Text color={theme.chrome}>Check the core installation and launch Teammates again.</Text></>
        : <Text color={theme.muted}>Connecting...</Text>}
    </Box>;
  }

  return <Box ref={rootElement} flexDirection="column" width="100%" paddingX={fullScreen ? 2 : 1} height={fullScreen && stdout.isTTY ? Math.max(1, stdout.rows - 1) : undefined}>
    <Box marginBottom={1} flexDirection="row" flexWrap="wrap" columnGap={1}>
      <Text bold color={theme.brand}>Luma</Text><Text color={theme.chrome}>·</Text><Text color={theme.text}>Teammates</Text>
      {view !== "list" ? <><Text color={theme.chrome}>·</Text><Text color={theme.muted}>{view}</Text></> : null}
      <Text color={theme.chrome}>·</Text><Text color={connected ? theme.muted : theme.warning}>{connected ? "connected" : "connecting"}</Text>
      {fullScreen ? <Text color={theme.brand}>· fullscreen</Text> : null}
    </Box>

    {view === "list" ? <Box marginTop={1} flexDirection={wideLayout ? "row" : "column"} flexGrow={fullScreen ? 1 : 0} minHeight={0}>
      <Box width={wideLayout ? "34%" : "100%"} paddingRight={wideLayout ? 2 : 0} flexDirection="column">
        <Text color={theme.chrome}>{agents.length} teammates</Text>
        {agents.length === 0 ? <Text color={theme.muted}>No teammates yet. Create one with `luma teammates create &lt;name&gt;`.</Text> : agents.map((item, index) => <Box key={item.id} ref={(element) => { agentRowElements.current[index] = element; }} flexDirection="column" marginTop={1}>
          <Text color={index === selected ? theme.text : theme.muted}><Text color={index === selected ? theme.brand : theme.chrome}>{index === selected ? "› " : "  "}</Text>{item.name}<Text color={theme.chrome}>  {item.status === "idle" ? "" : `· ${item.status}`}</Text></Text>
        </Box>)}
      </Box>
      {selectedAgent ? <Box flexGrow={1} flexDirection="column" borderLeft={wideLayout} borderColor={theme.border} paddingLeft={wideLayout ? 2 : 0} marginTop={wideLayout ? 0 : 2}>
        <Text bold color={theme.text}>{selectedAgent.name}</Text>
        <Text color={theme.muted}>{selectedAgent.kind} · {selectedAgent.status}</Text>
        <Text color={theme.secondary}>{selectedAgent.description || "No description"}</Text>
        <Box marginTop={1} flexDirection="row" flexWrap="wrap" columnGap={1}><Text color={theme.chrome}>Skills</Text><Text color={theme.secondary}>{selectedAgent.configuration.skills.length ? selectedAgent.configuration.skills.join(", ") : "none"}</Text></Box>
        <Box flexDirection="row" flexWrap="wrap" columnGap={1}><Text color={theme.chrome}>Workspace</Text><Text color={theme.secondary}>{selectedAgent.configuration.workspace || "default"}</Text></Box>
        <Box flexDirection="row" gap={2} marginTop={1}><Box ref={chatAction}><Text color={theme.brand}>C chat</Text></Box><Box ref={settingsAction}><Text color={theme.brand}>E settings</Text></Box></Box>
      </Box> : null}
      {confirmDelete && selectedAgent ? <Box marginTop={1}><Text color={theme.warning}>Delete {selectedAgent.name} and its dependent data? (y/N)</Text></Box> : null}
    </Box> : null}
    {!fullScreen && view === "list" ? <Box marginTop={1}><Text color={theme.chrome}>↑/↓ select · C chat · E settings · D delete · Q quit</Text></Box> : null}

    {view === "settings" && selectedAgent ? <Box marginTop={1} flexDirection="column" gap={1} flexGrow={fullScreen ? 1 : 0} minHeight={0}>
      <Text bold color={theme.text}>{selectedAgent.name} settings</Text>
      <Text color={theme.muted}>1 · Personality</Text>
      <Text color={theme.chrome}>{selectedAgent.configuration.system_prompt || "Not configured"}</Text>
      <Text color={theme.muted}>2 · Workspace</Text><Text color={theme.chrome}>{selectedAgent.configuration.workspace || "Default workspace"}</Text>
      <Text color={theme.muted}>3 · Skills</Text><Text color={theme.chrome}>{selectedAgent.configuration.skills.join(", ") || "None"}</Text>
      <Text color={theme.muted}>4 · Mascot</Text><Text color={theme.chrome}>{selectedAgent.configuration.mascot || "Auto"}</Text>
      <Text color={theme.muted}>G  Use global skills, memory, connectors, etc.  <Text color={selectedAgent.configuration.use_global_context ? theme.success : theme.warning}>{selectedAgent.configuration.use_global_context ? "ON" : "OFF"}</Text></Text>
      <Text color={theme.muted}>5  Internet access / web search  <Text color={(selectedAgent.configuration.network_access ?? true) ? theme.success : theme.warning}>{(selectedAgent.configuration.network_access ?? true) ? "ON" : "OFF"}</Text></Text>
      {editing ? <Box marginTop={1} flexDirection="column"><Text color={theme.brand}>Editing {editing} · Enter saves · Esc cancels</Text><Text color={theme.text}>{editValue}▌</Text></Box> : null}
      {!fullScreen ? <Text color={theme.chrome}>1–4 edit · G global context · 5 internet · Esc back</Text> : null}
    </Box> : null}

    {view === "chat" && selectedAgent ? <Box marginTop={1} flexDirection="column" flexGrow={fullScreen ? 1 : 0} minHeight={0}>
      <Box flexDirection="row" alignItems="center" gap={1}><Text bold>{selectedAgent.name}</Text><Text color={theme.muted}>· {selectedAgent.status}</Text></Box>
      <Box marginTop={1} flexDirection="column" gap={1} flexGrow={fullScreen ? 1 : 0} minHeight={0} overflow={fullScreen ? "hidden" : "visible"}>
        {chatOffset > 0 ? <Text color={theme.chrome}>↑ Older messages · scroll down or press End for latest</Text> : null}
        {historyLoading ? <Text color={theme.muted}>Loading this teammate's history…</Text> : chatLines.length === 0 ? <Text color={theme.muted}>Send a message to start a task with this teammate.</Text> : visibleChatLines.map((line) => line.role === "user" ? (
          <Box key={line.id} flexDirection="row" marginTop={1}><Text color={theme.brand}>› </Text><Markdown content={line.content} /></Box>
        ) : <Box key={line.id} flexDirection="column" marginTop={1}>
          {line.role === "status" ? <Text color={theme.warning}>{line.content}</Text> : line.role === "tool" ? <Text color={theme.tool}>↳ {line.content}</Text> : <Markdown content={line.content} />}
        </Box>)}
      </Box>
      {activeTaskId ? <TaskActivity taskId={activeTaskId} /> : null}
      {pendingQuestion ? <Box borderStyle="single" borderColor={theme.warning} paddingX={1} marginTop={1} flexDirection="column">
        <Text color={theme.warning} bold>Teammate needs your input</Text>
        <Text>{pendingQuestion.prompt}</Text>
        {pendingQuestion.choices.length ? <Text color={theme.muted}>Options: {pendingQuestion.choices.join("  ·  ")}</Text> : null}
      </Box> : null}
      <Box marginTop={1} flexDirection="row" width="100%">
        <Text color={theme.brand}>› </Text><Text color={theme.text}>{chatInput.slice(0, chatCursor)}<Text color={theme.brand}>▏</Text>{chatInput.slice(chatCursor)}</Text>
      </Box>
      {!fullScreen ? <Text color={theme.chrome}>{pendingQuestion ? "Enter answer · Esc back" : "Enter send · Esc back"}{activeTaskId ? " · Ctrl+C cancel task" : ""}</Text> : null}
    </Box> : null}

    {error ? <Box marginTop={1}><Text color={theme.error}>{error}</Text></Box> : null}
    {fullScreen ? <>
      <Text color={theme.chrome}>{view === "list" ? "↑/↓ select · C chat · E settings · D delete · Q quit" : view === "settings" ? "1–4 edit · G global context · 5 internet · Esc back" : "Enter send · scroll history · Esc back"} · Ctrl+F exit fullscreen</Text>
    </> : null}
  </Box>;
}
