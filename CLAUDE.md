# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

- **Build**: `cargo build --release`
- **Run**: `cargo run --release`
- **Test**: `cargo test`
- **Run single test**: `cargo test <test_name>`
- **Lint**: `cargo clippy`
- **Format**: `cargo fmt`

## Architecture Overview

Luma is a modular AI coding agent built in Rust. It uses a single-model ReAct loop: the model chooses an action, observes its result, and decides the next step or final answer.

### Core Components

- **Agent Loop (`src/agent.rs`)**: The central orchestration logic that manages the conversation, state, and the execution loop. It handles user input, agent decisions, and tool execution.
- **ReAct (`src/react.rs`)**: Builds the action prompt, parses the model response, validates tool names, and returns one action or a final answer. It shares the configured agent model; there is no separate planner model.
- **Model Abstraction (`src/model/`)**: Provides a unified interface for interacting with OpenAI-compatible APIs (Ollama, LM Studio, etc.).
- **Tools (`src/tools/`)**: A registry of capabilities including:
  - **Filesystem**: `list_directory`, `read_file`, `search_files`, `write_file`, `patch_file` (for precise edits).
  - **Shell**: `run_command` for executing arbitrary terminal commands.
- **Terminal UI (`src/tui/`)**: A compact, interactive interface built with `ratatui` for streaming responses, displaying tool activity, and managing user input.
- **Workspace & Memory (`src/workspace/`)**: Handles project detection and manages `GALAXY.md`, which serves as persistent project-level memory.
- **Context Management (`src/context.rs`)**: Manages the conversation history and workspace observations.

### Data Flow

1. **User Request** $\rightarrow$ `Agent`
2. `Agent` $\rightarrow$ `ReAct` (with the live `Context`)
3. `ReAct` $\rightarrow$ `Action` or `Final Answer` (JSON)
4. `Agent` $\rightarrow$ `ToolRegistry` $\rightarrow$ `Tool Execution`
5. `Tool Result` $\rightarrow$ `Observation` $\rightarrow$ `Context`
6. Repeat the ReAct step until a final answer, cancellation, or `MAX_STEPS`.

### Key Design Patterns

- **ReAct**: Each action is followed by an observation in the conversation context; the same model decides what to do next.
- **Modular Tools**: Tools are registered in a `ToolRegistry` and implemented as discrete units, making it easy to add new capabilities.
- **Event-Driven TUI**: The TUI communicates with the Agent via asynchronous channels (`mpsc`), allowing for non-blocking streaming of model output and tool status.
