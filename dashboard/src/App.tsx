import { useEffect, useRef, useState, type CSSProperties } from "react";
import { MascotAvatar } from "./components/MascotAvatar";
import type { AgentConfiguration, AgentRecord, MemoryEntry, RuntimeEvent, ScheduleRecord, TaskRecord } from "../../shared/protocol";
import { formatChatPrompt, readBrowserChatAttachments, type ChatAttachment } from "../../shared/attachments";
import { MascotCustomizer } from "./components/MascotCustomizer";

type Mode = "agent" | "ide" | "changes";
function ModePicker({ teammatesOpen, onSelectCode, onOpenTeammates }: { teammatesOpen: boolean; onSelectCode: () => void; onOpenTeammates: () => void }) {
  return (
    <nav
      className="mode-picker"
      aria-label="Teammate"
      style={{ "--mode-index": teammatesOpen ? 1 : 0, "--mode-count": 2 } as CSSProperties}
    >
      <span className="mode-picker-indicator" aria-hidden="true" />
      <button type="button" className="mode-picker-button" data-section="code" aria-label="Code" aria-pressed={!teammatesOpen} aria-current={!teammatesOpen ? "page" : undefined} title="Code" onClick={onSelectCode}>
        <span className="mode-code-icon">&lt;/&gt;</span>
      </button>
      <button type="button" className="mode-picker-button" data-section="teammates" aria-label="Teammates" aria-pressed={teammatesOpen} aria-current={teammatesOpen ? "page" : undefined} title="Teammates" onClick={onOpenTeammates}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M18 20a6 6 0 0 0-12 0" /><circle cx="12" cy="8" r="4" />
        </svg>
      </button>
    </nav>
  );
}

