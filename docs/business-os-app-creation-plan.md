# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation reliable through the real Business
OS entry points. CTOX should build and modify small runnable Business OS apps as
runtime-installed vanilla HTML/CSS/browser ESM modules, using the Business OS
app-module skill resources and normal CTOX validation.

This is an editable work plan. Agents working on this must update the tracker,
phase notes, evidence log, and open issues in this file before handing off.

## How To Use This Plan

This file is the running project ledger for Business OS app creation. Every
agent that works on app creation must edit this file during the work, not only
at the end.

Update rules:

- Move exactly one active phase to `in_progress` before starting material work.
- Add owner, date, and concrete evidence for every phase update.
- Append an Evidence Log entry after every meaningful run, failed run, patch,
  validator change, or release-install test.
- Add blockers to Open Issues immediately when work cannot continue.
- Mark a phase `done` only when its exit criteria are met with evidence.
- Keep notes factual: command, commit, path, run id, task id, validator output,
  browser-smoke result, or failure classification.
- Keep the Current Execution Slice below current. It is the first thing a
  continuation agent should read and the last thing it should update.

Do not use this plan as an app-building prompt. CTOX must still build apps
through normal agent execution, the Business OS app-module skill resources, and
the Business OS command/task pipeline.

Editable sections:

- Current Status
- Current Execution Slice
- Tracker
- Phase update checklists
- Evidence Log
- Open Issues
- Handoff Notes

Do not edit stable sections such as Work Policy, Non-Negotiables, Acceptance
Gates, or Failure Classification unless the Evidence Log shows that the current
rule is wrong or incomplete.

## Current Status

Status: `in_progress`

Last updated: `2026-06-20`

Current baseline:

- The former direct-file writer in App Creator has been removed.
- The App Creator now creates `ctox.business_os.app.create` tasks instead of
  writing app files directly.
- The `business-os-app-module-development` skill is a concise English resource
  index, not a task script.
- Skill resources are English and cover module contract, Do/Don't list, green
  checklist, and architecture translation.
- `ctox upgrade --dev` installed release `branch-main-20260620T120455Z`
  from current main after these changes.
- `ctox upgrade --dev` then installed release `branch-main-20260620T124515Z`
  with the lifecycle fix from commit `212aa2d0`.
- `ctox upgrade --dev` then installed release `branch-main-20260620T130820Z`
  with the reference-catalog and skill-resource fix from commit `c1267d0d`.
- `ctox upgrade --dev` then installed release `branch-main-20260620T141728Z`
  with the runtime module-id finalization fix from commit `b7632c6a` and the
  bench CLI help guard.
- `ctox business-os app bench run` now submits the core-five app-create bench
  through the normal Business OS command path and writes JSONL evidence under
  `runtime/business-os/app-creation-bench/`.
- `ctox business-os app bench status --run-id rcli --validate` now writes a
  read-only status snapshot and distinguishes status collection from a green
  bench.
- Browser smoke against a validation-green `rcli` app exposed a real runtime
  lifecycle gap: private 0.x installed apps were projected into
  `business_module_catalog`, but lacked creator/responsible assignment and an
  initial app-version snapshot, so they were not openable by the local Business
  OS user.
- A source patch now makes app-create validation success seed the creating actor
  as founder/responsible and records the initial `app_create` module version
  before catalog projection. The bench runner also defaults to the local
  Business OS session user instead of an artificial `rxdb-command` actor when
  no actor is supplied.
- App creation is not yet production-ready until a fresh CTOX-native bench run
  passes end to end on the installed release path.
- Fresh run `rfix1` proves the bench commands now default to actor `local-dev`,
  but it also exposed a `reference_gap`: the current installed reference
  catalog shows source app manifest fields that the runtime validator rejects
  for generated apps, including `layout.icon_svg`, `store.installable`, and
  unqualified `layout.right`.
- The installed reference catalog now emits explicit runtime rules, marks
  recommended business-workflow references, and warns that source-only manifest
  fields must not be copied into runtime-installed apps.
- App creation is still not production-ready. Source forensics found and fixed
  a queue rework-recognition bug: rework prompts now begin with
  `Business OS app validation failed.`, but the dispatcher only matched the
  older `Business OS app artifact validation failed.` marker. The patch accepts
  both markers, was pushed in commit `0d315c66`, and is installed in release
  `branch-main-20260620T144851Z`. Installed verification shows CTOX leased
  `bench_quality_rfix2` rework instead of idling, and Quality then became
  terminal-green. Historical run `rfix2` reached static-validation green under
  the previous validator: all five apps were `handled`, all five installed
  validations passed, every app had complete runtime-installed artifacts, and
  `ctox status` reported no pending queue work. The service did not lease
  Subscriptions until a clean service restart, which classifies the queue
  progress issue as a worker-idle wakeup/liveness gap. The next proof must be a
  fresh five-app run that reaches terminal queue evidence, installed
  validation, browser smoke, persistence, and automation without a manual
  service restart.
- The Inventory finalization bug was a concrete runtime lifecycle bug: direct
  validation previously succeeded on files but failed finalization because
  app-version snapshot recording slugified `bench_inventory_rfix2` to
  `bench-inventory-rfix2`. Release `branch-main-20260620T141728Z` fixes this
  on the installed path.
- Browser smoke against static-green `bench_subscriptions_rfix2` opened the
  module but logged `QUERY_NOT_SUPPORTED: collection is not V1.5-enabled` for
  module-owned collections. This is a native CTOX DB data-plane gap: runtime
  app schemas were registered in the browser from `schema.js`, but the native
  peer only registered the static Business OS schema contract. The source patch
  now loads runtime-installed `collections.schema.json` schemas into the
  native peer and refreshes a running in-process peer after app validation
  success.
- `ctox upgrade --dev` installed release `branch-main-20260620T160000Z` with
  the dynamic runtime collection patch from commit `a9b4d1a5`. `readlink
  /Users/michaelwelsch/.local/lib/ctox/current` points to that release, and
  `ctox status` reports the Business OS native RxDB peer running with
  `replicationUp=true`.
- Browser smoke after that install opens all five `rfix2` apps without
  `QUERY_NOT_SUPPORTED` or other console data-plane errors. A fresh browser
  profile needs about 60 seconds for the packaged fallback catalog to refresh
  into the synced runtime catalog before private `0.1.0` apps become visible.
- Persistence/automation smoke is not green. Inventory can create records
  through the UI, survives reload, and dispatches a real
  `business_os.chat.task` through `ctx.commandBus.dispatch`. Subscriptions,
  Projects, and Contracts have hidden modal overlays that still intercept
  pointer events because their CSS lacks a `[hidden] { display: none; }`
  equivalent for the custom modal. Quality opens but has no primary create
  flow on an empty app, so it cannot create the first complaint/action/audit
  record through the UI.
- Source validation now covers those Browser E2E findings without repairing
  generated app files: installed apps must expose a primary create action, and
  custom hidden modal overlays with display rules must also have CSS that
  actually hides them. The concise skill resources now state the same two
  expectations.
- Commit `f2727698` is pushed to `main` and installed through
  `ctox upgrade --dev` as release `branch-main-20260620T163623Z`. Installed
  CLI validation now rejects the four known-bad `rfix2` apps for the intended
  reasons while accepting the known-good Inventory app. `ctox status` reports
  the service running, Business OS web and MCP autostarted, no pending/blocked
  queue work, and native RxDB peer `replicationUp=true`.
- Fresh CTOX-native run `rfix3` completed all five queue tasks on the installed
  release path with `minimax-m3` and `256k` context. `ctox upgrade --dev`
  installed release `branch-main-20260620T180649Z` with the second queue
  liveness patch from commit `641bf86f`; after that install, CTOX completed
  the existing Subscriptions validation rework without manual generated-app
  edits. `ctox business-os app bench status --run-id rfix3 --validate --json`
  under the old installed validator produced
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781979433802.json`
  with five handled tasks and five validation passes.
- Browser smoke then proved `rfix3` is not production-green. Subscriptions,
  Inventory, Contracts, and Quality mount with `RangeError: Maximum call stack
  size exceeded` because generated `index.js` files declare a top-level
  `renderDetail` helper and a nested `renderDetail` function that shadows it.
  Projects mounts without console errors, but its primary create modal renders
  form fields without a visible Save/Submit control, so the create workflow
  cannot be completed. Console evidence is in `.playwright-cli/console-2026-06-20T18-18-54-096Z.log`,
  `.playwright-cli/console-2026-06-20T18-19-31-889Z.log`,
  `.playwright-cli/console-2026-06-20T18-19-47-816Z.log`,
  `.playwright-cli/console-2026-06-20T18-20-04-007Z.log`, and
  `.playwright-cli/console-2026-06-20T18-20-20-455Z.log`.
- Source validation now covers the new browser findings without repairing
  generated app files. `module_static_check.mjs` rejects duplicate runtime
  function declarations and forms with submit handlers but no visible
  submit/save control. `node src/apps/business-os/scripts/validate-app-module.test.mjs`
  and `git diff --check` pass. Running the source validator against installed
  `rfix3` artifacts rejects all five apps for those concrete browser-runtime
  reasons.
- Commit `c5939b54` is pushed to `main` and installed through
  `ctox upgrade --dev` as release `branch-main-20260620T183056Z`. The active
  install symlink points to
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T183056Z`,
  and `ctox status --json` reports Business OS web and MCP autostarted, no
  pending or blocked queue work, and native RxDB peer `replicationUp=true`.
  Installed validation of historical run `rfix3` now rejects all five apps:
  duplicate runtime function declarations in Subscriptions, Inventory,
  Contracts, and Quality; missing visible Save/Submit controls in Inventory
  and Projects. Status snapshot:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781980973056.json`.
- App creation is still not production-ready. The next proof must be a fresh
  CTOX-native five-app run after release `branch-main-20260620T183056Z` that
  passes installed validation, browser mount, `ctx.db` persistence, and
  `ctx.commandBus.dispatch` automation without generated-app repairs.
- Fresh post-runtime-validator bench run `rfix4` is reserved for the next
  production-readiness attempt. It must run through installed CTOX on release
  `branch-main-20260620T183056Z` with `minimax-m3` and `256k` context.
- Run `rfix4` has started through installed CTOX. It submitted five real
  `ctox.business_os.app.create` tasks, removed only old `rfix3` bench modules,
  and preserved the runner contract: no app-file creation and no app repair in
  the bench runner. Initial status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781981170610.json`
  shows Contracts leased by `ctox-service`, four tasks pending, no artifacts
  yet, and `ctox status --json` reports `busy=true` with one active worker.
- In `rfix4`, Contracts reached terminal success and installed validation
  passed with 12 required files and 23 passing module tests. Snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781982232527.json`
  proves `handled=1` and `validation_passed=1`. Follow-up snapshots show CTOX
  stayed `busy=false` with `pending_count=4`, `worker_active_count=0`, and no
  leased task. The current blocking failure class is
  `runtime_orchestration_gap`: the queue did not automatically lease the next
  pending app-create task after a terminal-green app-validation worker.
- Source now patches that queue finalization path: durable queue tasks leased
  during worker finalization are dispatched through a direct active-state
  handoff instead of the regular `enqueue_prompt` path that intentionally
  releases durable leases during an active worker loop. Regression coverage is
  green for the new handoff, the existing worker-finalization lease path, the
  active-worker enqueue release guard, app-validation rework priority, stale
  rework inflight keys, and stale idle inflight keys.
- Commit `9294efb2` is pushed to `main` and installed through
  `ctox upgrade --dev` as release `branch-main-20260620T192755Z`. The active
  install symlink points to
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T192755Z`.
  After this install, the existing `rfix4` run continued without generated-app
  repairs: Quality, Inventory, and Subscriptions reached terminal success, so
  four of five apps are handled and installed validation-green. Evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781985099846.json`.
- A prior `rfix4` snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781985781354.json`
  shows `handled=4`, `validation_passed=4`, and only Projects pending. A live
  `ctox status --json` check on the active release reports the CTOX service
  not running while Projects remains pending. The current blocking failure
  class at that point was `runtime_orchestration_gap`, narrowed to
  service/queue liveness after a validation-green run had progressed to 4/5
  apps. Later evidence below shows `rfix4` reached all-five installed static
  validation green after manual launchd recovery; the clean install lifecycle
  proof remains open.
- Source now patches the narrowed liveness gap as a host lifecycle issue:
  macOS installs and upgrades write a launchd user agent for the CTOX daemon,
  service status reports `launchd-user` when that agent owns the process, and
  `ctox upgrade --dev` restarts the daemon when durable queue work is pending
  even if the old detached process is already dead. This patch does not touch
  generated app files, does not add a deterministic app builder, and does not
  change the Business OS app skill. Source verification is green for
  `bash -n install.sh`, `rustfmt --check src/core/service/service.rs
  src/core/install/mod.rs`, `git diff --check`, `cargo check --bin ctox`, and
  targeted launchd/systemd lifecycle tests. The initial patch was committed as
  `51ba8fc2` and pushed to `main`.
- The first post-push `ctox upgrade --dev` installed
  `branch-main-20260620T202511Z`, but used the old installer binary and did
  not start the daemon. The second upgrade attempted
  `branch-main-20260620T203232Z` with the new installer but rolled back after
  `launchctl bootstrap` failed. Manual forensic replay showed the launchd
  plist itself is valid: `launchctl enable
  gui/$(id -u)/com.metric-space.ctox.service` followed by `launchctl
  bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.metric-space.ctox.service.plist`
  starts CTOX under `manager=launchd-user`. The remaining source fix is the
  launchd start ordering: enable before bootstrap, never disable during start.
  That fix is committed and pushed as `52763ea7`; it must still be installed
  through `ctox upgrade --dev` and proved without manual launchctl commands.
- The manual launchd start resumed `rfix4`: `ctox status --json` reports
  `running=true`, `manager=launchd-user`, native RxDB peer
  `replicationUp=true`, and Projects leased by `ctox-service`. This is useful
  forensic evidence, but not final proof for the source patch because it used a
  manual launchctl replay.
- `rfix4` has now reached installed static-validation green for all five bench
  apps. `ctox business-os app bench status --run-id rfix4 --validate --json`
  wrote
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781989262104.json`
  with `bench_green=true`, `handled=5`, `failed=0`, `validation_passed=5`,
  and all five runtime-installed app artifact directories present.
- App creation is still not production-ready. The next gates are browser mount,
  `ctx.db` persistence, and `ctx.commandBus.dispatch` automation on the fresh
  `rfix4` apps, plus a clean `ctox upgrade --dev` proof for the launchd
  lifecycle fixes without manual launchctl recovery.
- A source-binary `ctox upgrade --dev` attempt for
  `branch-main-20260620T204719Z` reached service restart but rolled back after
  `launchctl` returned an empty error. Source now has a diagnostic/lifecycle
  patch in progress: source upgrades prefer the managed install `current`
  symlink when `CTOX_INSTALL_ROOT` is known, `launchctl` failures include args,
  status, stdout, and stderr, and `kickstart` is treated as diagnostic because
  `bootstrap` plus `RunAtLoad` can already start the agent. This patch must be
  verified and was committed/pushed as `03ec39b0`.
- `ctox upgrade --dev` from the source binary with
  `CTOX_INSTALL_ROOT=/Users/michaelwelsch/.local/lib/ctox` installed release
  `branch-main-20260620T210628Z`. `readlink
  /Users/michaelwelsch/.local/lib/ctox/current` points to that release, the
  LaunchAgent is `running`, `ctox status --json` reports
  `manager=launchd-user`, `pending_count=0`, native RxDB peer
  `replicationUp=true`, and `ctox business-os app bench status --run-id rfix4
  --validate --json` remains `bench_green=true` with all five validations
  passing. The install lifecycle gate is now green; browser mount,
  persistence, and automation smoke remain open.
- Browser mount smoke for the fresh `rfix4` apps is green. All five generated
  modules open through the Business OS shell with the expected active module id,
  `ctoxOperational=ok`, and no console/page/request failures. Evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/browser-mount-smoke-20260620T212626Z.json`.
- Persistence and automation smoke for `rfix4` is red and is the active
  production-readiness blocker. Inventory and Quality created UI records,
  persisted to the native CTOX DB, survived reload, and dispatched real
  Business OS commands. Subscriptions, Projects, and Contracts created records
  locally in the UI, but their main module collections did not reach the native
  DB before timeout. Side event collections for those failed apps did sync, and
  the browser run had no console/page/request failures. Evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/persistence-automation-smoke-20260620T213437Z/result.json`.
- Current failure classification: `validator_gap` under investigation, with a
  likely `skill_resource_gap` follow-up if the validated artifacts used
  browser/native schema shapes that are too easy for an agent to drift. Do not
  patch generated app files. Patch only the validator, concise skill resources,
  or CTOX runtime if root-cause evidence shows a systemic gap.
- Root cause for the `rfix4` persistence blocker is confirmed as schema/record
  parity drift, not a generated-app runtime repair target. The failing apps had
  browser `schema.js` collection versions that drifted from
  `collections.schema.json`, and several record normalizers returned numeric
  date values for fields declared as strings. Side event collections synced
  because those record shapes matched their schemas.
- Source validation now imports installed app `schema.js` and record helper
  modules and rejects schema parity drift plus helper outputs that do not match
  declared JSON types. The concise skill resources now state the same rule:
  keep `schema.js`, `collections.schema.json`, and persisted record helpers in
  type parity; date fields are either ISO strings in `*_date` or numeric
  milliseconds in `*_date_ms`.
- Commit `ebfba103` is pushed to `main`. `ctox upgrade --dev` installed release
  `branch-main-20260620T220404Z` through the managed install root. The active
  install symlink points to that release, and `ctox status --json` reports
  `running=true`, `manager=launchd-user`, Business OS web and MCP autostarted,
  and native RxDB peer `replicationUp=true`.
- The installed validator now correctly rejects all five historical `rfix4`
  apps. Status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781993473758.json`
  reports `bench_green=false`, `handled=5`, `validation_failed=5`, and no
  missing artifact directories. `rfix4` is therefore forensic evidence only,
  not a green signoff run.
