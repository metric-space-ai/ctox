# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation production-ready through the real
CTOX paths. A user should be able to ask CTOX for a small business app and
receive a runtime-installed, immediately runnable vanilla HTML/CSS/browser ESM
app that persists through CTOX DB and dispatches normal Business OS commands.

This file is the live working plan. Update it while work is being done, not only
at handoff.

## Update Protocol

Update these sections whenever work changes:

- Current State
- Active Slice
- Phase Tracker
- Bench Matrix
- Evidence Log
- Open Issues
- Next Actions

Rules:

- Keep newest factual status at the top.
- Record run ids, release ids, commits, commands, and evidence paths.
- Mark a phase `done` only when its exit criteria have evidence.
- Keep one active slice.
- Classify a failure before patching.
- Do not use this file as an app-building prompt.
- Do not patch generated app artifacts by hand for a proof run.

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Current State

Last updated: `2026-06-21`

Overall status: `in_progress`, not production-ready yet.

Current CTOX installed release:

- Active install: `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T035217Z`
- Source head: `5a555f81 fix: keep business os rxdb peer open`
- Upgrade path: `ctox upgrade --dev` completed.
- CTOX status after upgrade: `running=true`, `busy=false`,
  `pending_count=0`, `worker_active_count=0`.
- Business OS status: `ok=true`, native RxDB peer `running=true`,
  `replicationUp=true`, `http_bridge_available=false`.

Current proof run:

- Run id: `rfix6`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX.
- Evidence dir: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6`
- Static status snapshot: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782010063510.json`
- Post-upgrade browser catalog/mount proof:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/post-upgrade-catalog-mount-1782014629790.json`

Latest result:

- Static/install proof is green: all five `rfix6` apps are `handled`, installed
  validation is green, and the queue is drained.
- The previous browser catalog blocker is fixed in the installed release.
  A fresh browser with `#bench_subscriptions_rfix6` mounted the generated app
  and the browser IndexedDB catalog contains the `rfix6` modules.
- Full five-app browser E2E is still pending: each generated app must prove UI
  create/edit, reload persistence, native CTOX DB sync, and automation command
  dispatch.
- Entry-point proof is still pending: Chat, App Creator, App Store/template
  flow, CLI, and inbound/MCP paths must all attach the same app-module creation
  contract.

Root cause closed in the latest source fix:

- Failure class: `data_plane_gap`.
- Direct `masterChangesSince` against `business_module_catalog` returned
  `RC_PULL` with `SQLITE_CLOSED`.
- Fix: duplicate helper opens of the Business OS database no longer close the
  long-lived native peer database, and post-ACK query-stream failures now emit
  `rxdb.query.error`.
