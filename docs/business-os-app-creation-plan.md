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

Overall status: `done`. The CTOX app-creation path is production-ready for
the clean five-app MiniMax M3 bench after real skill dispatch, including
installed validation, browser smoke, save/reload persistence, native CTOX DB
sync, and command-bus automation. Entry-point coverage is now installed and
verified. The installed runtime now enforces the major-version app-line rule
for releases; required production gates are green. Remaining items are
non-blocking watch/UX follow-ups, not app-creation blockers.

Current failure classification:

- `entry_point_tool_gap`: fixed and installed. CTOX now exposes small typed
  create/modify entry tools that submit normal durable Business OS commands and
  do not write app artifacts.
- `versioning_major_line_gap`: source now rejects an in-place release that
  would move an existing public runtime app line from Major `1+` to another
  Major `1+` on the same module id. Same-major releases remain allowed, and
  `0.x` to `1.0.0` remains the normal first Team release. This fix is verified
  locally and in the installed runtime.

Installed CTOX:

- Source branch: `main`
- Last runtime-relevant source head installed and checked:
  `fd97322c Update app creation versioning install status`
- Installed versioning source commit:
  `dc544612 Enforce Business OS app major release lines`
- Active install:
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T191207Z`
- Install path: applied through `ctox upgrade --dev`
- State root:
  `/Users/michaelwelsch/.local/state/ctox`
- Runtime app target:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/<module-id>`
  which resolves into the managed runtime/state root. Runtime apps must not be
  written into source paths.
- Latest CTOX status after install and cleanup: `running=true`,
  `busy=false`, Business OS `ok=true`, native RxDB peer `replicationUp=true`,
  `http_bridge_available=false`, `pending_count=0`, `blocked_count=0`.
- Disk/build cleanup after install removed generated Cargo/build/cache output
  from `/Users/michaelwelsch/.cache/ctox`, release-local desktop build output,
  and generated backup `node_modules`/browser cache output. Free disk space is
  about `266 GiB`; retained CTOX backup contents are SQLite rollback state, not
  build artifacts.

Current proof run:

- Run id: `rfix12`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX
- Evidence dir:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12`
- Static status:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/status-1782055293408.json`
- Browser smoke evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/browser-smoke/`
- Deep browser E2E evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/deep-e2e/`

Latest result:

- `rfix12` is the first clean post-dispatch CTOX bench proof.
- Bench status with `--validate`: `bench_green=true`, `needs_attention=false`,
  5 handled, 0 pending, 0 leased, 0 failed, 0 blocked, 0 cancelled,
  5 validation passed, 0 validation failed, and no missing required files.
- Installed browser smoke is green for all five generated runtime apps:
  Subscriptions, Inventory, Projects, Contracts, and Quality. Each smoke opened
  the primary Create/New flow in the real Business OS shell with no console
  errors, page errors, or failed requests.
- Installed deep browser E2E is green for all five generated runtime apps:
  Subscriptions, Inventory, Projects, Contracts, and Quality. Each E2E creates a
  primary record through the real UI, observes the marker after save, reloads
  the module and observes the marker again, verifies the marker in native CTOX
  DB/RxDB SQLite tables, clicks a record-scoped CTOX follow-up/review action,
  and verifies a matching native `business_commands` row.
- The first red Subscriptions deep-E2E attempt clicked a global batch action
  before the record action; the first red Contracts deep-E2E attempt filled
  toolbar filters outside the form. Both were classified and fixed as
  `test_harness_gap`, not generated-app, skill, or deterministic-builder fixes.
- Projects hit one transient `database is locked` worker/rework event before
  succeeding on the same normal queue task. Classification:
  `runtime_orchestration_gap` to watch, not a skill or app-code failure.
- The installed app smoke CLI is a validation tool only; it must not generate,
  repair, or rewrite app artifacts.
- The installed app E2E CLI is a validation/proof tool only; it must not
  generate, repair, or rewrite app artifacts.
- No generated `rfix8`, `rfix9`, `rfix10`, `rfix11`, or `rfix12` app files may
  be patched by hand.
- App Creator entry-point update in installed CTOX: the Creator now creates a
  normal `ctox.business_os.app.create` command directly from the user's app
  request. Technical fields are optional hints only; there is no local
  specification derivation, preset-to-schema mapping, or layout/collection
  guessing. Installed verification:
  `ctox business-os app validate creator --source --json`,
  `node /Users/michaelwelsch/.local/lib/ctox/current/src/apps/business-os/modules/creator/creator.test.mjs`,
  and an installed-source audit for removed deterministic markers. Local
  verification:
  `node src/apps/business-os/modules/creator/creator.test.mjs`,
  `node src/apps/business-os/scripts/validate-app-module.mjs creator --source --json`,
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`, and
  `node --check` for the changed JS tools.

Historical source fix: read-only validation:

- Classification: `runtime_orchestration_gap`, not skill text or model output.
- Patch in `src/core/service/business_os.rs`: remove the side effect that made
  `ctox business-os app validate` complete matching leased app-creator queue
  tasks. `validate` now runs static/module tests and reports only.
- Explicit mutation remains available through
  `ctox business-os app finalize <module-id> --task-id <queue-task-id>`.
- Regression guard:
  `app_validate_success_does_not_finalize_matching_leased_creator_task`.
- Verification runs:
  `cargo test --bin ctox app_validate_success_does_not_finalize_matching_leased_creator_task -- --nocapture`,
  `cargo test --bin ctox app_bench_run_submits_real_tasks_without_writing_app_artifacts -- --nocapture`,
  and `rustfmt --check src/core/service/business_os.rs`.
- Source fix status: committed, pushed, and installed as
  `branch-main-20260621T110239Z`.

Historical source fix:

- Classification: `skill_resource_gap` plus `validator_gap`.
- Patch in `src/skills/system/product_engineering/business-os-app-module-development/references/`
  clarifies that runtime app `mount(ctx)` must load `index.html` into
  `ctx.host` or render an equivalent primary UI itself.
- Patch in
  `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`
  rejects installed modules that expose a primary create action only in
  `index.html` but never render that fragment into `ctx.host`.
- Regression guard:
  `src/apps/business-os/scripts/validate-app-module.test.mjs` adds the
  `shellpreload` negative control.
- Verification runs:
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`,
  `node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`,
  `node --check src/apps/business-os/scripts/validate-app-module.test.mjs`, and
  source-validator checks against all five `rfix10` modules. The new validator
  rejects the four browser-smoke-red apps and passes Projects.
- Source fix status: committed, pushed, installed as
  `branch-main-20260621T125644Z`, and superseded by the clean `rfix12`
  smoke/deep-E2E proof.

Historical source fix: app queue serialization:

- Classification: `runtime_orchestration_gap`, not skill text or model output.
- Patch in `src/core/service/service.rs`: serialize Business OS app queue
  leasing so only one app-create queue task can be leased while another app
  task is still leased or app recovery is active. The normal durable queue can
  still lease non-app tasks when appropriate.
