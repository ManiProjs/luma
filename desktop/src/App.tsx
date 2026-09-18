import { useState } from "react";

import ModeBar, { type AppMode } from "./components/ModeBar";

type FileItem = {
  name: string;
  type: "file" | "folder";
  children?: FileItem[];
};

const workspace: FileItem[] = [
  {
    name: "src",
    type: "folder",
    children: [
      { name: "agent", type: "folder" },
      { name: "commands", type: "folder" },
      { name: "context", type: "folder" },
      { name: "planner", type: "folder" },
      { name: "tools", type: "folder" },
      { name: "event.rs", type: "file" },
      { name: "main.rs", type: "file" },
    ],
  },
  {
    name: "tests",
    type: "folder",
  },
  {
    name: "Cargo.toml",
    type: "file",
  },
  {
    name: "GALAXY.md",
    type: "file",
  },
];

function FileIcon({ name }: { name: string }) {
  const extension = name.split(".").pop()?.toLowerCase();

  if (extension === "rs") {
    return (
      <span className="flex h-4 w-4 items-center justify-center text-[9px] font-semibold text-orange-400/70">
        R
      </span>
    );
  }

  if (extension === "toml") {
    return (
      <span className="flex h-4 w-4 items-center justify-center text-[9px] font-semibold text-zinc-500">
        T
      </span>
    );
  }

  if (extension === "md") {
    return (
      <span className="flex h-4 w-4 items-center justify-center text-[9px] font-semibold text-blue-400/60">
        M
      </span>
    );
  }

  return (
    <span className="flex h-4 w-4 items-center justify-center text-[10px] text-zinc-600">
      ·
    </span>
  );
}

function FolderIcon({ open }: { open: boolean }) {
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
      className={open ? "text-zinc-400" : "text-zinc-600"}
    >
      {open ? (
        <>
          <path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v1H3.5v-3z" />
          <path d="M3.5 9.5h17l-1.5 8.5a2 2 0 0 1-2 1.7H6a2 2 0 0 1-2-1.7L3.5 9.5z" />
        </>
      ) : (
        <path d="M3.5 6.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-11z" />
      )}
    </svg>
  );
}

