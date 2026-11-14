import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { useFocusTrap } from "./hooks/useFocusTrap";

import Editor from "@monaco-editor/react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import LumaWordmark, { LumaMark } from "./components/LumaWordmark";

import { themes, useTheme } from "./renderer/Theme";

import { useLuma, type AgentTool } from "./renderer/hooks/useLuma";

import SetupWizard from "./setup/SetupWizard";
import ModelPickerDialog from "./components/ModelPickerDialog";
import ExtensionsPanel from "./components/ExtensionsPanel";

import type { SetupConfig, SetupModel, SetupProvider } from "./stores/setupStore";
import { useSetupStore } from "./stores/setupStore";

import { useWorkspaceStore, type Chat } from "./stores/workspaceStore";
import { MascotAvatar } from "./components/MascotAvatar";
import type { AgentConfiguration, AgentRecord, ApprovalRecord, MemoryEntry, MemoryScope, ScheduleRecord, TaskRecord, RuntimeEvent } from "../../shared/protocol";
import { formatChatPrompt, type ChatAttachment } from "../../shared/attachments";
import { MascotCustomizer } from "./components/MascotCustomizer";
import { getProfileDisplayName, useProfileStore } from "./stores/profileStore";

type AppMode = "agent" | "ide" | "changes" | "teammates";
type SettingsSection = "appearance" | "general" | "gateway" | "behavior" | "extensions";
type TitlebarStatus = { text: string; color: "blue" | "green" | "amber" | "red" | "muted" };
type TeammateToolRun = { id: string; name: string; input: string; durationMs?: number; failed?: boolean; diff?: string };
type TeammateTimelineSegment =
  | { kind: "text"; id: string; content: string }
  | { kind: "tools"; id: string; tools: TeammateToolRun[] };

export type WorkspaceNode =
  | {
      type: "folder";
      name: string;
      path: string;
      children: WorkspaceNode[];
    }
  | {
      type: "file";
      name: string;
      path: string;
    };

export type GitChange = {
  path: string;
  indexStatus: string;
  worktreeStatus: string;
  kind: "added" | "modified" | "deleted" | "renamed" | "untracked";
};

const SLASH_COMMANDS = [
  { command: "/help", description: "Show available commands" },
  { command: "/clear", description: "Clear the conversation" },
  { command: "/new", description: "Start a fresh conversation" },
  { command: "/quit", description: "Quit Luma" },
  { command: "/exit", description: "Quit Luma (alias)" },
  { command: "/history", description: "List recent saved chats" },
  { command: "/history list", description: "List saved chats" },
  { command: "/history show", description: "Inspect an archived chat by ID" },
  { command: "/history delete", description: "Delete an archived chat by ID" },
  { command: "/history prune", description: "Delete every saved chat" },
  { command: "/restore", description: "Restore a saved chat by ID" },
  { command: "/summary:add", description: "Save a summary for this chat" },
  { command: "/status", description: "Show model, workspace and session status" },
  { command: "/workspace", description: "Show the active workspace" },
  { command: "/version", description: "Show the Luma version" },
  { command: "/doctor", description: "Inspect model, workspace and session health" },
  { command: "/tools", description: "List tools enabled in this session" },
  { command: "/skills", description: "Discover available Agent Skills" },
  { command: "/plugins", description: "List installed native Luma plugins" },
  { command: "/mcp", description: "List configured MCP servers" },
  { command: "/teammates", description: "List persistent personal agents" },
  { command: "/agents", description: "List persistent personal agents (alias)" },
  { command: "/tasks", description: "List recent persistent tasks" },
  { command: "/approvals", description: "List pending approvals" },
  { command: "/schedules", description: "List scheduled teammate work" },
  { command: "/memory", description: "List saved user and workspace memories" },
  { command: "/resume", description: "Resume an interrupted task" },
  { command: "/plan", description: "Create a plan without implementing" },
  { command: "/init", description: "Initialize workspace instructions" },
  { command: "/model", description: "Change the default model" },
  { command: "/model session", description: "Change the model for this session only" },
];

const TEAMMATE_SLASH_COMMANDS = [
  { command: "/help", description: "Show Teammates commands" },
  { command: "/new", description: "Start a new task in this teammate's history" },
  { command: "/history", description: "Show this teammate's full history" },
  { command: "/settings", description: "Open teammate settings" },
  { command: "/memory", description: "Open teammate memory" },
  { command: "/schedule", description: "Manage recurring work" },
  { command: "/status", description: "Show teammate status" },
  { command: "/pause", description: "Pause this teammate" },
  { command: "/resume", description: "Resume this teammate" },
  { command: "/stop", description: "Stop the active task" },
];

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`transition-transform ${open ? "rotate-90" : ""}`}
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M3 6.5A2.5 2.5 0 0 1 5.5 4H10l2 2h6.5A2.5 2.5 0 0 1 21 8.5v9A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-11z" />
    </svg>
  );
}

function FileIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v5h5" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
    >
      <circle cx="11" cy="11" r="6.5" />
      <path d="m16 16 5 5" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

function PanelCloseIcon() {
  return <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="3.5" y="4.5" width="17" height="15" rx="3" /><path d="M14.5 4.5v15" /></svg>;
}

function toolActivityLabel(tools: { name: string }[]) {
  const displayNames: Record<string, string> = {
    read_file: "Read", list_directory: "List", search_files: "Grep", write_file: "Write",
    patch_file: "Edit", run_command: "Terminal", web_search: "Search", fetch_url: "Fetch",
    http_request: "HTTP",
  };
  const labels = [...new Set(tools.map((tool) => displayNames[tool.name] ?? tool.name.split(/[_-]/g).map((part) => part.charAt(0).toUpperCase() + part.slice(1)).join(" ")))];
  const names = labels.length > 1 ? `${labels.slice(0, -1).join(", ")} and ${labels[labels.length - 1]}` : labels[0] ?? "tools";
  return `Used ${tools.length} ${names}`;
}

function routeTeammateMentions(prompt: string, agents: AgentRecord[]) {
  const destinations = agents.filter((agent) => {
    const escaped = agent.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return new RegExp(`(^|\\s)@${escaped}(?=$|\\s|[,.;:!?])`, "i").test(prompt);
  });
  let routedPrompt = prompt;
  for (const agent of destinations) {
    const escaped = agent.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    routedPrompt = routedPrompt.replace(new RegExp(`(^|\\s)@${escaped}(?=$|\\s|[,.;:!?])`, "gi"), "$1");
  }
  return { destinations, prompt: routedPrompt.replace(/\s+/g, " ").trim() };
}

function TeammateModelDropdown({ agent, onChanged }: { agent: AgentRecord; onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<{ top: number; left: number } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [config, setConfig] = useState<SetupConfig | null>(null);
  const [provider, setProvider] = useState<SetupProvider | null>(null);
  const [models, setModels] = useState<SetupModel[]>([]);
  const [search, setSearch] = useState("");
  const [activeModelIndex, setActiveModelIndex] = useState(0);
  const [selectedLabel, setSelectedLabel] = useState(agent.configuration.model ?? "");
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const currentLabel = selectedLabel || agent.configuration.model || config?.model.name || "Loading model…";

  useEffect(() => {
    let active = true;
    void window.luma.getSetupConfig().then((saved) => { if (active) setConfig(saved); })
      .catch((reason: unknown) => { if (active) setError(reason instanceof Error ? reason.message : String(reason)); });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    setSelectedLabel(agent.configuration.model ?? "");
  }, [agent.configuration.model]);

  useEffect(() => {
    if (!open) { setMenuPosition(null); return; }
    const updatePosition = () => {
      const anchor = triggerRef.current;
      if (!anchor) return;
      const rect = anchor.getBoundingClientRect();
      const menuHeight = Math.min(340, window.innerHeight - 24);
      const openAbove = rect.top >= menuHeight + 12;
      const top = openAbove ? rect.top - menuHeight - 8 : Math.min(rect.bottom + 8, window.innerHeight - menuHeight - 12);
      const left = Math.max(8, Math.min(rect.right - 256, window.innerWidth - 264));
      setMenuPosition({ top: Math.max(8, top), left });
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  async function showModels() {
    setOpen((value) => !value);
    if (open || models.length > 0 || loading) return;
    setLoading(true);
    setError("");
    try {
      const [saved, available] = await Promise.all([window.luma.getSetupConfig(), window.luma.getProviders()]);
      if (!saved) throw new Error("Configure a model before choosing one for this teammate.");
      const providerId = agent.configuration.provider ?? saved.model.provider;
      const selectedProvider = available.find((item) => item.id === providerId) ?? null;
      if (!selectedProvider) throw new Error(`Provider not found: ${providerId}`);
      setConfig(saved);
      setProvider(selectedProvider);
      const endpoint = agent.configuration.endpoint ?? saved.model.endpoint;
      const apiKey = saved.provider_api_keys?.[providerId]
        ?? (saved.model.provider === providerId ? saved.model.api_key ?? undefined : undefined);
      setModels(await window.luma.getModels(selectedProvider, { apiKey, endpoint }));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function chooseModel(model: SetupModel) {
    if (!provider) return;
    setLoading(true);
    setError("");
    try {
      await window.luma.updateAgent({
        id: agent.id,
        configuration: {
          ...agent.configuration,
          provider: provider.id,
          model: model.id,
          endpoint: agent.configuration.endpoint ?? config?.model.endpoint ?? provider.endpoint,
        },
      });
      setSelectedLabel(model.name);
      setOpen(false);
      onChanged();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }

  const filtered = models.filter((model) => `${model.name} ${model.id}`.toLowerCase().includes(search.toLowerCase()));
  return <div className="relative shrink-0">
    <button ref={triggerRef} type="button" aria-haspopup="listbox" aria-expanded={open} onKeyDown={(event) => {
      if (["ArrowDown", "Enter", " "].includes(event.key)) { event.preventDefault(); if (!open) void showModels(); }
    }} onClick={() => void showModels()} disabled={loading} className="luma-model-inline max-w-[150px]" title={`Current model: ${currentLabel}. Change model`}>
      <span className="max-w-[120px] truncate">{currentLabel}</span><span aria-hidden="true">⌄</span>
    </button>
    {open && menuPosition && createPortal(<div ref={menuRef} style={{ position: "fixed", top: menuPosition.top, left: menuPosition.left, width: 256, maxHeight: "min(340px, calc(100vh - 24px))", zIndex: 9999 }} className="overflow-hidden rounded-md border border-[var(--luma-border-strong)] bg-[var(--luma-surface)] shadow-xl">
      <div className="border-b border-[var(--luma-border)] p-2"><input autoFocus value={search} onChange={(event) => { setSearch(event.target.value); setActiveModelIndex(0); }} aria-activedescendant={filtered[activeModelIndex] ? `teammate-model-option-${activeModelIndex}` : undefined} onKeyDown={(event) => {
        if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key) && filtered.length) {
          event.preventDefault();
          const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
          const next = (activeModelIndex + direction + filtered.length) % filtered.length;
          setActiveModelIndex(next);
          document.getElementById(`teammate-model-option-${next}`)?.scrollIntoView({ block: "nearest" });
        }
        else if (event.key === "Enter" && filtered[activeModelIndex]) { event.preventDefault(); void chooseModel(filtered[activeModelIndex]); }
        else if (event.key === "Escape") { event.preventDefault(); setOpen(false); triggerRef.current?.focus(); }
      }} placeholder="Filter models…" aria-label="Filter teammate models" aria-controls="teammate-model-options" className="luma-input h-7 w-full text-[11px]" /></div>
      <div id="teammate-model-options" className="max-h-56 overflow-y-auto p-1" role="listbox" aria-label="Available models">
        {loading ? <p className="px-2 py-3 text-[10px] text-[var(--luma-text-muted)]">Loading models…</p> : filtered.map((model, index) => <button id={`teammate-model-option-${index}`} key={model.id} type="button" role="option" aria-selected={index === activeModelIndex} onMouseEnter={() => setActiveModelIndex(index)} onClick={() => void chooseModel(model)} className={`block w-full rounded px-2 py-1.5 text-left ${index === activeModelIndex ? "bg-[var(--luma-accent-soft)]" : "hover:bg-[var(--luma-surface-hover)]"}`}><span className="block truncate text-[11px] text-[var(--luma-text)]">{model.name}</span>{model.id !== model.name && <span className="block truncate text-[9px] text-[var(--luma-text-muted)]">{model.id}</span>}</button>)}
        {!loading && filtered.length === 0 && <p className="px-2 py-3 text-[10px] text-[var(--luma-text-muted)]">No matching models.</p>}
      </div>
      {error && <p role="alert" className="border-t border-[var(--luma-border)] px-2 py-2 text-[10px] text-[var(--luma-danger)]">{error}</p>}
      <div className="flex justify-end border-t border-[var(--luma-border)] p-1"><button type="button" onClick={() => setOpen(false)} className="px-2 py-1 text-[10px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">Close</button></div>
    </div>, document.body)}
  </div>;
}

function PinIcon({ filled }: { filled?: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M15 4.5 19.5 9l-3.2.8a2 2 0 0 0-1.5 1.5L14 15l-5-5 .7-.8a2 2 0 0 0 1.5-1.5L12 4.5z" />
      <path d="m9 15-5 5" />
    </svg>
  );
}

function SettingsIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.64 5.64l1.42 1.42M16.94 16.94l1.42 1.42M18.36 5.64l-1.42 1.42M7.06 16.94l-1.42 1.42" />
      <circle cx="12" cy="12" r="3.5" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
      <rect x="6" y="6" width="12" height="12" rx="1.5" />
    </svg>
  );
}

function SendIcon() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4 12h15" />
      <path d="m13 6 6 6-6 6" />
    </svg>
  );
}

function SettingsPanel({
  open,
  onClose,
  initialSection,
  connected,
  setupConfig,
  workspacePath,
  sidebarBlur,
  onSidebarBlurChange,
  compactHistory,
  onCompactHistoryChange,
  reducedMotion,
  onReducedMotionChange,
  sendWithEnter,
  onSendWithEnterChange,
  autoScrollToLatest,
  onAutoScrollToLatestChange,
  showToolActivity,
  onShowToolActivityChange,
  onChangeModel,
  onOpenTour,
  onRerunSetup,
}: {
  open: boolean;
  onClose: () => void;
  initialSection: SettingsSection;
  connected: boolean;
  setupConfig: SetupConfig | null;
  workspacePath: string;
  sidebarBlur: boolean;
  onSidebarBlurChange: (enabled: boolean) => void;
  compactHistory: boolean;
  onCompactHistoryChange: (enabled: boolean) => void;
  reducedMotion: boolean;
  onReducedMotionChange: (enabled: boolean) => void;
  sendWithEnter: boolean;
  onSendWithEnterChange: (enabled: boolean) => void;
  autoScrollToLatest: boolean;
  onAutoScrollToLatestChange: (enabled: boolean) => void;
  showToolActivity: boolean;
  onShowToolActivityChange: (enabled: boolean) => void;
  onChangeModel: () => void;
  onOpenTour: () => void;
  onRerunSetup: () => void;
}) {
  const { theme, setTheme, definition } = useTheme();
  const profileName = useProfileStore((state) => state.name);
  const setProfileName = useProfileStore((state) => state.setName);
  const trapRef = useFocusTrap(open);
  const [accentColor, setAccentColor] = useState(() => {
    try { return window.localStorage.getItem("luma.accent") ?? ""; }
    catch { return ""; }
  });
  const [customProvidersDraft, setCustomProvidersDraft] = useState(() =>
    JSON.stringify(setupConfig?.providers ?? [], null, 2),
  );
  const [providerSaveError, setProviderSaveError] = useState("");
  const [gatewayStatus, setGatewayStatus] = useState<"running" | "stopped" | "unknown">("unknown");
  const [gatewayBusy, setGatewayBusy] = useState(false);
  const [gatewayError, setGatewayError] = useState("");
  const [gatewayLogs, setGatewayLogs] = useState("");
  const [gatewayInfo, setGatewayInfo] = useState<{ directory: string; endpoint: string; runtimeDatabase: string; logFile: string } | null>(null);
  const [coreBinaryMode, setCoreBinaryMode] = useState<"bundled" | "path" | "custom">("bundled");
  const [coreBinaryPath, setCoreBinaryPath] = useState("");
  const [coreBinaryCurrent, setCoreBinaryCurrent] = useState("");
  const [coreBinaryOverride, setCoreBinaryOverride] = useState(false);
  const [coreBinaryBusy, setCoreBinaryBusy] = useState(false);
  const [coreBinaryError, setCoreBinaryError] = useState("");

  useEffect(() => {
    setCustomProvidersDraft(JSON.stringify(setupConfig?.providers ?? [], null, 2));
  }, [setupConfig]);

  useEffect(() => {
    const root = document.documentElement;
    if (/^#[0-9a-f]{6}$/i.test(accentColor)) {
      root.dataset.customAccent = "true";
      root.style.setProperty("--luma-accent", accentColor);
      root.style.setProperty("--luma-accent-soft", `color-mix(in srgb, ${accentColor} 12%, transparent)`);
      try { window.localStorage.setItem("luma.accent", accentColor); } catch { /* Keep the current session preference. */ }
    } else {
      delete root.dataset.customAccent;
      root.style.removeProperty("--luma-accent");
      root.style.removeProperty("--luma-accent-soft");
      try { window.localStorage.removeItem("luma.accent"); } catch { /* Keep the current session preference. */ }
    }
  }, [accentColor]);

  const [section, setSection] = useState<SettingsSection>(initialSection);

  useEffect(() => {
    if (open) setSection(initialSection);
  }, [initialSection, open]);

  const refreshGateway = useCallback(async () => {
    const [status, logs, info] = await Promise.all([
      window.luma.gatewayCommand("status"),
      window.luma.getGatewayLogs(100),
      window.luma.getGatewayInfo(),
    ]);
    setGatewayStatus(status.trim() === "running" ? "running" : "stopped");
    setGatewayLogs(logs);
    setGatewayInfo(info);
    setGatewayError("");
  }, []);

  useEffect(() => {
    if (!open || section !== "gateway") return;
    let active = true;
    void refreshGateway().catch((reason: unknown) => {
      if (active) setGatewayError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { active = false; };
  }, [open, refreshGateway, section]);

  useEffect(() => {
    if (!open || section !== "general") return;
    let active = true;
    void window.luma.getCoreBinary().then((settings) => {
      if (!active) return;
      setCoreBinaryMode(settings.mode);
      setCoreBinaryPath(settings.customPath ?? "");
      setCoreBinaryCurrent(settings.binary);
      setCoreBinaryOverride(settings.environmentOverride);
      setCoreBinaryError("");
    }).catch((reason: unknown) => {
      if (active) setCoreBinaryError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { active = false; };
  }, [open, section]);

  async function applyCoreBinary() {
    setCoreBinaryBusy(true);
    setCoreBinaryError("");
    try {
      const settings = await window.luma.setCoreBinary({
        mode: coreBinaryMode,
        customPath: coreBinaryMode === "custom" ? coreBinaryPath : null,
      });
      setCoreBinaryCurrent(settings.binary);
      setCoreBinaryPath(settings.customPath ?? "");
      setCoreBinaryOverride(settings.environmentOverride);
    } catch (reason) {
      setCoreBinaryError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCoreBinaryBusy(false);
    }
  }

  async function chooseCoreBinary() {
    try {
      const selected = await window.luma.chooseCoreBinary();
      if (selected) {
        setCoreBinaryPath(selected);
        setCoreBinaryMode("custom");
        setCoreBinaryError("");
      }
    } catch (reason) {
      setCoreBinaryError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function manageGateway(command: "start" | "stop" | "restart") {
    setGatewayBusy(true);
    setGatewayError("");
    try {
      await window.luma.gatewayCommand(command);
      await refreshGateway();
    } catch (reason) {
      setGatewayError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setGatewayBusy(false);
    }
  }

  useEffect(() => {
    if (!open) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  return (
    <>
      <button
        type="button"
        aria-label="Close settings"
        onClick={onClose}
        className="luma-modal-overlay fixed inset-0 z-30 cursor-default bg-black/40"
      />

      <div
        ref={trapRef as React.RefObject<HTMLDivElement>}
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        aria-describedby="settings-description"
        className="
          luma-modal-content
          no-drag
          fixed left-1/2 top-1/2 z-40
          flex h-[560px] w-[720px]
          -translate-x-1/2 -translate-y-1/2
          overflow-hidden
          rounded-lg
          border border-[var(--luma-border-strong)]
          bg-[var(--luma-surface-raised)]
          shadow-lg ring-1 ring-[var(--luma-border)]
        "
      >
        <button type="button" onClick={onClose} aria-label="Close settings" title="Close settings" className="luma-settings-close luma-settings-close--corner">×</button>
        <div
          className="
            w-[140px] shrink-0
            border-r border-[var(--luma-border)]
            bg-[var(--luma-surface)]
            p-2
          "
        >
          <div className="mb-1 flex items-center justify-between px-2 py-1"><div id="settings-title" className="luma-kicker">Settings</div></div>
          <div id="settings-description" className="sr-only">
            Configure Luma's appearance, connection, and model settings
          </div>

          <SettingsNavButton
            active={section === "appearance"}
            onClick={() => setSection("appearance")}
          >
            Appearance
          </SettingsNavButton>

          <SettingsNavButton
            active={section === "general"}
            onClick={() => setSection("general")}
          >
            General
          </SettingsNavButton>
          <SettingsNavButton active={section === "gateway"} onClick={() => setSection("gateway")}>Gateway</SettingsNavButton>
          <SettingsNavButton
            active={section === "behavior"}
            onClick={() => setSection("behavior")}
          >
            Behavior
          </SettingsNavButton>
          <SettingsNavButton active={section === "extensions"} onClick={() => setSection("extensions")}>Extensions</SettingsNavButton>
        </div>

        <div className="min-w-0 flex-1 overflow-y-auto p-4">
          {section === "appearance" && (
            <>
          <div className="mb-4">
            <h2 className="text-[13px] font-medium text-[var(--luma-text)]">Appearance</h2>
            <p className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">
              Customize the look and feel of Luma.
            </p>
          </div>

              <div className="grid grid-cols-2 gap-2">
                {themes.map((item) => (
                  <ThemeCard
                    key={item.id}
                    theme={item}
                    selected={theme === item.id}
                    onClick={() => setTheme(item.id)}
                  />
                ))}
              </div>

              <div className="mt-4 flex items-center justify-between border-b border-[var(--luma-border)] py-3">
                <div><div className="text-[13px] font-medium">Accent color</div><div className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">Customize highlights and controls without changing surface colors.</div></div>
                <div className="flex items-center gap-2"><input aria-label="Custom accent color" type="color" value={accentColor || definition.accent} onChange={(event) => setAccentColor(event.target.value)} className="h-7 w-9 cursor-pointer rounded border-0 bg-transparent p-0" /><button type="button" onClick={() => setAccentColor("")} className="text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">Reset</button></div>
              </div>

      {window.luma.platform !== "linux" && <SettingsRow
        title="Sidebar blur"
        description="Use the native translucent surface"
        value={sidebarBlur ? "Enabled" : "Disabled"}
        actionLabel={sidebarBlur ? "Disable" : "Enable"}
        onAction={() => onSidebarBlurChange(!sidebarBlur)}
      />}

              <div
                className="
                  mt-3 rounded-md
                  border border-[var(--luma-border)]
                  bg-[var(--luma-surface)]
                  p-3
                "
              >
                <div className="flex items-center gap-2">
                  <div
                    className="
                      h-7 w-7 rounded-md
                      bg-[var(--luma-accent-soft)]
                    "
                  />

                  <div>
                    <div className="text-[13px] font-medium">
                      {definition.name}
                    </div>

                    <div className="text-[12px] text-[var(--luma-text-muted)]">
                      {definition.description}
                    </div>
                  </div>

                  <div
                    className="
                      ml-auto h-2 w-2 rounded-full
                      bg-[var(--luma-accent)]
                    "
                  />
                </div>
              </div>
            </>
          )}

          {section === "general" && (
            <>
              <div className="mb-4">
                <h2 className="text-[13px] font-medium text-[var(--luma-text)]">General</h2>
                <p className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">
                  Workspace, model, and connection settings.
                </p>
              </div>

              <div className="mb-3 border-b border-[var(--luma-border)] pb-4">
                <label className="block text-[12px] font-medium text-[var(--luma-text)]" htmlFor="luma-profile-name">What should Luma call you?</label>
                <input id="luma-profile-name" value={profileName} onChange={(event) => setProfileName(event.target.value)} maxLength={48} placeholder="User" autoComplete="nickname" className="luma-input mt-2 h-9 w-full text-[12px]" />
                <p className="mt-1.5 text-[10px] text-[var(--luma-text-muted)]">Used for your profile in the sidebar. Leave blank to use “User”.</p>
              </div>

              <SettingsRow
                title="Workspace"
                description="Files shown in the IDE panel"
                value={workspacePath || "Loading…"}
              />

              <SettingsRow
                title="Connection"
                description="Desktop server status"
                value={connected ? "Connected" : "Disconnected"}
              />

              <div className="mt-4 border-t border-[var(--luma-border)] pt-4">
                <div className="mb-3">
                  <h3 className="text-[13px] font-medium text-[var(--luma-text)]">Luma core runtime</h3>
                  <p className="mt-0.5 text-[11px] leading-5 text-[var(--luma-text-muted)]">Choose which luma-core executable powers this desktop app. Changing it restarts the core connection.</p>
                </div>
                <label htmlFor="luma-core-source" className="mb-1.5 block text-[11px] font-medium text-[var(--luma-text-secondary)]">Core source</label>
                <select id="luma-core-source" value={coreBinaryMode} onChange={(event) => setCoreBinaryMode(event.target.value as "bundled" | "path" | "custom")} className="luma-input h-9 w-full text-[12px]">
                  <option value="bundled">Bundled with Luma</option>
                  <option value="path">Global executable on PATH</option>
                  <option value="custom">Custom executable…</option>
                </select>
                {coreBinaryMode === "custom" && <div className="mt-2 flex gap-2">
                  <input aria-label="Custom Luma core executable path" readOnly value={coreBinaryPath} placeholder="Choose an executable file…" className="luma-input h-9 min-w-0 flex-1 text-[10px]" />
                  <button type="button" disabled={coreBinaryBusy} onClick={() => void chooseCoreBinary()} className="rounded-md border border-[var(--luma-border)] px-3 text-[11px] text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] disabled:opacity-40">Browse…</button>
                </div>}
                <div className="mt-2 break-all rounded-md bg-[var(--luma-surface)] px-2.5 py-2 font-mono text-[10px] leading-4 text-[var(--luma-text-muted)]">Current: {coreBinaryCurrent || "Loading…"}</div>
                {coreBinaryOverride && <p className="mt-1.5 text-[10px] text-[var(--luma-warning)]">LUMA_BINARY overrides this selection. Unset that environment variable before applying a different core source.</p>}
                {coreBinaryError && <p role="alert" className="mt-2 text-[10px] text-[var(--luma-danger)]">{coreBinaryError}</p>}
                <div className="mt-2 flex justify-end"><button type="button" disabled={coreBinaryBusy || coreBinaryOverride || (coreBinaryMode === "custom" && !coreBinaryPath.trim())} onClick={() => void applyCoreBinary()} className="rounded-md bg-[var(--luma-accent)] px-3 py-1.5 text-[11px] font-medium text-white disabled:cursor-not-allowed disabled:opacity-40">{coreBinaryBusy ? "Restarting core…" : "Apply and restart"}</button></div>
              </div>

              <SettingsRow
                title="Model"
                description={
                  setupConfig
                    ? setupConfig.model.provider
                    : "No provider configured"
                }
                value={setupConfig?.model.name ?? "Not configured"}
                actionLabel="Change"
                onAction={onChangeModel}
              />

              <details className="mt-3 rounded-md border border-[var(--luma-border)] p-3">
                <summary className="cursor-pointer text-[12px] font-semibold">Custom provider registry</summary>
                <p className="mt-2 text-[10px] leading-5 text-[var(--luma-text-muted)]">Add, edit, or remove custom model providers. Use the provider protocol supported by the endpoint. API credentials remain in the separate credential store.</p>
                <textarea spellCheck={false} aria-label="Custom provider definitions" value={customProvidersDraft} onChange={(event) => { setCustomProvidersDraft(event.target.value); setProviderSaveError(""); }} rows={10} className="luma-input mt-2 w-full resize-y font-mono text-[10px] leading-5" placeholder={'[\n  {\n    "id": "custom-my-server",\n    "name": "My server",\n    "description": "My model endpoint",\n    "category": "Custom",\n    "protocol": "OpenAiCompatible",\n    "endpoint": "http://localhost:1234/v1",\n    "requires_api_key": false,\n    "supports_model_discovery": true,\n    "api_key_env": null\n  }\n]'} />
                {providerSaveError && <p role="alert" className="mt-2 text-[10px] text-[var(--luma-danger)]">{providerSaveError}</p>}
                <div className="mt-2 flex justify-end"><button type="button" disabled={!setupConfig} onClick={() => {
                  try {
                    if (!setupConfig) throw new Error("Load the current model configuration before editing providers.");
                    const providers = JSON.parse(customProvidersDraft) as unknown;
                    if (!Array.isArray(providers) || providers.some((item) => !item || typeof item !== "object" || (item as { category?: unknown }).category !== "Custom")) {
                      throw new Error("Enter an array containing only custom provider definitions.");
                    }
                    const ids = providers.map((item) => (item as { id?: unknown }).id);
                    if (ids.some((id) => typeof id !== "string" || !/^custom-[a-z0-9][a-z0-9_-]{0,62}$/.test(id)) || new Set(ids).size !== ids.length) {
                      throw new Error("Each provider needs a unique ID beginning with custom-.");
                    }
                    void window.luma.saveSetupConfig({ ...setupConfig, providers: providers as NonNullable<SetupConfig["providers"]> })
                      .then(() => setProviderSaveError(""))
                      .catch((reason: unknown) => setProviderSaveError(reason instanceof Error ? reason.message : String(reason)));
                  } catch (reason) {
                    setProviderSaveError(reason instanceof Error ? reason.message : String(reason));
                  }
                }} className="luma-btn-primary h-7 px-3 text-[10px]">Save provider registry</button></div>
              </details>

              <SettingsRow
                title="Confirmations"
                description="Luma asks before running tools that change files"
                value="Required"
              />

              <SettingsRow
                title="Desktop tour"
                description="Take another look around the Luma interface"
                value="Available"
                actionLabel="Show tour"
                onAction={onOpenTour}
              />
              <SettingsRow
                title="Run setup again"
                description="Revisit your profile, provider, and model configuration"
                value="Available"
                actionLabel="Open setup"
                onAction={onRerunSetup}
              />
            </>
          )}

          {section === "behavior" && (
            <>
              <div className="mb-4">
                <h2 className="text-[13px] font-medium text-[var(--luma-text)]">Behavior</h2>
                <p className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">Tune motion and workspace density.</p>
              </div>
              <SettingsRow
                title="Compact history"
                description="Use denser conversation rows in the sidebar"
                value={compactHistory ? "Enabled" : "Disabled"}
                actionLabel={compactHistory ? "Disable" : "Enable"}
                onAction={() => onCompactHistoryChange(!compactHistory)}
              />
              <SettingsRow
                title="Reduce motion"
                description="Minimize interface transitions and animations"
                value={reducedMotion ? "Enabled" : "Disabled"}
                actionLabel={reducedMotion ? "Disable" : "Enable"}
                onAction={() => onReducedMotionChange(!reducedMotion)}
              />
              <SettingsRow
                title="Send with Enter"
                description="Send messages with Enter; use Shift+Enter for a new line"
                value={sendWithEnter ? "Enabled" : "Disabled"}
                actionLabel={sendWithEnter ? "Disable" : "Enable"}
                onAction={() => onSendWithEnterChange(!sendWithEnter)}
              />
              <SettingsRow
                title="Auto-scroll to latest message"
                description="Keep the Code conversation pinned to its newest activity"
                value={autoScrollToLatest ? "Enabled" : "Disabled"}
                actionLabel={autoScrollToLatest ? "Disable" : "Enable"}
                onAction={() => onAutoScrollToLatestChange(!autoScrollToLatest)}
              />
              <SettingsRow
                title="Show tool activity"
                description="Show tool runs and results in the Code conversation"
                value={showToolActivity ? "Shown" : "Hidden"}
                actionLabel={showToolActivity ? "Hide" : "Show"}
                onAction={() => onShowToolActivityChange(!showToolActivity)}
              />
            </>
          )}
          {section === "gateway" && (
            <>
              <div className="mb-4">
                <h2 className="text-[13px] font-medium text-[var(--luma-text)]">Gateway</h2>
                <p className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">Manage the persistent Luma core service that runs Teammates tasks and schedules.</p>
              </div>
              <div className="flex items-center justify-between rounded-md border border-[var(--luma-border)] px-3 py-2.5">
                <div><div className="text-[11px] font-medium">Gateway status</div><div className={`mt-0.5 text-[10px] ${gatewayStatus === "running" ? "text-[var(--luma-success)]" : "text-[var(--luma-text-muted)]"}`}>{gatewayStatus === "unknown" ? "Checking…" : gatewayStatus === "running" ? "Running" : "Stopped"}</div></div>
                <button type="button" onClick={() => void refreshGateway().catch((reason: unknown) => setGatewayError(reason instanceof Error ? reason.message : String(reason)))} disabled={gatewayBusy} className="luma-btn-secondary h-7 px-2.5 text-[10px]">Refresh</button>
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                <button type="button" disabled={gatewayBusy || gatewayStatus === "running"} onClick={() => void manageGateway("start")} className="luma-btn-primary h-8 px-3 text-[10px]">Start</button>
                <button type="button" disabled={gatewayBusy || gatewayStatus !== "running"} onClick={() => void manageGateway("stop")} className="luma-btn-secondary h-8 px-3 text-[10px]">Stop</button>
                <button type="button" disabled={gatewayBusy} onClick={() => void manageGateway("restart")} className="luma-btn-secondary h-8 px-3 text-[10px]">Restart</button>
              </div>
              {gatewayBusy && <p className="mt-2 text-[10px] text-[var(--luma-text-muted)]">Applying gateway command…</p>}
              {gatewayError && <p role="alert" className="mt-2 text-[10px] text-[var(--luma-danger)]">{gatewayError}</p>}
              {gatewayInfo && <div className="mt-4 space-y-2 rounded-md border border-[var(--luma-border)] p-3 text-[10px]"><div className="luma-kicker">SERVICE PATHS</div><div><div className="text-[var(--luma-text-muted)]">Control endpoint</div><div className="break-all font-mono text-[var(--luma-text-secondary)]">{gatewayInfo.endpoint}</div></div><div><div className="text-[var(--luma-text-muted)]">Runtime database</div><div className="break-all font-mono text-[var(--luma-text-secondary)]">{gatewayInfo.runtimeDatabase}</div></div><div><div className="text-[var(--luma-text-muted)]">Log file</div><div className="break-all font-mono text-[var(--luma-text-secondary)]">{gatewayInfo.logFile}</div></div></div>}
              <div className="mt-4"><div className="mb-2 flex items-center justify-between"><h3 className="luma-kicker">RECENT LOGS</h3><div className="flex gap-2"><button type="button" onClick={() => void window.luma.getGatewayLogs(100).then(setGatewayLogs).catch((reason: unknown) => setGatewayError(reason instanceof Error ? reason.message : String(reason)))} className="text-[10px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">Refresh logs</button><button type="button" onClick={() => void window.luma.openGatewayLog()} className="text-[10px] text-[var(--luma-accent)] hover:underline">Open log file</button></div></div><pre className="max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] p-2 font-mono text-[9px] leading-4 text-[var(--luma-text-muted)]">{gatewayLogs || "No gateway log output yet."}</pre></div>
            </>
          )}
          {section === "extensions" && <ExtensionsPanel />}
        </div>
      </div>
    </>
  );
}

function SettingsNavButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`
        mb-0.5 flex w-full items-center
        rounded-md px-2 py-1.5
        text-left text-[12px]
        transition-colors
        ${
          active
            ? `
              bg-[var(--luma-accent-soft)]
              text-[var(--luma-text)]
            `
            : `
              text-[var(--luma-text-muted)]
              hover:bg-[var(--luma-surface-hover)]
              hover:text-[var(--luma-text-secondary)]
            `
        }
      `}
    >
      {children}
    </button>
  );
}

function SettingsRow({
  title,
  description,
  value,
  actionLabel,
  onAction,
}: {
  title: string;
  description: string;
  value: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="flex items-center justify-between border-b border-[var(--luma-border)] py-3 last:border-none">
      <div className="min-w-0 pr-4">
        <div className="text-[13px] font-medium">{title}</div>
        <div className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">
          {description}
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <span className="text-[12px] text-[var(--luma-text-secondary)]">
          {value}
        </span>

        {actionLabel && onAction && (
          <button
            type="button"
            onClick={onAction}
            className="text-[12px] font-medium text-[var(--luma-accent)] hover:text-[var(--luma-accent-hover)]"
          >
            {actionLabel}
          </button>
        )}
      </div>
    </div>
  );
}

function ThemeCard({
  theme,
  selected,
  onClick,
}: {
  theme: (typeof themes)[number];
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`
        text-left
        rounded-md
        p-2.5
        transition-colors
        ${
          selected
            ? `
              bg-[var(--luma-accent-soft)]
              ring-1 ring-[var(--luma-accent)]
            `
            : `
              hover:bg-[var(--luma-surface-hover)]
            `
        }
      `}
    >
      <div className="flex items-center gap-2">
        <span
          className="h-2 w-2 rounded-full shrink-0"
          style={{
            backgroundColor: theme.accent,
          }}
        />

        <span className="text-[12px] font-medium">{theme.name}</span>
      </div>

      <div className="mt-1 text-[11px] leading-4 text-[var(--luma-text-muted)]">
        {theme.description}
      </div>
    </button>
  );
}

function filterWorkspace(nodes: WorkspaceNode[], query: string): WorkspaceNode[] {
  const normalized = query.trim().toLowerCase();

  if (!normalized) {
    return nodes;
  }

  const next: WorkspaceNode[] = [];

  for (const node of nodes) {
    if (node.type === "file") {
      if (
        node.name.toLowerCase().includes(normalized) ||
        node.path.toLowerCase().includes(normalized)
      ) {
        next.push(node);
      }

      continue;
    }

    const children = filterWorkspace(node.children, query);

    if (children.length > 0 || node.name.toLowerCase().includes(normalized)) {
      next.push({
        ...node,
        children,
      });
    }
  }

  return next;
}

function formatChatTime(timestamp: number) {
  const date = new Date(timestamp);
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();

  if (sameDay) {
    return date.toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
  }

  return date.toLocaleDateString([], {
    month: "short",
    day: "numeric",
  });
}

function SidebarSection({
  title,
  section,
  actionLabel,
  onAction,
  children,
}: {
  title: string;
  section?: "projects" | "pinned" | "today" | "earlier";
  actionLabel?: string;
  onAction?: () => void;
  children: React.ReactNode;
}) {
  return (
    <section className={`mb-3 ${section ? `luma-sidebar-section-${section}` : ""}`}>
      <div className="mb-1 flex h-6 items-center px-1">
        <span className="luma-kicker">{title}</span>

        {actionLabel && onAction && (
          <button
            type="button"
            onClick={onAction}
            className="
              ml-auto
              text-[11px]
              text-[var(--luma-text-muted)]
              hover:text-[var(--luma-text-secondary)]
            "
          >
            {actionLabel}
          </button>
        )}
      </div>

      {children}
    </section>
  );
}

function ChatRow({
  chat,
  active,
  projectName,
  onSelect,
  onTogglePin,
  onRename,
  onDelete,
}: {
  chat: Chat;
  active: boolean;
  projectName?: string;
  onSelect: () => void;
  onTogglePin: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={`
        luma-chat-row group mb-0.5 flex items-center
        rounded-md
        ${
          active
            ? "bg-[var(--luma-accent-soft)]"
            : "hover:bg-[var(--luma-surface-hover)]"
        }
      `}
    >
      <button
        type="button"
        onClick={onSelect}
        className="
          min-w-0 flex-1
          px-2 py-1.5
          text-left
        "
      >
        <div className="flex items-center gap-1.5">
          {chat.pinned && (
            <span className="text-[var(--luma-accent)]">
              <PinIcon filled />
            </span>
          )}

          <span
            className={`
              truncate text-[11px]
              ${active ? "text-[var(--luma-text)]" : "text-[var(--luma-text-secondary)]"}
            `}
          >
            {chat.title}
          </span>
        </div>

        <div className="mt-0 flex items-center gap-1.5 text-[10px] leading-4 text-[var(--luma-text-muted)]">
          {projectName && <span className="truncate">{projectName}</span>}

          {projectName && <span className="opacity-40">·</span>}

          <span>{formatChatTime(chat.updatedAt)}</span>
        </div>
      </button>

      <div className="mr-1 flex items-center gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
        <button
          type="button"
          onClick={onTogglePin}
          aria-label={chat.pinned ? 'Unpin chat' : 'Pin chat'}
          className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--luma-text-muted)] hover:text-[var(--luma-text-secondary)] focus:text-[var(--luma-text-secondary)]"
        >
          <PinIcon filled={chat.pinned} />
        </button>
        <button
          type="button"
          onClick={onRename}
          aria-label={`Rename chat ${chat.title}`}
          className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text-secondary)] focus:text-[var(--luma-text-secondary)]"
        >
          Rename
        </button>
        <button
          type="button"
          onClick={onDelete}
          aria-label={`Delete chat ${chat.title}`}
          className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-danger)] focus:text-[var(--luma-danger)]"
        >
          Delete
        </button>
      </div>
    </div>
  );
}

