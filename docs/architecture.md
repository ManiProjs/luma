# Architecture

Luma uses a ReAct (Reason + Act) loop. The configured model chooses an action,
the runtime executes it, and the resulting observation is supplied to the
model on the next step. There is no separate planner model or mandatory
plan-generation pass between the agent and its tools.

```text
User request
    │
    ▼
ReAct model ◄──────────────┐
    │                      │
    ▼                      │
Tool execution             │
    │                      │
    ▼                      │
Observation ───────────────┘
    │
    ▼
Final response
```

The model receives the conversation and previous tool observations at every
step. It can inspect the workspace, propose a change, run verification, and
continue until it has enough evidence to respond. Tool permissions, change
previews, and approval gates are enforced by the runtime.

## Runtime components

- **Core:** Rust runtime, model abstraction, agent loop, tools, and event
  streaming.
- **Ink CLI:** terminal setup, interactive coding UI, history, and teammate
  management.
- **Desktop:** Code and Teammates workspace with file, task, and change
  inspectors.
- **Dashboard:** browser interface backed by the local Core API.
- **Gateway:** durable task supervision and schedule execution, independent of
  the desktop, dashboard, and Ink clients.

The gateway stores task state, approvals, schedules, and commands in the shared
SQLite runtime database. It runs queued or scheduled teammate work when a UI
client is closed and starts again when Luma is next opened after a restart.
On Unix, its IPC endpoint is local and does not expose a network listener.

Independent workspaces execute concurrently (up to eight workers globally),
with per-Teammate concurrency limits and exclusive canonical workspace leases.
Delegated task dependencies are stored in SQLite and checked before execution;
cycles are rejected and failed prerequisites stop dependent work. Once every
child finishes, the runtime atomically queues one continuation for its parent
conversation with the collected child results.

Teammates can be grouped into persistent Teams with named specialist roles.
Their specialization is injected into task context, while team membership and
role changes are stored in the runtime database. Scheduled repository monitors
reuse the same durable schedule, permission, and approval infrastructure.

## Workspace memory

Luma can maintain project context in `GALAXY.md`, including project identity,
languages, important files, structure, and architecture notes. This provides
workspace-specific context without relying on an unbounded conversation
transcript.

## Design goals

- Provider-flexible model access.
- Useful local-model support and modest resource requirements.
- Grounded tool use with observable results.
- Explicit approval for sensitive operations.
- A modular runtime shared by multiple clients.
