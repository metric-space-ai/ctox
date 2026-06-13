# Business OS Skill Bench 2026-06-12

Purpose: test whether a mid-level coding agent using MiniMax M3 can one-shot a
running CTOX Business OS app module when given the
`business-os-app-module-development` skill. Prompts are intentionally simple to
avoid overfitting the skill to a single run.

## Shared Bench Rules

- Model: `minimax/MiniMax-M3`
- Agent: MiniMax Code `coder`
- Each run uses an isolated workspace copy.
- Agent prompt points to the skill id and notes the source-checkout path only
  because this bench runs inside a repository copy. Release installs must use
  the embedded system skill (`ctox skills system show/export`) or a versioned
  GitHub fallback, not a developer-local source path.
- Success is not feature completeness. Success means the app is structurally
  correct for CTOX Business OS, mounts in the real shell, uses the right data
  plane, persists correctly, and has no implementation errors around CTOX DB,
  dynamic collections, or command/projection flows.
- Every app must include one small automation component that triggers normal
  CTOX work, chat, or ticket flow.

## Prompt 1: Subscriptions

```text
You are in the CTOX repository. Build a new CTOX Business OS app module for Subscription / Recurring Management.

Use the Business OS app module development skill (`business-os-app-module-development`; in this source checkout it is at src/skills/system/product_engineering/business-os-app-module-development/SKILL.md). Keep the app small but actually runnable in Business OS. It should manage subscription contracts, MRR, renewal dates, churn risk, and include one automation action that creates or dispatches a normal CTOX ticket/chat/work item for a renewal or churn-risk follow-up.

Implement under src/apps/business-os/modules/subscriptions. Do not build a Next.js/React/API app. Verify the module with the relevant Business OS checks and report exactly what you ran and what remains unproven.
```

## Prompt 2: Inventory

```text
You are in the CTOX repository. Build a new CTOX Business OS app module for Lager / Inventory.

Use the Business OS app module development skill (`business-os-app-module-development`; in this source checkout it is at src/skills/system/product_engineering/business-os-app-module-development/SKILL.md). Keep the app small but actually runnable in Business OS. It should manage inventory items, stock locations, minimum stock, and pick/reorder status, and include one automation action that creates or dispatches a normal CTOX ticket/chat/work item when stock is below the minimum.

Implement under src/apps/business-os/modules/inventory. Do not build a Next.js/React/API app. Verify the module with the relevant Business OS checks and report exactly what you ran and what remains unproven.
```

## Prompt 3: Projects

```text
You are in the CTOX repository. Build a new CTOX Business OS app module for Projekte / Aufträge.

Use the Business OS app module development skill (`business-os-app-module-development`; in this source checkout it is at src/skills/system/product_engineering/business-os-app-module-development/SKILL.md). Keep the app small but actually runnable in Business OS. It should manage customer projects, time-material vs fixed price, milestones, budget vs actual, and include one automation action that creates or dispatches a normal CTOX ticket/chat/work item when a project is over budget or ready for invoicing.

Implement under src/apps/business-os/modules/projects. Do not build a Next.js/React/API app. Verify the module with the relevant Business OS checks and report exactly what you ran and what remains unproven.
```

## Prompt 4: Contracts

```text
You are in the CTOX repository. Build a new CTOX Business OS app module for Verträge.

Use the Business OS app module development skill (`business-os-app-module-development`; in this source checkout it is at src/skills/system/product_engineering/business-os-app-module-development/SKILL.md). Keep the app small but actually runnable in Business OS. It should manage customer contracts, SLAs, renewal dates, cancellation periods, and linked customers/opportunities, and include one automation action that creates or dispatches a normal CTOX ticket/chat/work item for an upcoming renewal or cancellation deadline.

Implement under src/apps/business-os/modules/contracts. Do not build a Next.js/React/API app. Verify the module with the relevant Business OS checks and report exactly what you ran and what remains unproven.
```

## Prompt 5: Quality

```text
You are in the CTOX repository. Build a new CTOX Business OS app module for Quality / Compliance.

Use the Business OS app module development skill (`business-os-app-module-development`; in this source checkout it is at src/skills/system/product_engineering/business-os-app-module-development/SKILL.md). Keep the app small but actually runnable in Business OS. It should manage complaints, audit findings, corrective actions, owners, due dates, and evidence status, and include one automation action that creates or dispatches a normal CTOX ticket/chat/work item when a corrective action is overdue or high severity.

Implement under src/apps/business-os/modules/quality. Do not build a Next.js/React/API app. Verify the module with the relevant Business OS checks and report exactly what you ran and what remains unproven.
```

## Forensic Checklist

For each run, collect:

```text
session id
workspace path
final agent message
files changed
whether the agent read the skill and architecture translation reference
whether it inspected at least three existing apps
whether it created a phase plan and porting/architecture map
module file contract completeness
collections.schema.json/module.json consistency
forbidden patterns: ctx.db.raw, ctx.collections, ctox.db, HTTP data APIs, localStorage/sessionStorage/indexedDB, rxdb/node imports
automation path: real command/projection or fake button
tests the agent ran
tests we ran independently
real-shell mount proof or reason absent
major Business OS architecture mistakes
skill gaps indicated by majority failure pattern
```

## Round 1 Findings

Workspaces: `/tmp/ctox-bos-skill-bench-20260612-093155/<app>`

Sessions:

```text
subscriptions mvs_50ed191ed8254d0ca181020f619d0f0e
inventory     mvs_2870d87f963347a4a588cd8c8c2395e5
projects      mvs_d08c3717601e4b8d989b8be34d78fc2d
contracts     mvs_7f049e6192db4bc5b95c6cda325f406f
quality       mvs_501e7b97614f4ca1a3beec1cb804d0be
```

Round result: failed. The agents read substantial context but did not converge
to five one-shot runnable modules within the round window.

Evidence:

```text
subscriptions: required module files existed, but no editable plan file.
inventory: no index.js at timeout and module.json was invalid JSON due hand-written inline SVG.
projects: required module files existed, but no editable plan file and local npm/.opencode package artifacts were created for esbuild.
contracts: no index.js at timeout and no editable plan file.
quality: no index.js at timeout, only partial module files, plan existed.
```

