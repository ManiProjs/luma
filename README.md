# ✨ Luma

> A lightweight AI coding agent built in Rust, designed for local LLMs, tool usage, and low-memory environments.

Luma is an autonomous coding assistant that runs inside your developer workspace.

It helps developers understand codebases, inspect files, debug problems, and automate development tasks using local language models.

---

## Features

- 🦀 Built with Rust
- 🧠 Local LLM support
- 💾 Low-memory friendly
- 🔧 Tool-based agent architecture
- 📂 Workspace exploration
- 📖 Codebase understanding
- 💻 Shell command execution
- 🔄 Planner → Tool → Observation agent loop
- ⚡ Streaming responses
- 🖥️ CLI/TUI architecture

---

## How It Works

Luma uses a planning-based agent loop:

    User Request
          |
          v
       Planner
          |
          v
     Tool Selection
          |
          v
     Tool Execution
          |
          v
      Observation
          |
          v
      Final Answer

Instead of guessing, Luma inspects the workspace and gathers information before responding.

---

## Supported Models

Luma supports OpenAI-compatible model APIs.

Supported providers:

- Ollama
- LM Studio
- llama.cpp servers
- Other compatible APIs

Recommended models:

- Qwen Coder 3B
- StarCoder 3B
- Phi coding models

---

## Tools

Luma can interact with the developer workspace.

Current tools:

| Tool | Description |
|---|---|
| `list_directory` | Explore files and folders |
| `read_file` | Read file contents |
| `run_command` | Execute shell commands |

Planned tools:

- Code editing
- Git operations
- Search
- LSP integration
- Plugin system

---

## Installation

Requirements:

- Rust toolchain
- Local LLM server

Clone:

    git clone https://github.com/ManiArasteh/luma
    cd luma

Build:

    cargo build --release

Run:

    cargo run --release

---

## Configuration

Example:

    let model = OpenAICompatibleModel::new(
        "http://localhost:11434/v1/chat/completions",
        "qwen2.5-coder:3b",
    );

---

## Architecture

    luma
    ├── agent
    │   └── Agent execution loop
    │
    ├── planner
    │   └── Decision making
    │
    ├── model
    │   └── LLM abstraction
    │
    ├── tools
    │   ├── filesystem
    │   └── shell
    │
    ├── context
    │   └── Conversation state
    │
    └── event
        └── Streaming events

---

## Design Goals

Luma focuses on:

- Local-first AI
- Small and efficient models
- Low memory usage
- Simple architecture
- Developer productivity

The goal is not to create the biggest AI agent.

The goal is to create a useful AI agent that can run anywhere.

---

## Roadmap

Completed:

- [x] Rust core
- [x] OpenAI-compatible model support
- [x] Streaming responses
- [x] Tool system
- [x] Workspace inspection
- [x] Planner loop

Planned:

- [ ] Terminal UI
- [ ] Code editing tools
- [ ] Git integration
- [ ] LSP support
- [ ] Long-term memory
- [ ] Plugin system

---

## Name

Luma is named after the small star-like creatures from Super Mario Galaxy.

Like its namesake, Luma guides developers through unknown spaces — in this case, unfamiliar codebases.

---

## License

MIT
