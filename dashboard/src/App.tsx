import { useEffect, useRef, useState } from "react";
import { MascotAvatar } from "./components/MascotAvatar";
import type { AgentConfiguration, AgentRecord, MemoryEntry, RuntimeEvent, ScheduleRecord, TaskRecord } from "../../shared/protocol";
import { formatChatPrompt, readBrowserChatAttachments, type ChatAttachment } from "../../shared/attachments";
import { MascotCustomizer } from "./components/MascotCustomizer";

type DashboardSection = "overview" | "teammates" | "tasks" | "approvals" | "schedules" | "memory" | "settings" | "chat";

const dashboardSections: { id: DashboardSection; label: string; icon: string }[] = [
  { id: "overview", label: "Overview", icon: "⌂" },
  { id: "teammates", label: "Teammates", icon: "◉" },
  { id: "tasks", label: "Tasks", icon: "≡" },
  { id: "approvals", label: "Approvals", icon: "✓" },
  { id: "schedules", label: "Schedules", icon: "◷" },
  { id: "memory", label: "Memory", icon: "⌘" },
  { id: "settings", label: "Settings", icon: "⚙" },
];

function DashboardShell() {
  const [section, setSection] = useState<DashboardSection>("overview");
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    const saved = Number(window.localStorage.getItem("luma.dashboard.sidebar-width"));
    return Number.isFinite(saved) && saved >= 176 && saved <= 360 ? saved : 224;
  });
  useEffect(() => { window.localStorage.setItem("luma.dashboard.sidebar-width", String(sidebarWidth)); }, [sidebarWidth]);
  const [appearance, setAppearance] = useState<"light" | "dark">(() => {
    const saved = window.localStorage.getItem("luma.dashboard.appearance");
    return saved === "light" || saved === "dark" ? saved : window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  });
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [schedules, setSchedules] = useState<ScheduleRecord[]>([]);
  const [memory, setMemory] = useState<MemoryEntry[]>([]);
  const [memoryDraft, setMemoryDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState("");
  const [lastUpdated, setLastUpdated] = useState(0);

  useEffect(() => {
    document.documentElement.dataset.theme = "luma";
    document.documentElement.dataset.appearance = appearance;
    window.localStorage.setItem("luma.dashboard.appearance", appearance);
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", appearance === "light" ? "#f4f6f8" : "#101010");
  }, [appearance]);

  const refresh = async () => {
    try {
      const [agentResponse, taskResponse, approvalResponse, scheduleResponse, memoryResponse] = await Promise.all([
        fetch("/api/agents"), fetch("/api/tasks?limit=500"), fetch("/api/approvals"), fetch("/api/schedules"),
        fetch("/api/memory?scope=user&scope_id=default"),
      ]);
      const responses = [agentResponse, taskResponse, approvalResponse, scheduleResponse, memoryResponse];
      const failed = responses.find((response) => !response.ok);
      if (failed) throw new Error(`Runtime API returned ${failed.status}`);
      const [nextAgents, nextTasks, nextApprovals, nextSchedules, nextMemory] = await Promise.all(responses.map((response) => response.json())) as [AgentRecord[], TaskRecord[], ApprovalRecord[], ScheduleRecord[], MemoryEntry[]];
      setAgents(nextAgents);
      setTasks(nextTasks);
      setApprovals(nextApprovals);
      setSchedules(nextSchedules);
      setMemory(nextMemory);
      setLastUpdated(Date.now());
      setConnected(true);
      setError("");
    } catch (reason) {
      setConnected(false);
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, []);

  async function perform(url: string, body?: unknown, method = "POST") {
    setBusy(true);
    setError("");
    try {
      const response = await fetch(url, { method, headers: { "content-type": "application/json" }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
      const result = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(result.error ?? `Request failed (${response.status})`);
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  const activeTasks = tasks.filter((task) => ["queued", "running", "waiting_for_approval", "waiting_for_input"].includes(task.status));
  const agentName = (id: string) => agents.find((agent) => agent.id === id)?.name ?? "Unknown teammate";
  const sectionTitle = section === "chat" ? "Chat" : dashboardSections.find((item) => item.id === section)?.label ?? "Overview";

  return <div className="management-shell">
    <aside className="management-sidebar" style={{ width: sidebarWidth, flexBasis: sidebarWidth }}>
      <div className="management-brand"><span className="management-brand-mark">L</span><div><strong>Luma</strong><small>CONTROL CENTER</small></div></div>
      <div className="management-nav-label">MANAGE</div>
      <nav aria-label="Luma management">
        {dashboardSections.map((item) => <button key={item.id} type="button" className={section === item.id ? "active" : ""} aria-current={section === item.id ? "page" : undefined} onClick={() => setSection(item.id)}><span>{item.icon}</span>{item.label}{item.id === "approvals" && approvals.length > 0 ? <b>{approvals.length}</b> : null}</button>)}
      </nav>
      <div className="management-sidebar-bottom"><span className={`management-status-dot ${connected ? "connected" : "disconnected"}`} />Runtime {connected ? "connected" : "disconnected"}</div>
    </aside>
    <div className="management-sidebar-resize" role="separator" aria-orientation="vertical" aria-label="Resize sidebar" aria-valuemin={176} aria-valuemax={360} aria-valuenow={sidebarWidth} tabIndex={0}
      onPointerDown={(event) => { event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.dataset.dragging = "true"; }}
      onPointerMove={(event) => { if (event.currentTarget.dataset.dragging !== "true") return; const sidebar = event.currentTarget.previousElementSibling as HTMLElement | null; if (sidebar) setSidebarWidth(Math.max(176, Math.min(360, event.clientX - sidebar.getBoundingClientRect().left))); }}
      onPointerUp={(event) => { event.currentTarget.dataset.dragging = "false"; }}
      onKeyDown={(event) => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); setSidebarWidth((width) => Math.max(176, Math.min(360, width + (event.key === "ArrowRight" ? 12 : -12)))); } }} />
    <main className="management-main">
      <header className="management-topbar"><div><span className="management-breadcrumb">LUMA</span><span className="management-separator">/</span><strong>{sectionTitle}</strong></div><div><button type="button" className="management-refresh" onClick={() => void refresh()} disabled={busy}>Refresh</button><button type="button" className="management-chat-button" onClick={() => setSection(section === "chat" ? "overview" : "chat")}>{section === "chat" ? "Close chat" : "Open chat"}</button></div></header>
      {error && <div className="management-error" role="alert">{error}</div>}
      <div className="management-content" key={section}>
        {section === "overview" && <>
          <div className="management-page-heading"><div><div className="management-eyebrow">CONTROL CENTER</div><h1>Overview</h1><p>Monitor your agents and the work Luma is handling.</p></div><span>Updated {lastUpdated ? new Date(lastUpdated).toLocaleTimeString() : "—"}</span></div>
          <div className="management-metrics"><div><span>Teammates</span><strong>{agents.length}</strong><small>Persistent agents</small></div><div><span>Active tasks</span><strong>{activeTasks.length}</strong><small>Queued or running</small></div><div><span>Approvals</span><strong>{approvals.length}</strong><small>Waiting for you</small></div><div><span>Schedules</span><strong>{schedules.filter((item) => item.enabled).length}</strong><small>Enabled routines</small></div></div>
          <div className="management-overview-grid"><section className="management-section"><header><div><h2>Teammates</h2><p>Persistent agents and their current status.</p></div><button type="button" onClick={() => setSection("teammates")}>Manage</button></header>{agents.length === 0 ? <p className="management-empty">No teammates yet.</p> : agents.map((agent) => <div className="management-agent-row" key={agent.id}><MascotAvatar seed={agent.name} mascot={agent.configuration.mascot} size={32} active={agent.status === "active"} /><div><strong>{agent.name}</strong><small>{agent.id === "agent_global" ? "Global teammate" : agent.kind}</small></div><span className={`management-pill ${agent.status}`}>{agent.status}</span></div>)}</section>
            <section className="management-section"><header><div><h2>Recent work</h2><p>Latest tasks across all teammates.</p></div><button type="button" onClick={() => setSection("tasks")}>All tasks</button></header>{tasks.slice(0, 7).map((task) => <div className="management-task-row" key={task.id}><div><strong>{task.title || task.request.slice(0, 70)}</strong><small>{agentName(task.agent_id)} · {new Date(task.updated_at * 1000).toLocaleString()}</small></div><span className={`management-pill ${task.status}`}>{task.status.replaceAll("_", " ")}</span></div>)}{tasks.length === 0 && <p className="management-empty">No tasks have been run yet.</p>}</section></div>
          {approvals.length > 0 && <section className="management-section management-approval-summary"><header><div><h2>Needs your attention</h2><p>Tool actions waiting for approval.</p></div><button type="button" onClick={() => setSection("approvals")}>Review approvals</button></header>{approvals.slice(0, 3).map((approval) => <div className="management-task-row" key={approval.id}><div><strong>{agentName(approval.agent_id)} · {approval.tool}</strong><small>{approval.arguments.slice(0, 120)}</small></div><div className="management-row-actions"><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: false })}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: true })}>Approve</button></div></div>)}</section>}
        </>}
        {section === "teammates" && <TeammatesPanel />}
        {section === "chat" && <TeammatesPanel initialAgentId="agent_global" initialTab="chat" />}
        {section === "tasks" && <><div className="management-page-heading"><div><div className="management-eyebrow">OPERATIONS</div><h1>Tasks</h1><p>Work queued and completed by your teammates.</p></div></div><section className="management-section">{tasks.map((task) => <div className="management-task-row" key={task.id}><div><strong>{task.title || task.request.slice(0, 90)}</strong><small>{agentName(task.agent_id)} · {new Date(task.created_at * 1000).toLocaleString()}</small></div><div className="management-row-actions"><span className={`management-pill ${task.status}`}>{task.status.replaceAll("_", " ")}</span>{["failed", "interrupted"].includes(task.status) && <button type="button" disabled={busy} onClick={() => void perform(`/api/tasks/${task.id}/retry`)}>Retry</button>}{["queued", "running", "waiting_for_approval", "waiting_for_input"].includes(task.status) && <button type="button" disabled={busy} onClick={() => void perform(`/api/tasks/${task.id}/cancel`)}>Cancel</button>}</div></div>)}</section></>}
        {section === "approvals" && <><div className="management-page-heading"><div><div className="management-eyebrow">OPERATIONS</div><h1>Approvals</h1><p>Review actions requested by your teammates.</p></div></div><section className="management-section">{approvals.map((approval) => <div className="management-approval-row" key={approval.id}><div><strong>{agentName(approval.agent_id)} wants to run {approval.tool}</strong><pre>{approval.arguments}</pre></div><div className="management-row-actions"><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: false })}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: true })}>Approve</button></div></div>)}{approvals.length === 0 && <p className="management-empty">Nothing is waiting for approval.</p>}</section></>}
        {section === "schedules" && <><div className="management-page-heading"><div><div className="management-eyebrow">AUTOMATION</div><h1>Schedules</h1><p>Recurring work configured for your teammates.</p></div></div><section className="management-section">{schedules.map((schedule) => <div className="management-task-row" key={schedule.id}><div><strong>{schedule.title}</strong><small>{agentName(schedule.agent_id)} · {schedule.enabled ? `Next ${new Date(schedule.next_run_at * 1000).toLocaleString()}` : "Disabled"}</small></div><div className="management-row-actions"><span className={`management-pill ${schedule.enabled ? "active" : "disabled"}`}>{schedule.enabled ? "enabled" : "disabled"}</span><button type="button" disabled={busy} onClick={() => void perform(`/api/schedules/${schedule.id}`, undefined, "DELETE")}>Remove</button></div></div>)}{schedules.length === 0 && <p className="management-empty">No schedules configured.</p>}</section></>}
        {section === "memory" && <><div className="management-page-heading"><div><div className="management-eyebrow">CONTEXT</div><h1>Global memory</h1><p>Facts shared with teammates that use global context.</p></div></div><section className="management-section management-memory-form"><textarea value={memoryDraft} onChange={(event) => setMemoryDraft(event.target.value)} placeholder="Add a fact or preference to global memory…" rows={3} /><button type="button" disabled={busy || !memoryDraft.trim()} onClick={async () => { await perform("/api/memory", { scope: "user", scope_id: "default", content: memoryDraft.trim(), source: "dashboard" }); setMemoryDraft(""); }}>Save memory</button></section><section className="management-section">{memory.map((entry) => <div className="management-memory-row" key={entry.id}><p>{entry.content}</p><button type="button" disabled={busy} onClick={() => void perform(`/api/memory/${entry.id}`, undefined, "DELETE")}>Delete</button></div>)}{memory.length === 0 && <p className="management-empty">Global memory is empty.</p>}</section></>}
        {section === "settings" && <><div className="management-page-heading"><div><div className="management-eyebrow">CONFIGURATION</div><h1>Settings</h1><p>Runtime, appearance and workspace information.</p></div></div><section className="management-section management-settings"><div><span>Appearance</span><div className="management-appearance-switch"><button type="button" aria-pressed={appearance === "light"} className={appearance === "light" ? "active" : ""} onClick={() => setAppearance("light")}>Light</button><button type="button" aria-pressed={appearance === "dark"} className={appearance === "dark" ? "active" : ""} onClick={() => setAppearance("dark")}>Dark</button></div></div><div><span>Runtime</span><strong>{connected ? "Connected" : "Disconnected"}</strong></div><div><span>Workspace</span><strong>Managed by the Luma core process</strong></div><div><span>Agent persistence</span><strong>SQLite-backed task and teammate records</strong></div><div><span>Safety</span><strong>Tool permissions and approvals are enforced by the runtime</strong></div></section></>}
      </div>
    </main>
  </div>;
}