## Round 2 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r2-095159`

Sessions:

```text
subscriptions mvs_6dabe52af2be4b199980ad3a7b779413
inventory     mvs_85134bda23ad48f3bf7b1e86dd27942f
projects      mvs_8b7c700956134b49a202753d14d64617
contracts     mvs_1d50889a0e3b49b8bc45627da9fe52c7
quality       mvs_d5fb8cb282444bc7b8f1e6f2a66abf17
```

Round result: failed, but substantially better than Round 1.

Evidence:

```text
all five runs created editable phase plans.
all five runs produced the required module files and valid module/collection JSON.
module conformance passed in all five workspaces.
subscriptions, projects, contracts: own unit/static tests were green by the end.
inventory: tests became green, but only after debugging a custom fake DB fixture; the test fixture still asserted `db.raw`, which is not a pattern new agents should learn.
quality: remained red when the round was stopped; the test imported esbuild, read the registry through a wrong path earlier, and still had failing filter/export assertions.
most source modules did not update `src/apps/business-os/modules/registry.json`, so checked-in store modules could pass file conformance while remaining undiscoverable in the App Store/catalog.
```

Skill changes taken from Round 2:

```text
clarified release installs use the embedded system skill and `ctox skills system show/export`; source paths are dev-only.
added source-vs-runtime target decision: `src/apps/...` versus `installed-modules/...` through `ctox.module.save` / `ctox.source.save`.
required source `install_scope: store` modules to update and verify `src/apps/business-os/modules/registry.json`.
forbid packaged registry edits for runtime-installed App Creator/App Store modules unless the catalog itself is the target.
prefer one browser-safe `.mjs` helper imported by both production code and tests.
forbid duplicate `.js`/`.mjs` helper logic, new bundler imports for no-build module tests, and complex fake RxDB fixtures that assert `db.raw`.
```

## Round 3 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r3-101819`

Sessions:

```text
subscriptions mvs_6e47b98daa9842a4b12c548976e91509
inventory     mvs_375a86c0b8624d29b51cb941bf4d531d
projects      mvs_a1b5581c3c4f461baaed1f0fb4315135
contracts     mvs_852142dd51dd4923baeccb20eb9ea3d1
quality       mvs_208a0e2b1f1a47c1bb99e5841216db13
```

Round result: failed, but close. Four apps had green local unit tests and
module conformance; two apps still violated Business OS-specific schema rules.

Independent evidence:

```text
subscriptions: registry OK, 15/15 tests pass, conformance OK, no forbidden runtime patterns.
inventory: registry OK, 15/15 tests pass, conformance OK, no forbidden runtime patterns.
projects: 14/14 tests pass and conformance OK, but collections.schema.json redeclared business_commands. This is architecturally wrong even when the current conformance script tolerates it with migrations.
contracts: failed. collections.schema.json redeclared business_commands, conformance failed on business_commands migration, 18/21 tests pass, and the agent stopped at a verification approval questionnaire instead of running allowed local checks.
quality: registry OK, 44/44 tests pass, conformance OK, no forbidden runtime patterns.
```

Skill changes taken from Round 3:

```text
external-agent prompts for release/runtime work must include the embedded skill id, ctox skills system show/export, or a versioned GitHub skill URL; a developer-local /Users/... source path alone is now a hard stop.
normal module collections.schema.json must declare only module-owned collections; business_commands and other shell/core dependency collections are dependencies in module.json, not schemas to redeclare.
verification now includes an explicit Node guard that fails if collections.schema.json contains business_commands, ctox_queue_tasks, desktop_files, desktop_file_chunks, business_module_catalog, or business_module_source_files.
the default prompt now tells agents not to stop at local verification approval questionnaires when read/test commands are already allowed.
```

Assessment:

```text
The remaining failures are not random model failures. They are agent-trap failures caused by CTOX-specific architecture that common web-app training data does not encode: core dependency collections are not module schemas, release installs do not have source paths, and local verification gates should run without extra ceremony in an already-permitted execution environment.
```

## Round 4 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r4-104939`

Sessions:

```text
subscriptions mvs_fcab0cb03046463eac0ba7d1354a8ded
inventory     mvs_37021fcec82c470d80a95df4b6a661ec
projects      mvs_b645b9adb80e4ca981c96e7c331492b3
contracts     mvs_172aab82c74a47ce8c651760e02b642b
quality       mvs_8102cbe4e3894ca7a71d85b1c42fc353
```

Round result: failed, with four materially green modules and three remaining
skill gaps.

Independent evidence:

```text
subscriptions: JSON/registry/module-owned-only OK, 19/19 tests pass, conformance OK, rxdb-only OK, no forbidden runtime patterns. Failed process behavior: the agent stopped at a questionnaire for real-shell proof that would mutate ~/.local/lib/ctox/current instead of marking proof blocked.
inventory: JSON/registry/module-owned-only OK, 21/21 tests pass, conformance OK, rxdb-only OK, no forbidden runtime patterns. The agent initially wrote a wrong test path from tests/module.json, then repaired it.
projects: JSON/registry/module-owned-only OK, 19/19 tests pass, conformance OK, rxdb-only OK, no forbidden runtime patterns. UX concern: permanent right pane for budget/automation KPIs without strong proof that center/detail/drawer would be insufficient.
contracts: JSON/registry/module-owned-only OK, 19/19 tests pass, conformance OK, rxdb-only OK, no forbidden runtime patterns. UX concern: permanent context/right pane is better justified than projects, but still reflects third-pane drift.
quality: JSON/registry/module-owned-only OK, 24/24 tests pass, conformance OK, no forbidden runtime patterns in runtime files. Failed rxdb-only because tests/quality.test.mjs contained the literal `/api/business-os` in a negative assertion; assert-rxdb-only scans module tests too.
```

Skill changes taken from Round 4:

```text
do not ask approval for real-shell proof that would require mutating ~/.local/lib/ctox/current or another installed release; mark browser proof blocked instead.
assert-rxdb-only scans module tests and docs, so new module tests/README must not contain broad forbidden literals such as /api/business-os, /rxdb/pull, /commands, local-only, FallbackDatabase, or upstream rxdb import examples.
tests under tests/ must resolve moduleRoot explicitly from import.meta.url; do not accidentally read tests/module.json.
phase trackers must use the exact required header: Phase | Status | Owner/Agent | Started | Finished | Touched files | Gate/Evidence | Blocker | Next action | Notes.
small greenfield apps must not add a permanent right/third pane for KPIs, generic automation status, or summary cards unless the plan includes a right-pane proof naming the live object and why center/modal/drawer is insufficient.
```

Assessment:

```text
Round 4 shows the core Business OS architecture rules are mostly learnable from the skill: no core collection redeclarations, no fake command types, no npm/esbuild install, registry discovery, and module-owned schemas all held. Remaining issues are validation-surface literacy and UI discipline, not the CTOX DB persistence model.
```

## Round 5 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r5-112648`

Sessions:

```text
subscriptions mvs_31b7f02a1cb4430997f0bf5ab2a1892a
inventory     mvs_43a7be46c7564b1087017492ac1419c5
projects      mvs_daaef582f59f4a3798790f3743892c8a
contracts     mvs_cd9667ea426a4a0890f190aecb6c6ea2
quality       mvs_3953f1632e364813a1c71c8bbb0781ae
```

Round result: failed. All five apps were much more complete, but the
independent gates exposed legacy-pattern copying that the skill did not yet
forbid explicitly enough.

Independent evidence:

```text
subscriptions: JSON/registry/module-owned-only OK, 8/8 tests pass, conformance OK, rxdb-only OK, no runtime forbidden patterns. Failed documentation gate: implementation plan still cites esbuild and a db.raw few-shot detail as a copied test pattern.
inventory: JSON/registry/module-owned-only OK, test passes, conformance OK, rxdb-only OK. Failed cleanup/documentation gates: .DS_Store in module tree, runtime source comment contains forbidden API names, plan still cites esbuild legacy tests.
projects: JSON/registry/module-owned-only OK, 23/23 tests pass, conformance OK, rxdb-only OK. Failed architecture gates: test imports esbuild, README/plan recommend bundle checks, and greenfield automation manually upserts pending_sync into business_commands when commandBus is unavailable.
contracts: JSON/registry/module-owned-only OK, 20/20 tests pass, conformance OK, rxdb-only OK. Failed documentation gate: README recommends esbuild bundle verification. Its plan also records legacy command fallback patterns that should be rejected for greenfield.
quality: tests pass, conformance OK, rxdb-only OK, but independent owned-only guard fails: collections.schema.json redeclares business_commands. The test incorrectly allowlists business_commands based on old module patterns and imports esbuild.
```

Skill changes taken from Round 5:

```text
libraries in Business OS apps may only be local browser-compatible ESM modules: existing shell/repo ESM imports or vendored ESM source imported by relative path.
no dependency management for app code: no package.json/package-lock/node_modules, no npm/yarn/pnpm/bun, no CDN runtime imports, no CommonJS, no bare package imports, and no app build pipeline.
existing esbuild/fake-DOM module tests are legacy few-shots to reject for new greenfield apps; new tests should import pure browser-safe .mjs helpers directly.
README, plan, tests, and completion notes must not recommend esbuild/Vite/Webpack/Rollup/npm build as normal verification.
new module tests must fail if collections.schema.json contains business_commands or another shell/core dependency; do not allowlist business_commands.
greenfield automations must dispatch through ctx.commandBus.dispatch; if commandBus is unavailable, report/disable the action instead of manually inserting pending_sync into business_commands.
module trees must not contain .DS_Store, Thumbs.db, probe/temp files, or generated bundles.
source comments should not preserve forbidden API names such as ctx.collections, ctx.db.raw, ctox.db, or HTTP Business OS endpoints.
```

## Round 6 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r6-121254`

Sessions:

```text
subscriptions mvs_94ad604161d54a8f9676146dffc3f798
inventory     mvs_2e65f5182d414b719e7f941ae1c18ac6
projects      mvs_38668a898c784f45b39ae1f58c57e047
contracts     mvs_c2a1c8dcc2904f558f38f9b3a7363bcc
quality       mvs_4d8c317a03a643f79a93a4bac597938c
```

Round result: failed. R6 showed that the No-Bundler/ESM rule was learnable,
but exposed a more precise schema-contract gap.

Evidence:

```text
quality: recognized the legacy esbuild test pattern and used pure ESM helpers.
projects: declared business_commands only in module.json and module-owned collections in collections.schema.json.
inventory/projects/quality: several agents hit their own tool permission layer and stopped with permission-ask instead of marking verification blocked.
contracts: explicitly treated collections.schema.json as optional/aspirational because the agent overread legacy guard behavior, then skipped/undervalued the native schema contract and did not produce a complete module.
```

Skill changes taken from Round 6:

```text
the schema rule is no longer phrased as "module-owned only"; it now matches assert-module-conformance:
  - do not redeclare shell-registered collections: business_module_catalog, ctox_runtime_settings, business_commands, ctox_queue_tasks
  - every other collection in module.json must be declared in collections.schema.json
  - peer-module dependencies such as customer_accounts must use exact schema parity with the owning module or be deferred
agents may not treat incomplete guards or legacy modules as permission to ignore the skill; collections.schema.json is the native runtime contract and schema.js is only the browser facade.
```

## Round 7 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r7-123352`

Sessions:

```text
subscriptions mvs_60141ab97b164eee8d9dc13715a8efa8
inventory     mvs_68889333e46d44799cef4d1946e7ad84
projects      mvs_1d26345c05b44376b49c23a5297089b8
contracts     mvs_d99a0a1a346a4ccdac0a49e0de0d2083
quality       mvs_254820993cc1476a9d5fd61430f5d0d8
```

Round result: invalid/fail as a bench run.

Evidence:

```text
the workspace was created from git archive HEAD plus skill overlay, so it missed many current working-tree Business OS schema files; agents saw old modules without collections.schema.json.
the subscriptions agent wrote outside the isolated /tmp workspace into the real /Users/... checkout. The generated untracked subscriptions files were removed; unrelated existing registry changes were left untouched.
projects and contracts showed improvement: no esbuild-copying, contracts wrote collections.schema.json and tests instead of skipping the native schema contract.
inventory and quality reached local test/scan phases but were aborted after stale started status/no new messages.
```

