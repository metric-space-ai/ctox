# Business OS App Creator Production Readiness Plan

Purpose: make the CTOX Business OS App Creator and all inbound app-building
paths reliably produce runnable Business OS apps with MiniMax M3 in CTOX-native
execution, not only in source-checkout agent benches.

This plan is intentionally operational. Agents working through it should update
the phase tracker in this file as evidence is produced.

## Current State

The normal MiniMax M3 source-checkout bench passed in Round 16: five simple
Business OS app prompts produced five independently green source modules.

The CTOX-native installed-module bench did not pass. In native Round R8, the
first generated app (`contracts`) wrote files under the correct installed
module directory, but still failed installed-module semantics and architecture
checks:

- wrong `module.json.entry` for installed mode
- wrong `module.json.install_scope` for installed mode
- default `layout.right` / third pane
- `business_commands` exported from `schema.js`
- missing required files such as `index.css`, locales, and tests
- raw DB access patterns in `index.js`
- fragile large JavaScript generation through nested shell/Node string builders

Conclusion: the skill text is good enough for a source-checkout agent when
independent gates are run, but CTOX-native app creation is not production-ready
until runtime validation and repair feedback are hard-wired.

Update 2026-06-13: the CTOX runtime now has a mode-aware app artifact validator,
source/installed validator fixtures, and a same-queue repair path with a
`validator_rework` core-state witness.

Native R9 proved that a release-like isolated root must include the normal CTOX
runtime secret store before MiniMax M3 can run. That was an environment
bootstrap failure, not a skill failure.

Native R10 reached MiniMax M3 through CTOX with 256k context and produced real
app artifacts, but failed on repeatable Business OS module-contract mistakes:
installed-mode manifest errors, missing required files, forbidden third panes,
wrong schema ownership, remote/dependency-management patterns, `.bak` repair
artifacts, and root-level blocker/status notes. The service also showed that
generic completion review could intercept bad app output before the deterministic
app validator supplied its more precise repair prompt. The service hook has been
hardened so Business OS app validation runs for app tasks even when the generic
review would otherwise produce feedback, and generic review feedback is skipped
when app validation owns that rework turn.

Native R11 confirmed that prompt text alone still does not reliably stop a
mid-tier agent from producing Business OS-invalid artifacts. The generated app
again created root-level `module.json` and `collections.schema.json`, retained a
default third/right pane, and mixed shell-owned schema references into module
schema files. It also showed that relying on generic completion review before
the deterministic app validator adds latency and weaker feedback.

Native R12 proved the pre-review app-validator hook: the `subscriptions` app was
not marked complete, the validator wrote repair feedback back onto the same
queue task, recorded a `validator_rework` proof, and moved the task into
`review_rework`. R12 also exposed two production blockers that are now hardened:
the agent wrote root-level app artifacts via `python3 -c`/`pathlib.write_text`,
which bypassed the old simple shell-redirection guard, and the idle dispatcher
preferred fresh `pending` app tasks over validator `review_rework`. The runtime
now removes newly-created root-level app artifacts after exec, reports that as a
tool error, preserves app-build target metadata in validator repair prompts, and
releases Business OS app validation rework back through a legal
`ReworkRequired -> Pending -> Executing` core-transition path before leasing new
app work.

Native R13 is not valid evidence for App Creator quality. It used generic
`ctox queue add --skill` instead of Business OS command dispatch, so the worker
did not receive the App Creator target block and wrote a source module. The
takeaway is a bench-runner rule: production readiness evidence must enter
through `ctox.business_os.app.create`, App Creator, App Store, Business Chat, or
another real Business OS app-building route.

Native R13c is valid CTOX-native evidence and failed on the first worker. The
command used `ctox.business_os.app.create`, `module_id=subscriptions`,
`install_target=runtime-installed-module`, and a 256k MiniMax M3 worker prompt
containing the installed-module target contract. The worker wrote the main app
under `src/apps/business-os/installed-modules/subscriptions`, but also created
root-level harness aliases/status files, used a default right/third pane, used
`esbuild`/`npx` as a test workaround, left forbidden `esbuild` literals in app
files, and shipped a failing DOM test. The round was intentionally stopped after
the first worker because this was a systemic guard gap, not a domain-specific
app issue.

Post-R13c hardening now includes broader exec blocking and cleanup for
workspace-root app artifact aliases, direct validator detection of root aliases,
validator preference for its bundled/current static checker over a stale target
workspace copy, and stronger skill instructions forbidding root aliases, probe
files, bundler/test-transform workarounds, and default third panes.

Native R14 reached MiniMax M3 through the real
`ctox.business_os.app.create` path in a release-like isolated root after the
encrypted CTOX secret store was provisioned. The first worker (`inventory`)
eventually produced an app that the current artifact validator reported green,
but the forensic scan found validator gaps and repeated risky patterns:

- `module.json` was briefly written as invalid JSON and the Business OS module
  catalog sync observed the parse failure before the agent repaired it.
- `collections.schema.json` was initially missing
  `schema_format: "ctox-business-os-module-collections-v1"`.
- the agent created root-level probe files such as `_test_guard.txt`.
- the agent introduced a module-local `package.json` with `"type": "module"` to
  make Node tests easier, then later removed it.
- the final `index.js` still used `localStorage` for UI state even though
  generated apps must not introduce browser-local persistence.
- the app comments assumed a shell "right rail" and implemented resizer/local
  preference behavior instead of keeping state in the Business OS contract.

R14 is therefore a valid CTOX-native failure, even though the validator returned
green after the worker's final repair. The immediate blocker is no longer only
model behavior: the validator, exec guard, and skill must reject localStorage /
sessionStorage, root probe files, module-local package-manager side effects,
and transient invalid JSON exposure before a generated app can be accepted.

Native R15 is another valid CTOX-native failure and explains why the earlier
source-checkout guard confidence did not transfer into CTOX. The R15 workers
entered through real `ctox.business_os.app.create` commands and used MiniMax M3
with 256k context, but the MiniMax adapter still used the legacy shell command
path (`unified_exec_enabled() == false`). The Business OS root-artifact guard
had only protected `unified_exec`, so the worker bypassed it through the shell
tool, created root-level `module.json` / `test-file.json` probes, tried
symlink/hardlink aliases, and temporarily corrupted the installed inventory
manifest with invalid test content. R15 was stopped after the first worker
because this was a runtime enforcement gap, not an app-domain issue.

