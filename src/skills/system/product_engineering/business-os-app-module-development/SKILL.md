---
name: business-os-app-module-development
description: Use whenever CTOX or Business OS must build, modify, repair, install, review, or generate a Business OS app/module from chat, App Creator, App Store, CLI, inbound communication, or an external agent. Requires reading existing Business OS apps first, using the CTOX DB WebRTC data plane, and shipping a runnable no-build ESM module rather than a plan, skill file, or generic web app.
metadata:
  short-description: Build production-ready CTOX Business OS app modules with the native app, data, automation, and validation contracts
cluster: product_engineering
---

# Business OS App Module Development

This skill is instruction context. It is not the deliverable.

If the user asks to build, change, or repair a CTOX Business OS app, build the app/module. Do not create, copy, mirror, export, or edit skill files or skill-named directories unless the user explicitly asks to change a skill.

## Non-Negotiable Contract

Stop and report the blocker instead of coding when any hard stop is active:

```text
you have not inspected at least 3 existing Business OS modules for concrete patterns
you are about to create a skill file, skill trace, harness trace, README-only deliverable, or plan-only deliverable
you are about to build a generic Next.js/React/Vanilla app outside the Business OS module contract
you are about to use React, Vue, Svelte, Angular, Solid, Preact, Lit, JSX/TSX, a component framework, a framework runtime, or a compile/transpile step for a generated App Creator app
you are about to add package.json for any reason, npm/pnpm/yarn, node_modules, lockfiles, a bundler, CommonJS require, or CDN dependency management
you are about to use esbuild, Vite, Rollup, Webpack, node:vm, or new Function as a syntax-check, import, schema-transform, or test workaround
you are about to mention forbidden package-manager, bundler, or dependency names inside generated app files, tests, comments, or user-visible copy; keep those names only in validation/skill context
you are about to use IndexedDB directly, localStorage, sessionStorage, Postgres, SQLite from browser code, ctox.db, ctx.db.raw, HTTP data APIs, /rxdb/pull, /commands, or any fallback data path
you are about to write app files outside the resolved module directory, such as root-level module.json, root-level collections.schema.json, root-level <id>/, src/skills/, or any skill-named path
you are about to create or update a runtime-installed App Creator module whose module.json lacks a SemVer version in x.y.z form without a v prefix
you are about to expose, advertise, or call a module public/user-ready while its app version is below 1.0.0
you are about to use 2.0.0 or any later x.0.0 as an in-place update of the same app id/icon instead of a new parallel app line
you are about to call `ctox queue ack`, `ctox queue complete`, `ctox queue release`, `ctox queue fail`, `ctox queue block`, or edit queue/command/runtime-status rows directly; CTOX service owns lifecycle completion
you are about to let `current_queue_item_id`, an open-work block, or an unrelated queue row redirect the app build away from the authoritative module_id and only_allowed_app_artifact_directory
you believe a harness, artifact contract, benchmark note, or review example requires root-level module.json, root-level collections.schema.json, root-level harness-module.json, root-level harness-collections.schema.json, root-level artifact/status/blocker Markdown, or any other root alias for an app deliverable
you are about to test the guard by creating, moving, touching, symlinking, hardlinking, copying, or removing root-level app artifact probe files such as `test-*`, `_test_*`, `_probe_*`, `probe-*`, root `module.json`, root `collections.schema.json`, or guard/status scratch files
you are about to probe shell aliases, tool wrappers, guard behavior, or temporary root write behavior instead of implementing the app in the allowed module directory
the module has a visible button/action with no real handler, persistence change, automation command when relevant, and test or smoke assertion
the module declares collections in module.json but not in schema.js and collections.schema.json
the module-owned data model is unclear: central object, collection names, states, commands, and automation payload are not named
the app has a decorative third pane, layout.right by default, right-column resizers by default, decorative controls, fake AI buttons, fake status-only actions, or UI that is not needed for the workflow
module.json embeds `layout.icon_svg` instead of using the required separate `icon.svg`
module.json or collections.schema.json would be exposed in an invalid or incomplete state after any edit
any required module file is still missing: module.json, collections.schema.json, schema.js, index.html, index.css, index.js, icon.svg, locales/de.json, locales/en.json, or tests/*.test.mjs
the validator reports missing files, right/third-pane layout, schema, manifest, dependency, syntax, or test failures and you are about to finish instead of repairing the exact bullets
you are about to write a very large app file as one huge tool-call argument or here-doc; keep generated files concise and split large writes into bounded chunks
you are about to patch a large generated JavaScript file with fragile line-number sed edits instead of rewriting the relevant bounded helper/file
you are about to make a failing test match broken behavior instead of fixing the app contract violation it exposed
you are about to import browser entry files such as `index.js` or `schema.js` directly from Node tests, or through `data:text/javascript`, base64, `Buffer.from(source)`, or any generated data URL, instead of testing local `.mjs` helpers and JSON/text parity
tests and Business OS guards were not run after the last code change
```