Skill changes taken from Round 7:

```text
source-checkout agents must confirm the current workspace root before writing and must not switch to another local checkout or symlinked maintainer path.
```

Repeated failure patterns:

```text
agents searched wrong skill locations before using src/skills/...
free few-shot search drifted into large modules and three-pane references
editable phase tracker was skipped or created late
phase tracker/checklists marked future files/proofs done before evidence existed
inline layout.icon_svg in module.json caused invalid JSON
agents tried to install or create npm dependencies to imitate reference tests
automations invented module-specific command types without native handlers, or overclaimed ticket creation
first runnable slice was delayed by optional core/commands/views/test structure
```

Skill updates after Round 1:

```text
require exact src/skills/... path first
require docs/business-os-<module>-implementation-plan.md as first write
add curated few-shot set and reject copying large modules wholesale
forbid npm installs/package artifacts for no-build module tests
prefer icon.svg over hand-written layout.icon_svg and require JSON.parse after manifest edits
require existing business_os.chat.task for generic CTOX follow-up automations unless native handler is added
require first runnable slice before optional helper folders
```

## Round 8 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r8-130708`

Sessions:

```text
subscriptions mvs_291ac5be03b2412f9af9a69b5b7201fa
inventory     mvs_caac76dad21544038ccb239e14463386
projects      mvs_0167280e60aa4cdcb1cfa3cfde81112f
contracts     mvs_edd48abfeb2543f9bdf2f65e9a2b792b
quality       mvs_cd85be040e994ad7b6c9a312cb97d1c1
```

Round result: failed. The agents now reliably created complete module file
sets, used `collections.schema.json`, avoided source-checkout pollution, used
local ESM helpers instead of bundling, and passed conformance in 5/5 apps. The
remaining failures were concentrated in test/documentation hygiene, local
command-state modeling, and one real helper/test mismatch.

Independent evidence:

```text
subscriptions: JSON/registry/schema OK, 25/25 tests pass, conformance OK, runtime forbidden scan clean. Failed dependency/documentation scan because tests and README contain exact negative-proof strings such as esbuild and pending_sync.
inventory: JSON/registry/schema OK, 23/23 tests pass, conformance OK, runtime forbidden scan clean. Failed hygiene scans because comments/tests contain exact dependency terms and commands/reorder.mjs creates pending_sync local fallback/status labels.
projects: JSON/registry/schema OK, conformance OK, runtime forbidden scan clean, but 4/15 tests fail. It also models pending_sync as a module status, includes right-pane/third-pane prose in README/CSS comments, and embeds dependency/command-fallback guard strings in README/tests.
contracts: JSON/registry/schema OK, 17/17 tests pass, conformance OK, all independent hygiene scans clean. This is the first R8 app that met the static/module gates except the broad rxdb-only workspace issue below.
quality: JSON/registry/schema OK, 16/16 tests pass, conformance OK, but README/CSS/tests embed forbidden data-plane/dependency strings as negative examples. CSS comment includes a remote URL literal; README includes broad RxDB-only literals and right-pane prose.
```

Workspace caveat:

```text
node src/apps/business-os/scripts/assert-rxdb-only.mjs failed in every R8 workspace because the bench snapshot missed src/core/rxdb/tools/business_os_connection_modes_smoke.js.
The file exists in the real checkout, so this is a snapshot creation defect, not an app-specific RxDB-only finding.
Next rounds must create workspaces with a robust copy/rsync-like path that preserves untracked/current source files required by guards.
```

Skill changes taken from Round 8:

```text
new module files may not include exact dependency/build artifact words as negative proof in README, comments, test names, assertion strings, or test literals; build regexes from fragments or keep that scanner outside the module tree.
new module files may not include exact forbidden data-plane strings in sample grep commands or README "does not use X" prose.
greenfield modules may not use pending_sync as a local status enum, CSS class, README explanation, test literal, or fallback commandBus result; module UI should use neutral labels such as submitted, queued, unavailable, or failed while commandBus/native code owns raw command states.
verification guidance now makes these negative-proof strings explicit because multiple agents interpreted "no forbidden runtime pattern" as permission to write forbidden strings in tests/docs.
```

## Round 9 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r9-134027`

Sessions:

```text
subscriptions mvs_6b40d1fcde8b4f67898e1cbc464e2f13
inventory     mvs_9296a6bf74bb4f93945c7c5306ea16c3
projects      mvs_359c74a387324e3f87e7079ded7abfad
contracts     mvs_e55bba00478048e99a8f2cdfa47d106b
quality       mvs_be36f3626ce94bbe9903bc399a6bc0c1
```

Round result: failed, but the failures narrowed further. Core Business OS data
plane rules were mostly learned: schema coverage, RxDB/WebRTC-only, no HTTP
record APIs, and no dependency-managed runtime imports were green in most
complete apps. Remaining failures were largely completion hygiene and
negative-proof leakage.

Independent evidence:

```text
subscriptions: complete files, registry/schema OK, 19/19 tests pass, conformance OK, rxdb-only OK, runtime/broad data-plane scans clean. Failed because tests/plan still contained exact dependency-build words, README/index/test/plan still contained pending_sync or manual command fallback hints, and documentation still discussed right-pane rejection in a way the broad UI scan flagged.
inventory: complete files but test was placed at module root instead of tests/, registry entry was missing, module test failed because index.js contained a db.raw literal, conformance failed on db.raw, and plan/index/test still leaked dependency and pending_sync/fallback wording.
projects: incomplete at abort: no tests and no registry entry. Conformance, rxdb-only, and runtime scans were otherwise clean. Plan/index still leaked dependency and pending_sync/fallback wording.
contracts: complete files, tests pass, conformance OK, rxdb-only OK, runtime/broad data-plane scans clean, but registry entry was missing. Test/plan still contained dependency wording and manual command fallback hints.
quality: complete files, registry/schema OK, 17/17 tests pass, conformance OK, rxdb-only OK, runtime/broad data-plane scans clean. Failed because tests/plan still contained dependency-build words and README/index/plan still contained pending_sync/manual command fallback wording.
```

