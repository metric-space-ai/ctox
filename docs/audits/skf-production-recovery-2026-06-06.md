# skf.ctox.dev Production Recovery Tracker

Date: 2026-06-06
Owner: CTOX recovery workstream
Status: Not production ready
Deployment path: `ctox upgrade --dev`

This document tracks the concrete recovery work for `skf.ctox.dev` and the related CTOX core fixes that must land in `main`.

## Current Truth

`skf.ctox.dev` is not production ready.

The current failure is not one isolated UI bug. The durable queue, Business OS projections, and browser UI disagree about task state. That makes the UI show stale queued work, hides terminal failures, and breaks user trust in right-click tasks, bug reports, and research continuations.

Known production evidence:

- The canonical queue table reports old Research tasks as `failed`.
- The first production repair apply corrected the Business Store projection, but the active RxDB projection still reported six terminal canonical queue tasks as `queued` / `pending`.
- Old oversized task payloads failed with `Input exceeds the maximum length of 1048576 characters`.
- At least one old Bug Reporter task was leased by `ctox-service` from a payload generated before report payload redaction.
- Active code must not contain an HTTP command fallback. Old persisted commands may still contain `business-os-http-command-fallback` as historical context and must be quarantined, not re-executed.

## Non-Negotiable Done Criteria

The work is complete only when all of these pass on production with a fresh browser profile:

- `skf.ctox.dev` starts without local storage / RxDB startup failure.
- Login works.
- Logout works.
- Reload after login works.
- Reload after logout remains logged out.
- Tenant scope is correct for skf.
- Documents app shows expected skf data.
- Web Research app shows expected skf data.
- Right-click task creation creates a visible tracked task and CTOX processes it to a terminal state.
- Bug Reporter send creates a tracked task and does not hang on `Sende...`.
- `Research fortsetzen` creates visible chat/task feedback using `ctox-business-os-chat-submit`.
- `Fortschritt ansehen` focuses the task.
- `Ergebnis ansehen` opens the result.
- Browser console has no critical runtime, sync, RxDB, WebSocket, auth, or service worker errors.
- Network panel has no failed critical startup/sync/task requests.

## Scope Split

### CTOX General Fixes In Main

These fixes belong in `main` because they affect every CTOX Business OS instance, not only skf.

| ID | Area | File(s) | Required work | Status |
| --- | --- | --- | --- | --- |
| CORE-1 | Queue projection truth | `src/core/business_os/rxdb_peer.rs`, `src/core/business_os/store.rs`, `src/core/mission/channels.rs` | Reconcile `ctox_queue_tasks` from canonical `communication_routing_state`; terminal queue statuses must overwrite stale RxDB projection state. | Local tests passed: Store repair tests and RxDB projection reconciliation tests. Production repair apply pending. |
| CORE-2 | Command projection writeback | `src/core/business_os/store.rs` | Queue terminal statuses must update `business_commands.status`, `task_status`, `route_status`, error/status note, and `updated_at_ms`. | In progress: implemented locally; failed canonical queue test verifies command writeback. |
| CORE-3 | Worker completion/failure | `src/core/service/service.rs` | Worker success/failure must refresh command, queue task projection, and chat tracking immediately after ack/fail. | Pending |
| CORE-4 | Repair CLI | CLI/service command path | Add idempotent dry-run/apply command for queue projection repair. Command name target: `ctox business-os repair queue-projections`. | In progress: CLI wired locally; Store repair engine tests pass; production dry-run still pending. |
| CORE-5 | HTTP fallback guard | frontend + Rust guard tests | Keep active code free of `/api/business-os/commands`, `http-fallback`, and `business-os-http-command-fallback`. Historical records are allowed only as quarantined data. | Local guard passed: `assert-rxdb-only.mjs` OK; direct Subscription Auth HTTP fallback removed from `react-settings.js`. |
| CORE-6 | Bug Reporter payload | `src/apps/business-os/shared/business-reporter.js` | Remove screenshot data URLs and raw strokes from command payloads; send only metadata/reference context and timeout cleanly. | In progress |
| CORE-7 | Chat/task UI layout | `src/apps/business-os/shared/business-chat.js` | Prevent overflow/clipping in chat windows, task cards, status buttons, long URLs, and command IDs. | Local syntax check passed; browser layout E2E pending. |
| CORE-8 | Task/result focus | CTOX module + shared chat/task handlers | `Fortschritt ansehen` and `Ergebnis ansehen` must focus the correct task/result, not no-op. | Local CTOX module focus tests passed; browser E2E pending. |
| CORE-9 | Browser startup resilience | `src/apps/business-os/app.js`, `shared/db.js`, `shared/sync.js` | RxDB/IndexedDB startup must recover without destructive cache wipes and must show useful user-facing recovery text. | In progress |

