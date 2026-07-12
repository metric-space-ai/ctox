# CTOX Sync Engine Production Readiness Runbooks

Status: active operator contract
Last reviewed: 2026-07-11

Scope: CTOX Sync Engine, browser IndexedDB/RxDB working copy, native SQLite
authority, WebRTC signaling/TURN, Command Bus, runtime app actions, Sagas,
recovery journal, native backup and Business OS MCP access.

This file is a production-readiness contract, not a narrative troubleshooting
note. A release can claim CTOX Sync Engine 9.5/10 only when these runbooks exist
and their exercises are represented by
`runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json`.

Required exercise evidence schema:

```json
{
  "schema": "ctox.sync.production_readiness_95.runbook_exercises.v1",
  "ok": true,
  "age_days": 30,
  "exercised_runbooks": [
    "signaling_turn_outage",
    "webrtc_backpressure_stall"
  ],
  "open_followups": 0,
  "source": {
    "commit": "<current release commit>",
    "dirty": false,
    "artifactHashes": {
      "browserBundleSha256": "<sha256>",
      "smokeBinarySha256": "<sha256>"
    }
  },
  "attempts": {
    "requested": 1,
    "accepted": 1,
    "retries": 0
  }
}
```

The real artifact must include every runbook ID listed below, must be generated
from the candidate release line, and must have no unresolved P0/P1 follow-ups.
Use
`node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js`
to inspect the starter template, but do not submit a `template: true` artifact as
evidence. The 9.5 audit rejects templates explicitly.

## Common rules

These rules apply to every incident below.

- Symptoms: record the exact user-visible behavior and the first degraded
  status field.
- Status fields: collect `ctox business-os rxdb status --json`, advanced
  browser sync status and the relevant release/canary artifact ID.
- Immediate containment: prevent additional data loss, duplicate effects or
  unauthorized access before attempting full recovery.
- Recovery steps: restore service using only supported runtime state, typed
  config, persisted recovery metadata, validated packages and supervised peer
  restarts.
- Prohibited actions: do not use HTTP as a data fallback, do not delete pending
  journal/conflict/Saga state, do not edit generated contracts by hand, do not
  manually mutate SQLite/RxDB rows outside an approved repair procedure, and do
  not classify retry-based evidence as green.
- Verification after recovery: prove no lost confirmed writes, no duplicate
  effects, no unresolved terminal state, and matching commit/hash evidence.
- Exercise evidence: every drill records operator, commit, dirty flag, artifact
  hashes, start/end timestamps, injected fault, observed status fields, recovery
  commands, verification result and follow-ups.

## Runbook index

| ID | Incident | Minimum drill cadence |
| --- | --- | --- |
| `signaling_turn_outage` | Signaling/TURN outage | Quarterly and before WAN rollout |
| `webrtc_backpressure_stall` | WebRTC backpressure stall | Quarterly |
| `journal_growth_replay` | Journal growth and replay | Monthly |
| `quota_exhaustion` | Browser quota exhaustion | Quarterly |
| `blocked_indexeddb_primary` | Blocked IndexedDB primary | Quarterly |
| `browser_origin_loss` | Browser-origin loss | Quarterly |
| `native_sqlite_restore` | Native SQLite restore | Weekly automated, monthly manual |
| `schema_migration_block` | Schema migration block | Before every migration release |
| `saga_compensation_failure` | Saga compensation failure | Quarterly and before workflow rollout |
| `conflict_flood` | Conflict flood | Quarterly |
| `app_package_revocation` | App package revocation | Before app-runtime rollout |
| `key_revocation` | Key revocation | Quarterly |
| `mcp_access_incident` | MCP access incident | Quarterly |

## `signaling_turn_outage` — Signaling/TURN outage

Symptoms:

- Browser status shows connection loss, repeated room join failures or stale
  `replicationUp=false`.
- WAN-only users fail to reconnect while LAN users may still converge.
- `productionReadiness.transport.circuitBreakerState` is `open` or `half_open`.

Status fields:

- `sync.signaling_urls_source`
- `productionReadiness.transport.circuitBreakerState`
- selected ICE candidate type, TURN credential expiry, reconnect p95
- signaling terminal error code and next probe timestamp

Immediate containment:

- Stop rollout promotion for affected cohorts.
- Freeze app package activation and schema migrations until room health is
  restored.
- Keep local writes enabled only if the browser journal is available and export
  warnings are visible.

Recovery steps:

1. Confirm whether the failure is terminal auth/revocation/protocol mismatch or
   retryable network/TURN availability.
2. Rotate typed signaling/TURN runtime config only when credentials or endpoint
   validity is the root cause.
