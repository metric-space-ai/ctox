---
name: business-os-app-module-development
description: Use whenever CTOX or Business OS must build, modify, repair, install, review, or generate a Business OS app/module from chat, App Creator, App Store, CLI, inbound communication, or an external agent. Requires reading existing Business OS apps first, using the CTOX DB WebRTC data plane, and shipping a runnable no-build ESM module rather than a plan, skill file, or generic web app.
metadata:
  short-description: Build production-ready CTOX Business OS app modules with the native app, data, automation, and validation contracts
---

# Business OS App Module Development

This skill is instruction context. It is not the deliverable.

If the user asks to build, change, or repair a CTOX Business OS app, build the app/module. Do not create, copy, mirror, export, or edit skill files or skill-named directories unless the user explicitly asks to change a skill.

## Mandatory First Screen

For CTOX App Creator and runtime-installed app work, follow this short path before any broad exploration:

1. Target only the prompted module directory: `runtime/business-os/installed-modules/<id>/` for installed apps. Generated apps never belong under `src/`.
2. Inspect the existing scaffold first, but do not dump every scaffold file into context. Verify the file inventory, then inspect only the exact exports, selectors, or failing snippets needed for the next edit. Keep its mount wiring, `core/automation.mjs`, `core/records.mjs`, locales, and tests as the baseline. Do not read the generated scaffold file-by-file or in consecutive chunks to audit your own output. The scaffold is already structurally valid; validating it before requested-domain edits is a task failure, not progress. Run scaffold repair only if a required file is actually missing; never run it on a complete scaffold, and never count scaffold repair output as requested-domain implementation work.
3. Inspect exactly three shipped source modules for patterns, preferably `customers`, `shiftflow`, and `outbound`. Use exact files such as `src/apps/business-os/modules/customers/module.json`, `src/apps/business-os/modules/shiftflow/index.js`, and `src/apps/business-os/modules/outbound/core/automation.mjs`; do not run `ls`, `find`, `rg`, or `grep` over `src/apps/business-os/modules/` or any source-module directory. Use `notes` only as a supplemental simple-CRUD reference; do not treat it as a scaffold-helper template. Do not inspect `src/apps/business-os/app.js`, `shared/`, `router/loader` source, `src/core/business_os/`, or validator/checker source while building a generated app.
4. Preserve scaffold helper export names such as `buildFollowUpCommand`, `summarizeRecords`, and `visibleRecords` unless you update every import in `index.js` and `tests/*.mjs` in the same edit. For first-pass runtime apps, treat `buildFollowUpCommand(record = {})` exported from `core/automation.mjs` as a stable compatibility facade: change its implementation to build the requested-domain `business_os.chat.task`, but keep the export present, keep both `type` and `command_type`, and keep `payload.record_snapshot`. Do not replace it with only plan helpers, move the command builder into `core/records.mjs`, or import domain helpers from `core/records.mjs` before exporting them there.
5. For first-pass App Creator apps, preserve the scaffold's single primary module-owned collection and helper API by default. Translate the requested domain into fields, statuses, calculations, fixture records, labels, filters, and automation facts on that collection. Do not rename the scaffold collection to domain nouns such as items, locations, batches, projects, contracts, findings, notes, or pick lines. Do not invent extra collections unless the user explicitly needs separate persisted objects and you update `module.json`, `collections.schema.json`, `schema.js`, `core/records.mjs`, `core/automation.mjs`, `index.js`, locales, and every `tests/*.test.mjs` in the same edit before validation. Runtime helpers must not define constants for undeclared collections. Runtime files must not reference `business_commands`; the only automation write path is `ctx.commandBus.dispatch(...)`.
6. Use direct bounded exact-path file edits only. Shell heredocs or redirects are acceptable only when they write the final artifact directly to `$MODULE_DIR/<file>` or the exact prompted module path. Do not write app artifacts to `/tmp`, test them from `/tmp`, copy/move them from `/tmp`, stage `/tmp/*.patch` files, or treat scratch output as implementation work. If a shell write fails because of quoting, history expansion, command length, or escaping, do not switch to Python, base64, Node writer scripts, generated writer scripts, `/tmp` scratch files copied into the module, shell-level `apply_patch` discovery/invocation, module scratch/probe files, `cat >>` append chunks against app artifacts, huge `printf`/`echo`/`tee`/`cat` payload rewrites, `sed -i`/`perl -pi` line surgery, or data-URL workarounds. Shorten the file, keep simple DOM wiring in `index.js`, put pure record logic in `core/records.mjs`, put command payload logic in `core/automation.mjs`, and rewrite the smaller affected file directly at its final module path.
7. Required artifacts are canonical files, not shell patterns. Write `module.json`, `collections.schema.json`, `schema.js`, `index.html`, `index.css`, `index.js`, `icon.svg`, `locales/en.json`, `locales/de.json`, `core/records.mjs`, `core/automation.mjs`, and tests by exact path only. For first-pass runtime-installed App Creator apps, do not create extra `core/*.mjs` layers such as `ui.mjs`, `render.mjs`, `runtime.mjs`, or `panel.mjs`; those are refactor work, not one-shot app creation. For runtime-installed App Creator apps, the module root may contain only those root files plus `core/`, `locales/`, and `tests/`. Do not create or leave temporary schema/manifest aliases, typo files such as `m.json`, `m`, or `modul.json`, scratch notes, copied app roots, glob artifacts, or ad hoc helper directories in the module root. For runtime-installed manifests, preserve `entry="installed-modules/<id>/index.html"`, `install_scope="installed"`, SemVer `version` without a `v` prefix, `store.distribution="ctox-runtime-installed-module"`, `store.installable=false`, and the default two-pane `layout` without `layout.right`; never copy source/store manifest fields such as `entry="index.html"`, `entry="modules/.../index.html"`, `install_scope="store"`, `source="local"`, `store.source_path="modules/..."`, `store.distribution="ctox-repo-module"`, `store.installable=true`, `layout.icon_svg`, or any `layout.right`/right-resizer metadata.
   Treat `index.js` as the load-bearing mount file, not as a generated byproduct. Never delete, rename, move, copy over, omit, or temporarily replace `index.js`; keep it present after every edit. If a validator reports that `index.js` is missing, stop all optional work, restore the scaffold/mount file first, then reconnect it to the current `index.html`, `core/records.mjs`, and `core/automation.mjs` before editing anything else.
8. `schema.js` is invariant. Do not rename it, delete it, replace it with `schema.mjs`, or leave a root-level `schema.mjs`/`schema.cjs` alias next to it. Put reusable schema fragments in `core/*.mjs` and re-export them from `schema.js`.
9. Tests must prove useful behavior, not only syntax. At minimum they must cover record visibility/summary logic and a `business_os.chat.task` command payload whose title/instruction/prompt/record_snapshot contain actual facts from the fixture records. Keep fixture expectations hand-computed and synchronized with the helper's exported shape. If `summarizeRecords()` or another helper returns extra fields, either assert the full updated shape or assert individual fields intentionally; do not leave a stale partial `deepEqual` that fails only because the helper grew. For generated App Creator tests, do not write `assert.deepEqual(summarizeRecords(...), {...})` directly; assign the helper result to a variable and assert named fields deliberately, unless the expected object includes every returned key. Keep fixture status values aligned with `normalizeStatus()` and other normalizers. Do not write negative anti-pattern tests or assertion messages that contain forbidden layout/data/dependency literals; validators own those checks. All tests in `tests/*.test.mjs` must pass; adding a new green test while an old scaffold test imports a missing export is still red.
10. Do not run validation, tests, `node --check`, or scaffold repair as the first action on a complete fresh scaffold or as the first action in a validation-rework turn that says the validator is green but no required artifact was written after the scaffold baseline. First make the smallest requested-domain edits to `core/records.mjs`, `core/automation.mjs`, `index.html`, `index.js`, one locale file, and one `tests/*.test.mjs` file so they contain concrete requested-domain fixture facts. Do not edit `module.json`, `collections.schema.json`, or `schema.js` as an isolated first domain step; those files are scaffold contract surfaces and must be changed only in the same lockstep edit as helpers, UI, locales, and tests. Keep helper exports, schema fields, collection names, status values, command payload fields, fixture records, labels, filters, UI selectors, and HTML `data-action` values in lockstep with every importer, every concrete `index.js` handler/branch/action-map entry, and every test before running validation; partial helper rewrites are not a valid checkpoint. First-pass runtime apps should keep the scaffold's existing action surface: form submit plus `data-action="new"`, `data-action="delete"`, and `data-action="follow-up"`. Prefer changing labels, filters, and selected-record facts over adding new visible action buttons. Do not add `data-action="attention"`, bulk, renew, reorder, export, AI, or status-only buttons unless `index.js` gets an exact matching action branch with a real persistence or `ctx.commandBus.dispatch` effect in the same edit; remove extra buttons instead of leaving unhandled controls. If `index.html` changes after the scaffold baseline, `index.js` must also change after the baseline so selectors, form fields, render output, and actions match the new fragment. Then run `ctox business-os app validate <id> --installed` before claiming success and repair every bullet exactly. Validation completion is accepted only when `core/records.mjs`, `core/automation.mjs`, `index.html`, `index.js`, one locale file, and one `tests/*.test.mjs` file all changed after the scaffold baseline. If validation reports early, partial validation, "no required app artifact was written after CTOX recorded the scaffold baseline", or "generic App Creator records scaffold", immediately edit the runtime app files for the requested domain as one lockstep repair and validate again. Do not answer such a rework prompt by running the validator as the first or only command.
   If validation was green earlier and later turns it red, trust the later red result. Do not continue domain tests while any required root artifact is missing. The repair order is: restore missing required file, prove the required-file inventory, repair imports/handlers/selectors, rerun tests, rerun `ctox business-os app validate <id> --installed`.
