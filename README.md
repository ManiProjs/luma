# Luma

**A lightweight AI coding agent and persistent teammate runtime, built in Rust.**

Luma helps developers explore codebases, make reviewed changes, run commands,
and delegate ongoing work to persistent Teammates. It supports OpenAI-compatible
model endpoints, local model servers, a terminal interface, and a desktop
workspace.

## Quick start

Luma currently ships its Rust Core, Ink terminal client, local dashboard, and
Electron desktop app as source-buildable components. The first 1.0 beta is a
prerelease; packaged desktop installers are not yet published.

```sh
git clone https://github.com/ManiProjs/luma.git
cd luma
cargo build --release --no-default-features

cd ui
bun install
bun run start
```

The terminal client starts the Rust Core automatically. To use a different
Core executable, set `LUMA_BINARY` to its path. From `ui/`, start the local
dashboard with `bun run src/index.tsx dashboard`.

To build the desktop app on your current platform:

```sh
cd desktop
npm ci
npm run make
```

## What Luma includes

- Rust agent runtime with iterative tool use and streaming responses.
- Workspace inspection, code search, file editing, and command execution.
- Review and approval gates for changes and external actions.
- Desktop and terminal clients, plus a local dashboard.
- Persistent Teammates with conversation history, memory, schedules, and
  delegated work.
- Durable task-action journals, crash recovery, reconciled retries, and
  persisted approval-aware plans.
- A cross-agent Inbox, specialist role presets, and operational model/reliability
  scorecards.
- Teammate-only GitHub issue, pull request, review, and Actions tools, plus
  approval-gated Playwright browser automation.
- Agent Skills, native plugins, and Model Context Protocol (MCP) integrations.

## Documentation

- [Architecture and runtime](docs/architecture.md)
- [CLI and terminal interface](docs/cli.md)
- [Tools and change approval](docs/tools.md)
- [Configuration and models](docs/configuration.md)
- [Agent Skills](docs/skills.md)
- [Persistent Teammates](docs/teammates.md)
- [Native plugins](docs/plugins.md)
- [MCP integrations](docs/mcp.md)

## Development

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

The UI checks are `bun run typecheck` in `ui`, `npm run typecheck && npm run
build` in `dashboard`, and `npm run typecheck && npm run lint` in `desktop`.

Luma is licensed under MIT. Its name comes from the star-like creatures in
*Super Mario Galaxy*.