Post-R15 hardening moves the Business OS app-artifact write guard into the
shared shell paths used by legacy MiniMax execution, broadens the forbidden
root-probe patterns (`test-*`, `*_test.*`, `*-test.*`, `_probe_*`), blocks
root-level symlink/hardlink/copy attempts, and keeps module-directory writes
legal.

Native R16 is valid CTOX-native evidence and failed on a new runtime-control
gap. It used real `ctox.business_os.app.create` dispatch, MiniMax M3 with 256k
context, and a release-like isolated root. The first worker (`projects`) stayed
inside the installed-module directory and the deterministic validator correctly
caught Business OS contract failures (`layout.right`, forbidden dependency
literals in generated comments/tests, and a failing module test). However, the
MiniMax/Responses turn then aborted with `invalid function arguments json
string`. The service marked the app queue task `failed` and immediately leased a
fresh app task because app validation feedback only ran on successful worker
turns. That is not production-ready: provider/tool-call errors must not bypass
deterministic app validation when red app artifacts exist.

Post-R16 hardening runs Business OS app validation after worker/model/tool-call
errors for app tasks, writes deterministic feedback onto the same leased queue
task, records a `validator_rework` proof, and acknowledges the task as
`review_rework` instead of failing it or leasing the next app. The target prompt
and skill now also forbid guard probing, generated-file copies of forbidden
dependency names, and mammoth single tool-call writes.

Native R17 is valid CTOX-native evidence and failed on the next runtime-control
gap. It used real `ctox.business_os.app.create` dispatch, MiniMax M3 with 256k
context, and a release-like isolated root. The first worker (`subscriptions`)
hit deterministic app validation, received same-task repair feedback, and then
produced app artifacts that the validator reported green. However, the worker
session errored after the green validation, and the generic error path still
marked the queue task `failed`. That is not production-ready: once deterministic
Business OS app validation is green, a late worker/provider error must complete
the App Creator command as validator-verified rather than overriding the green
gate with a generic failure.

Post-R17 hardening completes Business OS app commands after green app validation
even when the worker turn ends with an error, updates the queue task as handled,
stores app-validation result metadata on the `business_commands` projection, and
keeps the synthetic no-command queue fallback handled for tests and legacy
tasks. The next valid evidence round is R18 from a fresh isolated root using
real Business OS command dispatch.

## Why CLI Passed But CTOX-Native Failed

The source bench and CTOX-native bench test different contracts.

Source-checkout bench:

```text
target: src/apps/business-os/modules/<module_id>/
entry: modules/<module_id>/index.html
install_scope: store
agent sees source tree directly
external harness runs static check, tests, conformance, rxdb-only
```

CTOX-native App Creator bench:

```text
target: src/apps/business-os/installed-modules/<module_id>/
entry: installed-modules/<module_id>/index.html
install_scope: installed
agent is driven through Business OS commands and queue prompts
validation must be automatic runtime feedback, not a manual forensic step
```

The R8 failure means the App Creator currently lacks a hard runtime equivalent
of the source-bench gates. Prompt instructions alone are not sufficient.

## Definition Of Production Ready

The App Creator is production-ready when a non-frontier coding model can create
or modify a Business OS app through CTOX-native Business OS entry points and the
result is safe, runnable, persisted correctly, and validated before completion.

Production-ready does not mean every generated app is feature-complete or
beautiful. It means the app creation system never silently accepts Business
OS-invalid output.

Required properties:

- app creation works through App Creator, App Store, Business Chat, and command
  queue entry points
- `business-os-app-module-development` is always required for app create/modify
  tasks and legacy skill names are stripped
- default context budget is 256k tokens for the worker path
- generated apps target the correct mode: source module or installed module
- generated modules use CTOX DB / RxDB / WebRTC data plane only
- generated modules do not introduce HTTP, direct IndexedDB, Postgres, SQLite,
  localStorage, sessionStorage, package-manager, bundler, or CommonJS fallbacks
- generated modules use browser-safe ESM only
- generated modules persist records through shell-provided Business OS module
  collections and command bus helpers
- shell-owned collections may be declared as dependencies but are not exported
  from module schemas
- one/two-pane UI plus modals/drawers is the default; third panes are explicit
  exceptions with a concrete workflow justification
- automation actions create normal CTOX chat/ticket/work follow-up through the
  Business OS command flow
- validation failures are returned to the agent as actionable repair feedback
- commands are not marked completed until validation and live smoke checks pass
- generated apps are removable, versioned, and do not contaminate future runs

## Non-Goals

Do not make the App Creator a broad generator workbench.

Do not expose internal prompts, raw harness logs, command IDs, or generated
debug artifacts as the normal user experience.

Do not solve CTOX-core first-class collection generation inside normal app
creation. Normal generated apps must use module-owned dynamic schemas and
existing shell collections. Creating native sync collections is a separate
CTOX-core change with wire-contract fixtures and guard tests.

Do not accept "green" from an agent's final message. Green means independent
runtime gates passed.

## Architecture Rules

### Entry Points

Every inbound app-building path must route to the same app module creation
contract:

- App Creator new app
- App Store new app
- App Store edit/duplicate/update
- Business Chat app request
- command queue `ctox.business_os.app.create`
- command queue `ctox.business_os.app.modify`
- inbound communication that clearly requests a CTOX or Business OS app

Each path must set or infer:

```text
required_skill: business-os-app-module-development
mode: create | modify
module_id: explicit normalized module id
install_target: runtime-installed-module | source-module
artifact_directory: exact allowed module directory
context_tokens: 262144
```

### Source vs Installed Mode

Source module:

```text
directory: src/apps/business-os/modules/<module_id>/
module.json.entry: modules/<module_id>/index.html
module.json.install_scope: store
registry: src/apps/business-os/modules/registry.json must be updated
```

Installed module:

