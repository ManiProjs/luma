import { useEffect, useMemo, useRef, useState } from "react";

import Editor from "@monaco-editor/react";

import { SparkIcon } from "./components/LumaWordmark";
import LumaWordmark from "./components/LumaWordmark";

import { themes, useTheme } from "./renderer/Theme";

import { useLuma } from "./renderer/hooks/useLuma";

import SetupWizard from "./setup/SetupWizard";

import type { SetupConfig } from "./stores/setupStore";

import { useWorkspaceStore, type Chat } from "./stores/workspaceStore";

type AppMode = "agent" | "ide" | "changes";

type WorkspaceNode =
  | {
      type: "folder";
      name: string;
      children: WorkspaceNode[];
    }
  | {
      type: "file";
      name: string;
      path: string;
    };

const workspace: WorkspaceNode[] = [
  {
    type: "folder",
    name: "src",
    children: [
      {
        type: "folder",
        name: "agent",
        children: [
          {
            type: "file",
            name: "mod.rs",
            path: "src/agent/mod.rs",
          },
          {
            type: "file",
            name: "runner.rs",
            path: "src/agent/runner.rs",
          },
        ],
      },
      {
        type: "folder",
        name: "workspace",
        children: [
          {
            type: "file",
            name: "mod.rs",
            path: "src/workspace/mod.rs",
          },
          {
            type: "file",
            name: "bootstrap.rs",
            path: "src/workspace/bootstrap.rs",
          },
        ],
      },
      {
        type: "file",
        name: "main.rs",
        path: "src/main.rs",
      },
      {
        type: "file",
        name: "event.rs",
        path: "src/event.rs",
      },
      {
        type: "file",
        name: "desktop.rs",
        path: "src/desktop.rs",
      },
    ],
  },
  {
    type: "folder",
    name: "tests",
    children: [
      {
        type: "file",
        name: "agent.rs",
        path: "tests/agent.rs",
      },
    ],
  },
  {
    type: "file",
    name: "Cargo.toml",
    path: "Cargo.toml",
  },
  {
    type: "file",
    name: "GALAXY.md",
    path: "GALAXY.md",
  },
  {
    type: "file",
    name: "README.md",
    path: "README.md",
  },
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
}: {
  mode: AppMode;
  setMode: (mode: AppMode) => void;
  connected: boolean;
  onSettings: () => void;
}) {
  return (
    <header
      className="
        drag-region
        flex h-[44px] shrink-0 items-center
        border-b border-[var(--luma-border)]
        bg-[var(--luma-surface)]
      "
    >
      <div
        className="
          flex h-full w-full items-center
          pl-[82px] pr-3
          pointer-events-none
        "
      >
        <div className="no-drag pointer-events-auto">
          <LumaWordmark compact />
        </div>

        <div className="no-drag pointer-events-auto ml-6 flex h-full items-center gap-0.5">
          <ModeButton
            active={mode === "agent"}
            onClick={() => setMode("agent")}
          >
            Agent
          </ModeButton>

          <ModeButton active={mode === "ide"} onClick={() => setMode("ide")}>
            IDE
          </ModeButton>

          <ModeButton
            active={mode === "changes"}
            onClick={() => setMode("changes")}
          >
            Changes
          </ModeButton>
        </div>

        <div className="no-drag pointer-events-auto ml-auto flex items-center gap-2">
          <div
            className="
              flex items-center gap-1.5
              text-[12px]
              text-[var(--luma-text-secondary)]
            "
          >
            <span
              className={`h-1.5 w-1.5 rounded-full ${
                connected
                  ? "bg-[var(--luma-success)]"
                  : "bg-[var(--luma-danger)]"
              }`}
            />

            {connected ? "Connected" : "Disconnected"}
          </div>

          <button
            type="button"
            onClick={onSettings}
            aria-label="Settings"
            className="
              flex h-7 w-7 items-center justify-center
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
        h-7 rounded-md px-2.5
        text-[13px] font-medium
        transition-colors
        ${
          active
            ? `
              bg-[var(--luma-surface-hover)]
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
  onReconfigure,
}: {
  open: boolean;
  onClose: () => void;
  connected: boolean;
  setupConfig: SetupConfig | null;
  onReconfigure: () => void;
}) {
  const { theme, setTheme, definition } = useTheme();

  const [section, setSection] = useState<"appearance" | "general">(
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
        className="fixed inset-0 z-30 cursor-default bg-black/40"
      />

      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        className="
          no-drag
          fixed left-1/2 top-1/2 z-40
          flex h-[560px] w-[720px]
          -translate-x-1/2 -translate-y-1/2
          overflow-hidden
          rounded-xl
          border border-[var(--luma-border-strong)]
          bg-[var(--luma-surface-raised)]
          shadow-2xl
        "
      >
        <div
          className="
            w-[140px] shrink-0
            border-r border-[var(--luma-border)]
            bg-[var(--luma-surface)]
            p-2
          "
        >
          <div id="settings-title" className="luma-kicker mb-1 px-2 py-1">
            Settings
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
        </div>

        <div className="min-w-0 flex-1 p-4">
          {section === "appearance" && (
            <>
              <div className="mb-3">
                <h2 className="text-[13px] font-medium">Appearance</h2>

                <p className="mt-0.5 text-[12px] text-[var(--luma-text-secondary)]">
                  Customize the visual appearance of Luma.
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

              <div
                className="
                  mt-3 rounded-md
                  border border-[var(--luma-border)]
                  bg-[var(--luma-surface)]
                  p-3
                "
              >
                <div className="luma-kicker mb-2">Preview</div>

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
              <div className="mb-3">
                <h2 className="text-[13px] font-medium">General</h2>

                <p className="mt-0.5 text-[12px] text-[var(--luma-text-secondary)]">
                  Workspace, model, and connection status for this session.
                </p>
              </div>

              <SettingsRow
                title="Workspace"
                description="Files shown in the IDE panel"
                value="~/projs/luma"
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
                onAction={onReconfigure}
              />

              <SettingsRow
                title="Confirmations"
                description="Luma asks before running tools that change files"
                value="Required"
              />
            </>
          )}
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
    <div
      className="
        mb-2 flex items-center
        rounded-md
        border border-[var(--luma-border)]
        bg-[var(--luma-surface)]
        px-3 py-2.5
      "
    >
      <div className="min-w-0">
        <div className="text-[13px] font-medium">{title}</div>

        <div className="mt-0.5 text-[12px] text-[var(--luma-text-muted)]">
          {description}
        </div>
      </div>

      <div className="ml-auto flex shrink-0 items-center gap-2">
        <span className="text-[12px] text-[var(--luma-text-secondary)]">
          {value}
        </span>

        {actionLabel && onAction && (
          <button type="button" onClick={onAction} className="luma-btn-ghost h-8 px-2.5">
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
        border
        p-2.5
        transition-colors
        ${
          selected
            ? `
              border-[var(--luma-accent)]
              bg-[var(--luma-accent-soft)]
            `
            : `
              border-[var(--luma-border)]
              bg-[var(--luma-surface)]
              hover:bg-[var(--luma-surface-hover)]
            `
        }
      `}
    >
      <div className="mb-2 flex items-center gap-1.5">
        <span
          className="h-2 w-2 rounded-full"
          style={{
            backgroundColor: theme.accent,
          }}
        />

        <span className="text-[12px] font-medium">{theme.name}</span>
      </div>

      <div className="text-[12px] leading-4 text-[var(--luma-text-muted)]">
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
        group mb-0.5 flex items-center
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
              truncate text-[12px]
              ${active ? "text-[var(--luma-text)]" : "text-[var(--luma-text-secondary)]"}
            `}
          >
            {chat.title}
          </span>
        </div>

        <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-[var(--luma-text-muted)]">
          {projectName && <span className="truncate">{projectName}</span>}

          {projectName && <span className="opacity-40">·</span>}

          <span>{formatChatTime(chat.updatedAt)}</span>
        </div>
      </button>

      <div className="mr-1 hidden items-center group-hover:flex">
        <button
          type="button"
          onClick={onTogglePin}
          aria-label={chat.pinned ? "Unpin chat" : "Pin chat"}
          className="
            flex h-6 w-6 items-center justify-center
            rounded-md
            text-[var(--luma-text-muted)]
            hover:text-[var(--luma-text-secondary)]
          "
        >
          <PinIcon filled={chat.pinned} />
        </button>
        <button
          type="button"
          onClick={onRename}
          className="
            px-1 text-[11px]
            text-[var(--luma-text-muted)]
            hover:text-[var(--luma-text-secondary)]
          "
        >
          Rename
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="
            px-1 text-[11px]
            text-[var(--luma-text-muted)]
            hover:text-[var(--luma-danger)]
          "
        >
          Delete
        </button>
      </div>
    </div>
  );
}

function Sidebar({ onOpenChat }: { onOpenChat: () => void }) {
  const chats = useWorkspaceStore((state) => state.chats);
  const projects = useWorkspaceStore((state) => state.projects);
  const activeChatId = useWorkspaceStore((state) => state.activeChatId);
  const activeProjectId = useWorkspaceStore((state) => state.activeProjectId);
  const createChat = useWorkspaceStore((state) => state.createChat);
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

  return (
    <aside
      className="
        flex w-[240px] shrink-0 flex-col
        border-r border-[var(--luma-border)]
        bg-[var(--luma-surface)]
      "
    >
      <div className="px-2.5 pt-2.5">
        <button
          type="button"
          onClick={() => {
            createChat();
            onOpenChat();
          }}
          className="
            luma-btn-primary
            flex h-8 w-full items-center justify-center gap-1.5
            text-[12px]
          "
        >
          <PlusIcon />
          New chat
        </button>
      </div>

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
                    {project.name}
                  </button>

                  <div className="mr-1 hidden group-hover:flex">
                    <button
                      type="button"
                      onClick={() => handleRenameProject(project.id, project.name)}
                      className="
                        px-1 text-[11px]
                        text-[var(--luma-text-muted)]
                        hover:text-[var(--luma-text-secondary)]
                      "
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteProject(project.id, project.name)}
                      className="
                        px-1 text-[11px]
                        text-[var(--luma-text-muted)]
                        hover:text-[var(--luma-danger)]
                      "
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

function GalaxyField() {
  return (
    <div className="pointer-events-none absolute inset-0 overflow-hidden opacity-40">
      <div
        className="
          absolute left-[15%] top-[20%]
          h-px w-[35%]
          bg-gradient-to-r
          from-transparent
          via-[var(--luma-border-strong)]
          to-transparent
        "
      />

      <div
        className="
          absolute right-[10%] top-[48%]
          h-px w-[28%]
          bg-gradient-to-r
          from-transparent
          via-[var(--luma-border)]
          to-transparent
        "
      />

      <div
        className="
          absolute left-[40%] bottom-[24%]
          h-px w-[25%]
          bg-gradient-to-r
          from-transparent
          via-[var(--luma-border)]
          to-transparent
        "
      />
    </div>
  );
}

function EmptyAgentState() {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
      <div className="mb-20 text-center">
        <div
          className="
            mx-auto mb-3
            flex h-9 w-9 items-center justify-center
            rounded-lg
            bg-[var(--luma-accent-soft)]
            text-[var(--luma-accent)]
          "
        >
          <SparkIcon />
        </div>

        <h1 className="text-[15px] font-medium tracking-tight">
          What are we building?
        </h1>

        <p className="mt-1 text-[12px] text-[var(--luma-text-muted)]">
          Ask Luma to inspect, change, or explain your code.
        </p>
      </div>
    </div>
  );
}

function AgentMessageView({
  role,
  content,
}: {
  role: "user" | "assistant";
  content: string;
}) {
  return (
    <div
      className={`
        flex gap-3
        ${role === "user" ? "justify-end" : "justify-start"}
      `}
    >
      {role === "assistant" && (
        <div
          className="
            mt-0.5 flex h-5 w-5 shrink-0
            items-center justify-center
            rounded-md
            bg-[var(--luma-accent-soft)]
            text-[var(--luma-accent)]
          "
        >
          <SparkIcon />
        </div>
      )}

      <div
        className={`
          max-w-[760px]
          whitespace-pre-wrap
          text-[12px]
          leading-5
          ${
            role === "user"
              ? `
                rounded-lg
                bg-[var(--luma-surface-raised)]
                px-3 py-2
                text-[var(--luma-text)]
              `
              : "text-[var(--luma-text-secondary)]"
          }
        `}
      >
        {content}
      </div>
    </div>
  );
}

function ToolActivity({
  tools,
}: {
  tools: ReturnType<typeof useLuma>["tools"];
}) {
  if (tools.length === 0) {
    return null;
  }

  return (
    <div className="mt-2 space-y-1">
      {tools.map((tool) => (
        <div
          key={tool.id}
          className="
            flex items-center gap-2
            rounded-md
            border border-[var(--luma-border)]
            bg-[var(--luma-surface)]
            px-2.5 py-1.5
          "
        >
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
      ))}
    </div>
  );
}

function ThinkingIndicator() {
  return (
    <div className="flex items-center gap-2">
      <div
        className="
          flex h-5 w-5 shrink-0
          items-center justify-center
          rounded-md
          bg-[var(--luma-accent-soft)]
          text-[var(--luma-accent)]
        "
      >
        <SparkIcon />
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
  onSend,
  onCancel,
}: {
  thinking: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState("");

  function submit() {
    const text = value.trim();

    if (!text) {
      return;
    }

    onSend(text);
    setValue("");
  }

  return (
    <div className="border-t border-[var(--luma-border)] p-3">
      <div
        className="
          mx-auto flex max-w-[820px]
          items-end gap-2
          rounded-lg
          border border-[var(--luma-border-strong)]
          bg-[var(--luma-surface-raised)]
          px-3 py-2
          shadow-lg
        "
      >
        <textarea
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          rows={1}
          placeholder={thinking ? "Luma is working..." : "Ask Luma anything..."}
          disabled={thinking}
          className="
            min-h-[26px] max-h-28 flex-1
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
              flex h-7 w-7 shrink-0
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
            disabled={!value.trim()}
            className="
              flex h-7 w-7 shrink-0
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

      <div className="mt-1.5 text-center text-[12px] text-[var(--luma-text-muted)]">
        Enter to send · Shift+Enter for a new line
      </div>
    </div>
  );
}

function AgentView() {
  const {
    messages,
    tools,
    thinking,
    error,
    confirmation,
    sendPrompt,
    respondToConfirmation,
    cancel,
    resetSession,
  } = useLuma();

  const chats = useWorkspaceStore((state) => state.chats);
  const activeChatId = useWorkspaceStore((state) => state.activeChatId);
  const setChatMessages = useWorkspaceStore((state) => state.setChatMessages);

  const loadedChatId = useRef<string | null>(null);
  const skipPersist = useRef(false);

  useEffect(() => {
    if (!activeChatId) {
      return;
    }

    if (loadedChatId.current === activeChatId) {
      return;
    }

    const chat = chats.find((item) => item.id === activeChatId);

    if (thinking || confirmation !== null) {
      void cancel();
    }

    skipPersist.current = true;
    resetSession(chat?.messages ?? []);
    loadedChatId.current = activeChatId;
  }, [activeChatId, cancel, chats, confirmation, resetSession, thinking]);

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

  return (
    <div className="relative flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-auto">
        {!hasContent && (
          <>
            <GalaxyField />
            <EmptyAgentState />
          </>
        )}

        {hasContent && (
          <div className="mx-auto max-w-[900px] space-y-4 px-6 py-5">
            {messages.map((message) => (
              <AgentMessageView
                key={message.id}
                role={message.role}
                content={message.content}
              />
            ))}

            <ToolActivity tools={tools} />

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

      <Composer thinking={thinking} onSend={sendPrompt} onCancel={cancel} />
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
}: {
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
}) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [query, setQuery] = useState("");

  const visible = filterWorkspace(workspace, query);

  return (
    <aside
      className="
        flex w-[220px] shrink-0 flex-col
        border-r border-[var(--luma-border)]
        bg-[var(--luma-surface)]
      "
    >
      <div className="flex h-9 items-center px-2.5">
        <span className="luma-kicker">Files</span>

        <div className="ml-auto flex items-center">
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
        <span className="truncate">~/projs/luma</span>
      </div>
    </aside>
  );
}

function IDEView({
  selectedFile,
  onFileSelect,
}: {
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
}) {
  const { theme } = useTheme();

  const fileName = selectedFile
    ? (selectedFile.split("/").pop() ?? selectedFile)
    : "untitled";

  const language = getLanguage(selectedFile);

  const monacoTheme = theme === "solarized" ? "vs-dark" : "vs-dark";

  return (
    <div className="flex h-full min-w-0">
      <FilesPanel selectedFile={selectedFile} onFileSelect={onFileSelect} />

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
          {language}
        </div>
      </div>

      <div className="min-h-0 flex-1">
        <Editor
          height="100%"
          language={language}
          theme={monacoTheme}
          defaultValue={
            selectedFile
              ? `// ${selectedFile}\n\n`
              : "// Select a file from the workspace\n"
          }
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
        />
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

        <span className="ml-auto">Luma Editor</span>
      </div>
      </div>
    </div>
  );
}

