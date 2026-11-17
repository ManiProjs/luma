# Teammates

Teammates are persistent personal agents backed by the Luma Core runtime. Their
identity, permissions, memory, task history, schedules, and event triggers are
stored in the shared SQLite runtime database. The gateway supervises queued
work independently of the Desktop and Dashboard clients.

## Teams and specializations

Teammates can have a specialization such as Architect, Graphics Engineer,
Reviewer, Tester, Researcher, or Security. The runtime adds that role to the
Teammate's task context while preserving its configured system instructions
and permission profile.

Persistent Teams group Teammates around a shared purpose. A Teammate can use
`create_team`, `add_team_member`, and `list_teams` to create a group, assign
named roles, and inspect its roster. Membership and role changes are stored in
SQLite and survive restarts.

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

```luma
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

`create_monitor` creates a recurring schedule for repository health, CI,
dependency, or security checks. These monitors run as ordinary Teammate tasks
and inherit the Teammate's workspace grants, network policy, and approval
requirements. Dependency updates and repository mutations are recommendations
that still require the normal approval flow.

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
history with a delegated marker. The gateway runs independent workspaces
concurrently, up to eight workers globally and the configured per-Teammate
limit (1–8). A canonical workspace lease prevents simultaneous workers from
modifying the same checkout. Tasks targeting a leased workspace remain queued
until it becomes available. `delegate_task` accepts an optional `workspace`
path for an explicitly granted alternate checkout, allowing parallel work on
separate checkouts of the same project. Both Teammates must have the path in
their workspace grants.

When a parent finishes before its children, Luma waits for every direct child
to reach a terminal state and then queues one continuation in the original
conversation. The continuation includes each child's response or failure so
the parent can review and integrate the results. A durable unique marker keeps
restarts and simultaneous child completions from creating duplicate
continuations.

Use `add_task_dependency` to make one queued delegated task wait for another.
Dependencies are cycle-checked; a task runs only after every prerequisite
completes successfully, and it fails with an explanation if a prerequisite
fails or is cancelled.

Delegation is bounded to depth 3, three direct children per task, and eight
descendants per root task. Only Teammates can delegate to other Teammates;
paused/stopped targets, self-delegation, cross-agent conversation IDs, and
workspace access outside either agent's grants are rejected. A child's
permission ceiling is the intersection of the parent's effective permissions
and the target Teammate's own profile: denials are combined, approval
requirements become stricter, and network access is never widened. The normal
tool approval flow applies to `delegate_task` unless the parent has explicitly
allowed it. Delegation queues work asynchronously rather than blocking the
supervisor, avoiding parent/child deadlocks.

## Restart recovery and observability

On restart, queued work remains queued and active or waiting tasks are marked
`interrupted`. Luma expires their pending approvals and resumes a completed
parent's continuation when all descendants have become terminal. It does not
automatically replay an interrupted request: tool calls can have external side
effects, and the runtime does not yet persist an idempotent action cursor. The
legacy `recover_on_restart` and `recover_interrupted_tasks` fields are retained
for stored/API compatibility but do not override this safety rule. Reconcile
external state before manually retrying an interrupted task.

The read-only `execution_metrics` tool reports persisted token usage, estimated
cost, tool durations/failures, and task lifecycle/execution durations. It reads
up to the latest 1,000 events per Teammate and up to 100 task records per call.
Cost is an estimate using Luma's default token rates, not authoritative billing
data for a provider or custom endpoint.