Bench harness note:

```text
full rsync of the dirty checkout hit local IO timeouts, so R9 used a focused workspace copy of docs, src/apps/business-os, src/core/rxdb, src/core/business_os, src/core/service, and the relevant skills.
This preserved src/core/rxdb/tools/business_os_connection_modes_smoke.js, so the previous broad rxdb-only false failure did not recur.
```

Skill changes taken from Round 9:

```text
added a bundled source-checkout static checker at src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs.
the checker validates module files, registry visibility, schema coverage, required tests under tests/*.test.mjs, forbidden runtime imports, no package artifacts, no dependency-managed app libraries, no exact negative-proof strings, and no pending_sync leakage in module files or the phase plan.
agents must run this checker before claiming a source module is complete and must not rewrite the checker inside module tests or README files.
user clarification captured as a hard rule: Business OS app libraries may only be integrated as local browser-compatible ESM modules by relative import; there is no app dependency management.
```

## Round 10 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r10-140936`

Sessions:

```text
subscriptions mvs_27c418a366bf4c75bdd83aebc3647fff
inventory     mvs_4cfc1843e12440edbf87bf49ffde98af
projects      mvs_bf97d285f68c45a0942f1742b3637586
contracts     mvs_744acbee61374cbebb9e0e975dcc1046
quality       mvs_99f2e1ff078e406289a897efc77c09e9
```

Round result: failed under the hardened checker, but the old gates were green.
All five agents produced complete module trees and eventually passed module
tests, conformance, and `assert-rxdb-only`. The newly hardened checker found a
real portability gap in the contracts module.

Independent evidence:

```text
subscriptions: static checker OK, 12/12 tests pass, conformance OK, rxdb-only OK.
inventory: static checker OK, 23/23 tests pass, conformance OK, rxdb-only OK.
projects: static checker OK after agent fixed tests to construct forbidden regexes from fragments, 33/33 tests pass, conformance OK, rxdb-only OK.
contracts: old static checker OK, 1/1 tests pass, conformance OK, rxdb-only OK; hardened checker fails because schema.js imports collections.schema.json with a JSON import attribute.
quality: static checker OK, 11/11 tests pass, conformance OK, rxdb-only OK.
```

Forensic takeaways:

```text
the bundled static checker changed agent behavior: agents ran it and fixed registry, tests, negative-proof leakage, package-artifact wording, and pending_sync leakage without manual intervention.
projects showed the intended hardening loop: the agent initially wrote forbidden strings into tests, saw module_static_check fail, then rebuilt the regexes from fragments and passed.
contracts showed a missing checker rule: schema.js used `import ... from './collections.schema.json' with { type: 'json' }`. Existing source modules do not rely on that pattern, and Business OS module schema.js should remain browser-safe JS/ESM rather than a JSON-module wrapper.
the quick_validate.py skill validator could not run in this local Python environment because PyYAML is not installed; no dependency was installed for the bench.
```

Skill changes taken from Round 10:

```text
module_static_check.mjs now rejects `.json` imports in browser runtime files.
SKILL.md hard stops and build rules now state that schema.js must not import collections.schema.json or any `.json` module; it must mirror schemas as browser-safe JS/ESM objects or import a local `.mjs` helper.
module-contract.md and verification.md document the same rule.
the alias skill and OpenAI metadata now carry the ESM-only/no-dependency-management rule and the JSON-import prohibition.
```

## CTOX-Native R5 Findings

Workspace: `/tmp/ctox-bos-native-install-r5-20260613-071624`

Runtime:

```text
source: CTOX service queue via `ctox business-os commands dispatch`
model: MiniMax-M3
context: 256k / 262144 tokens
data plane: native Business OS RxDB/WebRTC peer up, required collections ready, http_bridge_available=false
commands: subscriptions, inventory, projects, contracts, quality
```

Result: failed early and intentionally stopped after forensic evidence from
the first worker (`contracts`).

Evidence:

```text
the embedded system skill `business-os-app-module-development` was visible through `ctox skills system list/show`.
the Business OS command prompt included the new skill and `context_window=262144`.
no skill-named directory or `src/skills` artifact was created in the isolated root.
MiniMax inspected the app root and found `installed-modules`, but then wrote app artifacts to the workspace root:
  /tmp/ctox-bos-native-install-r5-20260613-071624/module.json
  /tmp/ctox-bos-native-install-r5-20260613-071624/collections.schema.json
  /tmp/ctox-bos-native-install-r5-20260613-071624/contracts/
the generated manifest used `entry: "contracts/index.html"` instead of `installed-modules/contracts/index.html`.
the generated layout reintroduced a permanent right pane and right-column resizer despite the prompt saying third panes should be exceptional.
```

Assessment:

```text
This is not just model failure. The skill and command prompt were still too
implicit about the runtime-installed target directory. A mid-level agent can
infer "Business OS module" but still write app files at repo root when the
allowed directory is not named as an exact, non-negotiable path.
```

Skill/Runtime changes taken from CTOX-native R5:

```text
SKILL.md now makes wrong-path app writes a hard stop:
  - no root-level module.json
  - no root-level collections.schema.json
  - no root-level <id>/ directory
  - no src/skills or skill-named deliverable paths
runtime-installed modules must write only under src/apps/business-os/installed-modules/<id>/.
the CTOX Business OS command prompt now includes:
  only_allowed_app_artifact_directory: src/apps/business-os/installed-modules/<id>
  first file action: create that directory and write all app files inside it
  explicit root-level artifact ban
module_static_check.mjs now supports `--installed` and fails the exact R5 pattern.
the UI rule is hardened: no layout.right, right-column resizers, or three-column grids by default; third panes need explicit workflow justification.
```

Next CTOX-native round:

```text
rebuild CTOX after the current repo Cargo lock clears.
create a fresh isolated R6 root.
run the same five commands through CTOX service with MiniMax-M3 256k.
for each generated app, run module_static_check.mjs <id> --installed before deeper tests.
stop the round immediately if any app writes outside installed-modules/<id>/ or creates a default third pane.
```

## CTOX-Native R6 Findings

