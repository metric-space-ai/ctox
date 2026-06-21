# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation production-ready through the real
CTOX product paths. A user should be able to ask CTOX for a small business app
and receive a runtime-installed, immediately runnable vanilla HTML/CSS/browser
ESM app that persists through CTOX DB and can dispatch normal Business OS
commands.

This is a live working plan. Update it during execution, not only at handoff.

## How To Update This Plan

Edit these sections after every material change:

- Current State
- Active Slice
- Phase Tracker
- Bench Matrix
- Evidence Log
- Open Issues
- Next Actions

Rules:

- Keep the newest factual status near the top.
- Record run ids, release ids, commits, commands, and evidence paths.
- Mark a phase `done` only when its exit criteria have evidence.
- Keep exactly one active slice.
- Classify each failure before patching.
- Do not use this file as an app-building prompt.
- Do not hand-edit generated app artifacts for a proof run.

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Current State

Last updated: `2026-06-21`

Overall status: `in_progress`, not production-ready yet.

Installed CTOX:

- Source branch: `main`
- Last source head checked before this plan edit:
  `aaf4bbb8 Fix Business OS app queue finalization lock`
- Active install:
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T064355Z`
- Install path: applied through `ctox upgrade --dev`
- State root:
  `/Users/michaelwelsch/.local/state/ctox`
- Runtime app target:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/<module-id>`
  which resolves into the managed runtime/state root. Runtime apps must not be
  written into source paths.
- CTOX status at latest check: `running=true`, `busy=false`,
  `worker_active_count=0`, `pending_count=0`
- Business OS status at latest check: `ok=true`, native RxDB peer
  `replicationUp=true`, `http_bridge_available=false`

Current proof run:

- Run id: `rfix7`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX
- Evidence dir:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7`
- Latest status snapshot:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782025675994.json`

Latest result:

- `rfix7` reached terminal static success: five handled tasks, zero leased
  tasks, five installed validations green.
- Browser proof is red and blocks production readiness. The first five-app E2E
  showed four generated apps with visible Create/New buttons that did not open
  their dialogs/forms; Quality created a complaint, persisted it through the
  browser collection, replicated it into native SQLite, and showed it after
  reload.
- Focused browser smoke classified the repeated failure as
  `skill_resource_gap` plus `validator_gap`: several generated apps put dialog
  elements outside the module root section, then queried them with
  `root.querySelector(...)`. Inventory crashes during mount; Contracts records
  the click but no handler reveals a dialog/form. Static validation did not
  catch this because it saw buttons, handlers, and tests but no real browser
  interaction.
- A new app smoke tool/CLI is under source test:
  `ctox business-os app smoke <module-id> --installed`. It is a validation
  tool only; it does not generate, repair, or rewrite app artifacts.
- No generated `rfix7` app files may be patched by hand.

Latest source fix under test:

- Forensic sample:
  `/tmp/ctox-real_2026-06-21_082121_SuRk.sample.txt`.
- Classification: `runtime_orchestration_gap`, not app-model failure. Status
  IPC, channel routing, app recovery, and work-hours dispatch were all waiting
  on the same service `SharedState` mutex while the prompt worker was inside
  `sync_workspace_root_to_business_os`.
- Patch in `src/core/service/service.rs`: release `SharedState` before slow
  Business OS workspace-file sync, then reacquire it only to record resulting
  events.