function Sidebar({ mode, onOpenChat, onOpenTeammates, onProjectFolderChange, teammatesSidebar, sidebarBlur, sidebarVisible, connected, onToggleSidebar, onSettings }: { mode: AppMode; onOpenChat: () => void; onOpenTeammates: () => void; onProjectFolderChange: (projectId: string, path: string) => Promise<void>; teammatesSidebar: React.ReactNode; sidebarBlur: boolean; sidebarVisible: boolean; connected: boolean; onToggleSidebar: () => void; onSettings: (section: SettingsSection) => void }) {
  const profileName = useProfileStore((state) => state.name);
  const displayName = getProfileDisplayName(profileName);
  const chats = useWorkspaceStore((state) => state.chats);
  const syncHistory = useWorkspaceStore((state) => state.syncHistory);
  const projects = useWorkspaceStore((state) => state.projects);
  const activeChatId = useWorkspaceStore((state) => state.activeChatId);
  const activeProjectId = useWorkspaceStore((state) => state.activeProjectId);
  const startNewChat = useWorkspaceStore((state) => state.startNewChat);
  const selectChat = useWorkspaceStore((state) => state.selectChat);
  const togglePin = useWorkspaceStore((state) => state.togglePin);
  const renameChat = useWorkspaceStore((state) => state.renameChat);
  const deleteChat = useWorkspaceStore((state) => state.deleteChat);
  const createProject = useWorkspaceStore((state) => state.createProject);
  const selectProject = useWorkspaceStore((state) => state.selectProject);
  const renameProject = useWorkspaceStore((state) => state.renameProject);
  const deleteProject = useWorkspaceStore((state) => state.deleteProject);

  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [projectsOpen, setProjectsOpen] = useState(true);
  const [creatingProject, setCreatingProject] = useState(false);
  const [projectName, setProjectName] = useState("");
  const [profileMenuOpen, setProfileMenuOpen] = useState(false);
  const profileMenuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!profileMenuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && !profileMenuRef.current?.contains(event.target)) setProfileMenuOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") setProfileMenuOpen(false); };
    document.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => { document.removeEventListener("pointerdown", onPointerDown); window.removeEventListener("keydown", onKeyDown); };
  }, [profileMenuOpen]);

  useEffect(() => {
    let mounted = true;
    const refreshHistory = async () => {
      try {
        const history = await window.luma.getHistory();
        if (mounted) syncHistory(history);
      } catch {
        // Keep the current sidebar usable while the core is starting or reconnecting.
      }
    };
    void refreshHistory();
    const timer = window.setInterval(() => void refreshHistory(), 4000);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, [syncHistory]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (mode === "teammates" || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      setSearchOpen((value) => !value);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mode]);

  const projectById = useMemo(() => {
    const map = new Map<string, string>();

    for (const project of projects) {
      map.set(project.id, project.name);
    }

    return map;
  }, [projects]);

  const visibleChats = useMemo(() => {
    const normalized = query.trim().toLowerCase();

    return chats
      .filter((chat) => {
        if (activeProjectId && chat.projectId !== activeProjectId) {
          return false;
        }

        if (!normalized) {
          return true;
        }

        const projectName = chat.projectId
          ? projectById.get(chat.projectId) ?? ""
          : "";

        return (
          chat.title.toLowerCase().includes(normalized) ||
          projectName.toLowerCase().includes(normalized)
        );
      })
      .sort((left, right) => right.updatedAt - left.updatedAt);
  }, [activeProjectId, chats, projectById, query]);

  const pinnedChats = visibleChats.filter((chat) => chat.pinned);
  const historyChats = visibleChats.filter((chat) => !chat.pinned);
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayChats = historyChats.filter((chat) => chat.updatedAt >= todayStart.getTime());
  const earlierChats = historyChats.filter((chat) => chat.updatedAt < todayStart.getTime());

  function submitProject() {
    const name = projectName.trim();

    if (!name) {
      setCreatingProject(false);
      setProjectName("");
      return;
    }

    createProject(name);
    setProjectName("");
    setCreatingProject(false);
    setProjectsOpen(true);
  }

  function handleRenameChat(chat: Chat) {
    const next = window.prompt("Rename chat", chat.title);

    if (next === null) {
      return;
    }

    renameChat(chat.id, next);
  }

  function handleDeleteChat(chat: Chat) {
    if (!window.confirm(`Delete “${chat.title}”?`)) {
      return;
    }

    deleteChat(chat.id);
  }

  function handleRenameProject(projectId: string, currentName: string) {
    const next = window.prompt("Rename project", currentName);

    if (next === null) {
      return;
    }

    renameProject(projectId, next);
  }

  function handleDeleteProject(projectId: string, currentName: string) {
    if (!window.confirm(`Delete project “${currentName}”? Chats stay in history.`)) {
      return;
    }

    deleteProject(projectId);
  }

  async function handleChooseProjectFolder(project: (typeof projects)[number]) {
    try {
      const result = await window.luma.chooseProjectWorkspace();
      if (result.canceled || !result.path) return;
      await onProjectFolderChange(project.id, result.path);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <aside
      className={`
        luma-sidebar flex w-full min-w-0 shrink-0 flex-col
        border-r border-[var(--luma-border)]
        ${sidebarBlur ? "luma-sidebar-glass" : "bg-[var(--luma-surface)]"}
      `}
      data-sidebar-blur={sidebarBlur}
      role="navigation"
      aria-label={mode === "teammates" ? "Teammates and workspace navigation" : "Chat and project navigation"}
    >
      <div className="luma-sidebar-brand drag-region">
        <div className="no-drag"><LumaWordmark compact section={mode === "teammates" ? "teammates" : undefined} /></div>
        <button type="button" className="no-drag luma-sidebar-icon-button" onClick={onToggleSidebar} aria-label={sidebarVisible ? "Hide sidebar" : "Show sidebar"} title={`${sidebarVisible ? "Hide" : "Show"} sidebar (${window.luma.platform === "darwin" ? "⌘" : "Ctrl"}+B)`}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></svg>
        </button>
      </div>
      <nav className="luma-primary-switcher no-drag" aria-label="Luma workspace" data-mode={mode === "teammates" ? "teammates" : "code"}>
        <span className="luma-primary-switcher-indicator" aria-hidden="true" />
        <button type="button" aria-current={mode !== "teammates" ? "page" : undefined} data-active={mode !== "teammates"} onClick={onOpenChat}>
          <span aria-hidden="true" className="luma-primary-switcher-icon">&lt;/&gt;</span>
          Code
        </button>
        <button type="button" aria-current={mode === "teammates" ? "page" : undefined} data-active={mode === "teammates"} onClick={onOpenTeammates}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="9" cy="10" r="5" /><circle cx="15" cy="14" r="5" /></svg>
          Teammates
        </button>
      </nav>

      {mode !== "teammates" && <div className="luma-sidebar-projects px-1.5">
        <SidebarSection
          title="Projects"
          section="projects"
          actionLabel="New"
          onAction={() => {
            setCreatingProject(true);
            setProjectsOpen(true);
          }}
        >
          <button
            type="button"
            onClick={() => setProjectsOpen((value) => !value)}
            className="
              mb-0.5 flex h-7 w-full items-center gap-1
              rounded-md px-1
              text-left text-[12px]
              text-[var(--luma-text-secondary)]
              hover:bg-[var(--luma-surface-hover)]
            "
          >
            <ChevronIcon open={projectsOpen} />
            <FolderIcon />
            <span className="truncate">
              {activeProjectId
                ? projectById.get(activeProjectId) ?? "Projects"
                : "All chats"}
            </span>
          </button>

          {projectsOpen && (
            <div className="mb-1">
              <button
                type="button"
                onClick={() => selectProject(null)}
                className={`
                  mb-0.5 flex h-7 w-full items-center
                  rounded-md px-2
                  text-left text-[12px]
                  ${
                    activeProjectId === null
                      ? "bg-[var(--luma-accent-soft)] text-[var(--luma-text)]"
                      : "text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)]"
                  }
                `}
              >
                All chats
              </button>

              {projects.map((project) => (
                <div
                  key={project.id}
                  className={`
                    group mb-0.5 flex items-center
                    rounded-md
                    ${
                      activeProjectId === project.id
                        ? "bg-[var(--luma-accent-soft)]"
                        : "hover:bg-[var(--luma-surface-hover)]"
                    }
                  `}
                >
                  <button
                    type="button"
                    onClick={() => selectProject(project.id)}
                    title={project.workspacePath ?? "Choose a folder for this Project"}
                    className={`
                      min-w-0 flex-1 truncate
                      px-2 py-1.5
                      text-left text-[12px]
                      ${
                        activeProjectId === project.id
                          ? "text-[var(--luma-text)]"
                          : "text-[var(--luma-text-secondary)]"
                      }
                    `}
                  >
                    <span className="truncate">{project.name}</span>
                    <span className="ml-2 shrink-0 text-[10px] text-[var(--luma-text-muted)]" aria-label={project.workspacePath ? "Folder assigned" : "Folder not assigned"}>
                      {project.workspacePath ? "Folder" : "Set folder"}
                    </span>
                  </button>

                  <div className="mr-1 flex gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
                    <button
                      type="button"
                      onClick={() => void handleChooseProjectFolder(project)}
                      aria-label={`Choose folder for ${project.name}`}
                      title={project.workspacePath ?? "Choose Project folder"}
                      className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-accent)] focus:text-[var(--luma-accent)]"
                    >
                      <FolderIcon />
                    </button>
                    <button
                      type="button"
                      onClick={() => handleRenameProject(project.id, project.name)}
                      aria-label={`Rename project ${project.name}`}
                      title={`Rename ${project.name}`}
                      className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text-secondary)] focus:text-[var(--luma-text-secondary)]"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m15 5 4 4M4 20l4-.8L19 8a2.1 2.1 0 0 0-3-3L5 16z" /></svg>
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteProject(project.id, project.name)}
                      aria-label={`Delete project ${project.name}`}
                      title={`Delete ${project.name}`}
                      className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-danger)] focus:text-[var(--luma-danger)]"
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 14h10l1-14M9 7V4h6v3" /></svg>
                    </button>
                  </div>
                </div>
              ))}

              {creatingProject && (
                <input
                  value={projectName}
                  onChange={(event) => setProjectName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      submitProject();
                    }

                    if (event.key === "Escape") {
                      setCreatingProject(false);
                      setProjectName("");
                    }
                  }}
                  onBlur={submitProject}
                  placeholder="Project name"
                  className="luma-input mt-1 h-8 w-full"
                  autoFocus
                />
              )}

              {!creatingProject && projects.length === 0 && (
                <div className="px-2 py-1 text-[12px] text-[var(--luma-text-muted)]">
                  No projects yet.
                </div>
              )}
            </div>
          )}
        </SidebarSection>
      </div>}

      {mode !== "teammates" && <div className="luma-new-chat px-4 pb-6">
        <button
          type="button"
          onClick={() => {
            startNewChat();
            onOpenChat();
          }}
          className="
            flex h-10 w-full items-center justify-center gap-2
            rounded-lg border border-[var(--luma-border-strong)]
            bg-[var(--luma-surface-raised)] px-4
            text-[13px] font-medium text-[var(--luma-text)]
            shadow-sm transition-colors
            hover:border-[var(--luma-accent)] hover:bg-[var(--luma-surface-hover)]
          "
        >
          <span className="text-[var(--luma-accent)]"><PlusIcon /></span>
          New session
          <span className="ml-auto text-[11px] font-normal text-[var(--luma-text-muted)]">{window.luma.platform === "darwin" ? "⌘ N" : "Ctrl N"}</span>
        </button>
        <button type="button" onClick={() => setSearchOpen((value) => !value)} aria-pressed={searchOpen} className="luma-sidebar-search">
          <SearchIcon /> <span>Search</span><kbd>{window.luma.platform === "darwin" ? "⌘K" : "Ctrl K"}</kbd>
        </button>
      </div>}

      {mode === "teammates" ? (
        <div className="min-h-0 flex-1 overflow-auto px-2.5 py-3">{teammatesSidebar}</div>
      ) : <>
      {searchOpen && (
        <div className="px-2.5 pb-2">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search chats"
            className="luma-input h-8 w-full"
            autoFocus
          />
        </div>
      )}

      <div className="luma-sidebar-history-list min-h-0 flex-1 overflow-auto px-1.5 pb-2">


        {pinnedChats.length > 0 && (
          <SidebarSection title="Pinned" section="pinned">
            {pinnedChats.map((chat) => (
              <ChatRow
                key={chat.id}
                chat={chat}
                active={chat.id === activeChatId}
                projectName={
                  chat.projectId ? projectById.get(chat.projectId) : undefined
                }
                onSelect={() => {
                  selectChat(chat.id);
                  onOpenChat();
                }}
                onTogglePin={() => togglePin(chat.id)}
                onRename={() => handleRenameChat(chat)}
                onDelete={() => handleDeleteChat(chat)}
              />
            ))}
          </SidebarSection>
        )}

        <SidebarSection title="Today" section="today">
          {todayChats.length === 0 ? (
            <div className="px-2 py-1 text-[12px] text-[var(--luma-text-muted)]">
              {query.trim()
                ? "No matching chats."
                : "Start a chat to see it here."}
            </div>
          ) : (
            todayChats.map((chat) => (
              <ChatRow
                key={chat.id}
                chat={chat}
                active={chat.id === activeChatId}
                projectName={
                  chat.projectId ? projectById.get(chat.projectId) : undefined
                }
                onSelect={() => {
                  selectChat(chat.id);
                  onOpenChat();
                }}
                onTogglePin={() => togglePin(chat.id)}
                onRename={() => handleRenameChat(chat)}
                onDelete={() => handleDeleteChat(chat)}
              />
            ))
          )}
        </SidebarSection>
        {earlierChats.length > 0 && <SidebarSection title="Earlier" section="earlier">
          {earlierChats.map((chat) => <ChatRow
            key={chat.id}
            chat={chat}
            active={chat.id === activeChatId}
            projectName={chat.projectId ? projectById.get(chat.projectId) : undefined}
            onSelect={() => { selectChat(chat.id); onOpenChat(); }}
            onTogglePin={() => togglePin(chat.id)}
            onRename={() => handleRenameChat(chat)}
            onDelete={() => handleDeleteChat(chat)}
          />)}
        </SidebarSection>}
      </div>
      </>}
      <footer className="luma-sidebar-footer no-drag">
        <div className="luma-profile-menu-anchor" ref={profileMenuRef}>
          <button type="button" className="luma-sidebar-account" aria-haspopup="menu" aria-expanded={profileMenuOpen} onClick={() => setProfileMenuOpen((open) => !open)}>
            <span className="luma-sidebar-account-avatar">{displayName.slice(0, 1).toLocaleUpperCase()}</span>
            <span className="min-w-0 flex-1 text-left"><strong>{displayName}</strong><small><i data-connected={connected} />{connected ? "Core connected" : "Core offline"}</small></span>
            <span className="luma-profile-chevron" aria-hidden="true">⌃</span>
          </button>
          {profileMenuOpen && <div className="luma-profile-menu" role="menu" aria-label="User menu">
            <div className="luma-profile-menu-heading"><strong>{displayName}</strong><span>Personal preferences</span></div>
            {([ ["appearance", "Appearance"], ["general", "General"], ["behavior", "Behavior"], ["gateway", "Gateway"], ["extensions", "Extensions"] ] as const).map(([section, label]) => <button key={section} type="button" role="menuitem" onClick={() => { setProfileMenuOpen(false); onSettings(section); }}><span>{label}</span>{section === "appearance" ? <span className="luma-profile-menu-shortcut">⌘ ,</span> : null}</button>)}
          </div>}
        </div>
      </footer>
    </aside>
  );
}