Workspace: `/tmp/ctox-bos-native-install-r6-20260613-095513`

Runtime:

```text
source: CTOX service queue via `ctox business-os commands dispatch`
model: MiniMax-M3
context: 256k / 262144 tokens
data plane: native Business OS RxDB/WebRTC peer up, required collections ready, http_bridge_available=false
commands: subscriptions, inventory, projects, contracts, quality
```

Result: failed early and intentionally stopped after the first worker
(`contracts`) reproduced the wrong-path family with a narrower cause.

Evidence:

```text
the embedded skill contained the R5 hardening, including only_allowed_app_artifact_directory and module_static_check.mjs --installed.
the first worker created the correct directory:
  src/apps/business-os/installed-modules/contracts/
but later shell heredocs wrote root-level artifacts:
  /tmp/ctox-bos-native-install-r6-20260613-095513/module.json
  /tmp/ctox-bos-native-install-r6-20260613-095513/collections.schema.json
  /tmp/ctox-bos-native-install-r6-20260613-095513/contracts/
the generated installed module lacked module.json and collections.schema.json in the installed target.
the manifest still declared layout.right without an explicit workflow justification.
schema.js exported the shell collection business_commands instead of only module-owned collections.
module_static_check.mjs contracts --installed failed on exactly those conditions.
```

Assessment:

```text
R6 was progress over R5 because the agent found and created the installed
module directory. The remaining failure is a shell-write translation gap: the
agent used the correct target conceptually but then redirected heredocs from
the install-root cwd to bare root-level paths. This is not a generic Web/DB
failure and not a skill lookup failure.
```

Skill/Runtime changes taken from CTOX-native R6:

```text
the Business OS command prompt now warns that shell tools run from the install root, not the module directory.
the prompt now requires a MODULE_DIR="src/apps/business-os/installed-modules/<id>" write pattern for every generated file.
the prompt explicitly forbids bare redirects such as > module.json, > collections.schema.json, > <id>/index.js, and mkdir <id>.
SKILL.md now contains the same MODULE_DIR write pattern.
SKILL.md now states schema.js and collections.schema.json must export only module-owned collections; shell collections may be listed in module.json but never exported by module schema files.
the app-target prompt regression test now asserts the cwd warning, MODULE_DIR pattern, and module-owned schema rule.
```

Next CTOX-native round:

```text
rebuild CTOX so the embedded skill and app-target prompt include the R6 fixes.
create a fresh isolated R7 root.
run the same five commands through CTOX service with MiniMax-M3 256k.
stop immediately if any worker uses a bare root-level heredoc redirect, creates root-level module artifacts, exports shell collections from schema.js, or declares layout.right by default.
```

## CTOX-Native R7 Findings

Workspace: `/tmp/ctox-bos-native-install-r7-20260613-105539`

Runtime:

```text
source: CTOX service queue via `ctox business-os commands dispatch`
model: MiniMax-M3
context: 256k / 262144 tokens
data plane: native Business OS RxDB/WebRTC peer up, required collections ready, http_bridge_available=false
commands: subscriptions, inventory, projects, contracts, quality
```

Result: failed early and intentionally stopped after the first worker
(`contracts`) proved that prompt-only path hardening is insufficient.

Evidence:

```text
the embedded skill included the MODULE_DIR write pattern and module-owned schema rule.
the worker eventually set MODULE_DIR correctly:
  MODULE_DIR="src/apps/business-os/installed-modules/contracts"
  mkdir -p "$MODULE_DIR/locales" "$MODULE_DIR/tests"
but a later write ignored it:
  cd /tmp/ctox-bos-native-install-r7-20260613-105539 && cat > module.json
the generated manifest again contained entry="modules/contracts/index.html" and layout.right.
root-level module.json and collections.schema.json were created.
the worker spent early turns searching ~/.codex and / for skill files instead of using the embedded CTOX skill access path.
```

Assessment:

```text
This is no longer a wording issue. A mid-level model can read the correct
MODULE_DIR rule, perform one correct directory command, and still regress to a
familiar root-level heredoc pattern minutes later. CTOX-native App Creator runs
need an execution guard in addition to the skill.
```

Runtime changes taken from CTOX-native R7:

```text
Unified exec now blocks root-level writes to module.json and collections.schema.json when the cwd is inside a Business OS app workspace.
The guard returns a tool error that tells the agent to write under src/apps/business-os/installed-modules/<module_id>/ using MODULE_DIR.
The guard is intentionally narrow: reads such as cat module.json remain allowed, and writes under installed-modules/<id>/module.json remain allowed.
Harness-core tests cover blocked root module.json writes, blocked root collections.schema.json writes, and allowed installed-module writes/reads.
```

Next CTOX-native round:

```text
rebuild CTOX so the unified-exec guard is active.
create a fresh isolated R8 root with the Business OS app root and app-builder skill assets.
run the same five commands through CTOX service with MiniMax-M3 256k.
watch whether the model self-corrects after the guard blocks a root-level write.
continue hardening remaining failures only from observed majority/repeated failure patterns.
```

## Round 11 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r11-144331`

Sessions:

```text
subscriptions mvs_ad37dd891de54c588a011877a7cc36a6
inventory     mvs_28c917082af54f8daf5cd1683f64388f
projects      mvs_b051d3276b87498ba48500547cae4978
contracts     mvs_6cf02f6cc9b74e078f1684122a1365eb
quality       mvs_7c981630d1eb43b3978ee7d90aba8a87
```

Round result: failed under the latest checker, but only one app failed. Four
agents produced complete source-checkout store modules that passed the bundled
static checker, their module tests, module conformance, and the RxDB-only guard.

Independent evidence:

```text
subscriptions: static checker OK, helper/schema tests pass, conformance OK, rxdb-only OK. The agent initially leaked pending_sync in README text and then self-corrected after running the checker.
inventory: static checker OK, 14/14 tests pass, conformance OK, rxdb-only OK.
projects: static checker OK, 21/22 tests pass with one mount test skipped because the isolated bench lacked the repo ESM package context for .js; conformance OK, rxdb-only OK.
contracts: static checker OK, 32/32 tests pass, conformance OK, rxdb-only OK. The agent initially introduced forbidden package/dependency literals while fixing tests and then self-corrected.
quality: failed the hardened static checker because schema.js exported the shell-registered collection key business_commands. The app also drifted into a custom schema.js transform-loader test harness instead of a simple shared schema helper.
```