- Verification runs:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`
  and
  `cargo test --bin ctox completion_hook_indexes_workspace_outputs_for_business_os -- --nocapture`.
- Source fix commit: `aaf4bbb8 Fix Business OS app queue finalization lock`.
- Installed through `ctox upgrade --dev` as
  `branch-main-20260621T064355Z`.
- Installed source proof:
  `/Users/michaelwelsch/.local/lib/ctox/current/src/core/service/service.rs`
  contains the lock release before Business OS workspace sync and the recovery
  regression test.

Current source patch under test:

- Added `src/apps/business-os/scripts/smoke-app-module.mjs`.
- Added `ctox business-os app smoke <module-id>` CLI wiring in
  `src/core/service/business_os.rs` and top-level help in `src/core/main.rs`.
- Updated concise skill resources to require real-shell Create/New/Add smoke
  and to call out DOM-scope handling for sibling dialogs/forms.
- Verification so far:
  `node --check src/apps/business-os/scripts/smoke-app-module.mjs`,
  `node src/apps/business-os/scripts/smoke-app-module.mjs bench_quality_rfix7 --installed --json --timeout-ms 90000`,
  `cargo run --bin ctox -- business-os app smoke bench_quality_rfix7 --installed --json --timeout-ms 90000`,
  and negative CLI/tool controls against Inventory and Contracts.
- `cargo fmt --check -- src/core/main.rs src/core/service/business_os.rs` was
  attempted but existing unrelated formatting diffs in other Business OS Rust
  files made it red; no broad formatting was applied.

Previous source fix:

- Skill/resources and installed-module validation require canonical collection
  access through `ctx.db.collection('<declared-collection-name>')`.
- Validation rejects `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, cached DB facade handles, and app-side
  `ctx.db.registerSchemas`.
- Installed validator proof: the old `bench_inventory_rfix6` artifact is
  rejected by installed `ctox business-os app validate`.

Latest local regression guard:

- `src/core/service/service.rs` has a committed regression test for status
  snapshot recovery of a leased app task whose target directory is missing.
- Verification run:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`.
- The worker-finalization sync-lock fix is committed, pushed, and installed.

## Non-Negotiables

The app creation path must stay simple, agent-led, and product-native.

Do:

- Let CTOX create apps through normal durable app-create tasks.
- Write runtime app output only under
  `$CTOX_INSTALL_ROOT/current/runtime/business-os/installed-modules/<module-id>`.
- Build apps as vanilla `index.html`, `index.css`, browser ESM `index.js`, and
  optional local browser ESM helper files.
- Persist data only through shell-provided collection handles from
  `ctx.db.collection('<declared-collection-name>')`.
- Dispatch automation only through `ctx.commandBus.dispatch`.
- Use `business_os.chat.task` or another allowed Business OS command for
  app-triggered AI, ticket, or follow-up work.
- Keep `module.json`, `collections.schema.json`, and `schema.js` in exact
  collection/version/type parity.
- Scope runtime collection names to the module id.
- Require the app-building agent to inspect at least three existing Business OS
  apps selected by the agent as concrete references before implementation.
- Patch skill resources only for reusable guidance gaps.
- Patch validator/runtime only when evidence shows a systemic CTOX gap.
- Install source fixes through `ctox upgrade --dev`.

Do not:

- Do not reintroduce a deterministic app builder or template writer.
- Do not repair generated app artifacts by hand during a proof run.
- Do not write runtime app output into `src/`; `src/` contains source code and
  store templates only.
- Do not import upstream `rxdb`, React, Next.js, Vue, package-manager
  dependencies, or build-time frameworks.
- Do not add HTTP data bridges, REST fallbacks, app-owned IndexedDB wrappers,
  or localStorage persistence.
- Do not use `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, cached DB handles, or legacy
  collection fallbacks.
- Do not call `ctx.db.registerSchemas` from app code.
- Do not add long prompt walls inside the skill or app creator.
- Do not copy source-only manifest fields from built-in modules into runtime
  app manifests.
- Do not ship UI slop: unused third columns, fake buttons, hidden overlays that
  intercept clicks, resize CSS without real resize behavior, or dead actions.

## Production Gates

App creation is production-ready only when every gate is green.

