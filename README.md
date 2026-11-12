# ✨ Luma

**A lightweight AI coding agent built in Rust for multi-provider models, tool use, and low-memory environments.**

Luma is an autonomous coding assistant that runs inside your developer workspace. It helps you understand codebases, inspect files, modify code, debug problems, and automate development tasks using local or hosted language models.

Built to be small, fast, and practical — without requiring a massive model.

---

## ✨ Features

- 🦀 Built with Rust
- 🧠 Local LLM support
- 💾 Designed for low-memory environments
- 🔧 Tool-based agent architecture
- 📂 Workspace exploration
- 📖 Codebase understanding
- ✏️ Targeted code editing with `patch_file`
- 📝 Full file creation and replacement with `write_file`
- 💻 Shell command execution
- 🔍 File and code search
- 🧭 Symbol and reference search
- 🧾 File-change previews with explicit approval
- 🧠 Persistent task plans and recovery checkpoints
- 🔄 ReAct agent loop with iterative tool use and observations
- ⚡ Streaming responses
- 🖥️ Interactive terminal UI
- 🪟 Desktop workspace for Code and persistent Teammates
- 🧰 Desktop management for Agent Skills, native plugins, and MCP servers
- 🛑 Interruptible generation
- 🧩 Project memory through `GALAXY.md`

---

## 🧠 How It Works

Luma uses **ReAct (Reason + Act)**. A single configured model alternates between
choosing an action and using the resulting observation to decide what to do
next. There is no separate planner model or plan-generation pass between the
agent and its tools.

```text
              User Request
                   │
                   ▼
              ReAct Model
                   │
          ┌────────┴────────┐
          │                 │
       Tool Action       Final Answer
          │
          ▼
      Tool Execution
          │
          ▼
       Observation
          │
          └───────► ReAct Model
```

The model receives the conversation and previous tool observations on every
step. It can inspect the workspace, make an approved change, run verification,
and continue until it has enough evidence to answer. Tool permissions, change
previews, and approval gates remain enforced by the runtime.

For example:

```text
User: "Fix the borrow checker error in agent.rs"
  │
  ▼
ReAct → read_file
  │
  ▼
Observation: source contents
  │
  ▼
ReAct → patch_file
  │
  ▼
Observation: patch result
  │
  ▼
ReAct → run_command (cargo check)
  │
  ▼
Observation: compiler output
  │
  ▼
ReAct → final answer
```

This keeps decisions grounded in the actual workspace and lets the model adapt
after every tool result instead of committing to a brittle multi-step plan.

---

## 🔧 Tools

Luma currently provides:

| Tool | Description |
| --- | --- |
| `list_directory` | Explore files and directories |
| `read_file` | Read file contents |
| `search_files` | Search for files and code |
| `write_file` | Create or replace complete files |
| `patch_file` | Make precise edits to existing files |
| `run_command` | Execute shell commands (approval required) |
| `find_symbols` | Find function, type, class, and module declarations |
| `find_references` | Find usages of a symbol across the workspace |
| `git_status` / `git_diff` | Inspect repository state and changes |
| `git_log` | Review recent commit history |
| `git_show` | Inspect a commit or file at a revision |
| `git_blame` | Attribute individual lines to commits |
| `git_branches` | List local and remote branches |
| `file_metadata` | Inspect file type, size, timestamps, and permissions |
| `find_todos` | Find TODO, FIXME, XXX, HACK, and NOTE markers |
| `count_lines` | Count files and lines by extension |
| `inspect_project` | Read project manifests and discover build targets/scripts |
| `list_skills` / `load_skill` | Discover and progressively load reusable Agent Skills |

The inspection tools are read-only. Git and search output is bounded so a
large repository cannot flood the model context. Luma can combine these tools
with `run_command` when it needs to execute a build, test suite, or formatter.
Persistent Teammates receive this complete tool set plus `web_search`,
`fetch_url`, and `http_request`. Their network permission is configurable per
agent, and external integration calls remain approval-gated.

### Agent Skills

Luma supports the open Agent Skills directory convention. Each skill is a
directory containing a `SKILL.md` file with YAML frontmatter and instructions:

```text
my-project/
└── .agents/
    └── skills/
        └── rust-testing/
            └── SKILL.md
```

The frontmatter must declare a matching directory `name` and a non-empty
`description`:

```markdown
---
name: rust-testing
description: Run focused Rust tests and diagnose failures.
---

# Rust testing

Prefer focused tests first, then run the full suite before finalizing.
```

Luma discovers workspace skills in `.agents/skills/` and `.luma/skills/`,
and personal skills in `~/.agents/skills/` and Luma's config directory at
`skills/` (for example `~/.config/luma/skills/` on Linux). Workspace skills
override personal skills with the same name. The ReAct agent uses `list_skills`
to discover metadata and `load_skill` to load a relevant skill only when
needed, keeping unrelated instructions out of the model context. A loaded
skill can reference companion files in its own directory; use `read_file` to
inspect those resources. Skills are guidance, not permission to bypass user
approval or Luma's safety boundaries. Each Teammate can attach its own skills
and optionally inherit the Global teammate's shared skills and memory.