Forensic takeaways:

```text
The remaining failure is not a CTOX DB misunderstanding about HTTP, ctx.db.raw, ctox.db, package installs, or commandBus fallbacks; those rules now mostly hold.
The weak spot is schema ownership: some agents still treat business_commands as a schema.js collection when module.json lists it as a dependency.
The second weak spot is test architecture: when .js ESM import is awkward in an isolated bench, agents may build vm/new Function/string-transform loaders instead of moving shared schema objects into a browser-safe .mjs helper.
Several agents ran module_static_check before later test/README edits. The skill must say that the checker is the final gate and must be rerun after any subsequent module or phase-plan change.
```

Skill changes taken from Round 11:

```text
module_static_check.mjs now rejects test/source use of node:vm and new Function as schema transform-loader workarounds.
SKILL.md, module-contract.md, verification.md, and architecture-translation.md now require a shared local browser-safe schemas.mjs/core helper for reusable schema objects, or simple text/JSON parity checks.
SKILL.md, verification.md, and the OpenAI agent metadata now say module_static_check.mjs is the last source-checkout gate; any module or phase-plan edit after a green run makes the evidence stale.
The ESM-only library rule was hardened from "no dependency-managed imports" to "no package-manager setup, not even as a future activation step; defer the feature or use a CTOX API when no shipped local ESM exists."
```

## Round 12 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r12-153010`

Sessions:

```text
subscriptions mvs_f310a27d8bc54b019729e8254799c201
inventory     mvs_72afb8a427754c7aa7f42e3daf772d70
projects      mvs_73f61db5a37f47f6b3e4af7a5580691b
contracts     mvs_30a906c7a6fd408da852b760634e44b2
quality       mvs_54da2efe584f4fa1aa66d0e861f1f05a
```

Round result: failed, but only one gate remained red after agent self-repair.
The static checker plus stronger schema rules drove agents to repair most
negative-proof leaks, shell-collection schema mistakes, registry omissions,
broken tests, and `.DS_Store` artifacts.

Independent evidence:

```text
subscriptions: static checker OK, tests OK, conformance OK, rxdb-only OK.
projects: static checker OK, 26/26 tests pass, conformance OK, rxdb-only OK.
contracts: static checker OK after self-removing .DS_Store and forbidden literal tests, 20/20 tests pass, conformance OK, rxdb-only OK.
quality: static checker OK after plan cleanup, 20/20 tests pass, conformance OK, rxdb-only OK.
inventory: static checker OK under the old checker, 27/27 tests pass, but assert-module-conformance failed on css-no-root-tokens because index.css defined custom properties on :root. The agent nevertheless claimed all gates green, so independent conformance remains mandatory.
```

Forensic takeaways:

```text
The majority failure pattern at the start of R12 was plan/README/test negative-proof leakage. Agents read the rule, then restated forbidden strings in docs. The final static checker eventually drove 5/5 static-clean modules, but cleanup consumed substantial time.
The static checker was missing one important conformance rule: module CSS must not define variables on :root or redefine shell/base tokens. The inventory app exposed this because tests and old static check passed while conformance failed.
Agents still sometimes prefill phase trackers with future "done" evidence before files or checks exist. The static checker catches missing files and stale strings, but not dishonest phase timing. This remains a process concern for review, although final gates matter more for source correctness.
Contracts showed the desired self-correction pattern: it removed self-authored forbidden-pattern tests and left broad scans to module_static_check.mjs.
Inventory showed the undesired false-green pattern: its final response said all gates green without running or honoring assert-module-conformance.
```

Skill changes taken from Round 12:

```text
module_static_check.mjs now mirrors the conformance CSS rules for :root custom properties and shell/base token redefinitions.
SKILL.md, module-contract.md, verification.md, architecture-translation.md, and OpenAI metadata now require module-local CSS variables to be scoped under the module root and forbid :root/html/body variables and shared shell token writes.
The hardened checker was validated against the R12 inventory workspace and now fails that module with: index.css defines custom properties on :root; scope module tokens under the module root class.
```

## R13-R15 Harness Notes

```text
R13/R13b: invalid as a bench round. MiniMax sessions were created, but the daemon restarted and the sessions disappeared from direct message access before producing module files. The empty workspaces do not count as app outcomes.
R14/R15: invalid as bench rounds. Initial workspace copy strategy included local agent/build artifacts and failed before copying src/apps/business-os/modules; agents saw an incomplete repo without src/ and began to improvise. These runs were aborted and excluded from app-quality evidence.
```

Harness fix:

```text
Use a reduced source-checkout copy for bench agents: root guardrails, docs/business-os.md, docs/ctox-rxdb.md, src/apps/business-os, src/core/rxdb, src/core/business_os, and the relevant skill directories. Validate src/apps/business-os/modules and the skill file exist before starting agents.
```

## Round 16 Findings

Workspace: `/tmp/ctox-bos-skill-bench-20260612-r16-161640`

Sessions:

```text
subscriptions mvs_35c4e5314ddc43db9249b13deb2fa1db
inventory     mvs_8e661e0f185e48df82fc5e5bcbbc7300
projects      mvs_512b80dcadb14e8facca5a2a32113c73
contracts     mvs_707ae9d9f634464bb14372fd157fa4a2
quality       mvs_474da7362248421985df215f8cc63e31
```

Round result: passed for the normal-agent/source-checkout bench. All five
MiniMax M3 agents produced complete modules from intentionally simple prompts,
and all five passed independent source gates after the reduced workspace was
given the missing `docs/business-os.md` file required by the RxDB-only guard.

Independent evidence:

```text
subscriptions: static checker OK, 18/18 tests pass, conformance OK, rxdb-only OK.
inventory: static checker OK, 20/20 tests pass, conformance OK, rxdb-only OK.
projects: static checker OK, 20/20 tests pass, conformance OK, rxdb-only OK.
contracts: static checker OK, 21/21 tests pass, conformance OK, rxdb-only OK.
quality: static checker OK, 20/20 tests pass, conformance OK, rxdb-only OK.
```