| Gate | Status | Required Evidence |
| --- | --- | --- |
| Skill shape | in_progress | English, concise, resource-based, no prompt wall, requires three reference apps, clear Do/Don't list, clear green checklist, includes browser-smoke finalization. |
| Correct install location | done | Generated apps are under `runtime/business-os/installed-modules/<module-id>` and survive `ctox upgrade --dev`. |
| CTOX-native creation | in_progress | Fresh five-app bench is created through real app-create tasks, not direct file writes. `rfix7` is running. |
| Static validation | done | `rfix7` reached terminal queue success and installed validation green for all five apps. |
| Browser mount | blocked | `rfix7` Inventory has a real mount error; fresh browser smoke catches it. |
| Five-app browser E2E | blocked | `rfix7` browser E2E found dead Create/New flows in four apps. Fresh run required after skill/tool patch. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module creation contract. |
| Versioning contract | pending | Existing app version metadata is audited; missing enforcement is listed or patched; users see only versions `>=1.0.0`; each `x.0.0` major is independently installable with its own app icon. |
| Install/upgrade lifecycle | in_progress | `ctox upgrade --dev` applies source fixes, preserves runtime modules, and leaves CTOX/Business OS healthy. |
| No regressions | in_progress | Relevant Rust/JS checks and browser evidence are green after final patch. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App creation uses durable tasks and agent implementation, not deterministic generated source. | Earlier deterministic builder artifacts removed; bench runner submits real app-create tasks. |
| 1. Simplify skill/resources | in_progress | Codex | Skill/resources are English, concise, reference/resource based, avoid prompt walls, state CTOX DB/command patterns without legacy fallbacks, and require browser-smoke proof. | Latest resources now include the DOM-scope lesson and `ctox business-os app smoke`; needs fresh run proof after install. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | Latest installed release `branch-main-20260621T064355Z`; forensic sample showed worker-finalization workspace sync holding `SharedState` and blocking router/recovery/status; commit `aaf4bbb8` moved sync outside the `SharedState` lock and is installed through `ctox upgrade --dev`. |
| 4. Close validator/resource gaps | in_progress | Codex | Validator/tooling rejects predictable bad app artifacts before signoff, without blocking valid vanilla apps. | `89c2a75d` rejects old DB fallbacks; current source adds `ctox business-os app smoke` to catch dead create flows in the real shell. |
| 5. Fresh five-app CTOX proof | done | Codex | One fresh post-validator run reaches terminal queue success and installed validation green for five apps. | `rfix7` terminal green: 5 handled, 0 leased, 5/5 installed validations green. |
| 6. Browser proof | blocked | Codex | Browser mount, UI persistence, reload persistence, native sync, and automation smoke pass for all five fresh apps. | `rfix7` browser evidence is red: Inventory mount error, Contracts dead create flow, five-app E2E shows four dead Create/New flows; Quality positive-control is green. |
| 7. Entry-point proof | pending | Codex | Every user-facing app creation/modification path uses the same skill/resource context and runtime app contract. | Not done. |
| 8. Versioning proof | pending | Codex | App version visibility and major-version independence are either implemented or listed as missing work. | Not done. |
| 9. Production signoff | pending | Codex | All production gates are green, latest source is installed, plan/docs updated, no unrelated dirty files staged. | Not done. |

Phase editing rules:

- A phase may move to `done` only with an Evidence Log entry and a command,
  file, run id, or browser evidence path.
- A failed browser or validation run must update Bench Matrix and Open Issues
  before a fix is applied.
- A source fix must list expected verification before patching and actual
  verification after patching.
- A generated app failure must not be fixed inside the generated app directory
  for the active proof run.

## Active Slice

Owner: `Codex`

Active phase: `4. Close validator/resource gaps`

Current rule: patch only reusable skill/tooling gaps exposed by the `rfix7`
browser evidence. Do not hand-edit generated app files. Before any source patch,
classify the evidence as `model_failure`, `skill_resource_gap`,
`validator_gap`, `runtime_orchestration_gap`, `data_plane_gap`, or
`entry_point_gap`.

Current focus:

- Finish the minimal browser-smoke CLI/source patch.
- Install it through `ctox upgrade --dev`.
- Start a fresh five-app CTOX bench run and require both installed static
  validation and `ctox business-os app smoke` proof before returning to full
  five-app browser E2E.

Immediate checklist:

- [x] Remove deterministic app-builder path from the product flow.
- [x] Make app creation use durable app-create tasks.
- [x] Commit and push the native peer/catalog fix to `main`.
- [x] Install native peer/catalog fix through `ctox upgrade --dev`.
- [x] Run `rfix6` and browser proof to expose remaining systemic gaps.
- [x] Classify legacy DB fallback usage as `skill_resource_gap` plus
  `validator_gap`.
- [x] Patch skill resources to use canonical `ctx.db.collection(...)` only.
- [x] Patch validator to reject legacy DB access and app-side schema
  registration.
