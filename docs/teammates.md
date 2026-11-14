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