### skf-Specific Production Cleanup

These are instance data repairs on the skf VPS/runtime after the general fix is deployed.

| ID | Artifact class | Detection | Cleanup action | Status |
| --- | --- | --- | --- | --- |
| SKF-1 | Stale queued Research projections | `business_records.collection='ctox_queue_tasks'` says `queued`, canonical queue says `failed` | Update projection to `failed`, copy `last_error/status_note`, update `business_commands` to terminal failed if still accepted. | Covered locally by `repair_queue_projections_updates_failed_canonical_queue_and_command`; production apply pending. |
| SKF-2 | Stale completed projections | Projection says `queued`, canonical queue says `handled` | Update projection to `completed/handled`, update command `completed`, refresh chat message if linked. | Covered locally by `repair_queue_projections_acks_leased_terminal_success_note`; production apply pending. |
| SKF-3 | Orphan queue projections | Projection has task id, canonical queue has no task, command is old active state | Mark projection and command `failed` with `orphaned_queue_projection`; do not requeue automatically. | Pending |
| SKF-4 | Legacy HTTP fallback commands | Payload/client context contains `http-fallback` or `business-os-http-command-fallback` | Mark as historical legacy transport; do not use as evidence of active fallback; do not replay. | Covered locally by legacy record count test; active-code guard passed. Production repair apply pending. |
| SKF-5 | Oversized reporter payloads | Payload contains `attachment.data_url` or raw `strokes`, or prompt exceeds worker input limit | Redact raw data from projections; keep metadata and mark failed if already terminal in queue. | Covered locally by inline artifact redaction test; production apply pending. |
| SKF-6 | Pending chat messages tied to terminal tasks | `business_chats.messages[].status` active but linked task terminal | Update message status/result/error from canonical task/command. | Pending |
| SKF-7 | Duplicate/stale task cards | UI projection includes terminal old tasks as active pipeline entries | Repair/deletion marker so UI does not show them in active pipeline. | Pending |

## Production Backup Commands

Run on the target VPS before any apply step:

```bash
set -euo pipefail
runtime="$HOME/.local/lib/ctox/current/runtime"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
cp "$runtime/ctox.sqlite3" "$runtime/ctox.sqlite3.pre-skf-recovery-$stamp"
cp "$runtime/business-os.sqlite3" "$runtime/business-os.sqlite3.pre-skf-recovery-$stamp"
cp "$runtime/business-os-rxdb.sqlite3" "$runtime/business-os-rxdb.sqlite3.pre-skf-recovery-$stamp"
```

## Diagnostic Queries

Canonical durable queue:

```sql
select
  m.message_key,
  r.route_status,
  r.lease_owner,
  r.leased_at,
  r.acked_at,
  substr(r.last_error, 1, 160) as last_error,
  r.updated_at
from communication_messages m
left join communication_routing_state r on r.message_key = m.message_key
where m.channel = 'queue'
  and m.direction = 'inbound'
order by r.updated_at desc;
```

Business OS queue projection:

```sql
select
  record_id,
  json_extract(payload_json, '$.command_id') as command_id,
  json_extract(payload_json, '$.status') as status,
  json_extract(payload_json, '$.route_status') as route_status,
  substr(json_extract(payload_json, '$.title'), 1, 120) as title,
  updated_at_ms
from business_records
where collection = 'ctox_queue_tasks'
  and deleted = 0
order by updated_at_ms desc;
```

Mismatch finder:

```sql
select
  q.record_id as projection_task_id,
  json_extract(q.payload_json, '$.command_id') as command_id,
  json_extract(q.payload_json, '$.status') as projection_status,
  json_extract(q.payload_json, '$.route_status') as projection_route_status,
  r.route_status as canonical_route_status,
  substr(r.last_error, 1, 160) as canonical_error
from business_records q
left join communication_routing_state r on r.message_key = q.record_id
where q.collection = 'ctox_queue_tasks'
  and q.deleted = 0
  and coalesce(json_extract(q.payload_json, '$.route_status'), '') != coalesce(r.route_status, '');
```

Legacy fallback detector:

```sql
select
  record_id,
  substr(payload_json, 1, 240)
from business_records
where collection in ('business_commands', 'ctox_queue_tasks', 'business_chats')
  and deleted = 0
  and (
    payload_json like '%http-fallback%'
    or payload_json like '%business-os-http-command-fallback%'
    or payload_json like '%/api/business-os/commands%'
  );
```

Oversized reporter detector:

```sql
select
  record_id,
  length(payload_json) as payload_len,
  json_extract(payload_json, '$.command_type') as command_type
from business_records
where collection in ('business_commands', 'ctox_queue_tasks')
  and deleted = 0
  and (
    payload_json like '%data:image/%'
    or payload_json like '%"strokes"%'
  )
order by payload_len desc;
```

## Repair Command Contract

Target command:

```bash
ctox business-os repair queue-projections --dry-run
ctox business-os repair queue-projections --apply
```

Required behavior:

- Dry-run prints exact counts and record IDs for every change class.
- Apply is idempotent.
- Apply never requeues failed legacy work automatically.
- Apply writes a repair note into repaired projections.
- Apply redacts oversized screenshot/stroke payloads from projections while keeping metadata.
- Apply exits non-zero if it cannot read the canonical queue or Business OS store.

Expected dry-run sections:

```text
Queue projection mismatches:
  failed_from_canonical: N
  completed_from_canonical: N
  orphaned_active_projection: N
  legacy_http_fallback_records: N
  oversized_reporter_payloads: N
  chat_messages_to_terminalize: N

No writes performed.
```

## Implementation Steps

### Step 1: Finish rebase-safe local baseline

- Resolve current rebase conflict in `src/apps/business-os/app.js`.
- Confirm no conflict markers:

```bash
rg -n "^(<<<<<<<|=======|>>>>>>>)" .
```

- Confirm active fallback guard:

```bash
rg -n "/api/business-os/commands|dispatchCommandViaHttp|restartBusinessCommandsSync|business-os-http-command-fallback|http-fallback" \
  src/apps/business-os src/core/business_os src/core/service \
  --glob '!src/apps/business-os/vendor/**'
```

Only guard/test references may remain.

### Step 2: Add queue projection reconciliation tests

Add focused tests for:

- canonical `failed`, projection `queued` -> projection `failed`.
- canonical `handled`, projection `queued` -> projection `completed`.
- missing canonical queue, old accepted command -> projection and command `failed`.
- legacy fallback context is marked, not replayed.
- reporter oversized payload is redacted.

### Step 3: Fix core reconciliation

Modify reconciliation to:

- Load projection task id.
- Load canonical queue task by id.
- If canonical exists, map `route_status` into projection and command state.
- If canonical missing and projection is active for more than repair threshold, mark failed.
- Persist both Business OS store and RxDB projection.

### Step 4: Fix worker writeback

Worker completion/failure must perform:

```text
ack/fail communication_routing_state
-> refresh business_commands
-> refresh ctox_queue_tasks
-> refresh business_chats linked tracking message
```

This must happen for success, failure, panic, timeout, and review-terminal failure.

### Step 5: Fix frontend task creation feedback

For right-click, Bug Reporter, and Research continuation:

- Always dispatch through `ctox-business-os-chat-submit` or the RxDB command bus.
- Always open or update a visible chat/task surface.
- Show task id and status.
- Poll/subscribe until terminal state.
- Keep user-visible state aligned with canonical projection.

### Step 6: Fix Bug Reporter payload

- Replace raw screenshot payload with compact metadata/reference.
- Add send timeout.
- On timeout: show explicit failure and leave the form usable.
- On success: show task id and close only after the task is visibly created.

### Step 7: Fix chat layout and task buttons