- [x] Run validator unit tests and whitespace check.
- [x] Commit and push validator/skill fix to `main`.
- [x] Install validator/skill fix through `ctox upgrade --dev`.
- [x] Prove old `rfix6` installed apps are rejected by the installed validator.
- [x] Start fresh five-app CTOX bench run `rfix7`.
- [x] Inventory `rfix7` reached terminal queue success with installed
  validation green.
- [x] Projects `rfix7` reached terminal queue success with installed
  validation green.
- [x] Classify the `bench_subscriptions_rfix7` leased-without-artifacts state
  as `runtime_orchestration_gap` evidence.
- [x] Install current `main` with `ctox upgrade --dev` and observe boot recovery
  requeue the stale Subscriptions task.
- [x] Quality `rfix7` reached terminal queue success with installed validation
  green after same-task repair.
- [x] Capture process sample proving the service loops were blocked on
  `SharedState` while the worker was in Business OS workspace sync:
  `/tmp/ctox-real_2026-06-21_082121_SuRk.sample.txt`.
- [x] Patch worker finalization so Business OS workspace sync runs outside the
  `SharedState` lock.
- [x] Run targeted source tests for missing-target recovery and workspace sync.
- [x] Verify service status is responsive after the restart.
- [x] Prove Contracts continues without manual artifact edits and reaches
  installed validation green.
- [x] Commit and push the worker-finalization sync-lock fix to `main`.
- [x] Install the worker-finalization sync-lock fix through `ctox upgrade --dev`
  after it is on `main`.
- [x] Prove Subscriptions continues without manual artifact edits.
- [x] Wait for all five `rfix7` tasks to reach terminal state.
- [x] Run installed validation for each `rfix7` app after terminal state.
- [x] Update Bench Matrix with terminal `rfix7` static results.
- [x] Run browser E2E/smoke against `rfix7` after static validation green.
- [x] Classify the repeated dead Create/New flows as `skill_resource_gap` plus
  `validator_gap`.
- [x] Add a minimal real-browser app smoke tool/CLI, not a deterministic app
  builder.
- [ ] Commit and push the browser-smoke/skill patch to `main`.
- [ ] Install the browser-smoke/skill patch through `ctox upgrade --dev`.
- [ ] Start fresh five-app CTOX bench run after install.
- [ ] Require installed validation and browser smoke for each fresh app before
  returning to full browser E2E.

Current slice exit criteria:

- Source has a committed, installed `ctox business-os app smoke` command.
- A fresh post-install bench run reaches static validation green and browser
  smoke green, or failures are classified with evidence before further patching.

## Bench Matrix

Historical run `rfix6`:

| Case | Module Id | Queue Status | Static Validation | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |
| Inventory | `bench_inventory_rfix6` | handled | invalid after hardening | red | Data persisted into browser/native DB but UI reopened with `ITEMS 0`; generated source used forbidden DB fallbacks. |
| Projects | `bench_projects_rfix6` | handled | invalid after hardening | harness-blocked | App looked plausible; browser smoke clicked hidden modal close; generated source used forbidden DB fallbacks. |
| Contracts | `bench_contracts_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |
| Quality | `bench_quality_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |

Active run `rfix7`:

| Case | Module Id | Queue Status | Static Validation | Browser Mount | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix7` | handled | green | mounted in first five-app E2E | red | Create flow timed out in first five-app E2E; source inspection shows modal refs queried from the inner root while dialogs are outside it. |
| Inventory | `bench_inventory_rfix7` | handled | green | red | red | Browser smoke catches mount error at `index.js:92` and dead `new-item` create flow. |
| Projects | `bench_projects_rfix7` | handled | green | mounted in first five-app E2E | red | Create flow timed out in first five-app E2E; source inspection shows same modal/root-scope pattern. |
| Contracts | `bench_contracts_rfix7` | handled | green | mounted | red | Browser smoke observes `new-contract` click, but no dialog/form/save flow appears. |
| Quality | `bench_quality_rfix7` | handled | green | mounted | partial green | Smoke green for `create-complaint`; prior focused E2E proved browser collection, native SQLite, and reload persistence. Automation still needs full fresh-run proof. |

Only the latest fresh post-fix run may be used for production signoff.

## Failure Classification

Use these classes before patching:

- `model_failure`: one generated app has inconsistent app logic, tests, or UI,
  and no reusable CTOX architecture rule was missed.
- `skill_resource_gap`: multiple apps miss the same CTOX-specific architecture
  concept, or the skill/resource wording makes the correct path unclear.
- `validator_gap`: bad artifacts pass installed validation but fail predictable
  static, browser, or runtime checks.
- `runtime_orchestration_gap`: queue, app lifecycle, install path, launchd,
  module catalog, native peer, or validation finalization is wrong.
- `data_plane_gap`: WebRTC/CTOX DB/schema registration/sync is wrong.
- `entry_point_gap`: a user-facing path does not attach the same app-module
  creation contract or does not create a normal durable app-create task.

Patch policy:

- Patch the skill only for repeated or clearly reusable app-building guidance.
- Patch the validator when it can reject a concrete bad artifact generically
  without becoming a rule maze.
- Patch CTOX runtime when app output is valid but lifecycle/data/queue
  machinery fails.
- Patch browser-smoke harness only when the harness is proven wrong.
- Do not patch generated app files for the proof run.

## Architecture Translation Cheatsheet

| Common web-app assumption | Business OS app equivalent |
| --- | --- |
| Next.js/React app with build step | Vanilla runtime module: `index.html`, `index.css`, browser ESM `index.js`. |
| npm/package dependency | No dependency management. Only browser ESM files shipped with the app or provided by the shell. |
| App-owned database setup | Shell supplies `ctx.db`; app uses declared collection handles. |
| IndexedDB/Postgres direct access | CTOX DB over WebRTC; never an HTTP bridge and never an app-owned IndexedDB wrapper. |
| REST API write | `ctx.db.collection('<collection>').<operation>` for records, or `ctx.commandBus.dispatch` for CTOX actions. |
| Queue/task/ticket side effect | Dispatch a normal Business OS command, commonly `business_os.chat.task` or an allowed ticket command. |
| Framework router/layout | Business OS shell mounts one module. Keep layout simple; use modals where appropriate. |
| Source app template | Reference only. Runtime apps must adapt to runtime manifest/schema rules. |

## Versioning Rules To Verify

Expected policy:

- `0.0.x`: UI/UX, feature, or bug-fix changes.
- `0.x.0`: database structure or other potentially breaking changes.
- `1.0.0`: first release version visible beyond the developer.
- `x.0.0`: independent major app line that can run in parallel with older
  major versions and has its own app icon.

Work still required:

- Audit the current app version metadata and visibility rules.
- List missing source work if CTOX does not already enforce the policy.
- Ensure non-developer users only see versions `>=1.0.0`.
- Ensure major versions can run independently rather than overwriting a
  production app line.

## Finalization Checklist For Each Generated App

Use this before marking any generated app green:

- [ ] Files exist: `module.json`, `collections.schema.json`, `schema.js`,
  `index.html`, `index.css`, `index.js`, `icon.svg`, `locales/en.json`,
  `locales/de.json`, tests, and helper ESM where needed.
- [ ] App is in the runtime installed-module directory, not `src/`.
- [ ] `module.json`, `collections.schema.json`, and `schema.js` agree.
- [ ] Runtime collection names are scoped to the module id.
- [ ] Persistence code obtains collection handles only with
  `ctx.db.collection('<declared-collection-name>')`.
- [ ] No `ctx.db[name]`, `ctx.db.collections`, direct `ctx.db.<collection>`,
  cached DB handle, raw IndexedDB, HTTP, app-owned sync fallback, or
  localStorage persistence exists.
- [ ] App code does not call `ctx.db.registerSchemas`.
- [ ] Record helper outputs match declared JSON types.
- [ ] No package manager, build step, React/Next/Vue, upstream `rxdb`, or HTTP
  data bridge.
- [ ] UI has a primary create/edit path for an empty state.
- [ ] Every visible button either works or is removed.
- [ ] Hidden modals/overlays are actually hidden and cannot intercept clicks.
- [ ] `ctox business-os app smoke <module-id> --installed` clicks the primary
  Create/New/Add flow in the real shell and passes.
- [ ] No ornamental third column unless the app genuinely needs it.
- [ ] No resize-column CSS unless the implemented layout actually supports it.
- [ ] Browser mount has no console/page/request failures.
- [ ] UI create/edit persists through `ctx.db`, reload, and native CTOX DB sync.
- [ ] Automation dispatches through `ctx.commandBus.dispatch` and creates a
  normal command record.

## Next Actions

1. Finish verification for the new `ctox business-os app smoke` command.
2. Commit and push the smoke/skill/plan patch to `main`.
3. Install through `ctox upgrade --dev`.
4. Start a fresh five-app CTOX bench run after install.
5. Require installed validation plus browser smoke for each fresh app.
6. Run full browser E2E only after the smoke gate is green for all five apps.
7. Do not hand-edit generated app artifacts.
8. If browser E2E is red, classify each failure before patching.
9. After browser E2E is green, verify entry paths: Chat, App Creator, App
   Store/template flow, CLI, and inbound/MCP.
10. Audit app versioning enforcement and list or patch the missing pieces.

## Evidence Log

- `2026-06-21`: `rfix6` snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782010063510.json`
  showed terminal queue success and installed validation green before DB-access
  hardening.