function FileTreeItem({ item, depth = 0 }: { item: FileItem; depth?: number }) {
  const [open, setOpen] = useState(true);

  if (item.type === "file") {
    return (
      <button
        type="button"
        className="flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px] text-zinc-500 transition-colors hover:bg-white/[0.035] hover:text-zinc-200"
        style={{ paddingLeft: `${8 + depth * 12}px` }}
      >
        <FileIcon name={item.name} />

        <span className="truncate">{item.name}</span>
      </button>
    );
  }

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px] text-zinc-400 transition-colors hover:bg-white/[0.035] hover:text-zinc-200"
        style={{ paddingLeft: `${8 + depth * 12}px` }}
      >
        <span className="flex h-4 w-4 items-center justify-center">
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            fill="none"
            className={[
              "transition-transform duration-150",
              open ? "rotate-90" : "",
            ].join(" ")}
          >
            <path
              d="M3.5 2L6.5 5L3.5 8"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </span>

        <FolderIcon open={open} />

        <span className="truncate">{item.name}</span>
      </button>

      {open && item.children && (
        <div>
          {item.children.map((child) => (
            <FileTreeItem key={child.name} item={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}

function Sidebar() {
  return (
    <aside className="flex w-[250px] shrink-0 flex-col border-r border-white/[0.06] bg-[#0a0a0c]">
      <div className="flex h-11 shrink-0 items-center border-b border-white/[0.05] px-4">
        <span className="text-[10px] font-semibold tracking-[0.14em] text-zinc-600">
          WORKSPACE
        </span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {workspace.map((item) => (
          <FileTreeItem key={item.name} item={item} />
        ))}
      </div>

      <div className="border-t border-white/[0.05] p-2">
        <button
          type="button"
          className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-[11px] text-zinc-600 transition-colors hover:bg-white/[0.035] hover:text-zinc-300"
        >
          <span className="flex h-5 w-5 items-center justify-center rounded border border-white/[0.06] text-[10px] text-zinc-600">
            ⌘
          </span>

          <span>Command Palette</span>

          <span className="ml-auto text-[10px] text-zinc-700">⌘K</span>
        </button>
      </div>
    </aside>
  );
}

function SendIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 19V5" />
      <path d="M6 11l6-6 6 6" />
    </svg>
  );
}

function Composer() {
  const [value, setValue] = useState("");

  const canSend = value.trim().length > 0;

  return (
    <div className="shrink-0 px-6 pb-5 pt-3">
      <div className="mx-auto max-w-[820px] overflow-hidden rounded-xl border border-white/[0.07] bg-[#101012] shadow-[0_16px_50px_rgba(0,0,0,0.28)] transition-colors focus-within:border-white/[0.12]">
        <textarea
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();

              if (value.trim()) {
                // Agent connection will be wired here.
              }
            }
          }}
          placeholder="Ask Luma..."
          rows={3}
          className="block w-full resize-none bg-transparent px-4 py-3.5 text-[13px] leading-6 text-zinc-200 outline-none placeholder:text-zinc-600"
        />

        <div className="flex items-center justify-between px-3 pb-2.5">
          <div className="flex items-center gap-0.5">
            <button
              type="button"
              className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[11px] text-zinc-600 transition-colors hover:bg-white/[0.05] hover:text-zinc-300"
            >
              <span className="text-[14px] leading-none">+</span>
              Context
            </button>

            <button
              type="button"
              className="flex h-7 items-center rounded-md px-2 text-[11px] text-zinc-600 transition-colors hover:bg-white/[0.05] hover:text-zinc-300"
            >
              Tools
            </button>
          </div>

          <button
            type="button"
            disabled={!canSend}
            className="flex h-7 items-center gap-1.5 rounded-md bg-zinc-100 px-2.5 text-[11px] font-medium text-zinc-900 transition-all hover:bg-white disabled:cursor-default disabled:opacity-15"
          >
            Send
            <SendIcon />
          </button>
        </div>
      </div>

      <p className="mt-2 text-center text-[10px] text-zinc-700">
        Luma can read, modify, and run code in your workspace.
      </p>
    </div>
  );
}

function AgentIcon() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M12 2.8l1.5 6.2L19.7 10.5l-6.2 1.5L12 18.2l-1.5-6.2-6.2-1.5 6.2-1.5L12 2.8z" />
    </svg>
  );
}

function IDEIcon() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <rect x="3.5" y="4" width="17" height="16" rx="2.5" />

      <path d="M8 8.5l-2 2 2 2" />
      <path d="M11 13h5" />
    </svg>
  );
}

function ChangesIcon() {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M6 4v11" />
      <path d="M6 15l-2.5-2.5" />
      <path d="M6 15l2.5-2.5" />

      <path d="M18 20V9" />
      <path d="M18 9l-2.5 2.5" />
      <path d="M18 9l2.5 2.5" />
    </svg>
  );
}

function AgentView() {
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto pb-4">
        <div className="flex min-h-full items-center justify-center px-6">
          <div className="flex max-w-[440px] flex-col items-center text-center">
            <div className="mb-5 flex h-12 w-12 items-center justify-center rounded-xl border border-white/[0.07] bg-white/[0.025]">
              <span className="text-[17px] font-semibold text-zinc-400">L</span>
            </div>

            <h1 className="text-[20px] font-medium tracking-tight text-zinc-200">
              What are we building?
            </h1>

            <p className="mt-2 text-[13px] leading-5 text-zinc-600">
              Ask Luma to explore your codebase, implement a feature, fix a bug,
              or work through a task.
            </p>
          </div>
        </div>
      </div>

      <Composer />
    </div>
  );
}

