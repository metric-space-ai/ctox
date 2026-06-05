# CTOX IoT Engine — Operations Guide

This is the operator guide for the native CTOX IoT engine under
`src/core/iot/`. It covers what the engine does today, how to run and verify it,
the hard invariants it enforces (multi-realm isolation, bounded/coalesced
telemetry, secret redaction), and which capabilities are deliberately deferred.

The IoT engine is a CTOX-native subsystem. Domain semantics are ported from
OpenRemote (AGPL-3.0); persistence and transport are reimplemented on CTOX's
single SQLite runtime store. It is **not** a wrapper around an external IoT
platform and it does **not** run a second automation engine — alarms and work
are routed into CTOX's own mission brain.

## Architecture at a Glance

```text
device (MQTT broker)
  -> protocol agent (src/core/iot/agents/, native MQTT 3.1.1 codec)
  -> runtime supervisor pump (src/core/iot/runtime.rs, 100ms PUMP_TICK)
  -> §2A.28 inbound base layer (filters/converters/coerce)
  -> store::process_attribute_event  [records the datapoint] -> runtime/ctox.sqlite3
  -> conditions::evaluate_and_emit   [alarms + bounded mission tasks]
  -> projector  -> business_records (iot_* collections)
  -> RxDB/WebRTC  -> Browser Business OS "IoT" module (read-only view)
```

Every layer reads and writes the **single** core store
`runtime/ctox.sqlite3` via `crate::paths::core_db(root)`. There is **no HTTP
data bridge**: the browser module reads the `iot_*` projections over RxDB/WebRTC
and mutates only through `business_commands`.

### Key files

| Path | Purpose |
|------|---------|
| `src/core/iot/store.rs` | Asset/attribute write path (§2A.1-8), per-asset write serialization, realm-scoped getters |
| `src/core/iot/datapoints.rs` | Time-series store + queries (all / interval+LOCF / LTTB / nearest), bounded with logged truncation |
| `src/core/iot/alarms.rs` | Alarm lifecycle, realm-scoped getters and transitions |
| `src/core/iot/conditions.rs` | Ruleset evaluation, startup suppression, bounded re-firing |
| `src/core/iot/projector.rs` | `iot_*` → `business_records` projection rows (bounded fan-out) |
| `src/core/iot/commands.rs` | CLI surface (`ctox iot …`) + `business_commands` executor, realm enforcement |
| `src/core/iot/agents/` | Native protocol agents (MQTT shipping; HTTP/WS bring-up) |
| `src/core/iot/runtime.rs` | Supervisor pump + per-step inbound/outbound wiring |

## Data Model and Collections

The engine projects nine read-only collections into Business OS over RxDB:

`iot_realms`, `iot_asset_types`, `iot_assets`, `iot_attributes`,
`iot_datapoints`, `iot_alarms`, `iot_agents`, `iot_agent_status`,
`iot_rulesets`.

Time model (do not mix the two):

- datapoint / attribute / alarm domain time is `i64` epoch-ms UTC (§2A.13).
- CRUD audit columns (`created_at` / `updated_at`) are RFC-3339 millis-precision
  UTC TEXT.

## Operating the Engine

### CLI surface (trusted operator)

The `ctox iot …` CLI is the trusted operator surface running with full host
access. It bypasses session realm scoping (realm enforcement applies to the
`business_commands` executor path — see Multi-Realm Isolation below).

```sh
# Register an asset type + asset
ctox iot asset upsert --realm master --type Thermostat --name "Living room" \
  --type-info '{"asset_type":"Thermostat","attributes":[{"name":"temp", ...}]}'

# Inspect
ctox iot asset list   --realm master
ctox iot asset show   --id <asset-id>

# Write a device-side value through the §2A write path (records a datapoint)
ctox iot attribute write --asset <asset-id> --name temp --value 22.5 --ts 0
ctox iot attribute read  --asset <asset-id> --name temp

# Query datapoints (bounded windows)
ctox iot datapoints query --asset <id> --name temp --from 0 --to 9999999999999 --shape all
ctox iot datapoints query --asset <id> --name temp --from 0 --to ... --shape interval --interval 60000
ctox iot datapoints query --asset <id> --name temp --from 0 --to ... --shape lttb --threshold 500

# Alarms
ctox iot alarm list --realm master
ctox iot alarm ack     --id <alarm-id>
ctox iot alarm resolve --id <alarm-id>

# Rulesets and supervised protocol agents
ctox iot rules save   --realm master --name "High temp" --data '{"when":"temp > 30"}'
ctox iot agent configure --realm master --name "Broker" --kind mqtt --data \
  '{"host":"localhost","links":[{"assetId":"<asset-id>","attributeName":"temp","subscriptionName":"telemetry/temp"}]}'

# Full idempotent resync of every projectable engine row into business_records
ctox iot project all
```