- `2026-06-21`: browser proof
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/post-upgrade-catalog-mount-1782014629790.json`
  showed a fresh browser mounted `bench_subscriptions_rfix6` and materialized
  the module catalog.
- `2026-06-21`: five-app browser proof
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/five-app-e2e-v3-1782016065115.json`
  showed Subscriptions, Contracts, and Quality green; Projects blocked by smoke
  harness behavior; Inventory red after reload.
- `2026-06-21`: targeted Inventory evidence
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/inventory-reopen-1782016530701.json`
  and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/inventory-idb-inspect-1782016709703.json`
  showed the saved marker in browser IndexedDB but not rendered in the reopened
  UI.
- `2026-06-21`: installed source audit found forbidden legacy DB fallback usage
  in all five generated `rfix6` apps. This was classified as
  `skill_resource_gap` plus `validator_gap`.
- `2026-06-21`: commit `89c2a75d` hardened skill resources and validation
  against legacy DB access. Verification:
  `node src/apps/business-os/scripts/validate-app-module.test.mjs` and
  `git diff --check`.
- `2026-06-21`: `ctox upgrade --dev` installed
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T045029Z`.
- `2026-06-21`: installed validator rejects old
  `bench_inventory_rfix6` with `ctx.db.registerSchemas` and `ctx.db[...]`
  failures.
- `2026-06-21`: `ctox business-os app bench run --suite core-five --model
  minimax-m3 --context 256k --run-id rfix7` accepted five real app-create
  tasks. Evidence dir:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782018818728.json`
  shows Inventory leased, four apps pending, and no terminal bench result yet.
- `2026-06-21`: commit `b338fe7e` updated this file as the live app creation
  execution plan and was pushed to `main`.
- `2026-06-21`: `ctox status --json` showed CTOX running idle with
  `busy=false`, `worker_active_count=0`, Business OS `ok=true`, native peer
  `replicationUp=true`, and `http_bridge_available=false`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782020545985.json`
  shows Inventory and Projects handled with installed validation green,
  Subscriptions leased with no artifact directory, and Contracts/Quality
  pending.
- `2026-06-21`: local regression check passed:
  `cargo test --bin ctox stale_app_recovery_requeues_leased_missing_target_before_validation -- --nocapture`.
- `2026-06-21`: local regression check passed:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`.
- `2026-06-21`: `ctox upgrade --dev` installed
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T055246Z`
  with state root `/Users/michaelwelsch/.local/state/ctox`.
- `2026-06-21`: installed CTOX boot recovery requeued
  `bench_subscriptions_rfix7` from stale leased/no-artifact state to pending
  with status note
  `business-os:requeued-unstarted-app: app target missing or empty`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782022224137.json`
  shows Inventory, Projects, and Quality handled with installed validation
  green; Subscriptions and Contracts pending.