Never say "done", "ready", "production-ready", or "runs" while any hard stop is active.

## Required First Steps

1. Identify the Business OS app root. In a source checkout this is usually `src/apps/business-os`. In a regular release it is the shipped Business OS app root. Use local files first; use the GitHub source only when local source is unavailable.
2. Read the core contracts before coding:
   - `docs/ctox-rxdb.md`
   - `src/apps/business-os/README.md`
   - `src/apps/business-os/RXDB_SYNC_CONTRACT.md`
   - `src/apps/business-os/scripts/assert-module-conformance.mjs`
   - `src/apps/business-os/scripts/assert-rxdb-only.mjs`
3. Inspect at least 3 existing modules with `module.json`, `collections.schema.json`, `schema.js`, `index.html`, `index.js`, tests, and locales. Good default few-shots:
   - `modules/notes` for simple CRUD, shell collections, local ESM, and tests
   - `modules/customers` for customer-linked records and richer schemas
   - `modules/shiftflow` for planning workflows, dates, two-pane work surfaces, and command dispatches
   - `modules/outbound` for automation/command payload patterns
   - `modules/creator` and `modules/app-store` for app creation/install flows
4. Write down a tiny analogue map before implementation:

```text
Requested domain object -> existing module object pattern
Requested primary list/workbench -> existing module surface pattern
Requested detail/edit flow -> modal/drawer/pane pattern
Requested automation -> existing business_commands pattern
Collections to own -> schema.js and collections.schema.json names
What not to implement because it would be slop
How to keep this app small enough to build and verify in one pass
```

5. Resolve the target directory and write a required-file inventory before any
   optional UI or polish:

```text
MODULE_DIR=<resolved target from the prompt>
required files: module.json, collections.schema.json, schema.js, index.html,
index.css, index.js, icon.svg, locales/de.json, locales/en.json,
tests/<id>.test.mjs
first repair action if red: create missing required files, then remove any
unjustified right/third pane, then rerun validation
```

Do not use queue IDs or open-work context as a target selector. The App
Creator/CTOX service will complete queue and command state after the app
validator is green.

## Architecture Translation Layer

Use this mapping when your instincts suggest a familiar web stack:

```text
Next.js route/page -> Business OS module index.html plus index.js mount(ctx)
React component tree -> direct browser ESM DOM rendering or existing local module pattern
API route/server action -> business_commands document dispatched through ctx.commandBus
Postgres table/migration -> module-owned collection declared in schema.js and collections.schema.json
Prisma/Drizzle model -> JSON schema collection with version and migrationStrategies
IndexedDB/localStorage/sessionStorage app store -> CTOX DB collection supplied by ctx.db, or transient in-memory state for non-persistent UI state
HTTP fetch to backend -> not allowed for Business OS data; use RxDB/WebRTC and command bus
npm dependency -> not allowed; use browser APIs, shipped vendor ESM, or local ESM modules
background job -> CTOX queue task created through business_commands
ticket/chat automation -> business_commands with outbound_channel/response_channel business_os_chat
```

CTOX DB is CTOX-owned and RxDB-derived. It is not npm `rxdb`, not `ctox.db`, and not a generic IndexedDB wrapper. Business data persists through the Business OS shell context and replicates WebRTC-only to the CTOX native peer. Do not add HTTP fallbacks.

Generated App Creator apps must be vanilla static Business OS modules:

```text
index.html -> static fragment loaded by the shell
index.css -> scoped module CSS
index.js -> browser ESM exporting mount(ctx)
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

Write `module.json` and `collections.schema.json` atomically. Build the full JSON
content first, write it to a temporary file inside `MODULE_DIR`, parse and check
the critical fields, then move it into place. Never expose an invalid final
`module.json` or `collections.schema.json`; the Business OS shell/catalog sync
may read it immediately.

Minimum installed-module manifest fields:

```json
{
  "id": "<id>",
  "version": "0.1.0",
  "entry": "installed-modules/<id>/index.html",
  "install_scope": "installed",
  "collections": ["business_commands", "<id>_records"],
  "layout": { "shell": "full-workspace", "left": "List", "center": "Details" }
}
```

This is also a negative example: do not add `"right": "Details"`,
`"right": "Inspector"`, or any `layout.right` unless the user explicitly asked
for a persistent third pane and you also add `layout.third_pane_justification`.
The default Business OS app surface is one or two panes plus a modal/drawer.
Do not embed SVG in `module.json.layout.icon_svg`; keep the icon in the
required separate `icon.svg` file.

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

Use JSON/text parity checks for `module.json`, `collections.schema.json`,
`schema.js`, and `index.js` when needed. Never read `index.js` or `schema.js`
and import it as a `data:text/javascript;base64,...` URL. That breaks relative
imports and is not how the Business OS shell loads apps.

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
- Default to one or two panes plus modals/drawers. Do not create `layout.right`, `.right` sections, right-column resizers, or three-column CSS grids by default.
- Do not assume a shell "right rail" or inspector exists. Only use shell toolbar/event APIs after inspecting an existing module that uses that exact API; otherwise keep the action inside the module's normal one/two-pane surface.
- Use a third pane only when the user explicitly asked for one or when the workflow has a persistent separate context stream that must remain visible while editing. If you use one, add a short code comment in the module explaining that workflow need. Otherwise use a modal or drawer.
- Do not add visible controls unless they work end to end.
- Do not add AI buttons, export buttons, filters, batch actions, or settings that only change local text.
- Prefer existing shell/base classes and local module CSS. Do not redefine shell tokens on `:root`.
- Use modals/drawers for focused create/edit flows when a third column would be decorative.
- Keep text and controls compact enough for the Business OS workspace, not a marketing page.

## Generation Discipline

- Keep the first version intentionally small: one primary list/workbench, one detail/edit flow, the required automation action, and focused tests.
- Do not create broad status/filter/export/settings surfaces unless the prompt asked for them and the handlers are implemented.
- For App Creator/runtime-installed apps, use vanilla DOM and browser APIs only. A local `.mjs` helper is fine; a framework runtime or generated bundle is not.
- Avoid huge single tool calls. If a file grows large enough to risk malformed tool-call JSON, reduce scope first; otherwise write it in bounded chunks and immediately run syntax checks.
- Do not inspect shell aliases or write temporary probe files to test the harness. Trust the target block and validator.
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
phase 3 manifest: module.json parses, id/entry/install_scope are correct, collections lists every read/write dependency
phase 4 versioning: new/runtime-installed module.json uses SemVer x.y.z without v prefix; 0.1.0 is the normal initial data-app version; <1.0.0 remains developer/founder/admin-only in shell/App Store; public/user-visible release requires >=1.0.0; 2.0.0+ is a new module id/icon line
phase 5 schema: collections.schema.json has schema_format ctox-business-os-module-collections-v1, contains only module-owned collections, and matches schema.js
phase 6 persistence: all durable records use ctx.db facade collections; no ctox.db, db.raw, Web Storage, HTTP data route, table creation, or manual database file
phase 7 automation: at least one visible action dispatches a real business_commands/commandBus work-chat-ticket flow and has a testable payload builder
phase 8 UI layout: default is one/two panes plus modal or drawer; no layout.right, right rail, right-column CSS, right resizer, or three-column grid unless explicitly justified by workflow
phase 9 UI controls: every visible button, filter, tab, menu, and form action has a real handler and state/persistence/dispatch effect
phase 10 CSS: module CSS is scoped under the module root; no :root token definitions, shell token redefinitions, decorative resize handles, or layout affordances copied from unrelated modules
phase 11 dependencies: browser runtime uses only vanilla HTML/CSS/browser ESM and local relative ESM imports; no UI framework, JSX/TSX, package manager, bare package import, remote import, CommonJS, bundler, transpiler, or generated bundle
phase 12 tests: tests import only local `.mjs` helpers and JSON/text files; they do not import `index.js`/`schema.js` directly, do not use data: URLs, and cover schema parity, core command builders, and at least one CRUD/automation path
phase 13 validation: node --check, `ctox business-os app validate <id> --installed|--source`, forbidden-pattern scan, and any available shell/browser smoke proof are green
phase 14 cleanup: no root-level artifacts, source-installed app artifacts, probe files, blocker notes, generated bundles, package files, or stale phase rows remain
```

Treat these common findings as automatic rework, not acceptable tradeoffs:

```text
module.json has layout.right without layout.third_pane_justification
module.json embeds layout.icon_svg instead of using icon.svg
index.html/index.css contains data-*-right, right-pane, right-column, right-resizer, or a three-column grid copied from another app without a real workflow need
collections.schema.json starts directly with collections and omits schema_format
module.json for a new/runtime-installed app has no SemVer version, uses legacy v1, uses 0.0.0, or claims public release below 1.0.0
version 2.0.0 or later is used without a new module id/icon for a parallel major app line
runtime-installed App Creator app uses React/Vue/Svelte/Angular/Solid/Preact/Lit, JSX/TSX, framework config, or any compile/transpile artifact
schema.js or collections.schema.json redeclares business_commands
automation helper sets only type and omits command_type, or omits record_snapshot
tests import index.js/schema.js directly, or load them through data:text/javascript/base64, instead of testing shared `.mjs` helpers plus JSON/text parity
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

Every new app should include at least one real automation action that creates a normal CTOX work/chat/ticket item. The action writes a `business_commands` document through `ctx.commandBus` or the existing module command helper.

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
1. exact target directory and no root/off-target artifacts
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