function ChangesView() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <div className="text-[12px] font-medium">No changes</div>

        <div className="mt-1 text-[12px] text-[var(--luma-text-muted)]">
          Changes and diffs will appear here.
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const [mode, setMode] = useState<AppMode>("agent");

  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  const [settingsOpen, setSettingsOpen] = useState(false);

  const [connected, setConnected] = useState(false);

  const [setupComplete, setSetupComplete] = useState(false);

  const [setupConfig, setSetupConfig] = useState<SetupConfig | null>(null);

  useEffect(() => {
    window.luma.getSetupConfig().then((config) => {
      if (config) {
        setSetupConfig(config);
        setSetupComplete(true);
      }
    });
  }, []);

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

      if (message.type === "Ready") {
        setConnected(true);
      }

      if (message.type === "ProcessExited") {
        setConnected(false);
      }
    });

    const unsubscribeSettings = window.luma.onOpenSettings(() => {
      setSettingsOpen(true);
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
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

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
        flex h-screen w-screen
        flex-col overflow-hidden
        bg-[var(--luma-bg)]
        text-[var(--luma-text)]
      "
    >
      <TopBar
        mode={mode}
        setMode={setMode}
        connected={connected}
        onSettings={() => setSettingsOpen((current) => !current)}
      />

      <SettingsPanel
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        connected={connected}
        setupConfig={setupConfig}
        onReconfigure={() => {
          setSettingsOpen(false);
          setSetupComplete(false);
        }}
      />

      <div className="flex min-h-0 flex-1">
        <Sidebar onOpenChat={() => setMode("agent")} />

        <main className="min-w-0 flex-1 overflow-hidden">
          {mode === "agent" && <AgentView />}

          {mode === "ide" && (
            <IDEView
              selectedFile={selectedFile}
              onFileSelect={setSelectedFile}
            />
          )}

          {mode === "changes" && <ChangesView />}
        </main>
      </div>
    </div>
  );
}