function DashboardShell({ teammatesOpen, onSelectCode, onOpenTeammates }: { teammatesOpen: boolean; onSelectCode: () => void; onOpenTeammates: () => void }) {
  return (
    <>
      <div className={`app ${teammatesOpen ? "teammates-mode" : ""}`}>
        <aside className="sidebar">
          <div className="brand">
            <span className="brand-mark" aria-hidden="true" />
            <div>
              <div className="brand-name">Luma</div>
              <div className="brand-subtitle">Code workspace</div>
            </div>
          </div>

          <div className="workspace-label">WORKSPACE</div>
          <div className="workspace">
            <span className="workspace-icon" aria-hidden="true">⌘</span>
            <span id="workspace-name">Workspace</span>
            <span className="workspace-chevron" aria-hidden="true">⌄</span>
          </div>

          <div className="section-label"><span>EXPLORER</span></div>
          <div id="explorer" className="explorer">
            <div className="empty">Loading workspace…</div>
          </div>

          <div className="section-label"><span>HISTORY</span><button id="refresh-history" className="icon-button" title="Refresh history" aria-label="Refresh history">↻</button></div>
          <div id="chat-history" className="chat-history"><div className="empty">Loading chats…</div></div>

          <div className="sidebar-bottom">
            <button id="new-file" className="sidebar-action"><span>＋</span>New File</button>
            <button id="new-folder" className="sidebar-action"><span>＋</span>New Folder</button>
            <div className="connection">
              <span id="connection-dot" className="connection-dot" />
              <span id="connection-text"> Connecting </span>
            </div>
          </div>
        </aside>

        <main className={`main ${teammatesOpen ? "teammates-mode" : ""}`}>
          <header className="topbar">
            <div className="breadcrumbs">
              <span className="breadcrumb-root">Luma</span>
              <span className="breadcrumb-separator">›</span>
              <span id="current-file">{teammatesOpen ? "Teammates" : "Agent"}</span>
            </div>

            <div className="topbar-actions">
              <ModePicker teammatesOpen={teammatesOpen} onSelectCode={onSelectCode} onOpenTeammates={onOpenTeammates} />
              <button id="toggle-ide" className="ide-toggle" title="Open IDE" hidden>Open IDE</button>
              <button id="refresh-tree" className="icon-button" title="Refresh workspace" aria-label="Refresh workspace">↻</button>
              <button id="theme-toggle" className="icon-button" title="Switch theme" aria-label="Switch theme">☼</button>
            </div>
          </header>

          {teammatesOpen ? <TeammatesPanel /> : <>
          <div className="workspace-layout" id="workspace-layout">
            <section id="changes" className="changes-area hidden">
              <div className="changes-header">
                <div><strong>Working tree</strong><span id="git-branch">Checking repository…</span></div>
                <button id="refresh-changes" className="ide-toggle">Refresh</button>
              </div>
              <div className="changes-content">
                <div id="change-files" className="change-files"><div className="empty">Loading changes…</div></div>
                <pre id="change-diff" className="change-diff">Select a changed file to review its diff.</pre>
              </div>
            </section>

            <section id="ide" className="editor-area hidden">
              <div id="tabs" className="tabs" />
              <div className="editor-toolbar">
                <div id="editor-path">No file open</div>
                <div className="editor-actions">
                  <button id="save-file" disabled>Save <kbd>⌘S</kbd></button>
                  <button id="show-diff" disabled>Diff</button>
                </div>
              </div>
              <div className="editor-container">
                <div id="line-numbers" className="line-numbers" />
                <div id="editor" />
              </div>
              <div className="editor-statusbar">
                <span id="editor-language"> Plain Text </span>
                <span id="editor-position"> Ln 1, Col 1 </span>
                <span id="editor-state"> No file </span>
              </div>
            </section>

            <aside className="agent-panel">
              <div className="agent-header">
                <div>
                  <div className="agent-title"><span id="agent-status-dot" className="agent-dot" />Code</div>
                  <div id="agent-status-detail" className="agent-detail">Ready</div>
                </div>
                <div className="agent-header-actions">
                  <div id="model-name" className="model-name">—</div>
                </div>
              </div>

              <div id="agent-content" className="agent-content">
                <div id="messages" className="messages">
                  <div id="chat-empty" className="chat-empty">
                    <h2>What would you like to work on?</h2>
                    <p>Ask a question or describe a change. Luma can work with the files in this workspace.</p>
                    <div className="suggestions">
                      <button data-prompt="Explain this project.">Explain this project</button>
                      <button data-prompt="Find the main entry point.">Find the entry point</button>
                      <button data-prompt="Check the project for errors.">Check for errors</button>
                    </div>
                  </div>
                </div>
              </div>

              <div id="plan-panel" className="plan-panel hidden">
                <div className="panel-title"><span>Plan</span><span id="plan-count"> 0 steps </span></div>
                <div id="plan" />
              </div>

              <div id="confirmation" className="confirmation hidden">
                <div className="confirmation-title">Luma wants to perform an action</div>
                <div id="confirmation-name" className="confirmation-name" />
                <pre id="confirmation-input" />
                <div className="confirmation-actions">
                  <button id="deny" className="secondary">Deny</button>
                  <button id="allow" className="primary">Allow</button>
                </div>
              </div>

              <form id="chat-form" className="composer">
                <input id="chat-file-input" type="file" multiple hidden />
                <button type="button" id="attach-button" aria-label="Attach files" title="Attach text files">+</button>
                <textarea id="chat-input" rows={1} placeholder="Ask Luma anything…" />
                <button type="submit" id="send-button">↑</button>
                <button type="button" id="cancel-button" hidden aria-label="Cancel generation" title="Cancel generation">■</button>
              </form>
            </aside>
          </div>

          <section className="terminal-panel">
            <div className="terminal-header">
              <div className="terminal-title"><span className="terminal-live-dot" />TERMINAL <span className="terminal-caption">Workspace shell</span></div>
              <button id="clear-terminal">Clear</button>
            </div>
            <div id="terminal-output" className="terminal-output">
              <div className="terminal-line muted">Luma terminal ready.</div>
            </div>
            <form id="terminal-form" className="terminal-input">
              <span>$</span>
              <input id="terminal-command" autoComplete="off" spellCheck={false} placeholder="Run a command…" />
            </form>
          </section>
          </>}
        </main>
      </div>
      <div id="toast" className="toast hidden" />
    </>
  );
}

type ApprovalRecord = { id: string; agent_id: string; task_id: string; tool: string; arguments: string };

