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

## Phase 34 - Post-Upgrade Server Truth Check

Status: Complete

Result: The `ctox upgrade --dev` deployment of `906ab4d7` is active on the VPS, but `skf.ctox.dev` is still not production ready.

Evidence from `/home/ubuntu/.local/lib/ctox/current`:

- Active release: `branch-main-20260607T022749Z`.
- Served Business OS asset marker: `app.js?v=20260606-rxdb-public-catalog2`.
- `business_users` is still empty immediately after deploy.
- RxDB `business_module_catalog` has 12 modules and `allowed_module_ids=[]`.
- Catalog module ids are `app-store`, `browser`, `calendar`, `creator`, `ctox`, `desktop`, `documents`, `knowledge`, `notes`, `reports`, `spreadsheets`, `tickets`.
- `research` is not in the projected module catalog, so Web Research cannot be considered restored for skf.

Next action: run a fresh browser login against `skf.ctox.dev` with the real account and then re-check whether the authenticated shell/login path seeds `business_users`. If it does not, the public skf login path is bypassing the core login seed path and needs either a CTOX core fix or an explicit skf instance data repair before canonical `ctox.app_store.install` can install `research`.

## Phase 35 - Configured Auth Users Back RxDB Admin Commands

Status: Complete locally, push/deploy pending

Result: Root cause confirmed and fixed in CTOX core. The `ctox-dev` public WebDeploy login injects an authenticated browser session, but it does not hit the Rust `/login` route; therefore `business_users` stayed empty and RxDB commands from the browser were downgraded to `user` by `trusted_rxdb_command_user`. That is why admin-class commands such as `ctox.app_store.install` were rejected even though the browser showed an admin user.

Implemented fix:

- `store.rs` now treats server-configured auth identities (`CTOX_AUTH_USERS` plus the default `CTOX_BUSINESS_USER`/`CTOX_BUSINESS_PASSWORD`) as authoritative Business OS users.
- `pull_business_users_for_rxdb` seeds those configured users before projecting `business_users`.
- `trusted_rxdb_command_user` seeds those configured users before resolving an RxDB command actor, so existing RxDB command authorization remains server-side and does not trust a browser-supplied role.
- No HTTP command fallback was added.

Local verification:

- `cargo test configured_auth_user_is_trusted_for_rxdb_admin_commands --bin ctox -- --nocapture` passed.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `cargo fmt --check` passed.
- `git diff --check` passed.
- `cargo check -q` passed with existing warnings.

Next action: push this fix to `main`, deploy via `ctox upgrade --dev`, configure the skf authenticated user in the VPS environment, verify `business_users` projects the admin, then install/restore `research` through the canonical RxDB/CTOX command path.

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

## Phase Log

### Phase 32 - Public catalog cleanup E2E and SKF module-scope diagnosis

Status: partial pass, not production ready.

Evidence:

- Browser E2E against `skf.ctox.dev` after `app.js?v=20260606-rxdb-public-catalog2` logged in and rendered CTOX data without new `/api/business-os/commands` requests.
- No console warnings/errors remained from `ctox.source.list_snapshots`; server command count for that command stayed unchanged during the run.
- The active server RxDB `business_module_catalog` contains 12 modules and does not contain `research`.
- The release files contain `src/apps/business-os/modules/research/module.json`, but it is `install_scope: "store"` and not present under `installed-modules`.
- `installed-modules` on SKF currently contains `matching` only.
- `business_users` on SKF is empty even though the browser displays `Michael Welsch@SKF` as Admin.
- Direct canonical Business OS command dispatch for `ctox.app_store.install` is correctly rejected with `chef or admin role required` because the server has no trusted Business OS admin user.

Conclusion:

- `research` is invisible because the server-projected RxDB module catalog is missing the installed Store module, not because the browser should seed packaged modules.
- App-Store installation through the RxDB command bus is blocked by missing trusted server-side user projection.
- Production readiness remains blocked until the login/launch user is persisted server-side, `research` is installed for SKF through the canonical App-Store path, and browser E2E proves Documents, Web Research, task processing, bug reporter, and logout.

### Phase 33 - General login user projection fix

Status: implemented locally, not yet deployed.

Change:

- `src/core/business_os/store.rs` now exposes `remember_authenticated_session_user`, a narrow wrapper around the existing `business_users` seeding path.
- `src/core/business_os/server.rs` calls it after successful `/login` and when serving the authenticated shell index.
- This keeps the existing trust model: RxDB commands still trust only server-known `business_users`; the browser-provided `is_admin` flag remains insufficient by itself.

Verification:

- `cargo check -q` passed with existing warnings only.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `node --check src/apps/business-os/app.js` passed.
- `git diff --check` passed.

Remaining:

- Push to `main`, deploy through `ctox upgrade --dev`, then repair/seed the SKF admin user and install `research` through the canonical App-Store command path.

### Phase 21: Production startup evidence after correct tenant login

Status: Completed, not production ready.

Findings:

- Correct tenant login succeeds.
- `skf.ctox.dev` receives the correct Business OS pairing room for the VPS instance.
- The active instance reports WebRTC/native transport and `http_bridge_available=false`.
- Browser WebRTC DataChannel receives large native master-change streams.
- Browser-side IndexedDB stays empty for required collections and startup remains in initial-sync state.
- Wire probe showed incoming native frames and browser ACKs, but no initial browser `ctoxProtocol` / token / pull request frames before the startup timeout.

Conclusion:

The immediate blocker is in the Browser/RxDB multiplex handshake path, before initial replication can mark required collections synced. This prevents correct data visibility and command processing verification on production.

### Phase 22: Multiplex handshake preflight removal

Status: Implemented locally, production deployment pending.

Change:

- Removed the browser-side wait for `collectCollectionCheckpoints()` from the first shared-room protocol payload in `src/apps/business-os/rxdb/src/replication-webrtc.mjs`.
- Rebuilt `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` so the served app-local RxDB bundle matches source.
- Bumped Business OS app/RxDB cache keys to `20260606-rxdb-multiplex-handshake1`.

Reason:

The first browser protocol frame must not wait on local checkpoint enumeration across all multiplexed collections. On a cold or partially synced browser, that preflight can block the handshake before the native CTOX peer ever receives the browser protocol/token frames.

Verification so far:

- `node --check` passed for changed source and rebuilt bundle.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs` passed.
- Active frontend search still has no command HTTP fallback implementation; remaining fallback strings are repair/guard references for historical data.

Open:

- `src/core/rxdb/tools/browser_rust_smoke.js` still hangs without honoring its configured timeout; this is not counted as a pass.
- Production must be upgraded via `ctox upgrade --dev` and then verified with fresh-browser E2E before any production-ready claim.

### Phase 23: Send queue drain on DataChannel open

Status: Deployed once, follow-up guard implemented locally.

Production evidence after Phase 22 deploy:

- `ctox upgrade --dev` successfully deployed release `branch-main-20260606T213446Z`.
- The served shell loaded build key `20260606-rxdb-multiplex-handshake1`.
- Login succeeded with the skf tenant user.
- The shell reached `dataPlaneReadyStatus=ready` and created the WebRTC sync runtime.
- Required collections still stayed in `initialReplicationState=pending`.
- Diagnostics showed `queuedFrames > 0` and `sentScheduledFrames > 0`, but `sentFrames = 0`; the DataChannel opened and received native frames, then timed out waiting for browser requests that were never actually sent.

Root cause:

`send()` can enqueue protocol/request frames before the RTC data channel is open. The previous `drainSendQueue()` call exits while the channel is not yet open, and `attachChannel().onopen` did not re-drain the queued frames.

Change:

- `src/apps/business-os/rxdb/src/webrtc-native.mjs` now drains the existing send queue immediately after `datachannel-open`.
- Rebuilt `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`.
- Bumped Business OS app/RxDB cache keys to `20260606-rxdb-sendqueue-drain1`.

Verification so far:

- `node --check` passed for changed source and rebuilt bundle.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs` passed.

Open:

- The first deployment of this phase (`20260606-rxdb-sendqueue-drain1`) exposed a missing guard: `drainSendQueue()` threw `Cannot set properties of undefined (setting 'draining')` for peers that opened before any send queue existed.
- Follow-up fix: `drainSendQueue(connection)` now returns immediately when `connection.sendQueue` is absent.
- Rebuilt bundle and bumped cache keys to `20260606-rxdb-sendqueue-drain2`.
- Must commit, push to `main`, deploy via `ctox upgrade --dev`, and rerun fresh-browser E2E.

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
| 2026-06-06 17:05 CEST | Phase 14 local command-path correction completed: no HTTP command endpoint was introduced or kept. The Browser command bus remains RxDB-only, but no longer reports local `pending_sync` as success; it now starts both `business_commands` and `ctox_queue_tasks`, writes the command transport envelope, flushes sync, and waits until the native CTOX peer projects a real `ctox_queue_tasks` row with a `task_id`. Chat, scheduled chat, Bug Reporter, Universal Importer, and Outbound local fallback paths were changed so they cannot show or return a successful task without a real CTOX queue projection. The RxDB contract doc now states that `business_commands` is a transport envelope, not a Business OS task system. Guardrail added to reject HTTP in `shared/command-bus.js` and local pending success in chat. Local checks passed: JS syntax for changed files, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `cargo test native_peer_consumes_pending_business_command --bin ctox -- --nocapture`, `cargo test native_peer_consumes_pending_report_command --bin ctox -- --nocapture`, and `git diff --check`. Production remains not ready until this is pushed, deployed via `ctox upgrade --dev`, skf pending/fake artifacts are cleaned, and browser E2E proves task creation, queue processing, result navigation, data visibility, login/logout, and tenant scope. |
| 2026-06-06 17:39 CEST | Phase 15 deployment completed through the required `ctox upgrade --dev` path: commit `137e02bb` is active as release `branch-main-20260606T150720Z`; previous release was `branch-main-20260606T134023Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260606T150724Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260606T150720Z`. `systemctl --user is-active ctox.service` reports `active`; `ctox status --json` reports `running=true`, `busy=false`, `pending_count=0`, `worker_active_count=0`, `business_os.ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `signaling_urls=[wss://signaling.ctox.dev]`, `native_rxdb_peer_status.running=true`, and `http_bridge_available=false`. Data plane tables exist with `business_commands=312`, `ctox_queue_tasks=38`, `desktop_files=3671`, and `desktop_file_chunks=363253`. Production remains not ready until stale command/queue artifacts are audited or cleaned and browser E2E passes the required user stories. |
| 2026-06-06 17:42 CEST | Phase 16 stale command cleanup completed: active RxDB had one stale `business_commands` row with `status=pending_sync`, empty `task_id`, `command_type=ctox.source.list_snapshots`, `dispatch_transport=rxdb-local-pending`, and `rxdb_sync_error=Timed out waiting for business_commands sync bridge readiness`. It did not exist as a canonical Business Store command and was not an authoritative CTOX queue task. Fresh backups were created at `/home/ubuntu/.local/state/ctox/backups/manual-skf-command-cleanup-20260606T154155Z` before changing runtime DBs. The stale row was marked `failed` with an explicit cleanup reason; post-cleanup verification reports `pending_sync_without_task=0`, active queue waiting-like rows `0`, and no command with a non-empty `task_id` missing a projected `ctox_queue_tasks` row. Production remains not ready until browser E2E passes. |
| 2026-06-06 17:48 CEST | Phase 17 browser E2E exposed a hard login/config blocker: a fresh Playwright session reached the login page and POSTed `/login`, but the server redirected to `/?loginFailed=1`. The running `ctox.service` environment had `CTOX_BUSINESS_OS_REQUIRE_LOGIN=1` but did not load `CTOX_AUTH_USERS` from `/home/ubuntu/.config/ctox/business-os.env`; therefore `store::session()` could not authenticate any explicit login. Root fix implemented locally in `install.sh` and `src/core/install/mod.rs`: generated `ctox.service` now includes `EnvironmentFile=-%h/.config/ctox/business-os.env` and `EnvironmentFile=-%h/.config/ctox/business-bridge.env`, and generated `ctox` launch wrappers source `business-os.env` before `business-bridge.env`. Gates passed: `bash -n install.sh`, `cargo fmt --check`, `git diff --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `cargo check`. Production remains not ready until this is pushed, deployed via `ctox upgrade --dev`, login passes, and the full browser E2E matrix passes. |
| 2026-06-06 18:02 CEST | Phase 18 deployed auth-env source fix through `ctox upgrade --dev`, but runtime verification failed: the active release source contains both `business-os.env` and `business-bridge.env`, while the installed `/home/ubuntu/.local/bin/ctox` wrapper and `ctox.service` drop-in still load only `business-bridge.env`. The service process therefore still misses `CTOX_AUTH_USERS` and login cannot be considered fixed. Next action is to repair the installed wrapper/unit on the VPS, verify the service environment contains `CTOX_AUTH_USERS` without enabling any HTTP path, then continue browser E2E. Production remains not ready. |
| 2026-06-06 18:38 CEST | Phase 19 deployment/runtime repair completed: reran the required `ctox upgrade --dev` path and activated release `branch-main-20260606T162730Z` with backup `/home/ubuntu/.local/state/ctox/backups/update-20260606T162734Z`. The new release source contains the `business-os.env` wrapper/unit fix, but the upgrade was launched by the previous binary and rewrote installed wrapper/unit with the previous template after activation. Repaired the installed `/home/ubuntu/.local/bin/ctox`, `/home/ubuntu/.local/lib/ctox/bin/ctox`, `ctox.service`, and `ctox.service.d/business-os.conf` on the VPS so they load `business-os.env` before `business-bridge.env`; restarted `ctox.service`. Runtime verification: service is active, process env contains `CTOX_BUSINESS_OS_REQUIRE_LOGIN=1`, `CTOX_BUSINESS_USER`, `CTOX_BUSINESS_PASSWORD`, and `CTOX_BUSINESS_OS_DEFAULT_ROLE=admin`; `ctox status --json` reports `business_os.ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, WSS signaling, `native_rxdb_peer_status.running=true`, and `http_bridge_available=false`. `CTOX_AUTH_USERS` is not present, but the code supports the configured single-user `CTOX_BUSINESS_USER`/`CTOX_BUSINESS_PASSWORD` path. Production remains not ready until browser E2E passes. |
| 2026-06-06 19:31 CEST | Phase 20 isolated the current production blocker with a fresh Playwright profile: after login, `skf.ctox.dev` injected and used stale sync room `biz_4e32...`, while `ctox business-os peer status` on the VPS reports the active room `biz_4310...` with `http_bridge_available=false`. This proves the command queue break is a split RxDB/WebRTC room, not browser cache and not an HTTP fallback. Root fix started in `ctox-dev`: the tenant route no longer falls back to `health_payload.static_pairing_config` for pairing/module allowlists and no longer accepts POST `/api/business-os/commands`; TypeScript passes with the existing `--ignoreDeprecations 6.0` workaround. CTOX-side local RxDB cursor patch also passed `storage-index-smoke`, vendor build provenance, `assert-rxdb-only`, and JS syntax checks. Repeated open local smoke failure: `no-package-manager-import-smoke.mjs` still fails with `WebRTC peer ctox-peer closed: frame-send-channel-closed`, so that transport lifecycle test remains unresolved. Production remains not ready until the ctox-dev route fix is pushed/deployed, fresh browser receives the active VPS sync room, commands appear in the active RxDB `business_commands` table, and full browser E2E passes. |
| 2026-06-06 20:44 CEST | Phase 21 fixed the ctox.dev routing/data target issue and exposed the next blocker. Deployed ctox-dev commit `dbdfdc3` from a clean clone to Vercel deployment `ctox-464jkmh1p...`, moved explicit aliases `skf.ctox.dev` and `cto1.kunstmen.com` to it, and verified header `x-business-os-instance-route-build=20260606-fail-closed-rxdb-pairing`. The `skf` tenant still used old SSH credentials for host `57.129.123.108`; copied the known working `51.210.246.120` credential from the `cto1.kunstmen.com` tenant to `skf`, verified a fresh browser now receives active sync room `biz_4310...` with `http_bridge_available=false`, then removed the stale `57.129.123.108` credentials. New E2E blocker: with the correct room, the browser still stays at `Inhalte werden synchronisiert 0/6`; WebRTC connects, but initial replication remains pending, modules do not show data, and a commandBus dispatch times out without any new `cmd_e2e_*` reaching active server `business_commands`. Root fix started locally: remove `desktop_file_chunks` from shell-critical startup/readiness gates so 141k chunk rows cannot block login, navigation, or command dispatch; chunks remain lazy data for file/document views. Production remains not ready until this is pushed, deployed via `ctox upgrade --dev`, and fresh browser E2E proves data visibility plus real CTOX queue projection. |
| 2026-06-07 01:25 CEST | Phase 22 CTOX core deploy completed through the required `ctox upgrade --dev` path after RxDB/WebRTC send-queue fixes. Commit `1c38831a` is active as release `branch-main-20260606T225646Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260606T225646Z`; `systemctl --user is-active ctox.service` reports `active`; served browser marker is `20260606-rxdb-sendqueue-drain2`; served shared RxDB bundle marker is `ctox-rxdb-js`. This phase only proves deployment and service liveness. Production remains not ready until a fresh browser E2E proves login, data visibility, tenant scope, command queue processing, result navigation, bug reporter send, research continue, logout, and clean console/network behavior. |
| 2026-06-07 01:31 CEST | Phase 23 fresh-browser E2E still fails. Login succeeds and the page receives the active room, but the shell remains at `Inhalte werden synchronisiert 0/6`; required local counts stay empty (`ctox_runtime_settings=0`, `business_module_catalog=1`, `business_commands=0`, `ctox_queue_tasks=0`, `desktop_files=0`), Documents/Research collections are not registered, and tenant scope is still wrong (`21` visible modules). WebRTC diagnostics prove the DataChannel opens and connects, inbound native frames arrive (`receivedFrames>200`), and the Browser send queue drains (`priorityQueueDepth=0`, `sentScheduledFrames>0`), but initial replication remains `pending` and lifecycle reports `peer_connect_timeout`. This rules out the previous queue-not-draining symptom and moves the active blocker to the Browser/native RxDB response demux or replication protocol completion path. Screenshot evidence: `output/playwright/skf-after-sendqueue-drain2/after-sync.png`. Production remains not ready. |
| 2026-06-07 01:47 CEST | Phase 24 local root-cause fix implemented: DataChannel instrumentation showed the Browser was receiving hundreds of unsolicited `masterChangeStream$` pushes for `ctox_ticket_self_work_notes`, a collection not registered in the shell-critical startup set, while startup stayed at `0/6`. The Rust multiplexed native master relay now filters master-change pushes through the `rxdb.activeCollections` control plane: generic handlers keep default broadcast behavior, but `WebRTCRsConnectionHandler` only pushes a collection to a peer after that peer marked it active. Initial pull/request-response paths are unchanged. Local gates passed: `cargo test --manifest-path src/core/rxdb/Cargo.toml active_collection_predicate_tracks_control_plane_state -- --nocapture`, `cargo test --manifest-path src/core/rxdb/Cargo.toml active_collection_frame_is_high_priority_others_normal -- --nocapture`, `cargo test --manifest-path src/core/rxdb/Cargo.toml apply_active_collections_reprioritizes_queued_frames -- --nocapture`, `cargo fmt --check`, `cargo check`, `git diff --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs`. Production remains not ready until this is pushed, deployed through `ctox upgrade --dev`, and fresh browser E2E passes. |
| 2026-06-07 01:50 CEST | Phase 25 main push completed: commit `c2bce001` (`fix: gate rxdb master change pushes by active collection`) was pushed to `main` (`1c38831a..c2bce001`). Next action is the required production rollout through `ctox upgrade --dev`; production remains not ready until deployed and browser E2E passes. |
| 2026-06-07 02:18 CEST | Phase 26 deployment completed through the required `ctox upgrade --dev` path. Active release is `branch-main-20260606T234919Z`; previous release was `branch-main-20260606T225646Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260606T234922Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260606T234919Z`. Runtime verification reports `ctox.service` active, Business OS `ok=true`, `runtime=native-rust`, `http_bridge_available=false`, required shell collections ready, server-side module catalog `module_count=12`, and native RxDB peer running with a fresh heartbeat. Production remains not ready until fresh browser E2E passes. |
| 2026-06-07 02:22 CEST | Phase 27 fresh-browser E2E improved but still fails production readiness. Login succeeds; `ctox_runtime_settings`, `business_module_catalog`, and `business_commands` initial replication now complete; browser counts become `ctox_runtime_settings=1`, `business_module_catalog=1`, `business_commands=200`; console has no critical errors; no `/api/business-os/commands` request was observed. Remaining blockers: tenant scope still shows `21` modules while server catalog reports `12`, `ctox_queue_tasks` and `desktop_files` are still not registered in the startup probe, and Documents/Research data visibility plus command processing have not passed. Screenshot evidence: `output/playwright/skf-after-active-collection-gate/after-sync.png`. Production remains not ready. |
| 2026-06-07 02:30 CEST | Phase 28 local tenant-scope fix implemented. Production server verification shows active `business_module_catalog` has `module_count=12`, while the fresh browser still rendered `21` modules because the public WebDeploy shell inserted/merged the packaged `modules/registry.json` catalog into local RxDB before the real server projection governed startup. `app.js` now disables packaged module catalog seeding on public/non-local WebDeploy surfaces and only allows the server-projected RxDB catalog to define visible modules; the browser DB name was bumped to `ctox_business_os_v11` and the `app.js` cache key was bumped to prevent stale local widened catalogs from surviving. Local gates passed: `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `node --check src/apps/business-os/app.js`, and `git diff --check`. Production remains not ready until this is pushed, deployed through `ctox upgrade --dev`, and fresh browser E2E proves tenant scope, data visibility, command queue processing, result navigation, bug reporter send, research continue, logout, and console/network health. |
| 2026-06-07 02:59 CEST | Phase 29 deployed the tenant-scope fix through the required `ctox upgrade --dev` path. Commit `b0c3c621` is active as release `branch-main-20260607T003215Z`; previous release was `branch-main-20260606T234919Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260607T003219Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260607T003215Z`. Runtime verification reports `ctox.service` active, Business OS `ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `http_bridge_available=false`, fresh native RxDB peer heartbeat, data-plane ready, server-side `business_module_catalog.module_count=12`, served `app.js?v=20260606-rxdb-public-catalog1`, and browser DB marker `ctox_business_os_v11`. Production remains not ready until fresh browser E2E passes. |
| 2026-06-07 03:18 CEST | Phase 30 browser E2E after Phase 29 is improved but still not production ready. A fresh Playwright login reaches the authenticated CTOX UI, shows replicated CTOX task data, emits no `/api/business-os/commands` requests, and no page errors. Tenant scope is visibly narrower than the previous 21-module topbar, but the E2E found a remaining command-contract violation: opening CTOX triggers `ctox.source.list_snapshots` for the module version dropdown; the server completes that `business_commands` row without `task_id`, so the frontend correctly warns that no authoritative `ctox_queue_tasks` projection exists. Local fix implemented: `loadModuleVersionsDropdown` no longer dispatches the read-only snapshot listing through the task-backed RxDB command bus; it only reads `business_module_catalog.version_states`, which is already replicated over RxDB. Local gates passed: `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `node --check src/apps/business-os/app.js`, and `git diff --check`. Production remains not ready until this is pushed, deployed with `ctox upgrade --dev`, and fresh browser E2E passes command processing, data apps, logout, and console/network checks. |
| 2026-06-07 03:51 CEST | Phase 31 deployed the CommandBus cleanup through the required `ctox upgrade --dev` path. Commit `e1a2f4ad` is active as release `branch-main-20260607T012242Z`; previous release was `branch-main-20260607T003215Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260607T012246Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260607T012242Z`. Runtime verification reports `ctox.service` active, Business OS `ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, `http_bridge_available=false`, native RxDB peer running with fresh heartbeat, server-side `business_module_catalog.module_count=12`, and served `app.js?v=20260606-rxdb-public-catalog2`. Production remains not ready until fresh browser E2E passes. |
| 2026-06-07 04:42 CEST | Phase 32 auth/runtime verification and Research install diagnosis: deployed auth fixes are active on skf; the configured Web login user is now present in `business_users` and in the active RxDB `business_users` projection, so RxDB commands from that actor can be trusted as admin. The canonical App Store install command for `research` completed and wrote `src/apps/business-os/installed-modules/research/module.json`, but active `business_module_catalog` stayed at 12 modules and did not include `research`. Root cause: direct `ctox.app_store.install` mutates installed module files/version records but did not synchronously write the active RxDB `business_module_catalog` projection; the background catalog path was not sufficient for the production user story. Production remains not ready until the Core writeback fix is pushed, deployed, the install command is rerun, and browser E2E proves Web Research is visible with data. |
| 2026-06-07 04:54 CEST | Phase 33 local Core fix for module catalog writeback completed. `write_module_catalog_projection_to_rxdb` now writes the server module catalog directly into active `ctox_business_os__business_module_catalog__v0`, and install, rollback, and uninstall paths call it after successful module mutation. Added `direct_module_catalog_projection_includes_installed_modules`, proving an installed `research` module appears in the RxDB catalog. Gates passed after correcting the install-path omission: `cargo test configured_auth_user_is_trusted_for_rxdb_admin_commands --bin ctox -- --nocapture`, `cargo test direct_module_catalog_projection_includes_installed_modules --bin ctox -- --nocapture`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `git diff --check`, and `cargo fmt --check`. Next action: run `cargo check`, commit, push to `main`, deploy through `ctox upgrade --dev`, rerun the Research install command, verify catalog count includes `research`, then run fresh-browser E2E. |
| 2026-06-07 05:01 CEST | Phase 34 main push and production deployment completed. Commit `b46eab2e` (`fix: refresh rxdb module catalog after app install`) was pushed to `main` and deployed through the required `ctox upgrade --dev` path. Active release is `branch-main-20260607T044056Z`; previous release was `branch-main-20260607T032406Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260607T044100Z`; `ctox.service` is active; `ctox status --json` reports Business OS `ok=true`, `runtime=native-rust`, `sync.transport=webrtc`, active room `biz_4310...`, native RxDB peer running with fresh heartbeat, and `http_bridge_available=false`. Reran the canonical App Store install command for `research`; it completed, and active RxDB `business_module_catalog` now reports `module_count=13` with `research|installed-modules/research/index.html|installed`. Production is still not declared ready until fresh-browser E2E passes login, data visibility, tenant scope, task processing, Research continue, Bug Reporter send, result/progress navigation, logout, and console/network checks. |
| 2026-06-07 08:02 CEST | Phase 35 Knowledge projection corruption isolated and immediate production cleanup applied. Active `/home/ubuntu/.local/state/ctox/ctox.sqlite3` `knowledge_data_tables` contains only Outbound tables and no Drone/Research catalog rows; active `/home/ubuntu/.local/state/ctox/business-os-rxdb.sqlite3` still contained six stale `knowledge_tables` projection docs for `drone_bearing_design`: three `table:kdt-...` docs with 9/12/5 embedded rows and three old `parquet:*` docs pointing at obsolete release paths. This explains why the browser could show 9/12/5 or 0 instead of stable real data. Hard-deleting the rows was insufficient because browsers already had local RxDB copies; the six IDs were therefore rewritten as RxDB tombstones (`deleted=1`, `_deleted=true`, fresh `lastWriteTime`) after stopping `ctox.service`, with backup `business-os-rxdb.sqlite3.pre-tombstone-fake-drone-knowledge-*`, then `ctox.service` was restarted and is active. Local code fix started: `knowledge_tables` sync now tombstones visible docs not present in the current canonical Knowledge catalog, and Parquet projection row counts now report the full table count instead of capped embedded rows. Production remains not ready until the code fix is checked, pushed, deployed via `ctox upgrade --dev`, the real Drone data source is restored into the canonical Knowledge catalog, and browser E2E proves no 9/12/5 data reappears. |
| 2026-06-07 08:31 CEST | Phase 36 immediate server data repair completed. The only current server Parquet matching Drone research is `/home/ubuntu/.local/state/ctox/knowledge/data/drone_bearing_loads_25kg/source_catalog.parquet`; isolated CTOX inspection reports `row_count=12`, `bytes=9977`, and schema hash `4324e652b2f22f2baf4c95e57939ed7d4b2c28a36e0f64355530cc1e80640c67`. No server `ctox.sqlite3` backup contains Drone `knowledge_data_tables` rows, and all inspected `business-os-rxdb.sqlite3` backups contain only the same 9/12/5 `drone_bearing_design` projection, not 320/320. Registered the current Parquet as canonical `knowledge_data_tables` row `drone_bearing_loads_25kg/source_catalog`, corrected its title/description, tombstoned old `research_drone_bearing_design`, `research_run_drone_bearing_design_20260525`, and `research_note_drone_bearing_design_summary`, and retitled the active `research_drone_bearing_loads_25kg` task to `Drohnen-Lagerbelastungsdaten`. Post-restart verification: active `knowledge_tables` has `drone_bearing_loads_25kg/source_catalog` with `row_count=12` and `payload.rows=12`; `old_fake_active=0`; `old_research_active=0`. Production remains not ready until the code fix is pushed/deployed and browser E2E verifies the UI no longer shows 9/12/5 or stale local copies. |
| 2026-06-07 09:35 CEST | Phase 37 Web Research disappearance isolated. Fresh browser E2E after login shows the Desktop route rendering only the server-projected module catalog, and `research` is missing from the active RxDB `business_module_catalog` even though `modules/registry.json` in the active release contains `research`. Root cause: the Rust RxDB module-catalog projector only ships scopes `core`, `starter`, and `internal`; packaged `research` declares `install_scope=local`, which falls through to `store` unless the module id is explicitly classified as starter. Local fix: add `research` to `STARTER_MODULE_IDS` and add a regression test proving packaged `modules/research/module.json` is projected with `entry=modules/research/index.html`. Server data search across active SQLite, current Parquet files, and server backups found no 320/320 or 350+ Drone Knowledge rows; it found only active `drone_bearing_loads_25kg/source_catalog` with 12 rows and older stale `drone_bearing_design` 22/98/9 projections with embedded 9/12/5 rows. Production remains not ready until this catalog fix is tested, pushed, deployed via `ctox upgrade --dev`, the active module catalog includes `research`, and browser E2E proves Web Research visible with non-stale data. |
| 2026-06-07 10:18 CEST | Phase 38 active deployment still projected only 12 modules after commit `72295a1a` and `ctox upgrade --dev` release `branch-main-20260607T074401Z`. Live server inspection isolated the second catalog bug: active `current/runtime` is a symlink to `/home/ubuntu/.local/state/ctox`, and that state directory contains an old `/home/ubuntu/.local/state/ctox/business-os` app root from 2026-05-25. `resolve_business_os_app_root` checked `root/business-os` before the active release source, so the native RxDB peer projected the stale State app root instead of `/home/ubuntu/.local/lib/ctox/current/src/apps/business-os`. Local fix started: when the root path is a release `runtime` directory, prefer sibling `../src/apps/business-os` before any state-local app root; added a regression test with a stale runtime `business-os` and a current release `src/apps/business-os` proving `research` is included from the release catalog. Production remains not ready until this resolver fix is tested, pushed, deployed via `ctox upgrade --dev`, active RxDB `business_module_catalog` includes `research`, and fresh-browser E2E passes. |
| 2026-06-07 10:37 CEST | Phase 39 release-root fix deployed but active catalog remained at 12 modules on release `branch-main-20260607T082542Z`. Server inspection showed the release does contain `src/apps/business-os/modules/research/module.json`, but the manifest explicitly declares `install_scope=store`; `module_install_scope` returned that explicit store scope before checking `STARTER_MODULE_IDS`, so `research` stayed in `marketplace` instead of visible `modules`. Local fix: starter module ids now override explicit `store` scope to `starter`, and the two regression tests now use the real `install_scope=store` case. Gates passed: `cargo test module_catalog_projection_includes_packaged_research --bin ctox -- --nocapture`, `cargo test module_catalog_prefers_release_source_over_stale_runtime_app_root --bin ctox -- --nocapture`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `cargo fmt --check`, and `git diff --check`. Production remains not ready until this is pushed, deployed via `ctox upgrade --dev`, active RxDB `business_module_catalog` includes `research`, and fresh-browser E2E confirms Web Research plus data visibility. |