- Fresh run `rfix5` is the active production-readiness attempt. It was started
  with `ctox business-os app bench run --suite core-five --model minimax-m3
  --context 256k --run-id rfix5`, removed only old `rfix4` bench modules, and
  submitted five real `ctox.business_os.app.create` tasks. Initial status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781993589002.json`
  shows Projects leased by `ctox-service`, four tasks pending, and no app
  artifacts yet.
- Latest `rfix5` status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781994605567.json`
  reports one handled app, one leased app, and three pending apps. Projects is
  terminal-success with installed validation green, 12 required files present,
  and 29 passing module tests. Quality is currently leased by `ctox-service`.
  Subscriptions, Inventory, and Contracts remain pending with no artifacts yet.
  `ctox status --json` reports `busy=true`, `worker_active_count=1`,
  `current_goal_preview` for Quality, `manager=launchd-user`, and native RxDB
  peer `replicationUp=true`. No failure class is assigned from this live
  in-progress state.

## Current Execution Slice

Owner: `Codex`

Started: `2026-06-20`

Active phase: `5. Installed CTOX-native app creation hardening`

Live execution board:

| Work item | Status | Required evidence before closing | Plan fields to update |
| --- | --- | --- | --- |
| Dynamic runtime collection patch | `done` | Targeted Rust tests green and `git diff --check` clean | Immediate checklist, Evidence Log, Open Issues |
| Install patched CTOX release | `done` | `ctox upgrade --dev` installed `branch-main-20260620T160000Z`; `readlink` points to `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T160000Z`; `ctox status` reports native RxDB peer `replicationUp=true` | Current Status, Tracker, Evidence Log |
| Browser smoke for `rfix2` apps | `done` | All five apps open without console data-plane errors after runtime catalog sync; no `QUERY_NOT_SUPPORTED` remains | Immediate checklist, Evidence Log |
| Persistence smoke | `blocked` | Create or edit one record per app, reload, record still visible | Immediate checklist, Evidence Log |
| Automation smoke | `blocked` | One valid `business_os.chat.task` or allowed ticket command per app | Immediate checklist, Evidence Log |
| Validator coverage for Browser E2E findings | `done` | Source and installed validators reject the hidden-modal and missing-create failures, accept the known-good Inventory app, validator test coverage is green, `git diff --check` is clean, commit `f2727698` is pushed, and release `branch-main-20260620T163623Z` is active | Immediate checklist, Evidence Log, Open Issues |
| Worker-idle wakeup fix | `done` | Regression test plus fresh bench reaches all five handled without service restart | Tracker, Evidence Log, Open Issues |
| Browser-runtime validator coverage | `done` | Commit `c5939b54` is pushed; `ctox upgrade --dev` installed `branch-main-20260620T183056Z`; installed `ctox business-os app bench status --run-id rfix3 --validate --json` rejects all five old `rfix3` apps for the browser-runtime failures | Current Status, Tracker, Evidence Log |
| Fresh CTOX-native five-app bench | `done` | Run id `rfix4` on installed CTOX reached `bench_green=true`, `handled=5`, `validation_passed=5`, and all required runtime app files present in `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781989262104.json` | Current Status, Tracker, Phase 5 checklist, Evidence Log |
| macOS service lifecycle patch | `done` | `ctox upgrade --dev` installed `branch-main-20260620T210628Z` through the managed install root; `readlink` points to that release; LaunchAgent is running; `ctox status --json` reports `manager=launchd-user`, `running=true`, native RxDB peer `replicationUp=true`, and no pending/blocked queue tasks | Current Status, Tracker, Phase 5 checklist, Evidence Log, Open Issues |
| Browser mount smoke for `rfix4` apps | `done` | `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/browser-mount-smoke-20260620T212626Z.json`: all five freshly generated `rfix4` apps open in Business OS, active module matches, `ctoxOperational=ok`, no console/page/request failures | Current Status, Tracker, Phase 5 checklist, Evidence Log |
| `ctx.db` persistence smoke for `rfix4` apps | `blocked` | `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/persistence-automation-smoke-20260620T213437Z/result.json`: Inventory and Quality persisted to native DB and survived reload; Subscriptions, Projects, and Contracts timed out waiting for native main collection rows although UI-local records and side event rows existed | Current Status, Tracker, Phase 5 checklist, Evidence Log, Open Issues |
| `ctx.commandBus.dispatch` automation smoke for `rfix4` apps | `blocked` | Same smoke result: Inventory dispatched `business_os.chat.task`; Quality dispatched `ctox.ticket.local.create`; Subscriptions, Projects, and Contracts did not reach automation smoke because native main-record persistence timed out first | Current Status, Tracker, Phase 5 checklist, Evidence Log, Open Issues |
| Root-cause and patch persistence blocker | `done` | Commit `ebfba103` is pushed and installed as `branch-main-20260620T220404Z`; installed validation now rejects all five old `rfix4` apps for schema/record parity drift, and `rfix4` status is red at `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781993473758.json` | Current Status, Tracker, Phase 5 checklist, Evidence Log, Open Issues |
| Fresh post-schema-validator five-app bench | `in_progress` | Run id `rfix5` submitted five real app-create tasks through installed CTOX with `minimax-m3` and `256k`; latest status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781994605567.json` shows Projects handled and installed-validation green, Quality leased, and Subscriptions/Inventory/Contracts pending | Current Status, Tracker, Phase 5 checklist, Evidence Log |
| Browser mount smoke for `rfix5` apps | `pending` | After all five tasks are terminal and installed validation-green, open each app in Business OS and record active module, console/page/request failures, and shell operational state | Current Status, Phase 5 checklist, Evidence Log |
| `ctx.db` persistence smoke for `rfix5` apps | `pending` | Create or edit one record per app through the UI, reload, and prove the record remains visible and exists in the native dynamic module table | Current Status, Phase 5 checklist, Evidence Log, Open Issues |
| `ctx.commandBus.dispatch` automation smoke for `rfix5` apps | `pending` | Dispatch one valid `business_os.chat.task` or allowed ticket command per app and prove it landed through normal Business OS command flow | Current Status, Phase 5 checklist, Evidence Log, Open Issues |
| Entry-point coverage | `pending` | App Creator, Chat, App Store/template, CLI, and inbound/MCP paths use the same skill resource context | Tracker, Phase 6 checklist, Evidence Log |

Update discipline:

- Change a row to `in_progress` before starting it and to `done` only after the
  listed evidence exists.
- If a row becomes blocked, add the blocker to Open Issues in the same edit.
- Do not mark the overall status production-ready until every Live execution
  board row is `done` or explicitly out of scope with evidence.

Objective: prove that the installed CTOX app-creation path can produce five
fresh runtime-installed Business OS apps that pass validation, browser mount,
`ctx.db` persistence, and automation in one run. Runs `rfix2`, `rfix3`, and
`rfix4` are forensic evidence only. Fresh run `rfix5` is the active proof run
after schema/record parity validation was installed.

Immediate checklist:

- [x] Collect queue status for all five `rcli` task ids.
- [x] Record every produced module path under
      `runtime/business-os/installed-modules/`.
- [x] Run installed validation only for apps with module artifacts.
- [x] Record missing files and validator failures per app.
- [x] Run first browser smoke against a validation-green installed app.
- [x] Classify the browser visibility failure as `runtime_orchestration_gap`.
- [x] Patch app-create validation success to seed lifecycle assignment and an
      initial app version for runtime-installed modules.
- [x] Patch bench default actor to use the local Business OS session user.
- [x] Verify the patched code with narrow Rust tests.
- [x] Run `git diff --check` for changed files.
- [x] Commit and push the systemic lifecycle fix.
- [x] Install lifecycle fix with `ctox upgrade --dev`.
- [x] Start a fresh five-app bench without `--actor` so it uses the local
      Business OS user.
- [x] Confirm fresh bench command actor is `local-dev`.
- [x] Classify first `rfix1` architecture failure as `reference_gap`.
- [x] Patch the reference catalog to avoid presenting source-only manifest
      fields as runtime app templates.
- [x] Patch skill resources with the same source-reference adaptation rule.
- [x] Verify the reference-catalog patch with a regression test.
- [x] Commit and push the reference-catalog/skill-resource fix.
- [x] Install reference fix with `ctox upgrade --dev`.
- [x] Verify installed reference output exposes runtime rules and source-field
      warnings.
- [x] Let any pre-patch queued bench tasks finish or explicitly record why they
      were superseded.
- [x] Start a fresh five-app bench after the reference fix is installed.
- [x] Confirm fresh bench command actors are `local-dev`.
- [x] Capture current `rfix2` status snapshot and record that only Inventory
      has validation-green artifacts so far.
- [x] Collect terminal installed validation status for all five fresh bench
      apps.
- [x] Let `bench_quality_rfix2` reach terminal evidence after installed
      rework-marker fix.
- [x] Classify the remaining idle pending Subscriptions state as a
      worker-idle wakeup/liveness gap after service restart immediately leased
      the task.
- [x] Let currently leased `bench_subscriptions_rfix2` reach terminal evidence.
- [ ] Record whether each fresh `rfix4` app dispatched a real automation
      command. Current `rfix4` evidence: Inventory and Quality dispatched real
      commands; Subscriptions, Projects, and Contracts are blocked by native
      main-record persistence timeout before automation.
- [x] Root-cause the `rfix4` native main-record persistence blocker as
      schema/record parity drift, not an app-output file repair task.
- [x] Patch installed validation so `schema.js`, `collections.schema.json`, and
      record helper outputs must agree on collection versions and JSON value
      types.
- [x] Patch concise skill resources with the same schema/record parity rule.
- [x] Commit and push the parity validator patch as `ebfba103`.
- [x] Install the patch through `ctox upgrade --dev` as release
      `branch-main-20260620T220404Z`.
- [x] Confirm installed validation now rejects all five old `rfix4` apps and
      makes the `rfix4` bench status red.
- [x] Start fresh post-parity-validator run `rfix5`.
- [x] Confirm `bench_projects_rfix5` reached terminal app-validation success
      with installed validation green and 29 passing module tests.
- [x] Confirm CTOX leased `bench_quality_rfix5` after Projects without
      generated-app edits or bench-runner repairs.
- [ ] Let `bench_quality_rfix5` reach terminal evidence.
- [ ] Continue pending `rfix5` tasks: Subscriptions, Inventory, and Contracts.
- [ ] Poll `rfix5` until every task is terminal or the failure class is clear.
- [ ] Run installed validation for every terminal `rfix5` app with complete
      artifacts.
- [ ] Run browser mount smoke for every validation-green `rfix5` app.
- [ ] Run persistence smoke through `ctx.db` for every browser-green `rfix5`
      app.
- [ ] Run automation smoke through `ctx.commandBus.dispatch` for every
      persistence-green `rfix5` app.
- [x] Run browser smoke after the fresh bench has validation-green artifacts.
- [x] Classify browser-smoke failure before editing code or skill resources.
- [x] Patch native peer runtime-installed collection registration from
      `collections.schema.json`.
- [x] Patch app-validation finalization to refresh a running in-process native
      peer after runtime app schema changes.
- [x] Verify dynamic native runtime app collection registration with targeted
      Rust tests.
- [x] Install the dynamic collection patch with `ctox upgrade --dev`.
- [x] Re-run browser smoke against all five `rfix2` apps after install.
- [x] Re-run browser smoke against all five `rfix4` apps.
- [ ] Re-run persistence smoke through `ctx.db` against all five `rfix4` apps.
      Current `rfix4` evidence is 2/5 green and 3/5 blocked.
- [ ] Re-run automation smoke through `ctx.commandBus.dispatch` against all
      five `rfix4` apps.
- [x] Classify every remaining failure before further code or skill-resource
      edits.
- [x] Patch installed-app validation for hidden modals that still intercept
      clicks while hidden.
- [x] Patch installed-app validation for missing primary create affordance.
- [x] Add only concise skill-resource guidance for those two repeated Browser
      E2E failure classes.
- [x] Confirm the source validator still accepts the known-good Inventory app.
- [x] Confirm the source validator now rejects the hidden-modal failures in
      Subscriptions, Projects, and Contracts.
- [x] Confirm the source validator now rejects Quality's missing create flow.
- [x] Run the validator test suite and `git diff --check` for the patch.
- [x] Commit, push, and install the validator/resource patch with
      `ctox upgrade --dev`.
- [x] Start a fresh CTOX-native five-app bench after the installed validator
      patch.
- [x] Confirm `bench_inventory_rfix3` reached terminal app-validation success.
- [x] Capture the first active `rfix3` continuation state: Projects leased,
      Inventory handled, three apps pending.
- [x] Confirm `bench_projects_rfix3` reached terminal app-validation success.
- [x] Confirm `bench_quality_rfix3` reached terminal app-validation success.
- [x] Confirm `bench_contracts_rfix3` reached terminal app-validation success.
- [x] Poll `rfix3` until every task is terminal or the failure class is clear.
- [x] Run installed validation for every terminal `rfix3` app with complete
      artifacts so far: Inventory, Projects, Quality, and Contracts.
- [x] Run installed validation for Subscriptions after it reaches terminal
      app-validation evidence.
- [x] Run browser smoke for every validation-green `rfix3` app.
- [ ] Run persistence and automation smoke for every browser-green `rfix3` app
      after browser-runtime validation is installed and a fresh bench is green.
- [x] Finish the worker-idle/liveness regression test for stale process-local
      leased message keys.
- [x] Commit and push the source liveness patch.
- [x] Install the source liveness patch via `ctox upgrade --dev` and record
      the active release id.
- [x] Confirm the installed release resumes `rfix3` queue work without manual
      service restart.
- [x] Reproduce the remaining pending Subscriptions stall without manual
      restart: service idle, one pending app rework, no active worker.
- [x] Patch source queue leasing to clear stale process-local keys for durable
      queue rows that are already available in `pending` or `review_rework`.
- [x] Prove queue continuation after the liveness patch without manually
      editing generated app files.
- [x] Classify the Inventory finalization failure as runtime lifecycle
      ID-normalization bug, not app-output failure.
- [x] Patch runtime app-version snapshotting to preserve underscore module ids.
- [x] Commit, push, and install the underscore module-id finalization fix via
      `ctox upgrade --dev`.
- [x] Re-run direct validation/finalization for `bench_inventory_rfix2` after
      the installed fix.
- [x] Decide whether remaining idle pending `rfix2` tasks are a worker
      scheduling gap, normal queue cadence, or require superseding/retry.
- [x] Let the currently leased `bench_projects_rfix2` worker reach terminal
      evidence before patching anything else.
- [x] Recheck `bench_quality_rfix2` rework after the active worker finishes.
- [x] Recheck pending Subscriptions and Contracts after the active worker
      finishes.
- [x] Classify current failures before patching code, skill resources, or
      validation.
- [x] Patch source rework detection to recognize the current validation
      feedback header and the legacy artifact-validation marker.
- [x] Verify rework priority with
      `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`.
- [x] Commit and push the rework-detection patch and this plan update.
- [x] Install the patch via `ctox upgrade --dev`.
- [x] Cleanup or cancel only superseded old-run `rcli` validation-rework tasks
      before letting the installed dispatcher resume `rfix2`.
- [x] Verify the installed service leases `bench_quality_rfix2` rework or one
      of the pending `rfix2` tasks instead of idling.
- [x] Collect terminal installed validation status for all five fresh bench
      apps.
- [x] Update Evidence Log and Open Issues before handoff.

Do not patch the app outputs directly. Do not add deterministic builders. Do not
add skill rules for the old `rcli` project-app helper-test failure unless the
same class repeats. The lifecycle patch was allowed because browser smoke proved
a load-bearing Business OS lifecycle/orchestration gap. The current reference
patch is allowed because `rfix1` copied source-only manifest patterns exposed by
the reference catalog. The current rework-detection patch is allowed because
the dispatcher skipped durable validation-rework tasks even though their
Business OS app module ids and validator reports were present.
- Current `rfix5` status is the active production-readiness attempt. `rfix4` is
  now forensic evidence only because the installed parity validator correctly
  marks it red.
- `rfix2` is forensic evidence only. It proved dynamic runtime collection
  registration and exposed Browser E2E validator gaps. Do not patch generated
  app outputs from any run.

## Tracker

| Phase | Status | Owner | Evidence | Notes |
| --- | --- | --- | --- | --- |
| 0. Remove wrong architecture | done | Codex | `e8bec3b8`, `b142e4c8`, installed release `branch-main-20260620T102259Z` | App Creator no longer writes app files itself; resource-index skill installed. |
| 1. Define acceptance gates | pending |  |  | Formalize what must pass for app creation, modification, validation, browser smoke, and automation. |
| 2. Build CTOX-native bench runner | done | Codex | `8a8cd236`; `cargo test --bin ctox app_bench_`; installed release `branch-main-20260620T113510Z`; CLI run `rcli` | Runner submits real `ctox.business_os.app.create` tasks, writes runtime JSONL evidence, and does not write app artifacts. |
| 3. Run five-app bench in CTOX | blocked | Codex | run `rcli`; installed status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781958189008.json`; browser smoke against `http://127.0.0.1:8765/#bench_subscriptions_rcli` | `rcli` produced two validation-green apps, but browser smoke showed validation-green private apps were not openable because creator/responsible lifecycle fields were empty. Superseded by Phase 4 fixes; continue with a fresh post-fix run in Phase 5. |
| 4. Patch systemic gaps | done | Codex | lifecycle commit `212aa2d0`; reference commit `c1267d0d`; installed releases `branch-main-20260620T124515Z` and `branch-main-20260620T130820Z`; run `rfix1`; `cargo test --bin ctox app_bench_`; `cargo test --bin ctox app_validation_success_accepts_postlease_artifact_write`; `cargo test --bin ctox app_references_mark_source_only_manifest_fields_as_non_templates`; `ctox business-os app references --json` | Classification from `rcli`: project helper-test mismatch is `model_failure`; private app visibility is `runtime_orchestration_gap`. Classification from `rfix1`: raw source reference metadata is `reference_gap`. Patched only lifecycle/orchestration and reference-resource gaps. No app-output repair and no deterministic builder. |
| 5. Repeat until green | in_progress | Codex | installed releases `branch-main-20260620T130820Z`, `branch-main-20260620T141728Z`, `branch-main-20260620T144851Z`, and `branch-main-20260620T160000Z`; `ctox status`; `ctox business-os app references --json`; `ctox queue cleanup-scope --match-run-id rfix1 --cancel-open`; `ctox queue cleanup-scope --match-run-id rcli --status review_rework --cancel-open`; `ctox stop`; `ctox start`; `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix2`; latest status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781972216259.json`; browser smoke for all five `rfix2` apps; UI persistence/automation smoke for `bench_inventory_rfix2`; source tests `cargo test --bin ctox app_validation_success_`, `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`, `cargo test --bin ctox app_bench_`, `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`; installed release check `readlink /Users/michaelwelsch/.local/lib/ctox/current` | Fresh `rfix2` uses actor `local-dev`; all five apps are terminal-green and pass installed validation. Browser smoke now opens all five apps without data-plane console errors after release `branch-main-20260620T160000Z`. Production signoff is still blocked by Browser E2E findings: Subscriptions, Projects, and Contracts hide modal overlays without a matching CSS hidden rule, so hidden modals intercept pointer events; Quality lacks a primary create flow for empty-state records. Inventory proves the intended path: UI create, reload persistence through `ctx.db`, and a real `business_os.chat.task` command through `ctx.commandBus.dispatch`. |
| 5a. Fresh post-validator bench | blocked | Codex | release `branch-main-20260620T163623Z`; `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix3`; evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/events.jsonl`; validated status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781979433802.json`; Playwright console logs under `.playwright-cli/console-2026-06-20T18-*` | Fresh bench submitted through installed CTOX with `minimax-m3`, `256k`, and real Business OS app-create commands. All five tasks reached terminal success and passed the old installed validator, but browser smoke rejected the run: four apps hit duplicate `renderDetail` stack overflow and Projects cannot complete primary create because the modal has no visible Save/Submit control. |
| 5b. Queue continuation and worker liveness | done | Codex | commits `71183644`, `641bf86f`; releases `branch-main-20260620T172452Z`, `branch-main-20260620T180649Z`; `rfix3` validated status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781979433802.json`; source tests `cargo test --bin ctox worker_finalization_leases_pending_app_rework_despite_stale_inflight_key`, `cargo test --bin ctox idle_dispatch_ignores_stale_inflight_queue_key_without_live_worker`, `cargo test --bin ctox worker_finalization_can_lease_next_durable_queue_task_before_activity_drop`, `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`, `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`, `cargo test --bin ctox app_bench_`; `rustfmt --check src/core/service/service.rs`; `git diff --check` | The installed liveness patches carried `rfix3` to all five handled without generated app edits. This closes the stale process-local queue-key failure class. |
| 5c. Browser-runtime validator coverage | done | Codex | commit `c5939b54`; release `branch-main-20260620T183056Z`; Playwright console logs under `.playwright-cli/`; `node src/apps/business-os/scripts/validate-app-module.test.mjs`; `git diff --check`; installed status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781980973056.json` | Source and installed validators now catch duplicate runtime function declarations and submit-handler forms without visible Save/Submit controls. Historical `rfix3` is correctly red under the installed validator. |
| 5d. Fresh post-runtime-validator bench | blocked | Codex | `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix4`; evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/events.jsonl`; Contracts-green status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781982232527.json`; installed direct-handoff release `branch-main-20260620T192755Z`; four-green status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781985781354.json`; all-five-green status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781989262104.json`; post-upgrade all-five-green status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781990148609.json`; latest validation status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781991086468.json`; browser mount smoke `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/browser-mount-smoke-20260620T212626Z.json`; persistence/automation smoke `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/persistence-automation-smoke-20260620T213437Z/result.json`; installed lifecycle release `branch-main-20260620T210628Z`; initial lifecycle commit `51ba8fc2`; failed upgrade attempts `branch-main-20260620T203232Z` and `branch-main-20260620T204719Z`; manual launchctl replay; start-order commit `52763ea7`; source-upgrade lifecycle commit `03ec39b0`; source lifecycle tests `cargo test --bin ctox parse_launchd_pid_reads_main_pid_line`, `cargo test --bin ctox launchd_user_unit_installed_requires_matching_root_when_only_global_plist_exists`, `cargo test --bin ctox resolve_active_root_prefers_managed_current_when_install_root_is_known`; `cargo check --bin ctox`; `bash -n install.sh`; `git diff --check` | Direct-handoff install and manual launchd recovery resumed `rfix4` without app repairs and carried all five apps to terminal installed-validation success. The clean dev-upgrade lifecycle proof and browser mount proof are green on release `branch-main-20260620T210628Z`. The run is blocked, not production-green: Inventory and Quality pass UI create, native `ctx.db` persistence, reload, and command dispatch; Subscriptions, Projects, and Contracts create UI-local records but their main module collections do not reach the native DB before timeout while side event collections sync. |
| 5e. Schema parity validator and fresh post-patch bench | in_progress | Codex | commit `ebfba103`; release `branch-main-20260620T220404Z`; source tests `node src/apps/business-os/scripts/validate-app-module.test.mjs`; `git diff --check`; source validator rechecked old `rfix4` app artifacts; installed status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781993473758.json`; fresh run `rfix5`; evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/events.jsonl`; initial status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781993589002.json`; Projects terminal/Quality leased snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781994605567.json` | Source and installed validators now reject browser/native schema parity drift and record helpers that return values incompatible with declared schema types. The historical `rfix4` run is correctly red under the installed validator. Fresh `rfix5` is active: Projects is terminal-green, Quality is leased, and three apps remain pending. Production signoff still requires all five terminal validation, browser mount, `ctx.db` persistence, and automation evidence. |
| 6. Entry point coverage | pending |  |  | Verify App Creator, Chat, App Store/template flow, CLI, and external inbound paths bind the same skill/resource context. |
| 7. Production signoff | pending |  |  | All entry points produce runnable validated apps with evidence. |

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Work Policy

The goal is a simple and robust app-creation path, not a deterministic app
generator.

Allowed work:

- Improve the Business OS app-module skill resources when repeated evidence
  shows that agents miss a CTOX-specific architecture concept.
- Improve validation when bad app artifacts are accepted.
- Improve CTOX task orchestration when app creation, review, validation,
  rework, or evidence collection is not durable.
- Add CLI commands that submit, inspect, validate, or benchmark real CTOX app
  tasks.
- Add tests and smoke checks that prove apps created through CTOX run on the
  release install path.

Forbidden work:

- Do not add a deterministic app generator that writes the app files itself.
- Do not add hidden templates that pretend to be agent-created apps.
- Do not repair bench apps inside the bench runner.
- Do not make source-checkout assumptions for runtime-created apps.
- Do not add long prompt templates to the skill or App Creator.
- Do not hide failures with validator bypasses, legacy exceptions, or fallback
  data paths.
- Do not expand the rule set for one-off model oddities unless the same failure
  repeats across runs or exposes a real architecture gap.

## Non-Negotiables

Do:

- Create apps under the installed CTOX runtime app directory:
  `runtime/business-os/installed-modules/<module-id>/`.
- Use source module paths only for checked-in store/template modules.
- Build apps as plain HTML fragments, CSS, and browser ESM.
- Use local browser ESM only. No package manager and no build step.
- Persist app records through the shell-provided `ctx.db` handle.
- Trigger intelligent workflows through `ctx.commandBus.dispatch(...)`.
- Use `business_os.chat.task` for normal CTOX follow-up automation with
  `payload.record_snapshot`.
- Use `ctox.ticket.local.*` only for real ticket lifecycle actions.
- Require `business-os-app-module-development` for create and modify tasks.
- Inspect three relevant shipped apps before implementation.
- Run `ctox business-os app validate <module-id> --installed`.

Do not:

- Do not reintroduce direct app-file builders or file templates that pretend to
  build the app.
- Do not add task scripts disguised as skill text.
- Do not write classic task text into the skill.
- Do not put user-created runtime apps under `src/`.
- Do not add React, Next.js, Vite, bundled dependencies, `node_modules`, or
  package-manager workflow.
- Do not create HTTP, REST, IndexedDB, SQLite, Postgres, `localStorage`, or
  `sessionStorage` persistence paths for app records.
- Do not write directly to `business_commands` or `ctox_ticket_*` projection
  collections from app code.
- Do not add dead controls, decorative third panes, fake AI buttons, or broad
  features that do not work.
- Do not mark an app task complete when validation is red.
- Do not hide failures by weakening validation.

## Target Entry Points

All of these must bind the same app-module skill resources and create the same
kind of Business OS app task:

- Business OS App Creator
- Business OS Chat
- Business OS App Store app creation or template flow
- CTOX CLI inbound app creation command
- External Business OS MCP or inbound communication path that asks CTOX to build
  or modify an app

Each entry point must pass module id, requested app description, install target,
mode, desired version, and required skill metadata as structured task context.

The app task should carry structured context, not a classic long prompt. It may
include the user request, target module id, install target, version intent,
required skill id, and validation expectations. The skill and references carry
the implementation rules.

## Acceptance Gates

An app creation task is green only when all gates pass:

1. Files exist only under the expected app target directory.
2. `module.json` uses installed-module semantics:
   `entry: installed-modules/<module-id>/index.html`,
   `install_scope: installed`, and semantic version such as `0.1.0`.
3. Required files exist:
   `module.json`, `collections.schema.json`, `schema.js`, `index.html`,
   `index.css`, `index.js`, `icon.svg`, local ESM helpers as needed,
   `locales/en.json`, `locales/de.json`, and tests.
4. `index.html` is an HTML fragment, not a full HTML document.
5. `index.js` exports `mount(ctx)` and renders into `ctx.host`.
6. Records persist via `ctx.db` and declared module collections.
7. At least one automation action dispatches a valid Business OS command.
8. The UI has no dead buttons and no unnecessary third pane.
9. Pure helper tests pass.
10. `ctox business-os app validate <module-id> --installed` passes.
11. Browser smoke opens the app, creates or edits one record, reloads, and sees
    persisted state.
12. Automation smoke dispatches one `business_os.chat.task` or valid local
    ticket command.

## Bench Suite

The bench intentionally uses simple user-level app requests. Do not
overspecify UI layout or implementation details. The agent must select three
reference apps itself.

| Bench App | Minimum Business Scope | Required Automation |
| --- | --- | --- |
| Subscriptions | Abo contracts, MRR, renewal date, churn risk | Create CTOX follow-up for renewal or churn-risk review. |
| Inventory | Items, stock locations, minimum stock, stock movement | Create CTOX follow-up for low-stock review. |
| Projects | Time/material vs fixed-price, milestones, budget vs actual | Create CTOX follow-up for over-budget or overdue milestone. |
| Contracts | Customer contracts, SLA, renewal, termination window | Create CTOX follow-up for renewal or cancellation deadline. |
| Quality | Complaints, corrective actions, audits, owner, due date | Create CTOX follow-up or local ticket for compliance action. |

## Bench Runner Requirements

Create a CTOX-native runner that:

- Removes old bench apps before each run.
- Creates five real `ctox.business_os.app.create` tasks.
- Uses MiniMax M3 through CTOX with 256k context.
- Waits for task completion or validation rework.
- Runs static validation for each produced app.
- Runs browser smoke for each produced app.
- Captures worker events, file lists, validation reports, browser console
  errors, and command dispatch evidence.
- Writes a compact JSONL evidence log under `runtime/` or another ignored
  runtime evidence directory.
- Never builds app files itself.
- Never repairs generated apps directly inside the runner.

Suggested command shape:

```text
ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k
```

The command shape is a suggestion for operator ergonomics, not a builder
contract.

The runner is only an evidence and orchestration tool. It must prove what CTOX
and the selected coding model do; it must not improve app output by writing or
rewriting app artifacts.

## Failure Classification

Each failed run must be classified before changing code:

- `skill_resource_gap`: the resource files omit a necessary Business OS concept.
- `validator_gap`: invalid app artifacts were accepted.
- `runtime_orchestration_gap`: CTOX queue, validation rework, completion, or
  provider handling broke despite valid/invalid app evidence being clear.
- `entrypoint_gap`: a route did not bind skill resources or did not create a
  structured app task.
- `model_failure`: one-off failure despite resources, validator, and runtime
  behaving correctly.
- `reference_gap`: reference app selection exposed bad examples or internal
  tools as app templates.

Patch only systemic failures. Do not add narrow rules for a single odd run.

## Phase Details

### Phase 1: Acceptance Gates

Status: `pending`

Tasks:

- Audit current validator coverage against the gates above.
- Decide how internal shell tools such as App Creator are excluded from normal
  app validation or validated with an explicit internal-tool mode.
- Add or update tests for installed-module manifest semantics, no source-path
  runtime output, no package manager, no local persistence, no dead controls
  where statically detectable, and command-bus automation shape.

Exit criteria:

- The validator rejects known bad artifacts and accepts known good installed
  runtime apps.
- Internal shell tools do not pollute normal generated-app validation evidence.

Phase update checklist:

- [ ] Validator audit recorded in Evidence Log.
- [ ] Known-bad fixtures or tests added where gaps exist.
- [ ] Known-good installed runtime app still validates.
- [ ] No validator bypass added for generated apps.

### Phase 2: CTOX-Native Bench Runner

Status: `done`

Tasks:

- Add the bench runner command.
- Make it submit real Business OS app-create tasks.
- Make it collect evidence without modifying app output.
- Make cleanup remove only bench apps by prefix/tag.

Exit criteria:

- A controlled run can submit tasks and collect evidence.
- A stopped run leaves enough evidence to explain what happened.
- The runner does not create, edit, or repair app files.

Phase update checklist:

- [x] CLI command or equivalent CTOX-native runner added.
- [x] Runner submits real `ctox.business_os.app.create` tasks.
- [x] Evidence path under ignored `runtime/` documented.
- [x] Cleanup only touches bench-tagged runtime apps.
- [x] Tests prove the runner does not write app artifacts.

### Phase 3: First Five-App CTOX Run

Status: `blocked`

Tasks:

- Run all five bench apps with MiniMax M3 through CTOX.
- Record produced paths, task IDs, validation result, browser smoke result, and
  automation smoke result.
- Stop after systemic failure if continuing would only create duplicate noise.

Exit criteria:

- Every failure has an evidence-backed classification.

Phase update checklist:

- [x] Run id recorded.
- [x] Five queue task ids recorded.
- [x] Produced module paths recorded for all apps that currently have
      artifacts.
- [x] Validation results recorded for all apps that currently have artifacts.
- [x] Initial browser smoke recorded for validation-green app.
- [ ] Automation dispatch evidence recorded per app.
- [x] Current failures classified before any patch.

### Phase 4: Systemic Fixes

Status: `done`

Tasks:

- Patch the smallest architecture-level cause for each majority failure.
- Prefer resource clarification, validator coverage, or runtime orchestration
  fixes over additional narrow rules.
- Keep the skill concise.
- Keep app creation agent-driven.

Exit criteria:

- Each patch maps to a repeated failure class or a clearly load-bearing
  architecture gap.

Phase update checklist:

- [x] Failure class named before patching.
- [x] Patch scope limited to skill resource, validator, entry point, or
      orchestration gap.
- [x] No app-specific bench repair committed.
- [x] Regression test or evidence added.
- [x] Lifecycle patched tree installed through `ctox upgrade --dev`.
- [x] Fresh CTOX-native bench started after lifecycle install.
- [x] Reference-catalog patched tree installed through `ctox upgrade --dev`.
- [x] Installed reference output inspected after upgrade.

### Phase 5: Repeat Bench

Status: `in_progress`

Tasks:

- Delete generated bench apps.
- Rerun the five-app suite.
- Compare failures against previous round.
- Continue until all five apps pass validation, browser smoke, persistence, and
  automation smoke in one CTOX-native run.

Exit criteria:

- One clean five-app run from a fresh bench root.

Phase update checklist:

- [ ] Previous bench apps removed by bench tag/prefix only.
- [x] Fresh five-app run completed.
- [x] Results compared with prior run.
- [x] Remaining failures classified.
- [x] Browser-runtime validator/resource patch installed through
      `ctox upgrade --dev` as release `branch-main-20260620T183056Z`.
- [x] Historical `rfix3` validation rechecked on the installed release and
      correctly rejected all five apps for browser-runtime failures.
- [x] Fresh post-runtime-validator five-app run started.
- [x] First `rfix4` task reached terminal installed-validation success:
      Contracts.
- [x] Commit and push the queue-finalization direct-handoff patch as
      `9294efb2`.
- [x] Install the direct-handoff patch with `ctox upgrade --dev` as release
      `branch-main-20260620T192755Z`.
- [x] Confirm the installed release resumed `rfix4` without generated-app
      edits and carried Quality, Inventory, and Subscriptions to terminal
      validation success.
- [x] Fresh post-runtime-validator run passed installed validation.
- [ ] Fresh post-runtime-validator run passed browser smoke.
- [ ] Fresh post-runtime-validator run passed persistence smoke through
      `ctx.db`.
- [ ] Fresh post-runtime-validator run passed automation smoke through
      `ctx.commandBus.dispatch`.
- [x] Fix or work around the remaining service/queue liveness gap enough for
      `rfix4` to reach five terminal-green apps without generated-app edits.
- [x] Classify the four-green `rfix4` service-death failure as host lifecycle
      `runtime_orchestration_gap`, not app output or skill failure.
- [x] Patch source so macOS installs/upgrades write a launchd user agent for
      CTOX service supervision.
- [x] Patch source so `ctox upgrade --dev` restarts the daemon when durable
      queue work is pending even if the old process is already stopped.
- [x] Verify the source lifecycle patch with targeted Rust tests,
      `cargo check --bin ctox`, `bash -n install.sh`, `rustfmt --check`, and
      `git diff --check`.
- [x] Commit and push the initial lifecycle patch to `main` as `51ba8fc2`.
- [x] Install the initial lifecycle patch through `ctox upgrade --dev`; first
      post-push run installed `branch-main-20260620T202511Z`, and second run
      attempted `branch-main-20260620T203232Z` but rolled back on launchd
      bootstrap failure.
- [x] Classify the launchd bootstrap failure as start-order
      `runtime_orchestration_gap`: the plist is valid, but a disabled
      LaunchAgent must be enabled before bootstrap.
- [x] Patch source so launchd start does bootout, enable, bootstrap,
      kickstart, and never disables during start.
- [x] Commit and push the launchd start-order patch to `main` as `52763ea7`.
- [x] Confirm `rfix4` completes the remaining Projects task without
      generated-app edits.
- [x] Verify the latest source-upgrade lifecycle patch:
      managed install root resolution for source-binary upgrades, detailed
      launchctl errors, and best-effort kickstart.
- [x] Commit and push the latest source-upgrade lifecycle patch as
      `03ec39b0`.
- [x] Install the latest launchd lifecycle patch through
      `ctox upgrade --dev`.
- [x] Confirm `ctox status --json` reports the installed service running under
      `manager=launchd-user` after that upgrade without manual launchctl
      commands.
- [x] Run browser mount smoke against all five `rfix4` apps. Evidence:
      `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/browser-mount-smoke-20260620T212626Z.json`.
- [x] Root-cause the `rfix4` native main-record persistence failure for
      Subscriptions, Projects, and Contracts. Result: `schema.js` collection
      versions drifted from `collections.schema.json`, and record helpers
      returned numeric date values for fields declared as strings.
- [x] Patch only the systemic gap: source and installed validation now enforce
      schema/record parity; concise skill resources now explain the same
      expectation. No generated app output was patched.
- [x] Install the patch through `ctox upgrade --dev` as release
      `branch-main-20260620T220404Z`.
- [x] Start a fresh five-app bench after the patch as run id `rfix5`.
- [ ] Require `rfix5` to pass installed validation for all five generated
      apps.
- [ ] Require `rfix5` to pass browser mount smoke for all five generated apps.
- [ ] Require `rfix5` to pass `ctx.db` persistence smoke for all five
      generated apps.
- [ ] Require `rfix5` to pass `ctx.commandBus.dispatch` automation smoke for
      all five generated apps.

### Phase 6: Entry Point Coverage

Status: `pending`

Tasks:

- Run at least one app creation through each required entry point.
- Verify every path binds the same skill resources and target metadata.
- Verify no route writes app files directly.

Exit criteria:

- App Creator, Chat, App Store, CLI, and external/inbound route are covered by
  tests or evidence.

Phase update checklist:

- [ ] App Creator route covered.
- [ ] Business OS Chat route covered.
- [ ] App Store or template route covered.
- [ ] CTOX CLI route covered.
- [ ] External MCP or inbound communication route covered.
- [ ] All covered routes attach the same skill/resource metadata.

### Phase 7: Production Signoff

Status: `pending`

Tasks:

- Write final evidence summary.
- Document known limitations.
- Ensure release install works via `ctox upgrade --dev`.
- Push main.

Exit criteria:

- CTOX can create and modify small Business OS apps on the release install path
  without source-checkout assumptions.

Phase update checklist:

- [ ] `ctox upgrade --dev` release path tested.
- [ ] Fresh installed runtime path tested.
- [ ] No generated app artifact under `src/`.
- [ ] Five-app bench green in CTOX with MiniMax M3.
- [ ] Modification flow tested for at least one generated app.
- [ ] Main pushed.

## Evidence Entry Template

Copy this template into the Evidence Log for every material run or patch.

```md
### YYYY-MM-DD <short title>