### External integrations

Luma supports two separate integration systems. **Native Luma Plugins** use
`plugin.toml` manifests and Luma's one-shot JSON process protocol. **MCP
servers** use the Model Context Protocol over local stdio or Streamable HTTP. Both
systems expose enabled tools to Code and to Teammates that inherit the Global
teammate's connectors. External tool calls require approval.

- [Native Luma Plugins](docs/plugins.md): install, inspect, enable, disable,
  and uninstall local plugin packages in Settings → Extensions or with `luma plugins`.
- [MCP servers](docs/mcp.md): register stdio or HTTP servers and inspect their
  tools in Settings → Extensions or with `luma mcp`.

### `patch_file`

`patch_file` is designed for targeted modifications.

Instead of rewriting an entire file, Luma can replace an exact piece of existing code:

```json
{
  "path": "src/main.rs",
  "old": "let value = old_value;",
  "new": "let value = new_value;"
}
```

The old text must match **exactly once**. This prevents ambiguous edits and makes small code changes safer.

The editing workflow is review-first:

```text
read_file
    │
    ▼
understand
    │
    ▼
 preview diff
    │
    ▼
approve / reject
    │
    ▼
 patch_file
    │
    ▼
run_command
    │
    ▼
verify
```

---

## 🤖 Supported Models

Luma works with OpenAI-compatible model APIs.

Supported providers include:

- Ollama
- LM Studio
- llama.cpp servers
- Other OpenAI-compatible endpoints

Luma is designed to work particularly well with smaller coding models.

Examples:

- Qwen Coder
- Gemma
- StarCoder
- Phi coding models

Model quality matters, but Luma's tool loop is designed to help smaller models stay grounded by giving them access to real workspace observations.

> [!NOTE]
> Although Luma can work with small models, it's always better to use bigger models. The bigger the model gets, the better the result will be.

---

## ⚙️ Configuration

Luma can connect to an OpenAI-compatible endpoint such as Ollama:

```text
http://localhost:11434/v1/chat/completions
```

Example model configuration:

```rust
let model = OpenAICompatibleModel::new(
    "http://localhost:11434/v1/chat/completions",
    "qwen2.5-coder:3b",
);
```

Configuration is stored separately from the agent itself, allowing the model and provider to be changed without changing the core agent architecture.

---

## 🚀 Installation

### Requirements

- Rust toolchain
- A local LLM server
- Or an OpenAI/Anthropic/Gemini-compatible model endpoint

### Clone

```bash
git clone https://github.com/ManiArasteh/luma
cd luma
```

### Build

```bash
cargo build --release
```

Cargo automatically builds the Vite dashboard before compiling `luma-core`;
if dashboard dependencies are not installed yet, its build script installs
them. The generated production assets are embedded directly into the core
executable, so a deployed `luma-core` does not need the dashboard directory,
Node.js, or npm. Dashboard source changes trigger a fresh dashboard build on
the next Cargo build. Use `luma dashboard` to start the Vite development server
and Rust API together. The Vite dev server proxies API and WebSocket traffic to
that core server. `LUMA_DASHBOARD_DIR` remains available as a development
override for assets on disk.

### Run the Ink CLI

```bash
cargo build --release
cd ui
bun install
bun run start
```

The Ink client discovers `target/release/luma-core` (or `luma-core` on `PATH`).
Run `luma setup` to configure a provider and model on first launch.

---

## 🖥️ Terminal UI

Luma includes an interactive terminal interface designed around a compact developer workflow.

It provides:

- Streaming model output
- Tool activity
- Command history
- Command autocomplete
- Workspace information
- Interruptible generation
- Interactive input
- Persistent conversation archives and summaries

### Saved chats

Luma automatically archives each conversation under the platform's Luma
configuration directory (`chats/`). Archives include the transcript, model,
workspace, timestamps, and an optional summary.

```bash
luma setup                                      # open the Ink provider/model setup
luma                                             # open the interactive Ink terminal UI
luma dashboard                                  # start Rust Core + browser dashboard together
luma dashboard --port 3001 --vite-port 5174      # choose alternate server ports
luma history                                    # list saved chats
luma history show <chat-id>                     # print a complete transcript
luma summary add <chat-id> "What this chat accomplished"
luma teammates                                   # open the Ink teammate manager
luma teammates list                              # list persistent teammates
luma teammates create Research --skill research --memory "Prefers concise reports"
luma teammates delete <agent-id>                 # delete a teammate and its owned data
luma tasks list [agent-id]
luma dashboard --host 127.0.0.1 --port 3000      # choose the Core bind address and port
luma ask "Inspect this project and explain its architecture" # one-shot prompt
```

