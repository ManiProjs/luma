import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useFocusTrap } from "./hooks/useFocusTrap";

import Editor from "@monaco-editor/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import LumaWordmark, { LumaMark } from "./components/LumaWordmark";

import { themes, useTheme } from "./renderer/Theme";

import { useLuma, type AgentTool } from "./renderer/hooks/useLuma";

import SetupWizard from "./setup/SetupWizard";
import ModelPickerDialog from "./components/ModelPickerDialog";
import ExtensionsPanel from "./components/ExtensionsPanel";

import type { SetupConfig } from "./stores/setupStore";

import { useWorkspaceStore, type Chat } from "./stores/workspaceStore";
import { MascotAvatar } from "./components/MascotAvatar";
import type { AgentConfiguration, AgentRecord, ApprovalRecord, MemoryEntry, MemoryScope, ScheduleRecord, TaskRecord, RuntimeEvent } from "../../shared/protocol";
import { formatChatPrompt, type ChatAttachment } from "../../shared/attachments";
import { MascotCustomizer } from "./components/MascotCustomizer";

type AppMode = "agent" | "ide" | "changes" | "teammates";
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

function PencilIcon() {
  return <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m16 5 3 3M4 20l4.5-1 11-11a2.1 2.1 0 0 0-3-3l-11 11L4 20Z" /></svg>;
}