- Verification passed before install:
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`,
  `cargo test --manifest-path src/core/rxdb/Cargo.toml stream_error_after_ack_emits_query_error_frame`,
  `cargo test --bin ctox open_database_does_not_close_existing_business_os_instance`,
  `cargo check --bin ctox`, and `git diff --check`.

## Non-Negotiables

The app creation path must stay simple and agent-led.

Do:

- Let CTOX create apps through normal durable app-create tasks.
- Use runtime-installed app files under
  `$CTOX_INSTALL_ROOT/current/runtime/business-os/installed-modules/<module-id>`.
- Build apps as vanilla `index.html`, `index.css`, browser ESM `index.js`, and
  small browser ESM helper files when useful.
- Persist app data only through shell-provided `ctx.db` collection handles.
- Dispatch automation only through `ctx.commandBus.dispatch`.
- Use `business_os.chat.task` or another allowed Business OS command for
  app-triggered AI/ticket/chat work.
- Keep `module.json`, `collections.schema.json`, and `schema.js` in exact
  collection/version/type parity.
- Scope runtime collection names to the module id.
- Require at least three existing Business OS apps as references before app
  implementation.
- Patch source, validation, skill resources, or CTOX runtime only when evidence
  shows a systemic gap.
- Install source fixes through `ctox upgrade --dev`.

Do not:

- Do not reintroduce a deterministic app builder or direct file writer.
- Do not repair generated app artifacts by hand during a proof run.
- Do not write runtime app output into `src/`; `src/` contains source and store
  templates only.
- Do not import upstream `rxdb`, React, Next.js, Vue, package-manager
  dependencies, or build-time frameworks.
- Do not add HTTP data bridges or HTTP data fallbacks.
- Do not add long prompt blocks inside the skill or app creator.
- Do not copy source-only manifest fields from built-in modules into runtime
  app manifests.
- Do not ship UI slop: unused third columns, fake actions, nonfunctional
  buttons, hidden overlays that intercept clicks, or resize CSS without a real
  layout need.

## Production Gates

App creation is production-ready only when every gate is green.

| Gate | Status | Required Evidence |
| --- | --- | --- |
| Skill shape | in_progress | English, concise, resource-based, no prompt wall, requires three reference apps, clear Do/Don't list, clear green checklist. |
| Correct install location | done | Generated apps are under `runtime/business-os/installed-modules/<module-id>` and survive `ctox upgrade --dev`. |
| CTOX-native creation | done | Five bench apps were created through real app-create tasks, not direct file writes. |
| Static validation | done | `rfix6` snapshot `status-1782010063510.json`: handled=5, validation_passed=5, pending=0, failed=0. |
| Browser mount | done | Post-upgrade proof `post-upgrade-catalog-mount-1782014629790.json` mounts `bench_subscriptions_rfix6` and materializes the module catalog. |
| Five-app browser E2E | pending | All five `rfix6` apps prove UI create/edit, reload persistence, native CTOX DB sync, and automation command dispatch. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module creation contract. |
| Versioning contract | pending | Existing app version metadata is audited; missing enforcement is listed; users see only versions `>=1.0.0`; each `x.0.0` major is independently installable with its own app icon. |
| Install/upgrade lifecycle | in_progress | `ctox upgrade --dev` applies source fixes, preserves runtime modules, and leaves CTOX/Business OS healthy. |
| No regressions | in_progress | Relevant Rust/JS checks and browser evidence are green after the final patch. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App creation uses durable tasks and agent implementation, not deterministic generated source. | Earlier commits `e8bec3b8`, `b142e4c8`; runtime installed path verified in bench runs. |
| 1. Simplify skill/resources | in_progress | Codex | Skill/resources are English, concise, reference/resource based, and avoid prompt walls. | Needs final review after E2E failures are known. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | Latest release `branch-main-20260621T035217Z`; `ctox status --json` healthy after upgrade. |
| 4. Close validator/resource gaps | done | Codex | Validator rejects predictable bad app artifacts before browser E2E finds them. | `0dd04c31` rejects unscoped collections; `aa945a71` accepts and checks namespaced `data-*-action`. |
| 5. Fresh five-app CTOX proof | done | Codex | One fresh post-validator run reaches terminal queue success and installed validation green for five apps. | `rfix6` snapshot `status-1782010063510.json`. |
| 6. Browser proof | in_progress | Codex | Browser mount, UI persistence, reload persistence, native sync, and automation smoke pass for all five fresh apps. | Mount/catalog proof is green after `5a555f81`; five-app E2E is pending. |
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

Active phase: `6. Browser proof`

Current rule: do not patch generated app artifacts. `rfix6` is the active
post-validator bench. Static validation and browser catalog/mount are green.
The next task is full five-app browser E2E.

Immediate checklist:

- [x] Commit and push the native peer/catalog source fix to `main`.
- [x] Install the fix through `ctox upgrade --dev`.
- [x] Verify CTOX/Business OS health after upgrade.
- [x] Verify fresh browser catalog materialization and module mount.
- [ ] Inspect the five generated apps only to identify real UI selectors and
  expected command dispatch paths.
- [ ] Run browser E2E for Subscriptions.
- [ ] Run browser E2E for Inventory.
- [ ] Run browser E2E for Projects.
- [ ] Run browser E2E for Contracts.
- [ ] Run browser E2E for Quality.
- [ ] Record every E2E result in Bench Matrix and Evidence Log.
- [ ] Classify every failure before patching.
- [ ] If E2E is green, move to entry-point proof.

Current slice exit criteria:

- Browser E2E evidence exists for all five `rfix6` apps.
- Each app proves mount, UI create/edit, reload persistence, native DB sync, and
  automation command dispatch.
- Every failure is classified as app defect, browser-smoke harness gap,
  data-plane gap, validator gap, skill/resource gap, or entry-point gap.

## Bench Matrix

Active run `rfix6`:

| Case | Module Id | Queue Status | Static Validation | Browser Mount | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix6` | handled | green | green | pending | Post-upgrade mount proof uses this app; full UI/native/automation E2E still required. |
| Inventory | `bench_inventory_rfix6` | handled | green | pending | pending | Generated collections are module-scoped; browser E2E still required. |
| Projects | `bench_projects_rfix6` | handled | green | pending | pending | Browser E2E must use app-specific detail/follow-up selectors. |
| Contracts | `bench_contracts_rfix6` | handled | green | pending | pending | Browser E2E still required. |
| Quality | `bench_quality_rfix6` | handled | green | pending | pending | Browser E2E should allow enough native sync time. |

Only the latest fresh post-fix run may be used for production signoff.

## Failure Classification

Use these classes before patching:

- `model_failure`: one generated app has inconsistent logic/tests or poor UI,
  but no reusable CTOX architecture rule was missed.
