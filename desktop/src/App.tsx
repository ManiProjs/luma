import { useEffect, useMemo, useState } from "react";

import Editor from "@monaco-editor/react";

import { themes, useTheme, type ThemeId } from "./renderer/Theme";

import { useLuma } from "./renderer/hooks/useLuma";

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

function SparkIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 2l1.7 6.3L20 10l-6.3 1.7L12 18l-1.7-6.3L4 10l6.3-1.7L12 2z" />
      <path d="M19 16l.7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7L19 16z" />
    </svg>
  );
}

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
      strokeWidth="1.7"
      strokeLinecap="round"
    >
      <path d="M12 5v14M5 12h14" />
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
          no-drag
          flex h-full w-full items-center
          pl-[82px] pr-3
        "
      >
        <div className="flex items-center gap-2">
          <div
            className="
              flex h-6 w-6 items-center justify-center
              rounded-md
              bg-[var(--luma-accent-soft)]
              text-[var(--luma-accent)]
            "
          >
            <SparkIcon />
          </div>

          <span className="text-[12px] font-medium tracking-tight">Luma</span>
        </div>

        <div className="ml-6 flex h-full items-center gap-0.5">
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

        <div className="ml-auto flex items-center gap-2">
          <div
            className="
              flex items-center gap-1.5
              text-[10px]
              text-[var(--luma-text-muted)]
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
        text-[11px] font-medium
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
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { theme, setTheme, definition } = useTheme();

  const [section, setSection] = useState<"appearance" | "general">(
    "appearance",
  );

  if (!open) {
    return null;
  }

  return (
    <>
      <button
        type="button"
        aria-label="Close settings"
        onClick={onClose}
        className="fixed inset-0 z-30 cursor-default"
      />

      <div
        className="
          no-drag
          absolute right-3 top-[49px] z-40
          flex w-[540px]
          overflow-hidden
          rounded-lg
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
          <div className="mb-1 px-2 py-1 text-[9px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">
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

                <p className="mt-0.5 text-[10px] text-[var(--luma-text-muted)]">
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
                <div className="mb-2 text-[9px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">
                  Preview
                </div>

                <div className="flex items-center gap-2">
                  <div
                    className="
                      h-7 w-7 rounded-md
                      bg-[var(--luma-accent-soft)]
                    "
                  />

                  <div>
                    <div className="text-[11px] font-medium">
                      {definition.name}
                    </div>

                    <div className="text-[9px] text-[var(--luma-text-muted)]">
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

                <p className="mt-0.5 text-[10px] text-[var(--luma-text-muted)]">
                  Configure how Luma behaves.
                </p>
              </div>

              <SettingsRow
                title="Workspace"
                description="Current workspace configuration"
                value="Coming soon"
              />

              <SettingsRow
                title="Agent behavior"
                description="Planning and execution preferences"
                value="Coming soon"
              />

              <SettingsRow
                title="Confirmations"
                description="Control when Luma asks before actions"
                value="Coming soon"
              />

              <SettingsRow
                title="Model"
                description="Configure the active model provider"
                value="Coming soon"
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
        text-left text-[10px]
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
}: {
  title: string;
  description: string;
  value: string;
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
        <div className="text-[10px] font-medium">{title}</div>

        <div className="mt-0.5 text-[9px] text-[var(--luma-text-muted)]">
          {description}
        </div>
      </div>

      <span className="ml-auto text-[9px] text-[var(--luma-text-muted)]">
        {value}
      </span>
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

        <span className="text-[10px] font-medium">{theme.name}</span>
      </div>

      <div className="text-[9px] leading-4 text-[var(--luma-text-muted)]">
        {theme.description}
      </div>
    </button>
  );
}

