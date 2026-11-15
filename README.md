# Luma

**A lightweight AI coding agent and persistent teammate runtime, built in Rust.**

Luma helps developers explore codebases, make reviewed changes, run commands,
and delegate ongoing work to persistent Teammates. It supports OpenAI-compatible
model endpoints, local model servers, a terminal interface, and a desktop
workspace.

## Quick start

```sh
git clone https://github.com/ManiArasteh/luma
cd luma
cargo build --release
cargo run -- setup
```

Run the interactive terminal client with `cd ui && bun install && bun run
start`, or start the browser dashboard with `cargo run -- dashboard`.

## What Luma includes

- Rust agent runtime with iterative tool use and streaming responses.
- Workspace inspection, code search, file editing, and command execution.
- Review and approval gates for changes and external actions.
- Desktop and terminal clients, plus a local dashboard.
- Persistent Teammates with conversation history, memory, schedules, and
  delegated work.
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

Luma is licensed under MIT. Its name comes from the star-like creatures in
*Super Mario Galaxy*.