3. Let the supervised peer respawn through the circuit breaker; do not force a
   reconnect hammer.
4. Verify one half-open probe succeeds, DataChannel opens and pending journal
   batches drain.

Prohibited actions:

- Do not add HTTP data fallback.
- Do not disable terminal signaling classification.
- Do not clear room state by deleting local IndexedDB data.

Verification after recovery:

- Reconnect p95 is at most 60 seconds for the affected profile.
- No journaled write was lost.
- Release/canary evidence records zero retry-based acceptance.

Exercise evidence:

- Include injected signaling outage, TURN-only outage, terminal-auth case and
  successful recovery artifact.
- Attach the
  `ctox.sync.production_readiness_95.wan_turn_external_measurements.v1`
  measurement input used by
  `run_sync_production_readiness_95_wan_turn_matrix.js`; simulated TURN
  evidence is not acceptable for release qualification.
- Record the exercise in
  `ctox.sync.production_readiness_95.runbook_exercise_log.v1` for
  `run_sync_production_readiness_95_runbook_exercises.js`, including date,
  outcome, evidence URI or hash and closed follow-ups.

## `webrtc_backpressure_stall` — WebRTC backpressure stall

Symptoms:

- Send buffer stays over capacity for 30 seconds.
- `ctox_webrtc_send_buffer_stalled` appears in transport and advanced status.
- Queue waiters are rejected and peer/DataChannel closes.

Status fields:

- backpressure stall count, rejected frame count, buffered amount
- peer close reason, queue length by priority, DataChannel state
- newest successful payload timestamp

Immediate containment:

- Stop large materialization and file-viewer demand loads for the affected peer.
- Keep control frames limited to the recovery lane.

Recovery steps:

1. Confirm the stalled peer closed and no payload was sent after timeout.
2. Wait for supervised peer recreation.
3. Resume push from journal/checkpoint.
4. Inspect high/normal/low scheduler fairness after recovery.

Prohibited actions:

- Do not raise send-buffer limits without new capacity evidence.
- Do not drop pending journal batches to unblock the queue.

Verification after recovery:

- Queue waiter count returns to zero.
- Pending journal count monotonically drains.
- No duplicate document or command effect is observed.

Exercise evidence:

- Include forced high send-buffer test and post-close no-send assertion.

## `journal_growth_replay` — Journal growth and replay

Symptoms:

- Journal pending count/bytes grow or oldest write age exceeds SLO.
- Primary data exists but master ack is missing.
- Startup replay reports conflicts or schema mismatch.

Status fields:

- journal pending count, pending bytes, oldest write age
- last replay, last export, last GC
- conflict-store entries and schema hash

Immediate containment:

- Warn the user to export recovery data before destructive browser actions.
- Pause primary reset until pending writes are acknowledged or exported.

Recovery steps:

1. Run startup replay before sync start.
2. Reconcile journal present/primary missing and primary present/journal pending
   paths idempotently.
3. Move schema/instance mismatches to conflicts; never discard them.
4. After native ack, mark batches `master_acked` and allow GC only after the
   retention window.

Prohibited actions:

- Do not compact or delete pending journal batches.
- Do not treat conflicts as successful replay.

Verification after recovery:

- Pending count is stable or decreasing.
- Every confirmed local write has either master ack or durable conflict entry.

Exercise evidence:

- Include crash-before/after journal, primary write and master ack cases.

## `quota_exhaustion` — Browser quota exhaustion

Symptoms:

- IndexedDB write fails with quota exceeded.
- Storage pressure is at or above warning threshold.
- Demand materialization or journal append fails.

Status fields:

- quota estimate, pressure ratio, persistent-storage state
- failing operation class: journal append, primary write, materialization
- `indexeddb_quota_exceeded` or `indexeddb_journal_unavailable`

Immediate containment:

- Do not confirm the business write if the journal cannot be written.
- Evict only replicated cache rows through the DB-wide coordinator.

Recovery steps:

1. Run safe global cache eviction.
2. Refresh storage estimate.
3. Retry the failed write exactly once.
4. Surface typed error if the second attempt fails.

Prohibited actions:

- Do not evict pushable writes, journal batches or unresolved conflicts.
- Do not loop quota retries.

Verification after recovery:

- Journal append durability is preserved.
- User-visible status explains remaining storage pressure.

Exercise evidence:

- Include quota during journal, primary write and demand materialization.

## `blocked_indexeddb_primary` — Blocked IndexedDB primary

Symptoms:

- Primary open/delete is blocked by another tab or old process.
- Recovery journal is accessible but main DB cannot be opened.
- App startup stalls before sync start.

Status fields:

- primary-open state, blocker age, leader lease state
- recovery DB open result, pending journal count
- last `retryPrimaryOpen` result

Immediate containment:

- Keep the recovery warning visible.
- Do not delete the primary database while pending writes exist unless a recovery
  export has been created.

Recovery steps:

1. Ask old tabs to close via BroadcastChannel when possible.
2. Release leader lease on `freeze`/`pagehide`.
3. Retry primary open explicitly through the recovery facade.
4. If reset is required, export first, reopen primary, replay journal and start
   WebRTC catch-up.

Prohibited actions:

- Do not silently reset the primary DB.
- Do not start sync before replay has run.

Verification after recovery:

- Primary opens, replay completes and peer catches up without full data loss.

Exercise evidence:

- Include old-tab blocker, reset-with-export and denied-reset cases.

## `browser_origin_loss` — Browser-origin loss

Symptoms:

- Complete IndexedDB/localStorage/service-worker state is gone.
- Recovery journal is gone.
- User expects offline writes that only existed in origin storage.

Status fields:

- persistent-storage status before loss if available
- newest encrypted recovery export timestamp
- import preview result, schema hashes, instance ID

Immediate containment:

- State the boundary plainly: complete origin deletion can only recover to the
  previous encrypted export.
- Stop accepting assumptions that vanished local-only writes still exist.

Recovery steps:

1. Open recovery import preview with passphrase.
2. Verify integrity, instance ID and schema hashes.
3. Apply import only after preview confirmation.
4. Start WebRTC catch-up and resolve imported conflicts.

Prohibited actions:

- Do not claim RPO 0 for unexported data after full origin deletion.
- Do not allow instance remapping in v2.

Verification after recovery:

- Imported pending batches are replayed or become durable conflicts.
- UI shows any unrecoverable boundary explicitly.

Exercise evidence:

- Include valid export, wrong password, tampered artifact and instance mismatch.

## `native_sqlite_restore` — Native SQLite restore

Symptoms:

- Native authority is corrupted, missing or rolled back.
- Browser peers cannot confirm pushes.
- Restore drill age exceeds seven days.

Status fields:

- backup freshness, restore drill timestamp, snapshot manifest hash
- native DB hash, schema/runtime hash, release commit
- replication catch-up and projection backlog

Immediate containment:

- Quiesce native writes and command admission.
- Preserve damaged files for forensic analysis.

Recovery steps:

1. Select newest encrypted off-host snapshot matching the release evidence set.
2. Verify signature, manifest, DB hash and schema/runtime hash.
3. Restore to a clean location.
4. Start native peer, run projections, then let browser peers catch up.
5. Compare post-restore state against canary and journal evidence.

Prohibited actions:

- Do not restore unsigned or mismatched snapshots.
- Do not skip projection/outbox reconciliation.

Verification after recovery:

- RPO is at most 15 minutes and RTO is at most 60 minutes.
- Restore drill artifact is fresh and linked to the commit.

Exercise evidence:

- Include automated weekly and manual monthly restore results.

## `schema_migration_block` — Schema migration block

Symptoms:

- Runtime package activation stops during Stage–Validate–Activate–Reconcile.
- Browser collection registration fails or schema hash mismatch appears.
- Existing app stays on old runtime hash.

Status fields:

- package runtime hash, migration manifest hash, schema hash
- activation phase, rollback target, affected collections
- compatibility capability and app package signature status

Immediate containment:

- Keep the previous package active.
- Block destructive migration rollout for all cohorts.

Recovery steps:

1. Validate manifest, schema, actions and migrations offline.
2. Confirm allowed v1 migration operation set only.
3. Roll back to prior package if activation did not reach healthy peer and
   visible collections.
4. Reconcile pending commands against definition snapshots.

Prohibited actions:

- Do not edit Rust code or rebuild backend for app schema changes.
- Do not run free JavaScript/SQL migrations.

Verification after recovery:

- Old package remains functional or new package activates with matching runtime
  hash and healthy peer.

Exercise evidence:

- Include invalid migration, rollback and successful compatible migration.

## `saga_compensation_failure` — Saga compensation failure

Symptoms:

- Saga enters `manual_intervention`.
- A compensation step failed or previous document evidence is incomplete.
- Lifecycle shows pending consistency longer than SLO.

Status fields:

- `saga_id`, `saga_phase`, `saga_step`, `saga_total_steps`
- `compensation_status`, failed effect key, previous document evidence hash
- command status and projection backlog

Immediate containment:

- Stop terminal success for the command.
- Block dependent business actions that would rely on the inconsistent state.

Recovery steps:

1. Inspect immutable action-definition snapshot.
2. Inspect forward and compensation effect keys.
3. Retry idempotent compensation if evidence is complete.
4. If retry cannot prove correctness, keep `manual_intervention` and follow the
   approved repair/audit procedure.

Prohibited actions:

- Do not mark a Saga successful without completed forward steps.
- Do not hide failed compensation from modules.

Verification after recovery:

- Saga is either fully compensated, successfully completed or durably visible as
  manual intervention with audit trail.

Exercise evidence:

- Include crash before/after each forward and compensation effect.

## `conflict_flood` — Conflict flood

Symptoms:

- Unresolved conflict count rises quickly.
- Strong future HLC or delete-vs-update conflicts appear.
- Users see stale or inconsistent query windows.

Status fields:

- conflict count and oldest age
- conflict types: structured, clock skew, delete-vs-update
- query-window invalidation count and clock-skew alert

Immediate containment:

- Warn affected users and disable auto-resolution for structured conflicts.
- Stop rollout if conflict rate began with a new package/schema.

Recovery steps:

1. Group conflicts by collection, schema hash and actor/device.
2. Resolve simple cases with `keep_master`, `keep_local` or `restore_as_copy`.
3. Preserve master tombstones as authoritative while keeping local copies
   recoverable.
4. Correct clock skew before accepting new local writes from affected clients.

Prohibited actions:

- Do not let future HLC automatically win a real conflict.
- Do not discard base/local/master evidence.

Verification after recovery:

- Unresolved count returns below alert threshold and every resolution is audited.

Exercise evidence:

- Include structured conflict, delete-vs-update and large clock skew.

## `app_package_revocation` — App package revocation

Symptoms:

- Package publisher key or app version is revoked.
- Runtime hash is no longer trusted.
- App activation or action admission returns typed denial.

Status fields:

- package signature, publisher trust state, revocation epoch
- active package hash, rollback package hash, affected actions
- `app_action_permission_denied` or package activation error

Immediate containment:

- Stop new activations and new action admissions for the revoked package.
- Preserve existing Saga snapshots for already-admitted commands.

Recovery steps:

1. Mark package as revoked through the trusted policy path.
2. Roll back to the last non-revoked active package.
3. Reconcile browser collection registration.
4. Review running Sagas against immutable snapshots and grants.

Prohibited actions:

- Do not delete command/Saga evidence for revoked packages.
- Do not bypass revocation by manually editing local manifests.

Verification after recovery:

- New actions from revoked package are denied; existing durable commands are
  completed or made visible according to policy.

Exercise evidence:

- Include active package revocation and rollback.

## `key_revocation` — Key revocation

Symptoms:

- Recovery export key, package signing key or room credential is compromised.
- Authentication succeeds unexpectedly or valid clients are denied after epoch
  change.

Status fields:

- credential epoch, revocation list version, key identifier
- circuit breaker state, package trust state, export/import error
- audit events for grant/credential changes

Immediate containment:

- Revoke the affected key via typed policy/config.
- Pause promotion and app package activation for impacted cohorts.

Recovery steps:

1. Rotate credentials through the approved key-management path.
2. Force package/signaling validation against the new epoch.
3. Require new encrypted recovery exports where export keys changed.
4. Verify denied access for the revoked credential.

Prohibited actions:

- Do not store recovery passphrases.
- Do not accept stale package signatures after revocation.

Verification after recovery:

- Revoked credentials fail, new credentials pass, and audit logs identify the
  change owner.

Exercise evidence:

- Include signing-key revocation, room credential rotation and export-key drill.

## `mcp_access_incident` — MCP access incident

Symptoms:

- External agent attempts or performs unauthorized Business OS access.
- MCP scope differs from native policy decision.
- Audit redaction or source visibility boundary is violated.

Status fields:

- actor, module, record scope, grant decision and audit reason
- MCP tool/action ID, command ID, policy decision hash
- security/privacy signoff control status

Immediate containment:

- Revoke or suspend the affected MCP grant.
- Stop external-agent actions for impacted scope until audit is complete.

Recovery steps:

1. Compare MCP decision with `src/core/business_os/policy.rs` outcome.
2. Confirm no browser HTTP data fallback or policy bypass was used.
3. Reconcile audit entries and redact exported evidence as required.
4. Re-enable grants only through the approved policy/approval path.

Prohibited actions:

- Do not mutate database tables directly to repair grants.
- Do not treat browser helper permissions as source of truth.

Verification after recovery:

- Unauthorized read/write count is zero after revocation and all attempted
  access is visible in audit evidence.

Exercise evidence:

- Include denied read, denied write, grant revocation and audit export check.
