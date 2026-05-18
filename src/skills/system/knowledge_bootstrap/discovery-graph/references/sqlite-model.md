# Discovery Graph SQLite Model

The discovery skill works in two layers:

1. raw captures from real commands
2. agent-authored normalized graph facts

The bundled store helper creates and writes this schema.

Important:

- the SQLite schema is canonical
- the `graph.json` payload is agent-authored
- helper scripts may produce or assist with `graph.json`, but the agent owns the interpretation
- the same 5-table kernel is reused by multiple ops skills
- `discovery_run.skill_key` separates `discovery_graph` from other skills such as `reliability_ops`

## Tables

### `discovery_run`

One logical discovery sweep.

- `run_id`
- `skill_key`
- `scope_json`
- `started_at`
- `finished_at`
- `status` (`open`, `capturing`, `captured`, `normalized`, `failed`)
- `note`

### `discovery_capture`

One stored raw collector result.

- `capture_id`
- `run_id`
- `collector`
- `tool`
- `target`
- `command_json`
- `stdout_text`
- `stderr_text`
- `exit_code`
- `captured_at`

### `discovery_entity`

One normalized resource.

- `entity_id`
- `kind`
- `natural_key`
- `title`
- `attrs_json`
- `first_seen_at`
- `last_seen_at`
- `last_run_id`
- `is_active`

### `discovery_relation`

One normalized edge.

- `relation_id`
- `from_entity_id`
- `relation`
- `to_entity_id`
- `attrs_json`
- `first_seen_at`
- `last_seen_at`
- `last_run_id`
- `is_active`

### `discovery_evidence`

Link between capture and normalized fact.

- `evidence_id`
- `capture_id`
- `entity_id`
- `relation_id`
- `note`
- `created_at`

## `graph.json` Contract

`store-graph` expects this shape:

```json
{
  "run_id": "run-20260325-001",
  "status": "normalized",
  "full_sweep": true,
  "note": "Merged host, service, repo, and listener findings for the Ubuntu target.",
  "entities": [
    {
      "kind": "host",
      "natural_key": "host:example-host",
      "title": "example-host",
      "attrs": {
        "hostname": "example-host"
      }
    }
  ],
  "relations": [
    {
      "from": {
        "kind": "systemd_unit",
        "natural_key": "systemd_unit:nginx.service"
      },
      "relation": "runs_on",
      "to": {
        "kind": "host",
        "natural_key": "host:example-host"
      },
      "attrs": {}
    }
  ],
  "evidence": [
    {
      "capture_id": "capture-001",
      "entity": {
        "kind": "host",
        "natural_key": "host:example-host"
      },
      "note": "Derived from hostnamectl output."
    }
  ]
}
```

## Entity Kinds

Use only kinds that are defensible from discovery output, for example:

- `host`
- `systemd_unit`
- `process`
- `listener`
- `container`
- `image`
- `mount`
- `volume`
- `repo`
- `repo_file`
- `k8s_pod`
- `k8s_service`
- `coverage_gap`
- `timer`
- `journal_finding`

## Relation Names

Prefer concrete names:

- `runs_on`
- `listens_on`
- `managed_by`
- `defined_in`
- `mounts`
- `depends_on`
- `contains`
- `scheduled_by`
- `about`

## Rule

Raw capture is the source of truth.  
Normalized graph facts are downstream interpretation.

One discovery sweep should reuse one `run_id` from raw capture through normalized graph persistence.

If `full_sweep` is `true`, entities and relations not seen in that sweep are marked `is_active = 0`.
