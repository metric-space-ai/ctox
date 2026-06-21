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
  `15a31429 Clarify app creation plan source head`
- Active install:
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T055246Z`
- Install path: applied through `ctox upgrade --dev`
- State root:
  `/Users/michaelwelsch/.local/state/ctox`
- Runtime app target:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/<module-id>`
  which resolves into the managed runtime/state root. Runtime apps must not be
  written into source paths.
- CTOX status at latest check: `running=true`, `busy=true`,
  `worker_active_count=1`, `pending_count=2`
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
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/status-1782022239634.json`

Latest result:

- `rfix7` is still running and must not be judged as final yet.
- Inventory reached `handled` with installed validation green and 26/26 module
  tests passing.
- Projects reached `handled` with installed validation green and 30/30 module
  tests passing.
- Quality reached `handled` with installed validation green and 10/10 module
  tests passing after CTOX asked the same app task to repair missing artifacts.
- Subscriptions was previously `leased` without artifacts while CTOX was idle.
  After `ctox upgrade --dev`, boot recovery requeued it to `pending` with
  `business-os:requeued-unstarted-app: app target missing or empty`. Classify
  this incident as `runtime_orchestration_gap` evidence unless later source
  inspection proves a narrower lifecycle cause.
- Contracts is still `pending`.
- Subscriptions is still `pending`.
- No generated `rfix7` app files may be patched by hand.

Latest source fix:

- Skill/resources and installed-module validation now require canonical
  collection access through `ctx.db.collection('<declared-collection-name>')`.
- Validation rejects `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, cached DB facade handles, and
  app-side `ctx.db.registerSchemas`.
- Verification before commit: `node src/apps/business-os/scripts/validate-app-module.test.mjs`
  and `git diff --check`.
- Installed validator proof: the old `bench_inventory_rfix6` artifact is now
  rejected by installed `ctox business-os app validate`.

Latest local regression guard:

- `src/core/service/service.rs` has an uncommitted regression test for status
  snapshot recovery of a leased app task whose target directory is missing.