11. Once `ctox business-os app validate ...` is green, stop. Do not backfill missed few-shot inspection, read checker internals, search prior bench apps, run repository-wide conformance scripts, source-wide RxDB scans, broad file dumps, cosmetic rewrites, helper refactors, or extra polishing passes. A green App Creator validator plus the required focused tests is the completion boundary. CTOX may still rework the task if the post-scaffold tool trace shows validation before direct module edits, `/tmp` artifact staging/testing/copying, source-module discovery or line-count sweeps, or broad generated-file readback audits; the repair is to write bounded final files directly under `MODULE_DIR`, then run the app-specific validator once.

## Priority Order

1. CTOX itself must be able to build the app through the Business OS App Creator, App Store, chat, or command flow. The primary acceptance target is a runtime-installed module under `runtime/business-os/installed-modules/<id>/` or `$CTOX_STATE_ROOT/business-os/installed-modules/<id>/` that validates and mounts without a build step.
2. Other agents may use this skill to build the same module contract. External-agent success is useful only when it matches the CTOX-native installed-module contract; it is not a substitute for CTOX App Creator proof.

For App Creator work, optimize every decision for the CTOX-native path first. Source-checkout modules under `src/apps/business-os/modules/<id>/` are for packaged store/templates and source development, not the default target for user-created apps in a regular CTOX installation.

## Non-Negotiable Contract

Stop and report the blocker instead of coding when any hard stop is active:

```text
you have not inspected at least 3 existing Business OS modules for concrete patterns
you are using generated installed modules, runtime/business-os/installed-modules, ~/.local/state/ctox/business-os/installed-modules, bench_* apps, previous App Creator outputs, or app-creator-bench artifacts as few-shot templates instead of shipped Business OS modules
you are about to create a skill file, skill trace, harness trace, README-only deliverable, or plan-only deliverable
you are about to build a generic Next.js/React/Vanilla app outside the Business OS module contract
you are treating a source-checkout or external-CLI bench as the acceptance target while the CTOX-native App Creator installed-module path remains unproven
you are about to use React, Vue, Svelte, Angular, Solid, Preact, Lit, JSX/TSX, a component framework, a framework runtime, or a compile/transpile step for a generated App Creator app
you are about to add package.json for any reason, npm/pnpm/yarn, node_modules, lockfiles, a bundler, CommonJS require, or CDN dependency management
you are about to run npm, npx, pnpm, yarn, bun, npm install, npm init, npm test, esbuild, Vite, Rollup, Webpack, or any package/bundler/transpiler command even as a temporary proof or syntax workaround
you are about to use esbuild, Vite, Rollup, Webpack, node:vm, or new Function as a syntax-check, import, schema-transform, or test workaround
you are about to mention forbidden package-manager, bundler, or dependency names as tooling, import, config, install, test, or build concepts inside generated app files, tests, comments, or user-visible copy; keep tooling references only in validation/skill context. Domain words that collide with tool names, such as "rollup" for KPI or finance summaries, are allowed when they clearly describe business data and not JavaScript tooling.
you are about to use IndexedDB directly, localStorage, sessionStorage, Postgres, SQLite from browser code, ctox.db, ctx.db.raw, HTTP data APIs, /rxdb/pull, /commands, or any fallback data path
you are about to use legacy module patterns as implementation authority instead of translating them to the current contract; examples to reject for new App Creator apps include ctx.db.raw, ctx.collections, manual business_commands inserts/upserts, pending_sync command fallbacks, window.dispatchEvent('ctox-business-os-chat-submit'), esbuild/fake-DOM tests, JSON-module schema wrappers, default layout.right panes, and right-drawer manifest metadata
you are about to request complete file dumps, delegate a broad subagent sweep, or spend more than a short targeted pass on existing modules before writing the first runnable module files
you are about to list or discover the source module root (`src/apps/business-os/modules/`) instead of opening the exact chosen few-shot module paths: customers, shiftflow, and outbound
you are about to run `find`, `ls`, `rg`, `grep`, `tree`, `sed`, `cat`, `head`, or `tail` over a source module directory, the Business OS shell source (`src/apps/business-os/app.js`, `shared/`, `router`, `loader`, `scripts`, `rxdb`), or native Business OS source (`src/core/business_os/`) while creating a runtime-installed app; these are not few-shots
you are about to read the generated App Creator scaffold back file-by-file or in consecutive line-range chunks; after file inventory, inspect only snippets needed for a concrete import, selector, syntax, test, or validator failure
you are about to run `ctox business-os app scaffold ... --repair-missing` on a complete scaffold, or count scaffold repair output as implementation work
you are about to run `ctox business-os app validate`, module tests, or `node --check` before editing requested-domain records, automation payload, visible UI/locales, and tests under the runtime module directory
you are about to use `/tmp` or another scratch directory to write, test, stage, copy, move, or patch generated app artifacts; write bounded payloads directly to `$MODULE_DIR/<file>` or the exact prompted module path instead
you changed `core/records.mjs`, `core/automation.mjs`, `schema.js`, `collections.schema.json`, UI selectors, or command payload fields but have not updated every importer and `tests/*.test.mjs` in the same implementation pass
you changed `summarizeRecords`, `visibleRecords`, `needsAttention`, `budgetStatus`, `billingReadiness`, `milestoneTotals`, status normalizers, or fixture records but left aggregate tests with stale `assert.deepEqual(summary, {...})` expectations that omit fields returned by the helper
you wrote `assert.deepEqual(summarizeRecords(...), {...})` directly in a generated App Creator test instead of assigning the summary result and asserting named fields or a complete exact object
you changed `module.json`, `collections.schema.json`, or `schema.js` before making lockstep requested-domain edits to `core/records.mjs`, `core/automation.mjs`, visible UI/locales, and tests; restore the installed scaffold contract first
you renamed the scaffold's primary module-owned collection to a domain noun in a first-pass App Creator app instead of adding requested-domain fields to the scaffold collection
you are about to introduce extra module-owned collections in a first-pass App Creator app instead of adapting the scaffold's primary collection, unless the user explicitly needs separately persisted objects and module.json, collections.schema.json, schema.js, helpers, UI, locales, and tests are updated in the same edit
runtime app code defines or references module collection names that are not declared in module.json and schema.js/collections.schema.json
runtime app code references business_commands directly; only module.json may list business_commands as a dependency, and automation must use ctx.commandBus.dispatch
you added or renamed `data-action="..."` in index.html without adding the exact matching click handler, action branch, or action-map key in index.js in the same edit
you added a new first-pass App Creator `data-action` value such as `attention`, bulk, renew, reorder, export, AI, or a status-only action instead of reusing `new`, `delete`, `follow-up`, and form submit, unless the matching `index.js` branch performs a real persistence or commandBus action
you changed index.html after the scaffold baseline but left index.js on the scaffold baseline, so selectors/actions/rendering cannot be trusted to match the fragment
you are about to copy a shipped/source module `module.json` shape into a runtime-installed app, including `entry="index.html"`, `install_scope="store"`, `version="v1"`, `source="local"`, `store.source_path="modules/..."`, `store.distribution="ctox-repo-module"`, `store.installable=true`, `layout.icon_svg`, or `layout.right_resizer`
you are about to add `layout.right`, right-pane text, right-resizer metadata, or third-pane manifest fields to a runtime-installed App Creator app unless the user explicitly requested a persistent third pane and a validator-accepted justification is included
you are about to run `wc -l`, multi-file `sed -n`, multi-file `grep`/`rg`, broad globs, `head`/`tail` over roughly 40 lines, line ranges over roughly 60 lines, or Node `fs.readFileSync` plus `console.log` dumps against generated runtime-installed module files to audit your own output; use validator/test output and one exact failing snippet only
you are about to run broad discovery commands over `$HOME`, `/Users`, the whole install root, the whole repo, or validator/checker names; use exact known paths, `MODULE_DIR`, and at most the three selected few-shot modules
you are about to read, list, stat, resolve, or write `$HOME/.local/state/ctox/business-os/installed-modules` directly; use the prompted `runtime/business-os/installed-modules/<id>/` module path only
you are about to write app files outside the resolved module directory, such as root-level module.json, root-level collections.schema.json, root-level <id>/, src/skills/, or any skill-named path
you are about to leave any unexpected file or directory directly under the runtime-installed module root; allowed root entries are only module.json, collections.schema.json, schema.js, index.html, index.css, index.js, icon.svg, core/, locales/, and tests/
you are about to create short alias or typo artifacts such as m.json, manifest.json, collections.json, schema.mjs, schema.cjs, modul.json, or any extra root file while trying to repair shell quoting or command-length issues
you are about to run rm, unlink, rmdir, mv, cp, or install on any required generated module artifact or on the module directory itself; required files must stay present after every edit, and replacement must be a direct bounded exact-path rewrite
you are building a new runtime-installed App Creator app and neither the CTOX service preflight nor your first explicit action has created a complete validator-clean scaffold under the target directory
you are about to delete, omit, or replace scaffold invariants such as core/automation.mjs, core/records.mjs, locales/de.json, locales/en.json, or tests/*.test.mjs instead of customizing them in place
you are about to create extra runtime App Creator helper layers under `core/` such as `ui.mjs`, `render.mjs`, `runtime.mjs`, `panel.mjs`, `selectors.mjs`, or `view.mjs`; first-pass runtime apps must keep pure record logic in `core/records.mjs`, command payload logic in `core/automation.mjs`, and simple DOM wiring in `index.js`
you are about to clean, reset, delete, or rewrite a validator-clean scaffold wholesale because it looks generic; keep it as the baseline and edit the smallest domain-specific parts
you are about to treat an untouched validator-clean scaffold as a completed requested app; a scaffold is only the baseline, and at least the requested-domain record model, labels, visible workflow, automation payload, and tests must be customized before final validation can complete the task
you are about to write, copy, rename, delete, or repair required app artifacts through shell globs, wildcard filenames, brace expansion, or fuzzy names such as co*.json, colle{ctions,ctions}.schema.json, *.schema.json, .tmp_schema.json, .csjson.tmp, module*.json, or tests/*.mjs; required artifacts must be exact-path edits only
you are about to run rm, mv, cp, or install over required artifacts unless every source and destination is an exact path inside MODULE_DIR and the command cannot delete module.json, collections.schema.json, schema.js, index.html, index.css, index.js, icon.svg, locales, core helpers, or tests
you are about to create or update a runtime-installed App Creator module whose module.json lacks a SemVer version in x.y.z form without a v prefix
you are about to expose, advertise, or call a module public/user-ready while its app version is below 1.0.0
you are about to use 2.0.0 or any later x.0.0 as an in-place update of the same app id/icon instead of a new parallel app line
you are about to call `ctox queue ack`, `ctox queue complete`, `ctox queue release`, `ctox queue fail`, `ctox queue block`, or edit queue/command/runtime-status rows directly; CTOX service owns lifecycle completion
you are about to let `current_queue_item_id`, an open-work block, or an unrelated queue row redirect the app build away from the authoritative module_id and only_allowed_app_artifact_directory
you believe a harness, artifact contract, benchmark note, or review example requires root-level module.json, root-level collections.schema.json, root-level harness-module.json, root-level harness-collections.schema.json, root-level artifact/status/blocker Markdown, or any other root alias for an app deliverable
you are about to test the guard by creating, moving, touching, symlinking, hardlinking, copying, or removing root-level app artifact probe files such as `test-*`, `_test_*`, `_probe_*`, `probe-*`, root `module.json`, root `collections.schema.json`, or guard/status scratch files
you are about to probe shell aliases, tool wrappers, guard behavior, or temporary root write behavior instead of implementing the app in the allowed module directory
you are about to probe module write limits by creating `_scratch*`, `_size*`, `_test*`, probe files, throwaway files, or temporary deletion/recreation cycles inside the generated module directory
you are about to run `which apply_patch`, `apply_patch --help`, inspect a shell `apply_patch` binary, invoke a shell `apply_patch` wrapper, or create `/tmp/*.patch` files for generated module repairs; CTOX App Creator agents must not discover or use shell patch tools
the module has a visible button/action with no real handler, persistence change, automation command when relevant, and test or smoke assertion; every HTML `data-action` value must be handled by exact action logic in index.js
the module's automation uses window.dispatchEvent, a shell chat CustomEvent, a direct business_commands write fallback, or any other compatibility path instead of ctx.commandBus.dispatch for a standard CTOX work/chat/ticket item
the module's automation uses `ctox.business_os.ticket.followup.create`, `ctox.ticket.*`, or a module-specific follow-up command type instead of the exact App Creator standard `business_os.chat.task`
the module's tests, comments, helper names, or assertions describe a business_commands fallback as valid behavior; tests must prove commandBus.dispatch-only automation with type and command_type business_os.chat.task plus record_snapshot
the module declares collections in module.json but not in schema.js and collections.schema.json
the module-owned data model is unclear: central object, collection names, states, commands, and automation payload are not named
index.js does not load the module fragment with `fetch(new URL('./index.html', import.meta.url))`, assign it into `ctx.host.innerHTML`, and attach `index.css` through a local `new URL('./index.css', import.meta.url)` stylesheet before DOM queries or event wiring
index.html is a full browser document or declares document/head resources; App Creator `index.html` must be a shell fragment only, with no `<!doctype>`, `<html>`, `<head>`, `<body>`, `<link>`, `<script>`, `<meta>`, `<title>`, or `<style>` tags
index.js queries a `data-*` selector that is absent from index.html and from generated markup
tests under `tests/` read module files with `../../module.json`, `../../collections.schema.json`, or `../../schema.js`; from `tests/*.test.mjs`, the module root is exactly `..`, so sibling files must resolve from `resolve(testDir, '..')`
the app has a decorative third pane, layout.right by default, layout.drawers.right/right-drawer metadata by default, right-column resizers by default, decorative controls, fake AI buttons, fake status-only actions, or UI that is not needed for the workflow
module.json embeds `layout.icon_svg` instead of using the required separate `icon.svg`
module.json embeds inline SVG in any field such as `icon_svg`, `iconSvg`, `layout.icon`, or `layout.icon_svg`; generated apps must keep all SVG markup only in icon.svg
index.css defines shell/base tokens such as `--surface` or creates self-referential local tokens such as `--<id>-bg: var(--<id>-bg)`
module.json or collections.schema.json would be exposed in an invalid or incomplete state after any edit
any required module file is still missing: module.json, collections.schema.json, schema.js, index.html, index.css, index.js, icon.svg, locales/de.json, locales/en.json, or tests/*.test.mjs
the validator reports missing files, right/third-pane layout, schema, manifest, dependency, syntax, or test failures and you are about to finish instead of repairing the exact bullets
you are about to write a very large app file as one huge tool-call argument or here-doc; keep generated files concise and split large writes into bounded chunks
you are about to rewrite generated app files with giant `printf`, `echo`, `tee`, or `cat` shell payloads; reduce scope and rewrite only the bounded final file that changed, using only the scaffold helpers `core/records.mjs` and `core/automation.mjs` for first-pass runtime apps
you are about to repair required app artifacts with `cat >>`, `tee -a`, or other append chunks; rewrite the bounded final file directly and leave required artifacts valid after each edit
you are about to use Python, base64 blobs, Node writer scripts, data URLs, generated writer scripts, `/tmp` scratch files copied into the module, `/tmp/*.patch` files, shell `apply_patch` wrappers, or temporary generated file-copy wrappers to create or repair app files; if a file needs that, split/refactor it into smaller ESM helpers and direct bounded exact-path edits
you are about to fix shell quoting, history expansion, command-length, or escaping problems by switching to Python/base64/Node writer scripts instead of shortening or splitting the affected app file
you are about to patch generated module files with `sed -i`, `gsed -i`, `perl -pi`, repeated line-number insert/delete commands, or a sed script staged in `/tmp`; rewrite the affected bounded helper/file directly instead
you are about to continue editing, reading whole generated files, running broad conformance scripts, or polishing after `ctox business-os app validate <id> --installed|--source` is already green
you are about to run source-wide validation scripts such as `assert-module-conformance.mjs` or `assert-rxdb-only.mjs` as an extra readiness gate for a runtime-installed App Creator app after the app-specific validator is green; use the app-specific validator as the acceptance gate and stop
you are about to dump complete generated files with `cat`, Node `fs.readFileSync`/`console.log` dumps, loops over every app artifact, `wc` audits, broad `head`/`tail` snippets, broad `find`/`rg` sweeps, consecutive `sed -n` chunks, or multi-file grep/sed commands after the scaffold exists; inspect only one targeted snippet needed to repair a concrete failing validator or test bullet
you are about to use `find .`, `find src`, `find runtime`, list `runtime/business-os/installed-modules/`, list `runtime/business-os/template-store`, or search prior `bench_*` apps while creating a runtime-installed app; use the prompted module path and selected shipped source modules only
you are about to patch any generated JavaScript file with fragile line-number sed edits instead of rewriting the relevant bounded helper/file
you are about to make a failing test match broken behavior instead of fixing the app contract violation it exposed
you are about to write or keep generated tests whose fixture expectations are not hand-computed and internally consistent with the helper logic they exercise
you are about to keep a generated `deepEqual` assertion against a stale partial aggregate object after the helper added legitimate fields; update the expected shape or assert named fields deliberately
you are about to write or keep tests that assert labels, messages, counts, named imports, or helper exports that the local helper being tested does not actually produce/export
you are about to rename or remove scaffold helper exports such as buildFollowUpCommand, summarizeRecords, or visibleRecords without updating every importer in index.js and tests/*.mjs in the same edit
you are about to remove `buildFollowUpCommand` from `core/automation.mjs`, replace it with only plan helpers such as `planFollowUpTasks`, move the command-builder facade into `core/records.mjs`, omit either `type` or `command_type` from the command, or import a domain helper from `core/records.mjs` before that helper is exported there
you are about to add a second replacement test file while leaving an existing tests/*.test.mjs file red; every existing test file is part of the acceptance contract and must be updated or removed for a validator-approved reason
you are about to write an automation test that checks only command type fields but not concrete fixture facts in title, instruction/prompt, and record_snapshot
you are about to import browser entry files such as `index.js` or `schema.js` directly from Node tests, or through `data:text/javascript`, base64, `Buffer.from(source)`, or any generated data URL, instead of testing local `.mjs` helpers and JSON/text parity
you are spending extra turns reading validator/static-checker implementation internals before the required module file set exists; do not open `validate-app-module.mjs`, `module_static_check.mjs`, `assert-module-conformance.mjs`, or `assert-rxdb-only.mjs` before the required files exist and a validation command reports a concrete failure
you are about to read validator/static-checker implementation internals after running `ctox business-os app validate`; run the validator and repair its concrete bullets, but do not inspect checker source to reverse-engineer it
you are trying to satisfy or avoid scanner keywords by mentally reconstructing the checker instead of writing the smallest valid Business OS module and then repairing actual validator bullets
you are about to write tests that contain forbidden legacy strings as negative-proof literals; generated tests must avoid the forbidden literals entirely and assert positive current-contract behavior instead
you are about to write generated tests that use assert.doesNotMatch, scan source files for forbidden anti-pattern absence, build forbidden tokens from fragments, use String.fromCharCode or regex assembly to bypass validators, or describe validator/checker workarounds; delete that test and write positive contract tests instead
you are about to assert that a manifest, HTML fragment, CSS file, or source file lacks a forbidden third-pane/right-pane pattern by embedding that forbidden term in a test name, assertion, failure message, regex, comment, property access, or string literal; validators own absence checks. Do not write `manifest.layout?.right`, `manifest.layout.drawers?.right`, or equivalent negative layout access. Test positive layout facts instead, such as `manifest.layout.shell === "full-workspace"`, the expected left/center labels, `Object.keys(manifest.layout).sort()` equalling only the expected keys, and the expected module root class.
tests and Business OS guards were not run after the last code change
```

Never say "done", "ready", "production-ready", or "runs" while any hard stop is active.

## Required First Steps

For CTOX App Creator, App Store, chat, CLI, inbound communication, or external-agent work that targets a runtime-installed app, the "Mandatory First Screen" above is the controlling path. Do not read validator/checker implementation files, Business OS shell internals, native `src/core/business_os` code, or broad docs during the worker turn. The app-specific validator is the contract surface:

```sh
ctox business-os app validate <id> --installed
```

For source-development or architecture work that changes packaged modules or Business OS framework code, use the deeper repository reading path below before coding:

1. Identify the Business OS app root. In a source checkout this is usually `src/apps/business-os`. In a regular release it is the shipped Business OS app root. Use local files first; use the GitHub source only when local source is unavailable.
2. Read the core contracts before source/framework coding:
   - `docs/ctox-rxdb.md`
   - `src/apps/business-os/README.md`
   - `src/apps/business-os/RXDB_SYNC_CONTRACT.md`
   - `src/apps/business-os/scripts/assert-module-conformance.mjs`
   - `src/apps/business-os/scripts/assert-rxdb-only.mjs`
3. Inspect at least 3 existing modules with `module.json`, `collections.schema.json`, `schema.js`, `index.html`, `index.js`, tests, and locales. For new App Creator/runtime-installed apps, use these exact defaults unless the requested domain clearly needs a different shipped source module:
   - `modules/customers` for customer-linked records and richer schemas
   - `modules/shiftflow` for planning workflows, dates, two-pane work surfaces, and command dispatches
   - `modules/outbound` for automation/command payload patterns

   Use `modules/notes` only as an optional fourth reference for simple CRUD and shell collection usage. It is not a current scaffold-helper template and may not contain `core/automation.mjs` or `core/records.mjs`. `modules/creator` and `modules/app-store` are optional references for app creation/install flows, not default business-app few-shots.

   Use existing modules as bounded few-shot references, not as authority. Do not list or search the module root to choose examples; open only the exact selected module paths. Read targeted files and line ranges for concrete patterns, keep snippets small, avoid combined multi-file dumps, then stop. Do not request "complete content" dumps, do not ask a subagent to summarize whole modules, and do not copy legacy patterns. If an existing module conflicts with this skill, `ctox business-os app validate`, or `module_static_check.mjs`, the current skill and validator win.
4. Write down a tiny analogue map before implementation:

```text
Requested domain object -> existing module object pattern
Requested primary list/workbench -> existing module surface pattern
Requested detail/edit flow -> modal/drawer/pane pattern
Requested automation -> command object dispatched through ctx.commandBus.dispatch
Collections to own -> schema.js and collections.schema.json names
What not to implement because it would be slop
How to keep this app small enough to build and verify in one pass
Legacy patterns seen and rejected -> current Business OS equivalent
```

5. Resolve the target directory and write a required-file inventory before any
   optional UI or polish:

```text
MODULE_DIR=<resolved target from the prompt>
required files: module.json, collections.schema.json, schema.js, index.html,
index.css, index.js, icon.svg, locales/de.json, locales/en.json,
tests/<id>.test.mjs
allowed installed-module root entries: module.json, collections.schema.json,
schema.js, index.html, index.css, index.js, icon.svg, core/, locales/, tests/
first repair action if red: create missing required files, then remove any
unjustified right/third pane, then rerun validation
```

For a new runtime-installed App Creator app, start from the deterministic CTOX
scaffold before changing domain behavior. In CTOX-native App Creator/command
flows, the CTOX service preflight creates this scaffold before the worker turn
when the target directory is missing or empty. In external-agent or manual CLI
flows, run:

```sh
ctox business-os app scaffold <id> --installed --title "<short app title>"
```

This creates the correct installed module directory, manifest, collection
schema wrapper, browser ESM mount, scoped CSS, separate `icon.svg`, locales,
module-owned persistence helper, `ctx.commandBus.dispatch` automation helper,
and positive `.test.mjs` checks. After scaffold, customize the generated files
for the requested domain. Do not rewrite `module.json`, schema files, mount
wiring, persistence helpers, automation helpers, or tests from scratch unless a
validator bullet requires a bounded repair. Do not use `--force` for an
existing app modification; use `--force` only when intentionally resetting a
failed new-app scaffold.

If validation reports missing scaffold files after a partial rewrite, repair
non-destructively before changing domain logic:

```sh
ctox business-os app scaffold <id> --installed --repair-missing
```

This command only creates missing scaffold files. It must not be used as a
reason to delete customized app files. After repair, reconcile the existing
domain files with the restored core helpers, locales, and tests.

Do not use queue IDs or open-work context as a target selector. The App
Creator/CTOX service will complete queue and command state after the app
validator is green.

Keep shell inspection bounded. Allowed discovery is limited to exact files in
`MODULE_DIR`, the exact three few-shot module directories you chose, and the
commands printed in this skill. Do not run `find`, `rg`, `grep`, or `ls` over
`$HOME`, `/Users`, `/`, the entire installed release root, or the whole source
repo to discover validators, scripts, examples, or guard internals. If you need
the validator, run `ctox business-os app validate <id> --installed|--source`;
do not search for validator filenames.

The CTOX execution harness enforces these App Creator constraints. It blocks
whole-file `cat` dumps of generated module artifacts, Python/Node/base64
programmatic writers against module files, oversized heredoc rewrites, source
tree installed-module writes, package-manager side effects, and root-level app
artifact aliases. Treat such guard output as an instruction to narrow the edit
or split the app into smaller module-local ESM helpers, not as a shell problem
to bypass.

After the required reading and tiny analogue map, write the first runnable
slice immediately. The first write action after the three-module summary must
create `MODULE_DIR` and the required file inventory. Do not open or inspect
`validate-app-module.mjs`,
`module_static_check.mjs`, `assert-module-conformance.mjs`,
`assert-rxdb-only.mjs`, shell wrappers, or guard internals while `module.json`,
`collections.schema.json`, `schema.js`, `index.html`, `index.css`, `index.js`,
`icon.svg`, locales, and at least one test are still missing. The correct loop
is:

```text
1. inspect contracts and 3 modules
2. choose MODULE_DIR and app scope
3. for a new runtime-installed app, confirm the CTOX service preflight scaffold exists or run `ctox business-os app scaffold`; otherwise write the required files with a small two-pane/modal app
4. customize the scaffold for the requested domain before final validation: update the owned collection shape, fixture records, labels, filters/statuses, visible workflow, `business_os.chat.task` payload, and positive tests
5. parse JSON and run syntax/test/validator gates
6. copy exact validator failure bullets into the repair checklist
7. repair the module files and rerun the same gates
```

Validators and static checkers are gates, not few-shot source material. Read
their user-facing failure output, not their implementation, unless the checker
itself crashes or the failure cannot be mapped to a module file.

Required artifact writes must be exact and boring. Do not use shell globs,
brace expansion, fuzzy path patterns, or temporary schema/manifest copy chains
for required files. Examples that are always wrong for App Creator modules:
`cp .tmp_schema.json co*.json`, `cp .csjson.tmp collections*.json`,
`rm collections.schema.json`, `rm *.json`, and files literally named
`m.json`, `co*ions.*json`, or `colle{ctions,ctions}.schema.json`. If a required file
needs replacement, write the final content directly to
`$MODULE_DIR/collections.schema.json` or the exact required target path and
parse or import it immediately.

## Architecture Translation Layer

Use this mapping when your instincts suggest a familiar web stack:

```text
Next.js route/page -> Business OS module index.html plus index.js mount(ctx)
React component tree -> direct browser ESM DOM rendering or existing local module pattern
API route/server action -> command object dispatched through ctx.commandBus.dispatch
Postgres table/migration -> module-owned collection declared in schema.js and collections.schema.json
Prisma/Drizzle model -> JSON schema collection with version and migrationStrategies
IndexedDB/localStorage/sessionStorage app store -> CTOX DB collection supplied by ctx.db, or transient in-memory state for non-persistent UI state
HTTP fetch to backend -> not allowed for Business OS data; use RxDB/WebRTC and command bus
npm dependency -> not allowed; use browser APIs, shipped vendor ESM, or local ESM modules
background job -> CTOX queue task requested through ctx.commandBus.dispatch
ticket/chat automation -> business_os.chat.task command object with outbound_channel/response_channel business_os_chat; no App Creator exception for ctox.business_os.ticket.followup.create or ctox.ticket.*
```

CTOX DB is CTOX-owned and RxDB-derived. It is not npm `rxdb`, not `ctox.db`, and not a generic IndexedDB wrapper. Business data persists through the Business OS shell context and replicates WebRTC-only to the CTOX native peer. Do not add HTTP fallbacks.

Generated App Creator apps must be vanilla static Business OS modules:

```text
index.html -> static fragment owned by the module, not a full HTML document
index.css -> scoped module CSS
index.js -> browser ESM exporting mount(ctx), loading index.html into ctx.host.innerHTML, attaching index.css, then wiring data, events, and automation
local .mjs helpers -> optional pure logic, schemas, reducers, and command builders
```

Do not use a UI framework, JSX/TSX, compile step, transpiler, generated bundle,
or dependency-managed runtime. CTOX DB/ctox-rxdb is already provided by the
Business OS shell as a local ESM runtime; app modules consume only the `ctx`
facade and must not import or bundle the data runtime themselves.

For more details, read the relevant bundled reference before coding:

- `references/business-os-app-architecture-porting.md` for the concise architecture translation layer.
- `references/architecture-translation.md` for a fuller porting guide across source checkouts and regular release installs.
- `references/module-contract.md` for exact module file, schema, registry, and install-scope rules.
- `references/verification.md` for validation and forensic checks.

## Module File Shape

For a module id `<id>`, create or edit only the module directory:

```text
modules/<id>/                 # core/starter/store source module
runtime/business-os/installed-modules/<id>/
                               # runtime-created installed module from an install root
  module.json
  collections.schema.json
  schema.js
  index.html
  index.js
  index.css                   # required; may be minimal
  icon.svg
  locales/de.json
  locales/en.json
  tests/<id>.test.mjs         # or <id>.test.mjs for small modules
```

The Business OS shell imports `index.js` and calls `mount(ctx)`. It does not
inject the module's `index.html` or `index.css`. Every generated App Creator
module must load its own static fragment and stylesheet before querying DOM
nodes:

```js
export async function mount(ctx) {
  attachStylesheetOnce();
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  // Now query ctx.host and wire persistence, actions, and automation.
}
```

Attach the stylesheet through a local
`new URL('./index.css', import.meta.url)` link. Do not leave `ctx.host` empty
while `mount(ctx)` only registers events against selectors that live in
`index.html`.

Use this exact stylesheet helper shape for runtime-installed apps. The only
allowed `fetch(...)` in `index.js` is the local template fetch above:

```js
function attachStylesheetOnce() {
  const href = new URL('./index.css', import.meta.url).href;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}
```

Do not call `fetch('./index.html')`, `fetch('/...')`, `fetch(new Request(...))`,
or helper-wrapped network fetches in generated App Creator modules. Load the
fragment exactly with `fetch(new URL('./index.html', import.meta.url))`.

`index.html` is inserted into an existing shell document through
`ctx.host.innerHTML`. Therefore it must contain only module UI markup such as
`<main>`, `<section>`, forms, buttons, tables, dialogs, and lists. It must not
start with `<!doctype>` and must not contain `<html>`, `<head>`, `<body>`,
`<link>`, `<script>`, `<meta>`, `<title>`, or `<style>`. A full document may
appear to render but causes real shell-browser failures such as
`/business-os/index.css` 404s when relative head resources are parsed after
injection.

Keep `index.html` and `index.js` synchronized. Every selector passed to
`querySelector`, `querySelectorAll`, `closest`, or `matches` for a `data-*`
attribute must point to an element in `index.html` or to markup that `index.js`
itself creates. If you replace a scaffold form, list, toolbar, tab, or action
button, update the event wiring in the same edit. Do not leave generic scaffold
JavaScript querying removed elements such as `[data-form]` or `[data-records]`.

Resolve the target directory before writing any file:

```text
install_target = runtime-installed-module -> runtime/business-os/installed-modules/<id>/
install_target = source-module or core/starter/store source -> src/apps/business-os/modules/<id>/
```

When `install_target` is `runtime-installed-module`, all generated app files must be under `runtime/business-os/installed-modules/<id>/` from a CTOX install/release root. That `runtime/` directory is the local CTOX state root, so the absolute install location is `$CTOX_STATE_ROOT/business-os/installed-modules/<id>/` (for example `~/.local/state/ctox/business-os/installed-modules/<id>/`). Do not write runtime-installed apps under `src/apps/business-os/installed-modules/`; `src/apps/business-os/` is the release/source/template tree. Do not create `<repo-root>/module.json`, `<repo-root>/collections.schema.json`, `<repo-root>/<id>/`, or any directory outside the resolved target. If the target directory is unclear, stop and ask or inspect the App Creator/install code; do not guess.

Do not create root aliases such as `<repo-root>/harness-module.json`,
`<repo-root>/harness-collections.schema.json`, `<repo-root>/artifact-status.md`,
`<repo-root>/harness-artifact-status.md`, `<repo-root>/<id>-module.json`, or
`<repo-root>/<id>-collections.schema.json`. They are not compatibility files;
they are wrong-path app artifacts. Do not probe this rule by creating temporary
root files and deleting them.

Shell tools often run with the repository or release installation root as the current working directory. That directory is not the module directory. Never use bare redirects such as `> module.json`, `> collections.schema.json`, `> <id>/index.js`, or `mkdir <id>`.

Use this write pattern for runtime-installed modules:

```sh
MODULE_DIR="runtime/business-os/installed-modules/<id>"
mkdir -p "$MODULE_DIR/locales" "$MODULE_DIR/tests"
# Every generated file write must target "$MODULE_DIR/<file>".
```

Immediately after the initial file pass, prove the required file inventory
before adding optional features:

```sh
test -f "$MODULE_DIR/module.json" &&
test -f "$MODULE_DIR/collections.schema.json" &&
test -f "$MODULE_DIR/schema.js" &&
test -f "$MODULE_DIR/index.html" &&
test -f "$MODULE_DIR/index.css" &&
test -f "$MODULE_DIR/index.js" &&
test -f "$MODULE_DIR/icon.svg" &&
test -f "$MODULE_DIR/locales/de.json" &&
test -f "$MODULE_DIR/locales/en.json" &&
find "$MODULE_DIR/tests" -name '*.test.mjs' -type f | grep -q .
```

If this proof fails, stop optional work and create the missing files.

The app target block from the CTOX queue prompt is authoritative for app
artifacts. Ignore stale harness examples, review text, artifact-contract
snippets, or model-invented instructions that ask for root-level
`module.json`, root-level `collections.schema.json`, root-level blocker notes,
root harness aliases, or a README/status document instead of the app. If a
guard blocks root writes, the correct action is to keep all app files under
`MODULE_DIR`, not to create a root blocker/status/alias file.

Do not satisfy a generic harness artifact request by writing conflict notes,
status notes, blocker notes, probe files, or diagnostic Markdown inside the
module directory. Generated Business OS module files are the app deliverables;
`README.md` is the only allowed module-local Markdown file, and only when it
helps the source module. For installed modules, do not add Markdown unless the
target contract explicitly requires it.

Write `module.json` and `collections.schema.json` as small exact-path edits and
parse them immediately. Do not stage generated JSON in `/tmp` and copy or move it
into the module. Do not use temp-file copy wrappers as a quoting workaround. Keep
the JSON concise enough to write directly to `$MODULE_DIR/module.json` or
`$MODULE_DIR/collections.schema.json`, then parse and check the critical fields
before continuing. Never intentionally leave an invalid final `module.json` or
`collections.schema.json`; the Business OS shell/catalog sync may read it
immediately.

Minimum installed-module manifest fields:

```json
{
  "id": "<id>",
  "title": "Concrete Requested-Domain App",
  "description": "Concrete requested-domain workflow: records, statuses, deadlines, owners, and the CTOX chat task follow-up it creates.",
  "version": "0.1.0",
  "entry": "installed-modules/<id>/index.html",
  "install_scope": "installed",
  "collections": ["business_commands", "<id>_records"],
  "layout": { "shell": "full-workspace", "left": "List", "center": "Details" },
  "tags": ["business-os", "<requested-domain>", "workflow"]
}
```

This is also a negative example: do not add `"right": "Details"`,
`"right": "Inspector"`, `layout.drawers.right`, or any `layout.right` unless
the user explicitly asked for a persistent third pane and you also add
`layout.third_pane_justification`. The default Business OS app surface is one
or two panes plus an in-module modal/drawer. Do not use right-drawer manifest
metadata as a decorative substitute for a third pane.
Do not embed SVG in `module.json.layout.icon_svg`; keep the icon in the
required separate `icon.svg` file. Do not put inline SVG markup in any manifest
field, including `icon_svg`, `iconSvg`, `layout.icon`, or `layout.icon_svg`.
Do not leave the generic scaffold description
`Business OS app for durable records and CTOX follow-up work`, and do not leave
only generic tags such as `business-os` and `app`. The manifest must name the
requested business domain in `title`, `description`, and at least one tag.

## App Versioning Contract

Use strict SemVer in `module.json.version` for new and runtime-installed apps.
Do not copy legacy `"v1"` from older built-in modules into a generated app.

```text
0.0.x -> UI/UX changes, non-breaking features, and bug fixes without data-shape changes
0.x.0 -> database/schema changes, migration changes, or other potentially breaking changes before public release
1.0.0 -> first release visible to users beyond the developer/founder
2.0.0 -> second independent app line that can run in parallel with 1.x legacy
x.0.0 -> every later major line is a separate app icon/module id, not an in-place overwrite
```

Practical rules:

```text
new generated app with a module-owned collection starts at 0.1.0
new generated UI-only shell with no durable schema may start at 0.0.1
never use 0.0.0
do not put dots in collection names; if a collection needs a version suffix, use a safe suffix such as v0_1_0
any schema version increase in schema.js or collections.schema.json requires a matching 0.x.0 or later minor bump and migrationStrategies
only call an app public/released/user-visible when module.json.version is >= 1.0.0 and the shell/product gate enforces that audience rule
when creating 2.0.0, choose a new module id and icon so 1.x can keep running and receiving legacy fixes
```

Current CTOX implementation facts to respect:

```text
business_module_versions exists and records whole-bundle restore points for install, edit, manual_release, rollback, and creator_deploy origins
business_module_releases exists but uses an integer release counter, not SemVer
legacy packaged modules may still have module.json version values such as v1; do not copy that pattern into new apps
the shell and App Store hide runtime-installed work versions below 1.0.0 from normal users; chef/admin and assigned founders can still see them
this is a client/catalog audience gate, not a SemVer-aware publish workflow
```

Missing CTOX hardening work before broad public app distribution:

```text
add a SemVer-aware release/publish command that records the public app version, validates migration evidence, and rejects invalid bumps
project SemVer release state into business_module_catalog from the release command instead of relying only on module.json
replace or bridge the integer business_module_releases version with SemVer metadata
add a major-line rule so 2.0.0+ requires a new module id/icon and can coexist with legacy 1.x modules
provide a CLI such as ctox business-os app release <module> --version <x.y.z> after validation is green
```

Minimum collection schema wrapper:

```json
{
  "schema_format": "ctox-business-os-module-collections-v1",
  "collections": {
    "<id>_records": {
      "version": 0,
      "primaryKey": "id",
      "type": "object",
      "properties": {
        "id": { "type": "string", "maxLength": 120 },
        "updated_at_ms": { "type": "number" }
      },
      "required": ["id", "updated_at_ms"]
    }
  }
}
```

The `schema_format` line is mandatory. Do not write this invalid shortcut:

```json
{ "collections": { "<id>_records": {} } }
```

After creating `collections.schema.json`, explicitly assert the wrapper before
writing `index.js`:

```sh
node -e "const s=JSON.parse(require('fs').readFileSync('$MODULE_DIR/collections.schema.json','utf8')); if (s.schema_format !== 'ctox-business-os-module-collections-v1') throw new Error('missing schema_format wrapper'); console.log('collections schema wrapper OK')"
```

After writing `module.json` or `collections.schema.json`, immediately parse it:

```sh
node -e "JSON.parse(require('fs').readFileSync('$MODULE_DIR/module.json','utf8')); JSON.parse(require('fs').readFileSync('$MODULE_DIR/collections.schema.json','utf8')); console.log('module JSON OK')"
```

Do not continue with `index.js` or tests while either JSON file is invalid.

For syntax checks use `node --check --input-type=module - < "$MODULE_DIR/index.js"`
or the project validator. Do not create `package.json` with `"type": "module"`
to make tests pass. Do not use `esbuild`, bundlers, transform loaders,
`node:vm`, or `new Function` to make browser ESM importable in tests.

Node tests must not import browser entry files such as `../index.js` or
`../schema.js` directly. Release-like installed-module benches may not have a
package context, so Node treats `.js` as CommonJS even though the Business OS
browser shell loads it as ESM. Put reusable schemas, command builders, reducers,
and calculations in local browser-safe `.mjs` helpers, import those helpers from
`index.js` or `schema.js`, and import the same helpers from tests:

```js
import { buildFollowUpCommand } from '../core/automation.mjs';
import { collectionSchemas } from '../core/schemas.mjs';
```

When a test reads sibling module files, derive the module root from the test
file. From `tests/*.test.mjs`, `..` is the module root and `../..` is wrong:

```js
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleRoot = resolve(testDir, '..');
const manifest = JSON.parse(readFileSync(resolve(moduleRoot, 'module.json'), 'utf8'));
const collectionSchemaDoc = JSON.parse(
  readFileSync(resolve(moduleRoot, 'collections.schema.json'), 'utf8')
);
```

Do not use `new URL('../../module.json', import.meta.url)` from a test under
`tests/`; that points outside the module directory in installed benches.

Use JSON/text parity checks for `module.json`, `collections.schema.json`,
`schema.js`, and `index.js` when needed. Never read `index.js` or `schema.js`
and import it as a `data:text/javascript;base64,...` URL. That breaks relative
imports and is not how the Business OS shell loads apps.

Tests must prove the positive current contract, not prove absence of legacy
contracts by embedding, naming, scanning for, or constructing forbidden
strings. Do not put strings such as `ctx.db.raw`, `db.raw`,
`window.dispatchEvent`, `ctox-business-os-chat-submit`, `pending_sync`,
`layout.right`, `right-resizer`, or `business_commands fallback` in generated
tests, comments, or helper names. Do not build those strings from fragments,
character codes, regex text, arrays, or helper constants to bypass validators.
Do not write tests named "does not use forbidden ...", "anti-patterns", or
similar. Validators own negative anti-pattern checks. Generated module tests
must assert positive behavior only: schema parity, reducers/calculations,
valid command payload shape, and `ctx.commandBus.dispatch` integration hooks.
For layout tests, assert the expected positive contract, for example
`manifest.layout.shell`, the left/center labels, the exact expected layout key
set, and the module root class used by the fragment/CSS. Do not write checks
or property reads such as `manifest.layout?.right`,
`manifest.layout.drawers?.right`, `!('right' in manifest.layout)`,
`!('right_resizer' in manifest.layout)`, or failure messages like "no default
right-resizer"; those literals make the test file itself fail the static
checker. A good assertion is
`assert.deepEqual(Object.keys(manifest.layout).sort(), ['center', 'left', 'shell'])`.

`module.json` must list every collection the module reads/writes. Shell collections such as `business_commands`, `ctox_queue_tasks`, `business_module_catalog`, and `ctox_runtime_settings` may be listed when used, but module-owned collections must be declared in both `schema.js` and `collections.schema.json`.

`schema.js` and `collections.schema.json` must export only module-owned collections. Do not export shell collections such as `business_commands`, `ctox_queue_tasks`, `business_module_catalog`, or `ctox_runtime_settings` from module schema files.

`schema.js` must be browser-safe ESM and export:

```js
export const collections = { /* collectionName: schema */ };
export const migrationStrategies = { /* collectionName: { version: fn } */ };
```

`collections.schema.json` must mirror the module-owned collection definitions and migration operations using the checked-in module schema format. New collections are created on the fly by the Business OS schema/module registration path when the module is loaded or the native peer registers collections. Do not create tables manually. Do not invent a separate database.

`index.js` must export `mount(ctx)` and return cleanup:

```js
export function mount(ctx) {
  const { host, db, commandBus, notifications, locale } = ctx;
  // Use ctx.db.collection("collection_name") or the facade pattern used by the existing modules.
  // Do not unwrap ctx.db.raw.
  return () => {
    // unsubscribe listeners, abort timers, remove transient handlers
  };
}
```

## UI Rules

- Start with the smallest work surface that fits the workflow. Two panes plus modals are usually better than three panes.
- Default to one or two panes plus in-module modals/drawers. Do not create `layout.right`, `layout.drawers.right`, right-drawer manifest metadata, `.right` sections, right-column resizers, or three-column CSS grids by default.
- Do not assume a shell "right rail" or inspector exists. Only use shell toolbar/event APIs after inspecting an existing module that uses that exact API; otherwise keep the action inside the module's normal one/two-pane surface.
- Use a third pane only when the user explicitly asked for one or when the workflow has a persistent separate context stream that must remain visible while editing. If you use one, add a short code comment in the module explaining that workflow need. Otherwise use a modal or drawer.
- Do not add visible controls unless they work end to end.
- Do not add AI buttons, export buttons, filters, batch actions, or settings that only change local text.
- Prefer existing shell/base classes and local module CSS. Do not define custom properties on `:root`, `html`, or `body`.
- Put module-local CSS variables on the module root class with module-local names and real fallback values, for example `.inventory-module { --inventory-bg: var(--surface, #fff); }`. Never redefine shell token names such as `--surface`, `--text`, `--line`, or `--accent`, and never write a self-referential alias such as `--inventory-bg: var(--inventory-bg)`.
- When fixing a design-token validation failure, edit the exact token declarations. Do not run broad search/replace that turns `--inventory-bg: var(--surface, #fff)` into `--inventory-bg: var(--inventory-bg)`.
- Use modals/drawers for focused create/edit flows when a third column would be decorative.
- Keep text and controls compact enough for the Business OS workspace, not a marketing page.

## Generation Discipline

- Keep the first version intentionally small: one primary list/workbench, one detail/edit flow, the required automation action, and focused tests.
- Prefer shipping a minimal valid slice over pre-solving every possible guard rule. Once the required files exist, let the validator tell you the next concrete repair.
- Do not read or paraphrase validator/static-checker source to plan around broad scanner terms before writing files. This causes analysis loops and often leaks forbidden strings into generated tests or comments.
- Do not create broad status/filter/export/settings surfaces unless the prompt asked for them and the handlers are implemented.
- For App Creator/runtime-installed apps, use vanilla DOM and browser APIs only. A local `.mjs` helper is fine; a framework runtime or generated bundle is not.
- Do not write negative source-scanner tests for forbidden legacy patterns. The validator already performs that role. If validation reports forbidden terms in a test file, remove the negative scanner test instead of splitting or reconstructing the terms.
- Do not write negative layout/data/dependency absence assertions in generated tests. If validation reports a forbidden term in a test assertion, comment, name, or failure message, delete that negative assertion and replace it with a positive contract assertion.
- Tests are required app artifacts, not optional documentation. Preserve `tests/*.test.mjs`; if you regenerate domain helpers, regenerate positive helper tests in the same turn and verify `rg --files "$MODULE_DIR/tests"` returns at least one test before final validation.
- For generated App Creator tests, avoid direct `assert.deepEqual(summarizeRecords(...), {...})` on aggregate helpers. Assign `const summary = summarizeRecords(...)` and assert named fields, or assert a full exact object that includes every helper-returned key.
- Keep helper exports and all local named imports in lockstep. Every named local import in `index.js`, `core/*.mjs`, and `tests/*.mjs` must exist as a real `export` in the target file. Preserve scaffold helper exports such as `COLLECTION_NAME`, `createRecord`, `normalizeStatus`, `summarizeRecords`, and `visibleRecords` unless every importer is updated in the same turn.
- For App Creator scaffold modules, prefer keeping the scaffold API stable and changing its implementation: `buildFollowUpCommand` may build the domain-specific follow-up or chat task even when the visible button label is "Create stock action", "Request renewal review", or similar. Rename the export only when `index.js` and all tests are updated in the same edit and validation is rerun.
- Keep the automation command-builder facade in `core/automation.mjs`. If you add domain helpers such as `needsAttention`, `budgetStatus`, `billingReadiness`, `milestoneTotals`, or risk classifiers, define and export them from `core/records.mjs` first, import only existing exports in `core/automation.mjs`, and keep `buildFollowUpCommand(record = {})` exporting the final `business_os.chat.task` command for `index.js` and tests.
- Keep first-pass actions boring. Reuse the scaffold action names `new`, `delete`, and `follow-up` plus the form submit flow. If you add any other `data-action`, update `index.js` in the same edit with an exact branch and real persistence/dispatch behavior, or remove the control before validation.
- Do not add a replacement test file and leave the scaffold test behind in a broken state. Validation runs every `tests/*.test.mjs`; stale tests with missing named imports, obsolete helper names, or wrong fixture expectations are real failures.
- Never delete required app artifacts as part of a replacement workflow. Keep `module.json`, `collections.schema.json`, `schema.js`, `index.html`, `index.css`, `index.js`, `icon.svg`, locales, core helpers, and tests present after every edit. If a command would make the module invalid between steps, use a smaller exact edit instead.
- Never create an alternate root schema file such as `schema.mjs` or `schema.cjs`. If tests need ESM helpers, keep those helpers under `core/*.mjs`; `schema.js` remains the only root schema module.
- Automation tests must assert concrete fixture facts. A passing test that checks only `type`, `command_type`, or "has record_snapshot" is too weak; also assert that title/instruction/prompt and `record_snapshot` include the selected records, counts, statuses, due dates, or amounts from the fixture.
- Avoid huge single tool calls. If a file grows large enough to risk malformed tool-call JSON, reduce scope first; do not stream it as a giant shell `printf`, `echo`, `tee`, or `cat` payload. For first-pass runtime-installed apps, keep the helper surface to `core/records.mjs` and `core/automation.mjs`; do not add `core/ui.mjs`, `core/render.mjs`, `core/runtime.mjs`, `core/panel.mjs`, or similar extra layers to make a small app feel architectural.
- Do not use Python, base64, Node one-off writer scripts, data URLs, `/tmp` scratch files copied into the module, `/tmp/*.patch` files, shell `apply_patch` wrappers, temporary generated file-copy wrappers, or `sed -i`/`perl -pi` line surgery to recover from malformed shell writes. When a direct write is fragile, reduce the app scope, split the file into a smaller local `.mjs` helper, or rewrite the affected bounded helper/file directly with simpler literals.
- Do not dump the whole generated module back into context, including by consecutive `sed -n` ranges that reconstruct a full file, `head`/`tail` requests for large chunks, or Node `fs.readFileSync` scripts that print generated source/JSON/HTML/CSS back with `console.log`. Do not run `wc` over generated artifacts or inspect several generated files in one command. Use `sed -n` or `head` only for a small line range tied to one exact selector/import/error, `rg -n` for one exact selector/import, and the validator report for repair targets. Full-file `cat` loops, Node file dumps, file-by-file scaffold rereads, broad `head`/`tail`, multi-file grep/sed/wc audits, and broad source scans are App Creator failure patterns.
- Do not keep working after the app-specific validator is green. The final action after a green validator is the final response; do not backfill missed few-shot/context inspection, inspect checker source, search prior bench apps, run extra repo-wide checks, or do cosmetic rewrites. Scope creep after a green validator is a bench failure even when the app itself is valid.
- Do not inspect shell aliases, `apply_patch` wrappers, or write temporary probe files to test the harness. Trust the target block and validator.
- Do not copy the skill's forbidden tool/dependency names into app comments or test comments. The static checker treats generated-file literals as violations.

## New App Finalization Checklist

Before saying the app is done, create and keep a short phase tracker current.
For source modules, use `docs/business-os-<id>-implementation-plan.md`. For
runtime-installed modules, keep the tracker in the active task/bench notes or
final response; do not add Markdown status files to the installed module
directory.

Use this checklist exactly. Mark each item `done`, `rework`, `blocked`, or
`deferred with reason`, and repair every `rework` item before final handoff:

```text
phase 0 target: resolved module directory is correct; runtime-installed apps use runtime/business-os/installed-modules/<id>, not src/apps/business-os/installed-modules
phase 1 few-shots: inspected at least 3 existing modules and copied only concrete proven patterns
phase 2 scope: app has one focused workbench, one create/edit/detail flow, one automation; no decorative views or fake future controls
phase 3 manifest: module.json parses, id/entry/install_scope are correct, collections lists every read/write dependency, description names the requested business domain, and tags include at least one domain tag beyond business-os/app/ctox/module/records
phase 4 versioning: new/runtime-installed module.json uses SemVer x.y.z without v prefix; 0.1.0 is the normal initial data-app version; <1.0.0 remains developer/founder/admin-only in shell/App Store; public/user-visible release requires >=1.0.0; 2.0.0+ is a new module id/icon line
phase 5 schema: collections.schema.json has schema_format ctox-business-os-module-collections-v1, contains only module-owned collections, and matches schema.js
phase 6 persistence: all durable records use ctx.db facade collections; no ctox.db, db.raw, Web Storage, HTTP data route, table creation, or manual database file
phase 7 automation: at least one visible action dispatches a real `business_os.chat.task` command through ctx.commandBus.dispatch and has a testable payload builder
phase 8 UI layout: default is one/two panes plus an in-module modal or drawer; no layout.right, layout.drawers.right/right-drawer manifest metadata, right rail, right-column CSS, right resizer, or three-column grid unless explicitly justified by workflow
phase 9 UI controls: every visible button, filter, tab, menu, and form action has a real handler and state/persistence/dispatch effect; every `data-action="..."` declared in index.html has an exact click handler, action branch, or action-map key in index.js; every `data-*` selector queried in index.js exists in index.html or generated markup
phase 9a mount file: index.js exists, exports mount(ctx), attaches ./index.css through new URL, fetches ./index.html through new URL, assigns ctx.host.innerHTML before DOM wiring, imports only existing local helpers, and was not deleted/renamed/moved/replaced during repair
phase 10 CSS: module CSS is scoped under the module root class; no :root/html/body custom property definitions, shell token redefinitions, self-referential custom properties, decorative resize handles, or layout affordances copied from unrelated modules
phase 11 dependencies: browser runtime uses only vanilla HTML/CSS/browser ESM and local relative ESM imports; no UI framework, JSX/TSX, package manager, bare package import, remote import, CommonJS, bundler, transpiler, or generated bundle
phase 12 tests/imports: tests import only local `.mjs` helpers and JSON/text files; every named local import in `index.js`, `core/*.mjs`, and `tests/*.mjs` is exported by that target file; first-pass runtime apps use only `core/records.mjs` and `core/automation.mjs` unless a post-validator rework has a concrete reason; scaffold export names such as buildFollowUpCommand, summarizeRecords, and visibleRecords are preserved unless every importer was updated; `core/automation.mjs` still exports `buildFollowUpCommand(record = {})` as the command-builder facade with both `type` and `command_type`, and it imports only helpers that `core/records.mjs` really exports; tests do not import `index.js`/`schema.js` directly, do not use data: URLs, do not contain forbidden anti-pattern literals as negative assertions/messages, and cover schema parity, core command builders, and at least one CRUD/automation path; automation assertions include concrete fixture facts in title/instruction/prompt and record_snapshot; fixture totals/counts are hand-computed in comments or small named facts and are consistent with the implementation; scaffold core/locales/tests still exist
phase 13 validation: focused node checks, `ctox business-os app validate <id> --installed|--source`, forbidden-pattern scan when needed for a concrete validator bullet, catalog/queue finalization from the CTOX command path when applicable, and any available shell/browser smoke proof are green; after the app-specific validator is green and CTOX finalization succeeds, stop instead of running source-wide conformance scripts or polishing
phase 14 cleanup: no unexpected installed-module root entries remain; runtime-installed module root contains only module.json, collections.schema.json, schema.js, index.html, index.css, index.js, icon.svg, core/, locales/, and tests/; no root-level artifacts, source-installed app artifacts, probe files, blocker notes, generated bundles, package files, stale phase rows, temporary schema/manifest files, literal wildcard/brace filenames, or stale failing replacement tests remain
```

Treat these common findings as automatic rework, not acceptable tradeoffs:

```text
module.json has layout.right or layout.drawers.right without layout.third_pane_justification
module.json embeds layout.icon_svg instead of using icon.svg
index.html/index.css contains data-*-right, right-pane, right-column, right-resizer, or a three-column grid copied from another app without a real workflow need
collections.schema.json starts directly with collections and omits schema_format
index.css contains self-referential custom properties such as `--module-bg: var(--module-bg)`, or broad token replacement has removed shell-token fallbacks
module.json for a new/runtime-installed app has no SemVer version, uses legacy v1, uses 0.0.0, or claims public release below 1.0.0
module.json keeps the generic scaffold durable-records/follow-up description or only generic tags such as business-os/app instead of requested-domain tags
version 2.0.0 or later is used without a new module id/icon for a parallel major app line
runtime-installed App Creator app uses React/Vue/Svelte/Angular/Solid/Preact/Lit, JSX/TSX, framework config, or any compile/transpile artifact
schema.js or collections.schema.json redeclares business_commands
automation helper sets only type and omits command_type, or omits record_snapshot
automation helper hides the required App Creator command shape behind an untested custom payload; tests must prove a builder returns business_os.chat.task with command_type and record_snapshot
tests import index.js/schema.js directly, or load them through data:text/javascript/base64, instead of testing shared `.mjs` helpers plus JSON/text parity
module directory contains temporary or literal pattern artifacts such as .tmp_schema.json, .csjson.tmp, co*ions.*json, colle{ctions,ctions}.schema.json, module*.json, or collections*.json
runtime-installed module root contains any non-canonical direct child such as m, modul.json, scratch manifests, notes/status Markdown, copied app folders, generated bundles, package files, or ad hoc helper directories outside core/, locales/, and tests/
module directory contains root-level schema.mjs or schema.cjs as an alias, migration workaround, or replacement for schema.js
any required artifact is deleted during a repair step, even briefly, and validation is not rerun after restoring it
new tests pass but an older tests/*.test.mjs still imports a missing helper export or asserts stale scaffold behavior
the phase tracker says done while validator or browser proof is red
```

## Persistence Pattern

Use the database handles from `ctx`. Typical module operations:

```js
const records = ctx.db.collection('my_records');
const docs = await records.find().sort({ updated_at_ms: 'desc' }).exec();
await records.upsert({ id, title, status, updated_at_ms: Date.now() });
```

Follow exact methods used by the inspected modules if the facade exposes a slightly different helper. Do not guess an upstream RxDB API that is not used locally.

Do not use `localStorage` or `sessionStorage`, even for UI preferences. Use
transient in-memory state, module-owned collections for persisted business
state, or a shell-provided settings/state API only after verifying that exact
pattern in an existing Business OS module.

For new module-owned data:

```text
1. Choose stable snake_case collection names scoped to the domain.
2. Add those names to module.json collections.
3. Declare JSON schemas in schema.js.
4. Mirror them in collections.schema.json.
5. Add migrationStrategies for any schema version above the current one.
6. Seed demo data only through app code/tests with unique ids, never as fixed colliding rows.
```

## Automation Pattern

Every new app should include at least one real automation action that creates a normal CTOX work/chat/ticket item. The action builds a command object and dispatches it through `ctx.commandBus.dispatch`.

For new App Creator/runtime-installed apps, this means `ctx.commandBus.dispatch`
only. Do not write, insert, upsert, or fallback directly to the
`business_commands` collection. Do not call `ctx.db.collection('business_commands')`
from app runtime code. Local tests must not describe direct `business_commands`
writes or fallbacks as valid behavior.

Command shape:

```js
await ctx.commandBus.dispatch({
  id: `cmd_${crypto.randomUUID()}`,
  module: '<id>',
  type: 'business_os.chat.task',
  command_type: 'business_os.chat.task',
  record_id: record.id,
  inbound_channel: '<id>',
  payload: {
    title: 'Concrete follow-up title',
    instruction: 'Concrete task with record facts and expected output.',
    prompt: 'Concrete task with record facts and expected output.',
    module_id: '<id>',
    collection: '<owned_collection>',
    record_id: record.id,
    record_snapshot: record,
    outbound_channel: 'business_os_chat',
    response_channel: 'business_os_chat',
    source_module: '<id>',
  },
  client_context: {
    source: '<id>-module',
    surface: '<id>.<actionName>',
    module: '<id>',
  },
});
```

The command must be connected to a real UI action and tested at least as a pure builder function when browser automation is not available.
For App Creator/runtime-installed modules, the automation must preserve both
`type: 'business_os.chat.task'` and
`command_type: 'business_os.chat.task'`, include a `record_snapshot`, and be
dispatched by runtime UI through `ctx.commandBus.dispatch`.

## Validation

Repair failures in this order. Do not spend time on later items while an
earlier layer is still red:

```text
1. exact target directory and no root/off-target artifacts or unexpected installed-module root entries
2. valid module.json and collections.schema.json JSON
3. source vs installed manifest fields: entry and install_scope
4. required file set, including index.css, locales, icon, tests
5. collection ownership: shell collections only in module.json, owned schemas in both schema files
6. UI contract: no default layout.right, right pane, right resizer, or decorative third pane
7. runtime dependency contract: local browser ESM only, no remote URL, no bare package, no Node runtime import
8. index.js ESM syntax
9. no-dependency module tests that import `.mjs` helpers, not browser `.js` entrypoints
10. real-shell smoke proof when available
```

For generated module tests, make the expected values auditable before using
them as a gate. Small aggregate tests must name or comment the hand-computed
facts that produce each count or total, for example which records are
over-budget, invoice-ready, follow-up-required, deleted, or excluded. If a test
fails, first check whether the expected value contradicts the fixture and the
business rule. Fix the app logic when the implementation violates the rule; fix
the test when the test expectation is mathematically inconsistent. Never leave
a validator red because a generated self-test expects impossible counts.

Run the most specific checks available:

```sh
ctox business-os app validate <id> --installed   # App Creator / runtime installed-modules targets
ctox business-os app validate <id> --source      # source modules under src/apps/business-os/modules
node --test src/apps/business-os/modules/<id>/**/*.test.mjs
node src/apps/business-os/scripts/assert-module-conformance.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <id>
```

When validating an installed runtime module outside `modules/`, adapt the path or run a targeted test plus a static forbidden-pattern scan:

```sh
ctox business-os app validate <id> --installed
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <id> --installed
rg -n "ctx\\.db\\.raw|ctox\\.db|indexedDB|/rxdb/pull|/commands|fetch\\(|require\\(|package\\.json|node_modules" <module-dir>
```

If a guard fails, the guard is right. Fix the module. Do not weaken the guard and do not patch generated dist bundles directly.

Do not run `npx`, `npm install`, `npm test`, `esbuild`, Vite, Rollup, Webpack,
or any package-managed test/build command for a new app module. Tests must use
Node built-ins plus local ESM files that already ship with the module or shell.
Do not import a bundler in tests and do not use a green bundling result as
readiness proof.

Do not leave repair artifacts in the module directory:

```text
*.md except README.md
*.bak
*.orig
*.rej
*.tmp
*.bundle.js
*.bundle.mjs
_probe_*
harness-*
harness_*
*artifact-conflict*
*artifact-status*
*blocker*
*probe*
test-*
*-test.*
node_modules/
package.json or lockfiles
```

Never create a workspace-root alias, symlink, or hardlink to satisfy a check that expects `module.json` or `collections.schema.json` at the root. The check is wrong for Business OS app modules; the module manifest belongs only in the resolved module directory.

If `ctox business-os app validate`, `module_static_check.mjs`, or `validate-app-module.mjs` reports failures,
copy its bullets into your repair checklist and address them exactly. Do not
claim completion because a separate custom test passes while the static checker
is red.

## Done Means

The work is only done when all are true:

```text
3 existing modules were inspected and influenced the implementation
the deliverable is a real Business OS module in the correct module directory
module.json, schema.js, collections.schema.json, index.html, index.js, locales, and icon are coherent
all visible actions work and persist or dispatch through the Business OS contract
at least one automation creates a normal CTOX command/chat/ticket flow
no dependency manager or non-ESM dependency was added
no forbidden data path appears
tests and guards pass or any remaining blocker is reported with exact command output
no skill file, skill trace, or skill-named workspace directory was created as the app deliverable
```