```text
directory: src/apps/business-os/installed-modules/<module_id>/
module.json.entry: installed-modules/<module_id>/index.html
module.json.install_scope: installed
registry: packaged source registry must not be edited
```

### Minimum Module Contract

Every generated app must include:

```text
module.json
index.html
index.css
index.js with export async function mount(ctx)
schema.js for module-owned schemas when records are persisted
collections.schema.json for module-owned collection schemas
icon.svg
locales/de.json
locales/en.json
tests/*.test.mjs
README.md or module-local implementation note only if it does not contain
negative-proof forbidden literals
```

### Persistence Contract

Do:

- use shell-provided Business OS module context
- use declared module-owned collections
- dispatch follow-up work through `ctx.commandBus` or the existing shell command
  helper
- declare shell collections in `module.json.collections` only when needed

Do not:

- create `ctox.db`
- use direct IndexedDB/Postgres/SQLite APIs
- use HTTP endpoints for Business OS records or commands
- import RxDB internals from generated app code
- export `business_commands`, `ctox_queue_tasks`, `business_module_catalog`, or
  `ctox_runtime_settings` from `schema.js` or `collections.schema.json`
- manually insert fallback `pending_sync` records when `ctx.commandBus` is
  unavailable; disable the action and show a clear error instead

## Required Validation Layers

### Layer 1: Command Router Contract

Before the worker starts, CTOX must normalize every app command:

- required skill is `business-os-app-module-development`
- legacy `business-basic-module-development` is removed
- install target and artifact directory are explicit
- prompt includes skill access instructions for release installs:
  embedded skill first, `ctox skills system show/export` second, versioned
  GitHub fallback third
- prompt includes source-vs-installed target contract
- prompt includes hard stop on root-level artifacts and wrong mode manifests

### Layer 2: Exec Write Guard

Runtime exec must block obvious bad writes:

- root-level `module.json`
- root-level `collections.schema.json`
- root-level `<module_id>/` app artifacts
- writes to `src/skills/` for app deliverables
- package manager lockfiles or `node_modules` in generated modules

The guard blocks root-level manifest writes through simple shell redirection,
common Python/Node write APIs, and a post-exec cleanup pass. If a command creates
a previously absent workspace-root `module.json` or `collections.schema.json`,
CTOX removes the file and returns a tool error to force immediate repair.

Still open: broaden runtime write-guard coverage for root-level `<module_id>/`,
`src/skills/` app deliverables, package-manager side effects, and long-running
processes that may write after the first exec yield.

### Layer 3: Artifact Validator

After relevant writes and before command completion, CTOX must run a
mode-aware validator.

Required checks:

- all minimum module files exist
- JSON files parse
- `module.json.id` matches command `module_id`
- `module.json.entry` matches source or installed mode
- `module.json.install_scope` matches source or installed mode
- source-mode registry is updated; installed-mode registry is not edited
- `layout.right` is absent unless `layout.third_pane_justification` is present
  and specific
- HTML/CSS does not implement a third/right pane by default
- `schema.js` and `collections.schema.json` export only module-owned
  collections
- shell collections are only dependencies, never module schemas
- no forbidden data path strings in production code
- no `localStorage` or `sessionStorage` in generated app code, tests, comments,
  or docs
- no dependency-management files
- no CommonJS or bundler-only imports
- `node --check index.js` passes
- module tests pass
- RxDB-only guard passes for relevant Business OS code

Validator output must be short, explicit, and repair-oriented. Example:

```text
Business OS app artifact validation failed for contracts:
- installed module manifests must use entry="installed-modules/contracts/index.html"; found "modules/contracts/index.html"
- installed module manifests must use install_scope="installed"; found "store"
- schema.js exports shell collection "business_commands"; keep it in module.json.collections only
- remove layout.right or add layout.third_pane_justification with a concrete workflow reason
Repair these files under src/apps/business-os/installed-modules/contracts/ and rerun validation.
```

### Layer 4: Agent Repair Loop

The queue worker must not finish on the first artifact attempt.

Required loop:

1. generate or modify module
2. run artifact validator
3. if validation fails, feed exact validator output back to the same worker
4. allow a bounded repair attempt
5. repeat until green or max attempts reached
6. mark command `completed` only after green gates
7. mark command `failed` with validator evidence after max attempts

Suggested limit:

```text
max_generation_attempts: 1
max_repair_attempts: 3
```

### Layer 5: Live Shell Smoke

For production readiness, static validation is not enough. CTOX must verify the
installed app loads in the Business OS shell.

Minimum smoke:

- app appears in installed modules/catalog
- shell can navigate to the app
- `mount(ctx)` resolves without console/runtime errors
- seeded or empty state renders coherently
- create/update path persists one module-owned record
- automation button dispatches a normal command flow through the command bus
- reload still shows persisted data
- app remains valid if shell/catalog sync reads `module.json` and
  `collections.schema.json` while generation is in progress; agents must use
  atomic writes for JSON manifests

## Phase Tracker

Agents should update this table as work progresses.

