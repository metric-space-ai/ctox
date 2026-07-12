# CTOX Sync Engine 9.5/10 Production Readiness Plan

Status: active readiness plan
Last reviewed: 2026-07-11

Scope: CTOX Sync Engine (`ctox-rxdb` browser and native), Business OS Command Bus,
runtime-installed app data, recovery, WebRTC signaling/TURN, native backups,
release gates and operator evidence.

This plan starts from the current hardening baseline in
`docs/ctox-sync-command-bus-hardening-plan.md`. It does not introduce an HTTP
data path, distributed browser transactions, global CRDT ordering or arbitrary
native app code. Browser IndexedDB stays the local working copy, native SQLite
stays authoritative, and Business OS data continues to move only over the
WebRTC/RxDB data plane.

## 1. Rating Target

9.5/10 production readiness means the engine is not only functionally hardened,
but operationally provable:

- confirmed journaled writes have RPO 0;
- no duplicate command, projection or Saga effects are accepted;
- LAN replication p95 is at most 2 seconds;
- WAN replication p95 is at most 5 seconds;
- reconnect after network, signaling or TURN failure p95 is at most 60 seconds;
- at least 99.9 percent of writes converge inside their SLO window;
- native off-host backup RPO is at most 15 minutes and RTO is at most 60 minutes;
- full browser-origin loss is recoverable only to the last encrypted recovery
  export, and the UI must state that boundary plainly;
- releases are blocked unless no-retry soak, restore drill, security/privacy
  signoff and artifact integrity evidence all match the release commit.

## 2. Release Evidence Gates

The existing RxDB soak release profile covers 33 required Sync/Command/App Runtime modes.
The Business OS production registry currently contributes 8 app/platform modes.
9.5 readiness adds the broader current full matrix as a qualification layer.
The matrix must contain at least 40 modes; at the time of this review it is 46
unique modes: 38 default Browser/Rust modes plus 8 Business OS production modes.
This keeps a green 33-mode soak from being misread as full production evidence.

Required gates:

- one clean full matrix on the exact candidate commit, with at least 40 unique
  modes covered;
- WAN/TURN matrix artifact:
  `runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json`;
- browser recovery matrix artifact:
  `runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json`;
- runtime app package gate artifact:
  `runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json`;
- release soak: 3 cycles x 33 required modes, no retry, clean worktree;
- nightly soak: 9 cycles x 33 required modes, no retry, clean worktree;
- 72 hour persistent canary with injected network, leader, quota, daemon restart,
  checkpoint, conflict and command/Saga faults;
- all artifacts include commit SHA, dirty flag, browser bundle hash, test binary
  hash, requested attempts and accepted attempts;
- failed Browser/Rust attempts retain bounded stdout/stderr tails in the matrix
  artifact so a no-retry failure remains diagnosable after runner cleanup;
- release blocks if any gate used a retry or if evidence comes from a different
  commit.

Operator templates for the custom artifacts are available through:

```bash
node src/core/rxdb/tools/print_sync_production_readiness_95_templates.js
```

The command prints templates by default and writes files only when
`--output-dir <dir>` is passed. Templates are marked with `template: true` and
`ok: false`; the 9.5 audit rejects them with `template_artifact`. Operators must
replace them with real evidence before a release can pass. The same command also
prints the current `security_source_hashes` map for the source files that must be
covered by `docs/business-os-security-privacy-signoff.json`; these hashes are a
review input, not a signoff.

Real custom artifacts should be built from measured gate JSON with:

```bash
node src/core/rxdb/tools/build_sync_production_readiness_95_artifact.js \
  --kind wan_turn_matrix \
  --input runtime/build/wan-turn-measurements.json \
  --output runtime/build/ctox-sync-production-readiness-95-wan-turn-matrix.json
```

The builder injects the current Git commit, dirty flag, browser bundle hash,
smoke binary hash and no-retry attempt envelope. It rejects template input,
schema/source/attempt fields supplied by hand and gate payloads that do not meet
the 9.5 pass predicates. It is an artifact-normalization tool, not a substitute
for running the gate.

