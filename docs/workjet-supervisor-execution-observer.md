# Native supervisor execution observer

The existing `ctox.workjet.project.supervisor.turn.watch` request and
`ctox.workjet.supervisor_turn.v1` result stay byte-shape compatible. Installed
clients using strict excess-property decoders receive no new fields unless they
explicitly request `payload.execution_page`.

```json
{
  "project_id": "the-owned-project",
  "thread_id": "the-registered-supervisor-UUID",
  "target_command_id": "the-native-submit-receipt-command-id",
  "execution_page": {"limit": 25}
}
```

Only an opted-in result adds `execution_contract` (constant
`ctox.workjet.supervisor_execution.v1`) and `execution_page`:

- `command_id`, `task_id`: the verified admitted command and canonical queue link.
- `attempt`: absent before a native worker has recorded an attempt. Its
  `attempt_id` comes from the task's persisted harness flow ledger, never from a
  queue ordinal or desktop chat ID. `attempt_index` is the separately recorded
  ordinal. New native executions allocate an independent random `worker-run:`
  identity in `worker_run_identities` before invoking the executor. The row binds
  the exact attempt, work key, conversation, source and native task window.
  `run_id` is exposed only for a matching task; it survives process restart and
  remains the terminal `ctox_runs.id`. Admission and a historical start do not
  prove current worker liveness or completion. Finalization recovery
  does not allocate a second run. Historical attempts without this ledger retain
  their finalization key, and an unregistered active attempt has no `run_id`.
  `finished_at_ms` reads the finalization ledger's Unix-millisecond TEXT value
  as well as historical RFC3339 values. Event timestamps remain RFC3339 in Core;
  the wire exposes safe nonnegative millisecond integers without rewriting rows.
- `events`: at most 50 eligible safe events. IDs and insertion sequences are
  native ledger facts; raw thinking text, tool arguments, tool output and raw
  metadata are not returned. Titles are bounded to 256 characters.
- `next_cursor: {after_sequence, after_event_id}` and `has_more`: continue a
  selected attempt by supplying the exact cursor and `attempt_id`. A page
  without `attempt_id` selects the most recently recorded attempt. Keep the
  explicit attempt while paging; a cursor from another or expired attempt fails
  closed and must be reset. Native insertion order retains late/backdated events.

Example follow-up request addition:

```json
{"execution_page": {"attempt_id": "actual-attempt-from-the-receipt",
                    "cursor": {"after_sequence": 23, "after_event_id": "actual-event"},
                    "limit": 25}}
```

The Shell's `workjetProjectControl` bridge accepts the outer camelCase
`executionPage` only for `project.supervisor.turn.watch`. Inside that object,
`attempt_id`, `cursor.after_sequence`, `cursor.after_event_id`, and `limit` use
the shared fixture's native names. It translates to `payload.execution_page`
and adds outer `executionContract` / `executionPage` only for that explicit
request; the returned page retains the fixture's snake_case fields. The bridge
validates both directions and correlates the page command/task/attempt and
continuation cursor with the native receipt. Legacy callers still receive only
`action`, `commandId`, `projectId`, `contract`, `binding`, and `turn`.

This is a typed native control command transported over the existing authorized
Business OS WebRTC/RxDB command/result lane, not an HTTP business-data endpoint.
Current native owner identity, the registered supervisor, admitted envelope and
Core task link are checked before reading. Foreign actors, projects, commands,
attempts and cursor anchors do not become readable by supplying their IDs.

The reader opens an ordinary read-only Core snapshot. It initializes no tables,
requires no writer fence, and publishes no projection or task. Shared fixture
`workjet-supervisor-execution-v1.json` generates the native and browser consumers
together. The source regressions are not an installed goals 8/9/A0 acceptance.

## Public assistant text (explicit opt-in)

A caller sets `execution_page.include_public_text: true` to request actual public
assistant chunks. The page returns `public_text_supported` and events of kind
`worker.assistant_text`, with a bounded `public_text` object. A request without
that flag retains the original page shape and event whitelist. Older natives
reject the new request field; callers must retain their ordinary history read
and explicitly report unavailable public-text support. Non-Unix natives return
`public_text_supported: false`; they do not invent a transcript stream.

The producer consumes V2 assistant item start/delta/completion notifications
for the exact provider thread and turn, only in the verified Workjet supervisor
session. It excludes reasoning and tool items and filters private ctox-crew
blocks even when their delimiters span model tokens. Commentary stays labelled
as commentary; a completion marker is not a successful task review.

Publication borrows the existing live native provider/worker transaction. The
native lease, worker lifetime, retained provider witness, actual turn and store
are rechecked at each write. Stable chunk IDs reject conflicting replays. Write
failure interrupts that exact provider turn and fails the slice; it never falls
back to the lossy progress recorder or a UI-generated text animation. Chunks
remain private to this authorized reader, excluded from cockpit projection.

Offsets count Unicode characters per provider item. Each chunk has at most 4096
characters, each item 65536, and the turn 262144, with at most 64 assistant items.
The first text is published immediately and following tokens are coalesced for
100 ms or 128 characters. Item completion flushes pending text; explicit
`truncated`/`completed` markers preserve limits and item lifecycle. The ordinary
terminal result remains the authoritative full public reply. The existing
retained event cursor supports backfill after reader reopen; a removed anchor
still requires restarting that attempt's page. No provider transport replay
sequence or new execution authority is claimed.