- Phase:
- Owner:
- Run id / task ids:
- Commands:
- Changed files:
- Evidence path:
- Result:
- Failure classification:
- Follow-up:
```

## Evidence Log

Append one entry per meaningful run.

### 2026-06-20 Baseline Cleanup

- Commits: `e8bec3b8`, `b142e4c8`
- Installed release: `branch-main-20260620T102259Z`
- Result: App Creator direct-file writing removed; skill converted to resource
  index; installed current tree scanned clean for old Creator/skill builder
  artifacts.
- Remaining blocker: CTOX-native five-app bench has not yet passed end to end
  after the cleanup.

### 2026-06-20 CTOX-Native Bench Runner

- Phase: 2 and Phase 3 start
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `cargo test --bin ctox app_bench_`;
  `cargo run --bin ctox -- business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rcli --actor rxdb-command`;
  `ctox upgrade --dev`;
  `ctox business-os app bench --help`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/core/main.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `runtime/business-os/app-creation-bench/rcli/events.jsonl`,
  `runtime/business-os/app-creation-bench/rcli/summary.json`
- Result: runner submitted five real `ctox.business_os.app.create` commands
  through `accept_rxdb_business_command`; tests prove it writes no app
  artifacts and rejects retired `128k` context. Dev upgrade installed
  `branch-main-20260620T113510Z`, and the installed CLI exposes the bench
  command.
- Failure classification: none for Phase 2; Phase 3 remains incomplete because
  worker execution, validation, browser smoke, and automation smoke have not
  been observed.
- Follow-up: let CTOX workers process the five queued tasks, then collect
  validation/browser/automation evidence and classify failures before patching.

### 2026-06-20 Editable Plan Ledger

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`
- Commands: `sed -n '1,260p' docs/business-os-app-creation-plan.md`,
  `git diff -- docs/business-os-app-creation-plan.md`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path: this file
