# ✨ Luma

**A lightweight AI coding agent built in Rust for local LLMs, tool use, and low-memory environments.**

Luma is an autonomous coding assistant that runs inside your developer workspace. It helps you understand codebases, inspect files, modify code, debug problems, and automate development tasks using local language models.

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
- 🔄 Planner → Tool → Observation agent loop
- ⚡ Streaming responses
- 🖥️ Interactive terminal UI
- 🛑 Interruptible generation
- 🧩 Project memory through `GALAXY.md`

---

## 🧠 How It Works

Luma uses a planning-based agent loop.

```text
             User Request
                  │
                  ▼
               Planner
                  │
                  ▼
             Tool Selection
                  │
                  ▼
             Tool Execution
                  │
                  ▼
              Observation
                  │
             ┌────┴────┐
             │         │
          More work   Done
             │         │
             ▼         ▼
          Planner   Final Answer
```

Instead of blindly generating an answer, Luma can inspect the workspace, gather observations, modify files, run commands, and verify its work.

For example:

```text
User
 │
 ▼
"Fix the borrow checker error in agent.rs"
 │
 ▼
Planner
 │
 ▼
read_file
 │
 ▼
Observation
 │
 ▼
Planner
 │
 ▼
patch_file
 │
 ▼
run_command
 │
 ▼
cargo check
 │
 ▼
Final Answer
```

This keeps the model grounded in the actual project instead of relying on guesses.

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
| `run_command` | Execute shell commands |

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

The typical editing workflow is:

```text
read_file
    │
    ▼
understand
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

### Run

```bash
cargo run --release
```

Then create a config if you don't have one.

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

The UI is built with **Ratatui** and is intentionally compact rather than trying to imitate a full IDE.

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
├── planner/
│   └── Decision making
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
Planner
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

- **Local-first AI**
- **Small and efficient models**
- **Low memory usage**
- **Grounded tool use**
- **Simple architecture**
- **Fast iteration**
- **Developer productivity**

The goal isn't to create the biggest AI coding agent.

The goal is to create an AI coding agent that is **actually useful on ordinary hardware**.

Luma should be able to run with a relatively small local model while still performing useful engineering tasks through tools, planning, and workspace observations.

---

## 🗺️ Roadmap

### Completed

- [x] Rust core
- [x] OpenAI-compatible model support
- [x] Streaming responses
- [x] Tool system
- [x] Workspace inspection
- [x] Planner loop
- [x] File search
- [x] File creation and replacement
- [x] Targeted file patching
- [x] Shell command execution
- [x] Project memory
- [x] Interactive TUI
- [x] Generation cancellation

### Planned

- [ ] Git integration
- [ ] LSP support
- [ ] Better project detection
- [ ] Improved context management
- [ ] Long-term memory
- [ ] Tool confirmation system
- [ ] Plugin system
- [ ] More specialized developer tools
- [ ] Smarter planning and recovery
- [ ] Parallel tool execution

---

## 🌌 Name

Luma is named after the small star-like creatures from **Super Mario Galaxy**.

Like its namesake, Luma guides developers through unknown spaces — except here, the unknown space is a codebase.

---

## 📄 License

MIT