- `skill_resource_gap`: multiple apps miss the same CTOX-specific architecture
  concept, or the skill/resource wording makes the correct path unclear.
- `validator_gap`: bad artifacts pass installed validation but fail predictable
  browser/static/runtime checks.
- `runtime_orchestration_gap`: queue, app lifecycle, install path, launchd,
  module catalog, native peer, or validation finalization is wrong.
- `data_plane_gap`: WebRTC/CTOX DB/schema registration/sync is wrong.
- `entry_point_gap`: a user-facing path does not attach the same app-module
  creation contract or does not create a normal durable app-create task.

Patch policy:

- Patch the skill only for repeated or clearly reusable app-building guidance.
- Patch the validator when it can reject a concrete bad artifact generically.
- Patch CTOX runtime when app output is valid but lifecycle/data/queue
  machinery fails.
- Do not patch generated app files.

## Architecture Translation Cheatsheet

| Common web-app assumption | Business OS app equivalent |
| --- | --- |
| Next.js/React app with build step | Vanilla runtime module: `index.html`, `index.css`, browser ESM `index.js`. |
| npm/package dependency | No dependency management. Only browser ESM files shipped with the app or provided by the shell. |
| App-owned database setup | Shell supplies `ctx.db`; app uses declared collection handles. |
| IndexedDB/Postgres direct access | CTOX DB over WebRTC; never an HTTP bridge and never an app-owned IndexedDB wrapper. |
| REST API write | `ctx.db.<collection>` write or `ctx.commandBus.dispatch`. |
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

## Finalization Checklist For Each New App

Use this before marking any generated app green:

- [ ] Files exist: `module.json`, `collections.schema.json`, `schema.js`,
  `index.html`, `index.css`, `index.js`, `icon.svg`, `locales/en.json`,
  `locales/de.json`, tests, and helper ESM where needed.
- [ ] App is in the runtime installed-module directory, not `src/`.
- [ ] `module.json`, `collections.schema.json`, and `schema.js` agree.
- [ ] Runtime collection names are scoped to the module id.
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

1. Run the five-app browser E2E for `rfix6`.
2. For each app, record evidence paths and update Bench Matrix immediately.
3. If a test fails, classify it before patching.
4. Patch only the smallest systemic layer shown by evidence: skill resource,
   validator, runtime/data plane, or browser-smoke harness.
5. Do not hand-edit generated app artifacts.
6. After five-app E2E is green, verify entry paths: Chat, App Creator,
   App Store/template flow, CLI, and inbound/MCP.
7. Audit app versioning enforcement and list or patch the missing pieces.
8. Update this file before handoff and after every material bench result.

## Evidence Log

- `2026-06-21`: `rfix6` snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782010063510.json`
  shows `bench_green=true`, `handled=5`, `validation_passed=5`, `pending=0`,
  `failed=0`.
- `2026-06-21`: browser diagnostics
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/transport-peer-pending-probe-1782012991802.json`
  and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/master-pull-probe-1782013261805.json`
  narrowed the catalog blocker to a native peer database close after duplicate
  helper opens.
- `2026-06-21`: commit `5a555f81` fixed the native peer duplicate-open behavior
  and post-ACK query-stream error reporting. Local verification passed:
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`,
  `cargo test --manifest-path src/core/rxdb/Cargo.toml stream_error_after_ack_emits_query_error_frame`,
  `cargo test --bin ctox open_database_does_not_close_existing_business_os_instance`,
  `cargo check --bin ctox`, and `git diff --check`.
- `2026-06-21`: `ctox upgrade --dev` installed
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T035217Z`.
  `ctox status --json` reports CTOX running, no pending tasks, Business OS
  healthy, native RxDB peer `replicationUp=true`, and
  `http_bridge_available=false`.
- `2026-06-21`: post-upgrade browser proof
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/post-upgrade-catalog-mount-1782014629790.json`
  shows a fresh browser mounted `bench_subscriptions_rfix6`; IndexedDB contains
  the module catalog and `rfix6` module ids.
- `2026-06-21`: historical validator hardening: `0dd04c31` rejects unscoped
  runtime collections and `aa945a71` accepts/validates namespaced
  `data-*-action="new"` affordances.

## Open Issues

- Full five-app browser E2E for `rfix6` is still pending.
- Entry-point proof across Chat, App Creator, App Store/template flow, CLI, and
  inbound/MCP is still pending.
- App versioning policy must be audited and either enforced or listed as missing
  implementation work.
- Skill/resource final review is still pending after browser E2E evidence is
  complete.
- Queue-drain latency should stay under watch, but the latest `rfix6` run
  completed all five app-create tasks without manual restart.
- Historical `rfix5` artifacts are invalid under the hardened validator because
  their runtime collections are not module-id scoped.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out of
  this work unless explicitly requested.
