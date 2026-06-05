# CTOX IoT Engine — Phase 7 Hardening Release Notes (DRAFT)

Status: DRAFT — not tagged. Releases go through the GitHub Actions pipeline by
pushing a `vX.Y.Z` tag on `main`; this draft does **not** create a tag.

Date: 2026-06-05

## Scope

Phase 7 hardens the native CTOX IoT engine (`src/core/iot/`) against the
backpressure/bounds, multi-realm isolation, and secret-redaction audits, adds a
CI soak job for the engine, and ships operator documentation. No new external
dependencies; all native Rust on the single SQLite runtime store; no HTTP data
bridge.

## Backpressure / Bounds

- `datapoints::interval()` now returns an explicit `truncated` flag alongside the
  bounded buckets (matching `all`/`lttb`). The command layer surfaces
  `truncated=true` in the `iot_datapoints` window projection instead of inferring
  it from the row count. Bucket-window clamping to `DEFAULT_QUERY_LIMIT`
  (100,000) is still logged.
- `projector::project_asset()` and `commands::asset_upsert()` cap attribute
  fan-out at `MAX_ATTRIBUTES_PER_ASSET = 1000` and log on overflow, so an asset
  that accumulates a runaway event-driven attribute set cannot fan out unbounded
  projection rows into RxDB.
- Telemetry remains coalesced: one last-value attribute row + one asset summary
  per inbound event; datapoints stay append-only (one per applied reading).

## Multi-Realm Isolation

- Every read / write / projection in `commands.rs` is now scoped to the realm
  derived from the `BusinessOsSession` (`session_realm`); the realm is **never**
  trusted from the client payload.
- New realm-scoped store/alarm getters enforce the realm at the SQL layer:
  `store::get_asset_in_realm`, `alarms::get_in_realm`,
  `alarms::update_status_in_realm`, `alarms::assign_in_realm`.
- Affected ops: `asset_show`, `asset_upsert`, `asset_delete`, `attribute_read`,
  `attribute_write`, `datapoints_query`, `alarm_update`, `ruleset_save`,
  `ruleset_toggle`, `agent_configure`. On create the session realm overrides the
  payload realm; on read/update/delete a cross-realm id resolves to "not found"
  (no existence leak).
- The trusted `ctox iot …` CLI passes `realm = None` (unscoped), consistent with
  its existing full-host operator posture.
- Fine-grained per-realm ACL is DEFERRED to the Phase 3 rules engine; Phase 2
  hard-enforces basic `resource.realm == session_realm` isolation today.

## Secret Redaction

- Custom `Debug` impls redact credentials in `ConnectParams` (MQTT
  username/password), `HttpAgentConfig` (auth-header key name + header values),
  and `WsConfig` (auth-header key name + verbatim connect frames).
- WebSocket connect-failure logging runs through a `redact_auth` sanitizer.
- The `iot_agent_status.error` column carries a redaction contract comment and is
  never written from a connect/transport failure path.
- Secrets stay in the secret-store allowlist and resolve via
  `runtime_env::env_or_config` — never `std::env` for runtime state.

## CI Soak

- New workflow `.github/workflows/iot-soak.yml` (`workflow_dispatch`, inputs:
  `cycles`, `events_per_cycle`, `assets`, `fail_on_retry`) builds `ctox`, runs the
  four soak tests single-threaded, runs the IoT module + app-store +
  RxDB-only contract checks, and uploads `iot-soak-summary.json`.
- New soak harness `src/core/iot/tests/ci_soak.rs` reuses a native loopback MQTT
  3.1.1 broker fixture (no external broker), an injected clock, and the runtime
  supervisor:
  - `iot_ci_soak_exit_multi_cycle_round_trip`
  - `iot_ci_soak_with_forced_disconnect_and_reconnect`
  - `iot_ci_soak_attribute_write_command_round_trip`
  - `iot_ci_soak_multi_realm_projection_isolation` — proves the realm-scoped
    projection/sync surface (`projector::project_all_in_realm`) never leaks
    another realm's rows into the RxDB-visible `business_records` that WebRTC
    replicates; the trusted operator resync (`None`) still mirrors every realm.
- `fail_on_retry` is wired end-to-end: `IOT_SOAK_FAIL_ON_RETRY` (default `true`)
  is consumed by the harness, which fails the steady-state inbound and command
  soaks if any pump needed a retry (a zero-progress re-step AFTER the batch
  began flowing on the established connection). The forced-reconnect soak is
  exempt because elapsing the §2A.24 backoff window is expected re-stepping. The
  summary node surfaces the flag and any tolerated-retry notes. (Mirrors the
  rxdb soak's `SOAK_FAIL_ON_RETRY` → `failOnRetry` gate.)
- Realm isolation is now enforced on the PROJECTION/SYNC surface, not only on
  the write/command/condition paths: `projector::project_all_in_realm(conn,
  Some(realm))` filters every realm-bearing scan (assets+attributes, alarms,
  realms, rulesets, agents+status) with `WHERE realm = ?1`; asset types stay
  global. `business_os::store::project_all_iot(root, realm)` threads the scope
  through; the `ctox iot project all` CLI passes `None` (trusted operator).
- Runnable locally:
  `cargo test --bin ctox iot_ci_soak_ -- --test-threads=1 --nocapture`.

## Documentation

- `docs/iot-operations.md` — operator guide (architecture, CLI, invariants,
  verification, deferred scope).
- Module `description` / `store.summary` in
  `src/apps/business-os/modules/iot/module.json` and
  `src/apps/business-os/modules/registry.json` now label the §2.4 deferred scope
  (Groovy/JS/Flow rules, non-MQTT protocol agents, forecasting, gateway
  federation) as DEFERRED — planned, not broken.

## Test Status

- `cargo check --bin ctox` — clean (pre-existing warnings only).
- `cargo test --bin ctox iot::` — all IoT tests green (139), including the new
  interval-truncation, realm-isolation (write + projection), and
  secret-redaction tests.
- `cargo test --bin ctox iot_ci_soak_` — 4 soak tests green (also green with
  `IOT_SOAK_FAIL_ON_RETRY=true`).
- `node iot.test.mjs`, `node --test app-store.test.mjs`,
  `node assert-rxdb-only.mjs` — green.
- `cargo run --bin ctox -- process-mining spawn-liveness` — exit 0.

## Deferred (planned, not broken)

Groovy/JS/Flow rule execution, HTTP/WebSocket protocol agents beyond MQTT,
forecasting/prediction, and gateway federation. See `docs/iot-operations.md`
§Deferred.