Forensic takeaways:

```text
The agents initially tried the Mavis skill tool with business-os-app-module-development, but that registry did not know the new skill id. They recovered by finding the source skill file in the checkout. This is acceptable for source benches, but a real CTOX queue/App Creator path must tell agents how to load/export the embedded system skill.
Legacy modules remain mixed-quality few-shots. Agents read calendar/notes and noticed stale patterns such as business_commands redeclared in schemas or raw DB usage, then used module_static_check.mjs to classify them as rejected legacy patterns instead of copying them.
The broad forbidden-literal rules are now learned. Multiple agents leaked forbidden strings into README, phase-plan, tests, or evidence, then self-corrected and reran checks.
The ESM-only/no-package rule held in the final outputs.
The schema helper pattern held: greenfield modules used local browser-safe ESM helpers instead of JSON imports or transform loaders.
The normal-agent source-checkout threshold is met. Further improvement should move from artificial /tmp source benches to CTOX-native queue/App Creator execution with MiniMax M3 and large context.
```

Skill/core changes taken from Round 16:

```text
The Business OS queue prompt now emits an explicit "Business OS app-module skill access" block whenever the module-development skill is required or inferred. It tells agents to use the embedded system skill id first, then ctox skills system show/export, then the source-checkout SKILL.md, then the official GitHub release-tag URL fallback.
The Business OS command-router and inbound message-router now recognize the German stem "entwickl" in addition to "entwickel", so phrasing such as "entwickle eine CTOX Business OS App" routes to business-os-app-module-development.
Targeted Rust tests now cover explicit App Creator required_skills, inferred Business OS chat tasks, and prompt inclusion of the skill-access block.
```

## Native CTOX Switch Criterion

Decision after Round 16: the normal-agent/source-checkout threshold is met.
The bench should now move to CTOX-native execution instead of repeating more
artificial `/tmp` source-copy runs.

Switch rationale:

```text
R16 produced 5/5 independently green MiniMax M3 source-checkout modules from intentionally underspecified prompts.
The remaining R16 finding was skill access through the runtime prompt, not a module-architecture failure.
That finding has a core prompt fix and targeted Rust coverage.
Further failures are now more likely to come from CTOX queue/App Creator/release-install integration than from the skill's base app-building instructions.
```

CTOX-native bench target:

```text
Run the same five simple app prompts through Business OS command/chat/App Creator style entry points.
Use MiniMax M3 through CTOX with the 256k chat-context setting.
Do not assume a source checkout exists when testing release behavior.
For source-development validation, keep writes under a disposable workspace or test scope and remove generated modules after each loop.
For release/App Creator validation, verify command completion, installed module files, live Business OS shell mount, and skill routing metadata.
```

Native acceptance gates:

```text
each command/queue task carries required or suggested skill business-os-app-module-development
worker prompt includes embedded-skill show/export or release-tag fallback instructions
app is created in the intended target location: source module for source bench, installed module for runtime bench
no npm/package/import-map/bundler path is introduced
schemas are created dynamically via collections.schema.json for module-owned collections, with shell collections left to the shell
the live shell can load the module without console/runtime errors for the implemented workflows
automation actions dispatch through commandBus and create normal CTOX chat/task follow-up flow
generated modules are removed or isolated before the next loop so results do not cross-contaminate
```

## CTOX-Native R8 Findings

Recorded: 2026-06-13 12:33 CEST

Workspace: `/tmp/ctox-bos-native-install-r8-20260613-121551`

Runtime:

```text
CTOX debug binary: runtime/build/cargo-target/debug/ctox
model: MiniMax-M3
context window: 262144 tokens
execution path: Business OS command queue / installed module target
```

Commands dispatched:

```text
contracts      bench_r8_contracts_1781345942656
inventory      bench_r8_inventory_1781345942656
projects       bench_r8_projects_1781345942656
quality        bench_r8_quality_1781345942660
subscriptions  bench_r8_subscriptions_1781345942656
```

Round result: failed early on the first worker (`contracts`). The service was
stopped after collecting enough evidence because continuing to the other four
commands would only spend runtime after a hard one-shot failure.

What improved:

```text
The agent did not create root-level module.json or collections.schema.json.
The root-write runtime guard and MODULE_DIR skill instructions moved file writes into src/apps/business-os/installed-modules/contracts/.
The agent inspected existing Business OS modules before writing the app.
The app attempted a normal automation path by creating business_commands records from follow-up actions.
```

Independent static check failures for `contracts --installed`:

```text
missing index.css
missing locales/de.json
missing locales/en.json
missing tests/*.test.mjs
module.json entry must be installed-modules/contracts/index.html
module.json install_scope must be installed
module.json layout.right requires layout.third_pane_justification
schema.js exports shell-registered collection key business_commands
index.html defines a third/right pane without explicit workflow justification
index.js contains forbidden raw DB access patterns
```

Forensic takeaways:

```text
Prompt-only hardening was enough for target-directory placement, but not enough for installed-mode manifest semantics.
The model still copied legacy three-pane UX reflexes despite the skill's two-pane/modals default.
The model understood that business_commands is shell-owned in prose, then still exported it in schema.js. The checker caught this inconsistency.
The model tried to generate large JavaScript through shell-embedded Node builders and hit quoting/escaping failures. The final index.js exists, but the build log shows failed intermediate writes and the file still violates architecture rules.
The next hardening should make installed-mode manifest/layout/schema violations immediate runtime feedback, not only post-hoc review feedback.
```

Next hardening targets:

```text
Add a post-write or post-command Business OS module artifact validator for installed-module manifests and schema exports.
Make the runtime feedback explicitly say that installed modules must use entry=installed-modules/<id>/index.html and install_scope=installed.
Treat layout.right / third panes as a hard failure unless third_pane_justification is present and specific.
Promote the shell-owned collection rule into validator feedback with examples: business_commands may be listed in module.json.collections but must not be exported from schema.js or collections.schema.json.
Discourage large JS generation through nested shell/Node string builders; agents should write normal module files directly under MODULE_DIR and run node --check on generated ESM.
```
