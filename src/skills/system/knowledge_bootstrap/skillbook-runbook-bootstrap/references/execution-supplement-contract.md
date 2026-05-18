# Execution Supplement Contract

Execution supplements are the explicit bridge from `desk-only history candidates` to
`promotion-ready runbook items`.

Ticket history may suggest recurring families, but it must not invent:

- `tool_actions`
- `verification`
- `writeback_policy`

Those fields must come from explicit execution-capable sources such as:

- manuals
- runbooks
- validated operator procedures
- successful reviewed CTOX executions

## V1 Intention

One supplement line enriches exactly one labeled candidate item.

The supplement unit is JSONL, one object per line.

## Required Fields

- `label`
- `execution_guidance`
- `tool_actions`
- `verification`
- `writeback_policy`
- `sources`

## Optional Fields

- `title`
- `entry_conditions`
- `earliest_blocker`
- `escalate_when`
- `pages`
- `trigger_phrases`
- `promote`
- `source_type`
- `notes`

## Meaning

### `label`

Must match an existing candidate runbook item label exactly.

### `execution_guidance`

The bounded execution detail that should be merged into the item's existing
`expected_guidance`.

This is not free prose. It must stay scoped to the labeled item.

### `tool_actions`

The explicit tool-level actions needed for this item.

### `verification`

The concrete success checks for the execution path.

### `writeback_policy`

The allowed writeback target and default mode.

### `sources`

The explicit execution evidence behind this supplement. These will be merged into
the item's source list and must be specific enough to audit later.

## Example

```json
{
  "label": "HIST-011",
  "execution_guidance": "Prüfe zuerst den aktuellen Monitoring-Status des Alerts. Wenn der Alert noch aktiv ist, dokumentiere den aktiven Zustand und eskaliere an Infrastructure Ops. Wenn der Alert nicht mehr aktiv ist, prüfe, ob ein frisches grünes Signal vorliegt, bevor Entwarnung notiert wird.",
  "tool_actions": [
    {
      "tool": "monitoring.check_service_alert",
      "mode": "read_only",
      "target": "prestige_printengine"
    },
    {
      "tool": "ticket.source-skill-review-note",
      "mode": "desk_execution_boundary",
      "target": "zammad"
    }
  ],
  "verification": [
    "Current alert state is visible from the monitoring system.",
    "A green or recovered signal is present before any Entwarnung is written."
  ],
  "writeback_policy": {
    "channel": "internal_note",
    "default_mode": "suggestion"
  },
  "escalate_when": [
    "monitoring state cannot be checked",
    "alert remains active after the check"
  ],
  "sources": [
    {
      "title": "Infrastructure monitoring playbook",
      "path": "manual://infrastructure-monitoring"
    }
  ],
  "promote": true
}
```

## Promotion Rule

An enriched item may be marked promotion-ready only if:

- the supplement matches an existing label
- `tool_actions` is non-empty
- `verification` is non-empty
- `writeback_policy` is explicit
- `sources` is non-empty
- the merged item still has deterministic `chunk_text`

If any of these fail, the item stays `candidate` and the unresolved gap remains open.