The public CLI is provided by the Ink app using Commander and is the command
center for Luma's setup, development servers, dashboard, history, teammates,
tasks, and one-shot prompts. `luma teammates` opens an interactive Ink manager;
use the arrow keys to select a teammate, `d` to delete, `r` to refresh, and `q`
to quit. Teammates start with no injected personality prompt; `--personality`
can provide custom guidance, `--skill` can be repeated, and `--memory` adds initial agent memory.
Selected skills are loaded into each task context. `luma dashboard` starts the Rust Core dashboard API and the Vite development
server as one managed process group; Ctrl+C stops both. The Vite proxy is pointed
at the selected Core address automatically. These commands use the Rust
core's JSONL desktop server internally. The no-argument interactive Ink TUI
remains the default experience, and the legacy Ratatui TUI entrypoint is still
available in the Rust core for now.

### Persistent Teammates gateway

The first Luma core connection starts a local background gateway. It owns the
durable task supervisor and schedule runner independently of the Desktop,
Dashboard, and Ink processes, so queued or scheduled teammate work continues
after those interfaces close. Task state, approvals, and gateway commands are
stored in the shared SQLite runtime database; approval and cancellation
commands are acknowledged by the gateway rather than held only in a UI-owned
channel. The gateway starts again automatically the next time Luma is opened
after a system restart. It binds only to a private local IPC endpoint and does
not expose a network listener on Unix systems.

Inside the interactive chat, `/history` lists recent archives, `/restore
<chat-id>` loads an archive as the active conversation (including its model
context), and `/summary:add <text>` saves a summary for the current conversation.

Use `/plan <task>` to request an explicit implementation plan without starting
implementation. For normal coding tasks, Luma can also propose a plan and wait
for approval before editing. File edits display a diff preview and require
approval; shell commands also require confirmation. Press `Y` to approve or
`N` to reject a pending action. Task progress is checkpointed per workspace;
after an interrupted or crashed run, `/resume` asks Luma to continue from the
last saved observations and completed actions. Recovery re-checks the live
workspace rather than assuming that an in-progress operation completed.

The terminal UI is built with **Ink**. Slash commands autocomplete as you type; press
`Tab` to complete a command. Open the model picker with `/model` (press Enter)
or the `Ctrl+G` shortcut. Use `↑`/`↓` to choose a provider or model and `Esc`
to close the picker.

---

## 🧬 Workspace Memory

Luma can maintain project-level memory through:

```text
GALAXY.md
```

The file contains information about the workspace that Luma has discovered, such as:

- Project identity
- Programming language
- Important files
- Project structure
- Architecture notes

This gives Luma persistent project context without requiring a huge conversation history.

---

## 🏗️ Architecture

```text
luma/
├── agent/
│   └── Agent execution loop
│
├── react/
│   └── ReAct action selection and response parsing
│
├── model/
│   └── LLM abstraction
│
├── tools/
│   ├── filesystem/
│   │   ├── list_directory
│   │   ├── read_file
│   │   ├── search_files
│   │   ├── write_file
│   │   └── patch_file
│   │
│   └── shell/
│       └── run_command
│
├── context/
│   └── Conversation and workspace context
│
├── history/
│   └── Conversation history
│
├── workspace/
│   └── Project detection and workspace memory
│
├── tui/
│   └── Terminal interface
│
└── event/
    └── Agent events and streaming
```

The core architecture is intentionally modular:

```text
Model
  │
  ▼
ReAct
  │
  ▼
Agent
  │
  ├── Tools
  │
  ├── Context
  │
  ├── Workspace
  │
  └── Events
       │
       ▼
      TUI
```

---

## 🎯 Design Goals

Luma focuses on:

- **Provider-flexible AI**
- **Small and efficient models**
- **Low memory usage**
- **Grounded tool use**
- **Simple architecture**
- **Fast iteration**
- **Developer productivity**

The goal isn't to create the biggest AI coding agent.

The goal is to create an AI coding agent that is **actually useful on ordinary hardware**.

Luma should be able to run with a relatively small local model while still performing useful engineering tasks through tools, iterative reasoning, and workspace observations.

---

## 🗺️ Roadmap

### Completed

- [x] Rust core
- [x] OpenAI-compatible model support
- [x] Streaming responses
- [x] Tool system
- [x] Workspace inspection
- [x] ReAct loop
- [x] File search
- [x] File creation and replacement
- [x] Targeted file patching
- [x] Shell command execution
- [x] Project memory
- [x] Interactive TUI
- [x] Generation cancellation
- [x] Git status and diff inspection

### Planned

- [ ] LSP support
- [ ] Better project detection
- [ ] Improved context management
- [ ] Long-term memory
- [x] Tool confirmation system
- [x] File-change preview and review
- [x] Task plans and approval
- [x] Task checkpoints and recovery
- [x] Symbol and reference search
- [x] Git history, commit inspection, blame, and branch listing
- [x] Project manifest inspection, TODO scanning, metadata, and line counting
- [ ] Plugin system
- [ ] Language-server protocol (LSP) diagnostics and semantic navigation
- [ ] Parallel tool execution

---

## 🌌 Name

Luma is named after the small star-like creatures from **Super Mario Galaxy**.

Like its namesake, Luma guides developers through unknown spaces — except here, the unknown space is a codebase.

---

## 📄 License

MIT