| Phase | Status | Evidence | Notes |
| --- | --- | --- | --- |
| 0. Preserve R8 evidence | Done | `docs/business-os-skill-bench-2026-06-12.md` R8 section | R8 stopped after first hard failure |
| 1. App command routing audit | In progress | `app_modify_queue_prompt_targets_app_module_not_skill_files`, `app_create_queue_prompt_targets_app_module_skill` | App create/modify command prompt and suggested skill covered; App Creator/App Store/browser entry audit still open |
| 2. Mode-aware artifact validator | In progress | `src/apps/business-os/scripts/validate-app-module.mjs`; `node src/apps/business-os/scripts/validate-app-module.test.mjs`; R8/R13c contracts validation failures; R14 false-green on `localStorage` | Covers source/installed static gate, source registry requirements, ESM syntax, module tests, JSON/report output; must now close Web Storage, root probe, package side-effect, and atomic JSON gaps |
| 3. Runtime repair loop | In progress | `business_os_app_validation_feedback_requeues_same_task`, `business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`, `business_os_app_validation_repair_attempt_count_caps_after_three`, `business_os_app_validation_feedback_is_repair_oriented`, `business_os_app_validation_worker_error_keeps_same_task_reworkable`, `business_os_app_validation_worker_error_after_green_completes_business_command`; R12/R16/R17 validator-rework event proof | Worker validates app module artifacts before generic completion review and after worker errors, writes failures to the same queue task, records `validator_rework`, preserves parseable app target metadata in repair prompts, routes through legal `review_rework -> pending -> leased`, completes commands after green validation despite late worker errors, and fails after bounded attempts; still needs fresh CTOX-native 5/5 confirmation |
| 4. Validator integration tests | In progress | `node src/apps/business-os/scripts/validate-app-module.test.mjs`; `cargo test --manifest-path src/core/harness/core/Cargo.toml business_os_`; `business_os_app_validation_feedback_requeues_same_task`; root-alias and stale-checker validator fixtures | Source/installed fixtures, queue-state proof, bundled-checker preference, and root-level exec guard are covered; broader App Creator/App Store flow tests still open |
| 5. App Creator UI flow check | Not started |  | Minimal user UI, no generator workbench |
| 6. CTOX-native R9-R18 bench | In progress | R9 isolated-root credential failure; R10 MiniMax execution and validator failures; R11 root-artifact/generic-review delay; R12 validator-rework proof and dispatch/root-write findings; R13 invalid generic queue setup; R13c valid command-dispatch first-worker failure; R14 validator false-green; R15 legacy-shell guard bypass; R16 worker-error red-artifact bypass; R17 green-validation late-error failure | Five simple app prompts through real CTOX app-command paths; next round must rerun from a fresh isolated root after R17 hardening |
| 7. Skill/resource cleanup from native evidence | In progress | R10/R13c/R14 failures folded into skill, module contract, verification docs, validator forbidden-file checks, bundled-checker preference, and exec guard prompt feedback | Only evidence-backed edits |
| 8. Repeat native bench until 5/5 | Not started |  | No overfitting to one app |
| 9. Release-install validation | Not started |  | No developer-local source paths required |
| 10. Production readiness sign-off | Not started |  | All gates green with evidence |

Status values:

```text
Not started
In progress
Blocked
Done
Rejected
```

## Phase Details

### Phase 1: App Command Routing Audit

Goal: every app-building entry point reaches the same CTOX-native contract.

Tasks:

- inspect App Creator command creation
- inspect App Store create/edit/duplicate flow
- inspect Business Chat app-intent routing
- inspect command queue prompt rendering
- inspect inbound communication routing for CTOX/Business OS app requests
- add tests for German and English app-create phrasing
- add tests that `business-basic-module-development` never remains in required
  skills for app work
- add tests that 256k context is selected for MiniMax M3 app worker tasks

Do not:

- add a second app-building path
- special-case only the current bench prompts
- rely on browser-only state for run status

Exit criteria:

- tests prove all app-create/modify paths carry the same skill and target
  metadata
- prompt preview contains the exact allowed artifact directory
- source vs installed target is unambiguous in prompt and command metadata

### Phase 2: Mode-Aware Artifact Validator

Goal: the CTOX runtime can independently reject Business OS-invalid generated
modules.

Tasks:

- promote or reuse `module_static_check.mjs` for runtime source and installed
  targets
- add a stable CTOX command or internal helper for validation
- make validator output concise and repair-oriented
- include explicit installed-mode checks from R8
- include schema ownership checks for shell collections
- include UI third-pane checks
- include forbidden runtime pattern checks
- include no package-manager/no dependency-management checks
- include `node --check` for generated ESM
- include module-local tests when present
- ensure validator is shipped in release builds or available through embedded
  skill resources, not only developer-local source paths

Do not:

- weaken checks to make generated apps pass
- accept legacy modules as green references if they violate current rules
- parse JavaScript with ad hoc string assumptions when a structured or
  conservative check is practical

Exit criteria:

- validator fails the R8 `contracts` artifacts for the real reasons
- validator passes known-good Round 16 outputs
- validator has unit/fixture tests for source and installed mode

### Phase 3: Runtime Repair Loop

Goal: failed app artifacts become immediate agent feedback, not forensic notes.

Tasks:

- run validator after the app worker writes files
- if failed, append validator output to the active worker context
- require the worker to repair only files under the allowed module directory
- rerun validation after each repair
- stop after max repair attempts with a failed command and evidence
- keep command status and progress projections durable in CTOX DB

Do not:

- mark commands completed because files exist
- let the worker continue after writing outside allowed directories
- hide validator errors behind generic "generation failed" messages

Exit criteria:

- an intentionally bad manifest is repaired or rejected with exact evidence
- command projection shows validation attempts and final gate result
- no app appears as installable until validation passes

### Phase 4: Validator Integration Tests

Goal: architecture regressions are caught by automated tests.

Required test cases:

- installed module with `entry: modules/<id>/index.html` fails
- installed module with `install_scope: store` fails
- source module missing registry entry fails
- installed module editing packaged registry fails
- `layout.right` without justification fails
- HTML/CSS third pane without justification fails
- `business_commands` in `schema.js` fails
- `business_commands` in `collections.schema.json` fails
- root-level `module.json` write is blocked
- package manager files in module directory fail
- CommonJS `require` in generated browser module fails
- raw DB / HTTP / direct IndexedDB patterns fail
- valid minimal installed module passes
- valid minimal source module passes

Exit criteria:

- targeted tests pass locally
- existing RxDB-only guard remains green
- no direct patches to `src/apps/business-os/rxdb/dist`

### Phase 5: App Creator UI Flow Check

Goal: the user-facing App Creator stays simple while the App Store carries the
full management surface.

Tasks:

- verify App Creator starts chat-guided create flow
- verify template selection is compact
- verify progress is user-readable
- verify technical diagnostics are behind details or in App Store
- verify successful install opens the new app or App Store detail
- verify failed validation shows concise user-level cause and a retry path

Do not:

- expose raw harness prompts as primary UI
- add a default three-column generator console
- show fake install/preview actions that do not execute real commands

Exit criteria:

- App Creator can create a run, show progress, show validation failure, and
  show success using persisted CTOX projections
- UI has no decorative or default third pane

### Phase 6: CTOX-Native R9+ Bench

Goal: rerun the five intentionally simple prompts through real CTOX paths.