function WorkspaceTree({
  nodes,
  selectedFile,
  onFileSelect,
  depth = 0,
}: {
  nodes: WorkspaceNode[];
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  depth?: number;
}) {
  return (
    <>
      {nodes.map((node) => {
        if (node.type === "folder") {
          return (
            <FolderRow
              key={`${depth}-${node.name}`}
              node={node}
              selectedFile={selectedFile}
              onFileSelect={onFileSelect}
              depth={depth}
            />
          );
        }

        return (
          <button
            key={node.path}
            type="button"
            onClick={() => onFileSelect(node.path)}
            className={`
              flex h-7 w-full items-center gap-1.5
              rounded-md pr-2
              text-left text-[12px]
              ${
                selectedFile === node.path
                  ? `
                    bg-[var(--luma-accent-soft)]
                    text-[var(--luma-text)]
                  `
                  : `
                    text-[var(--luma-text-secondary)]
                    hover:bg-[var(--luma-surface-hover)]
                  `
              }
            `}
            style={{
              paddingLeft: 8 + depth * 14,
            }}
          >
            <FileIcon />

            <span className="truncate">{node.name}</span>
          </button>
        );
      })}
    </>
  );
}

function FolderRow({
  node,
  selectedFile,
  onFileSelect,
  depth,
}: {
  node: Extract<WorkspaceNode, { type: "folder" }>;
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  depth: number;
}) {
  const [open, setOpen] = useState(depth < 1);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="
          flex h-7 w-full items-center gap-1
          rounded-md pr-2
          text-left text-[12px]
          text-[var(--luma-text-secondary)]
          hover:bg-[var(--luma-surface-hover)]
        "
        style={{
          paddingLeft: 6 + depth * 14,
        }}
      >
        <ChevronIcon open={open} />

        <FolderIcon />

        <span className="truncate">{node.name}</span>
      </button>

      {open && (
        <WorkspaceTree
          nodes={node.children}
          selectedFile={selectedFile}
          onFileSelect={onFileSelect}
          depth={depth + 1}
        />
      )}
    </>
  );
}

function EmptyAgentState({ onPrompt }: { onPrompt: (prompt: string) => void }) {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
      <div className="luma-agent-empty pointer-events-auto max-w-[560px] px-6">
        <div
          className="
            luma-agent-empty-mark mx-auto mb-5
            flex h-11 w-11 items-center justify-center
            rounded-lg border border-[var(--luma-border-strong)]
            bg-transparent
            text-[var(--luma-accent)]
            shadow-none
          "
        >
          <LumaMark />
        </div>

        <div className="luma-kicker mb-3">CODE WORKSPACE</div>
        <h1 className="text-[25px] font-semibold tracking-[-0.04em]">
          What are we working on?
        </h1>

        <p className="mt-3 max-w-[390px] text-[13px] leading-6 text-[var(--luma-text-muted)]">
          Ask about this codebase, trace a problem, or describe the change you want to make.
        </p>

        <div className="luma-agent-prompts mt-7 flex flex-wrap gap-2">
          {[
            { label: "Understand this codebase", prompt: "Give me a high-level overview of this codebase and its architecture." },
            { label: "Find a bug", prompt: "Inspect this project for likely bugs and explain the most important findings." },
            { label: "Build a feature", prompt: "Help me plan and implement a feature in this project." },
          ].map((item) => (
            <button
              key={item.label}
              type="button"
              onClick={() => onPrompt(item.prompt)}
              className="
                rounded-md border border-[var(--luma-border-strong)]
                bg-[var(--luma-surface)] px-3 py-2
                text-[12px] text-[var(--luma-text-secondary)]
                transition-colors hover:border-[var(--luma-accent)]
                hover:bg-[var(--luma-accent-soft)] hover:text-[var(--luma-text)]
              "
            >
              {item.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function AgentMessageView({
  role,
  content,
  detailsExpanded = false,
}: {
  role: "user" | "assistant" | "error";
  content: string;
  detailsExpanded?: boolean;
}) {
  if (role === "error") {
    const [, ...details] = content.split("\n");
    const detailText = details.length > 0 ? details.join("\n") : content;
    return <div className="luma-tool-error" role="status">
      <span className="luma-tool-error-title">Failed to run tool</span>
      {detailsExpanded && detailText && <pre>{detailText}</pre>}
    </div>;
  }
  return (
    <div
      className={`luma-message-row flex gap-3 ${role === "user" ? "is-user" : "is-assistant"}`}
    >
      {role === "assistant" && (
        <div
          className="luma-message-mark mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-[var(--luma-accent)]"
        >
          <LumaMark />
        </div>
      )}

      <div
        className={`
          luma-message-content max-w-[760px]
          whitespace-pre-wrap
          text-[12px]
          leading-5
          ${
            role === "user"
              ? `
                text-[var(--luma-text)]
              `
              : "text-[var(--luma-text-secondary)] prose prose-sm prose-invert max-w-none"
          }
        `}
      >
        {role === "assistant" ? (
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              code({ className, children, ...props }) {
                return (
                  <code className={className} {...props}>
                    {children}
                  </code>
                );
              },
              pre({ children }) {
                return (
                  <pre className="my-2 overflow-x-auto rounded border border-[var(--luma-border)] bg-[var(--luma-surface)] p-3">
                    {children}
                  </pre>
                );
              },
              p({ children }) {
                return <p className="mb-2 last:mb-0">{children}</p>;
              },
              ul({ children }) {
                return <ul className="mb-2 ml-4 list-disc">{children}</ul>;
              },
              ol({ children }) {
                return <ol className="mb-2 ml-4 list-decimal">{children}</ol>;
              },
              li({ children }) {
                return <li className="mb-1">{children}</li>;
              },
              blockquote({ children }) {
                return (
                  <blockquote className="my-2 border-l-2 border-[var(--luma-border)] pl-3 italic">
                    {children}
                  </blockquote>
                );
              },
              h1({ children }) {
                return <h1 className="mb-2 mt-3 text-[14px] font-semibold">{children}</h1>;
              },
              h2({ children }) {
                return <h2 className="mb-2 mt-3 text-[13px] font-semibold">{children}</h2>;
              },
              h3({ children }) {
                return <h3 className="mb-2 mt-3 text-[12px] font-semibold">{children}</h3>;
              },
            }}
          >
            {content}
          </ReactMarkdown>
        ) : (
          <span>{content}</span>
        )}
      </div>
    </div>
  );
}

function ToolActivity({
  tools,
  errorCount,
  expanded,
  onToggle,
}: {
  tools: AgentTool[];
  errorCount: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  if (tools.length === 0 && errorCount === 0) {
    return null;
  }

  return (
    <section className="luma-tool-group my-2 rounded-md border border-[var(--luma-border)] px-2 py-1">
      <button type="button" aria-expanded={expanded} onClick={onToggle} className="flex w-full items-center gap-2 py-1 text-left text-[12px] text-[var(--luma-text-secondary)] hover:text-[var(--luma-text)]">
        <span aria-hidden="true">{expanded ? "⌄" : "›"}</span>
        <span>{toolActivityLabel(tools)}{errorCount > 0 ? ` · ${errorCount} failed` : ""}</span>
        <span className="ml-auto text-[var(--luma-text-muted)]">⌘O</span>
      </button>
      {expanded && <div role="list" aria-label="Code tool activity" className="space-y-2 pb-1">
      {tools.map((tool) => (
        <div
          key={tool.id}
          role="listitem"
          aria-label={`${tool.name} ${tool.finished ? 'completed' : 'running'}`}
          className="luma-tool-activity-row rounded px-2 py-1"
        >
          <div className="flex items-center gap-2">
          <span
            className={`
              h-1.5 w-1.5 rounded-full
              ${
                tool.finished
                  ? "bg-[var(--luma-success)]"
                  : "animate-pulse bg-[var(--luma-accent)]"
              }
            `}
          />

          <span className="text-[12px] text-[var(--luma-text-secondary)]">
            {tool.name}
          </span>

          {tool.durationMs !== undefined && (
            <span className="ml-auto text-[12px] text-[var(--luma-text-muted)]">
              {tool.durationMs}ms
            </span>
          )}
          </div>
          {tool.input && <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-words pl-3 font-mono text-[11px] leading-5 text-[var(--luma-text-muted)]">{tool.input}</pre>}
          {tool.diff && <pre className="mt-2 max-h-72 overflow-auto border-l border-[var(--luma-border)] pl-3 font-mono text-[11px] leading-5 text-[var(--luma-text-secondary)]">{tool.diff}</pre>}
        </div>
      ))}
      </div>}
    </section>
  );
}

function ThinkingIndicator() {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-label="Luma is thinking"
      className="flex items-center gap-2"
    >
      <div className="luma-thinking-mark"
      >
        <LumaMark />
      </div>

      <div className="flex items-center gap-1">
        <span className="h-1 w-1 animate-pulse rounded-full bg-[var(--luma-text-muted)]" />
        <span className="h-1 w-1 animate-pulse rounded-full bg-[var(--luma-text-muted)] [animation-delay:120ms]" />
        <span className="h-1 w-1 animate-pulse rounded-full bg-[var(--luma-text-muted)] [animation-delay:240ms]" />
      </div>
    </div>
  );
}

function ConfirmationCard({
  confirmation,
  onConfirm,
}: {
  confirmation: {
    name: string;
    input: string;
  };
  onConfirm: (allowed: boolean) => void;
}) {
  return (
    <div
      className="
        rounded-lg
        border border-[var(--luma-border-strong)]
        bg-[var(--luma-surface-raised)]
        p-3
      "
    >
      <div className="text-[13px] font-medium">Luma needs confirmation</div>

      <div className="mt-1 text-[12px] text-[var(--luma-text-muted)]">
        {confirmation.name}
      </div>

      {confirmation.input && (
        <pre
          className="
            mt-2 max-h-28 overflow-auto
            rounded-md
            bg-[var(--luma-bg)]
            p-2
            font-mono text-[12px]
            leading-4
            text-[var(--luma-text-secondary)]
          "
        >
          {confirmation.input}
        </pre>
      )}

      <div className="mt-2 flex gap-1.5">
        <button
          type="button"
          onClick={() => onConfirm(true)}
          className="
            rounded-md
            bg-[var(--luma-accent)]
            px-2.5 py-1.5
            text-[12px] font-medium
            text-black
          "
        >
          Allow
        </button>

        <button
          type="button"
          onClick={() => onConfirm(false)}
          className="
            rounded-md
            border border-[var(--luma-border)]
            px-2.5 py-1.5
            text-[12px]
            text-[var(--luma-text-secondary)]
            hover:bg-[var(--luma-surface-hover)]
          "
        >
          Deny
        </button>
      </div>
    </div>
  );
}

function QuestionCard({ question, onAnswer }: { question: { prompt: string; choices: string[] }; onAnswer: (answer: string) => void }) {
  const [answer, setAnswer] = useState("");
  const [selectedChoice, setSelectedChoice] = useState(0);
  const handleChoiceKeys = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!question.choices.length) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.currentTarget.parentElement?.querySelector<HTMLInputElement>("input")?.focus();
      return;
    }
    if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key)) {
      event.preventDefault();
      const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
      const next = (selectedChoice + direction + question.choices.length) % question.choices.length;
      setSelectedChoice(next);
      (event.currentTarget.querySelectorAll("button")[next] as HTMLButtonElement | undefined)?.focus();
    }
  };
  return <form className="rounded-lg border border-[var(--luma-border-strong)] bg-[var(--luma-surface-raised)] p-3" onSubmit={(event) => { event.preventDefault(); onAnswer(answer); }}>
    <div className="text-[13px] font-medium">Luma needs your input</div>
    <p className="mt-1 whitespace-pre-wrap text-[12px] leading-5 text-[var(--luma-text-secondary)]">{question.prompt}</p>
    {question.choices.length > 0 && <div className="mt-3 flex flex-wrap gap-2" role="group" aria-label="Suggested answers" onKeyDown={handleChoiceKeys}>{question.choices.map((choice, index) => <button key={choice} type="button" tabIndex={index === selectedChoice ? 0 : -1} onFocus={() => setSelectedChoice(index)} onClick={() => onAnswer(choice)} className="rounded-md border border-[var(--luma-border-strong)] px-2.5 py-1.5 text-[11px] text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)]">{choice}</button>)}</div>}
    <div className="mt-3 flex gap-2"><input autoFocus value={answer} onChange={(event) => setAnswer(event.target.value)} aria-label="Your answer" placeholder="Type your answer…" className="luma-input min-w-0 flex-1" /><button type="submit" disabled={!answer.trim()} className="rounded-md bg-[var(--luma-accent)] px-3 py-1.5 text-[11px] font-medium text-black disabled:opacity-40">Send</button></div>
  </form>;
}

function TaskQuestionCard({ task, busy, onAnswer }: { task: TaskRecord; busy: boolean; onAnswer: (taskId: string, questionId: string, answer: string) => Promise<void> }) {
  const [answer, setAnswer] = useState("");
  const [selectedChoice, setSelectedChoice] = useState(0);
  if (task.status !== "waiting_for_input" || !task.checkpoint || typeof task.checkpoint !== "object") return null;
  const question = task.checkpoint as { question_id?: unknown; prompt?: unknown; choices?: unknown };
  if (typeof question.question_id !== "string" || typeof question.prompt !== "string" || !Array.isArray(question.choices) || !question.choices.every((choice) => typeof choice === "string")) return null;
  const choices = question.choices as string[];
  const submit = (value: string) => { if (!busy && value.trim()) void onAnswer(task.id, question.question_id as string, value.trim()); };
  const handleChoiceKeys = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!choices.length) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.currentTarget.parentElement?.querySelector<HTMLInputElement>("input")?.focus();
      return;
    }
    if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key)) {
      event.preventDefault();
      const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
      const next = (selectedChoice + direction + choices.length) % choices.length;
      setSelectedChoice(next);
      (event.currentTarget.querySelectorAll("button")[next] as HTMLButtonElement | undefined)?.focus();
    }
  };
  return <div className="mt-3 rounded-lg border border-[var(--luma-warning)]/35 bg-[var(--luma-warning)]/5 p-3">
    <div className="text-[11px] font-semibold text-[var(--luma-warning)]">Luma needs your input</div>
    <p className="mt-1 whitespace-pre-wrap text-[12px] leading-5">{question.prompt}</p>
    {choices.length > 0 && <div className="mt-3 flex flex-wrap gap-2" role="group" aria-label="Suggested answers" onKeyDown={handleChoiceKeys}>{choices.map((choice, index) => <button key={choice} type="button" tabIndex={index === selectedChoice ? 0 : -1} disabled={busy} onFocus={() => setSelectedChoice(index)} onClick={() => submit(choice)} className="rounded-md border border-[var(--luma-border-strong)] px-2.5 py-1.5 text-[11px] hover:bg-[var(--luma-surface-hover)] disabled:opacity-50">{choice}</button>)}</div>}
    <form className="mt-3 flex gap-2" onSubmit={(event) => { event.preventDefault(); submit(answer); }}><input value={answer} onChange={(event) => setAnswer(event.target.value)} maxLength={8000} placeholder="Type your answer…" className="luma-input min-w-0 flex-1" /><button type="submit" disabled={busy || !answer.trim()} className="luma-btn-primary h-8 px-3 text-[11px] disabled:opacity-40">Answer</button></form>
  </div>;
}

function ErrorMessage({ error }: { error: string }) {
  return (
    <div
      className="
        rounded-md
        border border-[var(--luma-danger)]
        bg-[var(--luma-danger)]/5
        px-3 py-2
        text-[12px]
        text-[var(--luma-danger)]
      "
    >
      {error}
    </div>
  );
}

function resizePrompt(textarea: HTMLTextAreaElement | null, minHeight: number, maxHeight: number) {
  if (!textarea) return;
  textarea.style.height = `${minHeight}px`;
  const height = Math.min(textarea.scrollHeight, maxHeight);
  textarea.style.height = `${height}px`;
  textarea.style.overflowY = textarea.scrollHeight > maxHeight ? "auto" : "hidden";
}

function PromptInput({ inputRef, value, onChange, onKeyDown, placeholder, disabled = false, maxHeight = 112 }: {
  inputRef: RefObject<HTMLTextAreaElement | null>;
  value: string;
  onChange: (value: string) => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  placeholder: string;
  disabled?: boolean;
  maxHeight?: number;
}) {
  useEffect(() => resizePrompt(inputRef.current, 24, maxHeight), [inputRef, maxHeight, value]);
  return <textarea
    ref={inputRef}
    value={value}
    onChange={(event) => onChange(event.target.value)}
    onKeyDown={onKeyDown}
    placeholder={placeholder}
    disabled={disabled}
    rows={1}
    className="luma-prompt-input"
  />;
}

function ComposerQuickSettings({ sendWithEnter, onSendWithEnterChange, autoScrollToLatest, onAutoScrollToLatestChange, showToolActivity, onShowToolActivityChange, onOpenSettings, showCodeOptions = true }: {
  sendWithEnter: boolean;
  onSendWithEnterChange: (value: boolean) => void;
  autoScrollToLatest: boolean;
  onAutoScrollToLatestChange: (value: boolean) => void;
  showToolActivity: boolean;
  onShowToolActivityChange: (value: boolean) => void;
  onOpenSettings: () => void;
  showCodeOptions?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    menuRef.current?.querySelector<HTMLElement>("input, button")?.focus();
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!menuRef.current?.contains(event.target as Node)) setOpen(false); };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", close); document.removeEventListener("keydown", key); };
  }, [open]);
  return <div className="luma-composer-quick-settings" ref={menuRef}>
    <button ref={triggerRef} type="button" aria-label="Prompt settings" title="Quick settings" aria-expanded={open} onKeyDown={(event) => {
      if (["ArrowDown", "Enter", " "].includes(event.key)) { event.preventDefault(); setActiveIndex(0); setOpen(true); }
    }} onClick={() => setOpen((value) => !value)}>···</button>
    {open && <div autoFocus tabIndex={-1} className="luma-composer-quick-menu" role="menu" aria-label="Prompt settings" onKeyDown={(event) => {
      const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("input, button"));
      if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key) && items.length) {
        event.preventDefault();
        const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
        const next = (activeIndex + direction + items.length) % items.length;
        setActiveIndex(next); items[next].focus();
      } else if (event.key === "Enter" && items[activeIndex] instanceof HTMLInputElement) {
        const target = items[activeIndex] as HTMLInputElement;
        event.preventDefault();
        target.click();
      } else if (event.key === "Escape") { event.preventDefault(); setOpen(false); triggerRef.current?.focus(); }
    }}>
      <label><input type="checkbox" checked={sendWithEnter} onChange={(event) => onSendWithEnterChange(event.target.checked)} /> Enter to send</label>
      {showCodeOptions && <>
        <label><input type="checkbox" checked={autoScrollToLatest} onChange={(event) => onAutoScrollToLatestChange(event.target.checked)} /> Auto-scroll to latest</label>
        <label><input type="checkbox" checked={showToolActivity} onChange={(event) => onShowToolActivityChange(event.target.checked)} /> Show tool activity</label>
      </>}
      <button type="button" onClick={() => { setOpen(false); onOpenSettings(); }}>Open settings…</button>
    </div>}
  </div>;
}

