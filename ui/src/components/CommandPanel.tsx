import { Box, Text, useInput } from "ink";
import { useEffect, useMemo, useRef, useState } from "react";

import { theme } from "../theme";

type PanelId = "git" | "files" | "find_files" | "search" | "doctor" | "monitor" | "teammates" | "tasks" | "approvals" | "schedules" | "memory" | "history" | "history_show" | "history_delete" | "history_prune" | "workspace" | "status" | "version" | "tools" | "skills" | "plugins" | "mcp";

export function CommandPanel({ id, output, loading, error, onRun, onAction, onDismissNotice, onClose }: {
  id: PanelId;
  output: string | null;
  loading: boolean;
  error: string | null;
  onRun: (id: string, query: string | null) => void;
  onAction: (action: "task_cancel" | "task_retry" | "approval_resolve", id: string, allowed?: boolean) => void;
  onDismissNotice: () => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [gitView, setGitView] = useState("status");
  const [selected, setSelected] = useState(0);
  const onRunRef = useRef(onRun);
  onRunRef.current = onRun;
  const records = useMemo(() => {
    if (!output) return [] as Record<string, unknown>[];
    try {
      const parsed: unknown = JSON.parse(output);
      if (Array.isArray(parsed)) return parsed.filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null);
      if (typeof parsed === "object" && parsed !== null) {
        return Object.values(parsed as Record<string, unknown>).flatMap((value) => Array.isArray(value) ? value : [])
          .filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null);
      }
    } catch { /* Plain-text command output is rendered below. */ }
    return [] as Record<string, unknown>[];
  }, [output]);
  const active = records[selected];
  useEffect(() => {
    setSelected((value) => Math.max(0, Math.min(value, records.length - 1)));
  }, [records.length]);
  useEffect(() => {
    if (id !== "tasks" && id !== "approvals" && id !== "monitor") return;
    const interval = setInterval(() => onRunRef.current(id, null), id === "monitor" ? 5_000 : 3_000);
    return () => clearInterval(interval);
  }, [id]);
  const title = ({ git: "Git Explorer", files: "Workspace Explorer", find_files: "Fuzzy File Finder", search: "Workspace Search", doctor: "Workspace Diagnostics", monitor: "Live System Monitor", teammates: "Teammate Control Center", tasks: "Task Monitor", approvals: "Approval Inbox", schedules: "Schedules", memory: "Memory Explorer", history: "Conversation Search", history_show: "Conversation Inspector", history_delete: "Delete Conversation", history_prune: "Prune Conversation History", workspace: "Active Workspace", status: "Runtime Status", version: "Luma Version", tools: "Available Tools", skills: "Installed Skills", plugins: "Plugins", mcp: "MCP Connections" } satisfies Record<PanelId, string>)[id];

  useInput((input, key) => {
    if (key.escape) { onClose(); return; }
    if (key.ctrl && input.toLowerCase() === "d") { onDismissNotice(); return; }
    if (key.upArrow) { setSelected((value) => Math.max(0, value - 1)); return; }
    if (key.downArrow) { setSelected((value) => Math.min(records.length - 1, value + 1)); return; }
    const recordId = typeof active?.id === "string" ? active.id : null;
    const status = typeof active?.status === "string" ? active.status : "";
    if (recordId && id === "approvals" && input.toLowerCase() === "a") { onAction("approval_resolve", recordId, true); return; }
    if (recordId && id === "approvals" && input.toLowerCase() === "d") { onAction("approval_resolve", recordId, false); return; }
    if (recordId && id === "tasks" && input.toLowerCase() === "c" && ["queued", "running", "waiting_for_input", "waiting_for_approval"].includes(status)) { onAction("task_cancel", recordId); return; }
    if (recordId && id === "tasks" && input.toLowerCase() === "r" && ["failed", "interrupted"].includes(status)) { onAction("task_retry", recordId); return; }
    if (id === "git" && !query && ["1", "2", "3", "4", "7"].includes(input)) {
      const next = ({ "1": "status", "2": "diff", "3": "log", "4": "branches", "7": "diff_staged" } as Record<string, string>)[input] ?? "status";
      setGitView(next);
      onRun("git", next);
      return;
    }
    if (id === "git" && !query && ["5", "6"].includes(input)) {
      setGitView(input === "5" ? "stage" : "unstage");
      setQuery("");
      return;
    }
    if (key.return) {
      if (id === "git" && (gitView === "stage" || gitView === "unstage")) {
        if (query.trim()) onRun("git", `${gitView}:${query.trim()}`);
      }
      else if (id === "git") onRun("git", gitView);
      else if (id === "files" && query.trim()) onRun("read_file", query.trim());
      else if (id === "find_files" && typeof active?.path === "string") onRun("read_file", active.path);
      else if (id === "find_files" && query.trim()) onRun("find_files", query.trim());
      else if (id === "search" || id === "history" || id === "memory") onRun(id, query.trim() || null);
      else if (id === "history_show") { if (query.trim()) onRun("history_show", query.trim()); }
      else if (id === "history_delete") { if (query.trim()) onRun("history_delete", query.trim()); }
      else if (id === "history_prune") { if (query.trim()) onRun("history_prune", query.trim()); }
      else onRun(id, query.trim() || null);
      return;
    }
    if (key.backspace || key.delete) { setQuery((value) => value.slice(0, -1)); return; }
    if (!key.ctrl && !key.meta && input && input.length === 1) setQuery((value) => value + input);
  });

  return <Box flexDirection="column" borderStyle="round" borderColor={theme.brand} paddingX={1} paddingY={1} marginY={1}>
    <Box justifyContent="space-between">
      <Text color={theme.text} bold>{title}</Text>
      <Text color={theme.chrome}>Esc close · Ctrl+D dismiss notice · Enter refresh/run</Text>
    </Box>
    {id === "git" ? <Text color={theme.brand}>1 Status · 2 Worktree diff · 3 Recent commits · 4 Branches · 5 Stage path · 6 Unstage path · 7 Staged diff  [{gitView}]</Text> : null}
    {id === "git" && (gitView === "stage" || gitView === "unstage") ? <Text color={theme.brand}>Workspace-relative path: {query || "type a file path, then Enter"}</Text> : null}
    {id === "search" ? <Text color={theme.brand}>⌕ {query || "Type a pattern, then press Enter"}</Text> : null}
    {id === "files" ? <Text color={theme.brand}>Path: {query || "(workspace root) · type a file path to preview"}</Text> : null}
    {id === "find_files" ? <Text color={theme.brand}>⌕ {query || "Type a path fragment, then Enter · Enter opens the selected result"}</Text> : null}
    {id === "history" ? <Text color={theme.brand}>Search · optional workspace:, model:, tool: filters: {query || "type text, then press Enter"}</Text> : null}
    {id === "memory" ? <Text color={theme.brand}>Filter: {query || "Type to filter, then press Enter"}</Text> : null}
    {id === "history_show" ? <Text color={theme.brand}>Conversation ID: {query || "Enter a saved conversation ID"}</Text> : null}
    {id === "history_delete" ? <Text color={theme.brand}>ID to permanently delete: {query || "Enter the exact conversation ID"}</Text> : null}
    {id === "history_prune" ? <Text color={theme.brand}>Type confirm to permanently delete all saved conversations: {query || ""}</Text> : null}
    {id === "tasks" ? <Text color={theme.brand}>↑↓ select · C cancel active task · R retry failed task</Text> : null}
    {id === "approvals" ? <Text color={theme.brand}>↑↓ select · A approve · D deny</Text> : null}
    {loading ? <Text color={theme.muted}>Loading from Luma Core…</Text> : null}
    {error ? <Text color={theme.error}>{error}</Text> : null}
    {records.length > 0 ? <Box flexDirection="column" marginTop={1}>
      {records.slice(0, 18).map((record, index) => {
        const name = String(record.title ?? record.name ?? record.id ?? record.key ?? `Item ${index + 1}`);
        const status = String(record.status ?? record.enabled ?? "");
        return <Text key={String(record.id ?? record.key ?? index)} color={index === selected ? theme.brand : theme.text} bold={index === selected}>
          {index === selected ? "› " : "  "}{name}{status ? `  ·  ${status}` : ""}
        </Text>;
      })}
      {id === "monitor" ? <MonitorSummary records={records} /> : active ? <Text color={theme.muted}>{JSON.stringify(active, null, 2)}</Text> : null}
    </Box> : output ? <Box flexDirection="column" marginTop={1}>
      <Text color={theme.chrome}>{"─".repeat(48)}</Text>
      <Text color={theme.secondary}>{output}</Text>
    </Box> : null}
  </Box>;
}