Native R9/R10 added two setup rules that every later round must follow:

- Use a fresh isolated root that still contains required CTOX root markers:
  `Cargo.toml`, `src/core/main.rs`, `contracts/history/creation-ledger.md`,
  `README.md`, `HARNESS.md`, and relevant docs.
- Copy or provision the CTOX SQLite secret store into the isolated
  `CTOX_STATE_ROOT` before starting the service. Do not put provider credentials
  into prompts, files, or process-env fallbacks.

Prompts:

```text
Build a Business OS app for subscription contracts: customers, MRR, renewals, churn risk, and one follow-up automation for upcoming renewal work.

Build a Business OS app for inventory: stock items, locations, minimum stock, pick lists, batches, and one follow-up automation for low-stock work.

Build a Business OS app for projects and orders: fixed price vs time-material work, milestones, budget vs actuals, billing readiness, and one follow-up automation for project work.

Build a Business OS app for customer contracts: SLAs, renewal dates, cancellation deadlines, linked customers, and one follow-up automation for deadline work.

Build a Business OS app for quality and compliance: complaints, audits, formal findings, due dates, evidence notes, and one follow-up automation for compliance work.
```

Rules:

- use MiniMax M3
- use 256k context
- use CTOX-native command/App Creator flow
- use installed-module targets
- do not overspecify prompts
- run all five from clean isolated state
- delete or isolate generated apps before each retry loop
- collect exact validation attempts and final statuses
- verify the runtime provider reports `ctox_core_api` or the configured release
  runtime API path, not a process-env fallback

Exit criteria:

- all five generated apps pass artifact validation
- all five load in the Business OS shell
- all five persist at least one module-owned record
- all five dispatch one normal automation command
- no app uses forbidden persistence or dependency patterns

### Phase 7: Evidence-Based Skill Cleanup

Goal: update skill/resources only from repeated bench evidence.

Tasks:

- classify native bench failures by majority pattern
- update the skill only where a rule was absent or ambiguous
- add curated good references and legacy anti-reference notes
- keep skill and resources in English
- ensure release users can access the skill without developer-local paths
- explicitly translate common web-app instincts into Business OS equivalents:
  no `npm install`, no bundler, no IndexedDB/Postgres/SQLite/HTTP backend, no
  root app directory, no default three-column shell, no app-owned shell
  collections

Do not:

- overfit to a single app domain
- add long prompt text that duplicates runtime validation
- let the skill become the only enforcement mechanism

Exit criteria:

- each skill edit cites a bench failure or missing rule
- validator handles hard constraints; skill explains architecture and workflow

### Phase 8: Repeat Native Bench Until Green

Goal: prove the App Creator is stable, not lucky.

Minimum success bar:

```text
two consecutive CTOX-native bench rounds
5/5 apps green in each round
no manual repair by the operator
no hidden source-checkout-only dependency
no stale generated apps influencing the next run
```

Failure handling:

- if 3/5 or more apps fail the same rule, harden validator/prompt/skill
- if 1/5 fails from generic coding error, check whether runtime feedback should
  catch it
- if a failure is pure model quality but app remains structurally valid, document
  it without overfitting

### Phase 9: Release-Install Validation

Goal: the flow works for normal CTOX installations, not only this development
checkout.

Tasks:

- run bench in a reduced/release-like workspace
- verify embedded skill access works
- verify versioned GitHub fallback URL works
- verify validator is available without local `src/skills/...` assumptions
- verify no path in prompts references `/Users/...`
- verify app writes target the runtime installed-module store

Exit criteria:

- release-like run creates and validates an installed module
- no developer-local path is needed for skill, validator, or examples

### Phase 10: Production Readiness Sign-Off

Required sign-off evidence:

```text
command routing tests green
artifact validator tests green
runtime repair-loop tests green
RxDB-only guard green
Business OS app module static checks green
CTOX-native R9+ bench: 5/5 green
second CTOX-native bench: 5/5 green
release-like installed-module run green
manual browser smoke for App Creator create/fail/success flow complete
generated bench apps removed or isolated
```

Only after this evidence should the App Creator be called production-ready.

## Implementation Backlog

### P0

- Close the R14 validator false-green: reject `localStorage`, `sessionStorage`,
  root probe files, module-local `package.json`/lockfiles/`node_modules`, and
  transient JSON-manifest exposure before an app can be accepted.
- Run CTOX-native R18 from a fresh isolated root using the rebuilt runtime and
  real Business OS app command dispatch. Do not use generic `queue add --skill`
  as production-readiness evidence.
- Prove worker/model/tool-call errors on app tasks still run the deterministic
  app validator and convert red app artifacts into same-task `review_rework`
  before any fresh app task is leased.
- Prove worker/model/tool-call errors after green app validation complete the
  Business OS app command as validator-verified instead of marking the queue
  task failed or leaving `business_commands.status` accepted.
- Prove app-validator `review_rework` tasks are leased before fresh pending app
  tasks in a native run, not only a unit test.
- Prove root-level app artifacts, aliases, and probe files are blocked or
  removed and returned to the agent as tool errors in a native run:
  `module.json`, `collections.schema.json`, `harness-module.json`,
  `harness-collections.schema.json`, artifact/status/blocker Markdown, and
  root-level `test-*` / `*_test.*` / `*-test.*` / `_test_*` / `_probe_*` files.
- Prove the native validator uses the current bundled validator/static checker,
  not a stale target-workspace skill copy.
- Extend the exec guard for root-level `<module_id>/` app directories.
- Extend the exec guard for generated app artifacts under `src/skills/`.
- Prove the exec guard blocks module-local package-manager side effects:
  `package.json`, lockfiles, and `node_modules`, including the `"type":
  "module"` test workaround.
- Decide whether exec should block `npx`, `esbuild`, Vite, Rollup, Webpack,
  `node:vm`, and `new Function` during Business OS app tasks, or whether
  validator failure is sufficient. Do not allow these patterns to reach a
  completed app.
- Prove forbidden package-manager/bundler/dependency literals in generated app
  files, tests, comments, and user-visible copy are rejected or repaired before
  completion.