function CodeModelPicker({ onChangeModel }: { onChangeModel: (scope: "global" | "session") => void }) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')[activeIndex]?.focus();
  }, [activeIndex, open]);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => { if (!rootRef.current?.contains(event.target as Node)) setOpen(false); };
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") setOpen(false); };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("pointerdown", close); document.removeEventListener("keydown", key); };
  }, [open]);
  return <div className="luma-composer-model-picker" ref={rootRef}>
    <button type="button" className="luma-composer-model-button" aria-haspopup="menu" aria-expanded={open} onKeyDown={(event) => {
      if (["ArrowDown", "Enter", " "].includes(event.key)) { event.preventDefault(); setActiveIndex(0); setOpen(true); }
    }} onClick={() => setOpen((value) => !value)}>Model <span aria-hidden="true">⌄</span></button>
    {open && <div ref={menuRef} className="luma-composer-model-menu" role="menu" onKeyDown={(event) => {
      if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key)) {
        event.preventDefault();
        const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
        const next = (activeIndex + direction + 2) % 2;
        setActiveIndex(next);
        event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')[next]?.focus();
      }
      else if (event.key === "Escape") { event.preventDefault(); setOpen(false); rootRef.current?.querySelector("button")?.focus(); }
    }}>
      <button type="button" role="menuitem" tabIndex={activeIndex === 0 ? 0 : -1} onFocus={() => setActiveIndex(0)} onClick={() => { setOpen(false); onChangeModel("global"); }}>Default model</button>
      <button type="button" role="menuitem" tabIndex={activeIndex === 1 ? 0 : -1} onFocus={() => setActiveIndex(1)} onClick={() => { setOpen(false); onChangeModel("session"); }}>This conversation</button>
    </div>}
  </div>;
}

function Composer({
  thinking,
  preparing,
  onSend,
  onCommand,
  onNewChat,
  onQuit,
  onCancel,
  value,
  onChange,
  onChangeModel,
  sendWithEnter,
  onSendWithEnterChange,
  autoScrollToLatest,
  onAutoScrollToLatestChange,
  showToolActivity,
  onShowToolActivityChange,
  onOpenSettings,
}: {
  thinking: boolean;
  preparing: boolean;
  onSend: (text: string) => void;
  onCommand: (text: string) => void;
  onNewChat: () => void;
  onQuit: () => void;
  onCancel: () => void;
  value: string;
  onChange: (value: string) => void;
  onChangeModel: (scope: "global" | "session") => void;
  sendWithEnter: boolean;
  onSendWithEnterChange: (value: boolean) => void;
  autoScrollToLatest: boolean;
  onAutoScrollToLatestChange: (value: boolean) => void;
  showToolActivity: boolean;
  onShowToolActivityChange: (value: boolean) => void;
  onOpenSettings: () => void;
}) {
  const [selectedCommand, setSelectedCommand] = useState(0);
  const [suggestionsDismissed, setSuggestionsDismissed] = useState(false);
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentError, setAttachmentError] = useState("");
  const [attaching, setAttaching] = useState(false);
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const matches = useMemo(() => {
    if (suggestionsDismissed || !value.startsWith("/")) return [];
    const query = value.toLowerCase();
    return SLASH_COMMANDS.filter((item) => item.command.startsWith(query));
  }, [suggestionsDismissed, value]);

  useEffect(() => setSelectedCommand(0), [value]);
  function chooseCommand(command: string) {
    onChange(`${command} `);
  }

  function submit() {
    const text = value.trim() || (attachments.length ? "Please inspect the attached files." : "");

    if (!text) {
      return;
    }

    const modelCommand = text.match(/^\/model(?:\s+(session))?$/i);
    const localNewChat = /^\/(?:new|clear)$/i.test(text);
    const localQuit = /^\/(?:quit|exit)$/i.test(text);
    if (attachments.length === 0 && modelCommand) {
      onChangeModel(modelCommand[1] ? "session" : "global");
    } else if (attachments.length === 0 && localNewChat) {
      onNewChat();
    } else if (attachments.length === 0 && localQuit) {
      onQuit();
    } else if (attachments.length === 0 && text.startsWith("/")) {
      onCommand(text);
    } else {
      onSend(formatChatPrompt(text, attachments));
    }
    setAttachments([]);
    setAttachmentError("");
    onChange("");
  }

  async function attachFiles() {
    setAttaching(true);
    setAttachmentError("");
    try {
      const selected = await window.luma.chooseChatAttachments();
      const next = [...attachments, ...selected];
      const totalBytes = next.reduce((total, item) => total + new TextEncoder().encode(item.content).byteLength, 0);
      if (next.length > 8 || totalBytes > 4 * 1024 * 1024) {
        throw new Error("A chat can include up to 8 text files and 4 MiB of content.");
      }
      setAttachments(next);
    } catch (reason) {
      setAttachmentError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setAttaching(false);
    }
  }

  return (
    <div className="luma-composer-shell border-t border-[var(--luma-border)] bg-[var(--luma-surface)] px-4 pb-4 pt-3">
      {attachments.length > 0 && <div className="mx-auto mb-2 flex max-w-[860px] flex-wrap gap-1.5">
        {attachments.map((attachment, index) => <span key={`${attachment.name}-${index}`} className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-[var(--luma-border)] px-2 py-1 text-[10px] text-[var(--luma-text-secondary)]"><span className="max-w-56 truncate">{attachment.name}</span><button type="button" aria-label={`Remove ${attachment.name}`} onClick={() => setAttachments((items) => items.filter((_, itemIndex) => itemIndex !== index))} className="text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">×</button></span>)}
      </div>}
      {attachmentError && <p role="alert" className="mx-auto mb-2 max-w-[860px] text-[10px] text-[var(--luma-danger)]">{attachmentError}</p>}
      {matches.length > 0 && !thinking && (
        <div className="luma-command-menu mx-auto mb-2 max-h-64 w-full max-w-[860px] overflow-y-auto rounded-lg border border-[var(--luma-border-strong)] bg-[var(--luma-surface-raised)] shadow-xl">
          {matches.map((item, index) => (
            <button
              key={item.command}
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => chooseCommand(item.command)}
              className={`flex w-full items-center gap-3 px-3 py-2 text-left text-[12px] ${index === selectedCommand ? "bg-[var(--luma-accent-soft)] text-[var(--luma-text)]" : "text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)]"}`}
            >
              <code className="min-w-[104px] text-[var(--luma-accent)]">{item.command}</code>
              <span>{item.description}</span>
              {index === selectedCommand && <span className="ml-auto text-[10px] text-[var(--luma-text-muted)]">↵</span>}
            </button>
          ))}
        </div>
      )}
      <div
        className="luma-composer
          mx-auto flex max-w-[860px]
          items-end gap-2
          rounded-lg
          border border-[var(--luma-border-strong)]
          bg-[var(--luma-surface-raised)]
          px-2.5 py-1.5
          shadow-sm transition-colors
          focus-within:border-[var(--luma-accent)]
        "
      >
        <button type="button" onClick={() => void attachFiles()} disabled={thinking || preparing || attaching} aria-label="Attach files" title="Attach text files" className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)] disabled:opacity-40"><PlusIcon /></button>
        <PromptInput
          inputRef={promptRef}
          value={value}
          onChange={(nextValue) => { setSuggestionsDismissed(false); onChange(nextValue); }}
          onKeyDown={(event) => {
            if (event.key === "Escape" && matches.length > 0) {
              event.preventDefault();
              setSuggestionsDismissed(true);
              return;
            }
            if (matches.length > 0 && ["ArrowDown", "ArrowRight"].includes(event.key)) {
              event.preventDefault();
              setSelectedCommand((current) => (current + 1) % matches.length);
              return;
            }
            if (matches.length > 0 && ["ArrowUp", "ArrowLeft"].includes(event.key)) {
              event.preventDefault();
              setSelectedCommand((current) => (current - 1 + matches.length) % matches.length);
              return;
            }
            if (matches.length > 0 && event.key === "Tab") {
              event.preventDefault();
              chooseCommand(matches[selectedCommand].command);
              return;
            }
            if (event.key === "Enter" && !event.shiftKey && sendWithEnter) {
              event.preventDefault();
              if (matches.length > 0) {
                const selected = matches[selectedCommand].command;
                if (selected !== value.trim()) {
                  chooseCommand(selected);
                  return;
                }
              }
              submit();
            }
          }}
          placeholder={preparing ? "Preparing a new conversation…" : thinking ? "Luma is working..." : "Ask Luma anything..."}
          disabled={thinking || preparing}
          maxHeight={112}
        />

        <CodeModelPicker onChangeModel={onChangeModel} />
        <ComposerQuickSettings sendWithEnter={sendWithEnter} onSendWithEnterChange={onSendWithEnterChange} autoScrollToLatest={autoScrollToLatest} onAutoScrollToLatestChange={onAutoScrollToLatestChange} showToolActivity={showToolActivity} onShowToolActivityChange={onShowToolActivityChange} onOpenSettings={onOpenSettings} />

        {thinking ? (
          <button
            type="button"
            onClick={onCancel}
            className="
              flex h-8 w-8 shrink-0
              items-center justify-center
              rounded-md
              bg-[var(--luma-surface-hover)]
              text-[var(--luma-text-secondary)]
              hover:text-[var(--luma-text)]
            "
            aria-label="Stop"
          >
            <StopIcon />
          </button>
        ) : (
          <button
            type="button"
            onClick={submit}
            disabled={preparing || (!value.trim() && attachments.length === 0)}
            className="
              flex h-8 w-8 shrink-0
              items-center justify-center
              rounded-md
              bg-[var(--luma-accent)]
              text-black
              disabled:cursor-default
              disabled:opacity-30
            "
            aria-label="Send"
          >
            <SendIcon />
          </button>
        )}
      </div>

      <div className="mx-auto mt-1 max-w-[860px] px-1 text-[10px] text-[var(--luma-text-muted)]">{sendWithEnter ? "Enter to send · Shift+Enter for a new line" : "Enter for a new line · Use Send to submit"}</div>
    </div>
  );
}

function AgentView({ onChangeModel, sendWithEnter, onSendWithEnterChange, autoScrollToLatest, onAutoScrollToLatestChange, showToolActivity, onShowToolActivityChange, onOpenSettings, workspacePath, onToggleInspector, inspectorVisible, onOpenEditor, handoffPrompt, onHandoffConsumed, titlebarStatus }: { onChangeModel: (scope: "global" | "session") => void; sendWithEnter: boolean; onSendWithEnterChange: (value: boolean) => void; autoScrollToLatest: boolean; onAutoScrollToLatestChange: (value: boolean) => void; showToolActivity: boolean; onShowToolActivityChange: (value: boolean) => void; onOpenSettings: () => void; workspacePath: string; onToggleInspector: () => void; inspectorVisible: boolean; onOpenEditor: () => void; handoffPrompt: string; onHandoffConsumed: () => void; titlebarStatus: TitlebarStatus | null }) {
  const [draft, setDraft] = useState("");
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  const [toolGroupExpansion, setToolGroupExpansion] = useState<Record<string, boolean>>({});
  const [branch, setBranch] = useState("");
  const transcriptScrollRef = useRef<HTMLDivElement>(null);
  const {
    sessionId,
    messages,
    tools,
    thinking,
    error,
    confirmation,
    question,
    sendPrompt,
    runCommand,
    respondToConfirmation,
    respondToQuestion,
    cancel,
  } = useLuma();

  useEffect(() => {
    if (!handoffPrompt) return;
    setDraft(handoffPrompt);
    onHandoffConsumed();
  }, [handoffPrompt, onHandoffConsumed]);

  useEffect(() => {
    if (!autoScrollToLatest) return;
    const container = transcriptScrollRef.current;
    if (container) container.scrollTop = container.scrollHeight;
  }, [autoScrollToLatest, messages, tools, thinking, error, confirmation]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "o") return;
      event.preventDefault();
      const latestGroup = [...messages].reverse().find((message) => message.role === "tools");
      if (latestGroup) {
        setToolGroupExpansion((current) => ({
          ...current,
          [latestGroup.id]: !(current[latestGroup.id] ?? false),
        }));
      } else {
        setDetailsExpanded((value) => !value);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [messages]);

  const chats = useWorkspaceStore((state) => state.chats);
  const activeChatId = useWorkspaceStore((state) => state.activeChatId);
  const isPendingNewChat = useWorkspaceStore((state) => state.isPendingNewChat);
  const draftChat = useWorkspaceStore((state) => state.draft);
  const historyLoaded = useWorkspaceStore((state) => state.historyLoaded);
  const setActiveChatId = useWorkspaceStore((state) => state.setActiveChatId);
  const setChatMessages = useWorkspaceStore((state) => state.setChatMessages);
  const startNewChat = useWorkspaceStore((state) => state.startNewChat);
  const selectChat = useWorkspaceStore((state) => state.selectChat);
  const activeChat = chats.find((chat) => chat.id === activeChatId);

  useEffect(() => {
    let active = true;
    const refreshBranch = () => void window.luma.getGitChanges().then((result) => {
      if (active) setBranch(result.branch);
    }).catch(() => {
      if (active) setBranch("");
    });
    refreshBranch();
    const timer = window.setInterval(refreshBranch, 10000);
    return () => { active = false; window.clearInterval(timer); };
  }, [workspacePath]);

  const handleCommand = useCallback((text: string) => {
    const restore = text.match(/^\/restore\s+(\S+)$/i);
    if (restore && chats.some((chat) => chat.id === restore[1])) {
      selectChat(restore[1]);
      return;
    }
    void runCommand(text);
  }, [chats, runCommand, selectChat]);
  const preparingNewChat = isPendingNewChat && (!sessionId || sessionId !== activeChatId);

  const loadedChatId = useRef<string | null>(null);
  const skipPersist = useRef(false);
  const clearRequestedForDraft = useRef<string | null>(null);
  const previousSessionForDraft = useRef<string | null>(null);

  useEffect(() => {
    if (!historyLoaded) {
      return;
    }

    if (!activeChatId) return;

    if (draftChat?.id === activeChatId) {
      if (clearRequestedForDraft.current !== draftChat.id) {
        if (thinking || confirmation !== null) void cancel();
        previousSessionForDraft.current = sessionId;
        clearRequestedForDraft.current = draftChat.id;
        skipPersist.current = true;
        loadedChatId.current = draftChat.id;
        void window.luma.clearSession();
        return;
      }

      // /clear is serialized before get_session in the main process. Wait for
      // its new core chat ID before associating the in-memory draft with it.
      if (sessionId && sessionId !== previousSessionForDraft.current && sessionId !== draftChat.id) {
        clearRequestedForDraft.current = sessionId;
        loadedChatId.current = sessionId;
        setActiveChatId(sessionId);
      }
      return;
    }

    if (loadedChatId.current === activeChatId) {
      return;
    }

    const chat = chats.find((item) => item.id === activeChatId);

    if (chat) {
      if (thinking || confirmation !== null) void cancel();
      skipPersist.current = true;
      loadedChatId.current = activeChatId;
      void window.luma.restoreChat(activeChatId);
      return;
    }

    if (sessionId) setActiveChatId(sessionId);
  }, [activeChatId, cancel, chats, confirmation, draftChat, historyLoaded, sessionId, setActiveChatId, thinking]);

  useEffect(() => {
    if (!activeChatId || loadedChatId.current !== activeChatId) {
      return;
    }

    if (skipPersist.current) {
      skipPersist.current = false;
      return;
    }

    setChatMessages(activeChatId, messages);
  }, [activeChatId, messages, setChatMessages]);

  const hasContent =
    messages.length > 0 ||
    tools.length > 0 ||
    thinking ||
    error !== null ||
    confirmation !== null || question !== null;

  // A pending chat is still an empty conversation: show the welcome state
  // until the first prompt is sent instead of leaving a blank transcript.
  const showEmptyState = preparingNewChat || !hasContent;
  const visibleStatus = titlebarStatus ?? { text: confirmation ? "Needs approval" : question ? "Needs input" : thinking ? "Working" : "Ready", color: confirmation || question ? "amber" as const : thinking ? "blue" as const : "green" as const };

  return (
    <div className="luma-agent-view relative flex h-full flex-col">
      <header className="luma-conversation-header shrink-0">
        <div className="min-w-0">
          <div className="truncate text-[13px] font-semibold">{activeChat?.title || "New session"}</div>
          <div className="luma-conversation-location truncate">{workspacePath ? `${workspacePath.split(/[\\/]/).filter(Boolean).pop() || workspacePath}${branch ? ` · ${branch}` : ""}` : "Choose a project workspace"}</div>
        </div>
        <span className={`luma-activity-pill ${thinking ? "is-working" : ""}`} data-status-color={visibleStatus.color}><i />{visibleStatus.text}</span>
        <div className="ml-auto flex items-center gap-1">
          <button type="button" className="luma-conversation-action" onClick={onOpenEditor} title="Open workspace in editor">Open in editor</button>
          <button type="button" className="luma-inspector-icon-button" onClick={onToggleInspector} aria-label={inspectorVisible ? "Hide inspector" : "Show inspector"} title={inspectorVisible ? "Hide inspector" : "Show inspector"}><PanelCloseIcon /></button>
        </div>
      </header>
      <div ref={transcriptScrollRef} className="min-h-0 flex-1 overflow-auto">
        {showEmptyState && (
          <>
            <EmptyAgentState onPrompt={setDraft} />
          </>
        )}

        {!showEmptyState && (
          <div className="luma-agent-transcript mx-auto max-w-[720px] space-y-4 px-6 py-5">
            {messages.filter((message) => showToolActivity || message.role !== "tools").map((message) => message.role === "tools" ? (
              <ToolActivity
                key={message.id}
                tools={message.tools ?? []}
                errorCount={message.tools?.filter((tool) => tool.failed).length ?? 0}
                expanded={toolGroupExpansion[message.id] ?? false}
                onToggle={() => setToolGroupExpansion((current) => ({
                  ...current,
                  [message.id]: !(current[message.id] ?? false),
                }))}
              />
            ) : (
              <AgentMessageView
                key={message.id}
                role={message.role}
                content={message.content}
                detailsExpanded={detailsExpanded}
              />
            ))}

            {thinking && <ThinkingIndicator />}

            {confirmation && (
              <ConfirmationCard
                confirmation={confirmation}
                onConfirm={respondToConfirmation}
              />
            )}

            {question && <QuestionCard question={question} onAnswer={respondToQuestion} />}

            {error && <ErrorMessage error={error} />}
          </div>
        )}
      </div>

      <Composer
        thinking={thinking}
        preparing={preparingNewChat}
        onSend={sendPrompt}
        onCommand={handleCommand}
        onNewChat={startNewChat}
        onQuit={() => void window.luma.quit()}
        onCancel={cancel}
        value={draft}
        onChange={setDraft}
        onChangeModel={onChangeModel}
        sendWithEnter={sendWithEnter}
        onSendWithEnterChange={onSendWithEnterChange}
        autoScrollToLatest={autoScrollToLatest}
        onAutoScrollToLatestChange={onAutoScrollToLatestChange}
        showToolActivity={showToolActivity}
        onShowToolActivityChange={onShowToolActivityChange}
        onOpenSettings={onOpenSettings}
      />
    </div>
  );
}

function getLanguage(file: string | null) {
  if (!file) {
    return "plaintext";
  }

  if (file.endsWith(".rs")) {
    return "rust";
  }

  if (file.endsWith(".ts") || file.endsWith(".tsx")) {
    return "typescript";
  }

  if (file.endsWith(".js") || file.endsWith(".jsx")) {
    return "javascript";
  }

  if (file.endsWith(".json")) {
    return "json";
  }

  if (file.endsWith(".css")) {
    return "css";
  }

  if (file.endsWith(".html")) {
    return "html";
  }

  if (file.endsWith(".md")) {
    return "markdown";
  }

  if (file.endsWith(".toml")) {
    return "ini";
  }

  if (file.endsWith(".yml") || file.endsWith(".yaml")) {
    return "yaml";
  }

  return "plaintext";
}

