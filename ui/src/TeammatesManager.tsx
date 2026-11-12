import { useEffect, useRef, useState } from "react";
import { Box, Text, useApp, useInput, useStdout } from "ink";
import { randomUUID } from "node:crypto";
import { LumaClient } from "./client";
import { startDesktopServer, stopDesktopServer } from "./server";
import type { AgentConfiguration, AgentRecord, ClientMessage, RuntimeEvent, ServerEvent, TaskRecord } from "./protocol";
import { Mascot } from "./components/Mascot";
import { Markdown } from "./components/Markdown";
import { theme } from "./theme";
import { useMotionFrame } from "./hooks/useMotionFrame";
import { usePulseColor } from "./hooks/usePulseColor";

type RequestEvent = Extract<ServerEvent, { data: { request_id: string } }>;
type View = "list" | "settings" | "chat";
type SettingField = "personality" | "workspace" | "skills" | "mascot";
type ChatLine = { id: string; role: "user" | "assistant" | "status" | "tool"; content: string };

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
  const [chatLines, setChatLines] = useState<ChatLine[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const sequence = useRef(0);
  const polling = useRef(false);
  const motionFrame = useMotionFrame(Boolean(activeTaskId));
  const activitySpinner = ["◐", "◓", "◑", "◒"][motionFrame % 4];
  const activeBrand = usePulseColor(Boolean(activeTaskId), theme.brand, "#B4D3FF");

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
    }).catch((reason) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)); });
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
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
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
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
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
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
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
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  async function sendChat() {
    const agent = selectedAgent;
    const prompt = chatInput.trim();
    if (!agent || !prompt || busy || activeTaskId) return;
    setChatInput("");
    setChatLines((lines) => [...lines, { id: randomUUID(), role: "user", content: prompt }, { id: `pending-${Date.now()}`, role: "assistant", content: "Thinking…" }]);
    setBusy(true); setError("");
    try {
      const requestId = randomUUID();
      const response = await request(client, {
        type: "task_create",
        data: { request_id: requestId, agent_id: agent.id, request: prompt, title: null, workspace: agent.configuration.workspace || process.cwd(), recover_on_restart: false },
      }, "task_saved", requestId);
      setActiveTaskId(response.data.task.id);
    } catch (reason) {
      setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "assistant", content: `Could not send message: ${reason instanceof Error ? reason.message : String(reason)}` }]);
    } finally { setBusy(false); }
  }

  useEffect(() => {
    if (view !== "chat" || !selectedAgent) return;
    let active = true;
    const agentId = selectedAgent.id;
    setHistoryLoading(true);
    setChatLines([]);
    setActiveTaskId(null);
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
          }
        }
        if (task.error && !["completed", "cancelled"].includes(task.status)) {
          restored.push({ id: `${task.id}-error`, role: "status", content: `${task.status.replaceAll("_", " ")}: ${task.error}` });
        }
      }
      setChatLines(restored);
      const activeTask = [...tasks].reverse().find((task) => ["queued", "running", "waiting_for_approval"].includes(task.status));
      setActiveTaskId(activeTask?.id ?? null);
    })().catch((reason) => {
      if (active) setError(reason instanceof Error ? reason.message : String(reason));
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
            }
          }
          if (["task.completed", "task.failed", "task.cancelled", "task.interrupted"].includes(event.event_type)) {
            setChatLines((lines) => lines.filter((line) => line.content !== "Thinking…"));
            if (event.event_type !== "task.completed") {
              const task = event.payload as { error?: string | null };
              if (task.error) setChatLines((lines) => [...lines.filter((line) => line.content !== "Thinking…"), { id: randomUUID(), role: "assistant", content: `**${event.event_type.replace("task.", "").replaceAll("_", " ")}:** ${task.error}` }]);
            }
            setActiveTaskId(null);
          }
        }
      } catch (reason) {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
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
      if (key.escape) { setView("list"); return; }
      if (key.return) { void sendChat(); return; }
      if (key.backspace || key.delete) setChatInput((value) => value.slice(0, -1));
      else if (input && !key.ctrl && !key.meta) setChatInput((value) => value + input);
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
    else if (input.toLowerCase() === "r") void refresh().catch((reason) => setError(String(reason)));
    else if (input.toLowerCase() === "e" && selectedAgent) setView("settings");
    else if (input.toLowerCase() === "c" && selectedAgent) { setChatLines([]); setView("chat"); }
  });

  const rule = "─".repeat(Math.max(1, Math.min((stdout.columns || 72) - 4, 96)));
  return <Box flexDirection="column" paddingX={1}>
    <Box marginBottom={1} justifyContent="space-between">
      <Box flexDirection="row" alignItems="center" gap={1}>
        <Mascot seed="luma-crew" compact />
        <Text bold color={activeBrand}>LUMA</Text><Text color={theme.chrome}>/</Text><Text bold>TEAMMATES</Text>
        {view !== "list" ? <><Text color={theme.chrome}>/</Text><Text color={theme.muted}>{view.toUpperCase()}</Text></> : null}
      </Box>
      <Text color={connected ? theme.success : theme.warning}>{connected ? "● connected" : "○ connecting"}</Text>
    </Box>
    <Text color={theme.chrome}>{rule}</Text>

    {view === "list" ? <>
      <Box marginTop={1} flexDirection="column">
        <Box flexDirection="row" justifyContent="space-between"><Text color={theme.muted} bold>YOUR TEAMMATES</Text><Text color={theme.chrome}>{agents.length} total</Text></Box>
        {agents.length === 0 ? <Text color={theme.muted}>No teammates yet. Create one with `luma teammates create &lt;name&gt;`.</Text> : agents.map((item, index) => <Box key={item.id} flexDirection="column" marginTop={1}>
          <Box flexDirection="row" gap={1}><Text color={index === selected ? theme.brand : theme.chrome}>{index === selected ? "›" : " "}</Text><Mascot seed={item.name} compact /><Text color={index === selected ? "white" : theme.muted} bold={index === selected}>{item.name}</Text><Text color={theme.chrome}>· {item.status}</Text></Box>
          {index === selected ? <Text color={theme.chrome}>  {item.kind} · {item.id === "agent_global" ? "protected global teammate" : "persistent agent"}</Text> : null}
        </Box>)}
      </Box>
      {selectedAgent && <Box marginTop={1} flexDirection="column"><Text color={theme.chrome}>{rule}</Text><Box flexDirection="row" alignItems="center" gap={2}><Mascot seed={selectedAgent.name} /><Box flexDirection="column"><Text bold>{selectedAgent.name}</Text><Text color={theme.muted}>{selectedAgent.status} · {selectedAgent.kind}</Text></Box></Box><Text color={theme.muted}>{selectedAgent.description || "No description"}</Text><Text color={theme.muted}>Skills  {selectedAgent.configuration.skills.length ? selectedAgent.configuration.skills.join(", ") : "none attached"}</Text><Text color={theme.muted}>Workspace  {selectedAgent.configuration.workspace || "default"}</Text></Box>}
      {confirmDelete && selectedAgent ? <Box marginTop={1}><Text color={theme.warning}>Delete {selectedAgent.name} and its dependent data? (y/N)</Text></Box> : null}
      <Box marginTop={1}><Text color={theme.chrome}>{rule}</Text><Text color={theme.muted}>↑/↓ select  ·  C chat  ·  E settings  ·  D delete  ·  R refresh  ·  Q quit</Text></Box>
    </> : null}

    {view === "settings" && selectedAgent ? <Box marginTop={1} flexDirection="column" gap={1}>
      <Text bold color="white">{selectedAgent.name} settings</Text>
      <Text color={theme.muted}>1  Personality / system instructions</Text>
      <Text color={theme.chrome}>{selectedAgent.configuration.system_prompt || "Not configured"}</Text>
      <Text color={theme.muted}>2  Workspace</Text><Text color={theme.chrome}>{selectedAgent.configuration.workspace || "Default workspace"}</Text>
      <Text color={theme.muted}>3  Skills</Text><Text color={theme.chrome}>{selectedAgent.configuration.skills.join(", ") || "No skills attached"}</Text>
      <Text color={theme.muted}>4  Mascot</Text><Text color={theme.chrome}>{selectedAgent.configuration.mascot || "auto"}</Text>
      <Text color={theme.muted}>G  Use global skills, memory, connectors, etc.  <Text color={selectedAgent.configuration.use_global_context ? theme.success : theme.warning}>{selectedAgent.configuration.use_global_context ? "ON" : "OFF"}</Text></Text>
      <Text color={theme.muted}>5  Internet access / web search  <Text color={(selectedAgent.configuration.network_access ?? true) ? theme.success : theme.warning}>{(selectedAgent.configuration.network_access ?? true) ? "ON" : "OFF"}</Text></Text>
      {editing ? <Box marginTop={1} flexDirection="column"><Text color={theme.brand}>Editing {editing} · Enter saves · Esc cancels</Text><Text color="white">{editValue}▌</Text></Box> : null}
      <Text color={theme.chrome}>{rule}</Text><Text color={theme.muted}>1–4 edit a setting  ·  G toggle global context  ·  5 toggle internet  ·  Esc back</Text>
    </Box> : null}

    {view === "chat" && selectedAgent ? <Box marginTop={1} flexDirection="column">
      <Box flexDirection="row" alignItems="center" gap={1}><Mascot seed={selectedAgent.name} compact /><Text bold>Chat with {selectedAgent.name}</Text><Text color={theme.muted}>· {selectedAgent.status}</Text></Box>
      <Box marginTop={1} flexDirection="column" gap={1}>
        {historyLoading ? <Text color={theme.muted}>Loading this teammate's history…</Text> : chatLines.length === 0 ? <Text color={theme.muted}>Send a message to start a task with this teammate.</Text> : chatLines.map((line) => line.role === "user" ? (
          <Box key={line.id} flexDirection="row" justifyContent="flex-end" marginTop={1}>
            <Box flexDirection="column" borderStyle="round" borderColor={theme.brand} paddingX={1}>
              <Text color={theme.brand} bold>YOU</Text>
              <Markdown content={line.content} />
            </Box>
          </Box>
        ) : <Box key={line.id} flexDirection="column" marginTop={1}>
          <Text color={line.role === "status" ? theme.warning : line.role === "tool" ? theme.tool : theme.assistant ?? "white"} bold>{line.role === "status" ? "STATUS" : line.role === "tool" ? "TOOL" : selectedAgent.name.toUpperCase()}</Text>
          <Box marginLeft={2}>{line.role === "tool" ? <Text color={theme.tool}>{line.content}</Text> : <Markdown content={line.content} />}</Box>
        </Box>)}
      </Box>
      {activeTaskId ? <Text color={theme.brand}>{activitySpinner} Task running · {activeTaskId.slice(0, 8)} · wait for the reply before sending another message.</Text> : null}
      <Box marginTop={1} borderStyle="round" borderColor={theme.brand} paddingX={1} flexDirection="column" width="100%">
        <Text color={theme.brand} bold>MESSAGE</Text>
        <Text color="white">{chatInput}▌</Text>
      </Box>
      <Text color={theme.muted}>Enter send  ·  Esc back</Text>
    </Box> : null}

    {error ? <Box marginTop={1}><Text color={theme.error}>{error}</Text></Box> : null}
  </Box>;
}