function Sidebar({
  selectedFile,
  onFileSelect,
}: {
  selectedFile: string | null;
  onFileSelect: (path: string) => void;
}) {
  return (
    <aside
      className="
        flex w-[220px] shrink-0 flex-col
        border-r border-[var(--luma-border)]
        bg-[var(--luma-surface)]
      "
    >
      <div className="flex h-9 items-center px-2.5">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-[var(--luma-text-muted)]">
          Workspace
        </span>

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
            aria-label="Search workspace"
          >
            <SearchIcon />
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
            aria-label="Add"
          >
            <PlusIcon />
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-1.5 pb-2">
        <WorkspaceTree
          nodes={workspace}
          selectedFile={selectedFile}
          onFileSelect={onFileSelect}
        />
      </div>

      <div
        className="
          flex h-8 shrink-0 items-center
          border-t border-[var(--luma-border)]
          px-2.5
          text-[9px]
          text-[var(--luma-text-muted)]
        "
      >
        <span className="truncate">~/projs/luma</span>
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
              text-left text-[10px]
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
          text-left text-[10px]
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

        <p className="mt-1 text-[10px] text-[var(--luma-text-muted)]">
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

          <span className="text-[10px] text-[var(--luma-text-secondary)]">
            {tool.name}
          </span>

          {tool.durationMs !== undefined && (
            <span className="ml-auto text-[9px] text-[var(--luma-text-muted)]">
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
      <div className="text-[11px] font-medium">Luma needs confirmation</div>

      <div className="mt-1 text-[10px] text-[var(--luma-text-muted)]">
        {confirmation.name}
      </div>

      {confirmation.input && (
        <pre
          className="
            mt-2 max-h-28 overflow-auto
            rounded-md
            bg-[var(--luma-bg)]
            p-2
            font-mono text-[9px]
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
            text-[10px] font-medium
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
            text-[10px]
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
        text-[10px]
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
            text-[11px]
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

      <div className="mt-1.5 text-center text-[9px] text-[var(--luma-text-muted)]">
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
  } = useLuma();

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

function IDEView({ selectedFile }: { selectedFile: string | null }) {
  const { theme } = useTheme();

  const fileName = selectedFile
    ? (selectedFile.split("/").pop() ?? selectedFile)
    : "untitled";

  const language = getLanguage(selectedFile);

  const monacoTheme = theme === "solarized" ? "vs-dark" : "vs-dark";

  return (
    <div className="flex h-full min-w-0 flex-col bg-[var(--luma-bg)]">
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
            text-[10px]
            text-[var(--luma-text-secondary)]
          "
        >
          <FileIcon />

          <span className="ml-1.5">{fileName}</span>
        </div>

        {selectedFile && (
          <span className="ml-3 truncate text-[9px] text-[var(--luma-text-muted)]">
            {selectedFile}
          </span>
        )}

        <div className="ml-auto px-3 text-[9px] text-[var(--luma-text-muted)]">
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
          text-[9px]
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
  );
}

function ChangesView() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <div className="text-[12px] font-medium">No changes</div>

        <div className="mt-1 text-[10px] text-[var(--luma-text-muted)]">
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

  useEffect(() => {
    let mounted = true;

    window.luma
      .status()
      .then((status) => {
        if (mounted) {
          setConnected(status.connected);
        }
      })
      .catch(() => {
        if (mounted) {
          setConnected(false);
        }
      });

    const unsubscribe = window.luma.onEvent((event) => {
      if (!mounted) {
        return;
      }

      if (event.type === "Ready") {
        setConnected(true);
      }

      if (event.type === "ProcessExited") {
        setConnected(false);
      }

      if (event.type === "Error") {
        setConnected(false);
      }
    });

    return () => {
      mounted = false;
      unsubscribe();
    };
  }, []);

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
      />

      <div className="flex min-h-0 flex-1">
        <Sidebar
          selectedFile={selectedFile}
          onFileSelect={(file) => {
            setSelectedFile(file);
            setMode("ide");
          }}
        />

        <main className="min-w-0 flex-1 overflow-hidden">
          {mode === "agent" && <AgentView />}

          {mode === "ide" && <IDEView selectedFile={selectedFile} />}

          {mode === "changes" && <ChangesView />}
        </main>
      </div>
    </div>
  );
}