- Result: added a Current Execution Slice with concrete update checkboxes and
  explicit editable/stable sections so continuation agents can track progress
  in-place during execution.
- Failure classification: none; planning/evidence hygiene update.
- Follow-up: keep this section current while collecting bench status,
  validation, browser smoke, automation smoke, and failure classifications.

### 2026-06-20 Bench Status Snapshot

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `cargo test --bin ctox app_bench_`;
  `cargo run --bin ctox -- business-os app bench status --run-id rcli --validate`;
  `ctox status`;
  `jq '{bench_green,needs_attention,counts,status_path, apps: [.apps[] | {case,module_id, route_status:.queue.route_status, validation_ran:.validation.ran, validation_ok:.validation.ok, artifacts_exist:.artifacts.exists, tests_present:.artifacts.tests_present, missing:.artifacts.required_missing}]}' runtime/business-os/app-creation-bench/rcli/status-1781956817565.json`
- Changed files:
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `runtime/business-os/app-creation-bench/rcli/status-1781956817565.json`,
  `runtime/business-os/app-creation-bench/rcli/events.jsonl`
- Result:
  `bench_green=false`, `needs_attention=true`; counts are `handled=2`,
  `leased=1`, `pending=2`, `validation_passed=2`, `validation_skipped=3`,
  `artifact_dirs_present=2`, `artifact_dirs_missing=3`,
  `apps_with_missing_required_files=3`. `bench_subscriptions_rcli` and
  `bench_quality_rcli` exist under `runtime/business-os/installed-modules/`
  and pass `ctox business-os app validate <module-id> --installed`.
  `bench_projects_rcli` is leased by `ctox-service` and currently has no module
  directory. `bench_inventory_rcli` and `bench_contracts_rcli` remain pending
  with no module directory. A first `ctox status` check reported the CTOX
  service running but idle; a follow-up check reported `busy=true`,
  `worker_active_count=1`, and `pending_count=3`.
- Failure classification:
  no app artifact or skill-resource failure is proven for the three unfinished
  apps yet. Earlier service idleness is a `runtime_orchestration_gap` candidate,
  but the follow-up status shows active work; continue observing before
  patching orchestration.
- Follow-up:
  let the leased task finish or fail, then collect another status snapshot. Do
  not patch app outputs or skill resources until unfinished tasks are terminal
  or their failure class is clear. After the remaining tasks complete, collect
  validation, browser smoke, and automation smoke evidence.

### 2026-06-20 Installed Bench Status After Dev Upgrade

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `ctox upgrade --dev`;
  `ctox business-os app bench --help`;
  `ctox business-os --help | rg "app bench (run|status)"`;
  `ctox business-os app bench status --run-id rcli --validate`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781957954819.json`
- Result:
  dev upgrade initially failed because the local disk was full during state
  backup. Only build cache under
  `/Users/michaelwelsch/.cache/ctox/cargo-target/ctox-main` was removed; state
  backups and runtime state were not deleted. The retry installed
  `branch-main-20260620T120455Z`, and the installed CLI exposes both
  `ctox business-os app bench run` and `ctox business-os app bench status`.
  The installed status snapshot reports `bench_green=false`,
  `needs_attention=true`, `handled=2`, `leased=1`, `pending=1`,
  `other=1`, `validation_passed=2`, `validation_failed=1`,
  `validation_skipped=2`, `artifact_dirs_present=3`, and
  `artifact_dirs_missing=2`. `bench_subscriptions_rcli` and
  `bench_quality_rcli` pass installed validation. `bench_projects_rcli` has
  complete installed artifacts but fails its own tests. `bench_inventory_rcli`
  is leased by `ctox-service`. `bench_contracts_rcli` is pending.
- Failure classification:
  `bench_projects_rcli` is currently `model_failure`: the agent produced a
  static-valid installed app with command-bus automation shape, but its own
  generated helper tests contradict the generated implementation. Evidence:
  `toMs` intentionally parses `YYYY-MM-DD` to UTC midnight while the test
  expects a timestamped ISO string to preserve `10:00:00Z`; another test
  asserts a unified follow-up list has length `3` and also expects four titles.
  This is not enough evidence for a new skill rule, validator change, or
  deterministic builder. The unfinished inventory and contracts tasks have no
  failure class yet.
- Follow-up:
  let CTOX review/rework the project app or mark it terminal, let inventory and
  contracts finish, then collect another installed `bench status --validate`
  snapshot. Patch only if a repeated architecture-level class appears.

### 2026-06-20 App Lifecycle Visibility Gap

- Phase: 3 classification and Phase 4 systemic fix
- Owner: Codex
- Run id / task ids: `rcli`; validation-green app
  `bench_subscriptions_rcli`
- Commands:
  `ctox business-os app bench status --run-id rcli --validate`;
  browser smoke against `http://127.0.0.1:8765/#bench_subscriptions_rcli`;
  SQLite inspection of installed `business_module_catalog`,
  `business_module_versions`, `business_module_acl`, and `business_users`;
  `cargo test --bin ctox app_bench_`;
  `cargo test --bin ctox app_validation_success_accepts_postlease_artifact_write`
- Changed files:
  `src/core/business_os/store.rs`,
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781958189008.json`
- Result:
  browser smoke could not open the validation-green Subscriptions app. The
  shell stayed on Desktop even with hash `#bench_subscriptions_rcli`. Installed
  catalog inspection showed the bench apps existed, but their lifecycle fields
  had empty `creator_user_id`, empty `responsible_user_ids`, and no founder ACL
  rows. The local installed Business OS user was `local-dev`, while the old
  bench run had used `rxdb-command`. The source patch now defaults fresh bench
  tasks to the local Business OS user and makes app-create validation success
  seed the creating actor as founder/responsible and record an initial
  `app_create` module version before catalog projection.
- Failure classification:
  `runtime_orchestration_gap`. This was not a skill prompt issue, not an app
  output issue, and not evidence for a deterministic builder.
- Follow-up:
  run `git diff --check`, commit and push the patch, install it via
  `ctox upgrade --dev`, then start a fresh five-app bench without `--actor`.
  Browser smoke must be repeated on the fresh validation-green apps.

### 2026-06-20 Fresh Bench Reference Gap

- Phase: 4 systemic fix
- Owner: Codex
- Run id / task ids: `rfix1`;
  `queue:system::9033ab1f0861ebf354c6d054`,
  `queue:system::c67e02fcd32f49606690ea7a`,
  `queue:system::18367dff92b11256a37261ff`,
  `queue:system::05e242d95bb62b270ea40562`,
  `queue:system::1204449be9c9d74ad281e6c7`
- Commands:
  `ctox upgrade --dev`;
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix1`;
  `ctox business-os app bench status --run-id rfix1 --validate`;
  SQLite inspection of `business_commands.client_context_json`;
  `ctox business-os app references --json`;
  `cargo test --bin ctox app_references_mark_source_only_manifest_fields_as_non_templates`;
  `cargo test --bin ctox app_bench_`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix1/status-1781960796182.json`
- Result:
  release `branch-main-20260620T124515Z` installed successfully and `rfix1`
  submitted five real app-create commands. All five command contexts use actor
  `local-dev`, proving the lifecycle default actor path is active. The
  Inventory task produced partial artifacts under
  `runtime/business-os/installed-modules/bench_inventory_rfix1`, but validation
  remained red. The generated `module.json` copied source-only patterns:
  `store.installable: true`, `layout.icon_svg`, inline SVG markup, and
  `layout.right` without `layout.third_pane_justification`. It also had helper
  test failures. The installed reference catalog exposed those source-manifest
  patterns directly in `layout`, and internal developer tools appeared as
  normal reference candidates.
- Failure classification:
  `reference_gap` for the source-manifest copying signal. Helper-test failures
  in the same partial app remain `model_failure` until repeated after the
  reference fix. The still-pending apps are not classified yet.
- Follow-up:
  commit and push the reference-catalog/skill-resource patch, install it
  through `ctox upgrade --dev`, then start a fresh bench run. Do not repair
  `bench_inventory_rfix1` directly.

### 2026-06-20 Reference Fix Installed

- Phase: 4 closeout and Phase 5 start
- Owner: Codex
- Run id / task ids: no fresh post-fix run yet; pre-fix run `rfix1` still has
  queued or active tasks in the service.
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox business-os app references --json`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T130820Z`;
  installed runtime store
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os.sqlite3`
- Result:
  `ctox upgrade --dev` installed `branch-main-20260620T130820Z` from main.
  The installed reference API now returns `runtime_rules`, marks normal
  business-workflow references as
  `recommended_for_generated_business_app: true`, and emits source-manifest
  warnings for fields such as `layout.icon_svg`, `store.installable`, and
  unqualified `layout.right`. `ctox status` reports the service running with
  WebRTC/RxDB replication up, but it is still busy with pre-reference-fix
  bench tasks. Those tasks are useful only as forensic evidence; they are not
  production-readiness proof for the installed reference fix.
- Failure classification:
  no new failure classification from this install step. Phase 5 must start with
  a fresh bench or explicitly record why old queued pre-fix tasks were
  superseded.
- Follow-up:
  wait for or supersede pre-fix `rfix1` tasks, run a fresh five-app bench after
  release `branch-main-20260620T130820Z`, collect validation/browser/
  persistence/automation evidence, and classify failures before editing code or
  skill resources again.

### 2026-06-20 Supersede Pre-Fix Bench Tasks

- Phase: 5
- Owner: Codex
- Run id / task ids:
  superseded run `rfix1`:
  `queue:system::9033ab1f0861ebf354c6d054`,
  `queue:system::c67e02fcd32f49606690ea7a`,
  `queue:system::18367dff92b11256a37261ff`,
  `queue:system::05e242d95bb62b270ea40562`,
  `queue:system::1204449be9c9d74ad281e6c7`;
  accidental run `r1781961729513`:
  `queue:system::64c5db8a79db1abdc743b1fe`
- Commands:
  `ctox queue cleanup-scope --match-run-id rfix1 --status pending --status leased --dry-run --cancel-open`;
  `ctox queue cleanup-scope --match-run-id rfix1 --status pending --status leased --cancel-open`;
  `ctox queue cleanup-scope --match-run-id r1781961729513 --status pending --status leased --dry-run`;
  `ctox queue cleanup-scope --match-run-id r1781961729513 --status pending --status leased --cancel-open`;
  `ctox stop`;
  `ctox start`;
  `ctox status`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix1/status-1781961712898.json`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/r1781961729513/events.jsonl`
- Result:
  `rfix1` still had five open tasks after the reference fix was installed: four
  pending and one leased. Because those tasks were created before the
  reference-catalog fix, they were cancelled as superseded through the official
  queue cleanup command. A CLI usability bug was also exposed: invoking
  `ctox business-os app bench run --help` did not show help and instead started
  an unintended default run `r1781961729513` far enough to submit one
  Subscriptions task. That task was cancelled through the same cleanup path.
  The service stayed busy in the already-running cancelled slice, so it was
  restarted. After restart, `ctox status` reported `busy=false`,
  `pending_count=0`, and Business OS web/MCP autostarted.
- Failure classification:
  `runtime_orchestration_gap` for the stale running cancelled slice;
  `validator_gap` is not implicated. `bench run --help` is a
  `bench_cli_guard_gap`, tracked as an Open Issue because the bench CLI must
  never submit work for help/usage requests.
- Follow-up:
  commit and later install the bench CLI help/usage guard after the active
  `rfix2` run is no longer running. Do not run `ctox upgrade --dev` while the
  fresh bench is active.

### 2026-06-20 Post-Reference Bench Started

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::ed23aa850a4334f4ab4f3303`,
  `queue:system::105d99183374108030b4ea9c`