- Regression guard:
  `app_queue_finalization_does_not_overlap_next_app_lease`.
- Verification runs:
  `cargo test --bin ctox app_queue_finalization_does_not_overlap_next_app_lease -- --nocapture`,
  `cargo test --bin ctox app_rework_waits_for_idle_despite_stale_inflight_key -- --nocapture`,
  `cargo test --bin ctox durable_queue -- --nocapture`,
  `cargo test --bin ctox business_os_app_module_tasks_skip_full_workspace_desktop_sync -- --nocapture`,
  and `rustfmt --check src/core/service/service.rs`.
- Source fix status: tested locally, committed, pushed, and installed.
- Source fix commit: `85ee58d2 Serialize Business OS app queue leasing`.
- Installed through `ctox upgrade --dev` as
  `branch-main-20260621T084616Z`.
- Installed-source proof:
  `/Users/michaelwelsch/.local/lib/ctox/current/src/core/service/service.rs`
  contains `durable_queue_lease_in_progress`,
  `leased_business_os_app_queue_task_exists`,
  `app_queue_finalization_does_not_overlap_next_app_lease`, and
  `app_rework_waits_for_idle_despite_stale_inflight_key`.

Installed smoke/tooling status:

- Added `src/apps/business-os/scripts/smoke-app-module.mjs`.
- Added `ctox business-os app smoke <module-id>` CLI wiring in
  `src/core/service/business_os.rs` and top-level help in `src/core/main.rs`.
- Updated concise skill resources to require real-shell Create/New/Add smoke
  and to call out DOM-scope handling for sibling dialogs/forms.
- Commit `5811f9c0 Add Business OS app browser smoke gate` was pushed to
  `main` and installed as `branch-main-20260621T073607Z`.
- Commit `710c3676 Fix installed Business OS app smoke runtime` was pushed to
  `main` and installed as `branch-main-20260621T074730Z`.
- Verification so far:
  `node --check src/apps/business-os/scripts/smoke-app-module.mjs`,
  `node src/apps/business-os/scripts/smoke-app-module.mjs bench_quality_rfix7 --installed --json --timeout-ms 90000`,
  `cargo run --bin ctox -- business-os app smoke bench_quality_rfix7 --installed --json --timeout-ms 90000`,
  installed `ctox business-os app smoke bench_quality_rfix7 --installed --json --timeout-ms 90000`,
  and negative CLI/tool controls against Inventory and Contracts.