type ApprovalRecord = { id: string; agent_id: string; task_id: string; tool: string; arguments: string };

function TeammateTaskQuestion({ task, busy, onAnswer }: { task: TaskRecord; busy: boolean; onAnswer: (taskId: string, questionId: string, answer: string) => void }) {
  const [answer, setAnswer] = useState("");
  if (task.status !== "waiting_for_input" || !task.checkpoint || typeof task.checkpoint !== "object") return null;
  const question = task.checkpoint as { question_id?: unknown; prompt?: unknown; choices?: unknown };
  if (typeof question.question_id !== "string" || typeof question.prompt !== "string" || !Array.isArray(question.choices) || !question.choices.every((choice) => typeof choice === "string")) return null;
  const submit = (value: string) => { if (!busy && value.trim()) onAnswer(task.id, question.question_id as string, value.trim()); };
  return <div className="teammate-task-question"><strong>Luma needs your input</strong><p>{question.prompt}</p>{question.choices.length > 0 && <div>{question.choices.map((choice) => <button key={choice} type="button" disabled={busy} onClick={() => submit(choice)}>{choice}</button>)}</div>}<form onSubmit={(event) => { event.preventDefault(); submit(answer); }}><input value={answer} onChange={(event) => setAnswer(event.target.value)} maxLength={8000} placeholder="Type your answer…" /><button type="submit" disabled={busy || !answer.trim()}>Answer</button></form></div>;
}

