# Luma terminal UI

To install dependencies:

```bash
bun install
```

To start a development server:

```bash
bun dev
```

To run for production:

```bash
bun start
```

The Ink client talks to `luma-core` over the JSONL desktop-server protocol.
File edits show a proposed diff and wait for approval. Press `Y` to approve
or `N` to reject; `/resume` resumes an interrupted workspace task and
`/plan <task>` requests a plan without implementing it.

The Commander-powered public CLI includes:

```bash
luma setup
luma history [list]
luma history show <id>
luma summary add <id> <summary...>
luma dashboard [--host <host>] [--port <port>]
luma ask <prompt...>
```