- Prove generated app files are small enough for stable tool-call JSON, or that
  agents split large writes into bounded chunks without malformed provider
  function arguments.
- Require agents to write `module.json` and `collections.schema.json`
  atomically or add runtime/catalog tolerance for in-progress invalid JSON.
- Add live shell smoke automation for installed modules.
- Keep this plan's phase tracker and evidence log current after every native
  bench round.

### P1

- Curate good app references for the skill and label legacy patterns.
- Add release-like skill/validator availability checks.
- Add App Creator UI failure/success projection states.
- Audit all app-building entry points beyond queue prompt rendering:
  App Creator UI, App Store create/edit/duplicate, Business Chat, and inbound
  communications.

### P2

- Add App Store version/rollback validation around generated modules.
- Add richer module permission review before install.
- Add optional publish/export flow after core production readiness is proven.

## Hard Do / Don't List

Do:

- enforce architecture with runtime validation
- use MiniMax M3 as the proving model
- keep prompts simple in the bench
- keep generated app files under the exact allowed module directory
- validate source and installed modes differently
- use CTOX DB / RxDB / WebRTC only
- use browser-safe ESM only
- prefer one/two-pane layouts with modals or drawers
- dispatch automation through the normal Business OS command flow
- make every failure actionable for the agent

Do not:

- accept agent self-reports as proof
- use developer-local source paths in release validation
- add npm/package-manager/bundler requirements to generated apps
- create HTTP or database fallbacks
- export shell-owned collections from module schemas
- create default third-pane UI
- patch RxDB dist bundles directly
- weaken guard tests to pass generated output
- continue a bench round after a hard systemic first-worker failure unless the
  remaining runs add new evidence

## Evidence Log

Append new bench rounds here.

### Native R8

```text
status: failed
first worker: contracts
main result: target directory fixed, installed-mode semantics still failed
next action: add runtime artifact validator and repair loop before R9
```

### Native R13/R13c 2026-06-13

```text
R13 status: invalid bench setup
R13 issue: used generic ctox queue add --skill; no Business OS App Creator
target block; worker wrote a source module
R13 takeaway: future evidence must use Business OS command dispatch, App
Creator, App Store, Business Chat, or another real app-building route

R13c status: valid CTOX-native first-worker failure
entry path: ctox.business_os.app.create
model: MiniMax M3
context: 256k
target: src/apps/business-os/installed-modules/subscriptions
main failures:
- created root-level harness-module.json
- created root-level harness-collections.schema.json
- created root-level harness-artifact-status.md
- claimed root harness aliases were permitted
- probed root writes instead of using MODULE_DIR only
- used layout.right/default right-third pane and right resizers
- searched for and used esbuild/npx test-transform workarounds
- left forbidden esbuild literals in generated app files/tests
- shipped a failing DOM test

hardening added after R13c:
- exec guard blocks root-level manifest/schema aliases and artifact/status notes
- post-exec cleanup removes newly-created root-level app artifact aliases
- module static checker scans all root files for forbidden app artifact aliases
- app validator directly scans root aliases and prefers its bundled/current
  static checker over stale target-workspace copies
- skill now explicitly forbids root aliases, root probe files, esbuild/bundler
  test workarounds, and default third panes

verification:
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --manifest-path src/core/harness/core/Cargo.toml business_os_ -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox
- node src/apps/business-os/scripts/validate-app-module.mjs subscriptions --installed --workspace /tmp/ctox-bos-native-r13c-20260613-165734 --json

next action:
- rebuild CTOX and run native R14 from a clean isolated root through
  ctox.business_os.app.create for all five bench prompts
```

### Native R14 2026-06-13

```text
status: valid CTOX-native first-worker failure
entry path: ctox.business_os.app.create
model: MiniMax M3
context: 256k
target: src/apps/business-os/installed-modules/inventory
isolated root: /tmp/ctox-bos-native-r14-20260613-174826

setup finding:
- a release-like isolated root needs both runtime/ctox.sqlite3 runtime settings
  and runtime/ctox-secrets.sqlite3 encrypted credentials; copying only
  runtime_env_kv caused pre-model auth failure

positive evidence:
- worker reached MiniMax through provider mode ctox_core_api
- worker inspected existing Business OS modules before implementation
- worker wrote under the installed-module target directory
- final project validator reported ok=true for inventory after agent repairs

production blockers:
- module.json was briefly invalid JSON and visible to native module catalog sync
- collections.schema.json was initially missing schema_format
- worker created root-level _test_guard.txt while probing guard behavior
- worker temporarily added module-local package.json with "type": "module" to
  make tests pass
- final index.js still used localStorage for UI preference state
- comments assumed a shell right rail instead of a verified Business OS module
  pattern

decision:
- stop after first worker; the new finding is a validator/guard false-green,
  not a domain-specific app issue

hardening required:
- validator/static checker must reject localStorage/sessionStorage
- validator/static checker must reject root probe files
- exec guard must reject module-local package-manager side effects
- skill must require atomic JSON writes and explicitly forbid package.json as a
  test workaround

hardening verification:
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.mjs inventory --installed --workspace /tmp/ctox-bos-native-r14-20260613-174826 --json
  now fails for _test_guard.txt and localStorage
- CARGO_BUILD_JOBS=1 cargo test -q --manifest-path src/core/harness/core/Cargo.toml business_os_ -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox

next action:
- verify R14 hardening with targeted tests and rerun native R15 from a clean
  isolated root through ctox.business_os.app.create
```

### Native R15 2026-06-13