### Browser module (Business OS)

The "IoT" module reads the `iot_*` projections over RxDB. Mutations and control
actions go through `business_commands` (`ctox.iot.*` command types), which the
executor in `commands.rs::handle_business_command` applies with realm
enforcement and then projects back over RxDB.

## Hard Invariants (Phase 7 Hardening)

### 1. Bounded, coalesced telemetry (backpressure)

High-frequency telemetry is coalesced so the firehose never fans out into RxDB:

- The supervisor drains device readings on a 100ms `PUMP_TICK`; each inbound
  event produces exactly the bounded projection set (the coalesced **last-value**
  attribute row plus the asset summary), not one row per sample.
- Datapoints are append-only history; **one** datapoint per applied reading.
- All datapoint query shapes apply a hard limit `DEFAULT_QUERY_LIMIT = 100_000`
  and **log on truncation** (never silent). The `interval` shape returns an
  explicit `truncated` flag to the caller (matching `all`/`lttb`); the command
  layer surfaces `truncated=true` in the window projection.
- Attribute projection fan-out per asset is capped at
  `MAX_ATTRIBUTES_PER_ASSET = 1000` (logged on overflow). Real assets carry far
  fewer; this bounds a runaway event-driven attribute set.

### 2. Multi-realm isolation

Every read / write / projection is scoped to the realm the **caller** is
authorized for. The realm is derived from the `BusinessOsSession`
(`session_realm`) — it is **never** trusted from the client payload:

- On create (`asset.upsert`, `ruleset.save`, `agent.configure`) the session
  realm **overrides** any `realm` field in the payload.
- On read / update / delete (`asset.show`, `attribute.read`/`write`,
  `datapoints.query`, `asset.delete`, `alarm.update`, `ruleset.toggle`) the
  resource must belong to the session realm or the op fails with "not found"
  (no cross-realm existence leak).
- Store/alarm getters enforce the realm at the SQL layer
  (`store::get_asset_in_realm`, `alarms::get_in_realm`).
- The PROJECTION/SYNC surface is realm-scopeable too:
  `projector::project_all_in_realm(conn, Some(realm))` filters every
  realm-bearing scan with `WHERE realm = ?1` (asset types are global), and
  `store::project_all_iot(root, realm)` threads that scope into the RxDB-visible
  `business_records` that WebRTC replicates — so a scoped resync never leaks
  another realm's rows to a paired peer. The
  `iot_ci_soak_multi_realm_projection_isolation` soak proves this.
- The trusted `ctox iot …` CLI passes `realm = None` (unscoped) for both the
  shared ops and `project all`, the same way it already bypasses the session
  ACL gate.

Fine-grained per-realm ACL (multiple authorized realms per user, role scoping)
is **DEFERRED** to the Phase 3 rules engine. Phase 2 still hard-enforces basic
`resource.realm == session_realm` isolation so no command can read or mutate
across realms.

### 3. Secret redaction

No plaintext credentials reach logs, the `iot_agent_status.error` column, error
messages, or a support bundle:

- Agent config structs that hold (or reference) credentials
  (`ConnectParams`, `HttpAgentConfig`, `WsConfig`) have **custom `Debug`** impls
  that redact usernames, passwords, secret-store key names, header values, and
  verbatim connect frames. Only non-secret topology (host/port/url, header
  names, presence) is formatted.
- The WebSocket connect-failure log runs through `redact_auth` to strip any
  `authorization:` material defensively.
- The `iot_agent_status.error` column is intentionally never written from a
  connect/transport failure path; agents absorb connect errors. If a future
  phase records a status error it MUST sanitize the message first (see the
  schema comment in `commands.rs::ensure_stub_schema`).
