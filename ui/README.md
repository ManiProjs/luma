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
