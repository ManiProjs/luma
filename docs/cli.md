# CLI and terminal interface

The Ink client provides setup, the interactive terminal UI, dashboard startup,
history, teammate management, task inspection, and one-shot prompts.

## Common commands

```sh
luma setup
luma
luma dashboard
luma dashboard --port 3001
luma history
luma history show <chat-id>
luma summary add <chat-id> "What this chat accomplished"
luma teammates
luma teammates list
luma teammates create Research --skill research --memory "Prefers concise reports"
luma teammates delete <agent-id>
luma tasks list [agent-id]
luma ask "Inspect this project and explain its architecture"
```

The no-argument command opens the interactive Ink terminal UI. The legacy
Ratatui entrypoint remains available in the Rust core. `luma teammates` opens
an interactive manager: use the arrow keys to select a teammate, `d` to
delete, `r` to refresh, and `q` to quit.

## Dashboard

`luma dashboard` starts the Rust Core and its embedded production dashboard;
Ctrl+C stops the process. The default bind address is loopback-only. Cargo
builds the dashboard before compiling `luma-core` and embeds the production
assets in the executable.

## Saved conversations

The terminal UI archives conversations in Luma's platform configuration
directory under `chats/`. Archives contain the transcript, model, workspace,
timestamps, and an optional summary.

Inside an interactive chat:

- `/history` lists recent archives.
- `/restore <chat-id>` loads an archive as the active conversation, including
  its model context.
- `/summary:add <text>` saves a summary for the current conversation.
- `/plan <task>` requests an explicit plan without starting implementation.
- `/resume` continues from the last saved observations after an interruption.

Slash commands autocomplete; press `Tab` to complete one. Open the model picker
with `/model` or `Ctrl+G`, navigate with the arrow keys, and press `Esc` to
close it. Toggle the alternate full-screen terminal with `Ctrl+F` or
`/fullscreen`.