function FilesPanel({
  selectedFile,
  onFileSelect,
  nodes,
  workspacePath,
  onChooseWorkspace,
}: {
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  nodes: WorkspaceNode[];
  workspacePath: string;
  onChooseWorkspace: () => void;
}) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");

  const visible = filterWorkspace(nodes, query);

  return (
    <aside
      className="
        flex w-[220px] shrink-0 flex-col
        border-r border-[var(--luma-border)]
        bg-[var(--luma-surface)]
      "
      role="navigation"
      aria-label="File explorer"
    >
      <div className="flex h-9 items-center px-2.5">
        <span className="luma-kicker">Files</span>

        <div className="ml-auto flex items-center">
          <button
            type="button"
            onClick={onChooseWorkspace}
            title="Choose workspace folder"
            aria-label="Choose workspace folder"
            className="flex h-6 items-center rounded px-1.5 text-[10px] text-[var(--luma-text-muted)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)]"
          >
            Open folder
          </button>
          <button
            type="button"
            className="
              flex h-6 w-6 items-center justify-center
              rounded-md
              text-[var(--luma-text-muted)]
              hover:bg-[var(--luma-surface-hover)]
              hover:text-[var(--luma-text-secondary)]
            "
            aria-label="Search files"
            aria-pressed={searchOpen}
            onClick={() => setSearchOpen((value) => !value)}
          >
            <SearchIcon />
          </button>
        </div>
      </div>

      {searchOpen && (
        <div className="px-2.5 pb-2">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter files"
            className="luma-input h-8 w-full"
            autoFocus
          />
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto px-1.5 pb-2">
        <WorkspaceTree
          nodes={visible}
          selectedFile={selectedFile}
          onFileSelect={onFileSelect}
        />
      </div>

      <div
        className="
          flex h-8 shrink-0 items-center
          border-t border-[var(--luma-border)]
          px-2.5
          text-[12px]
          text-[var(--luma-text-muted)]
        "
      >
        <span className="truncate" title={workspacePath}>{workspacePath}</span>
      </div>
    </aside>
  );
}

function IDEView({
  selectedFile,
  onFileSelect,
  onWorkspaceChange,
  onChooseWorkspace,
}: {
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
  onWorkspaceChange: (path: string) => void;
  onChooseWorkspace: () => Promise<boolean>;
}) {
  const { appearance, theme } = useTheme();
  const [nodes, setNodes] = useState<WorkspaceNode[]>([]);
  const [workspacePath, setWorkspacePath] = useState("");
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [fileError, setFileError] = useState<string | null>(null);

  const refreshWorkspace = useCallback(async () => {
    const [workspaceInfo, tree] = await Promise.all([
      window.luma.getWorkspace(),
      window.luma.listWorkspaceFiles(),
    ]);
    setWorkspacePath(workspaceInfo.path);
    setNodes(tree);
    onWorkspaceChange(workspaceInfo.path);
  }, [onWorkspaceChange]);

  useEffect(() => {
    void refreshWorkspace().catch((error: unknown) => {
      setFileError(error instanceof Error ? error.message : String(error));
    });
  }, [refreshWorkspace]);

  useEffect(() => {
    let active = true;
    if (!selectedFile) {
      setContent("");
      setDirty(false);
      setFileError(null);
      return () => { active = false; };
    }
    setLoading(true);
    setFileError(null);
    window.luma.readWorkspaceFile(selectedFile)
      .then((value) => {
        if (active) {
          setContent(value);
          setDirty(false);
        }
      })
      .catch((error: unknown) => {
        if (active) setFileError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [selectedFile]);

  const saveFile = useCallback(async () => {
    if (!selectedFile || !dirty || saving) return;
    setSaving(true);
    setFileError(null);
    try {
      await window.luma.writeWorkspaceFile(selectedFile, content);
      setDirty(false);
    } catch (error) {
      setFileError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }, [content, dirty, saving, selectedFile]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveFile();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [saveFile]);

  const chooseWorkspace = async () => {
    if (dirty && !window.confirm("Discard unsaved changes before opening another workspace?")) return;
    try {
      const changed = await onChooseWorkspace();
      if (!changed) return;
      onFileSelect("");
      await refreshWorkspace();
    } catch (error) {
      setFileError(error instanceof Error ? error.message : String(error));
    }
  };

  const selectFile = (path: string) => {
    if (dirty && !window.confirm("Discard unsaved changes in the current file?")) return;
    onFileSelect(path);
  };

  const fileName = selectedFile
    ? (selectedFile.split("/").pop() ?? selectedFile)
    : "untitled";

  const language = getLanguage(selectedFile);

  const monacoTheme = appearance === "light" ? "vs" : theme === "luma" ? "luma-dark" : "vs-dark";

  return (
    <div className="flex h-full min-w-0">
      <FilesPanel
        selectedFile={selectedFile}
        onFileSelect={selectFile}
        nodes={nodes}
        workspacePath={workspacePath}
        onChooseWorkspace={chooseWorkspace}
      />

      <div className="flex min-w-0 flex-1 flex-col bg-[var(--luma-bg)]">
      <div
        className="
          flex h-9 shrink-0 items-center
          border-b border-[var(--luma-border)]
          bg-[var(--luma-surface)]
        "
      >
        <div
          className="
            flex h-full items-center
            border-r border-[var(--luma-border)]
            px-3
            text-[12px]
            text-[var(--luma-text-secondary)]
          "
        >
          <FileIcon />

          <span className="ml-1.5">{fileName}</span>
        </div>

        {selectedFile && (
          <span className="ml-3 truncate text-[12px] text-[var(--luma-text-muted)]">
            {selectedFile}
          </span>
        )}

        <div className="ml-auto px-3 text-[12px] text-[var(--luma-text-muted)]">
          {dirty ? "Modified" : language}
        </div>
        <button
          type="button"
          onClick={() => void saveFile()}
          disabled={!dirty || saving}
          className="mr-2 rounded-md border border-[var(--luma-border)] px-2 py-1 text-[11px] text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] disabled:opacity-40"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>

      <div className="min-h-0 flex-1">
        {fileError ? (
          <div className="p-4"><ErrorMessage error={fileError} /></div>
        ) : loading ? (
          <div className="p-4 text-[12px] text-[var(--luma-text-muted)]">Loading file…</div>
        ) : selectedFile ? <Editor
          height="100%"
          language={language}
          theme={monacoTheme}
          beforeMount={(monaco) => {
            monaco.editor.defineTheme("luma-dark", {
              base: "vs-dark",
              inherit: true,
              rules: [
                { token: "comment", foreground: "8F8D88" },
                { token: "keyword", foreground: "9DB2F8" },
                { token: "string", foreground: "A7B3AB" },
                { token: "number", foreground: "E9B85C" },
                { token: "type", foreground: "F0955A" },
                { token: "type.identifier", foreground: "F0955A" },
                { token: "identifier", foreground: "ECEBE8" },
                { token: "delimiter", foreground: "A8A6A1" },
              ],
              colors: {
                "editor.background": "#161615",
                "editor.foreground": "#ECEBE8",
                "editorLineNumber.foreground": "#8F8D88",
                "editorLineNumber.activeForeground": "#A8A6A1",
                "editorCursor.foreground": "#9DB2F8",
                "editor.selectionBackground": "#22305C",
                "editor.inactiveSelectionBackground": "#2A3866",
                "editor.lineHighlightBackground": "#1F1E1C",
                "editorIndentGuide.background1": "#2E2D2B",
                "editorIndentGuide.activeBackground1": "#4A4845",
                "editorWidget.background": "#232220",
                "editorWidget.border": "#3A3937",
                "input.background": "#232220",
                "input.border": "#3A3937",
                "scrollbarSlider.background": "#3A3937",
                "scrollbarSlider.hoverBackground": "#4A4845",
                "editorGutter.background": "#161615",
                "editorOverviewRuler.border": "#2E2D2B",
                "editor.findMatchBackground": "#4A2E1C",
              },
            });
          }}
          value={content}
          onChange={(value) => {
            setContent(value ?? "");
            setDirty(true);
          }}
          options={{
            automaticLayout: true,

            minimap: {
              enabled: false,
            },

            fontSize: 12,
            lineHeight: 19,

            fontFamily: "'JetBrains Mono', SFMono-Regular, Menlo, Monaco, Consolas, monospace",

            padding: {
              top: 10,
              bottom: 10,
            },

            scrollBeyondLastLine: false,
            smoothScrolling: true,

            cursorBlinking: "smooth",

            renderWhitespace: "selection",

            roundedSelection: false,

            overviewRulerBorder: false,
            hideCursorInOverviewRuler: true,

            folding: true,
            glyphMargin: false,

            lineNumbersMinChars: 3,

            tabSize: 2,

            wordWrap: "off",

            scrollbar: {
              verticalScrollbarSize: 8,
              horizontalScrollbarSize: 8,
            },

            suggest: {
              showMethods: true,
              showFunctions: true,
              showVariables: true,
            },

            bracketPairColorization: {
              enabled: true,
            },

            guides: {
              bracketPairs: true,
              indentation: true,
            },
          }}
        /> : <div className="flex h-full items-center justify-center text-[12px] text-[var(--luma-text-muted)]">Choose a file to start editing.</div>}
      </div>

      <div
        className="
          flex h-6 shrink-0 items-center
          border-t border-[var(--luma-border)]
          bg-[var(--luma-surface)]
          px-3
          text-[12px]
          text-[var(--luma-text-muted)]
        "
      >
        <span>{language}</span>

        <span className="mx-2 opacity-40">•</span>

        <span>UTF-8</span>

        <span className="mx-2 opacity-40">•</span>

        <span>Spaces: 2</span>

        <span className="ml-auto">{dirty ? "Unsaved changes · ⌘S to save" : "Luma Editor"}</span>
      </div>
      </div>
    </div>
  );
}

function ChangesView({ onOpenFile }: { onOpenFile: (path: string) => void }) {
  const [branch, setBranch] = useState("");
  const [files, setFiles] = useState<GitChange[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [diff, setDiff] = useState("");
  const [loading, setLoading] = useState(true);
  const [diffLoading, setDiffLoading] = useState(false);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await window.luma.getGitChanges();
      setBranch(result.branch);
      setFiles(result.files);
      setSelected((current) => current && result.files.some((file) => file.path === current)
        ? current : result.files[0]?.path ?? null);
      setRefreshVersion((version) => version + 1);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setFiles([]);
      setSelected(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    if (!selected) { setDiff(""); return; }
    let active = true;
    setDiffLoading(true);
    window.luma.getGitDiff(selected)
      .then((value) => { if (active) setDiff(value); })
      .catch((cause: unknown) => { if (active) setDiff(cause instanceof Error ? cause.message : String(cause)); })
      .finally(() => { if (active) setDiffLoading(false); });
    return () => { active = false; };
  }, [selected, refreshVersion]);

  const statusLabel = (file: GitChange) => file.kind === "untracked" ? "?" : file.kind === "added" ? "A" : file.kind === "deleted" ? "D" : file.kind === "renamed" ? "R" : "M";

  return (
    <section className="flex h-full min-w-0 flex-col bg-[var(--luma-bg)]">
      <header className="flex h-12 shrink-0 items-center border-b border-[var(--luma-border)] px-4">
        <div className="min-w-0">
          <div className="text-[13px] font-semibold">Changes</div>
          <div className="text-[11px] text-[var(--luma-text-muted)]">{branch ? `Branch · ${branch}` : "Workspace Git status"}</div>
        </div>
        <div className="ml-auto flex items-center gap-3">
          <span className="text-[11px] text-[var(--luma-text-muted)]">{files.length} changed {files.length === 1 ? "file" : "files"}</span>
          <button type="button" onClick={() => void refresh()} disabled={loading} className="luma-btn-ghost h-8 px-2.5">{loading ? "Refreshing…" : "Refresh"}</button>
        </div>
      </header>
      {error ? <div className="p-4"><ErrorMessage error={error} /></div> : loading && files.length === 0 ? (
        <div className="p-5 text-[12px] text-[var(--luma-text-muted)]">Checking repository changes…</div>
      ) : files.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="text-center"><div className="text-[13px] font-medium">Working tree clean</div><p className="mt-1 text-[12px] text-[var(--luma-text-muted)]">Git reports no staged, unstaged, or untracked files.</p></div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1">
          <aside className="w-[290px] shrink-0 overflow-auto border-r border-[var(--luma-border)] p-2">
            <div className="px-2 py-2 text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">Working tree</div>
            {files.map((file) => (
              <button key={file.path} type="button" onClick={() => setSelected(file.path)} className={`flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-[12px] ${selected === file.path ? "bg-[var(--luma-accent-soft)] text-[var(--luma-text)]" : "text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)]"}`}>
                <span className="w-4 shrink-0 text-center font-semibold">{statusLabel(file)}</span>
                <span className="min-w-0 flex-1 truncate" title={file.path}>{file.path}</span>
                <span className="shrink-0 text-[10px] text-[var(--luma-text-muted)]">{file.indexStatus !== " " && file.indexStatus !== "?" ? "staged" : file.kind === "untracked" ? "new" : "worktree"}</span>
              </button>
            ))}
          </aside>
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex h-10 shrink-0 items-center border-b border-[var(--luma-border)] px-3">
              <span className="min-w-0 flex-1 truncate text-[12px] text-[var(--luma-text-secondary)]">{selected}</span>
              {selected && <button type="button" onClick={() => onOpenFile(selected)} className="luma-btn-ghost h-7 px-2">Open in IDE</button>}
            </div>
            <div className="min-h-0 flex-1 overflow-auto p-3">
              {diffLoading ? <div className="text-[12px] text-[var(--luma-text-muted)]">Loading diff…</div> : diff ? (
                <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-[var(--luma-text-secondary)]">{diff}</pre>
              ) : <div className="text-[12px] text-[var(--luma-text-muted)]">No textual diff is available for this file.</div>}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

type CodeInspectorTab = "changes" | "files" | "terminal";

function EmbeddedTerminal({ workspacePath }: { workspacePath: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    const pendingData: Array<{ id: string; data: string }> = [];
    let pendingExit: { id: string; code: number | null; signal: NodeJS.Signals | null } | null = null;
    const terminal = new Terminal({
      cursorBlink: true,
      convertEol: true,
      scrollback: 3000,
      fontSize: 12,
      lineHeight: 1.35,
      fontFamily: "'JetBrains Mono', 'SFMono-Regular', Menlo, monospace",
      theme: {
        background: "#0F0F0E",
        foreground: "#ECEBE8",
        cursor: "#ECEBE8",
        selectionBackground: "#22305C",
        black: "#0F0F0E",
        red: "#F08A82",
        green: "#6FCF8E",
        yellow: "#E9B85C",
        blue: "#9DB2F8",
        magenta: "#C5A0E6",
        cyan: "#7FC8D6",
        white: "#ECEBE8",
        brightBlack: "#8F8D88",
        brightRed: "#F08A82",
        brightGreen: "#6FCF8E",
        brightYellow: "#E9B85C",
        brightBlue: "#B4C4FA",
        brightMagenta: "#D3B2EF",
        brightCyan: "#A3D9E3",
        brightWhite: "#ffffff",
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminalRef.current = terminal;
    fitRef.current = fit;
    if (containerRef.current) terminal.open(containerRef.current);
    const fitTerminal = () => {
      try {
        fit.fit();
        if (sessionRef.current) void window.luma.resizeTerminal(sessionRef.current, terminal.cols, terminal.rows);
      } catch { /* The terminal can be hidden while its inspector tab is inactive. */ }
    };
    const resizeObserver = new ResizeObserver(fitTerminal);
    if (containerRef.current) resizeObserver.observe(containerRef.current);
    terminal.onData((data) => {
      if (sessionRef.current) void window.luma.writeTerminal(sessionRef.current, data).catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    });
    const unsubscribeData = window.luma.onTerminalData((event) => {
      if (!active) return;
      if (!sessionRef.current) {
        pendingData.push(event);
      } else if (event.id === sessionRef.current) {
        terminal.write(event.data);
      }
    });
    const unsubscribeExit = window.luma.onTerminalExit((event) => {
      if (!sessionRef.current) pendingExit = event;
      else if (event.id === sessionRef.current) terminal.write(`\r\n\x1b[90m[process exited: ${event.code ?? event.signal ?? "unknown"}]\x1b[0m\r\n`);
    });
    fitTerminal();
    void window.luma.startTerminal(terminal.cols, terminal.rows).then(({ id }) => {
      if (!active) {
        void window.luma.closeTerminal(id);
        return;
      }
      sessionRef.current = id;
      for (const event of pendingData) if (event.id === id) terminal.write(event.data);
      pendingData.length = 0;
      if (pendingExit?.id === id) terminal.write(`\r\n\x1b[90m[process exited: ${pendingExit.code ?? pendingExit.signal ?? "unknown"}]\x1b[0m\r\n`);
      pendingExit = null;
      fitTerminal();
      terminal.focus();
    }).catch((reason: unknown) => {
      if (active) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => {
      active = false;
      resizeObserver.disconnect();
      unsubscribeData();
      unsubscribeExit();
      if (sessionRef.current) void window.luma.closeTerminal(sessionRef.current);
      sessionRef.current = "";
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
    };
  }, []);

  return <div className="luma-embedded-terminal">
    <div className="luma-terminal-toolbar"><span className="luma-terminal-indicator" /> <span className="min-w-0 flex-1 truncate" title={workspacePath}>{workspacePath || "Workspace"}</span><button type="button" onClick={() => terminalRef.current?.clear()}>Clear</button><button type="button" onClick={() => terminalRef.current?.focus()}>Focus</button></div>
    <div ref={containerRef} className="luma-terminal-xterm" aria-label="Embedded terminal" />
    {error && <p role="alert" className="luma-inspector-error">{error}</p>}
  </div>;
}

function CodeInspector({ workspacePath, onOpenFile }: {
  workspacePath: string;
  onOpenFile: (path: string) => void;
}) {
  const [tab, setTab] = useState<CodeInspectorTab>("changes");
  const [files, setFiles] = useState<GitChange[]>([]);
  const [branch, setBranch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [diff, setDiff] = useState("");
  const [tree, setTree] = useState<WorkspaceNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [treeLoading, setTreeLoading] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const result = await window.luma.getGitChanges();
      setBranch(result.branch);
      setFiles(result.files);
      setSelected((current) => current && result.files.some((file) => file.path === current)
        ? current : result.files[0]?.path ?? null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setFiles([]);
      setSelected(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void refresh(); }, [refresh, refreshKey]);

  useEffect(() => {
    if (!selected || tab !== "changes") { setDiff(""); return; }
    let active = true;
    window.luma.getGitDiff(selected).then((value) => {
      if (active) setDiff(value);
    }).catch((reason: unknown) => {
      if (active) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { active = false; };
  }, [selected, tab, refreshKey]);

  useEffect(() => {
    if (tab !== "files") return;
    let active = true;
    setTreeLoading(true);
    window.luma.listWorkspaceFiles().then((value) => {
      if (active) setTree(value);
    }).catch((reason: unknown) => {
      if (active) setError(reason instanceof Error ? reason.message : String(reason));
    }).finally(() => { if (active) setTreeLoading(false); });
    return () => { active = false; };
  }, [tab, workspacePath, refreshKey]);

  async function stageSelected() {
    if (!selected || busy) return;
    setBusy(true);
    setError("");
    try {
      await window.luma.stageWorkspaceFile(selected);
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function revertSelected() {
    const file = files.find((item) => item.path === selected);
    if (!file || file.kind === "untracked" || file.kind === "added" || file.kind === "renamed" || busy) return;
    const confirmed = window.confirm(`Discard changes to ${file.path}? This restores that tracked file from HEAD.`);
    if (!confirmed) return;
    setBusy(true);
    setError("");
    try {
      await window.luma.revertWorkspaceFile(file.path);
      await refresh();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  const statusLabel = (file: GitChange) => file.kind === "untracked" ? "?" : file.kind === "added" ? "A" : file.kind === "deleted" ? "D" : file.kind === "renamed" ? "R" : "M";

  return (
    <aside className="luma-code-inspector" aria-label="Workspace inspector">
      <header className="luma-inspector-header">
        <div className="luma-inspector-tabs" role="tablist" aria-label="Workspace tools">
          {(["changes", "files", "terminal"] as const).map((item) => <button key={item} type="button" role="tab" aria-selected={tab === item} data-active={tab === item} onClick={() => setTab(item)}>{item[0].toUpperCase() + item.slice(1)}</button>)}
        </div>
      </header>
      {error && <div role="alert" className="luma-inspector-error">{error}</div>}
      {tab === "changes" ? <>
        <div className="luma-inspector-summary">
          <div className="min-w-0 flex-1"><strong>{files.length} {files.length === 1 ? "file" : "files"} changed</strong><span>{branch || "Workspace Git status"}</span></div>
          <button type="button" onClick={() => void refresh()} disabled={loading || busy} title="Refresh changes">↻</button>
        </div>
        <div className="luma-inspector-actions">
          <button type="button" onClick={() => void revertSelected()} disabled={!selected || !files.some((file) => file.path === selected && file.kind !== "untracked" && file.kind !== "added" && file.kind !== "renamed") || busy} title="Discard changes to the selected tracked file">Revert selected</button>
          <button type="button" onClick={() => void stageSelected()} disabled={!selected || busy} title="Stage only the selected file">{busy ? "Working…" : "Accept selected"}</button>
        </div>
        {loading && files.length === 0 ? <p className="luma-inspector-empty">Checking repository changes…</p> : files.length === 0 ? <p className="luma-inspector-empty">No changes in this workspace.</p> : <>
          <div className="luma-inspector-change-list">
            {files.map((file) => <button key={file.path} type="button" data-selected={selected === file.path} onClick={() => setSelected(file.path)}>
              <span className="luma-change-status">{statusLabel(file)}</span><span className="min-w-0 flex-1 truncate" title={file.path}>{file.path}</span>
              <span className="luma-change-kind">{file.kind === "untracked" ? "new" : file.indexStatus !== " " && file.indexStatus !== "?" ? "staged" : "worktree"}</span>
            </button>)}
          </div>
          <div className="luma-inspector-diff-header"><span className="min-w-0 flex-1 truncate">{selected ?? "Diff preview"}</span>{selected && <button type="button" onClick={() => onOpenFile(selected)}>Open</button>}</div>
          <pre className="luma-inspector-diff">{diff || (selected ? "No textual diff available." : "Select a file to inspect its diff.")}</pre>
        </>}
      </> : tab === "files" ? <>
        <div className="luma-inspector-summary"><div className="min-w-0 flex-1"><strong>Workspace files</strong><span title={workspacePath}>{workspacePath || "No workspace selected"}</span></div><button type="button" onClick={() => setRefreshKey((value) => value + 1)} title="Refresh files">↻</button></div>
        <div className="luma-inspector-file-tree">{treeLoading ? <p className="luma-inspector-empty">Loading files…</p> : tree.length ? <WorkspaceTree nodes={tree} selectedFile={null} onFileSelect={onOpenFile} /> : <p className="luma-inspector-empty">No files found.</p>}</div>
      </> : <EmbeddedTerminal workspacePath={workspacePath} />}
    </aside>
  );
}

function TeammatesPanel({ onSidebarChange, onOpenFile, onHandoffToCode, sendWithEnter, onSendWithEnterChange, autoScrollToLatest, onAutoScrollToLatestChange, showToolActivity, onShowToolActivityChange, onOpenSettings, titlebarStatus }: { onSidebarChange: (sidebar: React.ReactNode) => void; onOpenFile: (path: string) => void; onHandoffToCode: (prompt: string) => void; sendWithEnter: boolean; onSendWithEnterChange: (value: boolean) => void; autoScrollToLatest: boolean; onAutoScrollToLatestChange: (value: boolean) => void; showToolActivity: boolean; onShowToolActivityChange: (value: boolean) => void; onOpenSettings: () => void; titlebarStatus: TitlebarStatus | null }) {
  const [agents, setAgents] = useState<AgentRecord[]>([]);
  const [tasks, setTasks] = useState<TaskRecord[]>([]);
  const [runtimeEvents, setRuntimeEvents] = useState<RuntimeEvent[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [schedules, setSchedules] = useState<ScheduleRecord[]>([]);
  const [memoryEntries, setMemoryEntries] = useState<MemoryEntry[]>([]);
  const [memoryScope, setMemoryScope] = useState<MemoryScope>("agent");
  const [workspacePath, setWorkspacePath] = useState("");
  const [memoryRevision, setMemoryRevision] = useState(0);
  const [selectedAgentId, setSelectedAgentId] = useState<string>("");
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [settingsSection, setSettingsSection] = useState<"identity" | "behavior" | "capabilities" | "memory" | "schedules" | "advanced">("identity");
  const [sendingMessage, setSendingMessage] = useState(false);
  const [submittingTaskId, setSubmittingTaskId] = useState<string | null>(null);
  const [stoppingTaskId, setStoppingTaskId] = useState<string | null>(null);
  const [cancellingTasks, setCancellingTasks] = useState<string[]>([]);
  const [teammateSearchOpen, setTeammateSearchOpen] = useState(false);
  const [teammateSearch, setTeammateSearch] = useState("");
  const [inspectorVisible, setInspectorVisible] = useState(true);
  const [workspaceFiles, setWorkspaceFiles] = useState<WorkspaceNode[]>([]);
  const [filesLoading, setFilesLoading] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [agentName, setAgentName] = useState("");
  const [agentDescription, setAgentDescription] = useState("");
  const [agentPersonality, setAgentPersonality] = useState("");
  const [agentInitialMemory, setAgentInitialMemory] = useState("");
  const [agentWorkspace, setAgentWorkspace] = useState("");
  const [agentMascot, setAgentMascot] = useState("auto");
  const [agentUseGlobalContext, setAgentUseGlobalContext] = useState(true);
  const [personalityDraft, setPersonalityDraft] = useState("");
  const [skillsDraft, setSkillsDraft] = useState("");
  const [mascotDraft, setMascotDraft] = useState("auto");
  const [agentNameDraft, setAgentNameDraft] = useState("");
  const [agentDescriptionDraft, setAgentDescriptionDraft] = useState("");
  const [configurationDraft, setConfigurationDraft] = useState("");
  const [chatPrompt, setChatPrompt] = useState("");
  const [suggestionsDismissed, setSuggestionsDismissed] = useState(false);
  const [teammateNotice, setTeammateNotice] = useState("");
  const [activeMentionIndex, setActiveMentionIndex] = useState(0);
  const [activeSlashIndex, setActiveSlashIndex] = useState(0);
  const chatPromptRef = useRef<HTMLTextAreaElement>(null);
  const teammateHistoryRef = useRef<HTMLDivElement>(null);
  const [chatAttachments, setChatAttachments] = useState<ChatAttachment[]>([]);
  const [activeTab, setActiveTab] = useState<"chat" | "settings">("chat");
  const [inspectorTab, setInspectorTab] = useState<"tasks" | "notes" | "files">("tasks");
  const [memoryContent, setMemoryContent] = useState("");
  const [schedulePrompt, setSchedulePrompt] = useState("");
  const [scheduleTitle, setScheduleTitle] = useState("Recurring task");
  const [scheduleIntervalHours, setScheduleIntervalHours] = useState("24");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshing = useRef(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key.toLowerCase() !== "k") return;
      event.preventDefault();
      setTeammateSearchOpen((value) => !value);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const startNewTask = () => {
      if (agents.length === 0) setCreateOpen(true);
      else {
        if (!selectedAgentId) setSelectedAgentId(agents[0].id);
        setSelectedTaskId("__new__");
        setActiveTab("chat");
        setSubmittingTaskId(null);
      }
    };
    window.addEventListener("luma:new-teammate-task", startNewTask);
    return () => window.removeEventListener("luma:new-teammate-task", startNewTask);
  }, [agents, selectedAgentId]);

  const refresh = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const [nextAgents, nextApprovals, nextSchedules] = await Promise.all([
        window.luma.getAgents(), window.luma.getApprovals(), window.luma.getSchedules(),
      ]);
      const nextSelectedAgentId = selectedAgentId && nextAgents.some((agent) => agent.id === selectedAgentId)
        ? selectedAgentId : nextAgents.find((agent) => agent.kind === "teammates")?.id ?? nextAgents[0]?.id ?? "";
      const nextTasks = await window.luma.getTasks(nextSelectedAgentId || undefined);
      setAgents(nextAgents);
      setTasks(nextTasks);
      setCancellingTasks((current) => current.filter((id) => nextTasks.some((task) => task.id === id && ["queued", "running", "waiting_for_approval", "waiting_for_input", "paused"].includes(task.status))));
      setApprovals(nextApprovals);
      setSchedules(nextSchedules);
      setSelectedAgentId(nextSelectedAgentId);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      refreshing.current = false;
    }
  }, [selectedAgentId]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (!selectedAgentId) return;
    let disposed = false;
    void window.luma.getTasks(selectedAgentId).then((nextTasks) => {
      if (!disposed) setTasks(nextTasks);
    }).catch((reason) => {
      if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { disposed = true; };
  }, [selectedAgentId]);

  useEffect(() => {
    let disposed = false;
    void window.luma.getWorkspace().then((workspace) => {
      if (!disposed) setWorkspacePath(workspace.path);
    }).catch((reason) => {
      if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { disposed = true; };
  }, []);

  useEffect(() => {
    const scopeId = memoryScope === "agent" ? selectedAgentId : memoryScope === "user" ? "default" : workspacePath;
    if (!scopeId) { setMemoryEntries([]); return; }
    let disposed = false;
    const loadMemory = () => {
      void window.luma.getMemory(memoryScope, scopeId).then((entries) => {
        if (!disposed) setMemoryEntries(entries);
      }).catch((reason) => {
        if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
      });
    };
    loadMemory();
    const timer = window.setInterval(loadMemory, 5000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [memoryScope, selectedAgentId, workspacePath, memoryRevision]);

  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null;
  const agentTasks = useMemo(() => tasks.filter((task) => task.agent_id === selectedAgentId && !task.schedule_id), [tasks, selectedAgentId]);
  const inspectorActiveTasks = agentTasks.filter((task) => ["queued", "running", "waiting_for_approval", "waiting_for_input", "paused"].includes(task.status));
  const inspectorCompletedTasks = agentTasks.filter((task) => ["completed", "failed", "cancelled", "interrupted"].includes(task.status)).sort((left, right) => right.created_at - left.created_at).slice(0, 6);
  const agentApprovals = approvals.filter((approval) => approval.agent_id === selectedAgentId);
  const visibleAgentTasks = selectedTaskId
    ? selectedTaskId === "__new__" ? [] : agentTasks.filter((task) => task.id === selectedTaskId)
    : agentTasks;
  const mentionQuery = chatPrompt.match(/(?:^|\s)@([^\s]*)$/)?.[1];
  const mentionMatches = suggestionsDismissed || mentionQuery === undefined ? [] : agents
    .filter((agent) => agent.id !== selectedAgentId)
    .filter((agent) => `${agent.name} ${agent.description ?? ""}`.toLowerCase().includes(mentionQuery.toLowerCase()));
  const teammateSlashMatches = !suggestionsDismissed && chatPrompt.startsWith("/")
    ? TEAMMATE_SLASH_COMMANDS.filter((item) => item.command.startsWith(chatPrompt.toLowerCase()))
    : [];
  const hasMentionRoute = routeTeammateMentions(chatPrompt, agents).destinations.length > 0;
  const activeComposerTask = visibleAgentTasks.find((task) => ["queued", "running", "waiting_for_approval", "waiting_for_input", "paused"].includes(task.status));
  const activeComposerTaskId = activeComposerTask?.id ?? submittingTaskId;

  function chooseMention(agent: AgentRecord) {
    setChatPrompt((value) => value.replace(/(?:^|\s)@[^\s]*$/, (match) => `${match.startsWith(" ") ? " " : ""}@${agent.name} `));
    setActiveMentionIndex(0);
    chatPromptRef.current?.focus();
  }

  async function runTeammateCommand(rawCommand: string) {
    const command = rawCommand.trim().toLowerCase();
    setChatPrompt("");
    setTeammateNotice("");
    if (command === "/help") {
      setTeammateNotice(TEAMMATE_SLASH_COMMANDS.map((item) => `${item.command} — ${item.description}`).join("\n"));
    } else if (command === "/new" || command === "/clear") {
      setSelectedTaskId("__new__"); setChatAttachments([]); setActiveTab("chat");
    } else if (command === "/history") {
      setSelectedTaskId(null); setActiveTab("chat");
    } else if (command === "/settings") {
      setSettingsSection("identity"); setActiveTab("settings");
    } else if (command === "/memory") {
      setSettingsSection("memory"); setActiveTab("settings");
    } else if (command === "/schedule") {
      setSettingsSection("schedules"); setActiveTab("settings");
    } else if (command === "/status") {
      setTeammateNotice(`${selectedAgent?.name ?? "Teammate"} · ${selectedAgent?.status ?? "unknown"} · ${agentTasks.length} saved tasks · ${inspectorActiveTasks.length} active`);
    } else if (command === "/pause" && selectedAgent) {
      await perform(() => window.luma.setAgentStatus(selectedAgent.id, "paused"));
    } else if (command === "/resume" && selectedAgent) {
      await perform(() => window.luma.setAgentStatus(selectedAgent.id, "idle"));
    } else if (command === "/stop" && activeComposerTaskId) {
      await perform(() => window.luma.cancelTask(activeComposerTaskId));
    } else {
      setTeammateNotice(`Unknown Teammates command: ${rawCommand}. Type /help to see available commands.`);
    }
  }

  useEffect(() => {
    const history = teammateHistoryRef.current;
    if (activeTab !== "chat" || !selectedAgentId || !history) return;
    let frame = 0;
    const scrollToLatest = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        frame = window.requestAnimationFrame(() => {
          history.scrollTop = history.scrollHeight;
        });
      });
    };
    // Always start a selected teammate/task at the latest message. Keep following
    // streamed content only while the user's auto-scroll preference is enabled.
    scrollToLatest();
    const observer = new MutationObserver(() => {
      if (autoScrollToLatest) scrollToLatest();
    });
    observer.observe(history, { childList: true, characterData: true, subtree: true });
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [activeTab, selectedAgentId, selectedTaskId, autoScrollToLatest]);

  useEffect(() => {
    if (inspectorTab !== "files") return;
    let active = true;
    setFilesLoading(true);
    window.luma.listWorkspaceFiles().then((files) => {
      if (active) setWorkspaceFiles(files);
    }).catch((reason: unknown) => {
      if (active) setError(reason instanceof Error ? reason.message : String(reason));
    }).finally(() => { if (active) setFilesLoading(false); });
    return () => { active = false; };
  }, [inspectorTab, workspacePath, selectedAgentId]);

  useEffect(() => {
    if (!submittingTaskId) return;
    const task = tasks.find((item) => item.id === submittingTaskId);
    if (task && !["queued", "running", "waiting_for_approval", "waiting_for_input", "paused"].includes(task.status)) {
      setSubmittingTaskId(null);
    }
  }, [submittingTaskId, tasks]);

  useEffect(() => {
    if (selectedTaskId === "__new__") chatPromptRef.current?.focus();
  }, [selectedTaskId, selectedAgentId]);
  const taskTimeline = (taskId: string): TeammateTimelineSegment[] => {
    const segments: TeammateTimelineSegment[] = [];
    for (const event of runtimeEvents
      .filter((item) => item.task_id === taskId && item.event_type === "task.agent_event")
      .sort((left, right) => left.sequence - right.sequence)) {
      const agentEvent = event.payload as { type?: string; data?: unknown };
      if (agentEvent.type === "TextDelta" && typeof agentEvent.data === "string") {
        const last = segments[segments.length - 1];
        if (last?.kind === "text") last.content += agentEvent.data;
        else segments.push({ kind: "text", id: event.event_id, content: agentEvent.data });
      } else if (agentEvent.type === "PlanGenerated" && typeof agentEvent.data === "string") {
        segments.push({ kind: "text", id: event.event_id, content: `### Plan\n${agentEvent.data}` });
      } else if (agentEvent.type === "ToolStarted" && typeof agentEvent.data === "object" && agentEvent.data !== null && "name" in agentEvent.data) {
        const data = agentEvent.data as { name: string; input?: string };
        const tool = { id: event.event_id, name: data.name, input: data.input ?? "" };
        const last = segments[segments.length - 1];
        if (last?.kind === "tools") last.tools.push(tool);
        else segments.push({ kind: "tools", id: event.event_id, tools: [tool] });
      } else if (agentEvent.type === "ToolFinished" && typeof agentEvent.data === "object" && agentEvent.data !== null && "name" in agentEvent.data) {
        const data = agentEvent.data as { name: string; duration_ms?: number; failed?: boolean; diff?: string | null };
        const last = segments[segments.length - 1];
        if (last?.kind === "tools") {
          for (let index = last.tools.length - 1; index >= 0; index -= 1) {
            if (last.tools[index].name === data.name && last.tools[index].durationMs === undefined) {
              last.tools[index] = { ...last.tools[index], durationMs: data.duration_ms, failed: data.failed, diff: data.diff ?? undefined };
              break;
            }
          }
        }
      }
    }
    return segments;
  };
  const taskMascotActivity = (task: TaskRecord): "idle" | "working" | "thinking" | "writing" => {
    if (task.status === "queued") return "working";
    if (task.status !== "running") return "idle";
    const timeline = taskTimeline(task.id);
    const last = timeline[timeline.length - 1];
    if (last?.kind === "tools" && last.tools.some((tool) => tool.durationMs === undefined)) return "working";
    // Thinking is a still, attentive state. Only show motion when the
    // teammate is visibly producing text or actively using a tool.
    return last?.kind === "text" ? "writing" : "idle";
  };

  useEffect(() => {
    onSidebarChange(
      <div className="luma-teammate-sidebar-transition">
        <div className="luma-teammate-sidebar-heading"><span>Your team</span><button type="button" disabled={busy} onClick={() => setCreateOpen(true)} aria-label="Add teammate" title="Add teammate"><PlusIcon /></button></div>
        <div className="luma-teammate-sidebar-actions">
          <button type="button" disabled={busy || !agents.length} onClick={() => { if (!selectedAgentId && agents[0]) setSelectedAgentId(agents[0].id); setSelectedTaskId("__new__"); setActiveTab("chat"); setSubmittingTaskId(null); }}><PlusIcon /> New task <kbd>{window.luma.platform === "darwin" ? "⌘N" : "Ctrl N"}</kbd></button>
          <button type="button" aria-pressed={teammateSearchOpen} onClick={() => setTeammateSearchOpen((value) => !value)}><SearchIcon /> Search <kbd>{window.luma.platform === "darwin" ? "⌘K" : "Ctrl K"}</kbd></button>
        </div>
        {teammateSearchOpen && <input className="luma-input luma-teammate-search" autoFocus value={teammateSearch} onChange={(event) => setTeammateSearch(event.target.value)} placeholder="Search teammates" />}
        {agents.length === 0 && <p className="px-1 py-3 text-[11px] text-[var(--luma-text-muted)]">No teammates yet. Add one to give it a persistent identity and workspace.</p>}
        <div className="mt-3 space-y-1">
          {agents.filter((agent) => `${agent.name} ${agent.description ?? ""}`.toLowerCase().includes(teammateSearch.trim().toLowerCase())).map((agent) => (
            <button key={agent.id} type="button" aria-current={selectedAgentId === agent.id ? "page" : undefined} onClick={() => { setSelectedTaskId(null); setSelectedAgentId(agent.id); setActiveTab("chat"); setTeammateNotice(""); }} className={`luma-teammate-nav-item w-full rounded-md px-2.5 py-2 text-left transition-colors ${selectedAgentId === agent.id ? "is-selected" : ""}`}>
              <div className="flex items-center gap-2"><MascotAvatar seed={agent.name} mascot={agent.configuration.mascot} size={30} active={false} activity="idle" /><span className={`h-1.5 w-1.5 rounded-full ${agent.status === "active" ? "bg-[var(--luma-success)]" : agent.status === "waiting" ? "bg-[var(--luma-warning)]" : "bg-[var(--luma-text-muted)]"}`} /><span className="min-w-0 flex-1 truncate text-[12px] font-medium">{agent.name}</span></div>
              <div className="mt-1 pl-10 text-[10px] text-[var(--luma-text-muted)]">{agent.description || (agent.id === "agent_global" ? "Global teammate" : agent.status === "active" ? "Working now" : agent.status === "waiting" ? "Needs your attention" : "Ready when needed")}</div>
            </button>
          ))}
          {selectedAgent && <div key={selectedAgent.id} className="luma-teammate-history-list">
            <div className="flex items-center justify-between px-2 pb-1 pt-2"><span className="luma-kicker">HISTORY</span><button type="button" onClick={() => { setSelectedTaskId(null); setActiveTab("chat"); }} className="text-[10px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">All</button></div>
            {agentTasks.slice().sort((left, right) => right.created_at - left.created_at).slice(0, 30).map((task) => <button key={task.id} type="button" className={`luma-teammate-history-nav-item ${selectedTaskId === task.id ? "is-selected" : ""}`} onClick={() => { setSelectedTaskId(task.id); setActiveTab("chat"); }}><span>{task.title || task.request}</span><small>{new Date(task.created_at * 1000).toLocaleDateString()}</small></button>)}
            {agentTasks.length === 0 && <p className="px-2 py-2 text-[10px] text-[var(--luma-text-muted)]">No history yet.</p>}
          </div>}
        </div>
      </div>,
    );
    return () => onSidebarChange(null);
  }, [agents, agentTasks, busy, onSidebarChange, selectedAgentId, selectedTaskId, teammateSearch, teammateSearchOpen]);

  useEffect(() => {
    if (!selectedAgent) return;
    setAgentNameDraft(selectedAgent.name);
    setAgentDescriptionDraft(selectedAgent.description ?? "");
    setPersonalityDraft(selectedAgent.configuration.system_prompt);
    setSkillsDraft(selectedAgent.configuration.skills.join(", "));
    setMascotDraft(selectedAgent.configuration.mascot || "auto");
    setConfigurationDraft(JSON.stringify(selectedAgent.configuration, null, 2));
  }, [selectedAgentId]);

  useEffect(() => {
    if (!selectedAgentId) { setRuntimeEvents([]); return; }
    let disposed = false;
    const load = () => void window.luma.getRuntimeEvents(selectedAgentId).then((events) => {
      if (!disposed) setRuntimeEvents(events);
    }).catch((reason) => {
      if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
    });
    load();
    const timer = window.setInterval(load, 1500);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [selectedAgentId]);

  async function perform(action: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try { await action(); await refresh(); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setBusy(false); }
  }

  async function answerTaskQuestion(taskId: string, questionId: string, answer: string) {
    await perform(() => window.luma.answerTaskQuestion(taskId, questionId, answer));
  }

  async function attachChatFiles() {
    setError(null);
    try {
      const selected = await window.luma.chooseChatAttachments();
      const next = [...chatAttachments, ...selected];
      const totalBytes = next.reduce((total, item) => total + new TextEncoder().encode(item.content).byteLength, 0);
      if (next.length > 8 || totalBytes > 4 * 1024 * 1024) {
        throw new Error("A teammate message can include up to 8 text files and 4 MiB of content.");
      }
      setChatAttachments(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <section className="flex h-full min-h-0 w-full flex-col overflow-hidden bg-[var(--luma-bg)]" aria-label="Teammates">
        {error && <div role="alert" className="border-b border-[var(--luma-danger)]/25 bg-[var(--luma-danger)]/5 px-5 py-2 text-[12px] text-[var(--luma-danger)]">{error}</div>}

        <div className="flex min-h-0 flex-1">
          <div className="luma-teammates-content flex min-h-0 flex-1 flex-col">
            {selectedAgent && <div className="luma-teammate-header flex shrink-0 items-center gap-3 px-6 py-2">
              <MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={32} active={false} activity={sendingMessage ? "idle" : activeComposerTask ? taskMascotActivity(activeComposerTask) : "idle"} />
              <div className="min-w-0"><div className="text-[13px] font-semibold">{selectedAgent.name}</div><div className="truncate text-[11px] text-[var(--luma-text-muted)]">{selectedAgent.description || "Personal teammate"}</div></div>
              <span className="luma-activity-pill" data-status-color={titlebarStatus?.color ?? (selectedAgent.status === "active" ? "blue" : selectedAgent.status === "waiting" ? "amber" : selectedAgent.status === "stopped" ? "red" : "green")}><i />{titlebarStatus?.text ?? (selectedAgent.status === "active" ? "Working" : selectedAgent.status === "waiting" ? "Needs you" : selectedAgent.status === "paused" ? "Paused" : selectedAgent.status === "stopped" ? "Stopped" : "Ready")}</span>
              <div className="ml-auto flex items-center gap-1">
                <button type="button" className="luma-inspector-icon-button" onClick={() => setActiveTab("settings")} aria-label="Teammate settings" title="Teammate settings"><SettingsIcon /></button>
                <button type="button" className="luma-inspector-icon-button" onClick={() => setInspectorVisible((visible) => !visible)} aria-label={inspectorVisible ? "Hide tasks" : "Show tasks"} title={inspectorVisible ? "Hide tasks" : "Show tasks"}><PanelCloseIcon /></button>
              </div>
            </div>}
          <div className="min-h-0 flex-1 overflow-hidden p-4">
            {!selectedAgent ? <div className="flex h-full items-center justify-center text-[12px] text-[var(--luma-text-muted)]">Select a teammate to start chatting.</div> : activeTab === "chat" ? (
              <div className="mx-auto flex h-full w-full max-w-[720px] flex-col">
                <div ref={teammateHistoryRef} key={selectedAgent.id} className="luma-teammate-history-transition min-h-0 flex-1 space-y-5 overflow-auto pb-5">
                  {teammateNotice && <div role="status" className="whitespace-pre-wrap rounded-md border border-[var(--luma-border)] px-3 py-2 text-[11px] leading-5 text-[var(--luma-text-secondary)]">{teammateNotice}</div>}
                  {visibleAgentTasks.length === 0 ? <div className="flex h-full flex-col items-center justify-center text-center"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={96} active={false} activity="idle" /><h3 className="mt-4 text-[16px] font-semibold">{selectedTaskId === "__new__" ? `New chat with ${selectedAgent.name}` : `Chat with ${selectedAgent.name}`}</h3><p className="mt-1 max-w-sm text-[12px] leading-6 text-[var(--luma-text-muted)]">Start a conversation. This chat has its own history.</p></div> : visibleAgentTasks.slice().reverse().map((task) => {
                    const timeline = taskTimeline(task.id);
                    const hasAnswer = timeline.some((segment) => segment.kind === "text");
                    const activity = taskMascotActivity(task);
                    return <article key={task.id} className="space-y-3">
                      <div className="luma-teammate-user-row flex w-full justify-end"><div className="luma-teammate-user-message px-3 py-2"><div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">You</div><p className="whitespace-pre-wrap break-words text-[12px] leading-6">{task.request}</p></div></div>
                      <div className="luma-teammate-assistant-message flex items-start gap-3"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={42} active={false} activity={activity} /><div className="min-w-0 flex-1 pt-0.5"><div className="mb-1 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">{selectedAgent.name}<span className="font-normal normal-case tracking-normal">{task.status === "running" ? "· working" : task.status === "queued" ? "· queued" : task.status === "waiting_for_approval" ? "· needs approval" : task.status === "waiting_for_input" ? "· needs your answer" : ""}</span></div><div className="luma-teammate-assistant-bubble space-y-2">
                        {timeline.map((segment) => segment.kind === "tools" ? (
                          <details key={segment.id} className="luma-task-tool-group rounded-md border border-[var(--luma-border)] px-2 py-1.5">
                            <summary className="cursor-pointer select-none text-[10px] text-[var(--luma-text-muted)]">{toolActivityLabel(segment.tools)}</summary>
                            <div className="mt-2 space-y-2">{segment.tools.map((tool) => <div key={tool.id} className="border-l-2 border-[var(--luma-border-strong)] pl-2"><div className="text-[10px] font-medium text-[var(--luma-text-secondary)]">{tool.name}<span className="ml-2 font-normal text-[var(--luma-text-muted)]">{tool.durationMs === undefined ? "running" : `${tool.durationMs} ms${tool.failed ? " · failed" : ""}`}</span></div>{tool.input && <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{tool.input}</pre>}{tool.diff && <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{tool.diff}</pre>}</div>)}</div>
                          </details>
                        ) : <div key={segment.id} className="prose prose-sm max-w-none text-[12px] leading-6"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ pre({ children }) { return <pre className="my-2 overflow-x-auto rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] p-3">{children}</pre>; }, p({ children }) { return <p className="mb-2 last:mb-0">{children}</p>; }, ul({ children }) { return <ul className="mb-2 ml-4 list-disc">{children}</ul>; }, ol({ children }) { return <ol className="mb-2 ml-4 list-decimal">{children}</ol>; }, li({ children }) { return <li className="mb-1">{children}</li>; }, h1({ children }) { return <h1 className="mb-2 mt-3 text-[14px] font-semibold">{children}</h1>; }, h2({ children }) { return <h2 className="mb-2 mt-3 text-[13px] font-semibold">{children}</h2>; }, h3({ children }) { return <h3 className="mb-2 mt-3 text-[12px] font-semibold">{children}</h3>; } }}>{segment.content}</ReactMarkdown></div>)}
                        <TaskQuestionCard task={task} busy={busy} onAnswer={answerTaskQuestion} />
                        {!hasAnswer && timeline.length === 0 && task.status !== "waiting_for_input" && <div className="text-[11px] text-[var(--luma-text-muted)]">{task.status === "completed" ? "No text response was recorded for this task." : task.error ?? (task.status === "failed" ? "Task failed." : task.status === "cancelled" ? "Cancelled." : "Thinking…")}</div>}
                        {task.status === "failed" || task.status === "interrupted" ? <button type="button" disabled={busy} onClick={() => void perform(() => window.luma.retryTask(task.id))} className="mt-2 text-[10px] text-[var(--luma-accent)] hover:underline">Retry</button> : null}
                      </div></div></div>
                      <div className="flex justify-end"><button type="button" onClick={() => {
                        const response = timeline.filter((segment): segment is Extract<TeammateTimelineSegment, { kind: "text" }> => segment.kind === "text").map((segment) => segment.content).join("\n\n");
                        onHandoffToCode(`Continue this work in Code mode.\n\nOriginal task:\n${task.request}${response ? `\n\nTeammate response so far:\n${response}` : ""}\n\nReview the workspace and decide the next steps before making changes.`);
                      }} className="luma-handoff-to-code">Open in Code <span aria-hidden="true">↗</span></button></div>
                    </article>;
                  })}
                </div>
                <form className="luma-teammate-composer shrink-0" onSubmit={(event) => {
                  event.preventDefault();
                  const text = chatPrompt.trim() || (chatAttachments.length ? "Please inspect the attached files." : "");
                  if (!text || busy) return;
                  if (chatAttachments.length === 0 && TEAMMATE_SLASH_COMMANDS.some((item) => item.command === text.toLowerCase())) { void runTeammateCommand(text); return; }
                  const routing = routeTeammateMentions(text, agents);
                  const mentionedAgents = routing.destinations;
                  const destinations = mentionedAgents.length ? mentionedAgents : [selectedAgent];
                  if (destinations.some((agent) => agent.status === "stopped" || agent.status === "paused")) {
                    setError("A referenced teammate is paused or stopped. Resume it before routing this task.");
                    return;
                  }
                  if (!mentionedAgents.length && (selectedAgent.status === "stopped" || selectedAgent.status === "paused")) return;
                  setSendingMessage(true);
                  void perform(async () => {
                    const routedText = routing.prompt;
                    if (!routedText) throw new Error("Add a task description after the teammate mention.");
                    const createdTasks: TaskRecord[] = [];
                    for (const agent of destinations) {
                      createdTasks.push(await window.luma.createTask({ agentId: agent.id, request: formatChatPrompt(routedText, chatAttachments), workspace: agent.configuration.workspace || workspacePath || undefined }));
                    }
                    const created = createdTasks[0];
                    if (mentionedAgents.length) setSelectedAgentId(destinations[0].id);
                    setSelectedTaskId(null);
                    setSubmittingTaskId(created.id);
                    setChatPrompt("");
                    setChatAttachments([]);
                    setTeammateNotice("");
                  }).finally(() => setSendingMessage(false));
                }}>
                  {chatAttachments.length > 0 && <div className="mb-2 flex flex-wrap gap-1.5">{chatAttachments.map((attachment, index) => <span key={`${attachment.name}-${index}`} className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-[var(--luma-border)] px-2 py-1 text-[10px] text-[var(--luma-text-secondary)]"><span className="max-w-56 truncate">{attachment.name}</span><button type="button" aria-label={`Remove ${attachment.name}`} onClick={() => setChatAttachments((items) => items.filter((_, itemIndex) => itemIndex !== index))} className="text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">×</button></span>)}</div>}
                  {teammateSlashMatches.length > 0 && <div className="luma-command-menu luma-teammate-slash-menu" role="listbox" aria-label="Teammates commands">{teammateSlashMatches.map((item, index) => <button key={item.command} type="button" role="option" aria-selected={index === activeSlashIndex} className={index === activeSlashIndex ? "is-active" : ""} onMouseDown={(event) => event.preventDefault()} onClick={() => setChatPrompt(`${item.command} `)}><code>{item.command}</code><span>{item.description}</span></button>)}</div>}
                  {mentionMatches.length > 0 && <div className="luma-mention-menu" role="listbox" aria-label="Assign task to teammate"><p>Assign this task to a teammate</p>{mentionMatches.map((agent, index) => <button key={agent.id} type="button" role="option" aria-selected={index === activeMentionIndex} className={index === activeMentionIndex ? "is-active" : ""} onMouseDown={(event) => event.preventDefault()} onMouseEnter={() => setActiveMentionIndex(index)} onClick={() => chooseMention(agent)}>{agent.name}<span>{agent.description || "Teammate"}</span></button>)}</div>}
                  <div className="luma-teammate-composer-row flex items-center gap-2">
                    <button type="button" aria-label="Attach files" title="Attach text files" disabled={busy || (!hasMentionRoute && (selectedAgent.status === "stopped" || selectedAgent.status === "paused"))} onClick={() => void attachChatFiles()} className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)] disabled:opacity-40"><PlusIcon /></button>
                    <button type="button" aria-label="Route to a teammate" title="Route this task to another teammate" disabled={busy} onClick={() => { setChatPrompt((value) => `${value}${value && !value.endsWith(" ") ? " " : ""}@`); chatPromptRef.current?.focus(); }} className="luma-mention-button">@</button>
                    <PromptInput inputRef={chatPromptRef} value={chatPrompt} onChange={(value) => { setChatPrompt(value); setSuggestionsDismissed(false); setActiveMentionIndex(0); setActiveSlashIndex(0); }} onKeyDown={(event) => {
                      const move = (count: number, current: number, setter: (value: number) => void) => {
                        if (!["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"].includes(event.key) || count === 0) return false;
                        event.preventDefault(); const direction = ["ArrowDown", "ArrowRight"].includes(event.key) ? 1 : -1;
                        setter((current + direction + count) % count); return true;
                      };
                      if (teammateSlashMatches.length && move(teammateSlashMatches.length, activeSlashIndex, setActiveSlashIndex)) return;
                      if (mentionMatches.length && move(mentionMatches.length, activeMentionIndex, setActiveMentionIndex)) return;
                      if (event.key === "Escape" && (teammateSlashMatches.length || mentionMatches.length)) {
                        event.preventDefault();
                        setSuggestionsDismissed(true);
                        return;
                      }
                      if (event.key === "Enter" && !event.shiftKey && sendWithEnter) {
                        event.preventDefault();
                        const exactCommand = TEAMMATE_SLASH_COMMANDS.find((item) => item.command === chatPrompt.trim().toLowerCase());
                        if (exactCommand) { void runTeammateCommand(exactCommand.command); return; }
                        if (mentionMatches.length && mentionQuery !== undefined) { chooseMention(mentionMatches[activeMentionIndex] ?? mentionMatches[0]); return; }
                        if (teammateSlashMatches.length && chatPrompt.trim() !== teammateSlashMatches[activeSlashIndex]?.command) { setChatPrompt(`${teammateSlashMatches[activeSlashIndex]?.command ?? teammateSlashMatches[0].command} `); return; }
                        event.currentTarget.form?.requestSubmit();
                      }
                    }} placeholder={`Message ${selectedAgent.name}; use @ to assign…`} maxHeight={88} />
                    <TeammateModelDropdown key={selectedAgent.id} agent={selectedAgent} onChanged={() => void refresh()} />
                    <ComposerQuickSettings sendWithEnter={sendWithEnter} onSendWithEnterChange={onSendWithEnterChange} autoScrollToLatest={autoScrollToLatest} onAutoScrollToLatestChange={onAutoScrollToLatestChange} showToolActivity={showToolActivity} onShowToolActivityChange={onShowToolActivityChange} onOpenSettings={onOpenSettings} showCodeOptions={false} />
                    {activeComposerTaskId && !hasMentionRoute ? <button type="button" aria-label="Stop current task" title="Stop current task" disabled={busy || stoppingTaskId === activeComposerTaskId || cancellingTasks.includes(activeComposerTaskId)} onClick={() => {
                      const taskId = activeComposerTaskId;
                      setStoppingTaskId(taskId);
                      void perform(async () => {
                        await window.luma.cancelTask(taskId);
                        setCancellingTasks((current) => current.includes(taskId) ? current : [...current, taskId]);
                        await refresh();
                      }).finally(() => setStoppingTaskId(null));
                    }} className="luma-teammate-stop h-7 w-7 shrink-0" ><StopIcon /></button> : <button type="submit" aria-label="Send message" title={hasMentionRoute ? "Route task to mentioned teammate(s)" : "Send message"} disabled={busy || sendingMessage || (!chatPrompt.trim() && chatAttachments.length === 0) || (!hasMentionRoute && (selectedAgent.status === "stopped" || selectedAgent.status === "paused"))} className="luma-teammate-send h-7 w-7 shrink-0"><SendIcon /></button>}
                  </div>
                </form>
              </div>
            ) : (
              <div className="luma-teammate-settings space-y-5" data-settings-tab={settingsSection}>
                <section>
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex items-center gap-3"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={52} active={selectedAgent.status === "active"} /><h3 className="truncate text-[16px] font-semibold">{selectedAgent.name}</h3></div>
                      <p className="mt-1 text-[11px] text-[var(--luma-text-muted)]">Persistent identity · revision {selectedAgent.configuration_revision} · created {new Date(selectedAgent.created_at * 1000).toLocaleDateString()}</p>
                    </div>
                    <div className="flex gap-1.5">
                      {selectedAgent.status === "paused" ? <button type="button" disabled={busy} onClick={() => void perform(() => window.luma.setAgentStatus(selectedAgent.id, "idle"))} className="luma-btn-secondary h-7 px-2.5 text-[11px]">Resume</button> : <button type="button" disabled={busy || selectedAgent.status === "stopped" || selectedAgent.status === "active" || selectedAgent.status === "waiting"} onClick={() => void perform(() => window.luma.setAgentStatus(selectedAgent.id, "paused"))} className="luma-btn-secondary h-7 px-2.5 text-[11px]">Pause</button>}
                      <button type="button" disabled={busy || selectedAgent.status === "stopped" || selectedAgent.status === "active" || selectedAgent.status === "waiting"} onClick={() => { if (window.confirm(`Stop ${selectedAgent.name}?`)) void perform(() => window.luma.setAgentStatus(selectedAgent.id, "stopped")); }} className="luma-btn-ghost h-7 px-2.5 text-[11px] text-[var(--luma-danger)]">Stop</button>
                      {selectedAgent.id !== "agent_global" && <button type="button" disabled={busy} onClick={() => { if (window.confirm(`Delete ${selectedAgent.name}? Completed task history, schedules, approvals and agent-scoped memory will be deleted. Cancel active tasks first.`)) void perform(async () => { await window.luma.deleteAgent(selectedAgent.id); }); }} className="luma-btn-ghost h-7 px-2.5 text-[11px] text-[var(--luma-danger)]">Delete</button>}
                    </div>
                  </div>
                </section>

                <nav className="luma-teammate-settings-nav" aria-label="Teammate settings sections">
                  {(["identity", "behavior", "capabilities", "memory", "schedules", "advanced"] as const).map((section) => <button key={section} type="button" aria-pressed={settingsSection === section} data-active={settingsSection === section} onClick={() => setSettingsSection(section)}>{section[0].toUpperCase() + section.slice(1)}</button>)}
                </nav>

                <section data-settings-section="identity" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <div className="mb-2 luma-kicker">Identity & personality</div>
                  <div className="grid gap-2 sm:grid-cols-2">
                    <label className="text-[10px] text-[var(--luma-text-muted)]">Name<input value={agentNameDraft} onChange={(event) => setAgentNameDraft(event.target.value)} maxLength={120} className="luma-input mt-1 h-8 w-full text-[11px]" /></label>
                    <label className="text-[10px] text-[var(--luma-text-muted)]">Description<input value={agentDescriptionDraft} onChange={(event) => setAgentDescriptionDraft(event.target.value)} className="luma-input mt-1 h-8 w-full text-[11px]" /></label>
                  </div>
                  <button type="button" disabled={busy || !agentNameDraft.trim()} onClick={() => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, name: agentNameDraft.trim(), description: agentDescriptionDraft.trim() || null }); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Save identity</button>
                  <MascotCustomizer value={mascotDraft} seed={selectedAgent.name} onChange={setMascotDraft} />
                </section>

                <section data-settings-section="behavior" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <h4 className="mb-2 luma-kicker">Behavior & instructions</h4>
                  <p className="mb-2 text-[10px] leading-relaxed text-[var(--luma-text-muted)]">Define how this teammate approaches tasks, communicates, and makes decisions.</p>
                  <textarea value={personalityDraft} onChange={(event) => setPersonalityDraft(event.target.value)} rows={8} className="luma-input w-full resize-y text-[11px]" placeholder="How should this teammate behave?" />
                  <button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, system_prompt: personalityDraft } }); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Save instructions</button>
                </section>

                <section data-settings-section="capabilities" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <h4 className="mb-3 luma-kicker">Capabilities & access</h4>
                  {selectedAgent.id !== "agent_global" && <label className="flex items-start gap-2 text-[11px] text-[var(--luma-text-secondary)]"><input type="checkbox" checked={selectedAgent.configuration.use_global_context} onChange={(event) => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, use_global_context: event.target.checked, ...(event.target.checked ? { skills: [] } : {}) } }); })} className="mt-0.5 accent-[var(--luma-accent)]" /><span><strong>Use the global skills, memory, connectors, etc.</strong><small className="mt-0.5 block text-[var(--luma-text-muted)]">Share the global teammate's connected tools and shared context.</small></span></label>}
                  <label className="mt-3 flex items-start gap-2 text-[11px] text-[var(--luma-text-secondary)]"><input type="checkbox" checked={selectedAgent.configuration.network_access ?? true} onChange={(event) => { const enabled = event.target.checked; void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, network_access: enabled, permissions: { ...selectedAgent.configuration.permissions, allow_network: enabled } } }); }); }} className="mt-0.5 accent-[var(--luma-accent)]" /><span><strong>Internet access</strong><small className="mt-0.5 block text-[var(--luma-text-muted)]">Allow web search and fetching public webpages. External changes still require approval.</small></span></label>
                  {!selectedAgent.configuration.use_global_context && <label className="mt-3 block text-[11px] text-[var(--luma-text-muted)]">Assigned skills<input value={skillsDraft} onChange={(event) => setSkillsDraft(event.target.value)} className="luma-input mt-1 h-8 w-full text-[11px]" placeholder="research, writing, planning" /></label>}
                  <button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, skills: selectedAgent.configuration.use_global_context ? [] : [...new Set(skillsDraft.split(",").map((skill) => skill.trim()).filter(Boolean))] } }); })} className="mt-3 text-[11px] text-[var(--luma-accent)] hover:underline">Save capabilities</button>
                  {approvals.filter((approval) => approval.agent_id === selectedAgent.id).length > 0 && <div className="mt-4 border-t border-[var(--luma-border)] pt-3"><h4 className="mb-2 luma-kicker">Needs your approval</h4><div className="space-y-2">{approvals.filter((approval) => approval.agent_id === selectedAgent.id).map((approval) => <div key={approval.id} className="rounded-md border border-[var(--luma-warning)]/30 p-3"><div className="text-[11px] font-medium">{approval.tool}</div><pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{approval.arguments}</pre><div className="mt-2 flex justify-end gap-2"><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, false))} className="luma-btn-ghost h-7 px-2 text-[10px]">Deny</button><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, true))} className="luma-btn-primary h-7 px-2.5 text-[10px]">Approve</button></div></div>)}</div></div>}
                </section>

                <section data-settings-section="advanced" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <h4 className="mb-2 luma-kicker">Advanced agent configuration</h4>
                  <p className="mt-2 text-[10px] leading-5 text-[var(--luma-text-muted)]">Edit the complete AgentConfiguration object, including provider overrides, endpoint, allowed workspaces, enabled tools, permissions, network policy, and recovery behavior. Keep max_concurrent_tasks at 1 while the runtime uses a shared workspace worker.</p>
                  <textarea spellCheck={false} value={configurationDraft} onChange={(event) => setConfigurationDraft(event.target.value)} rows={18} className="luma-input mt-2 w-full resize-y font-mono text-[10px] leading-5" />
                  <div className="mt-2 flex justify-end"><button type="button" disabled={busy || !configurationDraft.trim()} onClick={() => void perform(async () => { let configuration: AgentConfiguration; try { configuration = JSON.parse(configurationDraft) as AgentConfiguration; } catch (reason) { throw new Error(`Invalid JSON: ${reason instanceof Error ? reason.message : String(reason)}`); } if (!configuration || typeof configuration !== "object" || !configuration.permissions || !Array.isArray(configuration.skills) || !Array.isArray(configuration.allowed_workspaces) || !Array.isArray(configuration.enabled_tools)) throw new Error("Configuration must include permissions, skills, allowed_workspaces, and enabled_tools."); await window.luma.updateAgent({ id: selectedAgent.id, configuration }); setConfigurationDraft(JSON.stringify(configuration, null, 2)); })} className="luma-btn-primary h-7 px-3 text-[10px]">Save full configuration</button></div>
                </section>

                <section data-settings-section="memory" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <div>
                    <h4 className="mb-2 luma-kicker">Agent memory</h4>
                    <p className="mb-2 text-[10px] leading-relaxed text-[var(--luma-text-muted)]">This teammate can learn useful preferences and project decisions automatically, and recall them in future tasks.</p>
                    <select value={memoryScope} onChange={(event) => setMemoryScope(event.target.value as MemoryScope)} className="luma-input mb-2 h-8 w-full text-[11px]">
                      <option value="agent">This agent</option><option value="user">Shared user memory</option><option value="project">This workspace</option>
                    </select>
                    <textarea value={memoryContent} onChange={(event) => setMemoryContent(event.target.value)} rows={2} placeholder="A preference or fact this teammate should remember…" className="luma-input w-full resize-y text-[11px]" />
                    <button type="button" disabled={busy || !memoryContent.trim() || (memoryScope === "project" && !workspacePath)} onClick={() => void perform(async () => { const scopeId = memoryScope === "agent" ? selectedAgent.id : memoryScope === "user" ? "default" : workspacePath; await window.luma.addMemory(memoryScope, scopeId, memoryContent.trim()); setMemoryContent(""); setMemoryRevision((revision) => revision + 1); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Save memory</button>
                    <div className="mt-2 max-h-32 space-y-1 overflow-auto">{memoryEntries.map((entry) => <div key={entry.id} className="flex items-start gap-2 border-t border-[var(--luma-border)] py-1.5"><p className="min-w-0 flex-1 whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-secondary)]">{entry.content}</p><button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.deleteMemory(entry.id); setMemoryRevision((revision) => revision + 1); })} className="text-[10px] text-[var(--luma-danger)] hover:underline">Delete</button></div>)}</div>
                  </div>
                </section>
                <section data-settings-section="schedules" className="rounded-lg border border-[var(--luma-border)] p-3">
                  <div>
                    <h4 className="mb-2 luma-kicker">Recurring work</h4>
                    <input value={scheduleTitle} onChange={(event) => setScheduleTitle(event.target.value)} maxLength={120} placeholder="Schedule name" className="luma-input mb-2 h-8 w-full text-[11px]" />
                    <textarea value={schedulePrompt} onChange={(event) => setSchedulePrompt(event.target.value)} rows={2} placeholder="What should this teammate repeat?" className="luma-input w-full resize-y text-[11px]" />
                    <label className="mt-2 block text-[10px] text-[var(--luma-text-muted)]">Repeat every (hours)<input type="number" min={1} max={8760} step={1} value={scheduleIntervalHours} onChange={(event) => setScheduleIntervalHours(event.target.value)} className="luma-input mt-1 h-8 w-full text-[11px]" /></label>
                    <button type="button" disabled={busy || !schedulePrompt.trim() || !scheduleTitle.trim() || !Number.isInteger(Number(scheduleIntervalHours)) || Number(scheduleIntervalHours) < 1 || Number(scheduleIntervalHours) > 8760} onClick={() => void perform(async () => { await window.luma.createSchedule({ agentId: selectedAgent.id, title: scheduleTitle.trim(), prompt: schedulePrompt.trim(), everySeconds: Number(scheduleIntervalHours) * 3600 }); setSchedulePrompt(""); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Create recurring schedule</button>
                    <p className="mt-1 text-[10px] text-[var(--luma-text-muted)]">{schedules.filter((schedule) => schedule.agent_id === selectedAgent.id && schedule.enabled).length} enabled schedules</p>
                    <div className="mt-2 space-y-1">{schedules.filter((schedule) => schedule.agent_id === selectedAgent.id).map((schedule) => <div key={schedule.id} className="flex items-center gap-2 border-t border-[var(--luma-border)] pt-1.5"><span className="min-w-0 flex-1 truncate text-[10px] text-[var(--luma-text-secondary)]">{schedule.title} · {schedule.every_seconds ? `every ${Math.round(schedule.every_seconds / 3600 * 10) / 10}h` : "one-time"} · {schedule.enabled ? `next ${new Date(schedule.next_run_at * 1000).toLocaleString()}` : "Disabled"}</span><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.setScheduleEnabled(schedule.id, !schedule.enabled))} className="text-[10px] text-[var(--luma-accent)] hover:underline">{schedule.enabled ? "Pause" : "Resume"}</button><button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.deleteSchedule(schedule.id); })} className="text-[10px] text-[var(--luma-danger)] hover:underline">Remove</button></div>)}</div>
                  </div>
                </section>
              </div>
            )}
          </div>
        </div>
        {inspectorVisible && <aside className="luma-teammate-inspector" aria-label="Teammate tasks, notes and files">
          <div className="luma-teammate-inspector-tabs">
            <button type="button" data-active={inspectorTab === "tasks"} onClick={() => setInspectorTab("tasks")}>Tasks <span>{agentTasks.length}</span></button>
            <button type="button" data-active={inspectorTab === "notes"} onClick={() => setInspectorTab("notes")}>Notes</button>
            <button type="button" data-active={inspectorTab === "files"} onClick={() => setInspectorTab("files")}>Files</button>
          </div>
          {inspectorTab === "tasks" ? <div className="luma-teammate-inspector-body">
            {agentApprovals.length > 0 && <section>
              <h3 className="is-attention">Needs you</h3>
              {agentApprovals.map((approval) => <div key={approval.id} className="luma-inspector-task is-attention">
                <div className="luma-inspector-task-title">{approval.tool}</div>
                <div className="luma-inspector-task-subtitle">{selectedAgent?.name} is waiting for approval</div>
                <div className="mt-2 flex gap-2"><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, false))}>Deny</button><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, true))}>Approve</button></div>
              </div>)}
            </section>}
            <section>
              <h3>In progress</h3>
              {inspectorActiveTasks.length === 0 ? <p className="luma-inspector-empty">Nothing running right now.</p> : inspectorActiveTasks.map((task) => <button key={task.id} type="button" className={`luma-inspector-task ${selectedTaskId === task.id ? "is-selected" : ""}`} onClick={() => { setSelectedTaskId(task.id); setActiveTab("chat"); }}>
                <span className="luma-inspector-task-avatar"><MascotAvatar seed={selectedAgent?.name ?? "Teammate"} mascot={selectedAgent?.configuration.mascot} size={25} active={task.status === "running"} activity={taskMascotActivity(task)} /></span>
                <span className="min-w-0 flex-1"><span className="luma-inspector-task-title">{task.title || task.request}</span><span className="luma-inspector-task-subtitle">{task.status.replace(/_/g, " ")}</span></span>
              </button>)}
            </section>
            <section>
              <h3>Done</h3>
              {inspectorCompletedTasks.length === 0 ? <p className="luma-inspector-empty">Completed work will appear here.</p> : inspectorCompletedTasks.map((task) => <button key={task.id} type="button" className={`luma-inspector-task ${selectedTaskId === task.id ? "is-selected" : ""}`} onClick={() => { setSelectedTaskId(task.id); setActiveTab("chat"); }}>
                <span className="luma-inspector-task-avatar"><MascotAvatar seed={selectedAgent?.name ?? "Teammate"} mascot={selectedAgent?.configuration.mascot} size={25} /></span>
                <span className="min-w-0 flex-1"><span className="luma-inspector-task-title">{task.title || task.request}</span><span className="luma-inspector-task-subtitle">{task.status} · {new Date(task.created_at * 1000).toLocaleDateString()}</span></span>
              </button>)}
            </section>
          </div> : inspectorTab === "notes" ? <div className="luma-teammate-inspector-body">
            <section><h3>Notes for {selectedAgent?.name ?? "this teammate"}</h3>
              <textarea value={memoryContent} onChange={(event) => setMemoryContent(event.target.value)} rows={3} placeholder="Add a note or something this teammate should remember…" className="luma-input w-full resize-y text-[11px]" />
              <button type="button" disabled={busy || !memoryContent.trim() || !selectedAgent} onClick={() => {
                const agent = selectedAgent;
                if (!agent) return;
                void perform(async () => { await window.luma.addMemory("agent", agent.id, memoryContent.trim()); setMemoryContent(""); setMemoryRevision((revision) => revision + 1); });
              }} className="luma-note-save">Save note</button>
              {memoryEntries.length === 0 ? <p className="luma-inspector-empty mt-4">Saved notes and memories will appear here.</p> : memoryEntries.map((entry) => <div key={entry.id} className="luma-inspector-memory"><span>{entry.content}</span><button type="button" disabled={busy} aria-label="Delete note" onClick={() => void perform(async () => { await window.luma.deleteMemory(entry.id); setMemoryRevision((revision) => revision + 1); })}>×</button></div>)}
            </section>
          </div> : <div className="luma-teammate-inspector-body">
            <section><h3>Workspace files</h3><p className="mb-3 truncate text-[10px] text-[var(--luma-text-muted)]" title={workspacePath}>{workspacePath || "No workspace selected"}</p>{filesLoading ? <p className="luma-inspector-empty">Loading files…</p> : workspaceFiles.length ? <WorkspaceTree nodes={workspaceFiles} selectedFile={null} onFileSelect={onOpenFile} /> : <p className="luma-inspector-empty">No files found in this workspace.</p>}</section>
          </div>}
        </aside>}
        </div>
        {createOpen && <div className="luma-create-overlay fixed inset-0 z-[120] flex items-center justify-center bg-black/60 p-5" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) setCreateOpen(false); }}>
          <form className="luma-create-dialog flex max-h-[min(820px,92vh)] w-full max-w-[820px] flex-col overflow-hidden rounded-xl border border-[var(--luma-border-strong)] bg-[var(--luma-surface)] shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="create-teammate-title" onSubmit={(event) => { event.preventDefault(); void perform(async () => {
            const configuration: AgentConfiguration = {
              system_prompt: agentPersonality.trim(), mascot: agentMascot, use_global_context: agentUseGlobalContext, skills: [],
              provider: null, model: null, endpoint: null, workspace: agentWorkspace.trim() || workspacePath || null,
              allowed_workspaces: agentWorkspace.trim() || workspacePath ? [agentWorkspace.trim() || workspacePath] : [], enabled_tools: [],
              network_access: true,
              permissions: { allowed_tools: ["read_file", "list_directory", "search_files", "git_status", "git_diff", "find_symbols", "find_references", "git_log", "git_show", "git_blame", "git_branches", "file_metadata", "find_todos", "count_lines", "inspect_project", "list_skills", "load_skill", "web_search", "fetch_url"], denied_tools: [], auto_approve_read_only: true, require_approval_for_mutations: true, allow_network: true },
              max_concurrent_tasks: 1, recover_interrupted_tasks: false,
            };
            const created = await window.luma.createAgent({ name: agentName.trim(), description: agentDescription.trim() || null, kind: "teammates", configuration });
            setSelectedAgentId(created.id); setSelectedTaskId(null); setCreateOpen(false); setAgentName(""); setAgentDescription(""); setAgentPersonality(""); setAgentInitialMemory(""); setAgentWorkspace(""); setAgentMascot("auto"); setAgentUseGlobalContext(true); setActiveTab("chat");
            if (agentInitialMemory.trim()) {
              try { await window.luma.addMemory("agent", created.id, agentInitialMemory.trim()); }
              catch (reason) { setError(reason instanceof Error ? `Teammate created, but initial memory could not be saved: ${reason.message}` : "Teammate created, but initial memory could not be saved."); }
            }
          }); }}>
            <header className="luma-create-header flex items-center justify-between border-b border-[var(--luma-border)] px-6 py-4"><div><div className="luma-kicker mb-1">TEAMMATES <span aria-hidden="true">/</span> CREATE</div><h3 id="create-teammate-title" className="text-[18px] font-semibold tracking-tight">Create a teammate</h3><p className="mt-1 text-[11px] text-[var(--luma-text-muted)]">Define who they are, then choose how they work.</p></div><button type="button" aria-label="Close dialog" onClick={() => setCreateOpen(false)} className="luma-create-close" disabled={busy}>×</button></header>
            <div className="luma-create-body min-h-0 flex-1 overflow-auto">
              <div className="luma-create-column luma-create-identity">
                <div className="luma-create-section-heading"><span>01</span><div><h4>Identity</h4><p>Start with the basics and make them recognizable.</p></div></div>
                <label className="luma-create-field">Name <span>Required</span><input autoFocus required maxLength={120} value={agentName} onChange={(event) => setAgentName(event.target.value)} placeholder="e.g. Researcher" className="luma-input mt-1.5 h-10 w-full text-[13px]" /></label>
                <label className="luma-create-field">What do they do?<input value={agentDescription} onChange={(event) => setAgentDescription(event.target.value)} placeholder="A short description of their role" className="luma-input mt-1.5 h-10 w-full text-[12px]" /></label>
                <div className="luma-create-mascot"><div className="luma-create-field-label">Appearance</div><MascotCustomizer value={agentMascot} seed={agentName} onChange={setAgentMascot} /></div>
              </div>
              <div className="luma-create-column luma-create-configuration">
                <div className="luma-create-section-heading"><span>02</span><div><h4>How they work</h4><p>Shape their behavior and give them useful context.</p></div></div>
                <label className="luma-create-field">Personality <span>Optional</span><textarea id="teammate-personality" value={agentPersonality} onChange={(event) => setAgentPersonality(event.target.value)} rows={4} placeholder="How should this teammate think, communicate and approach tasks? Leave blank for Luma's defaults." className="luma-input mt-1.5 w-full resize-y text-[12px]" /></label>
                <label className="luma-create-context"><input type="checkbox" checked={agentUseGlobalContext} onChange={(event) => setAgentUseGlobalContext(event.target.checked)} className="mt-0.5 accent-[var(--luma-accent)]" /><span><strong>Share global context</strong><small>Use the global skills, memory, connectors, etc.</small></span></label>
                <label className="luma-create-field">Initial memory <span>Optional</span><textarea value={agentInitialMemory} onChange={(event) => setAgentInitialMemory(event.target.value)} rows={3} placeholder="Facts or preferences this teammate should remember…" className="luma-input mt-1.5 w-full resize-y text-[12px]" /></label>
                <label className="luma-create-field">Workspace <span>Optional</span><input value={agentWorkspace} onChange={(event) => setAgentWorkspace(event.target.value)} placeholder={workspacePath || "Use current workspace"} className="luma-input mt-1.5 h-10 w-full text-[12px]" /></label>
                <div className="luma-create-security"><span aria-hidden="true">⌁</span><p><strong>Personal-agent ready.</strong> Web search and public webpages are available. External requests that can change data and workspace mutations require approval.</p></div>
              </div>
            </div>
            <footer className="luma-create-footer flex items-center justify-between border-t border-[var(--luma-border)] px-6 py-4"><span>{agentName.trim() ? "Ready when you are." : "A name is all you need to get started."}</span><div className="flex gap-2"><button type="button" disabled={busy} onClick={() => setCreateOpen(false)} className="luma-btn-secondary h-9 px-3 text-[11px]">Cancel</button><button type="submit" disabled={busy || !agentName.trim()} className="luma-btn-primary h-9 px-4 text-[11px]">{busy ? "Creating…" : "Create teammate"}</button></div></footer>
          </form>
        </div>}
        <footer className="flex shrink-0 items-center justify-between border-t border-[var(--luma-border)] px-5 py-2 text-[10px] text-[var(--luma-text-muted)]"><span>Work continues independently of this page.</span><button type="button" onClick={() => void refresh()} className="hover:text-[var(--luma-text)]">Refresh</button></footer>
    </section>
  );
}