function TrashIcon() {
  return <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 6h18M8 6V4h8v2m3 0-1 14H6L5 6m4 4v6m6-6v6" /></svg>;
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

function TopBar({
  mode,
  setMode,
  connected,
  onSettings,
  sidebarBlur,
  sidebarVisible,
  onToggleSidebar,
}: {
  mode: AppMode;
  setMode: (mode: AppMode) => void;
  connected: boolean;
  onSettings: () => void;
  sidebarBlur: boolean;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
}) {
  const platform = window.luma.platform;
  const macOS = platform === "darwin";
  return (
    <header
      className={`
        drag-region
        luma-topbar flex h-16 shrink-0 items-center
        border-b border-[var(--luma-border)]
        ${sidebarBlur ? "luma-topbar-glass" : "bg-[var(--luma-surface)]"}
      `}
    >
      <div
        className={`
          flex h-full w-full items-center
          ${macOS ? "pl-[82px]" : "pl-5"} pr-4
          pointer-events-none
        `}
      >
        <button
          type="button"
          onClick={onToggleSidebar}
          aria-label={sidebarVisible ? "Hide sidebar" : "Show sidebar"}
          aria-pressed={sidebarVisible}
          title={`${sidebarVisible ? "Hide" : "Show"} sidebar (${macOS ? "⌘" : "Ctrl"}+B)`}
          className="no-drag pointer-events-auto ml-3 flex h-8 w-8 items-center justify-center rounded-md text-[var(--luma-text-muted)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)]"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <rect x="3" y="4" width="18" height="16" rx="2" />
            <path d="M9 4v16" />
          </svg>
        </button>

        <div className="no-drag pointer-events-auto ml-2">
          <LumaWordmark compact section={mode === "teammates" ? "teammates" : undefined} />
        </div>

        <div className="luma-mode-picker no-drag pointer-events-auto relative ml-5 flex h-9 min-w-[220px] items-center rounded-lg border border-[var(--luma-border-strong)] bg-[var(--luma-bg)] p-1" role="group" aria-label="Luma workspace">
          <span
            aria-hidden="true"
            className="absolute bottom-1 left-1 top-1 w-[calc((100%_-_8px)_/_2)] rounded-md bg-[var(--luma-surface-raised)] shadow-sm transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none"
            style={{
              transform: `translateX(${mode === "teammates" ? 100 : 0}%)`,
            }}
          />
          <ModeButton active={mode !== "teammates"} onClick={() => setMode("agent")} tooltip="Code" label="Code">
            <span className="flex h-full w-full items-center justify-center gap-2"><span className="font-mono text-[12px] font-semibold">&lt;/&gt;</span><span>Code</span></span>
          </ModeButton>
          <ModeButton active={mode === "teammates"} onClick={() => setMode("teammates")} tooltip="Teammates" label="Teammates">
            <span className="flex h-full w-full items-center justify-center gap-2">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M18 20a6 6 0 0 0-12 0" />
                <circle cx="12" cy="8" r="4" />
              </svg>
              <span>Teammates</span>
            </span>
          </ModeButton>
        </div>

        <div className="no-drag pointer-events-auto ml-auto flex items-center gap-3">
          <div className="luma-connection-status flex items-center gap-2 text-[11px] text-[var(--luma-text-muted)]" role="status" aria-live="polite">
            <span
              className={`h-1.5 w-1.5 rounded-full ${
                connected
                  ? "bg-[var(--luma-success)]"
                  : "bg-[var(--luma-danger)]"
              }`}
              aria-hidden="true"
            />
            {connected ? "Core connected" : "Core offline"}
          </div>

          <button
            type="button"
            onClick={onSettings}
            aria-label="Settings"
            className="
              flex h-8 w-8 items-center justify-center
              rounded-md
              text-[var(--luma-text-muted)]
              transition-colors
              hover:bg-[var(--luma-surface-hover)]
              hover:text-[var(--luma-text-secondary)]
            "
          >
            <SettingsIcon />
          </button>
        </div>
      </div>
    </header>
  );
}

function ModeButton({
  active,
  onClick,
  tooltip,
  label,
  children,
}: {
  active: boolean;
  onClick: () => void;
  tooltip: string;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={tooltip}
      aria-label={label}
      aria-pressed={active}
      className={`
        relative z-10 flex flex-1 h-7 items-center justify-center gap-1.5 rounded-md
        text-[13px] font-medium
        transition-colors
        ${
          active
            ? `
              text-[var(--luma-text)]
            `
            : `
              text-[var(--luma-text-muted)]
              hover:text-[var(--luma-text-secondary)]
            `
        }
      `}
    >
      {children}
    </button>
  );
}

function SettingsPanel({
  open,
  onClose,
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
}: {
  open: boolean;
  onClose: () => void;
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
}) {
  const { theme, setTheme, definition } = useTheme();
  const trapRef = useFocusTrap(open);

  const [section, setSection] = useState<"appearance" | "general" | "behavior" | "extensions">(
    "appearance",
  );

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
  actionLabel,
  onAction,
  children,
}: {
  title: string;
  actionLabel?: string;
  onAction?: () => void;
  children: React.ReactNode;
}) {
  return (
    <section className="mb-3">
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

function Sidebar({ mode, onOpenChat, onOpenIDE, onOpenChanges, onProjectFolderChange, teammatesSidebar, sidebarBlur }: { mode: AppMode; onOpenChat: () => void; onOpenIDE: () => void; onOpenChanges: () => void; onProjectFolderChange: (projectId: string, path: string) => Promise<void>; teammatesSidebar: React.ReactNode; sidebarBlur: boolean }) {
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
      {mode !== "teammates" && <nav className="luma-workspace-picker no-drag relative mx-4 mb-6 grid grid-cols-3 items-center rounded-lg border border-[var(--luma-border-strong)] bg-[var(--luma-bg)] p-[3px]" aria-label="Workspace">
        <span
          aria-hidden="true"
          className="pointer-events-none absolute bottom-[3px] left-[3px] top-[3px] w-[calc((100%_-_6px)_/_3)] rounded-md bg-[var(--luma-surface-raised)] shadow-[inset_0_0_0_1px_var(--luma-border)] transition-transform duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none"
          style={{ transform: `translateX(${mode === "ide" ? 100 : mode === "changes" ? 200 : 0}%)` }}
        />
        <WorkspacePickerButton active={mode === "agent"} onClick={onOpenChat}><span className="font-mono text-[12px]">&lt;/&gt;</span>Code</WorkspacePickerButton>
        <WorkspacePickerButton active={mode === "ide"} onClick={onOpenIDE}><FileIcon />Files</WorkspacePickerButton>
        <WorkspacePickerButton active={mode === "changes"} onClick={onOpenChanges}><span className="text-[13px]">◷</span>Changes</WorkspacePickerButton>
      </nav>}

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
          New chat
          <span className="ml-auto text-[11px] font-normal text-[var(--luma-text-muted)]">{window.luma.platform === "darwin" ? "⌘ N" : "Ctrl N"}</span>
        </button>
      </div>}

      {mode === "teammates" ? (
        <div className="min-h-0 flex-1 overflow-auto px-2.5 py-3">{teammatesSidebar}</div>
      ) : <>
      <div className="mt-2 flex h-8 items-center px-2.5">
        <span className="luma-kicker">Menu</span>

        <button
          type="button"
          className="
            ml-auto flex h-6 w-6 items-center justify-center
            rounded-md
            text-[var(--luma-text-muted)]
            hover:bg-[var(--luma-surface-hover)]
            hover:text-[var(--luma-text-secondary)]
          "
          aria-label="Search chats"
          aria-pressed={searchOpen}
          onClick={() => setSearchOpen((value) => !value)}
        >
          <SearchIcon />
        </button>
      </div>

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

      <div className="min-h-0 flex-1 overflow-auto px-1.5 pb-2">
        <SidebarSection
          title="Projects"
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
                      className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-text-secondary)] focus:text-[var(--luma-text-secondary)]"
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteProject(project.id, project.name)}
                      aria-label={`Delete project ${project.name}`}
                      className="px-1 text-[11px] text-[var(--luma-text-muted)] hover:text-[var(--luma-danger)] focus:text-[var(--luma-danger)]"
                    >
                      Delete
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

        {pinnedChats.length > 0 && (
          <SidebarSection title="Pinned">
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

        <SidebarSection title="History">
          {historyChats.length === 0 ? (
            <div className="px-2 py-1 text-[12px] text-[var(--luma-text-muted)]">
              {query.trim()
                ? "No matching chats."
                : "Start a chat to see it here."}
            </div>
          ) : (
            historyChats.map((chat) => (
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
      </div>
      </>}
    </aside>
  );
}

function WorkspacePickerButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      aria-pressed={active}
      className={`relative z-10 flex h-7 min-w-0 items-center justify-center gap-1.5 rounded-md px-1 text-[11px] font-medium transition-colors ${active ? "text-[var(--luma-text)]" : "text-[var(--luma-text-muted)] hover:text-[var(--luma-text-secondary)]"}`}
    >
      {children}
    </button>
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
}) {
  const [selectedCommand, setSelectedCommand] = useState(0);
  const [attachments, setAttachments] = useState<ChatAttachment[]>([]);
  const [attachmentError, setAttachmentError] = useState("");
  const [attaching, setAttaching] = useState(false);
  const matches = useMemo(() => {
    if (!value.startsWith("/")) return [];
    const query = value.toLowerCase();
    return SLASH_COMMANDS.filter((item) => item.command.startsWith(query));
  }, [value]);

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
      <div
        className="luma-composer
          mx-auto flex max-w-[860px]
          items-end gap-2
          rounded-xl
          border border-[var(--luma-border-strong)]
          bg-[var(--luma-surface-raised)]
          px-3.5 py-3
          shadow-sm transition-colors
          focus-within:border-[var(--luma-accent)]
        "
      >
        <button type="button" onClick={() => void attachFiles()} disabled={thinking || preparing || attaching} aria-label="Attach files" title="Attach text files" className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)] disabled:opacity-40"><PlusIcon /></button>
        <textarea
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={(event) => {
            if (matches.length > 0 && event.key === "ArrowDown") {
              event.preventDefault();
              setSelectedCommand((current) => (current + 1) % matches.length);
              return;
            }
            if (matches.length > 0 && event.key === "ArrowUp") {
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
          rows={1}
          placeholder={preparing ? "Preparing a new conversation…" : thinking ? "Luma is working..." : "Ask Luma anything..."}
          disabled={thinking || preparing}
          className="
            min-h-[28px] max-h-32 flex-1
            resize-none
            bg-transparent
            py-1
            text-[13px]
            leading-5
            text-[var(--luma-text)]
            outline-none
            placeholder:text-[var(--luma-text-muted)]
            disabled:opacity-50
          "
        />

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

      <div className="mx-auto mt-2 flex max-w-[860px] items-center justify-between px-1 text-[11px] text-[var(--luma-text-muted)]">
        <span>{sendWithEnter ? "Enter to send · Shift+Enter for a new line" : "Enter for a new line · Use Send to submit"}</span>
        <span className="hidden sm:inline">Luma can make mistakes. Review important changes.</span>
      </div>
    </div>
  );
}

function AgentView({ onChangeModel, sendWithEnter, autoScrollToLatest, showToolActivity }: { onChangeModel: (scope: "global" | "session") => void; sendWithEnter: boolean; autoScrollToLatest: boolean; showToolActivity: boolean }) {
  const [draft, setDraft] = useState("");
  const [detailsExpanded, setDetailsExpanded] = useState(false);
  const [toolGroupExpansion, setToolGroupExpansion] = useState<Record<string, boolean>>({});
  const transcriptScrollRef = useRef<HTMLDivElement>(null);
  const {
    sessionId,
    messages,
    tools,
    thinking,
    error,
    confirmation,
    sendPrompt,
    runCommand,
    respondToConfirmation,
    cancel,
  } = useLuma();

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
    confirmation !== null;

  // A pending chat is still an empty conversation: show the welcome state
  // until the first prompt is sent instead of leaving a blank transcript.
  const showEmptyState = preparingNewChat || !hasContent;

  return (
    <div className="luma-agent-view relative flex h-full flex-col">
      <div ref={transcriptScrollRef} className="min-h-0 flex-1 overflow-auto">
        {showEmptyState && (
          <>
            <EmptyAgentState onPrompt={setDraft} />
          </>
        )}

        {!showEmptyState && (
          <div className="luma-agent-transcript mx-auto max-w-[900px] space-y-4 px-6 py-5">
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
  const { appearance } = useTheme();
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

  const monacoTheme = appearance === "light" ? "vs" : "vs-dark";

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

            fontFamily: "SFMono-Regular, Menlo, Monaco, Consolas, monospace",

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

function TeammatesPanel({ onSidebarChange, sendWithEnter }: { onSidebarChange: (sidebar: React.ReactNode) => void; sendWithEnter: boolean }) {
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
  const [sendingMessage, setSendingMessage] = useState(false);
  const [modelPickerOpen, setModelPickerOpen] = useState(false);
  const [showTeammateList, setShowTeammateList] = useState(true);
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
  const [chatPrompt, setChatPrompt] = useState("");
  const [chatAttachments, setChatAttachments] = useState<ChatAttachment[]>([]);
  const [activeTab, setActiveTab] = useState<"chat" | "settings">("chat");
  const [memoryContent, setMemoryContent] = useState("");
  const [schedulePrompt, setSchedulePrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshing = useRef(false);

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
    void window.luma.getMemory(memoryScope, scopeId).then((entries) => {
      if (!disposed) setMemoryEntries(entries);
    }).catch((reason) => {
      if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { disposed = true; };
  }, [memoryScope, selectedAgentId, workspacePath, memoryRevision]);

  const selectedAgent = agents.find((agent) => agent.id === selectedAgentId) ?? null;
  const agentTasks = tasks.filter((task) => task.agent_id === selectedAgentId && !task.schedule_id);
  const visibleAgentTasks = selectedTaskId
    ? selectedTaskId === "__new__" ? [] : agentTasks.filter((task) => task.id === selectedTaskId)
    : agentTasks;
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

  useEffect(() => {
    onSidebarChange(
      <div key={`${showTeammateList ? "list" : "history"}:${selectedAgentId || "teammates"}`} className="luma-teammate-sidebar-transition">
        {selectedAgent && !showTeammateList ? <>
          <button type="button" onClick={() => { setShowTeammateList(true); setSelectedTaskId(null); }} className="mb-3 flex h-7 w-full items-center gap-2 rounded-md px-1 text-left text-[11px] text-[var(--luma-text-muted)] transition-colors hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)]">
            <span aria-hidden="true">←</span> All teammates
          </button>
          <div className="mb-2 flex items-center gap-2 px-1">
            <MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={24} active={selectedAgent.status === "active"} />
            <span className="min-w-0 flex-1 truncate text-[12px] font-semibold">{selectedAgent.name}</span>
          </div>
          <div className="mb-2 flex items-center justify-between px-1">
            <span className="luma-kicker">History</span>
            <div className="flex items-center gap-2"><span className="text-[10px] tabular-nums text-[var(--luma-text-muted)]">{agentTasks.length}</span><button type="button" title="New chat" aria-label={`New chat with ${selectedAgent.name}`} onClick={() => setSelectedTaskId("__new__")} className="luma-btn-ghost inline-flex h-6 w-6 items-center justify-center" disabled={busy}><PlusIcon /></button></div>
          </div>
          {agentTasks.length === 0 ? <p className="px-1 py-3 text-[11px] text-[var(--luma-text-muted)]">Conversations with this teammate will appear here.</p> : <div className="space-y-0.5">
            {agentTasks.slice().sort((left, right) => right.created_at - left.created_at).map((task) => (
              <div key={task.id} className={`luma-teammate-history-item group flex items-center gap-1 rounded-md px-2 py-1.5 ${selectedTaskId === task.id ? "is-selected" : ""}`}>
                <button type="button" onClick={() => setSelectedTaskId(task.id)} aria-current={selectedTaskId === task.id ? "page" : undefined} className="min-w-0 flex-1 text-left"><span className="block truncate text-[11px] font-medium">{task.title || task.request}</span><span className="block truncate text-[9px] capitalize text-[var(--luma-text-muted)]">{task.status.replace(/_/g, " ")} · {new Date(task.created_at * 1000).toLocaleDateString()}</span></button>
                <button type="button" title="Rename chat" aria-label={`Rename ${task.title}`} onClick={() => { const title = window.prompt("Rename chat", task.title); if (title?.trim()) void perform(() => window.luma.renameTask(task.id, title.trim())); }} className="luma-history-action" disabled={busy}><PencilIcon /></button>
                <button type="button" title="Delete chat" aria-label={`Delete ${task.title}`} onClick={() => { if (window.confirm(`Delete “${task.title}”?`)) void perform(async () => { await window.luma.deleteTask(task.id); if (selectedTaskId === task.id) setSelectedTaskId("__new__"); }); }} className="luma-history-action is-danger" disabled={busy || ["queued", "running", "waiting_for_approval", "paused"].includes(task.status)}><TrashIcon /></button>
              </div>
            ))}
          </div>}
        </> : <>
          <div className="mb-2 flex items-center justify-between px-1">
            <span className="luma-kicker">Your teammates</span>
            <button type="button" disabled={busy} onClick={() => setCreateOpen(true)} className="luma-btn-ghost inline-flex h-6 items-center gap-1 px-2 text-[10px]" aria-label="Create teammate">
              <PlusIcon /> Create
            </button>
          </div>
          {agents.length === 0 && <p className="px-1 py-3 text-[11px] text-[var(--luma-text-muted)]">No teammates yet. Create one to give it a persistent identity and workspace.</p>}
          <div className="space-y-1">
            {agents.map((agent) => (
              <button key={agent.id} type="button" onClick={() => { setSelectedTaskId(null); setSelectedAgentId(agent.id); setShowTeammateList(false); }} className="luma-teammate-nav-item w-full rounded-md border px-2.5 py-2 text-left transition-colors">
                <div className="flex items-center gap-2">
                  <MascotAvatar seed={agent.name} mascot={agent.configuration.mascot} size={30} active={agent.status === "active"} />
                  <span className={`h-1.5 w-1.5 rounded-full ${agent.status === "active" ? "bg-[var(--luma-success)]" : agent.status === "waiting" ? "bg-[var(--luma-warning)]" : "bg-[var(--luma-text-muted)]"}`} />
                  <span className="min-w-0 flex-1 truncate text-[12px] font-medium">{agent.name}</span>
                </div>
                <div className="mt-1 pl-3.5 text-[10px] capitalize text-[var(--luma-text-muted)]">{agent.id === "agent_global" ? "Global defaults · protected" : `${agent.status} · ${agent.kind}`}</div>
              </button>
            ))}
          </div>
        </>}
      </div>,
    );
    return () => onSidebarChange(null);
  }, [agents, busy, onSidebarChange, selectedAgentId, selectedTaskId, showTeammateList, tasks]);

  useEffect(() => {
    if (!selectedAgent) return;
    setPersonalityDraft(selectedAgent.configuration.system_prompt);
    setSkillsDraft(selectedAgent.configuration.skills.join(", "));
    setMascotDraft(selectedAgent.configuration.mascot || "auto");
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
            {selectedAgent && <div className="luma-teammate-header flex shrink-0 items-center gap-2 px-4 py-2"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={38} active={selectedAgent.status === "active"} activity={sendingMessage ? "sending" : "idle"} /><div><div className="text-[12px] font-semibold">{selectedAgent.name}</div><div className="text-[10px] capitalize text-[var(--luma-text-muted)]">{selectedAgent.status} · {agentTasks.length} conversations</div></div><div className="ml-auto flex items-center gap-1 rounded-md p-0.5"><button type="button" onClick={() => setActiveTab("chat")} aria-pressed={activeTab === "chat"} className={`h-7 rounded px-2.5 text-[11px] transition-colors ${activeTab === "chat" ? "bg-[var(--luma-surface-raised)] text-[var(--luma-text)]" : "text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]"}`}>Chat</button><button type="button" onClick={() => setActiveTab("settings")} aria-pressed={activeTab === "settings"} className={`h-7 rounded px-2.5 text-[11px] transition-colors ${activeTab === "settings" ? "bg-[var(--luma-surface-raised)] text-[var(--luma-text)]" : "text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]"}`}>Settings</button></div></div>}
          <div className="min-h-0 flex-1 overflow-auto p-4">
            {!selectedAgent ? <div className="flex h-full items-center justify-center text-[12px] text-[var(--luma-text-muted)]">Select a teammate to start chatting.</div> : activeTab === "chat" ? (
              <div className="mx-auto flex h-full w-full max-w-[850px] flex-col">
                <div key={selectedAgent.id} className="luma-teammate-history-transition min-h-0 flex-1 space-y-5 overflow-auto pb-5">
                  {visibleAgentTasks.length === 0 ? <div className="flex h-full flex-col items-center justify-center text-center"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={96} active /><h3 className="mt-4 text-[16px] font-semibold">{selectedTaskId === "__new__" ? `New chat with ${selectedAgent.name}` : `Chat with ${selectedAgent.name}`}</h3><p className="mt-1 max-w-sm text-[12px] leading-6 text-[var(--luma-text-muted)]">Start a conversation. This chat has its own history.</p></div> : visibleAgentTasks.slice().reverse().map((task) => {
                    const timeline = taskTimeline(task.id);
                    const hasAnswer = timeline.some((segment) => segment.kind === "text");
                    const lastSegment = timeline[timeline.length - 1];
                    const activity = task.status === "queued" ? "sending" : task.status === "running" ? lastSegment?.kind === "tools" && lastSegment.tools.some((tool) => tool.durationMs === undefined) ? "writing" : "thinking" : "idle";
                    return <article key={task.id} className="space-y-3">
                      <div className="flex justify-end"><div className="luma-teammate-user-message max-w-[88%] px-3 py-2"><div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">You</div><p className="whitespace-pre-wrap break-words text-[12px] leading-6">{task.request}</p></div></div>
                      <div className="luma-teammate-assistant-message flex items-start gap-3"><MascotAvatar seed={selectedAgent.name} mascot={selectedAgent.configuration.mascot} size={42} active={task.status === "running"} activity={activity} /><div className="min-w-0 flex-1 pt-0.5"><div className="mb-1 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">{selectedAgent.name}<span className="font-normal normal-case tracking-normal">{task.status === "running" ? "· working" : task.status === "queued" ? "· queued" : task.status === "waiting_for_approval" ? "· needs approval" : ""}</span></div><div className="luma-teammate-assistant-bubble space-y-2">
                        {timeline.map((segment) => segment.kind === "tools" ? (
                          <details key={segment.id} className="luma-task-tool-group rounded-md border border-[var(--luma-border)] px-2 py-1.5">
                            <summary className="cursor-pointer select-none text-[10px] text-[var(--luma-text-muted)]">{toolActivityLabel(segment.tools)}</summary>
                            <div className="mt-2 space-y-2">{segment.tools.map((tool) => <div key={tool.id} className="border-l-2 border-[var(--luma-border-strong)] pl-2"><div className="text-[10px] font-medium text-[var(--luma-text-secondary)]">{tool.name}<span className="ml-2 font-normal text-[var(--luma-text-muted)]">{tool.durationMs === undefined ? "running" : `${tool.durationMs} ms${tool.failed ? " · failed" : ""}`}</span></div>{tool.input && <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{tool.input}</pre>}{tool.diff && <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{tool.diff}</pre>}</div>)}</div>
                          </details>
                        ) : <div key={segment.id} className="prose prose-sm max-w-none text-[12px] leading-6"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{ pre({ children }) { return <pre className="my-2 overflow-x-auto rounded-md border border-[var(--luma-border)] bg-[var(--luma-bg)] p-3">{children}</pre>; }, p({ children }) { return <p className="mb-2 last:mb-0">{children}</p>; }, ul({ children }) { return <ul className="mb-2 ml-4 list-disc">{children}</ul>; }, ol({ children }) { return <ol className="mb-2 ml-4 list-decimal">{children}</ol>; }, li({ children }) { return <li className="mb-1">{children}</li>; }, h1({ children }) { return <h1 className="mb-2 mt-3 text-[14px] font-semibold">{children}</h1>; }, h2({ children }) { return <h2 className="mb-2 mt-3 text-[13px] font-semibold">{children}</h2>; }, h3({ children }) { return <h3 className="mb-2 mt-3 text-[12px] font-semibold">{children}</h3>; } }}>{segment.content}</ReactMarkdown></div>)}
                        {!hasAnswer && timeline.length === 0 && <div className="text-[11px] text-[var(--luma-text-muted)]">{task.status === "completed" ? "No text response was recorded for this task." : task.error ?? (task.status === "failed" ? "Task failed." : task.status === "cancelled" ? "Cancelled." : "Thinking…")}</div>}
                        {task.status === "failed" || task.status === "interrupted" ? <button type="button" disabled={busy} onClick={() => void perform(() => window.luma.retryTask(task.id))} className="mt-2 text-[10px] text-[var(--luma-accent)] hover:underline">Retry</button> : null}
                      </div></div></div>
                    </article>;
                  })}
                </div>
                <form className="luma-teammate-composer shrink-0 p-2" onSubmit={(event) => {
                  event.preventDefault();
                  const text = chatPrompt.trim() || (chatAttachments.length ? "Please inspect the attached files." : "");
                  if (!text || busy || selectedAgent.status === "stopped" || selectedAgent.status === "paused") return;
                  setSendingMessage(true);
                  void perform(async () => {
                      const created = await window.luma.createTask({ agentId: selectedAgent.id, request: formatChatPrompt(text, chatAttachments), workspace: workspacePath || selectedAgent.configuration.workspace || undefined });
                    setSelectedTaskId(created.id);
                    setChatPrompt("");
                    setChatAttachments([]);
                  }).finally(() => setSendingMessage(false));
                }}>
                  {chatAttachments.length > 0 && <div className="mb-2 flex flex-wrap gap-1.5">{chatAttachments.map((attachment, index) => <span key={`${attachment.name}-${index}`} className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-[var(--luma-border)] px-2 py-1 text-[10px] text-[var(--luma-text-secondary)]"><span className="max-w-56 truncate">{attachment.name}</span><button type="button" aria-label={`Remove ${attachment.name}`} onClick={() => setChatAttachments((items) => items.filter((_, itemIndex) => itemIndex !== index))} className="text-[var(--luma-text-muted)] hover:text-[var(--luma-text)]">×</button></span>)}</div>}
                  <div className="flex items-center gap-2">
                    <button type="button" aria-label="Attach files" title="Attach text files" disabled={busy || selectedAgent.status === "stopped" || selectedAgent.status === "paused"} onClick={() => void attachChatFiles()} className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[var(--luma-text-secondary)] hover:bg-[var(--luma-surface-hover)] hover:text-[var(--luma-text)] disabled:opacity-40"><PlusIcon /></button>
                    <textarea value={chatPrompt} onChange={(event) => setChatPrompt(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && sendWithEnter) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} placeholder={`Message ${selectedAgent.name}…`} rows={1} className="luma-input min-h-[32px] min-w-0 flex-1 self-center resize-none border-0 bg-transparent py-1.5 text-[12px] leading-5 focus:ring-0" />
                  </div>
                  <div className="flex items-center justify-between px-1 pt-1"><span className="text-[10px] text-[var(--luma-text-muted)]">{sendWithEnter ? "Enter to send · Shift + Enter for a new line" : "Enter for a new line · Use Send to submit"}</span><div className="flex items-center gap-2"><button type="button" onClick={() => setModelPickerOpen(true)} className="luma-model-inline" title={`Change model for ${selectedAgent.name}`}>{selectedAgent.configuration.model ?? "Model"}<span aria-hidden="true">⌄</span></button><button type="submit" disabled={busy || (!chatPrompt.trim() && chatAttachments.length === 0) || selectedAgent.status === "stopped" || selectedAgent.status === "paused"} className="luma-btn-primary h-8 px-3 text-[11px]">{busy ? "Sending…" : "Send"}</button></div></div>
                </form>
              </div>
            ) : (
              <div className="space-y-5">
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

                <section className="rounded-lg border border-[var(--luma-border)] p-3">
                  <div className="mb-2 luma-kicker">Identity & personality</div>
                  <MascotCustomizer value={mascotDraft} seed={selectedAgent.name} onChange={setMascotDraft} />
                  <textarea value={personalityDraft} onChange={(event) => setPersonalityDraft(event.target.value)} rows={4} className="luma-input w-full resize-y text-[11px]" placeholder="How should this teammate behave?" />
                  {selectedAgent.id !== "agent_global" && <label className="mt-3 flex items-start gap-2 text-[11px] text-[var(--luma-text-secondary)]"><input type="checkbox" checked={selectedAgent.configuration.use_global_context} onChange={(event) => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, use_global_context: event.target.checked, ...(event.target.checked ? { skills: [] } : {}) } }); })} className="mt-0.5 accent-[var(--luma-accent)]" /><span>Use the global skills, memory, connectors, etc.</span></label>}
                  <label className="mt-3 flex items-start gap-2 text-[11px] text-[var(--luma-text-secondary)]"><input type="checkbox" checked={selectedAgent.configuration.network_access ?? true} onChange={(event) => { const enabled = event.target.checked; void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, network_access: enabled, permissions: { ...selectedAgent.configuration.permissions, allow_network: enabled } } }); }); }} className="mt-0.5 accent-[var(--luma-accent)]" /><span><strong>Internet access</strong><small className="mt-0.5 block text-[var(--luma-text-muted)]">Allow web search and fetching public webpages. Requests that can change external data still require approval.</small></span></label>
                  {!selectedAgent.configuration.use_global_context && <input value={skillsDraft} onChange={(event) => setSkillsDraft(event.target.value)} className="luma-input mt-2 h-8 w-full text-[11px]" placeholder="Skills: research, code-review" />}
                  <button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.updateAgent({ id: selectedAgent.id, configuration: { ...selectedAgent.configuration, mascot: mascotDraft, system_prompt: personalityDraft, skills: selectedAgent.configuration.use_global_context ? [] : [...new Set(skillsDraft.split(",").map((skill) => skill.trim()).filter(Boolean))] } }); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Save changes</button>
                </section>

                {approvals.filter((approval) => approval.agent_id === selectedAgent.id).length > 0 && <section>
                  <h4 className="mb-2 luma-kicker">Needs your approval</h4>
                  <div className="space-y-2">{approvals.filter((approval) => approval.agent_id === selectedAgent.id).map((approval) => <div key={approval.id} className="rounded-md border border-[var(--luma-warning)]/30 bg-[var(--luma-warning)]/5 p-3">
                    <div className="text-[11px] font-medium">{approval.tool}</div><pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-muted)]">{approval.arguments}</pre>
                    <div className="mt-2 flex justify-end gap-2"><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, false))} className="luma-btn-ghost h-7 px-2 text-[10px]">Deny</button><button type="button" disabled={busy} onClick={() => void perform(() => window.luma.resolveApproval(approval.id, true))} className="luma-btn-primary h-7 px-2.5 text-[10px]">Approve</button></div>
                  </div>)}</div>
                </section>}

                <section className="grid gap-3 md:grid-cols-2">
                  <div className="rounded-lg border border-[var(--luma-border)] p-3">
                    <h4 className="mb-2 luma-kicker">Agent memory</h4>
                    <select value={memoryScope} onChange={(event) => setMemoryScope(event.target.value as MemoryScope)} className="luma-input mb-2 h-8 w-full text-[11px]">
                      <option value="agent">This agent</option><option value="user">Shared user memory</option><option value="project">This workspace</option>
                    </select>
                    <textarea value={memoryContent} onChange={(event) => setMemoryContent(event.target.value)} rows={2} placeholder="A preference or fact this teammate should remember…" className="luma-input w-full resize-y text-[11px]" />
                    <button type="button" disabled={busy || !memoryContent.trim() || (memoryScope === "project" && !workspacePath)} onClick={() => void perform(async () => { const scopeId = memoryScope === "agent" ? selectedAgent.id : memoryScope === "user" ? "default" : workspacePath; await window.luma.addMemory(memoryScope, scopeId, memoryContent.trim()); setMemoryContent(""); setMemoryRevision((revision) => revision + 1); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Save memory</button>
                    <div className="mt-2 max-h-32 space-y-1 overflow-auto">{memoryEntries.map((entry) => <div key={entry.id} className="flex items-start gap-2 border-t border-[var(--luma-border)] py-1.5"><p className="min-w-0 flex-1 whitespace-pre-wrap break-words text-[10px] text-[var(--luma-text-secondary)]">{entry.content}</p><button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.deleteMemory(entry.id); setMemoryRevision((revision) => revision + 1); })} className="text-[10px] text-[var(--luma-danger)] hover:underline">Delete</button></div>)}</div>
                  </div>
                  <div className="rounded-lg border border-[var(--luma-border)] p-3">
                    <h4 className="mb-2 luma-kicker">Recurring work</h4>
                    <textarea value={schedulePrompt} onChange={(event) => setSchedulePrompt(event.target.value)} rows={2} placeholder="What should this teammate repeat?" className="luma-input w-full resize-y text-[11px]" />
                    <button type="button" disabled={busy || !schedulePrompt.trim()} onClick={() => void perform(async () => { await window.luma.createSchedule({ agentId: selectedAgent.id, title: "Recurring task", prompt: schedulePrompt.trim(), everySeconds: 86400 }); setSchedulePrompt(""); })} className="mt-2 text-[11px] text-[var(--luma-accent)] hover:underline">Schedule daily</button>
                    <p className="mt-1 text-[10px] text-[var(--luma-text-muted)]">{schedules.filter((schedule) => schedule.agent_id === selectedAgent.id && schedule.enabled).length} enabled schedules</p>
                    <div className="mt-2 space-y-1">{schedules.filter((schedule) => schedule.agent_id === selectedAgent.id).map((schedule) => <div key={schedule.id} className="flex items-center gap-2 border-t border-[var(--luma-border)] pt-1.5"><span className="min-w-0 flex-1 truncate text-[10px] text-[var(--luma-text-secondary)]">{schedule.title} · {schedule.enabled ? new Date(schedule.next_run_at * 1000).toLocaleString() : "Disabled"}</span><button type="button" disabled={busy} onClick={() => void perform(async () => { await window.luma.deleteSchedule(schedule.id); })} className="text-[10px] text-[var(--luma-danger)] hover:underline">Remove</button></div>)}</div>
                  </div>
                </section>
              </div>
            )}
          </div>
        </div>
        </div>
        {modelPickerOpen && selectedAgent && <ModelPickerDialog scope="agent" agent={selectedAgent} onClose={() => setModelPickerOpen(false)} onChanged={() => void refresh()} />}
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
            setSelectedAgentId(created.id); setSelectedTaskId(null); setShowTeammateList(false); setCreateOpen(false); setAgentName(""); setAgentDescription(""); setAgentPersonality(""); setAgentInitialMemory(""); setAgentWorkspace(""); setAgentMascot("auto"); setAgentUseGlobalContext(true); setActiveTab("chat");
            if (agentInitialMemory.trim()) {
              try { await window.luma.addMemory("agent", created.id, agentInitialMemory.trim()); }
              catch (reason) { setError(reason instanceof Error ? `Teammate created, but initial memory could not be saved: ${reason.message}` : "Teammate created, but initial memory could not be saved."); }
            }
          }); }}>
            <header className="luma-create-header flex items-start justify-between border-b border-[var(--luma-border)] px-6 py-5"><div><div className="luma-kicker mb-2">TEAMMATES / NEW</div><h3 id="create-teammate-title" className="text-[19px] font-semibold tracking-tight">Create a teammate</h3><p className="mt-1.5 text-[12px] text-[var(--luma-text-muted)]">Give a new agent a name, a personality and a place to work.</p></div><button type="button" aria-label="Close dialog" onClick={() => setCreateOpen(false)} className="luma-create-close" disabled={busy}>×</button></header>
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