- Commands:
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix2`;
  SQLite check of `business_commands.client_context_json`;
  `ctox queue list --status pending --status leased --limit 10`;
  `ctox business-os app bench status --run-id rfix2 --validate`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/events.jsonl`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781962112290.json`
- Result:
  fresh post-reference-fix run `rfix2` submitted five real app-create commands
  and accepted all five. All command contexts use actor `local-dev` and
  context `256k`. The latest snapshot shows `bench_green=false`,
  `needs_attention=true`, `pending=4`, `leased=1`, `handled=0`,
  `artifact_dirs_present=0`, and `validation_skipped=5`; Quality is leased by
  `ctox-service` and still running with no app artifacts yet.
- Failure classification:
  none yet for app output. The run is in progress, so missing artifact
  directories are expected until the leased worker writes files or terminates.
- Follow-up:
  keep monitoring `rfix2`. Do not patch skill resources, validation, or
  generated app artifacts until there is terminal evidence or repeated failure
  evidence from the post-reference-fix run.

### 2026-06-20 Bench CLI Help Guard Patched

- Phase: 5
- Owner: Codex
- Run id / task ids: not an app run; follows accidental run `r1781961729513`
- Commands:
  `cargo test --bin ctox app_bench_help_does_not_submit_or_cleanup`;
  `cargo test --bin ctox app_bench_`;
  `git diff --check -- src/core/service/business_os.rs docs/business-os-app-creation-plan.md`
- Changed files:
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path: source test output in this work block
- Result:
  source patch makes `ctox business-os app bench run --help` and
  `ctox business-os app bench status --help` return usage before cleanup,
  evidence writes, or Business OS command submission. Regression test
  `app_bench_help_does_not_submit_or_cleanup` passed, and the full
  `app_bench_` test filter passed with 4 tests.
- Failure classification:
  `bench_cli_guard_gap`, limited to the bench evidence tool. This did not
  change app-generation behavior, skill resources, validators, or generated app
  artifacts.
- Follow-up:
  commit the source patch and plan update. Install it later with
  `ctox upgrade --dev` only when doing so will not interrupt the active
  post-reference bench evidence run.

### 2026-06-20 rfix2 Partial Status Snapshot

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::ed23aa850a4334f4ab4f3303`,
  `queue:system::105d99183374108030b4ea9c`
- Commands:
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox status`;
  `ctox queue list --status pending --status leased --status review_rework --limit 20`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781964567806.json`
- Result:
  `bench_green=false`, `needs_attention=true`; counts are `pending=3`,
  `leased=1`, `other=1`, `validation_passed=1`, `validation_skipped=4`,
  `artifact_dirs_present=1`, and `artifact_dirs_missing=4`.
  `bench_inventory_rfix2` has complete installed artifacts, 12 files, and
  passes installed validation plus 17 helper tests. `bench_quality_rfix2` is in
  `review_rework` with no artifact directory after worker-error validation.
  Subscriptions, Projects, and Contracts are still pending with no artifact
  directories. `ctox status` reports `busy=false`, `worker_active_count=0`,
  and `pending_count=4`, with pending previews for Projects, Contracts,
  Subscriptions, and Inventory.
- Failure classification:
  Inventory is not a failure yet because the app artifacts validate, but the
  task has not finalized. Quality is a `runtime_orchestration_gap` candidate
  because no app output exists and the task reached rework after worker error.
  The idle pending queue state is also a `runtime_orchestration_gap` candidate,
  but do not patch until the next status check confirms it is stuck rather than
  normal cadence.
- Follow-up:
  keep Phase 5 active. Do not edit generated app artifacts. If the queue stays
  idle with pending/leased work, investigate worker scheduling/recovery before
  adding skill rules. Browser and automation smoke should wait until a task is
  terminal and validation-green.

### 2026-06-20 Runtime Module ID Finalization Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`
- Commands:
  `ctox business-os app validate bench_inventory_rfix2 --installed`;
  `cargo test --bin ctox app_validation_success_preserves_runtime_module_id_with_underscores`;
  `cargo test --bin ctox app_validation_success_`;
  `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`;
  `cargo test --bin ctox app_bench_`;
  `git diff --check -- src/core/business_os/store.rs docs/business-os-app-creation-plan.md`
- Changed files:
  `src/core/business_os/store.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  direct installed CLI output from release `branch-main-20260620T130820Z`;
  source regression test output in this work block
- Result:
  direct installed validation of `bench_inventory_rfix2` printed
  `Business OS app artifact validation OK: bench_inventory_rfix2 (installed mode)`
  and then failed finalization with
  `Error: module bench-inventory-rfix2 was not found`. The source patch changes
  app-version snapshot recording to preserve the validated runtime-installed
  module id instead of reusing the source-module slug sanitizer. The regression
  test proves that `bench_inventory_rfix2` completes as `handled`, records a
  `business_module_versions` row under `bench_inventory_rfix2`, records none
  under `bench-inventory-rfix2`, and remains present in the RxDB module
  catalog.
- Failure classification:
  `runtime_orchestration_gap`. This was a CTOX lifecycle/versioning bug for
  runtime-installed app ids with underscores, not a model failure, skill issue,
  validator gap, or generated-app issue.
- Follow-up:
  commit and push the source fix, install it through `ctox upgrade --dev`, then
  rerun direct validation/finalization for `bench_inventory_rfix2`. If
  Inventory becomes handled, continue Phase 5 by checking whether the remaining
  pending/rework tasks dispatch correctly or need a fresh post-fix bench.

### 2026-06-20 Runtime Module ID Fix Installed

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::105d99183374108030b4ea9c`,
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::ed23aa850a4334f4ab4f3303`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox business-os app validate bench_inventory_rfix2 --installed`;
  `ctox business-os app bench run --help`;
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox queue show --message-key queue:system::33aefcd7d41e5428b182d0a1`;
  `ctox status`
- Changed files:
  `src/core/business_os/store.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T141728Z`;
  status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781965773118.json`
- Result:
  release `branch-main-20260620T141728Z` is the active installed release.
  Direct installed validation of `bench_inventory_rfix2` now returns
  `Business OS app artifact validation OK: bench_inventory_rfix2 (installed mode)`
  without the old `bench-inventory-rfix2` finalization error. The Inventory
  queue task is `handled` with status note
  `business-os:terminal-success: app validation passed`. The installed
  `bench run --help` path returns usage and reports
  `submits_real_business_commands: false`, so the help guard is installed.
  At that time, `rfix2` status was still not green: Inventory was handled and
  validation-green; Projects is leased with partial schema-only artifacts and
  validation red while still running; Quality was in `review_rework` with no
  artifact directory; Subscriptions and Contracts were pending. `ctox status`
  reported the service running and busy with one queue worker on Projects.
- Failure classification:
  the Inventory failure is resolved as `runtime_orchestration_gap`. The
  remaining `rfix2` failures are not yet final: Projects is active, two apps
  are pending, and Quality is the only current no-artifact rework case.
- Follow-up:
  keep Phase 5 running. Do not patch generated app files or skill resources
  while Projects is active. After the active worker reaches terminal evidence,
  collect a fresh bench status snapshot, then classify Quality, pending apps,
  browser smoke, persistence smoke, and automation smoke before any further
  source change.

### 2026-06-20 rfix2 Projects Green And Rework Marker Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::105d99183374108030b4ea9c`,
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::ed23aa850a4334f4ab4f3303`
- Commands:
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox status`;
  `ctox queue list --status pending --status leased --status review_rework --limit 20`;
  `cargo test --bin ctox app_validation_success_`;
  `cargo test --bin ctox worker_idle_cleanup_leases_next_durable_queue_after_busy_clears`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `git diff --check -- src/core/service/service.rs`
- Changed files:
  `src/core/service/service.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781966787521.json`
- Result:
  `rfix2` now has two terminal-green apps. Inventory and Projects are
  `handled`, installed validation passes, and helper tests pass
  (`bench_inventory_rfix2`: 17 tests; `bench_projects_rfix2`: 27 tests).
  Quality remains `review_rework` with no artifact directory. Subscriptions
  and Contracts remain pending. `ctox status` reports the service running but
  idle with `pending_count=2`, `worker_active_count=0`, and work-hours not
  blocking. Source forensics found that current validation feedback prompts
  start with `Business OS app validation failed.`, while the dispatcher
  recognized only the older `Business OS app artifact validation failed.`
  marker. The targeted regression test initially leased a fresh pending app
  before rework; after the source patch it leases validation rework before
  fresh pending app tasks.
- Failure classification:
  `runtime_orchestration_gap`. The current failure is not caused by generated
  app code, skill resources, or deterministic builder absence. The installed
  dispatcher fails to see existing durable validation-rework work because the
  feedback header changed.
- Follow-up:
  commit and push the source patch plus this plan update, install through
  `ctox upgrade --dev`, then verify the installed dispatcher no longer idles.
  Before resuming active proof, cancel only superseded old-run `rcli`
  validation-rework tasks so `rfix2` Quality rework is not starved by stale
  forensic work. Do not patch generated app files.

### 2026-06-20 Rework Fix Installed And Quality Resumed

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  stale old-run task cancelled:
  `queue:system::81a6b65f041a523efc1134a6`;
  active rework task:
  `queue:system::105d99183374108030b4ea9c`
- Commands:
  `git commit -m "Fix Business OS app validation rework dispatch"`;
  `git push origin main`;
  `ctox queue cleanup-scope --match-run-id rcli --status review_rework --dry-run --cancel-open`;
  `ctox queue cleanup-scope --match-run-id rcli --status review_rework --cancel-open`;
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox queue list --status pending --status leased --status review_rework --limit 20`;
  `ctox business-os app bench status --run-id rfix2 --validate`
- Changed files:
  `src/core/service/service.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T144851Z`;
  status snapshots
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781967668545.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781967909873.json`
- Result:
  commit `0d315c66` was pushed to `main`, and `ctox upgrade --dev` installed
  release `branch-main-20260620T144851Z`. The old `rcli` validation-rework
  task was cancelled only after dry-run matched exactly one stale old-run task.
  After upgrade, CTOX no longer idled: the service leased
  `bench_quality_rfix2` from validation rework with 256k context. `rfix2`
  status improved to three terminal-green apps: Inventory, Projects, and
  Contracts are `handled` and pass installed validation. Quality is still
  leased and not terminal; it has partial artifacts and currently fails
  validation because `index.js` and tests are missing. Subscriptions is still
  pending with no artifacts.
- Failure classification:
  the prior idle state is confirmed as `runtime_orchestration_gap` and fixed on
  the installed path. Quality's current missing files are not yet classified
  because the worker is still active and may complete them in the same rework
  slice.
- Follow-up:
  let the active Quality worker reach terminal evidence. If Quality becomes
  validation-green, verify the service leases pending Subscriptions. If Quality
  returns to rework or fails with missing files after the worker ends, classify
  whether that is model failure, skill-resource gap, or another orchestration
  issue before patching anything.

### 2026-06-20 Quality Green And Final Pending Wakeup Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  Quality `queue:system::105d99183374108030b4ea9c`;
  Subscriptions `queue:system::f4efcb9ad60fb6ab0c35a495`
- Commands:
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox status`;
  `ctox queue show --message-key queue:system::f4efcb9ad60fb6ab0c35a495`;
  `ctox work-hours status`;
  `tail -n 80 /Users/michaelwelsch/.local/state/ctox/ctox_service.log`;
  `ctox stop`;
  `ctox start`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  status snapshots
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781968173323.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781968279283.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781968596145.json`;
  service log `/Users/michaelwelsch/.local/state/ctox/ctox_service.log`
- Result:
  Quality completed after the installed rework-marker fix: the task is
  `handled`, installed validation passes, and its helper test file
  `tests/records.test.mjs` passes 17 tests. `rfix2` then had four terminal
  green apps and exactly one pending app, Subscriptions. The service stayed
  idle for more than one dispatch interval with `pending_count=1` and
  `worker_active_count=0`; work-hours was not blocking. A clean service restart
  immediately leased the pending Subscriptions task, proving the task was
  valid/leasable and the remaining issue is service wakeup/liveness after
  worker completion, not a bad app artifact or missing skill rule.
- Failure classification:
  `runtime_orchestration_gap`: worker-idle wakeup/liveness after terminal
  Business OS app validation. This should be fixed in the service scheduling
  path before declaring app creation production-ready; the restart is evidence
  collection, not an acceptable production workaround.
- Follow-up:
  let the active Subscriptions worker reach terminal evidence. Then patch the
  worker-idle wakeup path with a targeted regression test so a fresh five-app
  bench can complete without service restart.

### 2026-06-20 Subscriptions Leased With Partial Artifacts

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  Subscriptions `queue:system::f4efcb9ad60fb6ab0c35a495`
- Commands:
  `ctox status`;
  `ctox business-os app bench status --run-id rfix2 --validate`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781968983457.json`
- Result:
  CTOX is busy with `worker_active_count=1` and current goal
  `Build a small Business OS Subscriptions app...`. The latest bench status has
  `handled=4`, `leased=1`, `validation_passed=4`, `validation_failed=1`,
  `artifact_dirs_present=5`, and `apps_with_missing_required_files=1`.
  Subscriptions has ten runtime files under
  `runtime/business-os/installed-modules/bench_subscriptions_rfix2/`, including
  `module.json`, `collections.schema.json`, `schema.js`, `index.html`,
  `index.css`, `icon.svg`, `core/records.mjs`, `core/automation.mjs`, and
  `locales/en.json` and `locales/de.json`. It is still missing `index.js` and
  tests, so installed validation is red while the worker is still active.
- Failure classification:
  none yet for Subscriptions; the task is leased and active, so this is a
  mid-run status, not terminal app-output evidence.
- Follow-up:
  wait for Subscriptions to finish, then collect terminal validation evidence.
  If it becomes terminal-green, continue with browser/persistence/automation
  smoke and the worker-idle wakeup fix. If it enters review rework or terminal
  failure, classify from the final validator and queue evidence before editing
  source, skill resources, or validation.

### 2026-06-20 rfix2 Static Validation Green

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  Subscriptions `queue:system::f4efcb9ad60fb6ab0c35a495`
- Commands:
  `ctox status`;
  `ctox business-os app bench status --run-id rfix2 --validate`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781969124918.json`
- Result:
  `bench_green=true`, `needs_attention=false`, `handled=5`, `leased=0`,
  `pending=0`, `validation_passed=5`, `validation_failed=0`,
  `artifact_dirs_present=5`, and `apps_with_missing_required_files=0`.
  Subscriptions completed as `handled` at `2026-06-20T15:24:48Z`, now has 12
  runtime-installed files including `index.js`,
  `tests/bench_subscriptions_rfix2.test.mjs`, and both locale files, and its
  generated helper tests pass 30 assertions. `ctox status` reports
  `busy=false`, `pending_count=0`, and `worker_active_count=0`.
- Failure classification:
  no terminal app-output failure in `rfix2` static validation. Remaining
  production blockers are outside static validation: browser/persistence smoke,
  automation command evidence, and the worker-idle wakeup/liveness gap observed
  before the restart that leased Subscriptions.
- Follow-up:
  run browser smoke for all five generated apps, prove at least one persisted
  record and one command-bus automation per app, then patch the worker-idle
  wakeup gap and rerun a fresh five-app bench without a service restart.

### 2026-06-20 Browser Smoke Dynamic Collection Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`; browser target
  `bench_subscriptions_rfix2`
- Commands:
  Playwright CLI open of
  `http://127.0.0.1:8765/#bench_subscriptions_rfix2`;
  console inspection of `.playwright-cli/console-2026-06-20T15-35-51-157Z.log`;
  generated app inspection under
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/bench_subscriptions_rfix2/`;
  source inspection of `src/apps/business-os/rxdb/src/query-demand-loader.mjs`,
  `src/core/rxdb/src/plugins/replication_webrtc/query_fetch_handler.rs`, and
  `src/core/business_os/rxdb_peer.rs`
- Changed files:
  `src/core/business_os/rxdb_peer.rs`,
  `src/core/business_os/store.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/ctox-rxdb.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `.playwright-cli/console-2026-06-20T15-35-51-157Z.log`
- Result:
  browser routing opened the module after catalog refresh, but the app logged
  `[bench_subscriptions_rfix2] initial load failed: QUERY_NOT_SUPPORTED:
  collection is not V1.5-enabled`. Source forensics showed the browser
  registers runtime module collections from each app's `schema.js`, while the
  native peer registered only the static Business OS schema contract from
  `business_os_schema_contract.json`. Query fetch therefore rejected
  module-owned runtime app collections even though installed validation was
  green.