- Secrets are registered in the secret-store allowlist
  (`CTO_IOT_MQTT_PASSWORD`, `CTO_IOT_HTTP_AUTH_HEADER`, `CTO_IOT_WS_AUTH_HEADER`)
  and resolved via `runtime_env::env_or_config` + the encrypted store — never
  `std::env` for runtime state. `CTO_IOT_MQTT_USERNAME` is intentionally **not**
  a secret (MQTT usernames are public identifiers); a deployment that treats it
  as sensitive must add it to a local secret-key override.

## Verification

### Local

```sh
cargo check --bin ctox
cargo test  --bin ctox iot::

# IoT soak harness (loopback MQTT fixture, no external broker)
cargo test --bin ctox iot_ci_soak_ -- --test-threads=1 --nocapture

# Tune the soak with CI-style parameters (test parameters, not runtime state)
IOT_SOAK_CYCLES=5 IOT_SOAK_EVENTS_PER_CYCLE=4 IOT_SOAK_ASSETS=3 \
  cargo test --bin ctox iot_ci_soak_ -- --test-threads=1 --nocapture

# Module / contract checks
node src/apps/business-os/modules/iot/iot.test.mjs
node --test src/apps/business-os/modules/app-store/app-store.test.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
```

### CI

`.github/workflows/iot-soak.yml` (`workflow_dispatch`) runs the four soak
tests single-threaded on `ubuntu-22.04`, then the module + RxDB-only contract
checks, and uploads `iot-soak-summary.json`. Inputs: `cycles`,
`events_per_cycle`, `assets`, `fail_on_retry`.

`fail_on_retry` (default `true`) is consumed by the harness via
`IOT_SOAK_FAIL_ON_RETRY`: the steady-state inbound and command soaks FAIL if any
pump needed a retry — a zero-progress re-step after the batch already began
flowing on the established connection. The connect/subscribe ramp and the
forced-reconnect backoff window are expected re-stepping and are NOT counted.

The soak harness lives in `src/core/iot/tests/ci_soak.rs` and proves:

1. `iot_ci_soak_exit_multi_cycle_round_trip` — N assets × M events/cycle through
   the loopback MQTT broker; bounded coalesced attribute rows + append-only
   datapoints after each cycle.
2. `iot_ci_soak_with_forced_disconnect_and_reconnect` — a forced socket drop at
   the 50% mark; the agent reconnects + resubscribes under the injected clock and
   post-reconnect events land.
3. `iot_ci_soak_attribute_write_command_round_trip` — `ctox.iot.attribute.write`
   business commands reach the engine and project, coalescing into one attribute
   row with one datapoint per write.
4. `iot_ci_soak_multi_realm_projection_isolation` — assets seeded in two realms;
   a realm-scoped projection (`project_all_in_realm(Some(realm))`) carries ONLY
   that realm's rows (the other realm's records never appear), while the trusted
   operator resync (`None`) sees both. This closes the projection/sync-side realm
   gap (the write/command paths were already scoped).

## Deferred (planned, not broken) {#deferred}

These are §2.4 scope items that are intentionally **deferred** to later phases.
They are not regressions and not broken — the engine surfaces them as records or
status, but the active behavior is not yet wired:

- **Rule execution languages** — Groovy / JavaScript / Flow rule evaluation.
  Phase 2 persists ruleset records and projects them; the JSON-condition path is
  evaluated, but the scripting languages are deferred.
- **Protocol agents beyond MQTT** — the MQTT agent is the shipping protocol
  agent. The HTTP and WebSocket agents are bring-up/transitional work; agent
  records and `iot_agent_status` exist, but only MQTT is exercised end-to-end.
- **Forecasting / prediction** — predictive datapoints and forecast attributes.
- **Gateway federation** — multi-instance IoT gateway federation / edge
  forwarding.

When a deferred capability lands, update this section, the module
`description`/`store.summary` in `src/apps/business-os/modules/iot/module.json`
and `src/apps/business-os/modules/registry.json`, and add soak coverage.

## Release Notes

See `docs/iot-engine-phase7-release-notes.md` for the Phase 7 hardening release
notes (bounds, realm isolation, secret redaction, CI soak).