- Installed smoke proof:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-quality-installed-runtime-loader.json`.
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

Historical regression guard:

- `src/core/service/service.rs` has a committed regression test for status
  snapshot recovery of a leased app task whose target directory is missing.
- Verification run:
  `cargo test --bin ctox status_snapshot_recovery_requeues_missing_app_target_without_prefetch -- --nocapture`.
- The worker-finalization sync-lock fix is committed, pushed, and installed.

## Non-Negotiables

The app creation path must stay simple, agent-led, and product-native.

Do:

- Let CTOX create apps through normal durable app-create tasks.
- Use typed product tools for entry points:
  `ctox business-os app create --instruction <text>`,
  `ctox business-os app modify <module-id> --instruction <text>`,
  MCP `business_os.create_app`, and MCP `business_os.modify_app`.
- Keep those tools as delegators only. They may submit Business OS commands,
  attach the app skill, and report command/task ids; they must not choose
  schemas, layouts, files, or code.
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
- Do not expose app creation as a generic `--prompt` API; use explicit user
  `instruction` plus CTOX resources/tools.
- Do not copy source-only manifest fields from built-in modules into runtime
  app manifests.
- Do not ship UI slop: unused third columns, fake buttons, hidden overlays that
  intercept clicks, resize CSS without real resize behavior, or dead actions.

## Production Gates

App creation is production-ready only when every gate is green.

| Gate | Status | Required Evidence |
| --- | --- | --- |
| Skill shape | done | English, concise, resource-based, no prompt wall, requires three reference apps, clear Do/Don't list, clear green checklist, includes browser-smoke finalization. `rfix12` proves the skill shape is usable by MiniMax M3 through CTOX. |
| Skill dispatch | done | A bound `suggested_skill` becomes an exact skill-body injection through a linked `SKILL.md` when the local skill name resolves uniquely. Commit `791d6da6` is pushed and installed as `branch-main-20260621T134556Z`; installed source contains `required_skill` and no `preferred_skill`. |
| Correct install location | done | `rfix12` generated all five apps under `runtime/business-os/installed-modules/<module-id>`, never under `src/`. |
| CTOX-native creation | done | `rfix12` created five apps through real app-create queue tasks, not a deterministic builder or direct bench file writer. |
| Static validation | done | `rfix12` reached terminal queue success and installed validation green for all five apps. Status evidence: `status-1782055293408.json`. |
| Browser mount | done | `rfix12` installed browser smoke is green for all five apps, including primary Create/New flow visibility and zero console/page/request failures. Evidence: `rfix12/browser-smoke/*.json`. |
| Five-app browser E2E | done | `rfix12/deep-e2e/*.json` is green for all five apps. Each app creates data through the real UI, reloads with the record still visible, syncs to native CTOX DB/RxDB SQLite tables, and dispatches a record-scoped command-bus automation. |
| Entry-point coverage | done | Queue/app-create path is proven by `rfix12`. App Creator, shell context chat, App Store selected-app chat, source-module app chat, CLI, MCP, and Matching context metadata all route to `ctox.business_os.app.create/modify` with runtime install targets. Installed CLI/MCP and Matching proof is green in `branch-main-20260621T182855Z`. |
| Versioning contract | done | Native and browser code enforce valid SemVer, private pre-1.0 runtime apps, Team visibility for Major >= 1, Team release `target_version >= 1.0.0`, and installed release `branch-main-20260621T191207Z` rejects in-place public Major-line bumps on the same runtime module id. Installed proof: `cargo test --bin ctox module_release_rejects_in_place_public_major_line_bump -- --nocapture` passed from `/Users/michaelwelsch/.local/lib/ctox/current`. |
| Install/upgrade lifecycle | done | `ctox upgrade --dev` applied latest `main` as `branch-main-20260621T191207Z`; installed CTOX reports `running=true`, Business OS `ok=true`, native peer `replicationUp=true`, and `http_bridge_available=false`. Generated build/cache artifacts were cleaned after install/test. Watch the non-fatal sudo/launchctl warning separately if it becomes user-visible. |
| No regressions | done | Relevant Rust/JS checks, browser evidence, installed source checks, and installed major-line regression test are green for the latest app-creation path. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App creation uses durable tasks and agent implementation, not deterministic generated source. | Earlier deterministic builder artifacts removed; bench runner submits real app-create tasks. |
| 1. Simplify skill/resources | done | Codex | Skill/resources are English, concise, reference/resource based, avoid prompt walls, state CTOX DB/command patterns without legacy fallbacks, and require browser-smoke proof. | `rfix12` produced 5/5 valid runtime apps with MiniMax M3 through CTOX after the skill-dispatch fix. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | done | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | `rfix12` completed 5/5. Latest installed release `branch-main-20260621T191207Z` reports healthy service, Business OS, module catalog, native peer replication, and zero pending/blocked tasks. The prior Projects `database is locked` event remains a watch item only if it recurs. |
| 4. Close validator/resource gaps | done | Codex | Validator/tooling rejects predictable bad app artifacts before signoff, without blocking valid vanilla apps. | `89c2a75d` rejects old DB fallbacks; `5811f9c0`/`710c3676` add and install `ctox business-os app smoke`; `1a15ed72` rejects runtime apps that never render their `index.html` primary create UI into `ctx.host`. Do not add more validator rules unless a new systemic contract gap is proven. |
| 5. Fresh five-app CTOX proof | done | Codex | One fresh post-skill-dispatch run reaches terminal queue success and installed validation green for five apps. | `rfix12`: 5 handled, 0 failed/blocked/cancelled, 5/5 installed validations green. |
| 6. Browser proof | done | Codex | Browser mount, UI persistence, reload persistence, native sync, and automation smoke pass for all five fresh apps. | `rfix12` smoke and deep E2E are green for all five apps. Evidence: `rfix12/browser-smoke/*.json` and `rfix12/deep-e2e/*.json`. |
| 7. Skill dispatch proof | done | Codex | Bound queue/app tasks load the exact skill body through the harness skill injector. | Commit `791d6da6` renders linked `SKILL.md` mentions for unique suggested skills; installed release `branch-main-20260621T134556Z` contains the dispatch code and the Business OS app skill file. |
| 8. Entry-point proof | done | Codex | Every user-facing app creation/modification path uses the same skill/resource context and runtime app contract. | Queue path is proven by `rfix12`; App Creator is installed and verified; CLI/MCP tools are installed and proven in `branch-main-20260621T173949Z`; Matching metadata normalization is committed, pushed, installed, and verified in `branch-main-20260621T182855Z`. |
| 9. Versioning proof | done | Codex | App version visibility and major-version independence are either implemented or listed as missing work. | Visibility/release enforcement exists and is tested. Commit `dc544612` rejects in-place public Major-line bumps on the same module id; installed release `branch-main-20260621T191207Z` contains the guard and passes `module_release_rejects_in_place_public_major_line_bump`. |
| 10. Production signoff | done | Codex | All production gates are green, latest runtime-relevant source is installed, plan/docs updated, no unrelated dirty files staged. | Production gates are green after `branch-main-20260621T191207Z`; generated build/cache output was cleaned; final plan-only commits do not require another install; only pre-existing unrelated dirty files remain unstaged. |

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

Active phase: `10. Production signoff`

Current rule: app creation is signed off. Do not add more app-generation
heuristics or deterministic artifact repair without a new classified systemic
failure.

Current focus:

- Keep `rfix12` as the current clean app-creation proof.
- Do not patch generated `rfix12` app artifacts.
- Keep the installed native major app-line release policy green.
- Keep CLI/MCP app create/modify tools routing-only. If a tool starts writing
  app files, deriving an app spec, or picking app internals, revert that design
  before testing.
- Keep the simplified App Creator installed proof green while verifying the
  remaining entry paths.
- Keep app versioning source-side: release policy may reject invalid in-place
  major bumps, but it must not generate, copy, or rewrite a replacement app.
- Keep the app E2E command as a proof tool only. It must not create, rewrite,
  or repair generated app files.

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
- [x] Commit and push the browser-smoke/skill patch to `main`.
- [x] Install the browser-smoke/skill patch through `ctox upgrade --dev`.
- [x] Fix the smoke runner to use the installed CTOX browser/Patchright runtime
  instead of source-local `playwright`.
- [x] Install the runtime-loader smoke fix through `ctox upgrade --dev`.
- [x] Start fresh five-app CTOX bench run after install.
- [x] `rfix8` Subscriptions reached terminal success with installed validation
  and browser smoke green.
- [x] Classify `rfix8` post-Subscriptions finalization stall as
  `runtime_orchestration_gap`.
- [x] Patch finalization to skip full workspace desktop indexing for runtime
  app module tasks.
- [x] Verify the new skip rule and the existing normal workspace sync path with
  targeted Rust tests.
- [x] Commit and push the app-finalization throughput fix.
- [x] Install the app-finalization throughput fix through `ctox upgrade --dev`.
- [x] Classify the `rfix8` double app lease as `runtime_orchestration_gap`.
- [x] Patch durable app queue leasing so only one Business OS app-create task
  can be leased while another app task is still leased or app recovery is active.
- [x] Verify app queue serialization with focused Rust tests.
- [x] Commit and push the app queue serialization fix.
- [x] Install the app queue serialization fix through `ctox upgrade --dev`.
- [x] Verify installed source contains the app queue serialization guard.
- [x] Let the forensic `rfix8` queue drain without hand-editing app artifacts.
- [x] Start fresh post-serialization five-app run `rfix9`.
- [x] Classify `rfix9` contamination as `runtime_orchestration_gap`: installed
  `ctox business-os app validate` finalized a live leased Projects app task.
- [x] Patch local source so `validate` is read-only and `finalize` is the only
  app-validation CLI path that mutates queue/command state.
- [x] Verify the read-only validation fix with focused Rust tests.
- [x] Commit and push the read-only validation fix.
- [x] Install the read-only validation fix through `ctox upgrade --dev`.
- [x] Start fresh post-read-only-validate five-app run `rfix10`.
- [x] `rfix10` reached terminal queue success with installed validation green
  for all five apps.
- [x] Run browser smoke for all five `rfix10` apps.
- [x] Classify the `rfix10` browser-smoke failures as `skill_resource_gap` plus
  `validator_gap`: runtime `mount(ctx)` assumed shell-preloaded `index.html`.
- [x] Patch local skill resources and static checker for the runtime
  `index.html` loading contract.
- [x] Verify the local validator rejects the four `rfix10` smoke-red apps and
  passes Projects.
- [x] Commit and push the runtime `index.html` loading validator/resource fix.
- [x] Install the validator/resource fix through `ctox upgrade --dev`.
- [x] Start fresh five-app run `rfix11`.
- [x] Classify `rfix11` as contaminated because it started before bound
  `suggested_skill` produced a real skill-body injection.
- [x] Patch local source so `suggested_skill` renders as a linked `SKILL.md`
  mention when the skill name resolves to exactly one local skill file.
- [x] Format and test the suggested-skill dispatch patch.
- [x] Commit and push the suggested-skill dispatch patch.
- [x] Install the suggested-skill dispatch patch through `ctox upgrade --dev`.
- [x] Verify installed source contains the linked skill dispatch path.
- [x] Start fresh post-dispatch five-app run `rfix12`.
- [x] `rfix12` reached terminal queue success with installed validation green
  for all five apps.
- [x] Run installed browser smoke for all five `rfix12` apps.
- [x] Persist browser-smoke JSON evidence under the `rfix12` evidence dir.
- [x] Classify the Projects `database is locked` event as a transient
  `runtime_orchestration_gap` watch item because same-task rework completed
  without generated app patching.
- [x] Add installed `ctox business-os app e2e` proof tooling without app
  generation or artifact repair.
- [x] Fix the E2E harness to prefer record-scoped automation actions over
  global batch actions.
- [x] Fix the E2E harness to fill only visible forms, not toolbar search/filter
  controls.
- [x] Run installed deep E2E for all five `rfix12` apps: save, reload
  persistence, native DB/RxDB sync, and command-bus automation are green.
- [x] Remove App Creator's deterministic local specification step, quickstart
  presets, and generator/harness registry language.
- [x] Make App Creator submit a direct `ctox.business_os.app.create` command
  from the user's app request, with optional metadata hints only.
- [x] Split app validator behavior so runtime app checks stay strict while
  internal source modules such as App Creator are not falsely treated as
  runtime-generated user apps.
- [x] Commit and push the App Creator simplification/tool-boundary checkpoint.
- [x] Install the simplified App Creator source through `ctox upgrade --dev`.
- [x] Verify installed App Creator validation is green in
  `branch-main-20260621T170122Z`.
- [x] Add source CLI and MCP app create/modify delegators that enqueue normal
  Business OS commands and do not write app artifacts.
- [x] Verify source CLI/MCP delegators with targeted Rust tests.
- [x] Install source CLI/MCP delegators through `ctox upgrade --dev`.
- [x] Verify installed CLI and inbound/MCP tool surfaces.
- [x] Audit Chat, App Store/template, CLI, and inbound/MCP entry points.
- [x] Audit app versioning enforcement.
- [x] Commit/install Matching legacy command-type metadata cleanup.
- [x] Add source enforcement for independent public major app lines.
- [x] Verify source major-line enforcement with focused Rust tests.
- [x] Commit and push major app-line release enforcement.
- [x] Install major app-line release enforcement.
- [x] Verify installed major app-line release enforcement.
- [x] Clean generated build/cache artifacts after install and installed test.

Current slice exit criteria:

- `rfix12` remains the clean five-app creation proof.
- User-facing entry paths prove they use the same app-module skill/resource
  contract and runtime install target as the green queue/app-create path.
- App versioning behavior is green in source and installed runtime.
- Any new failure is classified before patching. Generated app artifacts remain
  read-only for proof purposes.

## Bench Matrix

Historical run `rfix6`:

| Case | Module Id | Queue Status | Static Validation | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |
| Inventory | `bench_inventory_rfix6` | handled | invalid after hardening | red | Data persisted into browser/native DB but UI reopened with `ITEMS 0`; generated source used forbidden DB fallbacks. |
| Projects | `bench_projects_rfix6` | handled | invalid after hardening | harness-blocked | App looked plausible; browser smoke clicked hidden modal close; generated source used forbidden DB fallbacks. |
| Contracts | `bench_contracts_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |
| Quality | `bench_quality_rfix6` | handled | invalid after hardening | green before hardening | Browser path worked, but generated source used forbidden DB fallbacks. |

Completed forensic run `rfix7`:

| Case | Module Id | Queue Status | Static Validation | Browser Mount | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix7` | handled | green | mounted in first five-app E2E | red | Create flow timed out in first five-app E2E; source inspection shows modal refs queried from the inner root while dialogs are outside it. |
| Inventory | `bench_inventory_rfix7` | handled | green | red | red | Browser smoke catches mount error at `index.js:92` and dead `new-item` create flow. |
| Projects | `bench_projects_rfix7` | handled | green | mounted in first five-app E2E | red | Create flow timed out in first five-app E2E; source inspection shows same modal/root-scope pattern. |
| Contracts | `bench_contracts_rfix7` | handled | green | mounted | red | Browser smoke observes `new-contract` click, but no dialog/form/save flow appears. |
| Quality | `bench_quality_rfix7` | handled | green | mounted | partial green | Smoke green for `create-complaint`; prior focused E2E proved browser collection, native SQLite, and reload persistence. Automation still needs full fresh-run proof. |

Completed forensic run `rfix8`:

| Case | Module Id | Queue Status | Static Validation | Browser Smoke | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix8` | handled | green | green | Terminal success; installed smoke clicked `add-subscription` and revealed a form/save flow. |
| Inventory | `bench_inventory_rfix8` | handled | green | not run | Originally leased at `2026-06-21T08:27:26Z` under pre-fix code, then requeued as unstarted after the queue serialization fix and later completed. |
| Projects | `bench_projects_rfix8` | handled | green | not run | Completed after the app-queue serialization fix; not production signoff because the run already contained pre-fix overlap. |
| Contracts | `bench_contracts_rfix8` | handled | green | not run | Completed after the app-queue serialization fix. |
| Quality | `bench_quality_rfix8` | handled | green | not run | Was leased at `2026-06-21T08:27:25Z` and later reached terminal success. The overlapping lease is runtime evidence, not app evidence. |

Contaminated forensic run `rfix9`:

| Case | Module Id | Queue Status | Static Validation | Browser Smoke | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix9` | handled | skipped in latest status | not run | Reached terminal success before the run was declared contaminated. |
| Inventory | `bench_inventory_rfix9` | handled | skipped in latest status | not run | Reached terminal success after same-task rework; generated extra locale `.js` files but no source artifact was hand-edited. |
| Projects | `bench_projects_rfix9` | handled | contaminated | not run | `ctox business-os app validate bench_projects_rfix9` finalized the live leased task at `2026-06-21T10:46:46Z`; the worker later continued and malformed action attributes. This is a CLI lifecycle bug. |
| Contracts | `bench_contracts_rfix9` | handled | skipped in latest status | not run | Reached terminal success after same-task repair. |
| Quality | `bench_quality_rfix9` | pending | skipped | not run | Pending at latest status because Projects stayed active under the contaminated lifecycle. |

`rfix9` remains forensic-only and is superseded by clean `rfix10` and `rfix12`
evidence.

Clean post-read-only-validation run `rfix10`:

| Case | Module Id | Queue Status | Static Validation | Browser Smoke | Notes |
| --- | --- | --- | --- | --- | --- |
| Projects | `bench_projects_rfix10` | handled | green | green | Correctly loads `index.html` into `ctx.host`, exposes `create-project`, opens a dialog/form/save flow. |
| Inventory | `bench_inventory_rfix10` | handled | green | red | Browser smoke found no visible primary create action. Source clears `ctx.host` and assumes the shell already loaded `index.html`. The direct agent session hit the 1800s limit, then CTOX validation accepted the repaired artifacts. |
| Subscriptions | `bench_subscriptions_rfix10` | handled | green | red | Browser smoke found no visible primary create action. `index.html` contains Create UI, but `mount(ctx)` never renders it into `ctx.host`. |
| Quality | `bench_quality_rfix10` | handled | green | red | Browser smoke found no visible primary create action. `index.html` contains Create UI, but `mount(ctx)` never renders it into `ctx.host`. |
| Contracts | `bench_contracts_rfix10` | handled after rework | green | red | First attempt was missing required files, same-task rework repaired static artifacts. Browser smoke found no visible primary create action because `mount(ctx)` never renders `index.html` into `ctx.host`. |

`rfix10` proves durable CTOX creation, queue serialization, read-only
validation, and installed static validation. It is not production signoff
because browser smoke is red for 4/5 apps.

Clean post-skill-dispatch run `rfix12`:

| Case | Module Id | Queue Status | Static Validation | Browser Smoke | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix12` | handled | green | green | Runtime-installed app with primary `create-subscription` flow; browser evidence has no console/page/request failures. |
| Inventory | `bench_inventory_rfix12` | handled | green | green | Runtime-installed app with primary `create-item` flow; browser evidence has no console/page/request failures. |
| Projects | `bench_projects_rfix12` | handled after same-task rework | green | green | Initial worker/rework hit `database is locked`; same normal queue task later created a complete app and passed validation/smoke. Watch as orchestration evidence, not skill/app evidence. |
| Contracts | `bench_contracts_rfix12` | handled | green | green | Runtime-installed app with primary `create-contract` flow; browser evidence has no console/page/request failures. |
| Quality | `bench_quality_rfix12` | handled | green | green | Runtime-installed app with primary `create-case` flow; browser evidence has no console/page/request failures. |

`rfix12` proves the CTOX app-create path can produce five immediately mounted
runtime-installed vanilla apps with MiniMax M3 and the Business OS app skill.
The installed deep E2E proof also verifies UI save, reload persistence, native
CTOX DB/RxDB sync, and record-scoped command-bus automation for all five apps.

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

Verified behavior:

- Native lifecycle projection parses plain SemVer only and treats invalid or
  missing runtime app versions as private.
- Runtime-installed apps with Major `0` remain private/preview unless explicit
  app-view grants exist.
- Runtime-installed apps with Major `>= 1` are Team-visible by default unless
  explicitly restricted.
- App Store release dispatch requires `ctox.module.release`, valid SemVer, and
  `target_version >= 1.0.0` for Team releases.
- Browser lifecycle helpers mirror the same plain-SemVer and visibility rules.
- Native release policy rejects an in-place public Major-line bump on the same
  runtime module id. Example: an existing released `1.x.y` app may release
  `1.1.0`, but a `2.0.0` release over the same module id fails and leaves the
  existing `1.x` manifest/release state intact. A `2.0.0` line must be built as
  a separate app/module id through the normal app-create path.

Missing runtime work:

- No required runtime work remains for app-creation production signoff.
- Optional UX follow-up after the enforcement lands: make App Store/Creator
  offer a clear "new major app line" handoff that uses the same normal
  app-create command path. This must not become a deterministic app copier.

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

1. Do not hand-edit generated `rfix12` app artifacts.
2. If a future app-creation proof fails, classify it before patching:
   `model_failure`, `skill_resource_gap`, `validator_gap`,
   `runtime_orchestration_gap`, `data_plane_gap`, or `entry_point_gap`.
3. Patch skill/resources only for repeated or clearly reusable app-building
   guidance gaps; patch runtime only for lifecycle/data/queue failures.
4. Keep CLI/MCP/App Creator/App Store/chat entry points as routing-only
   delegators into normal durable app-create/app-modify commands.
5. Optional UX follow-up: add a "new major app line" handoff that submits the
   normal app-create command. Do not implement a deterministic copier.
6. Treat the observed Projects `database is locked` event as a watch item. Patch
   runtime orchestration only if it recurs or leaves a task non-terminal.

## Evidence Log

- `2026-06-21`: root-cause correction after deterministic-builder drift:
  current work classifies the remaining problem as `entry_point_tool_gap`.
  Correct fix class is a small set of routing/validation tools plus concise
  skill resources. Forbidden fix class remains deterministic app generation,
  template writing, schema/layout derivation, or generated-artifact repair.
- `2026-06-21`: source CLI/MCP tool boundary added for app creation and
  modification. `ctox business-os app create --instruction <text>`,
  `ctox business-os app modify <module-id> --instruction <text>`,
  MCP `business_os.create_app`, and MCP `business_os.modify_app` submit normal
  Business OS command records targeting
  `runtime/business-os/installed-modules/<module-id>` and
  `business-os-app-module-development`; they do not write app artifacts.
  Verification:
  `cargo test --bin ctox app_create -- --nocapture`,
  `cargo test --bin ctox app_modify -- --nocapture`,
  `cargo test --bin ctox create_app_tool_enqueues_agent_led_app_command_without_writing_files -- --nocapture`,
  `cargo test --bin ctox tool_descriptors_expose_only_typed_business_os_tools -- --nocapture`,
  `rustfmt --edition 2021 src/core/service/business_os.rs src/core/business_os/mcp_channel.rs src/core/main.rs`,
  and `git diff --check` for the changed files.
- `2026-06-21`: commit `ab3c1308` (`Add Business OS app creation entry tools`)
  was pushed to `main` and installed through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T173949Z`.
  Installed verification:
  `ctox version --json` reports
  `current_release=branch-main-20260621T173949Z`;
  `ctox status --json` reports `running=true`, Business OS `ok=true`, native
  peer `replicationUp=true`, `http_bridge_available=false`, `pending_count=0`,
  and `blocked_count=0`;
  `ctox business-os app create --help` and
  `ctox business-os app modify --help` expose `--instruction`; and
  `ctox business-os mcp tools` lists `business_os.create_app` and
  `business_os.modify_app`.
- `2026-06-21`: generated build/cache cleanup after install removed
  `/Users/michaelwelsch/.cache/ctox/cargo-target`, the transient desktop build
  output, and old build-heavy CTOX update snapshots. Free disk space is about
  `272 GiB`. One current update rollback backup remains because it contains
  SQLite state backup files, not build artifacts.
- `2026-06-21`: entry-point source audit found one legacy Matching
  context-action metadata type `business_os.app.modify`; the actual Matching
  chat-submit path already used `ctox.business_os.app.modify`. Source cleanup
  normalizes the metadata value to `ctox.business_os.app.modify`. Verification:
  `node --check src/apps/business-os/modules/matching/ui/businessOsControls.js`,
  `node src/apps/business-os/modules/matching/test.mjs`,
  `node src/apps/business-os/modules/creator/creator.test.mjs`,
  `node src/apps/business-os/modules/app-store/app-store.test.mjs`,
  `node src/apps/business-os/shared/app-lifecycle.test.mjs`,
  `node src/apps/business-os/shared/permissions.test.mjs`, and
  `git diff --check` for the changed files.
- `2026-06-21`: commit `ca116f37` (`Normalize app modify entry point
  metadata`) was pushed to `main` and installed through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T182855Z`.
  Installed verification: `ctox version --json` reports
  `current_release=branch-main-20260621T182855Z`; `ctox status --json` reports
  `running=true`, Business OS `ok=true`, native peer `replicationUp=true`,
  `http_bridge_available=false`, `pending_count=0`, and `blocked_count=0`;
  installed source contains `ctox.business_os.app.modify` in the Matching
  context-action metadata; `ctox business-os app create --help`,
  `ctox business-os app modify --help`, and `ctox business-os mcp tools` expose
  the app create/modify entry tools. Generated Cargo/build/cache output was
  cleaned after install; retained backups are SQLite state.
- `2026-06-21`: versioning source audit found implemented behavior in native
  and browser code: plain SemVer parsing, invalid/missing versions private,
  Major `0` runtime apps private/preview only, Major `>= 1` Team-visible unless
  restricted, and Team release `target_version >= 1.0.0`. Missing source work:
  independent major app-line enforcement for `2.0.0+` as separate module
  ids/icons instead of in-place overwrite. Verification:
  `node src/apps/business-os/shared/app-lifecycle.test.mjs`,
  `node src/apps/business-os/modules/app-store/app-store.test.mjs`,
  `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract -- --nocapture`,
  `cargo test --bin ctox module_catalog_projects_runtime_app_lifecycle_backfill -- --nocapture`,
  and
  `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state -- --nocapture`.
- `2026-06-21`: native versioning policy source fix added
  `public_runtime_app_line_major` and rejects an in-place release that would
  move an existing public runtime app from one Major `1+` line to another on the
  same module id. This enforces the product rule that `2.0.0+` app lines are
  separate apps/icons while keeping `0.x` to `1.0.0` and same-major releases
  valid. Verification:
  `cargo test --bin ctox module_release_rejects_in_place_public_major_line_bump -- --nocapture`,
  `cargo test --bin ctox business_app_semver_major_matches_browser_plain_semver_contract -- --nocapture`,
  `cargo test --bin ctox module_catalog_projects_release_state_data_access_and_rollback_target -- --nocapture`,
  `cargo test --bin ctox module_release_command_replay_does_not_duplicate_release_state -- --nocapture`,
  `node src/apps/business-os/shared/app-lifecycle.test.mjs`,
  `node src/apps/business-os/modules/app-store/app-store.test.mjs`, and
  `git diff --check -- src/core/business_os/store.rs docs/business-os-app-creation-plan.md`.
  Commit `dc544612` (`Enforce Business OS app major release lines`) was pushed
  to `main`. Install/runtime proof is recorded in the next entry.
- `2026-06-21`: commit `fd97322c`
  (`Update app creation versioning install status`) was pushed to `main`; then
  `ctox upgrade --dev` installed latest `main` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T191207Z`.
  Installed verification: `ctox version --json` reports
  `current_release=branch-main-20260621T191207Z`; `ctox status --json` reports
  `running=true`, `busy=false`, Business OS `ok=true`, native peer
  `replicationUp=true`, `http_bridge_available=false`, `pending_count=0`, and
  `blocked_count=0`; installed
  `/Users/michaelwelsch/.local/lib/ctox/current/src/core/business_os/store.rs`
  contains `public_runtime_app_line_major`,
  `module_release_rejects_in_place_public_major_line_bump`, and the rejection
  message for separate Business OS app lines; installed regression test
  `cargo test --bin ctox module_release_rejects_in_place_public_major_line_bump -- --nocapture`
  passed with 1 passed, 0 failed. Generated build/cache output was cleaned
  after install/test: `/Users/michaelwelsch/.cache/ctox` and
  `/Users/michaelwelsch/.cache/ctox/cargo-target` report `0B`; no generated
  `target`, `node_modules`, `ms-playwright`, or `build` directories remain in
  the checked CTOX cache/release/backup paths except the intentionally empty
  cache directory.
- `2026-06-21`: App Creator source path simplified from a local
  specification/preset flow to direct `ctox.business_os.app.create` task
  creation. The Creator now accepts a plain user app request, treats module id,
  title, description, category, layout, and collection names as optional hints,
  and always targets `runtime-installed-module` with
  `business-os-app-module-development`. Verification:
  `node src/apps/business-os/modules/creator/creator.test.mjs`.
- `2026-06-21`: removed stale App Creator registry/module metadata that still
  described a code-generator/harness workbench. `module.json` and
  `registry.json` now describe app request handoff to CTOX agents.
- `2026-06-21`: validator/tool boundary cleanup: installed runtime apps remain
  strict, while internal source modules such as App Creator are no longer
  falsely validated as runtime-generated user apps. Verification:
  `node src/apps/business-os/scripts/validate-app-module.mjs creator --source --json`,
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`,
  `node --check src/apps/business-os/scripts/validate-app-module.mjs`,
  `node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`,
  and `node --check src/apps/business-os/modules/creator/index.js`.
- `2026-06-21`: commit `903756a3` (`Simplify Business OS app creator flow`)
  was pushed to `main`. It removes the deterministic App Creator local
  specification step, quickstart presets, generator/harness registry language,
  and makes the Creator dispatch direct durable app-create tasks from the user
  request.
- `2026-06-21`: first install of the App Creator simplification through
  `ctox upgrade --dev` produced release `branch-main-20260621T165131Z`, but
  installed validation exposed a release-boundary test issue: `creator.test.mjs`
  imported dev dependency `esbuild`, which is not present in a normal release
  source tree. Classification: `validator_tool_installation_gap`, not app
  creation logic and not a reason for deterministic generation.
- `2026-06-21`: commit `41cd8600` (`Make creator source test release-safe`)
  was pushed to `main`. It removes the `esbuild` dependency from
  `creator.test.mjs` and imports the Creator module directly as browser ESM.
  Local verification:
  `node src/apps/business-os/modules/creator/creator.test.mjs`,
  `node src/apps/business-os/scripts/validate-app-module.mjs creator --source --json`,
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`, and
  `node --check src/apps/business-os/modules/creator/creator.test.mjs`.
- `2026-06-21`: `ctox upgrade --dev` installed
  `branch-main-20260621T170122Z`. Installed App Creator verification passed:
  `ctox business-os app validate creator --source --json` returned `ok=true`
  with `module_static_check`, `node_check`, and
  `module_test:src/apps/business-os/modules/creator/creator.test.mjs` all
  green; direct installed test
  `node /Users/michaelwelsch/.local/lib/ctox/current/src/apps/business-os/modules/creator/creator.test.mjs`
  passed 9/9 tests. Installed source audit found no
  `deriveSpecFromRequest`, `btn-apply-request`, `select-preset-request`,
  `code-generator`, `Native standalone`, or `esbuild` Creator-test markers.
- `2026-06-21`: clean post-dispatch five-app proof `rfix12` through installed
  CTOX, MiniMax M3, `256k`, real app-create queue tasks. Command:
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix12`.
  Evidence dir:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12`.
- `2026-06-21`: `ctox business-os app bench status --run-id rfix12 --validate --json`
  produced `bench_green=true`, `needs_attention=false`, 5 handled, 0 pending,
  0 leased, 0 failed, 0 blocked, 0 cancelled, 5 validation passed, 0 validation
  failed, and 0 apps with missing required files. Status evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/status-1782055293408.json`.
- `2026-06-21`: installed browser smoke was green for all five `rfix12` apps:
  `bench_subscriptions_rfix12`, `bench_inventory_rfix12`,
  `bench_projects_rfix12`, `bench_contracts_rfix12`, and
  `bench_quality_rfix12`. Persisted evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/browser-smoke/*.json`.
  Each smoke reports `ok=true`, empty `failures`, empty `console_errors`,
  empty `page_errors`, and empty `failed_requests`.
- `2026-06-21`: commit `81570f94` (`Add Business OS app E2E proof command`)
  added `ctox business-os app e2e` as a proof-only CLI. It validates generated
  runtime apps by exercising the real shell; it does not generate, rewrite, or
  repair app artifacts.
- `2026-06-21`: commit `37c6cf95` (`Prefer record-scoped app automation in
  E2E`) fixed the proof harness after the first Subscriptions run clicked a
  global batch action before the app's valid record-scoped automation action.
  Classification: `test_harness_gap`.
- `2026-06-21`: commit `25fb0260` (`Limit app E2E form filling to visible
  forms`) fixed the proof harness after the first Contracts run filled toolbar
  filters outside the visible create form. Native DB evidence already showed
  the generated app had saved the marker. Classification: `test_harness_gap`.
- `2026-06-21`: installed `main` through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T161830Z`.
  Installed help shows the expected `validate`, `smoke`, and `e2e` app proof
  commands.
- `2026-06-21`: installed deep E2E was green for all five `rfix12` apps:
  `bench_subscriptions_rfix12`, `bench_inventory_rfix12`,
  `bench_projects_rfix12`, `bench_contracts_rfix12`, and
  `bench_quality_rfix12`. Persisted evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix12/deep-e2e/*.json`.
  The evidence covers UI create/save, reload persistence, native CTOX DB/RxDB
  SQLite visibility, and one record-scoped command-bus automation per app.
- `2026-06-21`: transient Projects issue during `rfix12`: CTOX reported
  `database is locked`, converted it into same-task app validation rework, then
  completed `bench_projects_rfix12` without hand-editing generated artifacts.
  Classification: `runtime_orchestration_gap` watch item, not a
  `skill_resource_gap`.
- `2026-06-21`: latest health check after deep E2E: `ctox status --json`
  reports `running=true`, Business OS `ok=true`, native RxDB peer
  `replicationUp=true`, and `http_bridge_available=false`. CTOX is expected to
  be temporarily `busy=true` while it processes the normal follow-up tasks
  queued by the generated apps during E2E.
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
- `2026-06-21`: commit `5811f9c0` (`Add Business OS app browser smoke gate`)
  was pushed to `main` and installed through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T073607Z`.
  The installed smoke initially failed because the runner imported source-local
  `playwright`; classification: `validator_tool_installation_gap`, not app
  creation behavior.
- `2026-06-21`: commit `710c3676` (`Fix installed Business OS app smoke
  runtime`) was pushed to `main` and installed through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T074730Z`.
- `2026-06-21`: installed smoke proof
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix7/browser-e2e/smoke-quality-installed-runtime-loader.json`
  passed for `bench_quality_rfix7` and loaded Patchright plus Chromium from
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T074730Z/runtime/browser/interactive-reference`.
- `2026-06-21`: `rfix8` submitted five real app-create tasks with
  `creates_app_files=false`, `repairs_app_files=false`, and
  `submits_real_business_commands=true`.
- `2026-06-21`: `rfix8` Subscriptions reached terminal success in status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix8/status-1782029381927.json`
  and installed browser smoke
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix8/smoke-subscriptions-rfix8.json`
  passed by clicking `add-subscription` and revealing a form/save flow.
- `2026-06-21`: process/status evidence showed CTOX remained in
  `worker_phase=queue: finalizing` after Subscriptions was handled, with four
  app tasks still pending. Sample path:
  `/tmp/ctox-finalizing-rfix8.sample.txt`. Classification:
  `runtime_orchestration_gap`; source patch skips full Business OS workspace
  desktop indexing for runtime app module tasks.
- `2026-06-21`: targeted source checks passed:
  `cargo test --bin ctox business_os_app_module_tasks_skip_full_workspace_desktop_sync -- --nocapture`
  and
  `cargo test --bin ctox completion_hook_indexes_workspace_outputs_for_business_os -- --nocapture`.
- `2026-06-21`: commit `6766b9d1` (`Skip full workspace sync for app module
  finalization`) was pushed to `main` and installed through `ctox upgrade --dev`
  as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T081830Z`.
- `2026-06-21`: installed `rfix8` routing evidence showed app queue overlap:
  Quality (`queue:system::8dfedc6473b59b0d5f10a301`) was leased at
  `2026-06-21T08:27:25Z`; Inventory
  (`queue:system::bc31b6c02e9c1c33c7fd27bc`) was leased at
  `2026-06-21T08:27:26Z`. Classification: `runtime_orchestration_gap`, not
  model failure and not a reason for a deterministic builder or more app
  prompt rules.
- `2026-06-21`: `ctox business-os app bench status --run-id rfix8 --validate`
  wrote
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix8/status-1782031263632.json`.
  Subscriptions and Quality are handled with installed validation green;
  Inventory is leased; Projects and Contracts are pending.
- `2026-06-21`: source patch in `src/core/service/service.rs` serializes
  Business OS app queue leasing by blocking a second app-create lease while an
  app task is leased, app recovery is active, or a durable queue lease attempt
  is in progress. Verification passed:
  `cargo test --bin ctox app_queue_finalization_does_not_overlap_next_app_lease -- --nocapture`,
  `cargo test --bin ctox app_rework_waits_for_idle_despite_stale_inflight_key -- --nocapture`,
  `cargo test --bin ctox durable_queue -- --nocapture`,
  `cargo test --bin ctox business_os_app_module_tasks_skip_full_workspace_desktop_sync -- --nocapture`,
  and `rustfmt --check src/core/service/service.rs`.
- `2026-06-21`: commit `85ee58d2` (`Serialize Business OS app queue leasing`)
  was pushed to `main` and installed through `ctox upgrade --dev` as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T084616Z`.
  `ctox version --json` confirms `current_release=branch-main-20260621T084616Z`.
  `ctox status --json` confirms CTOX running, Business OS `ok=true`, native
  peer `replicationUp=true`, and `http_bridge_available=false`.
- `2026-06-21`: post-install `rfix8` status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix8/status-1782032199956.json`
  shows exactly one app task leased (`bench_projects_rfix8`), Inventory
  requeued to pending as unstarted, Contracts pending, and Subscriptions plus
  Quality handled. This is early evidence that the serialization fix is active,
  but `rfix8` is still forensic-only because it contains pre-fix overlap.
- `2026-06-21`: forensic `rfix8` drained without manual app artifact edits.
  Final status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix8/status-1782034301490.json`
  shows `bench_green=true`, `handled=5`, `leased=0`, `validation_passed=5`,
  no missing app artifact directories, and no failed queue tasks. This confirms
  the queue serialization fix allowed the remaining apps to drain, but it is not
  a clean production signoff because the run started before that fix.
- `2026-06-21`: `rfix9` submitted five real app-create tasks with
  `creates_app_files=false`, `repairs_app_files=false`,
  `submits_real_business_commands=true`, and runtime install targets.
- `2026-06-21`: `rfix9` was declared contaminated as production evidence.
  Status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix9/status-1782039498716.json`
  shows four handled tasks and one pending task, but the Projects task had been
  prematurely finalized by the installed `ctox business-os app validate`
  command while the worker was still active. Classification:
  `runtime_orchestration_gap`.
- `2026-06-21`: local source patch in `src/core/service/business_os.rs` makes
  `ctox business-os app validate` read-only by removing leased queue task
  completion from validation. Explicit app completion remains under
  `ctox business-os app finalize <module-id> --task-id <queue-task-id>`.
  Verification passed:
  `cargo test --bin ctox app_validate_success_does_not_finalize_matching_leased_creator_task -- --nocapture`,
  `cargo test --bin ctox app_bench_run_submits_real_tasks_without_writing_app_artifacts -- --nocapture`,
  and `rustfmt --check src/core/service/business_os.rs`.
- `2026-06-21`: commit `0d0980e8` (`Make Business OS app validation
  read-only`) was pushed to `main` and installed through `ctox upgrade --dev`
  as
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T110239Z`.
- `2026-06-21`: clean run `rfix10` submitted five real app-create tasks with
  `creates_app_files=false`, `repairs_app_files=false`,
  `submits_real_business_commands=true`, and runtime install targets. Task ids:
  Subscriptions `queue:system::6e72dcba71e74168520908c0`, Inventory
  `queue:system::2dd5af10f49497245156a5d5`, Projects
  `queue:system::2468c793b037f744ba8d8609`, Contracts
  `queue:system::f3b854a977627e69d97dbe87`, and Quality
  `queue:system::7d66db45e787489fec35f36e`.
- `2026-06-21`: `rfix10` terminal static status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/status-1782045591482.json`
  shows `bench_green=true`, `handled=5`, `leased=0`,
  `validation_passed=5`, `validation_failed=0`, `artifact_dirs_present=5`,
  and no missing required files.
- `2026-06-21`: `rfix10` browser smoke passed for Projects and failed for
  Inventory, Subscriptions, Quality, and Contracts with
  `no visible primary create action found under module root`. Evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/browser-smoke-bench_projects_rfix10.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/browser-smoke-bench_inventory_rfix10.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/browser-smoke-bench_subscriptions_rfix10.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/browser-smoke-bench_quality_rfix10.json`,
  and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix10/browser-smoke-bench_contracts_rfix10.json`.
- `2026-06-21`: source inspection of `rfix10` generated apps found the repeated
  runtime mount defect: four apps wrote a valid static `index.html` with a
  Create/New/Add affordance, but `mount(ctx)` never loaded that fragment into
  `ctx.host`. Projects loaded the fragment and passed smoke. Classification:
  `skill_resource_gap` plus `validator_gap`.
- `2026-06-21`: local source patch updates
  `module_static_check.mjs` and the app-module skill resources so installed
  runtime apps must render their primary UI into `ctx.host`. Verification
  passed:
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`,
  `node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`,
  `node --check src/apps/business-os/scripts/validate-app-module.test.mjs`,
  and source-validator checks against all five `rfix10` modules. The new local
  validator rejects the four browser-smoke-red apps and passes Projects.
- `2026-06-21`: source commit `1a15ed72` (`Require runtime apps to render
  module HTML`) was pushed to `main` and installed through `ctox upgrade --dev`
  as `branch-main-20260621T125644Z`.
- `2026-06-21`: root-cause audit found the active skill-binding defect:
  `render_skill_dispatch_block` emitted `preferred_skill: <name>`, while the
  forked harness only injects skill bodies from structured `UserInput::Skill`
  or explicit `$skill` / linked `SKILL.md` mentions. This means prior app
  creation runs were not guaranteed to load the Business OS app skill at all.
  Local source now resolves unique `src/skills/**/SKILL.md` paths by `name:`
  and renders linked skill mentions.
- `2026-06-21`: local suggested-skill dispatch verification passed:
  `cargo test --bin ctox render_chat_prompt -- --nocapture`,
  `cargo test --manifest-path src/core/harness/core/Cargo.toml collect_explicit_skill_mentions_prefers_resource_path -- --nocapture`,
  `rustfmt --check src/core/context/live_context.rs`, and `git diff --check`.
- `2026-06-21`: commit `791d6da6` (`Wire suggested skills into runtime
  prompts`) was pushed to `main` and installed through `ctox upgrade --dev` as
  `branch-main-20260621T134556Z`. Installed verification:
  `ctox version --json` reports `current_release:
  branch-main-20260621T134556Z`; `ctox status --json` reports
  `running=true`, Business OS `ok=true`, native peer `replicationUp=true`, and
  `http_bridge_available=false`; installed
  `src/core/context/live_context.rs` contains `required_skill`,
  `resolve_suggested_skill_path`, and no `preferred_skill`.

## Open Issues

- No blocking app-creation production issue is open.
- Optional UX follow-up: App Store/Creator can add a clear handoff for "new
  major app line" that submits the normal app-create command. This must stay a
  handoff/tool path, not a deterministic app copier or generated-artifact
  repair path.
- `ctox upgrade --dev` currently succeeds and leaves CTOX healthy, but the
  non-fatal `sudo: a password is required` and launchctl SIGKILL warnings
  should be fixed separately if they create visible install/restart problems.
- Deep E2E intentionally queued normal command-bus follow-up tasks from the
  generated apps. That is positive automation evidence, but the runtime may
  need cleanup if a clean local CTOX queue is required for another proof run.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out
  of this work unless explicitly requested.