- Failure classification:
  `runtime_orchestration_gap` with a concrete CTOX DB data-plane cause. This is
  not a generated-app output fix and not a deterministic builder issue.
- Follow-up:
  verify the source patch with targeted Rust tests, install it with
  `ctox upgrade --dev`, rerun browser smoke for all five `rfix2` apps, then run
  persistence and automation smoke. After this, run a fresh five-app bench
  without manual service restart.

### 2026-06-20 Dynamic Collection Patch Source Verification

- Phase: 5
- Owner: Codex
- Run id / task ids: source verification for `rfix2` browser-smoke failure
- Commands:
  `cargo test --bin ctox runtime_installed_module_schemas_extend_native_collection_creators`;
  `cargo test --bin ctox app_validation_success_accepts_postlease_artifact_write`;
  `cargo test --bin ctox worker_finalization_can_lease_next_durable_queue_task_before_activity_drop`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `cargo test --bin ctox app_validation_success_`;
  `cargo test --bin ctox app_bench_`;
  `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs src/core/service/service.rs`;
  `git diff --check`;
  `cargo check`;
  `cargo test --manifest-path src/core/rxdb/Cargo.toml`;
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`;
  `node src/apps/business-os/rxdb/tests/run-all.mjs`
- Changed files:
  `src/core/business_os/rxdb_peer.rs`,
  `src/core/business_os/store.rs`,
  `src/core/service/service.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/ctox-rxdb.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  local command output in this work block; prior browser console
  `.playwright-cli/console-2026-06-20T15-35-51-157Z.log`
- Result:
  source verification is green. The new regression proves runtime-installed
  `collections.schema.json` extends native collection creators. Existing
  app-validation, rework-dispatch, bench, root `cargo check`, native RxDB
  crate tests, and browser RxDB smokes all pass. The browser RxDB suite
  reports 37 passed, 0 failed, and 2 skipped cross-process tests because the
  local wire daemon is not built.
- Failure classification:
  confirms the source fix for the `runtime_orchestration_gap`; installed
  browser/persistence/automation proof is still pending.
- Follow-up:
  commit and push the verified source patch, install it with
  `ctox upgrade --dev`, then rerun browser, persistence, and automation smoke
  on the installed `rfix2` apps.

### 2026-06-20 Dynamic Collection Patch Installed

- Phase: 5
- Owner: Codex
- Run id / task ids: installed proof for static-green run `rfix2`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `df -h /Users/michaelwelsch/.local/state/ctox`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T160000Z`;
  state backup
  `/Users/michaelwelsch/.local/state/ctox/backups/update-20260620T160004Z`
- Result:
  `ctox upgrade --dev` initially failed during state backup with
  `database or disk is full`. Only build/cache output was removed:
  `/Users/michaelwelsch/.cache/ctox/cargo-target` and
  `/Users/michaelwelsch/Documents/ctox.nosync/runtime/build`; runtime state and
  state backups were not deleted. The retry installed
  `branch-main-20260620T160000Z`, switched
  `/Users/michaelwelsch/.local/lib/ctox/current` to that release, prepared the
  browser/Patchright runtime successfully, pruned old cached release artifacts,
  and restarted the CTOX background service. `ctox status` reports the service
  running, Business OS web on `http://127.0.0.1:8765`, native RxDB peer
  available, and `replicationUp=true`. The filesystem had 37 GiB free after
  install.
- Failure classification:
  no new app-creation failure. The disk-full event is local environment
  pressure during release backup/build, not an app-output or skill-resource
  problem.
- Follow-up:
  rerun browser smoke for all five static-green `rfix2` apps on the installed
  release. If browser smoke is green, run one persistence smoke and one
  command-bus automation smoke per app. Then run a fresh five-app bench to prove
  no-restart queue liveness on the installed path.

### 2026-06-20 Browser And UI Smoke After Dynamic Collection Install

- Phase: 5
- Owner: Codex
- Run id / task ids: static-green run `rfix2`; UI smoke stamp
  `Smoke 1781972759451`
- Commands:
  browser automation against
  `http://127.0.0.1:8765/#bench_subscriptions_rfix2`,
  `#bench_inventory_rfix2`, `#bench_projects_rfix2`,
  `#bench_contracts_rfix2`, and `#bench_quality_rfix2`;
  `ctox business-os app bench status --run-id rfix2 --validate`;
  SQLite inspection of runtime app collections and
  `ctox_business_os__business_commands__v1`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781972216259.json`;
  installed app files under
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/bench_*_rfix2/`
- Result:
  browser smoke opened all five modules after the runtime catalog finished
  syncing in a fresh browser profile. No app logged `QUERY_NOT_SUPPORTED`,
  `collection is not V1.5-enabled`, `initial load failed`, page errors, or
  schema registration failures. Native SQLite tables exist for the runtime app
  collections. Inventory passed the deeper UI smoke: the test created an active
  location and low-stock item through the app UI, reloaded the browser, still
  saw `Smoke 1781972759451 Item`, and clicked the app's follow-up action. The
  command bus inserted
  `cmd_33ca0a56-ae93-45af-956f-b4f7ada4451d`, a real
  `business_os.chat.task` for module `bench_inventory_rfix2` with
  `payload.record_snapshot` and a native capability token.
- Failure classification:
  `validator_gap` plus `skill_resource_gap` for browser-interaction quality.
  Static validation accepted apps that Browser E2E shows are not actually
  usable:
  Subscriptions, Projects, and Contracts have custom modal overlays marked
  `hidden`, but their CSS lacks a rule equivalent to
  `.module-modal[hidden] { display: none; }`; Playwright reports those hidden
  overlays intercept pointer events on primary buttons. Inventory has the
  corresponding `.biv .biv-modal[hidden] { display: none; }` rule and passes
  the same UI path. Quality opens without console errors but exposes no primary
  create action on an empty app, so the user cannot create the first complaint,
  corrective action, or audit record through the UI.
- Follow-up:
  do not repair generated `rfix2` app files. Patch the app validator to reject
  hidden custom modals that can intercept pointer events and to require a
  primary create affordance for generated runtime apps. Keep the skill/resource
  change short: empty-state create flow required; hidden modal overlays must not
  block clicks. Then rerun the bench on fresh generated apps.

### 2026-06-20 Validator Coverage For Browser E2E Findings

- Phase: 5
- Owner: Codex
- Run id / task ids: source validator patch against installed `rfix2` apps
- Commands:
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`;
  `node src/apps/business-os/scripts/validate-app-module.mjs bench_inventory_rfix2 --installed --workspace /Users/michaelwelsch/.local/lib/ctox/current --json`;
  `node src/apps/business-os/scripts/validate-app-module.mjs bench_subscriptions_rfix2 --installed --workspace /Users/michaelwelsch/.local/lib/ctox/current --json`;
  `node src/apps/business-os/scripts/validate-app-module.mjs bench_projects_rfix2 --installed --workspace /Users/michaelwelsch/.local/lib/ctox/current --json`;
  `node src/apps/business-os/scripts/validate-app-module.mjs bench_contracts_rfix2 --installed --workspace /Users/michaelwelsch/.local/lib/ctox/current --json`;
  `node src/apps/business-os/scripts/validate-app-module.mjs bench_quality_rfix2 --installed --workspace /Users/michaelwelsch/.local/lib/ctox/current --json`;
  `git diff --check`
- Changed files:
  `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`;
  `src/apps/business-os/scripts/validate-app-module.test.mjs`;
  `src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md`;
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed `rfix2` apps under
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/installed-modules/bench_*_rfix2/`
- Result:
  source validation now rejects the Browser E2E failures before a user sees the
  app. The validator accepts the known-good Inventory app. It rejects
  Subscriptions, Projects, and Contracts because their hidden modal classes
  have display rules but no matching `.module-modal[hidden] { display: none; }`
  rule. It rejects Quality because the installed app does not expose a primary
  create action for its main business record. The validator test suite includes
  fixture coverage for missing create actions, bad hidden modal CSS, and a good
  hidden modal case. `git diff --check` is clean.
- Failure classification:
  confirmed `validator_gap` plus concise `skill_resource_gap`. No generated
  app files were patched, and no deterministic builder was introduced.
- Follow-up:
  run `git diff --check`, commit and push this source patch, install it with
  `ctox upgrade --dev`, then run a fresh CTOX-native five-app bench. The next
  green proof must come from newly generated apps, not repaired `rfix2`
  artifacts.

### 2026-06-20 Validator Patch Installed

- Phase: 5
- Owner: Codex
- Run id / task ids: install proof for commit `f2727698`
- Commands:
  `git push origin main`;
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox business-os app validate bench_inventory_rfix2 --installed --json`;
  `ctox business-os app validate bench_subscriptions_rfix2 --installed --json`;
  `ctox business-os app validate bench_projects_rfix2 --installed --json`;
  `ctox business-os app validate bench_contracts_rfix2 --installed --json`;
  `ctox business-os app validate bench_quality_rfix2 --installed --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  active release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T163623Z`;
  state backup
  `/Users/michaelwelsch/.local/state/ctox/backups/update-20260620T163628Z`
- Result:
  commit `f2727698` pushed to `main`. `ctox upgrade --dev` installed release
  `branch-main-20260620T163623Z`, prepared the browser/Patchright runtime
  successfully, switched `current` to the new release, and started the CTOX
  background service. The restart path printed two `sudo: a password is
  required` lines, but the upgrade completed with `updated: true`; `ctox
  status` reports the service running, Business OS web on
  `http://127.0.0.1:8765`, Business OS MCP on `http://127.0.0.1:8788/mcp`, no
  pending or blocked queue work, and native RxDB peer `replicationUp=true`.
  Installed validation accepts `bench_inventory_rfix2` and rejects the known
  Browser E2E failures:
  `bench_subscriptions_rfix2`, `bench_projects_rfix2`, and
  `bench_contracts_rfix2` fail on hidden modal CSS; `bench_quality_rfix2` fails
  on missing primary create action.
- Failure classification:
  validator/resource patch is now active in the installed release. The old
  generated `rfix2` artifacts remain intentionally unrepaired and are evidence
  that the installed validator catches the failure class.
- Follow-up:
  start a fresh CTOX-native five-app bench from the installed release and
  evaluate newly generated apps. The bench must prove the validator feedback is
  understandable to the creator agent and that green apps are browser-usable.

