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
  ordinal. `run_id` stays absent until that attempt has a canonical durable
  `worker_attempt_finalizations` row; its key is the source of `ctox_runs.id`.
  This does not invent a separate harness run UUID.
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

This is a typed native control command transported over the existing authorized
Business OS WebRTC/RxDB command/result lane, not an HTTP business-data endpoint.
Current native owner identity, the registered supervisor, admitted envelope and
Core task link are checked before reading. Foreign actors, projects, commands,
attempts and cursor anchors do not become readable by supplying their IDs.

The reader opens an ordinary read-only Core snapshot. It initializes no tables,
requires no writer fence, and publishes no projection or task. Shared fixture
`workjet-supervisor-execution-v1.json` generates the native and browser consumers
together. The source regressions are not an installed goals 8/9/A0 acceptance.
