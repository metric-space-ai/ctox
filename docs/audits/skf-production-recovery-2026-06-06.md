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
| 2026-06-07 11:52 CEST | Phase 40 deployed commit `ea7e212a` through the required `ctox upgrade --dev` path after one interrupted SSH run left only an orphan build and did not switch releases. The successful run applied release `branch-main-20260607T093218Z` with backup `/home/ubuntu/.local/state/ctox/backups/update-20260607T093222Z`; `current` resolves to that release; `ctox.service` and `ctox-business-os-web.service` are active. Server-side RxDB verification now reports `business_module_catalog.module_count=13` and `research|modules/research/index.html|starter`. Active `knowledge_tables` still has only the canonical Drone row `drone_bearing_loads_25kg` with `row_count=12` and `payload.rows=12`; old `drone_bearing_design`/9/12/5 fake rows active count is `0`; `http_bridge_available=false` and sync transport is `webrtc`. Production remains not ready until fresh-browser E2E proves the browser receives this catalog, shows Web Research, shows non-stale data, and does not get stuck in sync/readiness. |
| 2026-06-07 10:36 UTC | Phase 41 skf module scope repaired and desktop sync blocker isolated. Set persistent `CTOX_BUSINESS_OS_MODULE_ALLOWLIST=desktop,ctox,documents,knowledge,app-store,research,reports,explorer` in active `runtime/ctox-runtime.sqlite3`, restarted `ctox.service` and `ctox-business-os-web.service`, and verified active RxDB `business_module_catalog.allowed_module_ids` contains exactly those ids. Fresh browser E2E with a clean context now proves login succeeds, logout succeeds, logged-out reload is protected, Web Research routes correctly instead of CTOX Flow, Drone data shows `drone_bearing_loads_25kg · 12 rows`, `Sources (12)`, no `Sources (9)`, no `Sources (320)`, no empty state, no console errors, and no HTTP command fallback requests. Remaining blocker: Desktop first paint still takes too long and shows `Inhalte werden synchronisiert 5/6` because the Desktop module declared `desktop_files` and `desktop_file_chunks` as its own sync readiness collections; skf currently has `desktop_files=4172` and `desktop_file_chunks=141393`, while the Desktop icon launcher only needs module catalog, `desktop_icons`, `desktop_layout`, notifications, and command stream. Local Core fix: remove `desktop_files` and `desktop_file_chunks` from `modules/desktop/module.json` so file content sync remains lazy in Files/File Viewer instead of blocking Desktop readiness. Local gates passed: `node src/apps/business-os/modules/desktop/registry-launch-smoke.mjs`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Production remains not ready until this is committed, pushed to `main`, deployed via `ctox upgrade --dev`, and fresh-browser E2E proves Desktop icons render quickly without a stuck sync toast while Research data remains visible. |
| 2026-06-07 11:22 UTC | Phase 42 desktop lazy-sync fix deployed and Research readiness blocker narrowed. Commit `2757a533` was pushed to `main` and deployed through the required `ctox upgrade --dev` path as release `branch-main-20260607T104936Z`; `ctox.service` and `ctox-business-os-web.service` are active, `ctox status --json` reports WebRTC sync and `http_bridge_available=false`, and active `modules/desktop/module.json` declares only `business_commands`, `desktop_icons`, `desktop_layout`, and `desktop_notifications`. Fresh browser E2E after the deploy proves login works, Desktop renders the skf-scoped app set by 15s without a sync toast, disallowed apps (`Calendar`, `Notes`, `Spreadsheets`, `Browser`, `App Creator`, `Tickets`) are absent, Web Research routes correctly, Drone data shows `drone_bearing_loads_25kg · 12 rows` and `Sources (12)`, no `Sources (9)`, no `Sources (320)`, no empty Research state, no console errors, no failed requests, and logout clears the session. Remaining blocker from that E2E: Research still shows `Inhalte werden synchronisiert 7/10`; local fix now reduces `modules/research/module.json` startup collections to the six collections used by the initial Research workbench (`business_commands`, `ctox_queue_tasks`, `research_tasks`, `research_runs`, `research_notes`, `knowledge_tables`) so report/blob/chat side collections stay RxDB-only but lazy. Local gates passed: `jq empty modules/research/module.json`, `git diff --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `node src/apps/business-os/modules/desktop/registry-launch-smoke.mjs`. Production remains not ready until this Research manifest fix is committed, pushed to `main`, deployed via `ctox upgrade --dev`, and fresh-browser E2E proves Research no longer has a stuck sync toast while all previously passing checks remain green. |
| 2026-06-07 12:10 UTC | Phase 43 Research readiness deployment exposed the final stuck-toast collection. Commit `8b1bc43f` was pushed to `main` and deployed via `ctox upgrade --dev` as release `branch-main-20260607T112309Z`; active services are healthy, `http_bridge_available=false`, active Research manifest has six startup collections, and Desktop manifest remains lazy. Fresh browser E2E proves login, scoped Desktop apps, Web Research route, Drone `drone_bearing_loads_25kg · 12 rows`, `Sources (12)`, no `Sources (9)`, no `Sources (320)`, no empty Research state, no console errors, no `/api/business-os/commands` responses, and logout. Remaining blocker: Research toast stays at `5/6`. Browser sync diagnostics identify the only missing collection as `business_commands`; `ctox_queue_tasks`, `research_tasks`, `research_runs`, `research_notes`, and `knowledge_tables` are complete. Local fix pushed in `e68e0fea`: remove `business_commands` from the Research module catalog/readiness manifest. This does not remove the RxDB command path: `shared/command-bus.js` still starts `business_commands` and `ctox_queue_tasks` before dispatch, and `modules/research/index.js` still starts/subscribes command state for run tracking. Production remains not ready until this manifest-only readiness fix is deliberately deployed via `ctox upgrade --dev` and fresh-browser E2E proves Research has no stuck sync toast. |
| 2026-06-07 12:18 UTC | Phase 44 stopped an unreviewed follow-up deployment before activation. A new `ctox upgrade --dev` for `e68e0fea` was started, but after review it was stopped while still compiling and before switching `current`; active production remains `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260607T112309Z`, and both `ctox.service` and `ctox-business-os-web.service` are active. Additional code reading and browser diagnostics confirm the narrower cause: `app.js` computes the sync toast from the server-projected module catalog `collections`; active Research data collections are complete, and only `business_commands` is pending. The attempted Playwright asset override could not change the server-projected catalog, which confirms that any real fix must be projected through `business_module_catalog`, not by assuming `module.json` is fetched by the browser. Next allowed step before any compile/deploy: verify from source that the module catalog projector will pick up the `e68e0fea` manifest change and produce Research collections without `business_commands`; only then decide whether the expensive `ctox upgrade --dev` is justified. |
| 2026-06-07 12:49 UTC | Phase 45 source and server-state audit explains why the pushed manifest fix is not visible yet. Source proof: `store::module_catalog_for_rxdb()` resolves the Business OS app root, loads `modules/*/module.json` into `ModuleManifest { collections, ... }`, and serializes those manifests directly into the `business_module_catalog` document; the native peer calls `sync_module_catalog_with_database()` in a 3-second background loop. Server proof: the abnormally stopped `e68e0fea` release directory `branch-main-20260607T121118Z` exists and its `modules/research/module.json` is correct without `business_commands`, but `update_state.json` is still `phase=building`, `current` still points to `branch-main-20260607T112309Z`, and the old peer process started before the aborted upgrade keeps projecting from `current`/`112309Z`. The shared runtime DB therefore still contains the old Research catalog `["business_commands","ctox_queue_tasks","research_tasks","research_runs","research_notes","knowledge_tables"]`. Root cause of the remaining `5/6` sync toast is now the inconsistent aborted upgrade state plus old peer projection, not a need for HTTP fallback or SQLite data patching. Next safe action: complete the official `ctox upgrade --dev` path so wrapper, `current`, systemd services, peer root, app assets, and projected catalog converge on the same release; then run fresh-browser E2E before calling anything fixed. |
| 2026-06-07 16:07 CEST | Phase 46 skf data source and clean-browser E2E update. Completed the official `ctox upgrade --dev` path to release `branch-main-20260607T125050Z`; `ctox status --json` reports Business OS `ok=true`, native Rust runtime, WebRTC sync, fresh native peer, and `http_bridge_available=false`. Active Runtime RxDB `knowledge_tables` now contains only canonical `drone_bearing_loads_25kg` docs: `source_catalog=59`, `evidence_points=15`, and `evaluation_matrix=59`, each with matching embedded row counts; no active `drone_bearing_design`/9/12/5 projection remains. Fresh Playwright E2E proved login, scoped Desktop apps (`CTOX`, `Bugs & Features`, `Documents`, `Knowledge`, `Web Research`, `App Store`, `Files`), Web Research route, visible Research counts `Sources (59)`, `Measurements (15)`, `Knowledge (3)`, reload persistence, logout, no failed requests, and no Business data HTTP fallback requests. The E2E still correctly failed production readiness because the console logged `WebRTC replication failed for knowledge_tables: masterWrite conflicts remained`. |
| 2026-06-07 16:07 CEST | Phase 47 local fix for the `knowledge_tables` replication conflict. Source proof: `src/core/business_os/rxdb_peer.rs` documents `knowledge_tables` as the single native writer projection, while `src/apps/business-os/shared/sync.js` omitted `knowledge_tables` from `isReadOnlyProjectionCollection`, so the browser tried to push locally replicated Knowledge docs back to the native peer. Local fix marks `knowledge_tables` pull-only by adding it to `isReadOnlyProjectionCollection`; the RxDB-only guard now enforces this contract. Local gates passed: `node --check src/apps/business-os/shared/sync.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Production remains not ready until this fix is committed, pushed to `main`, deployed via `ctox upgrade --dev`, and fresh-browser E2E passes without console/network errors. |
| 2026-06-07 16:42 CEST | Phase 48 corrected the read-only push disable semantics after production E2E. Release `branch-main-20260607T140931Z` deployed successfully and served `shared/sync.js` with `knowledge_tables` in `isReadOnlyProjectionCollection`, but fresh Playwright E2E still failed on the same `masterWrite conflicts remained for knowledge_tables` console error while all UI assertions passed. Code reading showed why: `shared/sync.js` passed `push: undefined` for read-only collections, and `replicateWebRTC({ push = { batchSize: 10 } })` treats `undefined` as the default push configuration. Local fix changes read-only projection collections to pass `push: null`; the RxDB-only guard now enforces this so read-only projections cannot fall back into the default push path. Local gates passed: `node --check src/apps/business-os/shared/sync.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Production remains not ready until this second fix is committed, pushed, deployed via `ctox upgrade --dev`, and clean-browser E2E passes with no console errors. |
| 2026-06-07 17:14 CEST | Phase 49 deployment and server-state verification completed for commit `593201ad`. Commit `593201ad` (`fix: disable browser push for read-only projections`) was pushed to `main` and deployed through the required `ctox upgrade --dev` path as release `branch-main-20260607T144421Z`; `ctox.service` and `ctox-business-os-web.service` are active. The installed release file and live `https://skf.ctox.dev/shared/sync.js?v=20260607-outbound-rxdb-main1` both contain `push: isReadOnlyProjectionCollection(collection) ? null : { batchSize }` and include `knowledge_tables` in the read-only projection collection list. `ctox status --json` reports Business OS `ok=true`, native Rust runtime, WebRTC sync, fresh native peer, `http_bridge_available=false`, and four old pending CTOX spill-restore tasks unrelated to the Research knowledge projection. Active Runtime RxDB `knowledge_tables` contains only canonical Drone rows for `drone_bearing_loads_25kg`: `source_catalog=59`, `evidence_points=15`, and `evaluation_matrix=59`, each with matching embedded row counts; no active `drone_bearing_design` rows are present. Production is still not declared ready until the next fresh-browser Playwright E2E passes login, scoped Desktop apps, Research data visibility, reload persistence, logout, no Business data HTTP fallback requests, and no console/network sync errors. |
| 2026-06-07 17:45 CEST | Phase 50 clean-browser production E2E passed for `skf.ctox.dev` after release `branch-main-20260607T144421Z`. Playwright ran with a fresh persistent profile and screenshots/report in `/private/tmp/ctox-skf-e2e-593201ad`: `01-login.png`, `02-desktop.png`, `03-research.png`, `04-research-reload.png`, `05-desktop-reload.png`, `06-after-logout.png`, and `report.json`. Passed user stories: fresh profile starts logged out; login sets the server session cookie; authenticated Desktop shows only the scoped apps (`CTOX`, `Bugs & Features`, `Documents`, `Knowledge`, `Web Research`, `App Store`, `Files`) and does not show forbidden apps/modules (`Calendar`, `Notes/Notizen`, `Spreadsheets`, `Browser`, `App Creator`, `Tickets`, `Coding Agents`, `Requirement Matching`, `Outbound`, `Shiftflow`, `Buchhaltung`, `IoT`); Web Research shows canonical `drone_bearing_loads_25kg` data with `Sources (59)`, `Measurements (15)`, `Knowledge (3)`, and `Fachberichte (12)`; Research reload preserves the same counts; returning to Desktop remains scoped; logout returns to the login gate; `#research` after logout remains protected. The run recorded `pageErrors=0`, `requestFailures=0`, `businessHttpRequests=0`, and no console errors/warnings, including no `knowledge_tables` `masterWrite` conflict. Within this verified scope, `skf.ctox.dev` is production-ready for login/logout, scoped app visibility, and Research data visibility. Remaining work outside this E2E scope: clean or classify the four old CTOX spill-restore pending tasks shown by `ctox status --json`, and run equivalent production E2E on `cto1.kunstmen.com` before declaring the CTOX-general rollout complete. |
| 2026-06-07 18:31 CEST | Phase 51 browser Chat command E2E isolated the next production blocker. A real clean-browser Chat submission with marker `E2E_COMMAND_BUS_1780848808875` used the standard UI, did not call any `/api/business-os/commands` HTTP fallback, inserted `business_commands.cmd_8740866b-8394-4bab-bfee-13534484913f`, and produced authoritative queue task `queue:system::67b14f988cfff87c650c38f7` on the server. CTOX leased the task, so the RxDB command bus and native queue ingestion were proven for that run. The task then failed in the worker because the runtime still selected `chatgpt_subscription` auth with an expired/reused refresh token even though `OPENAI_API_KEY` exists; logs show `OPENAI_API_KEY ignored` and `Your access token could not be refreshed because your refresh token was already used`. This is not production ready: auth selection must be corrected and refresh-token failures must not be treated as retryable worker progress. |
| 2026-06-07 18:31 CEST | Phase 52 production runtime auth mode repair applied, but command-path E2E remains unproven after the service restart. Backed up the runtime DB, set `OPENAI_AUTH_MODE=api_key` and `CTOX_OPENAI_AUTH_MODE=api_key` in `runtime_env_kv`, restarted `ctox.service`, and verified `ctox_runtime_settings.auth.mode=api_key`, `api_key_configured=1`, `subscription_selected=0`; `ctox status --json` still reports Business OS ok, native Rust runtime, WebRTC sync, fresh RxDB peer, `http_bridge_available=false`, and four unrelated old spill-restore pending tasks. A second browser Chat submission with marker `E2E_APIKEY_COMMAND_1780849443755` showed local UI tracking without HTTP fallback, but server SQL did not find that marker in `business_commands` or `ctox_queue_tasks` before the browser context was closed. Current blocker: rerun the Chat E2E with the browser kept open until command-bus success/failure timeout, then prove whether post-restart RxDB push reaches the server and whether the API-key worker completes or fails with a specific terminal error. |
| 2026-06-07 19:28 CEST | Phase 53 long browser Chat command E2E completed with marker `E2E_APIKEY_LONG_1780850871241`. Fresh Chrome profile logged in, opened the standard Business OS chat, submitted through the UI, and observed a real task state (`running`) without any `/api/business-os/commands` HTTP fallback, page errors, console errors/warnings, or failed requests. Server proof: `business_commands.cmd_82430e83-9d80-4c26-a7d8-e83734dbb89d` was accepted and linked to authoritative queue task `queue:system::21ac475b8bf1e9ca0b113d26`; CTOX leased and ran the task. Worker terminal blocker is now the configured OpenAI API key itself: logs show `401 Unauthorized` / `invalid_api_key`. Separately, the canonical Business Store command row is correctly `failed`, but active RxDB `ctox_business_os__business_commands__v0` and `__v1` still show `accepted/queued`, so the browser can display stale task state after terminal failure. |
| 2026-06-07 19:45 CEST | Phase 54 local schema-drift fix implemented for the stale command projection blocker. Live SQLite inspection showed internal RxDB metadata has only `collection|business_commands-1`, but both `ctox_business_os__business_commands__v0` and `__v1` contain 990 rows; this contradicts the migration-version guard expectation that stale v0 remains empty. Root code defect: `repair_optional_rxdb_collection_schema_drift` was a No-op even though the status advertised repair availability. Local fix adds real stale version table detection from the RxDB internal store, drops obsolete collection tables only when the active internal meta version equals the expected schema version and the expected table exists, runs that cleanup before native peer startup, and treats SQLite primary-key/unique projection conflicts as recoverable upsert conflicts. Focused gates passed: `cargo fmt --check --manifest-path Cargo.toml`, `cargo test --locked --bin ctox projection_upsert_recovers_from_tombstone_conflict --quiet`, and `cargo test --locked --bin ctox rxdb_schema_drift_repair_drops_stale_version_table_after_active_meta_upgrade --quiet`. Production remains not ready until this is pushed to `main`, deployed via `ctox upgrade --dev`, skf stale v0 is removed by the fixed repair/startup path, command projection catches up to terminal failure, and browser E2E is rerun. |
| 2026-06-07 20:14 CEST | Phase 55 production trigger drift isolated and runtime cleaned. Deploying `e88710ea` through `ctox upgrade --dev` applied release `branch-main-20260607T172729Z`; startup removed 6 stale RxDB schema tables and left Business OS healthy with native Rust runtime, WebRTC sync, WSS signaling, and `http_bridge_available=false`. Follow-up SQLite inspection found the remaining projection failure: stale triggers `sync_commands_v1_to_v0_insert` and `sync_commands_v1_to_v0_update` on `ctox_business_os__business_commands__v1` still wrote into deleted `ctox_business_os__business_commands__v0`, producing `no such table: main.ctox_business_os__business_commands__v0`. The two invalid triggers were dropped on the skf runtime DB; after the next sync, command `cmd_82430e83-9d80-4c26-a7d8-e83734dbb89d`, its canonical Business Store row, and queue task `queue:system::21ac475b8bf1e9ca0b113d26` all converged to terminal `failed/failed` with the same worker error. No new projection errors appeared in the checked log window and `ctox status --json` reported `errorTotal=0`. Production task completion remains blocked by the configured invalid OpenAI API key, not by command ingestion or projection sync. |
| 2026-06-07 20:14 CEST | Phase 56 source fix for stale RxDB trigger cleanup implemented and locally verified. `repair_stale_rxdb_collection_schema_versions` now detects and drops stale collection-version triggers before dropping obsolete collection tables, and startup repair logs include both repaired table and trigger counts. The regression test now seeds a v1-to-v0 trigger, proves dry-run reports it, applies repair, then verifies a write to active `business_commands__v1` no longer references deleted `business_commands__v0`. Gates passed: `cargo fmt --manifest-path Cargo.toml`, `cargo fmt --check --manifest-path Cargo.toml`, `cargo test --locked --bin ctox rxdb_schema_drift_repair_drops_stale_version_table_after_active_meta_upgrade --quiet`, and `git diff --check`. Production remains not ready until this trigger repair is committed, pushed to `main`, deployed via `ctox upgrade --dev`, and a fresh-browser command E2E confirms terminal task status propagation. |
| 2026-06-07 21:30 CEST | Phase 57 trigger cleanup source fix deployed and server state verified. Commit `f21ebca6` (`fix: repair stale rxdb schema triggers`) was pushed to `main` and deployed through the required `ctox upgrade --dev` path as release `branch-main-20260607T181802Z`; previous release was `branch-main-20260607T172729Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260607T181806Z`. `ctox.service` and `ctox-business-os-web.service` are active. `ctox status --json` reports Business OS `ok=true`, native Rust runtime, WebRTC sync, WSS signaling, fresh native RxDB peer, `health.errorTotal=0`, and `sync.http_bridge_available=false`. Active Runtime RxDB now has only `ctox_business_os__business_commands__v1`, no `business_commands` v0 table, no stale command triggers, and only `collection|business_commands-1` in RxDB internal metadata. The previously stuck command/queue pair stayed converged at terminal `failed/failed`, and no projection `no such table` errors appeared in the checked post-deploy logs. |
| 2026-06-07 21:30 CEST | Phase 58 clean-browser E2E after `f21ebca6` split pass/fail clearly. Passed: login works; Desktop shows the skf-scoped app set (`CTOX`, `Bugs & Features`, `Documents`, `Knowledge`, `Web Research`, `App Store`, `Files`) and forbidden apps remain absent; Web Research opens from the Desktop tile via double-click; Research shows canonical `drone_bearing_loads_25kg` data with `Sources (59)`, `Measurements (15)`, `Knowledge (3)`, and `Fachberichte (12)`; reload preserves those Research counts; old fake `9/12/5` data is not shown; no Business HTTP fallback requests were observed. Failed and active blocker: a fresh standard Chat submission with marker `E2E_CHAT_ONLY_1780860068231` remained visible in the browser as `pending_sync` for four minutes, while server-side active RxDB `business_commands` and canonical Business Store contained no row for that marker and no authoritative CTOX queue task was created. Console/network capture had no page errors and no Business HTTP fallback. Current root-cause target is therefore browser-to-server RxDB push for `business_commands`, not stale SQLite triggers or Research data projection. Production is not ready for Chat/task processing until this is fixed and proven by Browser E2E. |
| 2026-06-07 22:08 CEST | Phase 59 command-push root cause isolated and fixed locally. Direct browser instrumentation showed an intermittent push-loss condition inside the browser IndexedDB storage: a newly written local `business_commands` row could carry `_meta.lwt` below the current WebRTC push checkpoint, so `getChangedDocumentsSince(checkpoint, ..., { excludeReplicationOriginRole: 'ctox_instance' })` returned zero local docs even though the command existed locally. A later direct diagnostic marker `DIAG_DIRECT_1780861742229` did reach active server RxDB and authoritative `ctox_queue_tasks`, proving the single CTOX command bus can work; that task failed only in worker execution because the configured server OpenAI API key is invalid. Local fix in `storage-indexeddb.mjs` now assigns non-replication browser writes a monotonic LWT above the current collection maximum while preserving server-origin LWTs. Checks passed: `node --check` for source and shipped bundle, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and a real Chromium IndexedDB regression test where a local command written after a server-origin doc at LWT `2000` was assigned LWT `2001` and became visible to `getChangedDocumentsSince()`. Production is still not ready until this is committed, pushed, deployed through `ctox upgrade --dev`, and clean-browser Chat E2E proves command creation, server queue projection, terminal status propagation, no HTTP fallback, and no stuck sync/data regressions. |
| 2026-06-07 22:16 CEST | Phase 60 local pre-push gates for the LWT fix passed. While running `storage-index-smoke`, an existing source/dist drift was exposed: source already exported and used `replicationScanLimit`, but the shipped `dist/ctox-rxdb-js.mjs` did not. The shipped bundle was brought back into parity with the source by adding the replication scan-limit constants, scan cutoff, helper, and test-internals export. Gates passed after that correction: `node --check src/apps/business-os/rxdb/src/storage-indexeddb.mjs`, `node --check src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`, `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs`, `node src/scripts/vendor-builds/build-ctox-rxdb-js.mjs`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `git diff --check`, and the real Chromium IndexedDB regression against the shipped bundle. No Rust compile was run because this phase changed only browser RxDB JS and the audit document. |
| 2026-06-07 22:20 CEST | Phase 61 main push completed for the browser RxDB LWT fix. Commit `930d3e83` (`fix: preserve local rxdb command push order`) was pushed to `main` (`f21ebca6..930d3e83`). The commit scope is limited to browser RxDB storage/source-dist parity and the recovery tracker. Next action is the required production rollout through `ctox upgrade --dev`; production remains not ready until post-deploy server checks and clean-browser E2E pass. |
| 2026-06-07 22:22 CEST | Phase 62 rollout was stopped on request before activation. `ctox upgrade --dev` downloaded target release `branch-main-20260607T200939Z` from current `main` and entered installer build phase, but was stopped before switching `current`. Verified after stopping: `current` still points to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260607T181802Z`, `update_state.json` is `phase=failed` with target `branch-main-20260607T200939Z`, and both `ctox.service` and `ctox-business-os-web.service` are active. Therefore the browser RxDB LWT fix is pushed to `main` but is not active on `skf.ctox.dev` yet. |
| 2026-06-08 07:12 CEST | Phase 63 required rollout path completed after the later `ctox upgrade --dev` run. Active server release is now `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260607T203003Z`; `update_state.json` is `phase=completed`, `current_release=target_release=branch-main-20260607T203003Z`, `previous_release=branch-main-20260607T181802Z`, `last_error=null`, and both `ctox.service` plus `ctox-business-os-web.service` are active. `ctox status --json` reports Business OS `ok=true`, native Rust runtime, WebRTC sync room `ctox-business-os:biz_4310f6bf-fce6-4fec-ba09-700d9e20b752:UF1KWuJm8icD4SW2rn4ISy`, fresh native RxDB peer, and `http_bridge_available=false`. The current release bundle and live `https://skf.ctox.dev/rxdb/dist/ctox-rxdb-js.mjs` contain the LWT fix markers `localWriteLwtFloor`, `latestCollectionLwtInTransaction`, and `replicationScanLimit`. |
| 2026-06-08 07:12 CEST | Phase 64 browser/server patch verification completed. A real authenticated Playwright run against `https://skf.ctox.dev` used the Business OS RxDB command bus with marker `E2E_LWT_RETRY_1780895112404` and recorded no `/api/business-os/commands` or other Business command HTTP fallback requests. Browser state had `hasApp=true`, `hasDb=true`, `hasCommandBus=true`, `sessionAuth=true`, and the expected WebRTC sync room. Server-side active RxDB contains `business_commands.id=cmd_e2e_lwt_retry_1780895112404` with `lastWriteTime=1780895260223.31005`; active RxDB also contains authoritative queue task `queue:system::99e01aad8dd7119f41cdfdd8` titled `Patch verification E2E_LWT_RETRY_1780895112404` with `lastWriteTime=1780895261195.83007`. `journalctl` proves CTOX picked up the queue task and started a prompt worker at `2026-06-08 05:06:12 UTC`. The worker then failed with `401 Unauthorized` / `invalid_api_key`, so the LWT patch is verified for browser-command ingestion and queue projection, but the complete task-execution user story remains not production ready until the server OpenAI API key/runtime auth is fixed. The Playwright run also saw transient 502/dynamic-import startup failures for JS assets before the command evidence; those asset startup errors remain a separate production-readiness blocker to re-test after the worker auth fix. |
| 2026-06-08 09:03 CEST | Phase 65 corrected the SKF Drone Research source-of-truth mismatch. The explicit expected source is now `/Users/michaelwelsch/Documents/ctox_research_dashboard.html`, whose extracted arrays contain `rawSources=322`, `rawMeasurements=816`, `rawLibrary=192`, and `documents=13`. Before repair, active CTOX Knowledge had no `drone_bearing_design` tables and only the reduced replacement domain `drone_bearing_loads_25kg` with `source_catalog=59`, `evidence_points=15`, and `evaluation_matrix=59`. Created backup `/home/ubuntu/.local/state/ctox/backups/manual-skf-dashboard-html-restore-20260608T064033Z`; imported `drone_bearing_design/source_catalog=322`, `drone_bearing_design/measured_load_points=816`, and `drone_bearing_design/load_data_library=192` through `ctox knowledge data` using the local HTML export. The reduced `drone_bearing_loads_25kg` tables were archived, leaving `ctox knowledge data list --domain drone_bearing_loads_25kg` at `count=0`. Active RxDB `knowledge_tables` now has `drone_bearing_design` rows with `row_count` and embedded row counts `322/816/192`, while all `drone_bearing_loads_25kg` projections are tombstoned. |
| 2026-06-08 09:03 CEST | Phase 66 restored the visible Research task and fixed the UI count regression locally. Active RxDB `research_tasks` now has `research_drone_bearing_design` undeleted, `status=ready`, `knowledge_domain=drone_bearing_design`, keys `source_catalog`, `load_data_library`, and `measured_load_points`; `research_drone_bearing_loads_25kg` is tombstoned/archived. Local Research UI fix changes `ROW_LIMIT` from `320` to `5000` and changes the Knowledge tab badge from table count to `state.curatedRows.length`, so the HTML dataset can render `Sources (322)`, `Measurements (816)`, and `Knowledge (192)` over RxDB instead of `322/320/3`. Local gates passed: `node --check src/apps/business-os/modules/research/index.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Production remains not fully verified until this UI fix is pushed to `main`, deployed through `ctox upgrade --dev`, and a fresh-browser E2E confirms the visible SKF UI counts and scoped app set. |
| 2026-06-08 09:55 CEST | Phase 67 deployed and browser-verified the SKF Drone Research restoration. Commit `0d060c5f` (`fix: show full skf research counts`) was pushed to `main` and deployed through `ctox upgrade --dev` as active release `branch-main-20260608T070631Z`; `ctox.service` and `ctox-business-os-web.service` are active. Server truth now matches `/Users/michaelwelsch/Documents/ctox_research_dashboard.html`: `ctox knowledge data count` returns `source_catalog=322`, `measured_load_points=816`, and `load_data_library=192`; `ctox knowledge data list --domain drone_bearing_loads_25kg` returns `count=0`. Active RxDB `knowledge_tables` contains only live `drone_bearing_design` rows with embedded row counts `322/816/192`; old `drone_bearing_loads_25kg` rows are tombstoned. Playwright authenticated against `https://skf.ctox.dev/#research`, reloaded the page, and verified visible UI assertions: `Web Research` present, `Drohnen-Lagerbelastungsdaten`, `drone_bearing_design · 1.330 rows`, tabs `Sources (322)`, `Measurements (816)`, `Knowledge (192)`, right-side metrics `322 Sources` and `816 Measurements`, status `ready`, and no visible `Sources (9)`, `Measurements (12)`, `Knowledge (5)`, `129 rows`, or `drone_bearing_loads_25kg`. Console check reported `Errors: 0, Warnings: 0`; network capture showed only static app/module requests and no Business data HTTP fallback. Evidence screenshot: `output/playwright/skf-research-322-816-192-post-reload.png`. This resolves the SKF Web Research data-visibility regression; overall skf production readiness still depends on the separate Chat/task worker E2E and configured worker auth issues tracked above. |
| 2026-06-08 10:34 CEST | Phase 68 MiniMax proxy routing fix implemented locally. The skf instance runtime DB/env now targets `MiniMax-M3` through `https://llm.ctox.dev`, but the active code still inferred the `llm.ctox.dev` upstream as OpenAI and selected the wrong API-key name for direct-session, prompt-worker, and `ctox_runtime_settings` projection. Local source fix maps `llm.ctox.dev` and `/api/fallback-llm` upstreams to provider `minimax`, uses `CTOX_LLM_PROXY_API_KEY` when the MiniMax provider is routed through the CTOX LLM proxy, and projects that same auth key into Business OS runtime settings. Changed files: `src/core/execution/models/runtime_state.rs`, `src/core/execution/agent/turn_loop.rs`, `src/core/execution/agent/direct_session.rs`, and `src/core/business_os/store.rs`. Gates passed: `cargo fmt --check`, `git diff --check`, and `cargo test --locked infer_api_provider_uses_proxy_key_for_ctox_llm_proxy`. Production remains not ready for Chat/task execution until this is pushed to `main`, deployed through `ctox upgrade --dev`, and browser E2E proves the worker no longer calls `api.openai.com` and completes or fails only with a MiniMax/proxy-specific terminal error. |
| 2026-06-08 11:18 CEST | Phase 69 MiniMax-M3 registry blocker isolated and fixed locally. Commit `4970e086` was pushed and deployed through `ctox upgrade --dev` as release `branch-main-20260608T093422Z`, and active runtime state showed `MiniMax-M3` plus `https://llm.ctox.dev`; however live CLI task `E2E_MINIMAX_PROXY_CLI_1780913100` still called `https://api.openai.com/v1/responses`. Code reading found the precise reason: `MiniMax-M3` was absent from `SUPPORTED_MINIMAX_API_CHAT_MODELS` and the remote chat family map, so `api_provider_supports_model("minimax", "MiniMax-M3")` rejected the configured provider and DirectSession fell back to the built-in OpenAI provider. Local fix adds `MiniMax-M3` to the MiniMax API registry, supported chat list, family mapping, and adapter fallback model; added regression test `minimax_m3_proxy_settings_resolve_core_api_provider` proving `MiniMax-M3` + `CTOX_UPSTREAM_BASE_URL=https://llm.ctox.dev` resolves to `ctox_core_api` with base URL `https://llm.ctox.dev/v1` and proxy key `CTOX_LLM_PROXY_API_KEY`. Gates passed: `cargo fmt --check`, `git diff --check`, `cargo test --locked minimax_m3_proxy_settings_resolve_core_api_provider`, `cargo test --locked recognizes_minimax_m3_api_chat_model`, and `cargo test --locked infer_api_provider_uses_proxy_key_for_ctox_llm_proxy`. Production remains not ready until this second fix is pushed to `main`, deployed via `ctox upgrade --dev`, and a live worker/browser E2E confirms no `api.openai.com` request remains. |
| 2026-06-08 13:01 CEST | Phase 70 MiniMax-M3 proxy code path deployed and live worker blocker narrowed to proxy authorization. Commit `5ef97566` (`fix: support minimax m3 proxy runtime`) was pushed to `main` and deployed through `ctox upgrade --dev` as active release `branch-main-20260608T102146Z`; previous release was `branch-main-20260608T093422Z`, backup `/home/ubuntu/.local/state/ctox/backups/update-20260608T102150Z`. Server checks show `ctox.service=active`, `ctox-business-os-web.service=active`, active runtime `source=api`, `base_model=requested_model=active_model=MiniMax-M3`, `upstream_base_url=https://llm.ctox.dev`, and Business OS runtime projection `provider=minimax`, `chat_model=MiniMax-M3`, `upstream_base_url=https://llm.ctox.dev`. Live CLI worker task `E2E_MINIMAX_PROXY_CLI_1780915891` no longer called `api.openai.com`; journal proof shows `[ctox direct-session] provider mode=ctox_core_api id=ctox_core_api base_url=https://llm.ctox.dev/v1 wire_api=responses`. The worker still failed terminally because the proxy returned `403 Forbidden` with `Fallback-LLM ist fuer diese Instanz nicht freigeschaltet.` Direct proxy probes with the stored `CTOX_LLM_PROXY_API_KEY` also returned the same 403, so the remaining blocker is not RxDB, command ingestion, or OpenAI fallback; it is `llm.ctox.dev` authorization/freischaltung for this skf instance/key. Chat/task execution is therefore still not production-ready until the proxy key/instance is enabled and a browser E2E proves standard Chat submission reaches CTOX, runs through `llm.ctox.dev`, and completes. |
| 2026-06-08 14:04 CEST | Phase 71 ctox.dev proxy authorization fixed and deployed. Correct repo was identified as `/Users/michaelwelsch/Documents/ctox-dev-llm-prod-deploy`; commit `e4250c6` (`fix: allow explicit byo llm proxy tokens`) was pushed to `main` and deployed to Vercel production deployment `dpl_GCDcgTwpmM5NWLKX4kqM2DzGCKMd` for `ctox.dev`/`llm.ctox.dev`. The proxy policy now permits active `byo_vps` tenants only when explicitly enabled with a tenant-scoped token hash; managed-fleet cloud-token eligibility remains constrained to active service orders and fleet plans. The `skf` tenant now has `fallback_llm_enabled=true`, `fallback_llm_model=MiniMax-M3`, and a rotated tenant token installed in the skf CTOX secret store as `CTOX_LLM_PROXY_API_KEY`. A direct probe from the skf VPS to `https://llm.ctox.dev/v1/responses` using the stored secret returned HTTP 200 with model `MiniMax-M3`. |
| 2026-06-08 14:04 CEST | Phase 72 CTOX core runtime secret blocker isolated and fixed locally. After Phase 71, direct proxy access succeeded but `ctox chat --wait` still failed in the worker with `403 Forbidden`; journal proof showed the worker used `https://llm.ctox.dev/v1`, so routing was correct. Code reading found the precise core defect: `CTOX_LLM_PROXY_API_KEY` was not listed in `src/core/secrets.rs` `SECRET_KEYS`, so runtime secret resolution did not treat the proxy token as an encrypted credential. Local fix adds `CTOX_LLM_PROXY_API_KEY` to `SECRET_KEYS` and regression test `llm_proxy_api_key_is_treated_as_encrypted_credential`. Gates passed: `cargo fmt --check`, `git diff --check`, `cargo test --locked llm_proxy_api_key_is_treated_as_encrypted_credential`, `cargo test --locked infer_api_provider_uses_proxy_key_for_ctox_llm_proxy`, and `cargo test --locked minimax_m3_proxy_settings_resolve_core_api_provider`. Production remains not ready until this core fix is pushed to `main`, deployed with `ctox upgrade --dev`, and CLI plus browser E2E prove chat tasks are created, processed through CTOX, and complete through `llm.ctox.dev` without HTTP Business OS fallback. |
| 2026-06-08 15:32 CEST | Phase 73 core secret fix deployed through the required `ctox upgrade --dev` path after clearing a real VPS disk-space blocker. First rollout of commit `427cb7d7` (`fix: load llm proxy token from secret store`) failed at installer copy with `No space left on device`; `/` was 100% full with only 640 MB available. Safe cleanup removed only CTOX source/download caches under `~/.cache/ctox`, freeing `/` to 19 GB available without touching active releases, state DBs, RxDB, or browser data. The second `ctox upgrade --dev` completed and activated release `branch-main-20260608T124021Z`; previous release was `branch-main-20260608T102146Z`; backup `/home/ubuntu/.local/state/ctox/backups/update-20260608T124025Z`. Post-upgrade `/` had 22 GB free, `ctox.service` and `ctox-business-os-web.service` were active, Business OS reported `ok=true`, native Rust runtime, WebRTC room `biz_4310...`, and `http_bridge_available=false`. |
| 2026-06-08 15:32 CEST | Phase 74 llm.ctox.dev gateway compatibility fixed and deployed. With the core secret fix active, live CLI worker requests used `[ctox direct-session] provider mode=ctox_core_api id=ctox_core_api base_url=https://llm.ctox.dev/v1 wire_api=responses`, proving the old OpenAI/403 path was gone. The proxy then exposed two MiniMax compatibility errors: it rejected `parallel_tool_calls=false`, then rejected OpenAI built-in tools in the `tools` array. In `/Users/michaelwelsch/Documents/ctox-dev-llm-prod-deploy`, commits `c70c2b6` (`fix: omit minimax parallel tool flag`) and `e8d1fdb` (`fix: filter minimax unsupported tools`) were pushed to `main` and deployed to Vercel production as `dpl_DmegyQ3LZed35TAx61co2A8MJ42R` and `dpl_HFQsUPKvdtGF3jrNNF8dB1dF8eJ3`. Gates passed for both changes: fallback proxy guard, fallback Responses state test, fallback Responses route test, ESLint on changed files, and `npm run typecheck`. Direct probes from skf to `https://llm.ctox.dev/v1/responses` using the tenant secret now return HTTP 200 for payloads containing `parallel_tool_calls=false` and OpenAI built-in tools. |
| 2026-06-08 15:32 CEST | Phase 75 live CTOX worker execution passed at the LLM path. Fresh `ctox chat` marker `Healthcheck 20260608-1528` started a prompt worker, used `https://llm.ctox.dev/v1`, did not call `api.openai.com`, did not hit 403, did not hit the `parallel_tool_calls` or `tools` gateway errors, and ended with `ctox prompt worker end source=tui ok` in `journalctl`. `ctox status --json` after the run reported `running=true`, `busy=false`, `worker_active_count=0`, Business OS `ok=true`, WebRTC active, `health.errorTotal=0`, and `http_bridge_available=false`. Remaining production-readiness work: the CLI `--wait` client still times out after the worker has already ended successfully (`ctox service request error: failed to flush service socket response` appears repeatedly), four old `spill restore` pending tasks remain as cleanup debt, and the browser Business OS chat/RxDB user story still needs fresh Playwright E2E proof. |
| 2026-06-08 16:01 CEST | Phase 76 ctox.dev repo/token and browser-chat queue completion verified. The correct `ctox.dev` / `llm.ctox.dev` repo is `/Users/michaelwelsch/Documents/ctox-dev-llm-prod-deploy`, remote `git@github.com:mkh-welsch/ctox-dev.git`, currently at `origin/main` commit `e8d1fdb` with the BYO-VPS token authorization and MiniMax payload sanitizer fixes deployed. On the skf VPS, `ctox secret get --scope credentials --name CTOX_LLM_PROXY_API_KEY` returns a present `ctox_llm...` tenant token in the encrypted CTOX credential store. A direct skf-to-`https://llm.ctox.dev/v1/responses` probe using that stored secret returns HTTP 200, model `MiniMax-M3`, even with `parallel_tool_calls=false` and a built-in OpenAI tool in the client payload. The previous browser-chat marker `BROWSER_CHAT_COORD_1780926384336` is now terminal: active RxDB `business_commands` row `cmd_4995e1dc-040a-4a73-98c7-324d9c0a6a72` is `completed/completed/handled`; active RxDB and Business Store queue task `queue:system::c195ed36b0cc37d16acea535` is `completed/handled` with status note `business-os:terminal-success: chat reply stored`; canonical `communication_routing_state` is `handled` with `acked_at=2026-06-08T13:52:39Z`. This proves the standard Browser Chat path can write through RxDB into the single CTOX queue and be processed through `llm.ctox.dev`. Production readiness still requires a fresh clean-browser run that repeats login, Research/Documents/Knowledge data visibility, chat task completion, result/focus buttons, logout, and console/network checks. |
| 2026-06-08 16:34 CEST | Phase 77 fresh-browser Research data visibility blocker reproduced and fixed locally. Corrected Playwright login selector proved login and authenticated reload work in a fresh profile, and no Business OS HTTP fallback requests, page errors, or critical console/network failures were captured. The same fresh profile failed the visible Research assertion after five minutes: UI showed Web Research but `0 aktiv`, `Keine Domain`, and no `Sources (322)` / `Measurements (816)` / `Knowledge (192)`. Browser-side inspection then proved the data was present locally after WebRTC sync: `window.CTOX_BUSINESS_OS_APP.db.raw.knowledge_tables` contained the `drone_bearing_design` tables with embedded rows `source_catalog=322`, `load_data_library=192`, `measured_load_points=816`, and `research_tasks` contained `research_drone_bearing_design`. Therefore the remaining Research failure was not SQLite, Parquet, server RxDB, ctox.dev token, or WebRTC transport; it was a Research module state refresh bug after late initial replication. Local fix in `src/apps/business-os/modules/research/index.js`: bump module build key, increase collection read timeout from `1600ms` to `10000ms` for large embedded Knowledge docs, and add a bounded debounced refresh on `ctox-business-os-sync-diagnostics` / post-sync so the module rereads RxDB after relevant collections finish initial replication. Local gates passed: `node --check src/apps/business-os/modules/research/index.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. `node --test src/apps/business-os/modules/research/test.mjs` could not run in this worktree because `esbuild` is not installed/resolvable for ESM tests; no repo dependency install was performed. Next action: push this focused frontend fix to `main`, deploy through `ctox upgrade --dev`, and rerun the clean-browser E2E matrix. |
| 2026-06-08 17:26 CEST | Phase 78 Research refresh fix was pushed and deployed through the required `ctox upgrade --dev` path. Commit `0a22e8b2` (`fix: refresh research after rxdb sync`) is active as release `branch-main-20260608T143818Z`; `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260608T143818Z`; `ctox.service` and `ctox-business-os-web.service` are active; the served Research module contains marker `20260608-skf-dashboard-sync-refresh1`; the skf encrypted credential store still contains `CTOX_LLM_PROXY_API_KEY`. A fresh authenticated Playwright run then proved the Research UI can render the correct data after sync: visible `Drohnen-Lagerbelastungsdaten`, `drone_bearing_design · 1.330 rows`, `Sources (322)`, `Measurements (816)`, `Knowledge (192)`, and no visible `Sources (9)`, `Measurements (12)`, `Knowledge (5)`, `129 rows`, or `drone_bearing_loads_25kg`; the same run captured no Business OS HTTP fallback requests, no page errors, no critical console errors, and no critical request failures. The first raw RxDB inspection in that script ran before the UI data wait and was therefore reordered for the next run. |
| 2026-06-08 17:36 CEST | Phase 79 isolated the remaining fresh-browser shell blocker after the Research fix: a repeat clean Playwright run passed login and authenticated reload, then failed before Research because shell bootstrap threw `Modulkatalog wurde noch nicht synchronisiert` after `business_module_catalog` initial sync stalled. Server SQLite has the authoritative `business_module_catalog` row (`module-catalog`) and the previous run proved Research data visibility when the shell reaches the module, so the blocker is a browser bootstrap race, not missing server data. Root cause in `src/apps/business-os/app.js`: after the first catalog timeout, bootstrap immediately called `repairBusinessDataPlane()` and retried `loadModules(20000)`, so a slow WebRTC initial catalog sync after cache repair could still hard-fail the whole shell and produce the observed missing/falsely scoped apps. Local fix: bump `APP_BUILD` to `20260608-module-catalog-sync-wait1`, keep web deployments on `allowShellSeed:false`, extend the post-stall WebRTC catalog wait to 180s before any cache repair, and after repair retry the authoritative RxDB catalog for 180s instead of 20s. Updated `index.html` to the same build key. Local gates passed: `node --check src/apps/business-os/app.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Next action: commit/push this shell fix, deploy through `ctox upgrade --dev`, then rerun the clean-browser E2E matrix. |
| 2026-06-08 18:17 CEST | Phase 80 found and fixed an earlier unauthenticated shell-start hang. After deploying Phase 79, Research E2E passed, but a separate clean Chat E2E sometimes stayed at `Starting native runtime` / `System wird gestartet...` for 180s before the login gate appeared. Static HTML inspection showed the server injects `window.CTOX_BUSINESS_OS_SESSION={authenticated:false}` immediately for unauthenticated web loads, but `loadSession()` awaited `readBusinessOsLaunchConfig()` before reading that injected session. If launch-config discovery stalled, unauthenticated users never saw the login gate. Local fix in `src/apps/business-os/app.js`: read and return the injected session before awaiting launch config; bump `APP_BUILD` and `index.html` to `20260608-session-before-config1`. This preserves RxDB-only data/command transport and changes only shell auth-gate ordering. Local gates passed: `node --check src/apps/business-os/app.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Next action: push, deploy through `ctox upgrade --dev`, then rerun clean-browser login/Research/Chat E2E. |
| 2026-06-08 19:19 CEST | Phase 81 deployed Phase 80 through `ctox upgrade --dev` as release `branch-main-20260608T162038Z`; server checks passed (`ctox.service=active`, `ctox-business-os-web.service=active`, active app marker `20260608-session-before-config1`, Research marker `20260608-skf-dashboard-sync-refresh1`, and stored `CTOX_LLM_PROXY_API_KEY`). Direct skf-to-`https://llm.ctox.dev/v1/responses` probe using the stored token returned HTTP 200 with model `MiniMax-M3`. Fresh browser Research E2E passed the user-visible data story again: login, authenticated reload, `#research`, `Drohnen-Lagerbelastungsdaten`, `drone_bearing_design · 1.330 rows`, tabs `Sources (322)`, `Measurements (816)`, `Knowledge (192)`, no fake `9/12/5` or `drone_bearing_loads_25kg`, no Business OS HTTP fallback requests, no page errors, no critical console errors, and no critical request failures. The new Chat E2E then failed before dispatch because `#ctox` never mounted `[data-ctox-chat-root]`; screenshot/body stayed at `Modulkatalog wird synchronisiert...`. A dedicated 6-minute browser diagnostic showed `business_module_catalog` stuck in RxDB/WebRTC with `connectionStatus=connecting/reconnecting`, `initialReplicationState=pending`, no local `module-catalog` doc, and frame transport stalled at three received frames, while server SQLite already had the authoritative `module-catalog` row. Local fix implemented: bump `APP_BUILD`/`index.html` to `20260608-module-catalog-stall-restart1` and add a bounded cooldown-protected `business_module_catalog` collection restart inside `loadModuleCatalog()` when the RxDB/WebRTC catalog sync is stale. This is not an HTTP/IndexedDB/fake-data fallback; it restarts the same RxDB/WebRTC collection until the authoritative catalog doc arrives. Local gates passed: `node --check src/apps/business-os/app.js`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and `git diff --check`. Production remains not ready until this is pushed, deployed through `ctox upgrade --dev`, and fresh-browser Chat E2E proves command creation plus CTOX queue completion. |
| 2026-06-08 20:38 CEST | Phase 82 fresh Browser Chat E2E after the module-catalog restart deploy reached `[data-ctox-chat-root]` and used the visible CTOX chat composer, not a backend insert. Submitted marker `SKF_CHAT_E2E_1780943724608` through the UI. Server RxDB proof: `business_commands` row `cmd_caae0cd6-37b0-47db-a34e-71fc54600109` was created with `command_type=business_os.chat.task`, `inbound_channel=business_os.llm.chat`, `dispatch_transport=rxdb-command-bus`, and linked `task_id=queue:system::b2630e94af53f79188d9d396`; `ctox_queue_tasks` row `queue:system::b2630e94af53f79188d9d396` was created in the single CTOX queue. Harness proof: `journalctl --user -u ctox.service` shows the prompt worker started with provider `ctox_core_api` and `base_url=https://llm.ctox.dev/v1`, started a direct session, then aborted and ended with `error=database is locked`. Terminal state after the run: command `status=failed`, `task_status=failed`; queue task `status=failed`, `route_status=failed`, and the task JSON still lacks a useful persisted error message. Answer to the user story is therefore: Browser Chat does reach CTOX via RxDB and creates a real CTOX queue task, but it is not processed successfully because the worker currently fails on a SQLite lock during execution. Production remains not ready until the SQLite lock source is fixed and the same Browser E2E completes successfully. |
| 2026-06-08 21:08 CEST | Phase 83 local SQLite-lock queue handling fix implemented. Code reading showed the lock comes after `InProcessAppServerClient`/Direct Session startup and is classified by the generic Queue worker error branch. Existing behavior only treated SQLite locks as retryable for Founder/outbound paths; normal `queue` / Business OS chat work therefore terminalized a transient intra-process SQLite contention as `failed`. Local fix: `hard_runtime_blocker_retry_cooldown_secs` now classifies `database is locked`, `database is busy`, `sqlite_busy`, and `sqlite locked` as a 30s transient blocker, and `runtime_error_is_transient_api_failure` routes the same signatures through the existing durable Queue retry path. Added regression `queue_sqlite_lock_is_retryable_runtime_failure` proving a non-founder `queue` job stays retryable/pending. Gates passed: `cargo fmt --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `git diff --check`, `cargo test --locked queue_sqlite_lock_is_retryable_runtime_failure --bin ctox -- --nocapture`, and `cargo test --locked worker_failures_route_to_recoverable_states_never_blocked --bin ctox -- --nocapture`. Production remains not ready until this is pushed to `main`, deployed via `ctox upgrade --dev`, and Browser Chat E2E proves completion rather than requeue/failure. |
| 2026-06-08 21:41 CEST | Phase 84 SQLite-lock retry fix pushed and deployed through the required `ctox upgrade --dev` path. Commit `aeefd853` (`fix: retry queue work after sqlite locks`) was pushed to `main` and activated as release `branch-main-20260608T185214Z`; previous release was `branch-main-20260608T172002Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260608T185217Z`; active `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260608T185214Z`. `systemctl --user is-active ctox.service ctox-business-os-web.service` reports both services `active`, and the active release source contains the `database is locked` retry signatures in `turn_loop.rs` and `service.rs`. Production remains not ready until a fresh Browser Chat E2E proves a UI-submitted chat reaches the single CTOX queue and completes through `llm.ctox.dev`. |
| 2026-06-09 09:34 CEST | Phase 85 regression rollback started. Post-Phase-84 fresh browser diagnostics showed the native WebRTC peer was visible through WSS signaling and ICE reached `connected`, but browser DataChannels stayed `connecting` until the skf instance was configured with `CTOX_WEBRTC_UDP_BIND_ADDR=51.210.246.120:0` in `/home/ubuntu/.config/ctox/business-os.env` and `ctox.service` was restarted. After that instance-only configuration change, clean browser diagnostics showed open RxDB/WebRTC channels and completed initial replication for shell-critical collections including `business_module_catalog`, `ctox_runtime_settings`, `business_chats`, and `ctox_queue_tasks`, with `http_bridge_available=false`. Because the earlier `20260608-module-catalog-stall-restart1` frontend patch restarts the catalog collection while the real issue was the native peer bind address, it is being removed from `main` as a self-caused sync-churn regression. Production remains not ready until this rollback is pushed, deployed through `ctox upgrade --dev`, and clean-browser E2E verifies Research data plus Chat task completion. |
| 2026-06-09 10:06 CEST | Phase 86 regression rollback deployed. Commit `79871530` (`fix: remove module catalog sync restart`) was pushed to `main` and deployed through the required `ctox upgrade --dev` path as release `branch-main-20260608T221455Z`; previous release was `branch-main-20260608T185214Z`; upgrade backup is `/home/ubuntu/.local/state/ctox/backups/update-20260608T221458Z`. Server verification: `current` resolves to `/home/ubuntu/.local/lib/ctox/releases/branch-main-20260608T221455Z`, `ctox.service` and `ctox-business-os-web.service` are active, `APP_BUILD` and served `index.html` use `20260609-module-catalog-no-restart1`, the instance env still contains `CTOX_WEBRTC_UDP_BIND_ADDR=51.210.246.120:0`, `ctox business-os peer status` reports `peer_id=ctox-core-Qz4MkUa2aN`, `peer_role=ctox_instance`, `sync_mode=p2p-first`, and `http_bridge_available=false`. External `https://skf.ctox.dev/` and `/app.js` both serve the new marker. Production remains not ready until the clean browser E2E matrix passes. |
| 2026-06-09 01:42 CEST | Phase 87 WebRTC peer room recovery verified. After the Phase 86 deploy, a clean browser login reached the shell but `business_module_catalog` initial replication stayed pending while the browser DataChannel remained `connecting`. `ctox business-os peer rotate` changed the advertised room, but the native peer session did not rejoin the new room until `ctox.service` and `ctox-business-os-web.service` were restarted. After the restart, a fresh browser WebRTC probe opened the DataChannel, reached `connectionState=connected`, reported `activePeerCount=1`, and received host plus srflx candidates with `http_bridge_available=false`. This identifies the remaining transport issue as a native peer room/session lifecycle problem after rotate/redeploy, not an HTTP fallback and not missing server data. |
| 2026-06-09 01:42 CEST | Phase 88 local Chat dock regression fix implemented. Fresh browser E2E after the WebRTC recovery proved login, authenticated reload, and Web Research data visibility without fake `9/12/5` data, but failed on the CTOX Chat story because clicking the chat FAB left `[data-ctox-chat-root]` collapsed with no composer. Code reading found the precise frontend regression: `toggleChatDock()` counted all open chats across all dates, while `renderChatRoot()` only renders chats for `state.selectedDate`; old open chats from another date could therefore prevent a visible chat from being created for today. Local fix scopes `toggleChatDock()` to the selected date and ignores stale restore IDs from other days. `APP_BUILD` and `index.html` were bumped to `20260609-chat-dock-date-scope1`. Production remains not ready until gates pass, this is pushed to `main`, deployed via `ctox upgrade --dev`, and clean-browser E2E proves Chat task completion through the single CTOX queue. |
| 2026-06-09 03:22 CEST | Phase 89 local terminal projection regression fix implemented. After the Phase 88 deploy, a UI-submitted Chat task reached the single CTOX queue through `dispatch_transport=rxdb-command-bus` and completed through `llm.ctox.dev`, but the active RxDB `business_commands` row stayed stale at `accepted/queued` while `business-os.sqlite3.business_records`, canonical `communication_routing_state`, and the active RxDB queue projection were terminal. Code reading found the exact missing writeback: `process_business_chat_reply()` and the Business OS command terminal paths updated Business Store records, and `refresh_queue_task_projection()` updated the Business Store queue record, but they did not write the terminal documents into the active RxDB collection tables. Local fix now mirrors terminal `business_commands`, `business_chats`, and `ctox_queue_tasks` projections into active RxDB. Added regression `queue_worker_success_marks_active_rxdb_business_command_completed`, which seeds stale active RxDB rows and asserts successful Chat completion updates `business_commands` and `ctox_queue_tasks` to completed/handled with the reply text. Gates passed: `cargo fmt`, `git diff --check`, `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, `cargo test queue_worker_success_marks_active_rxdb_business_command_completed`, `cargo test queue_worker_`, and `cargo test repair_queue_projections`. Production remains not ready until this is committed, pushed to `main`, deployed via `ctox upgrade --dev`, runtime repair/verification confirms stale production command projections are gone, and clean-browser E2E proves login, Research data, Chat completion, visible reply/result, logout, and no critical console/network failures. |