function TeammatesPanel() {
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [schedules, setSchedules] = useState<ScheduleRecord[]>([]);
  const [memoryEntries, setMemoryEntries] = useState<MemoryEntry[]>([]);
  const [runtimeEvents, setRuntimeEvents] = useState<RuntimeEvent[]>([]);
  const [selected, setSelected] = useState("");
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
  const chatFileInput = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState<"chat" | "settings">("chat");
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
      const nextSelected = selected && nextAgents.some((agent) => agent.id === selected) ? selected : nextAgents.find((agent) => agent.kind === "teammates")?.id ?? nextAgents[0]?.id ?? "";
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
      <header className="teammates-dialog-header"><div className="teammates-title-wrap"><MascotAvatar seed="luma-crew" size={42} /><div><span className="teammates-wordmark-luma teammates-brand-kicker">LUMA</span><h2 id="teammates-heading" className="teammates-wordmark-name">teammates</h2><p>Persistent agents, tasks and approvals</p></div></div><div className="teammates-counts">{agents.length} agents · {tasks.filter((task) => ["queued", "running", "waiting_for_approval"].includes(task.status)).length} active</div></header>
      {error && <div className="teammates-error" role="alert">{error}</div>}
      <div className="teammates-body">
        <aside className="teammates-list"><div className="teammates-list-heading"><div className="teammates-section-title">AGENTS</div><button type="button" disabled={busy} onClick={() => setCreateOpen(true)}>+ Create</button></div>{agents.map((agent) => <div className={`teammate-row-wrap ${selected === agent.id ? "selected" : ""}`} key={agent.id}><button type="button" onClick={() => setSelected(agent.id)} className="teammate-row"><MascotAvatar seed={agent.name} mascot={agent.configuration.mascot} size={30} active={agent.status === "active"} /><span><strong>{agent.name}</strong><small>{agent.id === "agent_global" ? "Global · protected" : agent.status}</small></span></button></div>)}</aside>
        <main className="teammates-detail">{!activeAgent ? <div className="teammates-empty"><MascotAvatar seed="luma-welcome" size={76} /><p>Create a teammate and give it a name, a workspace and work to own.</p></div> : <>
          <div className="teammates-agent-heading"><div className="teammates-agent-identity"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={42} active={activeAgent.status === "active"} /><div><h3>{activeAgent.name}</h3><p>{activeAgent.id === "agent_global" ? "Global teammate · shared defaults" : `${activeAgent.status} · ${activeAgent.kind}`}</p></div></div><div className="teammates-agent-actions"><button type="button" className={activeTab === "chat" ? "active" : ""} onClick={() => setActiveTab("chat")}>Chat</button><button type="button" className={activeTab === "settings" ? "active" : ""} onClick={() => setActiveTab("settings")}>Settings</button>{activeAgent.id !== "agent_global" && <button type="button" disabled={busy} onClick={() => { if (window.confirm(`Delete ${activeAgent.name}? Cancel active tasks first.`)) void perform(`/api/agents/${activeAgent.id}`, undefined, "DELETE"); }}>Delete</button>}</div></div>
          {activeTab === "chat" ? <div className="teammates-chat"><div className="teammates-chat-messages">{agentTasks.length === 0 ? <div className="teammates-empty"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={76} active /><h3>Chat with {activeAgent.name}</h3><p>Ask a question, hand over a task, or just start a conversation. Your teammate keeps the thread's context between messages.</p></div> : agentTasks.slice().reverse().map((task) => { const answer = taskAssistantText(task.id); return <article className="teammates-chat-turn" key={task.id}><div className="teammates-chat-user"><small>YOU</small><p>{task.request}</p></div><div className="teammates-chat-reply"><MascotAvatar seed={activeAgent.name} mascot={activeAgent.configuration.mascot} size={28} active={task.status === "running"} /><div><small>{activeAgent.name} {task.status === "running" ? "· working" : task.status === "queued" ? "· queued" : ""}</small><p>{answer || task.error || (task.status === "completed" ? "No text response was recorded." : "Thinking…")}</p>{["failed", "interrupted"].includes(task.status) && <button type="button" onClick={() => void perform(`/api/tasks/${task.id}/retry`)}>Retry</button>}</div></div></article>; })}{approvals.filter((approval) => approval.agent_id === activeAgent.id).map((approval) => <div className="teammate-approval" key={approval.id}><strong>{activeAgent.name} needs approval · {approval.tool}</strong><pre>{approval.arguments}</pre><div><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: false })}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(`/api/approvals/${approval.id}`, { allowed: true })}>Approve</button></div></div>)}</div><form className="teammates-chat-composer" onSubmit={(event) => { event.preventDefault(); const text = chatPrompt.trim() || (chatAttachments.length ? "Please inspect the attached files." : ""); if (!text || busy || activeAgent.status === "stopped" || activeAgent.status === "paused") return; void (async () => { await perform("/api/tasks", { agent_id: activeAgent.id, request: formatChatPrompt(text, chatAttachments), workspace: activeAgent.configuration.workspace ?? undefined }); setChatPrompt(""); setChatAttachments([]); })(); }}>
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
  const [mode, setMode] = useState<Mode>("agent");
  const [teammatesOpen, setTeammatesOpen] = useState(false);
  const modeRef = useRef(mode);
  modeRef.current = mode;

  useEffect(() => {
    void import("./controller.js").then(() => {
      window.dispatchEvent(new CustomEvent<Mode>("luma:mode-change", { detail: modeRef.current }));
    });
  }, []);

  useEffect(() => {
    window.dispatchEvent(new CustomEvent<Mode>("luma:mode-change", { detail: mode }));
  }, [mode]);

  return <DashboardShell teammatesOpen={teammatesOpen} onSelectCode={() => { setMode("agent"); setTeammatesOpen(false); }} onOpenTeammates={() => setTeammatesOpen(true)} />;
}