- Long URLs, command IDs, and generated answers must wrap.
- Chat window must not overflow the viewport.
- `Fortschritt ansehen` must focus CTOX task.
- `Ergebnis ansehen` must focus the completed result/chat.

### Step 8: Local verification

Run:

```bash
node --check src/apps/business-os/shared/business-chat.js
node --check src/apps/business-os/shared/business-reporter.js
node --check src/apps/business-os/shared/command-bus.js
node --check src/apps/business-os/app.js
node --check src/apps/business-os/shared/db.js
node --check src/apps/business-os/shared/sync.js
node src/apps/business-os/scripts/assert-rxdb-only.mjs
cargo fmt --check
cargo check
git diff --check
```

Run the new focused Rust tests for queue projection repair.

### Step 9: Push to main

After tests pass:

```bash
git add <changed files>
GIT_EDITOR=true git rebase --continue
git fetch origin main
git rebase origin/main
git push origin HEAD:main
```

### Step 10: Deploy via upgrade --dev

On VPS:

```bash
ctox upgrade --dev
ctox status --json
ctox queue list --status pending --status leased --limit 20
```

No other deployment path is acceptable for this recovery.

### Step 11: Run skf cleanup

After new code is live:

```bash
ctox business-os repair queue-projections --dry-run
ctox business-os repair queue-projections --apply
ctox status --json
ctox queue list --status pending --status leased --limit 20
```

Then rerun mismatch SQL. Expected result: no active projection that disagrees with canonical terminal queue state.

### Step 12: Production browser E2E

Use Browser/Playwright with a clean profile.

Required scenarios:

| Story | Expected result | Status |
| --- | --- | --- |
| Fresh open `https://skf.ctox.dev` | App starts; no storage failure. | Pending |
| Login | User becomes authenticated. | Pending |
| Authenticated reload | Session and data persist. | Pending |
| Tenant scope | Only skf-allowed apps/data are visible. | Pending |
| Documents data | Expected documents list/details visible. | Pending |
| Web Research data | Expected dashboard/source counts visible. | Pending |
| Right-click task | Task created, visible, processed to terminal state. | Pending |
| Bug Reporter | Send completes, tracked task appears, no `Sende...` hang. | Pending |
| Research fortsetzen | Visible chat/task created via standard submit path. | Pending |
| Fortschritt ansehen | Opens/focuses the running/completed task. | Pending |
| Ergebnis ansehen | Opens/focuses the result. | Pending |
| Logout | Session ends; protected state unavailable after reload. | Pending |
| Console/network | No critical errors. | Pending |

## Rollout Impact

### Affects CTOX generally

- Queue projection reconciliation.
- Command/task/chat writeback.
- Reporter payload shape.
- Chat/task UI rendering.
- Startup/RxDB resilience.
- HTTP fallback guard.
- Repair CLI.

### Affects skf only

- Runtime DB backups.
- Runtime DB repair apply.
- Redaction/terminalization of old skf artifacts.
- Validation of skf tenant data and skf-specific app visibility.

### Also verify after skf

- `cto1.kunstmen.com` startup, tenant scope, data visibility, task creation, and logout because CTOX-general fixes affect it too.

## Progress Log