function DesktopTour({ onFinish }: { onFinish: () => void }) {
  const [step, setStep] = useState(0);
  const steps = [
    { title: "Welcome to Luma", body: "Luma brings coding work and persistent teammates into one focused desktop workspace. This quick tour shows you where everything lives." },
    { title: "Your projects and history", body: "Create Projects and give each one its own folder. Your recent conversations stay in the sidebar, ready to pick up where you left off." },
    { title: "Code, files, and changes", body: "Use Code to talk through a task, Files to inspect your workspace, and Changes to review what has been modified." },
    { title: "Meet your teammates", body: "Switch to Teammates to chat with persistent agents, tune their instructions, manage memory and skills, and review their ongoing work." },
    { title: "Make Luma yours", body: "Open Settings to adjust appearance and your model. You can replay this tour any time from Settings → General." },
  ];
  const current = steps[step];
  return (
    <div className="luma-tour-backdrop fixed inset-0 z-[180] flex items-center justify-center p-5" role="presentation">
      <section className="luma-tour-panel w-full max-w-[440px] rounded-xl border border-[var(--luma-border-strong)] bg-[var(--luma-surface)] p-6 shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="luma-tour-title">
        <div className="mb-5 flex items-center justify-between"><span className="luma-kicker">A QUICK TOUR</span><span className="text-[11px] text-[var(--luma-text-muted)]">{step + 1} of {steps.length}</span></div>
        <div className="mb-5 flex gap-1.5" aria-hidden="true">{steps.map((item, index) => <span key={item.title} className={`h-1 flex-1 rounded-full ${index <= step ? "bg-[var(--luma-accent)]" : "bg-[var(--luma-border-strong)]"}`} />)}</div>
        <h2 id="luma-tour-title" className="text-[20px] font-semibold tracking-tight">{current.title}</h2>
        <p className="mt-3 min-h-[76px] text-[13px] leading-6 text-[var(--luma-text-secondary)]">{current.body}</p>
        <footer className="mt-6 flex items-center justify-between"><button type="button" onClick={onFinish} className="text-[12px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">Skip tour</button><div className="flex items-center gap-2">{step > 0 && <button type="button" onClick={() => setStep((value) => value - 1)} className="luma-btn-secondary h-8 px-3 text-[12px]">Back</button>}<button type="button" onClick={() => step === steps.length - 1 ? onFinish() : setStep((value) => value + 1)} className="luma-btn-primary h-8 px-3 text-[12px]">{step === steps.length - 1 ? "Finish" : "Next"}</button></div></footer>
      </section>
    </div>
  );
}