export default function App() {
  const [mode, setMode] = useState<AppMode>("agent");
  const [sidebarVisible, setSidebarVisible] = useState(true);
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
  const [modelPickerScope, setModelPickerScope] = useState<"global" | "session" | null>(null);

  const [connected, setConnected] = useState(false);

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
    window.luma.getSetupConfig().then((config) => {
      if (config) {
        setSetupConfig(config);
        setSetupComplete(true);
      }
    });
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

    window.luma
      .getStatus()
      .then((status: { connected: boolean }) => {
        if (mounted) {
          setConnected(status.connected);
        }
      })
      .catch(() => {
        if (mounted) {
          setConnected(false);
        }
      });

    const unsubscribe = window.luma.onEvent((event: unknown) => {
      if (typeof event !== "object" || event === null) {
        return;
      }

      const message = event as {
        type?: string;
      };

      if (message.type === "ready") {
        setConnected(true);
      }

      if (message.type === "process_exited") {
        setConnected(false);
      }
    });

    const unsubscribeSettings = window.luma.onOpenSettings(() => {
      setSettingsOpen(true);
    });
    const unsubscribeNewChat = window.luma.onNewChat(() => {
      useWorkspaceStore.getState().startNewChat();
      setMode("agent");
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

    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
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
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [chooseWorkspaceForCurrentScope]);

  if (!setupComplete) {
    return (
      <SetupWizard
        onComplete={() => {
          void window.luma.getSetupConfig().then((config) => {
            setSetupConfig(config);
            setSetupComplete(true);
          });
        }}
      />
    );
  }

  return (
    <div
      className="
        luma-app-shell flex h-screen w-screen
        flex-col overflow-hidden
        bg-[var(--luma-bg)]
        text-[var(--luma-text)]
      "
    >
      {/* Live region for screen reader announcements */}
      <div id="luma-announcer" aria-live="polite" className="sr-only" />
      {tourOpen && <DesktopTour onFinish={finishTour} />}
      <TopBar
        mode={mode}
        setMode={setMode}
        connected={connected}
        sidebarBlur={sidebarBlur}
        sidebarVisible={sidebarVisible}
        onToggleSidebar={() => setSidebarVisible((visible) => !visible)}
        onSettings={() => setSettingsOpen((current) => !current)}
      />

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
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
      />
      {modelPickerScope && <ModelPickerDialog scope={modelPickerScope} onClose={() => setModelPickerScope(null)} onChanged={(config) => { if (modelPickerScope === "global") setSetupConfig(config); }} />}

      <div className="flex min-h-0 flex-1">
        <div
          className="luma-sidebar-slot"
          data-open={sidebarVisible}
          aria-hidden={!sidebarVisible}
          inert={!sidebarVisible || workspaceSwitching}
        >
          <Sidebar mode={mode} onOpenChat={() => setMode("agent")} onOpenIDE={() => setMode("ide")} onOpenChanges={() => setMode("changes")} onProjectFolderChange={handleProjectFolderChange} teammatesSidebar={teammatesSidebar} sidebarBlur={sidebarBlur} />
        </div>

        <main className="luma-main-surface relative min-w-0 flex-1 overflow-hidden bg-[var(--luma-bg)]" aria-busy={workspaceSwitching} inert={workspaceSwitching}>
          <div key={`${mode}:${workspacePath}`} className="luma-view-transition h-full min-w-0">
          {mode === "teammates" && <TeammatesPanel onSidebarChange={setTeammatesSidebar} sendWithEnter={sendWithEnter} />}
          {mode === "agent" && <AgentView onChangeModel={setModelPickerScope} sendWithEnter={sendWithEnter} autoScrollToLatest={autoScrollToLatest} showToolActivity={showToolActivity} />}

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