function IDEView() {
  const [open, setOpen] = useState(true);

  return (
    <div className="flex min-w-0 flex-1">
      <div className="flex w-[230px] shrink-0 flex-col border-r border-white/[0.06] bg-[#0a0a0c]">
        <div className="flex h-11 items-center justify-between border-b border-white/[0.05] px-4">
          <span className="text-[10px] font-semibold tracking-[0.14em] text-zinc-600">
            EXPLORER
          </span>

          <button
            type="button"
            onClick={() => setOpen((value) => !value)}
            className="text-[11px] text-zinc-700 transition-colors hover:text-zinc-400"
          >
            {open ? "−" : "+"}
          </button>
        </div>

        {open && (
          <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
            {workspace.map((item) => (
              <FileTreeItem key={item.name} item={item} />
            ))}
          </div>
        )}
      </div>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-10 shrink-0 items-center border-b border-white/[0.05] bg-[#0b0b0d] px-4">
          <span className="text-[11px] text-zinc-600">No file open</span>
        </div>

        <div className="flex min-h-0 flex-1 items-center justify-center">
          <div className="text-center">
            <div className="mb-2 text-[13px] text-zinc-600">
              Select a file to start editing
            </div>

            <div className="text-[11px] text-zinc-800">
              Monaco Editor will appear here
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function ChangesView() {
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex h-12 shrink-0 items-center border-b border-white/[0.05] px-5">
        <div>
          <h2 className="text-[13px] font-medium text-zinc-300">Changes</h2>

          <p className="mt-0.5 text-[11px] text-zinc-600">
            Review changes made by Luma
          </p>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 items-center justify-center">
        <div className="text-center">
          <div className="mx-auto mb-3 flex h-10 w-10 items-center justify-center rounded-lg border border-white/[0.06] bg-white/[0.02]">
            <ChangesIcon />
          </div>

          <h2 className="text-[15px] font-medium text-zinc-300">No changes</h2>

          <p className="mt-1.5 text-[12px] text-zinc-600">
            Changes made by Luma will appear here.
          </p>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const [mode, setMode] = useState<AppMode>("agent");

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-[#09090b] text-zinc-100">
      <header className="flex h-12 shrink-0 items-center border-b border-white/[0.06] bg-[#0b0b0d] px-4">
        <div className="flex items-center gap-2">
          <div className="flex h-6 w-6 items-center justify-center rounded-md border border-white/[0.07] bg-white/[0.025]">
            <span className="text-[10px] font-semibold text-zinc-300">L</span>
          </div>

          <span className="text-[13px] font-semibold tracking-tight text-zinc-200">
            Luma
          </span>
        </div>

        <div className="ml-auto flex items-center gap-1">
          <div className="mr-1 flex items-center gap-1.5 rounded-md px-2 py-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />

            <span className="text-[10px] text-zinc-600">Ready</span>
          </div>

          <button
            type="button"
            className="flex h-7 items-center rounded-md px-2 text-[11px] text-zinc-600 transition-colors hover:bg-white/[0.04] hover:text-zinc-300"
          >
            ⌘K
          </button>

          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded-md text-zinc-600 transition-colors hover:bg-white/[0.04] hover:text-zinc-300"
            aria-label="Settings"
          >
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
              <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z" />
              <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-1.7 1.7-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5v.2h-2.4v-.2a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1L8 17l.1-.1A1.7 1.7 0 0 0 8.4 15a1.7 1.7 0 0 0-1.5-1H6.7v-2.4h.2a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9L8 8.6l1.7-1.7.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.5v-.2h2.4v.2a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1 1.7 1.7-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.5 1h.2V14h-.2a1.7 1.7 0 0 0-1.5 1z" />
            </svg>
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <Sidebar />

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="min-h-0 flex-1 overflow-hidden">
            {mode === "agent" && <AgentView />}

            {mode === "ide" && <IDEView />}

            {mode === "changes" && <ChangesView />}
          </div>

          {/* Dedicated navigation layer */}
          <div
            className="
      flex
      h-[86px]
      shrink-0
      items-center
      justify-center
    "
          >
            <ModeBar mode={mode} onModeChange={setMode} />
          </div>
        </main>
      </div>
    </div>
  );
}