function CoreConnectionScreen({ state, error, onRetry, onSwitchCore }: {
  state: "connecting" | "failed";
  error: string;
  onRetry: () => void;
  onSwitchCore: (settings: { mode: "bundled" | "path" | "custom"; customPath: string | null }) => Promise<void>;
}) {
  const [showCoreOptions, setShowCoreOptions] = useState(false);
  const [coreMode, setCoreMode] = useState<"bundled" | "path" | "custom">("bundled");
  const [corePath, setCorePath] = useState("");
  const [coreBinary, setCoreBinary] = useState("");
  const [environmentOverride, setEnvironmentOverride] = useState(false);
  const [coreOptionsLoading, setCoreOptionsLoading] = useState(false);
  const [coreSwitching, setCoreSwitching] = useState(false);
  const [coreOptionsError, setCoreOptionsError] = useState("");

  useEffect(() => {
    if (state !== "failed") return;
    let active = true;
    setCoreOptionsLoading(true);
    void window.luma.getCoreBinary().then((settings) => {
      if (!active) return;
      setCoreMode(settings.mode);
      setCorePath(settings.customPath ?? "");
      setCoreBinary(settings.binary);
      setEnvironmentOverride(settings.environmentOverride);
    }).catch((reason: unknown) => {
      if (active) setCoreOptionsError(reason instanceof Error ? reason.message : String(reason));
    }).finally(() => {
      if (active) setCoreOptionsLoading(false);
    });
    return () => { active = false; };
  }, [state]);

  async function browseCoreBinary() {
    try {
      const selected = await window.luma.chooseCoreBinary();
      if (selected) {
        setCorePath(selected);
        setCoreMode("custom");
        setCoreOptionsError("");
      }
    } catch (reason) {
      setCoreOptionsError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function switchCoreBinary() {
    setCoreSwitching(true);
    setCoreOptionsError("");
    try {
      await onSwitchCore({ mode: coreMode, customPath: coreMode === "custom" ? corePath : null });
    } catch (reason) {
      setCoreOptionsError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setCoreSwitching(false);
    }
  }

  return <div className="luma-connection-screen" role={state === "failed" ? "alert" : "status"} aria-live="polite">
    <div className="luma-connection-mark">L</div>
    {state === "connecting" ? <>
      <span className="luma-connection-spinner" aria-hidden="true" />
      <p className="luma-connection-title">Connecting...</p>
      <p className="luma-connection-detail">Starting Luma Core</p>
    </> : <>
      <p className="luma-connection-title">Couldn't connect to Luma Core</p>
      <p className="luma-connection-detail">{error || "The core process failed to start."}</p>
      <button type="button" onClick={onRetry} className="luma-btn-primary mt-3 h-8 px-4 text-[11px]">Try again</button>
      <div className="mt-5 w-full max-w-[360px] border-t border-[var(--luma-border)] pt-4 text-left">
        <button type="button" aria-expanded={showCoreOptions} onClick={() => setShowCoreOptions((open) => !open)} className="flex w-full items-center justify-between text-[12px] font-medium text-[var(--luma-text-secondary)] hover:text-[var(--luma-text)]">
          <span>Change Luma Core source</span><span aria-hidden="true">{showCoreOptions ? "−" : "+"}</span>
        </button>
        {showCoreOptions && <div className="mt-3">
          <p className="mb-2 text-[11px] leading-5 text-[var(--luma-text-muted)]">Choose another core installation. Luma will save the selection and restart the connection.</p>
          <label htmlFor="luma-failure-core-source" className="mb-1.5 block text-[11px] font-medium text-[var(--luma-text-secondary)]">Core source</label>
          <select id="luma-failure-core-source" value={coreMode} disabled={coreOptionsLoading || coreSwitching || environmentOverride} onChange={(event) => setCoreMode(event.target.value as "bundled" | "path" | "custom")} className="luma-input h-9 w-full text-[12px]">
            <option value="bundled">Bundled with Luma</option>
            <option value="path">Global executable on PATH</option>
            <option value="custom">Custom executable…</option>
          </select>
          {coreMode === "custom" && <div className="mt-2 flex gap-2">
            <input aria-label="Custom Luma core executable path" readOnly value={corePath} placeholder="Choose an executable file…" className="luma-input h-9 min-w-0 flex-1 text-[10px]" />
            <button type="button" disabled={coreSwitching || environmentOverride} onClick={() => void browseCoreBinary()} className="rounded-md border border-[var(--luma-border)] px-3 text-[11px] text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] disabled:opacity-40">Browse…</button>
          </div>}
          {coreBinary && <p className="mt-2 break-all font-mono text-[10px] leading-4 text-[var(--luma-text-muted)]">Current: {coreBinary}</p>}
          {environmentOverride && <p className="mt-2 text-[10px] leading-4 text-[var(--luma-warning)]">LUMA_BINARY overrides this setting. Change or unset that environment variable before switching sources.</p>}
          {coreOptionsError && <p role="alert" className="mt-2 text-[10px] leading-4 text-[var(--luma-danger)]">{coreOptionsError}</p>}
          <div className="mt-3 flex justify-end"><button type="button" disabled={coreOptionsLoading || coreSwitching || environmentOverride || (coreMode === "custom" && !corePath.trim())} onClick={() => void switchCoreBinary()} className="luma-btn-primary h-8 px-3 text-[11px] disabled:cursor-not-allowed disabled:opacity-40">{coreSwitching ? "Switching core…" : "Switch and restart"}</button></div>
        </div>}
      </div>
    </>}
  </div>;
}

export default function App() {
  const [mode, setMode] = useState<AppMode>("agent");
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    try { const saved = Number(window.localStorage.getItem("luma.sidebar-width")); return Number.isFinite(saved) && saved >= 200 && saved <= 420 ? saved : 280; }
    catch { return 280; }
  });
  const [codeInspectorVisible, setCodeInspectorVisible] = useState(true);
  const [teammatesSidebar, setTeammatesSidebar] = useState<React.ReactNode>(null);
  const [sidebarBlur, setSidebarBlur] = useState(() => {
    try {
      return window.localStorage.getItem("luma.sidebar-blur") !== "false";
    } catch {
      return true;
    }
  });
  const [compactHistory, setCompactHistory] = useState(() => {
    try { return window.localStorage.getItem("luma.compact-history") !== "false"; }
    catch { return true; }
  });
  const [reducedMotion, setReducedMotion] = useState(() => {
    try { return window.localStorage.getItem("luma.reduce-motion") === "true"; }
    catch { return false; }
  });
  const [sendWithEnter, setSendWithEnter] = useState(() => {
    try { return window.localStorage.getItem("luma.send-with-enter") !== "false"; }
    catch { return true; }
  });
  const [autoScrollToLatest, setAutoScrollToLatest] = useState(() => {
    try { return window.localStorage.getItem("luma.auto-scroll-latest") !== "false"; }
    catch { return true; }
  });
  const [showToolActivity, setShowToolActivity] = useState(() => {
    try { return window.localStorage.getItem("luma.show-tool-activity") !== "false"; }
    catch { return true; }
  });

  useEffect(() => {
    try {
      window.localStorage.setItem("luma.sidebar-blur", String(sidebarBlur));
    } catch {
      // Keep the preference in component state when storage is unavailable.
    }
  }, [sidebarBlur]);

  useEffect(() => {
    try { window.localStorage.setItem("luma.sidebar-width", String(sidebarWidth)); } catch { /* Keep the width for this session. */ }
  }, [sidebarWidth]);

  useEffect(() => {
    document.documentElement.dataset.compactHistory = String(compactHistory);
    document.documentElement.dataset.reducedMotion = String(reducedMotion);
    try {
      window.localStorage.setItem("luma.compact-history", String(compactHistory));
      window.localStorage.setItem("luma.reduce-motion", String(reducedMotion));
      window.localStorage.setItem("luma.send-with-enter", String(sendWithEnter));
      window.localStorage.setItem("luma.auto-scroll-latest", String(autoScrollToLatest));
      window.localStorage.setItem("luma.show-tool-activity", String(showToolActivity));
    } catch {
      // Keep the current preferences for this session if storage is unavailable.
    }
  }, [compactHistory, reducedMotion, sendWithEnter, autoScrollToLatest, showToolActivity]);

  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [workspacePath, setWorkspacePath] = useState("");
  const [workspaceSwitching, setWorkspaceSwitching] = useState(false);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("appearance");
  const [codeHandoffPrompt, setCodeHandoffPrompt] = useState("");
  const [modelPickerScope, setModelPickerScope] = useState<"global" | "session" | null>(null);

  const [connected, setConnected] = useState(false);
  const [titlebarStatus, setTitlebarStatus] = useState<TitlebarStatus | null>(null);
  const [connectionState, setConnectionState] = useState<"connecting" | "connected" | "failed">("connecting");
  const [connectionError, setConnectionError] = useState("");

  const [setupComplete, setSetupComplete] = useState(false);
  const [tourOpen, setTourOpen] = useState(false);
  const tourChecked = useRef(false);

  const [setupConfig, setSetupConfig] = useState<SetupConfig | null>(null);
  const activeProjectId = useWorkspaceStore((state) => state.activeProjectId);
  const setProjectWorkspace = useWorkspaceStore((state) => state.setProjectWorkspace);
  const selectProject = useWorkspaceStore((state) => state.selectProject);

  useEffect(() => {
    if (!setupComplete || tourChecked.current) return;
    tourChecked.current = true;
    try {
      if (window.localStorage.getItem("luma.desktop-tour") !== "completed") setTourOpen(true);
    } catch {
      setTourOpen(true);
    }
  }, [setupComplete]);

  const finishTour = useCallback(() => {
    try { window.localStorage.setItem("luma.desktop-tour", "completed"); } catch { /* Tour can still be dismissed for this session. */ }
    setTourOpen(false);
  }, []);

  const chooseWorkspaceForCurrentScope = useCallback(async (): Promise<boolean> => {
    setWorkspaceSwitching(true);
    try {
      const state = useWorkspaceStore.getState();
      const activeProject = state.projects.find((project) => project.id === state.activeProjectId);
      if (activeProject) {
        const selected = await window.luma.chooseProjectWorkspace();
        if (selected.canceled || !selected.path) return false;
        state.setProjectWorkspace(activeProject.id, selected.path);
        const switched = await window.luma.setWorkspace(selected.path);
        setWorkspacePath(switched.path);
        return true;
      }
      const result = await window.luma.chooseWorkspace();
      if (result.canceled) return false;
      setWorkspacePath(result.path);
      return true;
    } finally {
      setWorkspaceSwitching(false);
    }
  }, []);

  const handleProjectFolderChange = useCallback(async (projectId: string, path: string) => {
    setProjectWorkspace(projectId, path);
    if (useWorkspaceStore.getState().activeProjectId !== projectId) return;
    setWorkspaceSwitching(true);
    try {
      const switched = await window.luma.setWorkspace(path);
      setWorkspacePath(switched.path);
    } finally {
      setWorkspaceSwitching(false);
    }
  }, [setProjectWorkspace]);

  useEffect(() => {
    document.documentElement.dataset.platform = window.luma.platform;
  }, []);

  useEffect(() => {
    window.luma.getWorkspace().then((workspaceInfo) => {
      setWorkspacePath(workspaceInfo.path);
    }).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!setupComplete) return;
    let cancelled = false;
    const activateWorkspace = async () => {
      setWorkspaceSwitching(true);
      try {
        if (activeProjectId) {
          const project = useWorkspaceStore.getState().projects.find((item) => item.id === activeProjectId);
          if (!project) return;
          let projectPath = project.workspacePath;
          if (!projectPath) {
            const selected = await window.luma.chooseProjectWorkspace();
            if (cancelled) return;
            if (selected.canceled || !selected.path) {
              selectProject(null);
              return;
            }
            projectPath = selected.path;
            setProjectWorkspace(project.id, projectPath);
          }
          const result = await window.luma.setWorkspace(projectPath);
          if (!cancelled) setWorkspacePath(result.path);
          return;
        }
        const result = await window.luma.getDefaultWorkspace();
        if (cancelled) return;
        const active = await window.luma.getWorkspace();
        if (active.path !== result.path) {
          const switched = await window.luma.setWorkspace(result.path);
          if (!cancelled) setWorkspacePath(switched.path);
        } else {
          setWorkspacePath(active.path);
        }
      } catch (error) {
        if (!cancelled) window.alert(error instanceof Error ? error.message : String(error));
      } finally {
        if (!cancelled) setWorkspaceSwitching(false);
      }
    };
    void activateWorkspace();
    return () => { cancelled = true; };
  }, [activeProjectId, selectProject, setProjectWorkspace, setupComplete]);

  useEffect(() => {
    let mounted = true;
    let loadingSetup = false;

    const handleReady = async () => {
      if (!mounted || loadingSetup) return;
      loadingSetup = true;
      setConnected(true);
      setConnectionError("");
      try {
        const config = await window.luma.getSetupConfig();
        if (!mounted) return;
        if (config) {
          setSetupConfig(config);
          setSetupComplete(true);
        }
        setConnectionState("connected");
      } catch (reason) {
        if (!mounted) return;
        setConnectionError(reason instanceof Error ? reason.message : String(reason));
        setConnectionState("failed");
      } finally {
        loadingSetup = false;
      }
    };

    const unsubscribe = window.luma.onEvent((event: unknown) => {
      if (typeof event !== "object" || event === null) return;
      const message = event as { type?: string; data?: { message?: string; category?: string } };
      if (message.type === "ready") void handleReady();
      if (message.type === "error" && message.data?.category === "initialization_failed") {
        setConnected(false);
        setConnectionError(message.data.message ?? "Luma Core failed to start.");
        setConnectionState("failed");
      }
      if (message.type === "process_exited") {
        setConnected(false);
        setConnectionError("Luma Core stopped unexpectedly.");
        setConnectionState("failed");
      }
    });

    window.luma
      .getStatus()
      .then((status: { connected: boolean }) => {
        if (mounted && status.connected) void handleReady();
      })
      .catch((reason: unknown) => {
        if (!mounted) return;
        setConnectionError(reason instanceof Error ? reason.message : String(reason));
        setConnectionState("failed");
      });

    const unsubscribeSettings = window.luma.onOpenSettings(() => {
      setSettingsOpen(true);
    });
    const unsubscribeNewChat = window.luma.onNewChat(() => {
      if (modeRef.current === "teammates") {
        window.dispatchEvent(new Event("luma:new-teammate-task"));
      } else {
        useWorkspaceStore.getState().startNewChat();
        setMode("agent");
      }
    });
    const unsubscribeOpenWorkspace = window.luma.onOpenWorkspace(() => {
      void chooseWorkspaceForCurrentScope().catch((error) => {
        window.alert(error instanceof Error ? error.message : String(error));
      });
    });
    const unsubscribeToggleSidebar = window.luma.onToggleSidebar(() => {
      setSidebarVisible((visible) => !visible);
    });
    const unsubscribeWorkspace = window.luma.onWorkspaceChanged((path) => {
      setWorkspacePath(path);
      setSelectedFile(null);
    });
    const unsubscribeTitlebarStatus = window.luma.onTitlebarStatus((status) => {
      setTitlebarStatus(status ? { text: status.text, color: status.color as TitlebarStatus["color"] } : null);
    });

    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "b") {
        event.preventDefault();
        setSidebarVisible((visible) => !visible);
      }
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      mounted = false;
      unsubscribe();
      unsubscribeSettings();
      unsubscribeNewChat();
      unsubscribeOpenWorkspace();
      unsubscribeToggleSidebar();
      unsubscribeWorkspace();
      unsubscribeTitlebarStatus();
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [chooseWorkspaceForCurrentScope]);

  if (connectionState !== "connected") {
    return <CoreConnectionScreen
      state={connectionState}
      error={connectionError}
      onRetry={() => {
        setConnectionError("");
        setConnectionState("connecting");
        void window.luma.retryConnection().then(async () => {
          const config = await window.luma.getSetupConfig();
          setConnected(true);
          if (config) {
            setSetupConfig(config);
            setSetupComplete(true);
          }
          setConnectionState("connected");
        }).catch((reason: unknown) => {
          setConnectionError(reason instanceof Error ? reason.message : String(reason));
          setConnectionState("failed");
        });
      }}
      onSwitchCore={async (settings) => {
        setConnectionError("");
        setConnectionState("connecting");
        try {
          await window.luma.setCoreBinary(settings);
          const config = await window.luma.getSetupConfig();
          setConnected(true);
          if (config) {
            setSetupConfig(config);
            setSetupComplete(true);
          }
          setConnectionState("connected");
        } catch (reason) {
          setConnectionError(reason instanceof Error ? reason.message : String(reason));
          setConnectionState("failed");
          throw reason;
        }
      }}
    />;
  }

  if (!setupComplete) {
    return (
      <div className="luma-app-enter"><SetupWizard
          onComplete={() => {
            void window.luma.getSetupConfig().then((config) => {
              setSetupConfig(config);
              setSetupComplete(true);
            }).catch((reason: unknown) => {
              setConnectionError(reason instanceof Error ? reason.message : String(reason));
            });
          }}
        /></div>
    );
  }

  return (
    <div
      className="
        luma-app-shell luma-app-enter flex h-screen w-screen
        flex-row overflow-hidden
        bg-[var(--luma-bg)]
        text-[var(--luma-text)]
      "
      data-workspace={mode === "teammates" ? "teammates" : "code"}
      data-sidebar-open={sidebarVisible}
    >
      {/* Live region for screen reader announcements */}
      <div id="luma-announcer" aria-live="polite" className="sr-only" />
      {tourOpen && <DesktopTour onFinish={finishTour} />}

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        initialSection={settingsSection}
        connected={connected}
        setupConfig={setupConfig}
        workspacePath={workspacePath}
        sidebarBlur={sidebarBlur}
        onSidebarBlurChange={setSidebarBlur}
        compactHistory={compactHistory}
        onCompactHistoryChange={setCompactHistory}
        reducedMotion={reducedMotion}
        onReducedMotionChange={setReducedMotion}
        sendWithEnter={sendWithEnter}
        onSendWithEnterChange={setSendWithEnter}
        autoScrollToLatest={autoScrollToLatest}
        onAutoScrollToLatestChange={setAutoScrollToLatest}
        showToolActivity={showToolActivity}
        onShowToolActivityChange={setShowToolActivity}
        onChangeModel={() => { setSettingsOpen(false); setModelPickerScope("global"); }}
        onOpenTour={() => { setSettingsOpen(false); setTourOpen(true); }}
        onRerunSetup={() => {
          setSettingsOpen(false);
          useSetupStore.getState().reset();
          setSetupComplete(false);
        }}
      />
      {modelPickerScope && <ModelPickerDialog scope={modelPickerScope} onClose={() => setModelPickerScope(null)} onChanged={(config) => { if (modelPickerScope === "global") setSetupConfig(config); }} />}

      <div className="flex min-h-0 min-w-0 flex-1">
        <div
          className="luma-sidebar-slot"
          style={{ width: sidebarVisible ? sidebarWidth : 0, flexBasis: sidebarVisible ? sidebarWidth : 0 }}
          data-open={sidebarVisible}
          aria-hidden={!sidebarVisible}
          inert={!sidebarVisible || workspaceSwitching}
        >
          <Sidebar mode={mode} onOpenChat={() => setMode("agent")} onOpenTeammates={() => setMode("teammates")} onProjectFolderChange={handleProjectFolderChange} teammatesSidebar={teammatesSidebar} sidebarBlur={sidebarBlur} sidebarVisible={sidebarVisible} connected={connected} onToggleSidebar={() => setSidebarVisible((visible) => !visible)} onSettings={(section) => { setSettingsSection(section); setSettingsOpen(true); }} />
        </div>
        {sidebarVisible && <div className="luma-sidebar-resize" role="separator" aria-orientation="vertical" aria-label="Resize sidebar" aria-valuemin={200} aria-valuemax={420} aria-valuenow={sidebarWidth} tabIndex={0}
          onPointerDown={(event) => { event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId); event.currentTarget.dataset.dragging = "true"; }}
          onPointerMove={(event) => { if (event.currentTarget.dataset.dragging !== "true") return; const slot = event.currentTarget.previousElementSibling as HTMLElement | null; if (slot) setSidebarWidth(Math.max(200, Math.min(420, event.clientX - slot.getBoundingClientRect().left))); }}
          onPointerUp={(event) => { event.currentTarget.dataset.dragging = "false"; }}
          onKeyDown={(event) => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); setSidebarWidth((width) => Math.max(200, Math.min(420, width + (event.key === "ArrowRight" ? 12 : -12)))); } }} />}

        <main className="luma-main-surface relative min-w-0 flex-1 overflow-hidden bg-[var(--luma-bg)]" aria-busy={workspaceSwitching} inert={workspaceSwitching}>
          {!sidebarVisible && <button type="button" className="luma-sidebar-reopen" onClick={() => setSidebarVisible(true)} aria-label="Open sidebar" title={`Open sidebar (${window.luma.platform === "darwin" ? "⌘" : "Ctrl"}+B)`}>
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /></svg>
          </button>}
          <div key={`${mode}:${workspacePath}`} className="luma-view-transition h-full min-w-0">
          {mode === "teammates" && <TeammatesPanel onSidebarChange={setTeammatesSidebar} onOpenFile={(path) => { setSelectedFile(path); setMode("ide"); }} onHandoffToCode={(prompt) => { setCodeHandoffPrompt(prompt); setMode("agent"); }} sendWithEnter={sendWithEnter} onSendWithEnterChange={setSendWithEnter} autoScrollToLatest={autoScrollToLatest} onAutoScrollToLatestChange={setAutoScrollToLatest} showToolActivity={showToolActivity} onShowToolActivityChange={setShowToolActivity} onOpenSettings={() => { setSettingsSection("behavior"); setSettingsOpen(true); }} titlebarStatus={titlebarStatus} />}
          {mode === "agent" && <div className="luma-code-workspace flex h-full min-w-0">
            <div className="min-w-0 flex-1"><AgentView onChangeModel={setModelPickerScope} sendWithEnter={sendWithEnter} onSendWithEnterChange={setSendWithEnter} autoScrollToLatest={autoScrollToLatest} onAutoScrollToLatestChange={setAutoScrollToLatest} showToolActivity={showToolActivity} onShowToolActivityChange={setShowToolActivity} onOpenSettings={() => { setSettingsSection("behavior"); setSettingsOpen(true); }} workspacePath={workspacePath} onToggleInspector={() => setCodeInspectorVisible((visible) => !visible)} inspectorVisible={codeInspectorVisible} onOpenEditor={() => setMode("ide")} handoffPrompt={codeHandoffPrompt} onHandoffConsumed={() => setCodeHandoffPrompt("")} titlebarStatus={titlebarStatus} /></div>
            {codeInspectorVisible && <CodeInspector workspacePath={workspacePath} onOpenFile={(path) => { setSelectedFile(path); setMode("ide"); }} />}
          </div>}

          {mode === "ide" && (
            <IDEView
              selectedFile={selectedFile}
              onFileSelect={setSelectedFile}
              onWorkspaceChange={setWorkspacePath}
              onChooseWorkspace={chooseWorkspaceForCurrentScope}
            />
          )}

          {mode === "changes" && <ChangesView onOpenFile={(path) => { setSelectedFile(path); setMode("ide"); }} />}
          </div>
          {workspaceSwitching && <div className="luma-workspace-switch-overlay"><span className="luma-progress-indicator" />Opening Project workspace…</div>}
        </main>
      </div>
    </div>
  );
}