### 2026-06-20 Fresh Post-Validator Bench `rfix3`

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`;
  subscriptions `queue:system::c67d20e100cf3a0c8a2d48a1`;
  inventory `queue:system::0017a28fa89b9e3edff7ec82`;
  projects `queue:system::01ff26f7ee64636dc1e00798`;
  contracts `queue:system::c8f72ea8ae19238ed4160d00`;
  quality `queue:system::c2a5da3dfcbff43901ef9a50`
- Commands:
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix3`;
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox status`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/`;
  latest captured status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781975914827.json`
- Result:
  fresh run `rfix3` was submitted through installed CTOX release
  `branch-main-20260620T163623Z`, using `minimax-m3` and `256k` context. The
  runner removed old `rfix2` runtime modules. `bench_inventory_rfix3` reached
  terminal success with complete runtime-installed artifacts. At the latest
  captured status, `bench_projects_rfix3` is leased by `ctox-service` and has
  partial artifacts missing `index.js` and `tests/*.test.mjs`;
  `bench_subscriptions_rfix3`, `bench_contracts_rfix3`, and
  `bench_quality_rfix3` are pending, and `ctox status` reports one active queue
  worker running the Projects app task.
- Failure classification:
  not terminal yet. The important current risk is queue continuation/liveness:
  after one app succeeds, remaining app-create tasks must continue without a
  manual service restart. The focused source regression test for stale
  process-local leased-message keys passes, but that patch is not installed
  proof yet.
- Follow-up:
  let the current Projects worker reach terminal evidence, then keep polling
  `rfix3`. If queue continuation stalls again, prove and install the liveness
  fix through `ctox upgrade --dev`; then continue the fresh bench without
  editing generated app files. Only after all five apps are terminal should the
  work move to installed validation, browser smoke, persistence smoke, and
  automation smoke.

### 2026-06-20 Queue Liveness Regression Test

- Phase: 5
- Owner: Codex
- Run id / task ids: source regression for worker-idle wakeup
- Commands:
  `cargo test --bin ctox idle_dispatch_ignores_stale_inflight_queue_key_without_live_worker`;
  `cargo test --bin ctox worker_finalization_can_lease_next_durable_queue_task_before_activity_drop`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`;
  `cargo test --bin ctox app_bench_`;
  `rustfmt --check src/core/service/service.rs`;
  `git diff --check`
- Changed files:
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  local cargo test output from source checkout
  `/Users/michaelwelsch/Documents/ctox.nosync`
- Result:
  the focused test passed: one pending app queue task with a stale
  process-local `leased_message_keys_inflight` entry and no live worker is
  leased by idle dispatch. This proves the source-side behavior intended by the
  candidate patch. Adjacent service tests also pass: worker finalization can
  lease the next durable queue task before activity drops, app-validation
  rework keeps priority over fresh pending app tasks, green app validation
  still completes the Business OS command even when the worker later errors,
  and the app-bench CLI tests still pass. `rustfmt --check
  src/core/service/service.rs` and `git diff --check` are clean.
- Noted verification limit:
  repository-wide `cargo fmt --check` remains red because unrelated
  pre-existing files under `src/core/business_os/` are not rustfmt-clean. Those
  files are outside this source patch and were not modified.
- Failure classification:
  candidate fix for `runtime_orchestration_gap`. This is source evidence only,
  not installed release proof.
- Follow-up:
  run any remaining targeted service tests needed for confidence, run
  `git diff --check`, commit and push when green, install with
  `ctox upgrade --dev`, then prove queue continuation on `rfix3` or a fresh
  bench without manual service restart.

### 2026-06-20 Queue Liveness Patch Pushed

- Phase: 5
- Owner: Codex
- Run id / task ids: source patch for worker-idle wakeup
- Commands:
  `git add docs/business-os-app-creation-plan.md src/core/service/service.rs`;
  `git commit -m "Fix app queue liveness after stale leases"`;
  `git push origin main`;
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox status`
- Changed files:
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  commit `71183644`;
  latest captured status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781976173025.json`
- Result:
  commit `71183644` is pushed to `main`. The installed release has not been
  upgraded yet because the existing release `branch-main-20260620T163623Z`
  is actively running `bench_projects_rfix3`. At the latest captured status,
  Inventory is handled, Projects is still leased with all required files
  present, and Subscriptions, Contracts, and Quality remain pending.
- Failure classification:
  source patch is pushed but not yet installed. Do not claim release-path
  liveness proof until `ctox upgrade --dev` installs the patch and a resumed or
  fresh bench advances without manual service restart.
- Follow-up:
  wait for the current Projects worker to reach terminal evidence or clearly
  stall. Then install commit `71183644` via `ctox upgrade --dev`, record the
  release id, and continue `rfix3`/fresh-bench proof without editing generated
  app artifacts.

### 2026-06-20 Queue Liveness Patch Installed

- Phase: 5
- Owner: Codex
- Run id / task ids:
  install proof for commit `71183644`;
  active bench `rfix3`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox business-os app validate bench_inventory_rfix3 --installed --json`;
  `ctox business-os app validate bench_projects_rfix3 --installed --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  active release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T172452Z`;
  state backup
  `/Users/michaelwelsch/.local/state/ctox/backups/update-20260620T172456Z`;
  latest captured bench status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781976901759.json`
- Result:
  `ctox upgrade --dev` installed release
  `branch-main-20260620T172452Z` from pushed main. The upgrade completed
  successfully after compiling the CLI, desktop host, Qwen backend, and browser
  runtime; it printed two `sudo: a password is required` lines during restart,
  but reported `"updated": true`. After upgrade, `ctox status` reported the
  service running, Business OS web and MCP autostarted, native RxDB peer
  `replicationUp=true`, and the queue resumed without manual service restart.
  A transient `database is locked` worker error on Subscriptions was converted
  into Business OS app validation rework on the same queue task. After the
  runtime-blocker cooldown expired, CTOX leased `bench_quality_rfix3`.
  Installed validation passes for the two terminal apps so far:
  `bench_inventory_rfix3` and `bench_projects_rfix3`.
- Failure classification:
  positive release-path liveness evidence, but not final proof. Quality,
  Contracts, and Subscriptions still need terminal queue evidence. Subscriptions
  also needs its validation rework to finish the missing `index.js` and tests.
- Follow-up:
  let `bench_quality_rfix3` reach terminal evidence, then continue polling
  until Contracts and Subscriptions are terminal. Do not edit generated app
  files. Once all five apps are terminal, run installed validation, browser
  smoke, persistence smoke, and automation smoke on the newly generated apps.

### 2026-06-20 rfix3 Quality Worker Progress

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`;
  Quality task `queue:system::c2a5da3dfcbff43901ef9a50`
- Commands:
  `ctox status`;
  `ctox business-os app bench status --run-id rfix3 --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781977229471.json`
- Result:
  CTOX remains busy with one active worker on `bench_quality_rfix3`. The app
  directory now contains `collections.schema.json`, `core/automation.mjs`,
  `core/records.mjs`, `icon.svg`, `index.css`, `index.html`, `index.js`,
  `locales/de.json`, `locales/en.json`, `module.json`, and `schema.js`; only
  `tests/*.test.mjs` is still missing. Inventory and Projects remain
  terminal-green. Subscriptions is pending after validation rework feedback and
  still misses `index.js` plus tests. Contracts is pending with no artifacts.
- Failure classification:
  no new failure class yet. The worker is still active and writing installed
  runtime artifacts, so continue observing before patching skill resources,
  validators, or orchestration.
- Follow-up:
  wait for Quality to become terminal or clearly fail. Then continue polling
  Contracts and Subscriptions without manual restart or generated-app edits.

### 2026-06-20 rfix3 Quality Terminal And Contracts Leased

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`;
  Contracts task `queue:system::c8f72ea8ae19238ed4160d00`;
  Subscriptions task `queue:system::c67d20e100cf3a0c8a2d48a1`
- Commands:
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox status`;
  `ctox business-os app validate bench_inventory_rfix3 --installed --json`;
  `ctox business-os app validate bench_projects_rfix3 --installed --json`;
  `ctox business-os app validate bench_quality_rfix3 --installed --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978104946.json`
- Result:
  `bench_quality_rfix3` reached terminal success with complete installed
  artifacts and passes installed validation. Inventory and Projects also pass
  installed validation. CTOX is busy with one active queue worker on
  `bench_contracts_rfix3`, proving another no-manual-restart queue continuation
  step after Quality. Contracts has started writing installed runtime artifacts
  and currently misses `index.js` and tests. `bench_subscriptions_rfix3`
  remains pending validation rework with an incomplete artifact directory
  missing `index.js` and tests.
- Failure classification:
  no new failure class. The current state is positive liveness evidence, but
  production signoff is still blocked until Contracts and Subscriptions are
  terminal and the newly generated apps pass browser, persistence, and
  automation smoke.
- Follow-up:
  keep polling the installed bench. Do not edit generated app files and do not
  add a new orchestration patch unless the installed release stalls again with
  evidence that no live worker owns a pending app task.

### 2026-06-20 rfix3 Subscriptions Pending Rework Stall

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`;
  Subscriptions task `queue:system::c67d20e100cf3a0c8a2d48a1`
- Commands:
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox business-os app bench status --run-id rfix3 --validate --json`;
  `ctox business-os app validate bench_contracts_rfix3 --installed --json`;
  repeated `ctox status`;
  `ctox queue show --message-key queue:system::c67d20e100cf3a0c8a2d48a1`;
  `cargo test --bin ctox worker_finalization_leases_pending_app_rework_despite_stale_inflight_key`;
  `cargo test --bin ctox idle_dispatch_ignores_stale_inflight_queue_key_without_live_worker`;
  `cargo test --bin ctox worker_finalization_can_lease_next_durable_queue_task_before_activity_drop`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`;
  `cargo test --bin ctox app_bench_`;
  `rustfmt --check src/core/service/service.rs`;
  `git diff --check`
- Changed files:
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  validated status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978219067.json`;
  repeated idle snapshots
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978242451.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978268452.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978294495.json`,
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781978320451.json`
- Result:
  Contracts reached terminal success and installed validation passes, so four
  of five `rfix3` apps are static-green. Subscriptions remains pending
  validation rework with missing `index.js` and tests. Multiple status polls
  over more than one minute showed `busy=false`, `worker_active_count=0`,
  `pending_count=1`, no recent events, and no last error. `ctox queue show`
  confirms the Subscriptions task is a `pending` Business OS app validation
  rework prompt. Source forensics found the remaining liveness bug: stale
  process-local `leased_message_keys_inflight` could still block a durable
  queue row that had already been released back to `pending` while another
  worker was active. The source patch now clears stale process-local ownership
  for durable queue rows that are available in `pending` or `review_rework`,
  while still treating an in-memory queued prompt as live ownership. The new
  regression test and adjacent queue/app-bench tests pass; format and diff
  checks are clean.
- Failure classification:
  `runtime_orchestration_gap`. This is not a generated app-output issue and
  does not require skill-resource changes or deterministic app generation.
- Follow-up:
  commit and push the queue-leasing patch, install it through
  `ctox upgrade --dev`, and prove the installed release leases and completes
  the existing Subscriptions rework without manual service restart or generated
  app-file edits.

### 2026-06-20 rfix3 Queue Liveness Patch Installed And Proven

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`;
  Subscriptions task `queue:system::c67d20e100cf3a0c8a2d48a1`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox business-os app bench status --run-id rfix3 --json`;
  `ctox business-os app bench status --run-id rfix3 --validate --json`
- Changed files:
  none in this evidence step; installed previously pushed commit `641bf86f`
- Evidence path:
  active release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T180649Z`;
  validated status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781979433802.json`
- Result:
  `ctox upgrade --dev` installed release `branch-main-20260620T180649Z`.
  After the install, `ctox status` reported `pending_count=0`,
  `worker_active_count=0`, no blocked work, Business OS web/MCP autostarted,
  and native RxDB peer `replicationUp=true`. The existing Subscriptions
  validation rework reached terminal success without editing generated app
  files. Under the installed validator from that release, all five `rfix3`
  apps were `handled` and validation passed.
- Failure classification:
  the prior Subscriptions stall was a `runtime_orchestration_gap`; the source
  and installed evidence now cover that queue-liveness class.
- Follow-up:
  browser smoke, persistence smoke, and automation smoke still decide
  production readiness. Do not treat static validation as sufficient.

### 2026-06-20 rfix3 Browser Runtime Smoke And Validator Gap

- Phase: 5
- Owner: Codex
- Run id / task ids:
  `rfix3`
- Commands:
  Playwright CLI sessions against
  `http://127.0.0.1:8765/#bench_subscriptions_rfix3`,
  `#bench_inventory_rfix3`, `#bench_projects_rfix3`,
  `#bench_contracts_rfix3`, and `#bench_quality_rfix3`;
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`;
  source validator runs against all five installed `rfix3` modules;
  `git diff --check`
- Changed files:
  `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`;
  `src/apps/business-os/scripts/validate-app-module.test.mjs`;
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  Playwright console logs
  `.playwright-cli/console-2026-06-20T18-18-54-096Z.log`,
  `.playwright-cli/console-2026-06-20T18-19-31-889Z.log`,
  `.playwright-cli/console-2026-06-20T18-19-47-816Z.log`,
  `.playwright-cli/console-2026-06-20T18-20-04-007Z.log`,
  `.playwright-cli/console-2026-06-20T18-20-20-455Z.log`
- Result:
  Browser smoke rejected the static-green `rfix3` bench. Subscriptions,
  Inventory, Contracts, and Quality fail at mount with `RangeError: Maximum
  call stack size exceeded` caused by duplicate `renderDetail` function names
  in `index.js`. Projects mounts without console errors but its primary create
  modal has no visible Save/Submit control, so the create workflow cannot be
  completed. The source validator now rejects duplicate runtime function
  declarations and submit-handler forms without visible submit/save controls.
  The validator test suite and `git diff --check` pass, and source validation
  rejects all five installed `rfix3` artifacts for these concrete reasons.
- Failure classification:
  `validator_gap` plus concise `skill_resource_gap`. This is still not a
  reason for a deterministic app builder; the correct fix is a small validator
  feedback loop plus a short Do/Don't rule.
- Follow-up:
  commit and push the validator/resource patch, install it with
  `ctox upgrade --dev`, then run a fresh five-app bench. Production signoff
  requires newly generated apps to pass installed validation, browser mount,
  persistence through `ctx.db`, and automation through `ctx.commandBus`.

### 2026-06-20 Browser Runtime Validator Patch Installed

- Phase: 5
- Owner: Codex
- Run id / task ids:
  historical run `rfix3`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status --json`;
  `ctox business-os app bench status --run-id rfix3 --validate --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  active release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T183056Z`;
  installed validation snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix3/status-1781980973056.json`
- Result:
  `ctox upgrade --dev` installed commit `c5939b54` as
  `branch-main-20260620T183056Z`. A first upgrade attempt failed while
  creating a local state backup because the disk was full; old CTOX
  `update-*` backups were pruned before retrying, and the successful upgrade
  created a fresh backup. The active install symlink points to the new release.
  `ctox status --json` reports CTOX running, no pending or blocked queue work,
  Business OS web on `http://127.0.0.1:8765`, MCP on
  `http://127.0.0.1:8788/mcp`, and native RxDB peer `replicationUp=true`.
  Installed validation of historical `rfix3` now reports `bench_green=false`,
  `needs_attention=true`, `handled=5`, `validation_passed=0`, and
  `validation_failed=5`. The failures match the browser evidence: duplicate
  `renderDetail` declarations in Subscriptions, Inventory, Contracts, and
  Quality; missing visible submit/save controls in Inventory and Projects.
- Failure classification:
  validator patch install proof; no new failure class. Historical `rfix3` is
  now correctly red and remains forensic evidence only.
- Follow-up:
  start a fresh five-app bench after release `branch-main-20260620T183056Z`.
  Production readiness requires the fresh run to pass installed validation,
  browser smoke, persistence smoke, and automation smoke without generated-app
  edits or deterministic app building.

### 2026-06-20 Fresh Post-Runtime-Validator Bench Started

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`;
  Subscriptions `queue:system::e8c02582c37a2e861d6171bc`;
  Inventory `queue:system::8eda6d3dd77a2779c7874b01`;
  Projects `queue:system::f115b955ee616f886464c54f`;
  Contracts `queue:system::1ee89d974aaba0e9c2e0bc4d`;
  Quality `queue:system::85146e296c40b9a5e9498025`
- Commands:
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix4`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox status --json`;
  `tail -n 40 /Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/events.jsonl`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/events.jsonl`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781981170610.json`
- Result:
  the bench runner accepted all five real Business OS app-create commands on
  installed release `branch-main-20260620T183056Z`. It removed only old
  `rfix3` bench modules and reported `creates_app_files=false`,
  `repairs_app_files=false`, `submits_real_business_commands=true`, and
  `install_target=runtime-installed-module`. Initial status shows Contracts
  leased by `ctox-service`, four tasks pending, no artifacts yet, and CTOX
  `busy=true` with one active worker.
- Failure classification:
  none yet. The run is active and too early to classify.
- Follow-up:
  poll until every task is terminal or a failure class is evident. Then run
  installed validation. Browser, persistence, and automation smoke are allowed
  only for newly generated `rfix4` apps that pass installed validation.

### 2026-06-20 rfix4 Contracts Green And Queue Idle Gap

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`; Contracts `queue:system::1ee89d974aaba0e9c2e0bc4d`;
  remaining pending tasks:
  Subscriptions `queue:system::e8c02582c37a2e861d6171bc`,
  Inventory `queue:system::8eda6d3dd77a2779c7874b01`,
  Projects `queue:system::f115b955ee616f886464c54f`,
  Quality `queue:system::85146e296c40b9a5e9498025`
- Commands:
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox status --json`;
  repeated delayed status checks after Contracts completed
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  Contracts green snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781982232527.json`;
  idle pending snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781982411858.json`
- Result:
  Contracts reached terminal success with
  `status_note=business-os:terminal-success: app validation passed`, 12
  required runtime files, and installed validation green. Its module tests ran
  23 tests with 23 passes. After that terminal success, CTOX stayed
  `busy=false` with `worker_active_count=0`, `pending_count=4`, and no leased
  follow-up bench task. A delayed bench status still showed `handled=1`,
  `pending=4`, `leased=0`, `validation_passed=1`, and four apps missing
  artifacts.
- Failure classification:
  `runtime_orchestration_gap`. The first newly generated app passed the
  installed validator; the current blocker is queue continuation after terminal
  validation success, not app output, skill content, or deterministic app
  generation.
- Follow-up:
  inspect and fix queue leasing/liveness so the installed service leases the
  next pending app-create task after a terminal-green app-validation worker
  without a service restart. Add or adjust a regression test before installing
  the fix through `ctox upgrade --dev`, then continue the same `rfix4` run if
  possible.

### 2026-06-20 Queue Finalization Direct Handoff Patch

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`; Contracts green then four pending tasks remained idle
- Commands:
  `cargo test --bin ctox worker_finalization_direct_dispatch_tracks_durable_queue_lease`;
  `cargo test --bin ctox worker_finalization_can_lease_next_durable_queue_task_before_activity_drop`;
  `cargo test --bin ctox enqueue_prompt_releases_durable_queue_lease_instead_of_buffering_during_active_worker`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `cargo test --bin ctox worker_finalization_leases_pending_app_rework_despite_stale_inflight_key`;
  `cargo test --bin ctox idle_dispatch_ignores_stale_inflight_queue_key_without_live_worker`;
  `rustfmt --check src/core/service/service.rs`;
  `git diff --check`
- Changed files:
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  source tree and command output in this run
- Result:
  source forensics found the missing coverage layer: the worker-finalization
  path could lease the next durable queue task while the current worker was
  still finalizing, but then sent that leased prompt through regular
  `enqueue_prompt`. `enqueue_prompt` is intentionally conservative during an
  active worker and releases durable queue leases back to `pending` rather
  than buffering them. The patch adds a direct active-state handoff for durable
  queue prompts already leased by worker finalization, so the next prompt is
  marked active/inflight and started instead of being released. The existing
  enqueue guard remains intact and tested.
- Failure classification:
  `runtime_orchestration_gap`.
- Follow-up:
  commit, push, install with `ctox upgrade --dev`, then prove the installed
  release continues the existing `rfix4` run by leasing one of the four pending
  app-create tasks without manual generated-app edits.

### 2026-06-20 Direct Handoff Patch Installed And rfix4 Advanced To 4/5

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`;
  Quality `queue:system::85146e296c40b9a5e9498025`;
  Inventory `queue:system::8eda6d3dd77a2779c7874b01`;
  Subscriptions `queue:system::e8c02582c37a2e861d6171bc`;
  Projects still pending `queue:system::f115b955ee616f886464c54f`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox status --json`;
  `ctox queue list --status pending --limit 8 --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  active release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T192755Z`;
  Quality terminal-green snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781983525257.json`;
  Inventory handled and Subscriptions leased snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781984458819.json`;
  Subscriptions terminal-green snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781985099846.json`;
  latest four-green snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781985781354.json`
- Result:
  `ctox upgrade --dev` installed commit `9294efb2` as
  `branch-main-20260620T192755Z`. The installed release resumed the existing
  `rfix4` run without direct generated-app edits: Quality, Inventory, and
  Subscriptions reached terminal app-validation success after Contracts. The
  latest bench snapshot reports `handled=4`, `pending=1`,
  `validation_passed=4`, and `apps_with_missing_required_files=1`; the only
  pending task is Projects. A live status check on the active release reports
  the CTOX service not running while the Projects task remains pending.
- Failure classification:
  `runtime_orchestration_gap`. The direct-handoff patch improved queue
  progress from 1/5 to 4/5, but the remaining blocker is still service/queue
  liveness, not app output, skill content, or deterministic app generation.
- Follow-up:
  inspect why the installed service stopped or is reported stopped after
  Subscriptions terminal success, patch only the systemic service/queue
  liveness issue, install through `ctox upgrade --dev`, and continue the same
  `rfix4` run until Projects is terminal. Browser, persistence, and automation
  smoke are still pending and must use only the newly generated `rfix4` apps.

### 2026-06-20 macOS Service Lifecycle Source Patch Ready

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`; Projects still pending
  `queue:system::f115b955ee616f886464c54f`
- Commands:
  `ctox status --json`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox queue list --status pending --limit 16 --json`;
  `bash -n install.sh`;
  `rustfmt --check src/core/service/service.rs src/core/install/mod.rs`;
  `git diff --check`;
  `cargo check --bin ctox`;
  `cargo test --bin ctox parse_launchd_pid_reads_main_pid_line`;
  `cargo test --bin ctox launchd_user_unit_installed_requires_matching_root_when_only_global_plist_exists`;
  `cargo test --bin ctox refresh_launchd_agent_writes_current_root_and_marker`;
  `cargo test --bin ctox systemd_user_unit_installed_requires_matching_root_when_only_global_unit_exists`;
  aborted pre-push `ctox upgrade --dev`
- Changed files:
  `install.sh`;
  `src/core/install/mod.rs`;
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  latest rfix4 status
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781986365023.json`
- Result:
  Source patch adds macOS launchd user-agent support for CTOX service
  supervision, reports launchd-managed services as `manager=launchd-user`,
  disables/stops launchd on explicit `ctox stop`, and refreshes the launchd
  plist during source upgrades and regular macOS installs. The upgrade restart
  decision now treats pending, leased, or review-rework durable queue tasks as
  a restart reason even when the previous process is already stopped. Source
  compile/test evidence is green. The first `ctox upgrade --dev` attempt was
  stopped intentionally because it fetched a `main` source archive before this
  patch had been committed and pushed; continuing that run would have installed
  the old code path.
- Failure classification:
  `runtime_orchestration_gap`. This is a host service lifecycle and upgrade
  restart gap. It is not a generated app-output problem, not a Business OS app
  skill prompt problem, and not a reason to add deterministic app generation.
- Follow-up:
  fix launchd start ordering so start enables before bootstrap, commit and
  push the patch to `main`, rerun `ctox upgrade --dev`, verify the installed
  release is the pushed one, confirm the service is running under
  `launchd-user` without manual launchctl commands, then continue `rfix4` until
  the Projects task is terminal.

### 2026-06-20 Launchd Start-Order Forensics

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`; Projects `queue:system::f115b955ee616f886464c54f`
- Commands:
  `ctox upgrade --dev`;
  `ctox status --json`;
  `plutil -lint ~/Library/LaunchAgents/com.metric-space.ctox.service.plist`;
  `launchctl print-disabled gui/$(id -u)`;
  `launchctl enable gui/$(id -u)/com.metric-space.ctox.service`;
  `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.metric-space.ctox.service.plist`;
  `launchctl print gui/$(id -u)/com.metric-space.ctox.service`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `bash -n install.sh`;
  `rustfmt src/core/service/service.rs`;
  `git diff --check`;
  `cargo test --bin ctox parse_launchd_pid_reads_main_pid_line`;
  `cargo test --bin ctox launchd_user_unit_installed_requires_matching_root_when_only_global_plist_exists`;
  `cargo check --bin ctox`
- Changed files:
  `install.sh`;
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781987940566.json`
- Result:
  Commit `51ba8fc2` installed LaunchAgent generation, but the new start path
  failed because `launchd_bootstrap_and_start` disabled the service before
  bootstrapping it. Manual `launchctl enable` before `bootstrap` started the
  same plist successfully. CTOX then reported `running=true`,
  `manager=launchd-user`, and native peer `replicationUp=true`; `rfix4`
  Projects leased and began writing runtime-installed app artifacts. Source now
  changes the start order to bootout, enable, bootstrap, kickstart in both
  Rust service startup and `install.sh`.
- Failure classification:
  `runtime_orchestration_gap`. This is an installer/service lifecycle ordering
  bug, not an app-output failure and not a skill-rule gap.
- Follow-up:
  wait for the active Projects worker to reach terminal status or clearly fail,
  then install pushed commit `52763ea7` through a fresh `ctox upgrade --dev` run
  without manual launchctl commands.

### 2026-06-20 Launchd Start-Order Patch Pushed

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`; Projects `queue:system::f115b955ee616f886464c54f`
- Commands:
  `git commit -m "Fix launchd service start order"`;
  `git push origin main`;
  `git status --short`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox status --json`
- Changed files:
  `install.sh`;
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781988048776.json`
- Result:
  Start-order fix is pushed as commit `52763ea7`. Local worktree is clean
  except for the unrelated pre-existing `tests/business-os/ats_synthetic_generate.sh`.
  The manually started launchd service remains healthy and busy; Projects is
  still leased by `ctox-service` and writing app artifacts. The source patch is
  not considered production-proved until `ctox upgrade --dev` installs this
  commit and starts the service without manual launchctl commands.
- Failure classification:
  no new failure. Current failure class remains `runtime_orchestration_gap`
  until the install proof passes.
- Follow-up:
  wait for Projects to finish or fail, then run `ctox upgrade --dev` to install
  `52763ea7` and verify `manager=launchd-user`, native peer
  `replicationUp=true`, and no manual service recovery.

### 2026-06-20 rfix4 Static Validation Green

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`;
  `queue:system::e8c02582c37a2e861d6171bc`,
  `queue:system::8eda6d3dd77a2779c7874b01`,
  `queue:system::f115b955ee616f886464c54f`,
  `queue:system::1ee89d974aaba0e9c2e0bc4d`,
  `queue:system::85146e296c40b9a5e9498025`
- Commands:
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `ctox status --json`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781989262104.json`
- Result:
  Installed CTOX finished all five `rfix4` app-create tasks without
  generated-app repairs. The bench snapshot reports `bench_green=true`,
  `needs_attention=false`, `handled=5`, `failed=0`,
  `validation_passed=5`, `artifact_dirs_present=5`, and
  `apps_with_missing_required_files=0`.
- Failure classification:
  no generated app-output failure at the static-validation gate. Production
  signoff remains open because browser mount, `ctx.db` persistence,
  `ctx.commandBus.dispatch` automation, and clean dev-upgrade service
  lifecycle proof are not complete.
- Follow-up:
  run browser smoke, persistence smoke, and automation smoke against the fresh
  `rfix4` apps only. Do not use older runs for signoff and do not patch
  generated app artifacts.

### 2026-06-20 Source-Binary Dev-Upgrade Lifecycle Gap

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`
- Commands:
  `cargo run --bin ctox -- upgrade --dev`;
  `ctox status --json`;
  `ctox update status`;
  `launchctl print gui/$(id -u)/com.metric-space.ctox.service`;
  `rustfmt src/core/install/mod.rs src/core/service/service.rs`;
  `git diff --check`;
  `cargo check --bin ctox`;
  `cargo test --bin ctox resolve_active_root_prefers_managed_current_when_install_root_is_known`;
  `cargo test --bin ctox parse_launchd_pid_reads_main_pid_line`
- Changed files:
  `src/core/install/mod.rs`;
  `src/core/service/service.rs`;
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `ctox upgrade --dev` attempted `branch-main-20260620T204719Z` and rolled
  back to `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T202511Z`;
  `rfix4` status remains
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781989262104.json`
- Result:
  The source-binary upgrade reached service restart, then failed with an empty
  `launchctl` error and rolled back. The running service recovered under
  `launchd-user`, but the upgrade proof is not green. Source now improves the
  diagnosis and source-upgrade path: when an install root is known,
  `InstallLayout` resolves the active root to the managed `current` symlink;
  `launchctl` errors include command arguments, status, stdout, and stderr; and
  `kickstart` failure is a warning because `bootstrap` plus `RunAtLoad` can
  already start the agent while the wait loop verifies the real service state.
- Failure classification:
  `runtime_orchestration_gap`. This is installer/service lifecycle behavior,
  not app-output failure and not a reason to add deterministic app builders or
  more prompt rules.
- Follow-up:
  install pushed commit `03ec39b0` through `ctox upgrade --dev` with the
  managed install root and verify `ctox status --json` reports
  `manager=launchd-user`, `running=true`, and native RxDB peer
  `replicationUp=true` without manual launchctl recovery.

### 2026-06-20 Dev-Upgrade Lifecycle Proof Green

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`
- Commands:
  `CTOX_INSTALL_ROOT=/Users/michaelwelsch/.local/lib/ctox cargo run --bin ctox -- upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status --json`;
  `ctox business-os app bench status --run-id rfix4 --validate --json`;
  `launchctl print gui/$(id -u)/com.metric-space.ctox.service`
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781990148609.json`
- Result:
  Source-binary dev upgrade installed release `branch-main-20260620T210628Z`
  through the managed install root. `readlink` points to
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T210628Z`.
  The LaunchAgent is `running` with working directory
  `/Users/michaelwelsch/.local/lib/ctox/current`. `ctox status --json`
  reports `running=true`, `manager=launchd-user`, `pending_count=0`,
  `blocked_count=0`, Business OS web and MCP autostarted, and native RxDB peer
  `replicationUp=true`. The `rfix4` bench remains `bench_green=true` with
  `handled=5`, `validation_passed=5`, and all app artifact directories
  present. `launchctl kickstart` returned `SIGKILL` with empty output, but the
  new code correctly treated that as diagnostic because the service started and
  health checks passed.
- Failure classification:
  no active lifecycle failure. The install/service lifecycle gate is green.
- Follow-up:
  run browser mount smoke, `ctx.db` persistence smoke, and
  `ctx.commandBus.dispatch` automation smoke against the fresh `rfix4` apps.

### 2026-06-20 rfix4 Browser Mount Smoke Green

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`
- Commands:
  Patchright browser smoke against `http://127.0.0.1:8765/` with direct module
  hash navigation for the five installed `rfix4` modules.
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/browser-mount-smoke-20260620T212626Z.json`
- Result:
  All five freshly generated `rfix4` apps mounted in the Business OS shell.
  The active module id matched the requested module, `ctoxOperational=ok`, and
  the run recorded no console errors, page errors, or request failures.
- Failure classification:
  none for browser mount. This gate is green.
- Follow-up:
  continue with persistence and automation smoke on the same generated apps.

### 2026-06-20 rfix4 Persistence And Automation Smoke Red

- Phase: 5d
- Owner: Codex
- Run id / task ids:
  `rfix4`
- Commands:
  Patchright UI smoke for create/reload/automation on all five installed
  `rfix4` modules; SQLite inspection of the native CTOX DB dynamic module
  tables and `business_commands`.
- Changed files:
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/persistence-automation-smoke-20260620T213437Z/result.json`
- Result:
  The run is red. Inventory and Quality created records through the UI,
  persisted to native dynamic module tables, survived reload, and dispatched
  real Business OS commands (`business_os.chat.task` and
  `ctox.ticket.local.create`). Subscriptions, Projects, and Contracts created
  visible UI-local records, but native main collection rows did not appear
  before timeout. Their side event collections did sync to native tables. The
  run recorded zero console errors, page errors, and request failures.
- Failure classification:
  `validator_gap` under investigation. The accepted artifacts may have a
  browser/native schema or record-shape mismatch that current validation does
  not catch. If root cause confirms the skill made schema parity too easy to
  get wrong, add a concise `skill_resource_gap` follow-up. Do not patch
  generated app files.
- Follow-up:
  compare `schema.js`, `collections.schema.json`, generated record shape, and
  native dynamic table behavior for the three failed main collections; patch
  validator/resource/runtime only after root cause is evidence-backed.

### 2026-06-20 Schema And Record Parity Validator Patch

- Phase: 5e
- Owner: Codex
- Run id / task ids:
  `rfix4` forensic recheck.
- Commands:
  `node src/apps/business-os/scripts/validate-app-module.test.mjs`;
  `git diff --check`; source validator against old `rfix4` installed
  artifacts; `ctox upgrade --dev`; installed
  `ctox business-os app validate <module-id> --installed --skip-tests
  --skip-node-check` for all five `rfix4` modules;
  `ctox business-os app bench status --run-id rfix4 --validate --json`.
- Changed files:
  `src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs`;
  `src/apps/business-os/scripts/validate-app-module.test.mjs`;
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`;
  `src/skills/system/product_engineering/business-os-app-module-development/references/green-checklist.md`;
  `docs/business-os-app-creation-plan.md`.
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781993473758.json`
- Result:
  Root cause is confirmed: accepted `rfix4` artifacts drifted between
  browser `schema.js`, native `collections.schema.json`, and record helper
  output types. The new validator imports installed `schema.js` and record
  helper modules and rejects version, collection-shape, and persisted value
  type mismatches. Commit `ebfba103` is pushed to `main` and installed as
  release `branch-main-20260620T220404Z`. Under that installed validator all
  five historical `rfix4` apps are rejected, and the bench status is correctly
  red with `validation_failed=5`.
- Failure classification:
  `validator_gap` fixed; concise `skill_resource_gap` follow-up completed.
- Follow-up:
  use only a fresh post-patch bench for signoff. Do not repair `rfix4`
  generated app files.

### 2026-06-20 Fresh Post-Parity-Validator Bench Started

- Phase: 5e
- Owner: Codex
- Run id / task ids:
  `rfix5`; Subscriptions `queue:system::1f1fc12323db6d76c2d82b4f`;
  Inventory `queue:system::ba7b80e7822eba234b516731`;
  Projects `queue:system::0a6a72bdb295789065e82cf0`;
  Contracts `queue:system::265eb0e3dce584b5352ae416`;
  Quality `queue:system::19327b78a8dda35d19ce3cea`.
- Commands:
  `ctox business-os app bench run --suite core-five --model minimax-m3
  --context 256k --run-id rfix5`;
  `ctox business-os app bench status --run-id rfix5 --validate --json`;
  `ctox status --json`.
- Changed files:
  `docs/business-os-app-creation-plan.md`.
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/events.jsonl`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781993589002.json`
- Result:
  The bench submitted five real `ctox.business_os.app.create` tasks through
  installed CTOX, removed only old `rfix4` bench modules, and preserved the
  runner contract: the bench runner did not create or repair app files. The
  initial status shows Projects leased by `ctox-service`, four tasks pending,
  and no module artifacts yet. `ctox status --json` reports CTOX running under
  `manager=launchd-user`, Business OS web and MCP autostarted, and native RxDB
  peer `replicationUp=true`.
- Failure classification:
  none yet; run is in progress.
- Follow-up:
  poll `rfix5` until terminal evidence exists, then run installed validation,
  browser mount smoke, `ctx.db` persistence smoke, and
  `ctx.commandBus.dispatch` automation smoke.

### 2026-06-20 rfix5 Partial Projects Artifact Snapshot

- Phase: 5e
- Owner: Codex
- Run id / task ids:
  `rfix5`; Projects `queue:system::0a6a72bdb295789065e82cf0`.
- Commands:
  `ctox business-os app bench status --run-id rfix5 --validate --json`;
  `ctox status --json`.
- Changed files:
  `docs/business-os-app-creation-plan.md`.
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781993846314.json`
- Result:
  Projects remains leased by `ctox-service`; the other four app-create tasks
  remain pending. Projects has a partial artifact directory with ten files and
  is still missing `index.js` and tests. Installed validation is red for the
  incomplete artifact and also reports that the partial schema redeclares the
  shell collection `business_commands`.
- Failure classification:
  none yet. This is in-progress evidence while the queue task is still leased,
  not terminal proof. Do not patch from this snapshot alone.
- Follow-up:
  wait for terminal app-validation or rework evidence, then classify the final
  artifact output. If `business_commands` redeclaration remains in terminal
  artifacts, classify it as a validator/resource/reference gap or model failure
  based on whether the same pattern repeats beyond Projects.

### 2026-06-20 rfix5 Projects Terminal And Quality Leased

- Phase: 5e
- Owner: Codex
- Run id / task ids:
  `rfix5`; Projects `queue:system::0a6a72bdb295789065e82cf0`;
  Quality `queue:system::19327b78a8dda35d19ce3cea`.
- Commands:
  `ctox business-os app bench status --run-id rfix5 --validate --json`;
  `ctox status --json`;
  `ctox queue list --status pending --limit 10`;
  `ctox queue list --status leased --limit 10`.
- Changed files:
  `docs/business-os-app-creation-plan.md`.
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781994605567.json`
- Result:
  Projects reached terminal success with installed validation green, 12
  required runtime files, and 29 passing module tests. CTOX then leased Quality
  through the normal queue path. The live service reports `busy=true`,
  `worker_active_count=1`, `manager=launchd-user`, Business OS web and MCP
  autostarted, and native RxDB peer `replicationUp=true`. Subscriptions,
  Inventory, and Contracts are still pending with no artifacts yet.
- Failure classification:
  none. This is live in-progress evidence, not production signoff.
- Follow-up:
  let Quality reach terminal evidence, continue the remaining pending tasks,
  and only classify failures after terminal validation/rework evidence exists.

## Handoff Notes

Latest handoff:

- Continue Phase 5 from run id `rfix5`.
- Active installed release is `branch-main-20260620T220404Z`. `readlink
  /Users/michaelwelsch/.local/lib/ctox/current` points to
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T220404Z`.
- `rfix4` is forensic evidence only. Under the installed parity validator it is
  correctly red:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix4/status-1781993473758.json`
  reports `bench_green=false` and `validation_failed=5`.
- The `rfix4` persistence blocker was root-caused as schema/record parity
  drift. Source and installed validation now compare `schema.js`,
  `collections.schema.json`, and record helper output types.
- Commit `ebfba103` is pushed to `main`; the patch was installed with
  `ctox upgrade --dev` as release `branch-main-20260620T220404Z`.
- Fresh run `rfix5` was started through installed CTOX with
  `ctox business-os app bench run --suite core-five --model minimax-m3
  --context 256k --run-id rfix5`.
- `rfix5` submitted five real `ctox.business_os.app.create` tasks and the
  bench runner did not create or repair app files. Evidence:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/events.jsonl`.
- Initial `rfix5` status:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781993589002.json`
  shows Projects leased by `ctox-service`, four tasks pending, and no module
  artifacts yet.
- Latest `rfix5` status:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781994605567.json`
  shows Projects handled with installed validation green, Quality leased by
  `ctox-service`, and Subscriptions, Inventory, and Contracts still pending
  with no artifacts.
- `ctox status --json` at that point reports `running=true`,
  `manager=launchd-user`, `busy=true`, `worker_active_count=1`, current goal
  preview for Quality, Business OS web and MCP autostarted, and native RxDB
  peer `replicationUp=true`.
- Next required action: poll `rfix5` until all five tasks are terminal or a
  failure class is clear. Then run installed validation for complete app
  artifacts, browser mount smoke, `ctx.db` persistence smoke, and
  `ctx.commandBus.dispatch` automation smoke.
- Do not use `rcli`, `rfix1`, `rfix2`, `rfix3`, or `rfix4` as production
  signoff. They are forensic evidence only.
- Do not patch generated app files.
- Do not add deterministic app builders.
- Do not add new skill rules unless a repeated app-output failure shows a
  concrete missing Business OS architecture expectation or reusable validator
  boundary.
- Update Current Status, Current Execution Slice, Tracker, Evidence Log, Open
  Issues, and this Handoff section before each handoff.

## Open Issues

- Define validator behavior for internal shell tools such as App Creator.
- Track `bench_projects_rcli` as a current model-output failure unless CTOX
  review/rework repairs it or the same inconsistency repeats across more runs.
- Track `bench_inventory_rfix1` helper-test failures as model-output failures
  unless they repeat after the reference-catalog patch is installed.
- Add or wire wait/status collection for bench task completion beyond the
  current read-only status snapshot.
- Add browser smoke and automation smoke collection to bench evidence.
- Confirm the installed reference ranking prevents normal app creation from
  overfitting to internal developer tools.
- Confirm every app creation entry point attaches the same structured skill
  resource context.
- Complete the active `rfix5` run and classify any failure before patching:
  installed validation, browser mount, `ctx.db` persistence, and
  `ctx.commandBus.dispatch` automation must all be proven on the fresh
  post-parity-validator artifacts.
- Complete the final production proof on a fresh post-patch run:
  installed validation, browser mount, `ctx.db` persistence, and
  `ctx.commandBus.dispatch` automation for all five apps.