```text
status: valid CTOX-native first-worker failure
entry path: ctox.business_os.app.create
model: MiniMax M3
context: 256k
isolated root: /tmp/ctox-bos-native-r15-20260613-185315
commands dispatched: subscriptions, inventory, projects, contracts, compliance

positive evidence:
- commands entered through real Business OS app create dispatch, not generic
  queue add --skill
- queue tasks carried suggested_skill=business-os-app-module-development and
  the installed-module target block
- worker reached MiniMax M3 through ctox_core_api
- worker inspected existing Business OS modules and the bundled static checker
  before implementation

production blockers:
- MiniMax still used the legacy shell path because its adapter reports
  unified_exec_enabled() == false
- the Business OS root-artifact write guard protected unified_exec only, so the
  native MiniMax worker bypassed it through shell / local shell commands
- worker created root-level module.json and test-file.json probes
- worker attempted root-level symlink/hardlink aliasing for module.json
- worker temporarily overwrote the installed inventory module.json with invalid
  test content, causing the native module catalog to observe parse errors
- final validation remained red: root-level module.json, missing required
  module files, and no successful node syntax/test proof

decision:
- stop after the first worker; the finding is a shared runtime enforcement gap
  for legacy shell execution, not an inventory-app issue

hardening completed:
- expose the Business OS app root-artifact write guard for shared tool use
- apply the guard to ShellHandler Function, LocalShell, and shell_command paths
- detect quoted redirects and quoted tee targets such as > "$MODROOT/module.json"
- block root-level symlink/hardlink/copy/remove/probe attempts while allowing
  real installed-module directory writes
- extend validator and static checker root-artifact detection to test-*,
  *_test.*, *-test.*, _test_*, and _probe_* names
- update the skill hard-stop list to forbid root aliases, hardlinks, symlinks,
  and probe files instead of "testing" the guard

hardening verification:
- rustfmt --edition 2024 src/core/harness/core/src/tools/handlers/unified_exec.rs src/core/harness/core/src/tools/handlers/unified_exec_tests.rs src/core/harness/core/src/tools/handlers/shell.rs src/core/harness/core/src/tools/handlers/shell_tests.rs
- CARGO_BUILD_JOBS=1 cargo test -q --manifest-path src/core/harness/core/Cargo.toml business_os_guard -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --manifest-path src/core/harness/core/Cargo.toml shell_command_handler -- --nocapture
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox

next action:
- run native R16 from a fresh isolated root through ctox.business_os.app.create
  and prove the guard fires inside MiniMax legacy shell execution before any app
  can be accepted
```

### Native R16 2026-06-13

```text
status: valid CTOX-native first-worker failure
entry path: ctox.business_os.app.create
model: MiniMax M3
context: 256k
isolated root: /tmp/ctox-bos-native-r16-20260613-201357
commands dispatched: subscriptions, inventory, projects, contracts, compliance
first worker: projects

positive evidence:
- worker reached MiniMax M3 through ctox_core_api with 262144 context tokens
- worker inspected existing Business OS modules and app architecture before
  implementation
- worker initially used the provided installed-module target directory instead
  of writing root-level module.json or root-level app aliases
- manual mid-run validation caught the generated app as red instead of accepting
  the worker self-report

production blockers:
- module.json used layout.right without a justified persistent third pane
- generated app comments/tests contained forbidden package-manager/bundler
  literals copied from validation context
- generated module test failed because expected money formatting did not match
  the implemented output
- worker probed the tool environment (`type cat`, shell alias checks, temporary
  root writes) instead of simply implementing inside MODULE_DIR
- the MiniMax/Responses turn aborted with `invalid function arguments json
  string` after app artifacts existed
- the service marked the app queue task failed and leased the next pending app
  because app validation feedback only ran on successful worker turns

decision:
- stop after the first systemic failure; the finding is runtime control-flow,
  not a projects-domain app issue

hardening completed:
- run Business OS app artifact validation after worker/model/tool-call errors
  for app tasks when leased app queue work exists
- if red artifacts exist and repair attempts remain, write deterministic
  validator feedback back onto the same queue task, record validator_rework, and
  ack the task as review_rework
- if app validation repair attempts are exhausted after a worker error, fail the
  app task with an app-validation-specific reason instead of a generic prompt
  failure
- preserve app-validation rework as the owner of last_error/event text instead
  of overwriting it with the generic provider/tool-call failure
- extend the target prompt and skill to forbid guard probing, forbidden
  generated-file dependency literals, and huge one-shot file writes

hardening verification:
- rustfmt --edition 2024 src/core/service/service.rs src/core/business_os/store.rs
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture

next action:
- run app_modify_queue_prompt_targets_app_module_not_skill_files, rebuild ctox,
  push the checkpoint to main, then run native R17 from a fresh isolated root
  through ctox.business_os.app.create
```

### Native R17 2026-06-13

```text
status: valid CTOX-native first-worker failure
entry path: ctox.business_os.app.create
model: MiniMax M3
context: 256k
isolated root: /tmp/ctox-bos-native-r17-20260613-221048
commands dispatched: subscriptions, inventory, projects, contracts, compliance
first worker: subscriptions

positive evidence:
- commands entered through real Business OS app create dispatch
- worker reached MiniMax M3 through ctox_core_api with 262144 context tokens
- the R16 hardening fired: red app artifacts after a worker/session error were
  converted into same-task app-validator repair feedback
- the repair turn produced installed-module files under the allowed
  subscriptions module directory
- manual validator run after the failure reported ok=true and 8/8 module tests
  passing for subscriptions
- forensic root scan found no root-level app artifact leakage for the final app

production blocker:
- after green app validation, the worker/session errored again
- the generic worker-error path marked the queue task failed even though the
  deterministic Business OS app validator was green
- the remaining four app tasks stayed pending because the round was stopped
  after the first systemic orchestration failure

decision:
- stop after the first worker; the finding is runtime command/queue completion
  control flow, not a subscriptions-domain app issue

hardening completed:
- when an app task hits a worker/model/tool-call error and deterministic app
  validation returns green, complete the mapped Business OS app command with
  validator result metadata
- mark the queue task handled instead of failed
- keep a fallback handled path for synthetic app queue tasks that have no
  Business OS command mapping
- add unit coverage that green validation after worker error sets
  business_commands.status=completed and task_status=completed

verification after hardening:
- rustfmt --edition 2024 src/core/service/service.rs src/core/business_os/store.rs
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox
- git diff --check -- src/core/service/service.rs src/core/business_os/store.rs docs/business-os-app-creator-production-readiness-plan.md

next action:
- push the R17 control-flow hardening to main, then run native R18 from a fresh
  isolated root through ctox.business_os.app.create
```

### Validator Hook 2026-06-13