- Verification run:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`.
- This source change must be committed separately from generated app evidence
  and installed through `ctox upgrade --dev` before production signoff.

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
| Skill shape | in_progress | English, concise, resource-based, no prompt wall, requires three reference apps, clear Do/Don't list, clear green checklist. |
| Correct install location | done | Generated apps are under `runtime/business-os/installed-modules/<module-id>` and survive `ctox upgrade --dev`. |
| CTOX-native creation | in_progress | Fresh five-app bench is created through real app-create tasks, not direct file writes. `rfix7` is running. |
| Static validation | in_progress | Fresh five-app run reaches terminal queue success and installed validation green for all five apps. |
| Browser mount | pending | Fresh browser can mount all five apps from the installed module catalog. |
| Five-app browser E2E | pending | UI create/edit, reload persistence, native CTOX DB sync, and automation command dispatch pass for all five fresh apps. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module creation contract. |
| Versioning contract | pending | Existing app version metadata is audited; missing enforcement is listed or patched; users see only versions `>=1.0.0`; each `x.0.0` major is independently installable with its own app icon. |
| Install/upgrade lifecycle | in_progress | `ctox upgrade --dev` applies source fixes, preserves runtime modules, and leaves CTOX/Business OS healthy. |
| No regressions | in_progress | Relevant Rust/JS checks and browser evidence are green after final patch. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App creation uses durable tasks and agent implementation, not deterministic generated source. | Earlier deterministic builder artifacts removed; bench runner submits real app-create tasks. |
| 1. Simplify skill/resources | in_progress | Codex | Skill/resources are English, concise, reference/resource based, avoid prompt walls, and state CTOX DB/command patterns without legacy fallbacks. | Latest resources patched in `89c2a75d`; needs fresh run proof. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | Latest installed release `branch-main-20260621T055246Z`; CTOX/Business OS healthy; boot recovery requeued the stale Subscriptions lease, but non-restart recovery still needs evidence. |
| 4. Close validator/resource gaps | in_progress | Codex | Validator rejects predictable bad app artifacts before browser E2E finds them, without blocking valid vanilla apps. | `89c2a75d`; old `rfix6` artifacts are rejected by installed validation. |
| 5. Fresh five-app CTOX proof | in_progress | Codex | One fresh post-validator run reaches terminal queue success and installed validation green for five apps. | `rfix7` running; Inventory, Projects, and Quality handled/green; Subscriptions and Contracts pending. |
| 6. Browser proof | pending | Codex | Browser mount, UI persistence, reload persistence, native sync, and automation smoke pass for all five fresh apps. | Wait for `rfix7` terminal static result. |
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

Active phase: `5. Fresh five-app CTOX proof`

Current rule: finish `rfix7` through CTOX product paths. Do not hand-edit
generated app files. Before any source patch, classify the evidence as
`model_failure`, `skill_resource_gap`, `validator_gap`,
`runtime_orchestration_gap`, `data_plane_gap`, or `entry_point_gap`.

Current focus:

- Let `bench_contracts_rfix7` and `bench_subscriptions_rfix7` complete through
  normal CTOX app creation tasks.
- Watch for the same leased-without-artifacts condition without relying on a
  restart as proof of production readiness.
- If the queue stops while tasks are pending, inspect and patch the queue/app
  recovery source rather than nudging the proof run and calling it green.

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
- [ ] Prove pending Subscriptions and Contracts continue without manual
  artifact edits.
- [ ] Wait for all five `rfix7` tasks to reach terminal state.
- [ ] Run installed validation for each `rfix7` app after terminal state.
- [ ] Update Bench Matrix with terminal `rfix7` static results.
- [ ] If static validation is green, run browser E2E for all five apps.
- [ ] If static validation is red, classify failures before patching.

Current slice exit criteria:

- `ctox business-os app bench status --run-id rfix7 --validate` reports no
  pending/leased tasks and validation green for five apps, or failures are
  classified with evidence and next patches are scoped.

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
| Subscriptions | `bench_subscriptions_rfix7` | pending | skipped | pending | pending | Requeued by boot recovery after stale lease with no module dir; continue through normal queue execution. |
| Inventory | `bench_inventory_rfix7` | handled | green | pending | pending | Installed validation green; 26/26 module tests passed. |
| Projects | `bench_projects_rfix7` | handled | green | pending | pending | Installed validation green; 30/30 module tests passed. |
| Contracts | `bench_contracts_rfix7` | pending | skipped | pending | pending | Await queue execution. |
| Quality | `bench_quality_rfix7` | handled | green | pending | pending | Installed validation green after same-task repair; 10/10 module tests passed. |

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
- [ ] No ornamental third column unless the app genuinely needs it.
- [ ] No resize-column CSS unless the implemented layout actually supports it.
- [ ] Browser mount has no console/page/request failures.
- [ ] UI create/edit persists through `ctx.db`, reload, and native CTOX DB sync.
- [ ] Automation dispatches through `ctx.commandBus.dispatch` and creates a
  normal command record.

## Next Actions

1. Continue monitoring `rfix7` until Subscriptions and Contracts either run or
   expose a fresh queue/lifecycle failure.
2. Do not hand-edit `bench_subscriptions_rfix7` or
   `bench_contracts_rfix7` artifacts.
3. If pending tasks do not lease while CTOX is idle, inspect and patch the
   app-task dispatch/recovery path, verify locally, install with
   `ctox upgrade --dev`, and record the evidence here.
4. Record terminal static validation in Bench Matrix and Evidence Log.
5. If `rfix7` is static green, run browser E2E for all five fresh apps.
6. If `rfix7` is static red, classify each failure before patching.
7. For validator false positives, simplify the validator instead of adding
   arbitrary rule layers.
8. For repeated generated-app architecture mistakes, update concise skill
   resources, not long prompts.
9. For valid app output blocked by CTOX behavior, patch runtime/lifecycle/data
   plane source and install through `ctox upgrade --dev`.
10. Do not hand-edit generated app artifacts.
11. After static and browser E2E are green, verify entry paths: Chat, App
   Creator, App Store/template flow, CLI, and inbound/MCP.
12. Audit app versioning enforcement and list or patch the missing pieces.
13. Update this file after every material bench result and before handoff.

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

## Open Issues

- `rfix7` is still running. Do not mark the five-app proof green until all five
  tasks have terminal status and installed validation is green.
- `bench_subscriptions_rfix7` was recovered by boot recovery, but production
  readiness still needs evidence that pending app-create tasks continue without
  a manual restart or artifact edits.
- Contracts has not yet run.
- Subscriptions has not yet completed after being requeued.
- Entry-point proof across Chat, App Creator, App Store/template flow, CLI, and
  inbound/MCP is still pending.
- App versioning policy must be audited and either enforced or listed as missing
  implementation work.
- Browser E2E for fresh hardened apps is still pending.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out
  of this work unless explicitly requested.