| Time | Entry |
| --- | --- |
| 2026-06-06 | Recovery tracker created. Current status remains not production ready. |
| 2026-06-06 11:14 CEST | Phase 1 baseline stabilized locally: `cargo check` passed, JS syntax checks for `business-chat.js`, `business-reporter.js`, and `app.js` passed, no conflict markers found in touched files, and the `app.js` rebase conflict was resolved in the index. Status remains not production ready until repair tests, deployment, cleanup, and browser E2E pass. |
| 2026-06-06 11:49 CEST | Phase 2 Store repair engine verified locally: `cargo test repair_queue_projections --bin ctox -- --nocapture` passed 3/3 tests. Covered canonical failed queue -> failed projection/command with `last_error`, leased terminal success -> guard-compliant handled/completed repair, and oversized inline screenshot/stroke redaction with legacy fallback counting. Status remains not production ready until broader checks, push, `ctox upgrade --dev`, skf cleanup, and browser E2E pass. |
| 2026-06-06 11:53 CEST | Phase 3 HTTP fallback guard passed locally: removed the direct `fetch('/api/business-os/ctox/subscription-auth/start')` fallback from `shared/react-settings.js`; `node --check shared/react-settings.js` passed; `node src/apps/business-os/scripts/assert-rxdb-only.mjs` returned `RxDB-only contract OK`. Remaining fallback strings are limited to repair detectors, tests, and guard script rules. |
| 2026-06-06 11:58 CEST | Phase 4 local UI/core regression checks passed: `cargo test reconcile_ctox_queue_task_projections --bin ctox -- --nocapture` passed 2/2; JS syntax passed for Research, Chat, Reporter, and React Settings; CTOX module test passed using isolated `/tmp` `esbuild` install. Browser E2E still pending and required before production-ready. |
| 2026-06-06 12:10 CEST | Phase 5 local gates passed after `cargo fmt`: `cargo fmt --check` passed, `git diff --check` passed, `node src/apps/business-os/scripts/assert-rxdb-only.mjs` returned `RxDB-only contract OK`, `cargo check` passed, `cargo test repair_queue_projections --bin ctox -- --nocapture` passed 3/3, and `cargo test reconcile_ctox_queue_task_projections --bin ctox -- --nocapture` passed 2/2. Status remains not production ready until rebase/push, `ctox upgrade --dev`, skf cleanup, and browser E2E pass. |
| 2026-06-06 12:14 CEST | Rebase completed successfully on `codex/upgrade-dev-prod-hotfix-2`; Git created commit `9c699c45` for the local hotfix set. Next phase is main push plus production deploy through `ctox upgrade --dev`; status remains not production ready until deploy, cleanup, and browser E2E pass. |
| 2026-06-06 12:18 CEST | Rebased the hotfix onto current `origin/main`; resolved the only conflicts in `app.js` and `index.html` by setting the combined cachebuster to `20260606-skf-prod-recovery1`. Conflict-marker check, `node --check src/apps/business-os/app.js`, and `git diff --check` passed before continuing. New hotfix commit is `5ec2c140`. |
| 2026-06-06 12:24 CEST | Final pre-push gates passed on the `origin/main`-rebased candidate: `cargo fmt --check`, `git diff --check origin/main...HEAD`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, JS syntax checks for App/Chat/Reporter/React Settings/Research, `cargo check`, `cargo test repair_queue_projections --bin ctox -- --nocapture` 3/3, and `cargo test reconcile_ctox_queue_task_projections --bin ctox -- --nocapture` 2/2. Next action: push `HEAD` to `main`; still not production ready until `ctox upgrade --dev`, skf cleanup, and browser E2E pass. |
| 2026-06-06 12:25 CEST | Pushed hotfix commit `dc8557a5` to `main` (`ebd02c47..dc8557a5`). Next action is production rollout through `ctox upgrade --dev`; no direct deploy path or HTTP fallback path is being used. |
| 2026-06-06 15:14 CEST | Phase 6 deployment completed through the required `ctox upgrade --dev` path on the VPS. Upgrade exit code was `0`; active release is `branch-main-20260606T102658Z`; previous release was `branch-main-20260606T043130Z`; upgrade state backup is `/home/ubuntu/.local/state/ctox/backups/update-20260606T102702Z`; `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260606T102658Z`. `systemctl --user status ctox.service` reports `active (running)` with PID `523198`. `ctox status --json` reports `business_os.ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `signaling_urls=[wss://signaling.ctox.dev]`, `native_rxdb_peer_status.running=true`, and `http_bridge_available=false`. Build warnings observed but non-fatal: missing vendored Bubblewrap package with system `bwrap` fallback message from the linux sandbox build script, missing `nvcc` for CUDA glue archives, and existing Rust `unused/dead_code` warnings in RxDB, Web Stack, CTOX, and Desktop. Status remains not production ready until skf runtime cleanup and browser E2E pass. |
| 2026-06-06 16:05 CEST | Phase 7 runtime cleanup exposed the remaining root cause: `ctox business-os repair queue-projections --apply` fixed Business Store mismatches, but six rows in active `business-os-rxdb.sqlite3` still had `status=queued` / `route_status=pending` while canonical `communication_routing_state` was terminal `failed`. `ctox peer rotate` kept WSS-only sync and `http_bridge_available=false`, but did not materialize those rows. Required fix in `main`: repair writes must update both `business-os.sqlite3.business_records` and the active RxDB collection tables (`ctox_business_os__ctox_queue_tasks__v0`, `ctox_business_os__business_commands__v1`). Browser E2E remains blocked until this is deployed and the production mismatch count is zero. |
| 2026-06-06 16:28 CEST | Phase 8 local RxDB repair fix implemented and verified: `repair_queue_projections` now writes changed queue task projections, command projections, and redacted inline artifact payloads to both Business Store and active RxDB collection tables when `--apply` is used. Added test coverage that seeds stale `ctox_business_os__ctox_queue_tasks__v0` and `ctox_business_os__business_commands__v1` rows and asserts dry-run does not mutate them while apply does. Gates passed: `cargo fmt --check`, `git diff --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `cargo test repair_queue_projections --bin ctox -- --nocapture` 3/3, and `cargo test reconcile_ctox_queue_task_projections --bin ctox -- --nocapture` 2/2. Status remains not production ready until this is pushed to `main`, deployed via `ctox upgrade --dev`, production mismatch count is zero, and browser E2E passes. |
| 2026-06-06 16:35 CEST | Phase 9 pre-push gate completed: `cargo check` passed, then `cargo fmt --check` and `git diff --check` passed again after applying the one Rustfmt-required wrap in `src/core/iot/widget_runtime.rs`. Current commit scope is the RxDB repair writeback, tracker update, and that formatting-only Rustfmt line wrap. Next action: commit and push to `main`. |
| 2026-06-06 16:40 CEST | Phase 10 main push completed: commit `75333b48` (`fix: repair active rxdb queue projections`) was pushed to `main` (`edbd66a3..75333b48`). Next action: deploy this exact main state through `ctox upgrade --dev`; production remains not ready until deploy, repair apply, zero production mismatches, and browser E2E pass. |
| 2026-06-06 17:14 CEST | Phase 11 deployment completed through `ctox upgrade --dev`: release `branch-main-20260606T134023Z` applied; previous release was `branch-main-20260606T102658Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260606T134027Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260606T134023Z`. `systemctl --user is-active ctox.service` is `active`; `ctox status --json` reports `business_os.ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `signaling_urls=[wss://signaling.ctox.dev]`, `native_rxdb_peer_status.running=true`, and `http_bridge_available=false`. Known non-fatal build warnings repeated: Bubblewrap vendored package missing with system fallback, missing `nvcc`, and existing Rust unused/dead_code/deprecated warnings. Next action: run production repair apply and verify active RxDB mismatch count is zero. |
| 2026-06-06 17:18 CEST | Phase 12 skf runtime cleanup completed: fresh DB backups created with stamp `20260606T141354Z`; `ctox business-os repair queue-projections --dry-run` had no repair actions; `--apply` completed with `ok=true` and no actions. Post-apply SQL verification: Business Store terminal mismatch `0`, active RxDB queue terminal mismatch `0`, active RxDB command-active-for-terminal-queue mismatch `0`. `ctox status --json` after repair reports `running=true`, `busy=false`, `pending_count=0`, `worker_active_count=0`, `business_os.ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `signaling_urls=[wss://signaling.ctox.dev]`, `http_bridge_available=false`, and `native_rxdb_peer_running=true` with peer session `rxdb-rs-710f4ba330144556b5bc8fa7108f773b`. Production still not declared ready until browser E2E passes. |
| 2026-06-06 16:32 CEST | Phase 13 browser E2E failed and is the active blocker: clean Playwright session can log in as admin and load the shell, but the browser remains stuck at `Inhalte werden synchronisiert 0/5` after reload and long waits; Documents opens but shows `Keine Dokumente` while server-side RxDB contains 13 documents in `ctox_business_os__documents__v0`. Browser console has no later fatal errors after reload, but reports `Timed out waiting for business_commands sync bridge readiness` when commands are attempted. First login attempt also observed `request-timeout-masterChangesSince`. Backend cleanup is therefore not sufficient; production remains not ready until browser RxDB sync readiness and visible module data are fixed and the full E2E matrix passes. |