- `2026-06-21`: process sample
  `/tmp/ctox-real_2026-06-21_082121_SuRk.sample.txt` showed status IPC,
  channel router, app recovery, and work-hours dispatcher all blocked on the
  service `SharedState` mutex while the active prompt worker was inside
  `sync_workspace_root_to_business_os`. Classified as
  `runtime_orchestration_gap`.
- `2026-06-21`: source patch moved Business OS workspace-file sync out of the
  locked worker-finalization section in `src/core/service/service.rs`. Local
  checks passed:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`
  and
  `cargo test --bin ctox completion_hook_indexes_workspace_outputs_for_business_os -- --nocapture`.
- `2026-06-21`: `ctox upgrade --dev` installed
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T062812Z`
  and restarted CTOX successfully, but the installed release did not include
  the uncommitted local sync-lock patch. It did prove the service became
  responsive again and let `bench_contracts_rfix7` finish with installed
  validation green.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782024140977.json`
  shows Inventory, Projects, Contracts, and Quality handled with installed
  validation green; Subscriptions leased/running.
- `2026-06-21`: commit `aaf4bbb8` (`Fix Business OS app queue finalization
  lock`) was pushed to `main` and installed with `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T064355Z`.
  Installed-source proof found the `drop(shared)` lock release and recovery
  regression test in
  `/Users/michaelwelsch/.local/lib/ctox/current/src/core/service/service.rs`.
- `2026-06-21`: `ctox status --json` after the upgrade showed
  `running=true`, `busy=true`, `worker_active_count=1`, Business OS `ok=true`,
  native peer `replicationUp=true`, and `http_bridge_available=false`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782024995865.json`
  shows Inventory, Projects, Contracts, and Quality handled with installed
  validation green; Subscriptions leased/running same-task rework with
  incomplete artifacts missing `index.js` and `tests/*.test.mjs`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782025276623.json`
  temporarily showed all five installed validations green, including
  Subscriptions with 11/11 tests, while Subscriptions still had queue status
  `leased`.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782025474262.json`
  shows Subscriptions still `leased`; all required files are present and static
  validation/syntax are green, but `tests/records.test.mjs` fails 4/35
  app-local assertions. Failing areas: `selectReviewTargets`,
  due-soon/churn-risk partition helpers, `sortSubscriptionsForUi`, and high
  churn follow-up priority.
- `2026-06-21`: `rfix7` status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782025675994.json`
  is the terminal five-app static gate: `bench_green=true`, `handled=5`,
  `leased=0`, `validation_passed=5`, and no missing required files. Subscriptions
  reached `handled` at `2026-06-21T07:06:27Z` with 35/35 module tests passing.
- `2026-06-21`: first `rfix7` five-app browser E2E
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/five-app-e2e-1782026037405.json`
  was red: Subscriptions, Inventory, Projects, and Contracts did not complete
  their primary create dialog/form flow; Quality created and reloaded a
  complaint and native SQLite contained the record.
- `2026-06-21`: focused dialog probe
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/dialog-open-probe-1782026374558.json`
  showed visible Create/New clicks without `showModal()` or visible forms for
  the tested dialog apps.
- `2026-06-21`: source inspection of generated `rfix7` apps found the repeated
  DOM-scope defect: dialogs/forms are siblings of the module root section, but
  generated code queries them with `root.querySelector(...)`. Classification:
  `skill_resource_gap` plus `validator_gap`.
- `2026-06-21`: new browser-smoke tool positive control:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-quality-tool-v2.json`
  and CLI proof
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-quality-cli.json`
  both passed for `bench_quality_rfix7`.
- `2026-06-21`: new browser-smoke tool negative controls:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-inventory-cli.json`
  failed with the Inventory mount error and dead `new-item` flow; and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-contracts-tool-v2.json`
  failed because `new-contract` did not reveal a dialog/form/save flow.

## Open Issues

- The five-app static gate is green, but browser E2E for `rfix7` is red due to
  repeated dead Create/New flows. A fresh bench run is required after the
  smoke/skill patch is installed.
- Entry-point proof across Chat, App Creator, App Store/template flow, CLI, and
  inbound/MCP is still pending.
- App versioning policy must be audited and either enforced or listed as missing
  implementation work.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out
  of this work unless explicitly requested.