function MonitorSummary({ records }: { records: Record<string, unknown>[] }) {
  const agents = records.filter((record) => typeof record.agent_id === "string" && typeof record.name === "string");
  const tasks = records.filter((record) => typeof record.task_id === "string");
  const metric = (record: Record<string, unknown>) => record.metrics && typeof record.metrics === "object"
    ? record.metrics as Record<string, unknown>
    : {};
  const sum = (key: string) => agents.reduce((total, agent) => total + number(metric(agent)[key]), 0);
  const taskCounts = tasks.reduce<Record<string, number>>((counts, task) => {
    const status = string(task.status, "unknown");
    counts[status] = (counts[status] ?? 0) + 1;
    return counts;
  }, {});
  const toolsStarted = sum("tools_started");
  const toolsFinished = sum("tools_finished");
  const toolFailures = sum("tool_failures");

  return <Box flexDirection="column" marginTop={1}>
    <Text color={theme.brand} bold>System totals</Text>
    <Text color={theme.secondary}>
      {agents.length} agents · {tasks.length} tasks ({Object.entries(taskCounts).map(([status, count]) => `${count} ${status}`).join(", ") || "no task records"})
    </Text>
    <Text color={theme.secondary}>
      {sum("total_tokens").toLocaleString()} tokens · {sum("prompt_tokens").toLocaleString()} in / {sum("completion_tokens").toLocaleString()} out
    </Text>
    <Text color={theme.secondary}>
      {toolsStarted} tools started · {toolsFinished} finished · {toolFailures} failed
    </Text>
    {agents.length > 0 ? <>
      <Text color={theme.brand} bold>Agents</Text>
      {agents.slice(0, 8).map((agent) => <Text key={String(agent.agent_id)} color={theme.text}>
        {string(agent.name, "Unnamed agent")} · {string(agent.status, "unknown")} · {number(metric(agent).total_tokens).toLocaleString()} tokens · {number(metric(agent).tools_finished)} tools
      </Text>)}
    </> : null}
    {tasks.length > 0 ? <>
      <Text color={theme.brand} bold>Recent tasks</Text>
      {tasks.slice(0, 8).map((task) => <Text key={String(task.task_id)} color={theme.text}>
        {string(task.title, String(task.task_id).slice(0, 8))} · {string(task.status, "unknown")} · {number(metric(task).total_tokens).toLocaleString()} tokens
      </Text>)}
    </> : null}
  </Box>;
}

function number(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function string(value: unknown, fallback: string): string {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}