The full Browser/Rust matrix must be run through:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_full_matrix.js \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

The wrapper runs both the default matrix and the Business OS production matrix
with `SMOKE_MATRIX_ATTEMPTS=1`, zero warning/error/request-failure budgets and
the canonical 9.5 output paths. The underlying matrix runner records commit,
dirty flag, browser bundle hash and smoke binary hash; the audit rejects missing
default modes, missing Business OS production modes or fewer than 40 unique
modes.

WAN/TURN evidence must be produced through the dedicated runner:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_wan_turn_matrix.js \
  --external-measurements runtime/build/ctox-sync-production-readiness-95-wan-turn-external.json \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

The runner executes the local LAN/reconnect Browser/Rust smokes and then builds
the final `wan_turn_matrix` artifact through the common builder. Real WAN,
TURN-only, TURN credential rotation and eight-hour offline catch-up results must
come from an external measurement file with schema
`ctox.sync.production_readiness_95.wan_turn_external_measurements.v1` and
`environment_kind: "real_wan_turn"`. Simulated or missing TURN evidence is
rejected; local network-flap smokes are only a preflight, not a production
WAN/TURN claim.

Runbook exercise evidence must be produced through:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_runbook_exercises.js \
  --exercise-log runtime/build/ctox-sync-production-readiness-95-runbook-exercise-log.json \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

The exercise log uses schema
`ctox.sync.production_readiness_95.runbook_exercise_log.v1`. Every required
runbook must have a dated `passed` exercise with evidence URI or hash, and all
follow-ups must be closed. The final artifact records the oldest exercise age
and fails the 9.5 gate when any required runbook is older than 90 days, missing
or still has open follow-ups.

Long-running operational gates use the same strict wrapper:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_operational_gate.js \
  --gate canary_72h \
  --evidence-log runtime/build/ctox-sync-production-readiness-95-canary_72h-evidence-log.json \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

Supported gates are `canary_72h`, `native_restore_drill`,
`record_workbench_30_day_pilot` and `workflow_30_day_pilot`. Their input schemas
are `ctox.sync.production_readiness_95.canary_observation.v1`,
`ctox.sync.production_readiness_95.native_restore_drill_log.v1` and
`ctox.sync.production_readiness_95.pilot_observation.v1`. The runner validates
duration, incidents, retry count, restore snapshot integrity fields, closed
pilot metrics and required fault coverage before delegating to the common
artifact builder. Missing logs remain release blockers.

The browser recovery matrix has an executable runner:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_browser_recovery_matrix.js \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

It runs the recovery crypto, quota recovery, non-blocking recovery registration,
recovery journal browser and primary reset browser smokes, then builds
`runtime/build/ctox-sync-production-readiness-95-browser-recovery-matrix.json`.
The artifact is accepted only from a clean release commit with the real smoke
binary hash.

The app-runtime package gate also has an executable runner:

```bash
node src/core/rxdb/tools/run_sync_production_readiness_95_app_runtime_package_gate.js \
  --smoke-binary runtime/build/core-rxdb-integration-target/debug/ctox
```

It runs native runtime-installed schema, declarative migration, module release
and peer-revocation tests; the Business OS declarative migration checker; and
the `business-os-dynamic-apps-ui`, `business-os-app-release-ui` and
`business-os-app-audience-ui` Browser/Rust modes. It then builds
`runtime/build/ctox-sync-production-readiness-95-app-runtime-package-gate.json`.
The gate is intentionally red if browser modes are skipped.

The compact operator report is generated with:

```bash
node src/core/rxdb/tools/print_sync_production_readiness_95_report.js
```

It reads `runtime/build/ctox-sync-production-readiness-95-evidence-audit.json`,
writes `runtime/build/ctox-sync-production-readiness-95-operator-report.json`,
prints a concise gate summary, and exits non-zero while the audit is blocked.

Custom 9.5 artifacts (`canary`, `restore_drill`, `wan_turn_matrix`,
`browser_recovery_matrix`, `app_runtime_package_gate`, `pilot` and
`runbook_exercises`) must use this evidence envelope:

```json
{
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

The full smoke matrix also emits `source` with the same commit, dirty flag and
artifact hashes. The 9.5 audit rejects matrix artifacts whose `gitRevision` or
`source.commit` differs from the candidate commit.

The GitHub soak workflow must keep a timeout budget large enough for the
nine-cycle nightly profile. The workflow guard treats `timeout-minutes: 360` as
part of the readiness contract.

## 3. Production Observability Contract

`ctox business-os rxdb status --json` is the preferred operator interface. It
should expose a `productionReadiness` object instead of adding a parallel status
command.

Required fields:

- SLO samples: local submit, native observation, terminal projection, reconnect
  and convergence p50/p95/p99;
- journal state: pending count, pending bytes, oldest write age, last replay,
  last export and last GC;
- transport state: circuit breaker, selected ICE candidate type, RTT, TURN
  credential expiry, backpressure stalls and rejected frames;
- recovery state: last primary rebuild, last import preview, last import apply,
  last native restore drill and backup freshness;
- command state: intake backlog, projection backlog, consumer liveness, progress
  age, duplicate-effect count and command-induced restart count;
- Saga state: running, compensating, failed, `manual_intervention`, oldest
  incomplete Saga and compensation failures;
- conflict state: unresolved count, oldest conflict, clock-skew conflicts and
  delete-vs-update conflicts;
- multi-tab state: leader present, lease age and last handover duration;
- release evidence: commit SHA, dirty flag, bundle hash, test binary hash and
  last green release/nightly/canary artifacts.

Alerts must be derivable from the same fields. Required alerts are SLO breach,
stale journal, storage pressure with pending writes, missing TURN in WAN mode,
task/outbox/Saga progress stall, unresolved manual intervention, old restore
drill and artifact commit mismatch.

## 4. Recovery And Backup Gates

Browser recovery gates:

- crash before and after journal commit, primary write and master ack;
- replay with journal present and primary missing;
- replay with primary present and journal still pending;
- encrypted export/import with wrong password, tampered artifact, schema mismatch
  and instance mismatch;
- blocked IndexedDB open/delete from an old tab;
- quota failure during journal append, primary write and demand materialization;
- primary reset denied while pending writes exist unless a recovery export exists.

Native backup gates:

- encrypted off-host snapshot every 15 minutes;
- signed manifest with database hash, schema/runtime hash, commit SHA and
  artifact hash;
- weekly automated restore drill;
- monthly manual restore drill;
- release blocks when the newest successful restore drill is older than seven
  days or references a different release evidence set.

## 5. Runtime App Production Contract

Client-only Business OS apps are a core product claim. Production readiness
requires safe runtime evolution without Rust edits, source rebuilds or manual
daemon restarts.

No backend recompile is allowed for installing a new valid app package, adding
a new collection through the runtime schema contract, or activating a compatible
declarative migration.

Required additions:

- signed app packages with a `ctox.business_os.app_package_signature.v1`
  manifest over module metadata, collection schemas, action definitions,
  migrations and assets;
- trusted publisher and revocation checks before activation;
- declarative schema migrations in `collections.migrations.json`;
- runtime hash includes schemas, actions and migrations;
- N and N-1 package compatibility;
- destructive changes require a new collection plus a Saga migration;
- action definitions are snapshotted at command admission and cannot be changed
  for already-running Sagas.

Allowed v1 migration operations are adding a field with default, renaming a
JSON-pointer field, mapping enum values, adding a declared index and removing an
optional field after the compatibility window. Free JavaScript, SQL, filesystem
access, host paths and arbitrary network access are not allowed.

Runtime v1 apps are trusted, signed same-origin apps. Untrusted third-party app
sandboxing is out of scope for this 9.5 gate and should be tracked as a separate
runtime-v2 program.

## 6. WAN And TURN Matrix

The release candidate must pass deterministic LAN, WAN and adverse WAN profiles:

- LAN: 20 ms RTT, 0.1 percent loss, 50 Mbps;
- WAN: 120 ms RTT, 1 percent loss, 10 Mbps;
- adverse WAN: 300 ms RTT, 3 percent loss, 2 Mbps;
- TURN-only path with direct peer candidates unavailable;
- TURN credential expiry and rotation during active sync;
- signaling partition during local writes and command execution;
- five users, ten tabs, 50k documents;
- eight hour offline catch-up without full resync when checkpoints are valid.

The matrix must prove no lost confirmed writes, no duplicate effects, typed
failures for terminal conditions and status evidence that explains the failure
mode.

## 7. Security And Privacy Signoff

The machine-readable release blocker remains
`docs/business-os-security-privacy-signoff.json`. 9.5 readiness requires every
control to be signed off against the exact release commit:

- dynamic app runtime boundary;
- source visibility;
- data locked state;
- MCP scope;
- audit/export redaction;
- external effects;
- artifact integrity.
- sync recovery crypto boundary;
- WebRTC peer identity and transport boundary;
- Saga idempotency and compensation;
- production evidence and runbook integrity.

The signoff artifact must include SHA-256 hashes for the release workflow, the
9.5 readiness workflow, the readiness plan, runbooks, readiness audit/report
tools, smoke matrix registry and the security signoff checker. The audit rejects
missing, malformed or mismatched hashes with `source_hash_required_missing`,
`source_hash_invalid` or `source_hash_mismatch`.

An external review is required before claiming 9.5. The review must cover
workspace isolation, record grants, package tampering, replay/idempotency abuse,
peer impersonation, recovery export crypto, Saga/compensation bypass, MCP policy
bypass and audit-log manipulation.

## 8. Pilot Gates

Two 30-day pilots are required:

- Record Workbench app: runtime install, new collection, CRUD, reactive queries,
  offline writes, two devices per user, conflict handling, recovery import/export
  and schema upgrade.
- Multi-Collection Workflow app: declarative action, at least three Saga steps,
  grant checks, crash replay, idempotency, compensation, audit and an injected
  compensation failure with a practiced runbook.

Pilot blockers are data loss, duplicate effects, unauthorized read/write,
unexplained terminal states, `manual_intervention` older than 24 hours, SLO
convergence below 99.9 percent, failed restore drill or any P0/P1 incident.

## 9. Rollout And Runbooks

Rollout state must be typed and persisted, not controlled by production
environment toggles. Cohorts are `internal`, `pilot`, `10_percent`,
`25_percent`, `50_percent` and `100_percent`. Each step requires seven green
days before promotion.

The operator runbooks are tracked in
`docs/ctox-sync-production-readiness-runbooks.md`. Release evidence for exercised
runbooks is tracked in
`runtime/build/ctox-sync-production-readiness-95-runbook-exercises.json`.

Required runbooks:

- signaling/TURN outage;
- WebRTC backpressure stall;
- journal growth and replay;
- quota exhaustion;
- blocked IndexedDB primary;
- browser-origin loss;
- native SQLite restore;
- schema migration block;
- Saga compensation failure;
- conflict flood;
- app package revocation;
- key revocation;
- MCP access incident.

Each runbook must list symptoms, status fields, immediate containment, recovery
steps, prohibited actions and verification after recovery.

## 10. Completion Definition

CTOX Sync Engine reaches 9.5/10 only when all of these are true on the same
release line:

- browser RxDB suite green;
- native `ctox-rxdb` suite green;
- root `cargo check --bin ctox` green;
- one clean full matrix green with at least 40 unique modes covered;
- 3 x 33 release soak green with zero retries;
- 9 x 33 nightly soak green with zero retries;
- 72 hour canary green;
- WAN/TURN matrix green;
- browser recovery matrix green;
- native restore drill fresh and green;
- runtime app package signing and declarative migrations enforced;
- app-runtime browser gates run only against a `ctox` binary whose embedded
  `ctox version` commit matches the candidate HEAD;
- all security/privacy controls signed off on the release commit;
- both 30-day pilots completed without blocker;
- runbooks exist and have been exercised at least once.