```text
status: partial implementation
added: src/apps/business-os/scripts/validate-app-module.mjs
added: src/apps/business-os/scripts/validate-app-module.test.mjs
added: service worker hook that validates app module artifacts after successful worker turns
added: app.create routing recognition for business-os-app-module-development

verification:
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.mjs contracts --installed --workspace /tmp/ctox-bos-native-install-r8-20260613-121551
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_module_target_parses_installed_prompt_contract -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation_feedback_is_repair_oriented -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture

remaining before R9:
- rebuild ctox with the validator and `validator_rework` proof path
- run an App Creator/App Store UI flow check for validation-fail and success projections
- rerun CTOX-native R9 bench
```

### Validator Hook Hardening 2026-06-13

```text
status: implementation gates green
added: source-mode validator fixture with registry-positive and registry-negative coverage
added: bounded app validation repair-attempt accounting
added: same-queue app validation rework test
added: validator_rework core-state proof before review_rework ack

verification:
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node --check src/apps/business-os/scripts/validate-app-module.test.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_module_target_parses_installed_prompt_contract -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture

important finding:
- Direct review_rework ack is blocked unless a ReviewRequired witness exists.
  The app validator now records a dedicated validator_rework proof before the
  queue task is acknowledged as review_rework.

remaining before production-ready:
- CTOX-native R9 bench with MiniMax M3 and 256k context
- second CTOX-native 5/5 green bench
- live shell smoke for generated installed apps
- release-like install validation without developer-local paths
```

### Native R9/R10 2026-06-13

```text
status: failed, but useful

R9 setup:
- isolated root: /tmp/ctox-bos-native-r9-20260613-141229
- CTOX root validation required normal release markers, not only Business OS source files
- MiniMax tasks failed before model execution because the isolated state root had
  no CTOX SQLite runtime secret store
- after copying only the local secret store into the isolated state root,
  MiniMax M3 execution reached the provider through ctox_core_api

R10 runtime evidence:
- MiniMax M3 ran with 262144 context tokens
- prompts carried suggested_skill=business-os-app-module-development
- prompts targeted runtime-installed-module under src/apps/business-os/installed-modules/<module_id>
- subscriptions created a module directory but failed installed-module validation
- inventory wrote an invalid module.json and then stopped before required files existed

R10 recurring failures:
- missing index.css and other minimum module files
- module.json.entry used source-mode modules/<id>/index.html instead of
  installed-modules/<id>/index.html
- module.json.install_scope used store instead of installed
- collections.schema.json did not use the required schema format
- schema ownership was ambiguous for shell collections
- default layout.right / right-pane UI appeared without justification
- index.js referenced forbidden remote URLs or non-Business-OS data-plane patterns
- tests imported or mentioned esbuild
- repair workflow left .bak files
- the worker treated custom tests as success while static validation was red
- the worker hallucinated root-level blocker/status artifacts despite the target block

Runtime finding:
- generic completion review caught invalid app output before the deterministic
  Business OS app validator could inject its specific repair prompt

Changes made from R10 evidence:
- command prompt now forbids root-level blocker/status notes and stale
  artifact-contract examples
- command prompt requires first-file creation of index.css and icon.svg
- command prompt spells out installed manifest entry/install_scope
- command prompt forbids npx, esbuild/Vite/Rollup/Webpack proof, bundler
  imports in tests, and leftover .bak/.orig/.rej/.tmp/bundle/probe files
- skill and references now explain the Business OS translation layer more
  explicitly
- module_static_check now fails on generated .bundle.mjs, .bundle.css, .bak,
  .orig, and .rej files
- service app validation now runs for app tasks even when generic completion
  review returns FeedbackRetry/Hold; generic feedback persistence is skipped
  when app validation owns the rework turn

verification after hardening:
- node --check src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs
- node --check src/apps/business-os/scripts/validate-app-module.mjs
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox

next action:
- run native R11 from a fresh isolated root using the rebuilt ctox binary and
  the hardened prompt/validator hook
```

### Native R11/R12 2026-06-13

```text
status: failed, but validator ownership and repair routing improved

R11 recurring evidence:
- prompt text still did not stop root-level app artifacts
- the generated app again created workspace-root module.json and
  collections.schema.json
- default right/third pane patterns remained attractive to the model
- shell-owned schema and module-owned schema boundaries still required hard
  runtime enforcement
- generic completion review was too weak and too late compared with the
  deterministic app validator

R12 positive evidence:
- the pre-review Business OS app validator hook ran before command completion
- subscriptions was not marked complete while validation was red
- validator feedback was written back onto the same queue task
- the service recorded a validator_rework proof
- task state moved to review_rework instead of silently completing

R12 production blockers:
- the old exec write guard only caught simple shell redirection and tee writes
- python/pathlib writes to workspace-root module.json and collections.schema.json
  bypassed that guard
- after validator rework was queued, the dispatcher leased a fresh pending app
  task before returning to the validator rework task

changes made from R12 evidence:
- app validation repair prompts preserve parseable target metadata:
  module_id, install_target, and only_allowed_app_artifact_directory
- service dispatch now promotes Business OS app validation review_rework through
  legal ReworkRequired -> Pending -> Executing transition proof before leasing
  new app work
- exec now snapshots workspace-root app artifacts before each command and removes
  newly-created root-level module.json or collections.schema.json after command
  execution
- exec now reports such cleanup as a tool error, forcing the agent to repair in
  the allowed module directory
- guard tests cover shell redirection, python/pathlib writes, installed-module
  allowed writes, and cleanup of new root-level app artifacts

verification after hardening:
- node src/apps/business-os/scripts/validate-app-module.test.mjs
- CARGO_BUILD_JOBS=1 cargo test -q --manifest-path src/core/harness/core/Cargo.toml business_os_ -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox business_os_app_validation -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_create_queue_prompt_targets_app_module_skill -- --nocapture
- CARGO_BUILD_JOBS=1 cargo test -q --bin ctox app_modify_queue_prompt_targets_app_module_not_skill_files -- --nocapture
- CARGO_BUILD_JOBS=1 cargo build -q --bin ctox

next action:
- run native R14 from a fresh isolated root and require 5/5 app-validator green
  before moving from Phase 6 to Phase 8
```