function TeammatesPanel({ initialAgentId, initialTab = "chat" }: { initialAgentId?: string; initialTab?: "chat" | "settings" }) {
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [schedules, setSchedules] = useState<ScheduleRecord[]>([]);
  const [memoryEntries, setMemoryEntries] = useState<MemoryEntry[]>([]);
  const [runtimeEvents, setRuntimeEvents] = useState<RuntimeEvent[]>([]);
  const [selected, setSelected] = useState(initialAgentId ?? "");
  const [createOpen, setCreateOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [personality, setPersonality] = useState("");
  const [initialMemory, setInitialMemory] = useState("");
  const [agentWorkspace, setAgentWorkspace] = useState("");
  const [agentMascot, setAgentMascot] = useState("auto");
  const [agentUseGlobalContext, setAgentUseGlobalContext] = useState(true);
  const [editPersonality, setEditPersonality] = useState("");
  const [editSkills, setEditSkills] = useState("");
  const [editMascot, setEditMascot] = useState("auto");
  const [prompt, setPrompt] = useState("");
  const [chatPrompt, setChatPrompt] = useState("");
  const [chatAttachments, setChatAttachments] = useState<ChatAttachment[]>([]);
  const [errorDetailsExpanded, setErrorDetailsExpanded] = useState(false);
  const chatFileInput = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState<"chat" | "settings">(initialTab);
  const [memory, setMemory] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refreshing = useRef(false);

  const refresh = async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const [agentResponse, approvalResponse, scheduleResponse] = await Promise.all([
        fetch("/api/agents"), fetch("/api/approvals"), fetch("/api/schedules"),
      ]);
      for (const response of [agentResponse, approvalResponse, scheduleResponse]) if (!response.ok) throw new Error(`Runtime API returned ${response.status}`);
      const [nextAgents, nextApprovals, nextSchedules] = await Promise.all([agentResponse.json(), approvalResponse.json(), scheduleResponse.json()]) as [AgentRecord[], ApprovalRecord[], ScheduleRecord[]];
      const preferred = initialAgentId && nextAgents.some((agent) => agent.id === initialAgentId) ? initialAgentId : null;
      const nextSelected = selected && nextAgents.some((agent) => agent.id === selected) ? selected : preferred ?? nextAgents.find((agent) => agent.id === "agent_global")?.id ?? nextAgents.find((agent) => agent.kind === "teammates")?.id ?? nextAgents[0]?.id ?? "";
      const taskResponse = await fetch(`/api/tasks?agent_id=${encodeURIComponent(nextSelected)}&limit=500`);
      if (!taskResponse.ok) throw new Error(`Runtime API returned ${taskResponse.status}`);
      const nextTasks = await taskResponse.json() as TaskRecord[];
      setAgents(nextAgents); setTasks(nextTasks); setApprovals(nextApprovals); setSchedules(nextSchedules);
      setSelected(nextSelected);
      setError(null);
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { refreshing.current = false; }
  };

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);
    return () => window.clearInterval(timer);
  }, [selected]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "o") return;
      event.preventDefault();
      setErrorDetailsExpanded((expanded) => !expanded);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (!selected) { setMemoryEntries([]); setRuntimeEvents([]); return; }
    const currentAgent = agents.find((agent) => agent.id === selected);
    if (currentAgent) {
      setEditPersonality(currentAgent.configuration.system_prompt);
      setEditSkills(currentAgent.configuration.skills.join(", "));
      setEditMascot(currentAgent.configuration.mascot || "auto");
    }
    let disposed = false;
    const loadDetails = async () => {
      try {
        const params = new URLSearchParams({ scope: "agent", scope_id: selected });
        const [memoryResponse, eventsResponse] = await Promise.all([
          fetch(`/api/memory?${params}`), fetch(`/api/events?agent_id=${encodeURIComponent(selected)}&limit=1000`),
        ]);
        if (!memoryResponse.ok || !eventsResponse.ok) throw new Error("Could not load teammate memory and activity.");
        const [entries, events] = await Promise.all([memoryResponse.json(), eventsResponse.json()]) as [MemoryEntry[], RuntimeEvent[]];
        if (!disposed) { setMemoryEntries(entries); setRuntimeEvents(events); }
      } catch (reason) { if (!disposed) setError(reason instanceof Error ? reason.message : String(reason)); }
    };
    void loadDetails();
    const timer = window.setInterval(() => void loadDetails(), 3000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [selected]);

  const activeAgent = agents.find((agent) => agent.id === selected);
  const agentTasks = tasks.filter((task) => task.agent_id === selected && !task.schedule_id);
  const taskAssistantText = (taskId: string) => runtimeEvents
    .filter((event) => event.task_id === taskId && event.event_type === "task.agent_event")
    .map((event) => event.payload as { type?: string; data?: unknown })
    .filter((event) => event.type === "TextDelta" && typeof event.data === "string")
    .map((event) => event.data as string).join("");

  async function perform(url: string, body?: unknown, method = "POST") {
    setBusy(true); setError(null);
    try {
      const response = await fetch(url, { method, headers: { "content-type": "application/json" }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
      const result = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(result.error ?? `Request failed (${response.status})`);
      await refresh();
      return result;
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  const answerTaskQuestion = (taskId: string, questionId: string, answer: string) => void perform(`/api/tasks/${taskId}/answer`, { question_id: questionId, answer });

  async function attachTeammateFiles(files: FileList | File[]) {
    try {
      const selectedFiles = await readBrowserChatAttachments(files);
      const next = [...chatAttachments, ...selectedFiles];
      const totalBytes = next.reduce((total, item) => total + new TextEncoder().encode(item.content).byteLength, 0);
      if (next.length > 8 || totalBytes > 4 * 1024 * 1024) throw new Error("A teammate message can include up to 8 text files and 4 MiB of content.");
      setChatAttachments(next);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return <section className="teammates-page" aria-labelledby="teammates-heading">
      <header className="teammates-dialog-header"><div className="teammates-title-wrap"><MascotAvatar seed="luma-crew" size={42} /><div><span className="teammates-wordmark-luma teammates-brand-kicker">LUMA</span><h2 id="teammates-heading" className="teammates-wordmark-name">teammates</h2><p>Persistent agents, tasks and approvals</p></div></div><div className="teammates-counts">{agents.length} agents · {tasks.filter((task) => ["queued", "running", "waiting_for_approval", "waiting_for_input"].includes(task.status)).length} active</div></header>
      {error && <div className="teammates-error" role="alert">{error}</div>}
      <div className="teammates-body">
        <aside className="teammates-list"><div className="teammates-list-heading"><div className="teammates-section-title">AGENTS</div><button type="button" disabled={busy} onClick={() => setCreateOpen(true)}>+ Create</button></div>{agents.map((agent) => <div className={`teammate-row-wrap ${selected === agent.id ? "selected" : ""}`} key={agent.id}><button type="button" onClick={() => setSelected(agent.id)} className="teammate-row"><MascotAvatar seed={agent.name} mascot={agent.configuration.mascot} size={30} active={agent.status === "active"} /><span><strong>{agent.name}</strong><small>{agent.id === "agent_global" ? "Global · protected" : agent.status}</small></span></button></div>)}</aside>
        <main className="teammates-detail">{!activeAgent ? <div className="teammates-empty"><MascotAvatar seed="luma-welcome" size={76} /><p>Create a teammate and give it a name, a workspace and work to own.</p></div> : <>
          <div className="teammates-agent-heading"><div className="teammates-agent-identity"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={42} active={activeAgent.status === "active"} /><div><h3>{activeAgent.name}</h3><p>{activeAgent.id === "agent_global" ? "Global teammate · shared defaults" : `${activeAgent.status} · ${activeAgent.kind}`} · {agentTasks.length} saved tasks</p></div></div><div className="teammates-agent-actions"><button type="button" className={activeTab === "chat" ? "active" : ""} onClick={() => setActiveTab("chat")}>Chat</button><button type="button" className={activeTab === "settings" ? "active" : ""} onClick={() => setActiveTab("settings")}>Settings</button>{activeAgent.id !== "agent_global" && <button type="button" disabled={busy} onClick={() => { if (window.confirm(`Delete ${activeAgent.name}? Cancel active tasks first.`)) void perform(`/api/agents/${activeAgent.id}`, undefined, "DELETE"); }}>Delete</button>}</div></div>
          {activeTab === "chat" ? <div className="teammates-chat"><div className="teammates-chat-messages">{agentTasks.length === 0 ? <div className="teammates-empty"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={76} active /><h3>Chat with {activeAgent.name}</h3><p>Ask a question, hand over a task, or just start a conversation. This teammate's history stays separate from every other teammate.</p></div> : agentTasks.slice().reverse().map((task) => { const answer = taskAssistantText(task.id); const failed = ["failed", "interrupted"].includes(task.status); const detail = (task.error ?? "").replace(/^ReAct failed:\s*/i, "").replace(/^Failed to run tool\s*/i, ""); return <article className="teammates-chat-turn" key={task.id}><div className="teammates-chat-user"><small>YOU</small><p>{task.request}</p></div><div className="teammates-chat-reply"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={28} active={task.status === "running"} /><div className="teammates-chat-reply-content"><small>{activeAgent.name} {task.status === "running" ? "· working" : task.status === "queued" ? "· queued" : ""}</small><div className="teammates-chat-reply-bubble"><p>{answer || (failed ? "Failed to run tool · Cmd+O for details" : task.status === "completed" ? "No text response was recorded." : task.status === "waiting_for_input" ? "Waiting for your answer…" : "Thinking…")}</p><TeammateTaskQuestion task={task} busy={busy} onAnswer={answerTaskQuestion} />{failed && errorDetailsExpanded && detail && <pre className="teammates-error-details">{detail}</pre>}{failed && <button type="button" onClick={() => void perform(`/api/tasks/${task.id}/retry`)}>Retry</button>}</div></div></div></article>; })}{approvals.filter((approval) => approval.agent_id === activeAgent.id).map((approval) => <div className="teammate-approval" key={approval.id}><strong>{activeAgent.name} needs approval · {approval.tool}</strong><pre>{approval.arguments}</pre><div><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: false })}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: true })}>Approve</button></div></div>)}</div><form className="teammates-chat-composer" onSubmit={(event) => { event.preventDefault(); const text = chatPrompt.trim() || (chatAttachments.length ? "Please inspect the attached files." : ""); if (!text || busy || activeAgent.status === "stopped" || activeAgent.status === "paused") return; void (async () => { await perform("/api/tasks", { agent_id: activeAgent.id, request: formatChatPrompt(text, chatAttachments), workspace: activeAgent.configuration.workspace ?? undefined }); setChatPrompt(""); setChatAttachments([]); })(); }}>
            <input ref={chatFileInput} type="file" multiple hidden onChange={(event) => { if (event.currentTarget.files?.length) void attachTeammateFiles(event.currentTarget.files); event.currentTarget.value = ""; }} />
            {chatAttachments.length > 0 && <div className="chat-attachment-list">{chatAttachments.map((attachment, index) => <span className="chat-attachment-chip" key={`${attachment.name}-${index}`}><span>{attachment.name}</span><button type="button" aria-label={`Remove ${attachment.name}`} onClick={() => setChatAttachments((items) => items.filter((_, itemIndex) => itemIndex !== index))}>×</button></span>)}</div>}
            <div className="teammates-chat-input-row"><button type="button" className="chat-attach-button" aria-label="Attach files" title="Attach text files" disabled={busy || activeAgent.status === "stopped" || activeAgent.status === "paused"} onClick={() => chatFileInput.current?.click()}>+</button><textarea value={chatPrompt} onChange={(event) => setChatPrompt(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} placeholder={`Message ${activeAgent.name}…`} rows={2} /></div>
            <div><span>Enter to send · Shift + Enter for a new line</span><button type="submit" disabled={busy || (!chatPrompt.trim() && chatAttachments.length === 0) || activeAgent.status === "stopped" || activeAgent.status === "paused"}>{busy ? "Sending…" : "Send"}</button></div></form></div> : <>
          <section className="teammates-personality"><div className="teammates-section-title">IDENTITY & PERSONALITY</div><MascotCustomizer value={editMascot} seed={activeAgent.name} onChange={setEditMascot} /><label>Instructions<textarea value={editPersonality} onChange={(event) => setEditPersonality(event.target.value)} rows={4} /></label>{!activeAgent.configuration.use_global_context && <label>Attached skills<input value={editSkills} onChange={(event) => setEditSkills(event.target.value)} placeholder="research, code-review" /></label>}{activeAgent.id !== "agent_global" && <label className="teammates-global-check"><input type="checkbox" checked={activeAgent.configuration.use_global_context} onChange={(event) => { setEditSkills(""); void perform(`/api/agents/${activeAgent.id}`, { configuration: { ...activeAgent.configuration, use_global_context: event.target.checked, ...(event.target.checked ? { skills: [] } : {}) } }, "PUT"); }} /><span>Use the global skills, memory, connectors, etc.</span></label>}<button type="button" disabled={busy} onClick={() => void perform(`/api/agents/${activeAgent.id}`, { configuration: { ...activeAgent.configuration, mascot: editMascot, system_prompt: editPersonality, skills: activeAgent.configuration.use_global_context ? [] : [...new Set(editSkills.split(",").map((skill) => skill.trim()).filter(Boolean))] } }, "PUT")}>Save changes</button></section>
          <section className="teammates-agent-actions"><button type="button" disabled={busy || activeAgent.status === "stopped" || activeAgent.status === "active" || activeAgent.status === "waiting"} onClick={() => void perform(`/api/agents/${activeAgent.id}/status`, { status: "paused" })}>Pause</button><button type="button" disabled={busy} onClick={() => void perform(`/api/agents/${activeAgent.id}/status`, { status: "idle" })}>Resume</button><button type="button" disabled={busy} onClick={() => { if (window.confirm(`Stop ${activeAgent.name}?`)) void perform(`/api/agents/${activeAgent.id}/status`, { status: "stopped" }); }}>Stop</button></section>
          <section><div className="teammates-section-title">ACTIVITY</div>{runtimeEvents.length === 0 ? <p className="teammates-empty-inline">No recent activity.</p> : <div className="teammates-activity">{runtimeEvents.slice(-8).reverse().map((event) => <div key={event.event_id}><span>{event.event_type.replaceAll(".", " · ")}</span><time>{new Date(event.created_at * 1000).toLocaleTimeString()}</time></div>)}</div>}</section>
          <section><div className="teammates-section-title">APPROVALS</div>{approvals.filter((approval) => approval.agent_id === activeAgent.id).length === 0 ? <p className="teammates-empty-inline">No pending approvals.</p> : approvals.filter((approval) => approval.agent_id === activeAgent.id).map((approval) => <div className="teammate-approval" key={approval.id}><strong>{approval.tool}</strong><pre>{approval.arguments}</pre><div><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: false })}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: true })}>Approve</button></div></div>)}</section>
          <section className="teammates-memory"><div className="teammates-section-title">AGENT MEMORY</div><textarea value={memory} onChange={(event) => setMemory(event.target.value)} rows={2} placeholder="A fact or preference this teammate should remember…" /><button type="button" disabled={busy || !memory.trim()} onClick={async () => { await perform("/api/memory", { scope: "agent", scope_id: activeAgent.id, content: memory.trim(), source: "dashboard" }); setMemory(""); }}>Save memory</button>{memoryEntries.map((entry) => <div className="teammate-memory-entry" key={entry.id}><p>{entry.content}</p><button type="button" disabled={busy} onClick={() => void perform(`/api/memory/${entry.id}`, undefined, "DELETE")}>Delete</button></div>)}</section>
          <section className="teammates-schedules"><div className="teammates-section-title">RECURRING WORK</div><div className="teammates-schedule-create"><button type="button" disabled={busy || !prompt.trim()} onClick={async () => { await perform("/api/schedules", { agent_id: activeAgent.id, title: "Daily task", prompt: prompt.trim(), every_seconds: 86400 }); setPrompt(""); }}>Schedule task prompt daily</button><span>Uses the task prompt above</span></div>{schedules.filter((schedule) => schedule.agent_id === activeAgent.id).map((schedule) => <div className="teammate-task" key={schedule.id}><span className={`teammate-dot ${schedule.enabled ? "active" : "stopped"}`} /><div><strong>{schedule.title}</strong><small>{schedule.enabled ? `Next ${new Date(schedule.next_run_at * 1000).toLocaleString()}` : "Disabled"}</small></div><button type="button" disabled={busy} onClick={() => void perform(`/api/schedules/${schedule.id}`, undefined, "DELETE")}>Remove</button></div>)}</section>
        </>}</>}</main>
      </div>
      <footer className="teammates-footer">Tasks are stored by the core runtime and continue independently of this page.<button type="button" onClick={() => void refresh()}>Refresh</button></footer>
      {createOpen && <div className="teammates-create-overlay" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) setCreateOpen(false); }}><form className="teammates-create-dialog" role="dialog" aria-modal="true" aria-labelledby="create-teammate-heading" onSubmit={(event) => { event.preventDefault(); void (async () => {
        const workspace = agentWorkspace.trim() || undefined;
        const configuration: AgentConfiguration = {
          system_prompt: personality.trim(), skills: [], mascot: agentMascot, use_global_context: agentUseGlobalContext,
          provider: null, model: null, endpoint: null, workspace: workspace ?? null, allowed_workspaces: workspace ? [workspace] : [], enabled_tools: [],
          permissions: { allowed_tools: ["read_file", "list_directory", "search_files", "git_status", "git_diff", "find_symbols", "find_references", "git_log", "git_show", "git_blame", "git_branches", "file_metadata", "find_todos", "count_lines", "inspect_project", "list_skills", "load_skill"], denied_tools: [], auto_approve_read_only: true, require_approval_for_mutations: true, allow_network: false },
          max_concurrent_tasks: 1, recover_interrupted_tasks: false,
        };
        const created = await perform("/api/agents", { name: name.trim(), description: description.trim() || null, kind: "teammates", configuration }) as AgentRecord | undefined;
        if (created?.id) {
          setSelected(created.id); setCreateOpen(false); setName(""); setDescription(""); setPersonality(""); setInitialMemory(""); setAgentWorkspace(""); setAgentMascot("auto"); setAgentUseGlobalContext(true); setActiveTab("chat");
          if (initialMemory.trim()) await perform("/api/memory", { scope: "agent", scope_id: created.id, content: initialMemory.trim(), source: "dashboard" });
        }
      })(); }}>
        <header><div><h3 id="create-teammate-heading">Create teammate</h3><p>Define its identity, personality and capabilities.</p></div><button type="button" onClick={() => setCreateOpen(false)}>Close</button></header>
        <div className="teammates-create-fields">
          <label>Name<input autoFocus required maxLength={120} value={name} onChange={(event) => setName(event.target.value)} placeholder="e.g. Researcher" /></label>
          <label>Description<input value={description} onChange={(event) => setDescription(event.target.value)} placeholder="What should this teammate focus on?" /></label>
          <div><strong>Design your mascot</strong><MascotCustomizer value={agentMascot} seed={name} onChange={setAgentMascot} /></div>
          <label>Custom personality (optional)<textarea value={personality} onChange={(event) => setPersonality(event.target.value)} rows={5} placeholder="Leave blank to use Luma's default behavior." /></label>
          <label className="teammates-global-check"><input type="checkbox" checked={agentUseGlobalContext} onChange={(event) => setAgentUseGlobalContext(event.target.checked)} /><span>Use the global skills, memory, connectors, etc.</span></label>
          <label>Initial memory<textarea value={initialMemory} onChange={(event) => setInitialMemory(event.target.value)} rows={2} placeholder="Facts or preferences to remember…" /></label>
          <label>Workspace<input value={agentWorkspace} onChange={(event) => setAgentWorkspace(event.target.value)} placeholder="Current dashboard workspace" /></label>
          <p>Network access starts disabled. Mutating tools require approval. Selected skills are validated and loaded when tasks start.</p>
        </div>
        <footer><button type="button" onClick={() => setCreateOpen(false)}>Cancel</button><button type="submit" disabled={busy || !name.trim()}>{busy ? "Creating…" : "Create teammate"}</button></footer>
      </form></div>}
    </section>;
}

export default function App() {
  return <DashboardShell />;
}
