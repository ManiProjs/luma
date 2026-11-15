# Teammates

Teammates are persistent personal agents backed by the Luma Core runtime. Their
identity, permissions, memory, task history, schedules, and event triggers are
stored in the shared SQLite runtime database. The gateway supervises queued
work independently of the Desktop and Dashboard clients.

## Conversations and tasks

A conversation has a stable ID and can contain multiple task runs. Each user
message remains a separately cancellable, retryable execution task, while the
conversation ID controls which earlier turns are loaded into the model context.
The Desktop history list groups these task runs into conversations. Starting a
new chat creates a new conversation; sending another message in an existing
chat continues that conversation. Conversation IDs are scoped to their owning
Teammate and cannot be reused by another agent.

## Rich responses

Teammate replies support Markdown plus optional structured visual blocks. A
Teammate can place one JSON object inside a fenced code block tagged `luma`:

```json
{"type":"stat","label":"Tasks completed","value":12,"detail":"This week"}
```

Supported block types are `image`, `gallery`, `table`, `stat`, `chart`,
`callout`, and `link`. Image and link URLs must use HTTP or HTTPS; unsupported
or malformed blocks fall back to a readable JSON code block. Galleries, table
rows, and chart series are bounded to keep responses compact. Ordinary Markdown
and existing conversation history continue to render as before. Rich blocks
are presentation only: they cannot execute HTML, scripts, or arbitrary code.

## Long-term memory

Teammates can save durable facts with `save_memory` and retrieve them with
`recall_memory`. Recall ranks matching terms and phrase coverage rather than
requiring an exact contiguous substring. At task start, Luma injects only the
most relevant saved memories instead of every saved note. Exact facts with
different punctuation or word order are deduplicated. Agent memory stays
private; Global and shared-user memory are included only when the Teammate opts
into Global context. Secrets, credentials, temporary task state, and sensitive
personal information should not be saved.

## Proactive work

Teammates support two forms of proactive execution:

- **Schedules** run a prompt once or on a recurring interval.
- **Event triggers** queue a prompt when a supported runtime event occurs.

Event triggers can currently listen for:

- `task.completed`
- `task.failed`
- `approval.requested`
- `memory.created`

Each trigger has a minimum cooldown of one minute and a maximum of 24 hours.
The gateway records each trigger/event delivery transactionally, so a restart
does not duplicate an already queued run. Events that predate trigger creation
are ignored. A task started by an event trigger is marked with its origin; its
own events cannot recursively activate event triggers. Teammate-created memory
events are also excluded from trigger dispatch to avoid save-memory loops.

Triggers are managed in Teammate Settings → Schedules. Paused or stopped
Teammates do not receive new trigger work. Trigger prompts are background
instructions, not approvals: tool permissions, workspace grants, network
policy, and approval requirements continue to apply normally.

## Delegation and handoffs

Teammates can discover peers with `list_teammates`, queue independent work with
`delegate_task`, list work delegated anywhere in the conversation with
`list_delegated_tasks`, and inspect a child result with `get_delegated_task`. Delegation
creates a durable child task assigned to the target Teammate; the parent keeps
running and can continue its own work while the gateway later executes the
child. Child tasks inherit the parent's conversation ID while retaining their
own Teammate owner and task history. The parent conversation shows each direct
handoff with its target and status; selecting one opens the target Teammate's
child task. The returned child task ID is the handle used to retrieve its status,
error, and recorded response. Child tasks appear in the target Teammate's
history with a delegated marker. Because the gateway currently serializes task
execution, child work starts after the parent task finishes. A later turn in
the same conversation can list the earlier handoffs and collect their results;
the parent is not automatically resumed when a child finishes.

Delegation is bounded to depth 3, three direct children per task, and eight
descendants per root task. Only Teammates can delegate to other Teammates;
paused/stopped targets, self-delegation, cross-agent conversation IDs, and
workspace access outside either agent's grants are rejected. A child's
permission ceiling is the intersection of the parent's effective permissions
and the target Teammate's own profile: denials are combined, approval
requirements become stricter, and network access is never widened. The normal
tool approval flow applies to `delegate_task` unless the parent has explicitly
allowed it. Delegation queues work asynchronously rather than blocking the
single-task supervisor, avoiding parent/child deadlocks.
