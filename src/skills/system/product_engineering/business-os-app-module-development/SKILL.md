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
you are about to add package.json for any reason, npm/pnpm/yarn, node_modules, lockfiles, a bundler, CommonJS require, or CDN dependency management
you are about to use esbuild, Vite, Rollup, Webpack, node:vm, or new Function as a syntax-check, import, schema-transform, or test workaround
you are about to mention forbidden package-manager, bundler, or dependency names inside generated app files, tests, comments, or user-visible copy; keep those names only in validation/skill context
you are about to use IndexedDB directly, localStorage, sessionStorage, Postgres, SQLite from browser code, ctox.db, ctx.db.raw, HTTP data APIs, /rxdb/pull, /commands, or any fallback data path
you are about to write app files outside the resolved module directory, such as root-level module.json, root-level collections.schema.json, root-level <id>/, src/skills/, or any skill-named path
you believe a harness, artifact contract, benchmark note, or review example requires root-level module.json, root-level collections.schema.json, root-level harness-module.json, root-level harness-collections.schema.json, root-level artifact/status/blocker Markdown, or any other root alias for an app deliverable
you are about to test the guard by creating, moving, touching, symlinking, hardlinking, copying, or removing root-level app artifact probe files such as `test-*`, `_test_*`, `_probe_*`, `probe-*`, root `module.json`, root `collections.schema.json`, or guard/status scratch files
you are about to probe shell aliases, tool wrappers, guard behavior, or temporary root write behavior instead of implementing the app in the allowed module directory
the module has a visible button/action with no real handler, persistence change, automation command when relevant, and test or smoke assertion
the module declares collections in module.json but not in schema.js and collections.schema.json
the module-owned data model is unclear: central object, collection names, states, commands, and automation payload are not named
the app has a decorative third pane, layout.right by default, right-column resizers by default, decorative controls, fake AI buttons, fake status-only actions, or UI that is not needed for the workflow
module.json or collections.schema.json would be exposed in an invalid or incomplete state after any edit
you are about to write a very large app file as one huge tool-call argument or here-doc; keep generated files concise and split large writes into bounded chunks
you are about to patch a large generated JavaScript file with fragile line-number sed edits instead of rewriting the relevant bounded helper/file
you are about to make a failing test match broken behavior instead of fixing the app contract violation it exposed
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

For more details, read the relevant bundled reference before coding:

- `references/business-os-app-architecture-porting.md` for the concise architecture translation layer.
- `references/architecture-translation.md` for a fuller porting guide across source checkouts and regular release installs.
- `references/module-contract.md` for exact module file, schema, registry, and install-scope rules.
- `references/verification.md` for validation and forensic checks.

## Module File Shape

For a module id `<id>`, create or edit only the module directory:

```text
modules/<id>/                 # core/starter/store source module
installed-modules/<id>/       # runtime-created installed module
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
install_target = runtime-installed-module -> src/apps/business-os/installed-modules/<id>/
install_target = source-module or core/starter/store source -> src/apps/business-os/modules/<id>/
```

When `install_target` is `runtime-installed-module`, all generated app files must be under `src/apps/business-os/installed-modules/<id>/`. Do not create `<repo-root>/module.json`, `<repo-root>/collections.schema.json`, `<repo-root>/<id>/`, or any directory outside the resolved target. If the target directory is unclear, stop and ask or inspect the App Creator/install code; do not guess.

Do not create root aliases such as `<repo-root>/harness-module.json`,
`<repo-root>/harness-collections.schema.json`, `<repo-root>/artifact-status.md`,
`<repo-root>/harness-artifact-status.md`, `<repo-root>/<id>-module.json`, or
`<repo-root>/<id>-collections.schema.json`. They are not compatibility files;
they are wrong-path app artifacts. Do not probe this rule by creating temporary
root files and deleting them.

Shell tools often run with the repository or release installation root as the current working directory. That directory is not the module directory. Never use bare redirects such as `> module.json`, `> collections.schema.json`, `> <id>/index.js`, or `mkdir <id>`.

Use this write pattern for runtime-installed modules:

```sh
MODULE_DIR="src/apps/business-os/installed-modules/<id>"
mkdir -p "$MODULE_DIR/locales" "$MODULE_DIR/tests"
# Every generated file write must target "$MODULE_DIR/<file>".
```

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
  "entry": "installed-modules/<id>/index.html",
  "install_scope": "installed",
  "collections": ["business_commands", "<id>_records"],
  "layout": { "shell": "full-workspace", "left": "List", "center": "Details" }
}
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

After writing `module.json` or `collections.schema.json`, immediately parse it:

```sh
node -e "JSON.parse(require('fs').readFileSync('$MODULE_DIR/module.json','utf8')); JSON.parse(require('fs').readFileSync('$MODULE_DIR/collections.schema.json','utf8')); console.log('module JSON OK')"
```

Do not continue with `index.js` or tests while either JSON file is invalid.

For syntax checks use `node --check --input-type=module - < "$MODULE_DIR/index.js"`
or the project validator. Do not create `package.json` with `"type": "module"`
to make tests pass. Do not use `esbuild`, bundlers, transform loaders,
`node:vm`, or `new Function` to make browser ESM importable in tests. If a test
needs shared schema or logic, move that logic into a local browser-safe `.mjs`
helper and import it normally from both runtime and tests.

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
- Avoid huge single tool calls. If a file grows large enough to risk malformed tool-call JSON, reduce scope first; otherwise write it in bounded chunks and immediately run syntax checks.
- Do not inspect shell aliases or write temporary probe files to test the harness. Trust the target block and validator.
- Do not copy the skill's forbidden tool/dependency names into app comments or test comments. The static checker treats generated-file literals as violations.

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
9. no-dependency module tests
10. real-shell smoke proof when available
```

Run the most specific checks available:

```sh
node --test src/apps/business-os/modules/<id>/**/*.test.mjs
node src/apps/business-os/scripts/assert-module-conformance.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <id>
```

When validating an installed runtime module outside `modules/`, adapt the path or run a targeted test plus a static forbidden-pattern scan:

```sh
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

If `module_static_check.mjs` or `validate-app-module.mjs` reports failures,
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
