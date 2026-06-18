# Business OS Module Contract

Use this reference when implementing or reviewing module files.

## Required Files

A durable Business OS module normally ships the same file contract regardless
of how it is installed. In a source checkout, the files are checked in under:

```text
src/apps/business-os/modules/<module>/
  module.json
  collections.schema.json
  schema.js
  index.html
  index.css
  index.js
  icon.svg
  locales/de.json
  locales/en.json
  README.md
  tests/*.test.mjs
  core/*        optional pure domain logic
  commands/*    optional command builders
  views/*       optional DOM view modules
  vendor/*      optional browser ESM libraries copied into source
  templates/*   optional document/XML/email templates
```

In a normal release install, do not require a source checkout. App Creator,
App Store, or runtime chat flows should create/update the same file set through
`ctox.module.save` and `ctox.source.save`; the Business OS runtime materializes
those files under `installed-modules/<module>/` in the app root and loads
`entry: "installed-modules/<module>/index.html"`. Source-development prompts
may target `src/apps/business-os/modules/<module>/`; release/runtime prompts
must not mention developer-local paths such as `/Users/.../ctox.nosync`.

CTOX-native App Creator proof is the primary target for generated user apps.
External agents should build the same contract, but their output is not
production evidence until the CTOX App Creator or command flow can materialize,
validate, and mount the runtime-installed module from the installed app root.

The Business OS shell imports `index.js` and calls `mount(ctx)`. It does not
automatically inject a generated module's `index.html` or `index.css` into the
workspace. Runtime-installed App Creator modules must therefore do this before
any DOM query or event wiring:

```js
export async function mount(ctx) {
  attachStylesheetOnce();
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  // Now wire ctx.host selectors, data subscriptions, persistence, and actions.
}
```

Attach `index.css` through a local stylesheet URL such as
`new URL('./index.css', import.meta.url)`. A module that only calls
`host.querySelector(...)` against elements declared in `index.html` without
first assigning `ctx.host.innerHTML` will mount as a blank app even when all
files exist and `index.html` returns 200.

For runtime-installed App Creator modules, make the template and stylesheet
loading boring and literal:

```js
function attachStylesheetOnce() {
  const href = new URL('./index.css', import.meta.url).href;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

export async function mount(ctx) {
  attachStylesheetOnce();
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  // Wire selectors only after this line.
}
```

Do not hide the template load inside a wrapper, use `fetch('./index.html')`,
or add any other runtime network fetch. The installed-module validator allows
only `fetch(new URL('./index.html', import.meta.url))` for the local fragment.

Because `index.html` is inserted into the already-running Business OS shell, it
must be a fragment. Do not create a full browser document. The file must not
contain `<!doctype>`, `<html>`, `<head>`, `<body>`, `<link>`, `<script>`,
`<meta>`, `<title>`, or `<style>`. Relative head resources from an injected
document are resolved against `/business-os/` and create real browser failures
such as `/business-os/index.css` 404s.

`module.json` must list every collection read or written, including
dependencies such as `business_commands`, `customer_accounts`, `desktop_files`,
or adjacent module collections.

`collections.schema.json` should declare every collection from
`module.json.collections` except collections that the shell pre-registers.
Shell-registered collections are:

```text
business_module_catalog
ctox_runtime_settings
business_commands
ctox_queue_tasks
```

List shell-registered collections such as `business_commands` in
`module.json.collections` so the dependency is explicit, but do not redeclare
their schema in a greenfield module unless the task explicitly changes that
core collection contract and includes the required migration metadata.

Peer-module dependencies are different. If a module truly reads
`customer_accounts`, `customer_opportunities`, or another collection owned by a
different app and lists it in `module.json.collections`, then
`collections.schema.json` must declare an identical schema definition so
`schema-parity` passes. If that is unnecessary for the first durable slice,
prefer storing plain text references such as `customer_name` and defer the
cross-module dependency instead of inventing a divergent schema.

Run this guard after editing `collections.schema.json` for a normal module:

```sh
node -e "const fs=require('fs'); const mod=JSON.parse(fs.readFileSync('src/apps/business-os/modules/<module>/module.json','utf8')); const s=JSON.parse(fs.readFileSync('src/apps/business-os/modules/<module>/collections.schema.json','utf8')); const shell=new Set(['business_module_catalog','ctox_runtime_settings','business_commands','ctox_queue_tasks']); for (const name of shell) if (s.collections?.[name]) throw new Error('shell collection redeclared: '+name); for (const name of mod.collections||[]) if (!shell.has(name) && !s.collections?.[name]) throw new Error('non-shell collection missing from schema: '+name); console.log('schema coverage OK')"
```

Mirror this guard in the module test. Do not "allowlist" `business_commands`
in `collections.schema.json` for a greenfield app. `business_commands` belongs
in `module.json.collections` as a dependency, not in the module-owned schema
file.

Apply the same ownership rule to `schema.js`. It is a browser compatibility
facade for module-owned schemas, not a place to re-export shell-registered
schemas. A `schema.js` entry such as `business_commands: ...` is a failure even
when `collections.schema.json` is correct.

For new modules, the preferred maintainable pattern is a shared local
browser-safe ESM helper such as `core/schemas.mjs` or `schemas.mjs` that exports
plain schema objects. `schema.js` imports that helper by relative path and
wraps only the module-owned schemas for the browser facade; tests import the
same helper directly. Do not build a CommonJS, `node:vm`, `new Function`, or
string-transform loader in tests to execute `schema.js`. If a test cannot import
`schema.js` in the bench's Node context, test the shared `.mjs` helper or do a
text/JSON parity check instead.

Default source-checkout store module manifest:

```json
{
  "id": "<module>",
  "entry": "modules/<module>/index.html",
  "collections": ["business_commands", "<module_records>"],
  "install_scope": "store",
  "default_installed": false
}
```

For checked-in source modules with `install_scope: "store"`, add a matching
entry to:

```text
src/apps/business-os/modules/registry.json
```

The static packaged registry is the App Store/catalog seed. A source store
module whose directory exists but is missing from `modules/registry.json` can
pass file-level conformance while still being undiscoverable in the App Store.
Parse that registry after editing it and verify the entry's `id`, `entry`,
`install_scope`, `collections`, and store copy match `module.json`.

Default runtime-installed module manifest:

```json
{
  "id": "<module>",
  "version": "0.1.0",
  "entry": "installed-modules/<module>/index.html",
  "collections": ["business_commands", "<module_records>"],
  "install_scope": "installed"
}
```

Do not edit `modules/registry.json` for a runtime-installed App Creator/App
Store module unless the task explicitly changes the packaged app catalog.
Runtime-installed modules are discovered by scanning
`installed-modules/<module>/module.json`.

Generated installed-module artifacts must be written directly to their final
exact paths under the resolved module directory. Do not build app files in
`/tmp`, a sibling scratch directory, or a generated writer script and then copy
or move them into `installed-modules/<module>/`. If quoting or command length
becomes difficult, shrink the file, move pure logic into `core/*.mjs`, or write
the smaller affected file directly. Do not repair module files with `sed -i`,
`gsed -i`, `perl -pi`, or repeated line-number edits; rewrite the smallest
bounded helper/file and rerun validation.

When existing source modules show a different pattern, do not treat that as a
fallback. For new/runtime App Creator apps, these legacy patterns are forbidden
implementation choices:

```text
ctx.db.raw, db.raw, or ctx.collections
window.dispatchEvent or ctox-business-os-chat-submit for automation
manual insert/upsert into business_commands when commandBus is unavailable
pending_sync as local status, test fixture, CSS class, or README explanation
schema.js importing collections.schema.json as a JSON module
bundler/fake-DOM tests or dependency-managed proof commands
layout.right without third_pane_justification
```

Translate them to the current module contract instead of adding compatibility
branches.

## App Version Contract

New and runtime-installed Business OS apps use SemVer in `module.json.version`.
The legacy `v1` style still exists in some packaged modules and is not a
template for new app output.

Version meaning:

```text
0.0.x -> UI/UX, non-breaking features, and bug fixes without data-shape changes
0.x.0 -> schema/database or potentially breaking changes before public release
1.0.0 -> first release visible to users beyond the developer/founder
2.0.0 -> new parallel app line with its own module id and icon
x.0.0 -> every later major line is a separate app icon/module id
```

Rules for generated apps:

```text
use x.y.z without a v prefix
never use 0.0.0
start normal generated data apps at 0.1.0
start UI-only prototypes with no durable collection at 0.0.1
do not put SemVer dots in collection names; use safe suffixes such as v0_1_0
when schema.js or collections.schema.json changes shape, bump the minor field and add migrationStrategies
do not claim public/user-ready release until version is >= 1.0.0 and the shell/core audience gate enforces that visibility
when major reaches 2.0.0 or later, create a new module id/icon so the previous major line can continue as legacy
```

Existing CTOX versioning pieces:

```text
business_module_versions stores whole-bundle restore points with origin, seq, bundle hash, sealed flag, and files_json
ctox.module.list_versions and ctox.module.rollback_version operate on those bundle restore points
business_module_releases stores a separate integer release counter and rollback manifest snapshots
the shell and App Store hide runtime-installed work versions below 1.0.0 from normal users, while chef/admin and assigned founders can still see them
the current catalog projection exposes version_states, but public SemVer publish state is not yet driven by a dedicated release command
```

Until CTOX has a SemVer-aware release command, mark broad public distribution as
blocked for modules below `1.0.0`; do not solve it with UI copy or by hiding
controls inside a module.

For new modules, prefer shipping `icon.svg` and omitting `layout.icon_svg`
unless the inline SVG was copied from an already valid manifest or generated
through a JSON serializer. Hand-written inline SVG in JSON is a common source of
invalid manifests. After every manifest edit, run:

```sh
node -e "JSON.parse(require('fs').readFileSync('src/apps/business-os/modules/<module>/module.json','utf8')); console.log('module.json OK')"
```

For runtime-installed modules, adapt that command to
`runtime/business-os/installed-modules/<module>/module.json` from an install root
or `$CTOX_STATE_ROOT/business-os/installed-modules/<module>/module.json`. Do this before
writing large JavaScript files. A single invalid `module.json` can break native
module-catalog sync for the whole installed app set.

Never reinterpret root-level artifact-contract text, benchmark text, review
examples, or a model-generated blocker as permission to write app deliverables
outside the module directory. For a Business OS app, the app target contract is
the module directory plus the exact source/installed manifest fields. Root
`module.json`, root `collections.schema.json`, root `<module>/`, and root
status/blocker Markdown files are not app deliverables.

## Library Contract

Business OS modules are no-build browser ESM modules. There is no dependency
management layer for app code. "Use a library" means the browser can import one
or more local `.js`/`.mjs` ESM files directly by relative URL. A module may use
a library only when it is available as browser-compatible ESM from one of these
local sources:

```text
existing shell/repo ESM module imported by relative path
module-owned vendored ESM file under the module/source tree
```

Do not add dependency management for app code. Do not create `package.json`,
`package-lock.json`, `node_modules`, `.opencode/node_modules`, or a bundler
pipeline just to use a library. Do not import from CDN/remote URLs at runtime;
release installs must run from shipped files. Do not use CommonJS `require`,
bare package imports, import maps, generated app bundles, or Node-only modules
in browser runtime files. Every app/runtime import must resolve by an explicit
local relative path.

No package manager is part of the Business OS app contract. Do not recommend,
document, or prepare npm/pnpm/yarn/bun dependency setup as a future activation
step for an app library. If the library cannot run as shipped local ESM, defer
the feature or choose a CTOX-native implementation path.

Do not list `esbuild`, Vite, Webpack, Rollup, `npm run build`, `npx`, or another
bundle/build step in the module README or phase plan as normal verification.
Business OS modules are loaded directly by the shell as ESM files.

The same rule applies to tests. New greenfield module tests must not import a
bundler, invoke `npx`, or prove readiness by bundling app code. Put pure
business logic in local browser-safe `.mjs` helpers and import those helpers
directly from both `index.js` and `tests/*.test.mjs`.

Do not use exact dependency-management or bundler artifact words as negative
proof inside new module files. This includes README prose, source comments,
test names, assertion messages, and test string literals containing terms such
as `esbuild`, `webpack`, `rollup`, `vite`, `node_modules`, `package.json`,
`package-lock`, `importmap`, `import map`, `npm install`, or `npx`. If a test
needs to scan for such artifacts, construct the pattern from fragments or keep
the scanner outside the module tree.

If a desired library is not already available as local browser ESM, either
vendor a reviewed ESM build into the module/repo and import it by relative URL,
choose an existing CTOX helper/API, or defer that feature explicitly in the
phase plan.

## CSS Contract

`index.css` belongs to the module, but it is loaded into the shared Business OS
shell. Do not define custom properties on `:root`, `html`, or `body`, and do
not redefine shell/base tokens such as `--bg`, `--surface`, `--surface-2`,
`--line`, `--text`, `--accent`, `--danger`, `--panel-radius`, or
`--control-radius`. Those values leak into every app once the stylesheet is
loaded.

Scope module selectors and module-local custom properties under the module
root, for example:

```css
[data-subscriptions-root] {
  --subscriptions-row-gap: 12px;
  color: var(--text);
}
```

It is fine to read shell tokens with `var(--text)` or `var(--surface)`. It is
not fine to assign those token names in module CSS. The final static checker
and `assert-module-conformance.mjs` both reject `:root` custom properties and
shell-token redefinitions.

Module-local aliases must resolve to shell tokens or literal fallbacks, not to
themselves:

```css
[data-inventory-root] {
  --inventory-bg: var(--surface, #fff);
  --inventory-line: var(--line, #e5e7eb);
}
```

Do not write or leave aliases like this after a repair:

```css
[data-inventory-root] {
  --inventory-bg: var(--inventory-bg);
}
```

## Collection Schema Contract

`collections.schema.json` is the native-readable runtime contract:

```json
{
  "schema_format": "ctox-business-os-module-collections-v1",
  "collections": {
    "<module_records>": {
      "title": "<module_records>",
      "version": 0,
      "primaryKey": "id",
      "type": "object",
      "properties": {
        "id": { "type": "string", "maxLength": 160 },
        "created_at_ms": { "type": "number" },
        "updated_at_ms": { "type": "number" },
        "is_deleted": { "type": "boolean" }
      },
      "required": ["id"],
      "indexes": ["updated_at_ms"]
    }
  },
  "migration_strategies": {}
}
```

`schema.js` is only the browser compatibility facade over the same schemas:

```js
const collectionSchemas = { /* same schemas as collections.schema.json */ };

export const collections = Object.fromEntries(
  Object.entries(collectionSchemas).map(([name, schema]) => [name, { schema }])
);
```

Do not import `collections.schema.json` or any other `.json` file from
`schema.js`, `index.js`, or browser runtime helpers. JSON import
attributes/assertions are not the Business OS module contract and may be
unavailable in release browser contexts. Keep `collections.schema.json` as the
native-readable contract, and mirror the same objects in `schema.js` or in a
local browser-safe `.mjs` helper imported by relative path.

When using the helper pattern, keep the helper browser-safe too: no `node:*`
imports, no file-system reads, no dynamic import of JSON, no package imports,
and no import map assumptions. Tests may read `collections.schema.json` with
Node's `fs` APIs to compare parity, but runtime files must not.

Do not let `schema.js`, `collections.schema.json`, and `module.json` drift.
Parse `collections.schema.json` with `JSON.parse` immediately after editing.
Keep shell-registered collections out of both schema files; list them only in
`module.json.collections` as dependencies.
Do not treat a current conformance-script gap or a legacy module that relies on
`schema.js` as permission to skip `collections.schema.json`. The native runtime
contract for dynamically registered module collections is
`collections.schema.json`; `schema.js` mirrors it for browser compatibility.

## Shell Mount Contract

The shell calls:

```js
export async function mount(ctx) {
  // render into ctx.host, ctx.left, ctx.right
  return () => cleanup();
}
```

Use the context supplied by `createModuleContext()` in
`src/apps/business-os/app.js`:

```text
ctx.host
ctx.left
ctx.right
ctx.db
ctx.sync
ctx.commandBus
ctx.eventBus
ctx.contextMenu
ctx.notifications
ctx.openDesktopApp
ctx.openBusinessChat
ctx.reportIssue
```

Do not use `ctx.collections`. Collections live behind `ctx.db`.

CTOX DB is the public/runtime name for the Business OS data plane, not a
JavaScript object. There is no `ctox.db` API. Module code receives `ctx.db`
from `mount(ctx)` and must resolve collections through that shell facade.

Use this resolver pattern:

```js
function getCollection(name) {
  const db = state.ctx?.db;
  const collection =
    db?.collection?.(name) ||
    db?.collections?.[name] ||
    db?.[name];
  if (!collection) {
    throw new Error(`Business OS collection not available: ${name}`);
  }
  return collection;
}
```

Tests under `tests/` must resolve sibling module files from the module root,
not from the tests directory:

```js
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleRoot = resolve(testDir, '..');
const moduleJsonPath = resolve(moduleRoot, 'module.json');
```

Do not read `tests/module.json` by accident. Also do not use
`new URL('../../module.json', import.meta.url)` from a test under `tests/`.
That climbs out of the module directory and fails in installed benches because
it points at `installed-modules/module.json`. A test that fails only because it
resolved paths from the wrong directory is not module proof.

## Reactive Data

Load through CTOX DB collections and subscribe to changes:

```js
const collection = getCollection(PRIMARY_COLLECTION);
const sub = collection.find().$.subscribe((docs) => {
  state.records = docs.map((doc) => doc.toJSON());
  render();
});
cleanup.push(() => sub.unsubscribe?.());
```

Writes should use collection APIs such as `insert`, `findOne().exec()`,
`patch`, `incrementalPatch`, `upsert`, or local helper wrappers already used by
nearby modules.

For standard CTOX follow-up automations, dispatch through
`ctx.commandBus.dispatch({ command_type: 'business_os.chat.task', ... })`.
In a greenfield module, do not manually insert or upsert `pending_sync`
documents into `business_commands` as a fallback. If `ctx.commandBus` is
unavailable, render the action as unavailable and keep the phase proof blocked.
Special legacy dispatchers such as Tickets/Outbound are not default patterns for
new app modules.

Do not use shell chat CustomEvents or `window.dispatchEvent` as an automation
fallback. New App Creator apps must dispatch the command through
`ctx.commandBus.dispatch(...)`.

Do not create module-owned statuses named `pending_sync`. Treat raw command
states as commandBus/native details. Module UI, CSS, helper constants, README
text, and tests should use neutral local labels such as `submitted`, `queued`,
`unavailable`, or `failed` and render the real command result separately when
the shell provides one.

## Exact CTOX DB Persistence Path

Business OS module data persists through CTOX DB (`ctox-rxdb-js`) only. The
app-facing handle is `ctx.db`; the native RxDB document store is
`runtime/business-os-rxdb.sqlite3`.

```text
module UI event
-> ctx.db collection handle from the shell facade
-> IndexedDB-backed CTOX DB browser collection write
-> WebRTC replication
-> native CTOX RxDB peer
-> SQLite native RxDB document storage at runtime/business-os-rxdb.sqlite3
-> replicated updates back to every active browser peer
```

For command/projection flows, Rust may also mutate canonical core state in
`runtime/ctox.sqlite3` or Business OS state in `runtime/business-os.sqlite3`
before projecting documents into the native RxDB store. Browser modules still
observe and mutate Business OS records only through `ctx.db` and
`business_commands`, not direct SQLite or HTTP paths.

There is no HTTP fallback for Business OS records. Do not call `/api/...` for
module data, command state, files, tickets, users, runtime settings, module
catalog, or projections.

Every collection must be classified before coding:

```text
Module-owned direct CRUD:
  Browser module may insert/patch/upsert documents through ctx.db.
  Use for local-first records owned by this module.

CTOX-owned projection:
  Browser module must not write projection documents directly.
  Browser writes a business_commands document; Rust validates, mutates canonical
  CTOX state, and projects records back into replicated collections.
```

Examples of CTOX-owned projections include `ctox_queue_tasks`,
`ctox_runtime_settings`, `business_module_catalog`, `business_users`,
`communication_accounts`, `channel_pairing_state`, and `ctox_ticket_*`.

Do not write projection collections directly from module code. A module that
needs to create a normal CTOX follow-up should usually dispatch
`business_os.chat.task` through `ctx.commandBus`. Use a `ctox.ticket.*` command
only after confirming the existing native handler and payload shape. Do not
invent `<module>.*` automation command types unless the same change adds and
tests the native handler.

Do not put broad data-plane guard literals in new module tests or README files.
`assert-rxdb-only.mjs` scans `src/apps/business-os/modules` broadly, so a
literal such as `/api/business-os`, `/rxdb/pull`, `/commands`, `local-only`, or
an upstream `rxdb` import example can fail the guard even inside a test that is
trying to assert the runtime source does not contain it. Build regexes from
safe fragments or scan source files without embedding the forbidden string as
contiguous text.

The same applies to dependency-management, bundler, temp-artifact, and raw
command-state guard terms. A test whose own source contains the exact forbidden
word can fail the module even when runtime code is clean.

## Direct CRUD Pattern

For a module-owned collection:

```js
const MODULE_ID = '<module>';
const PRIMARY_COLLECTION = '<module_records>';

async function ensureCollectionReady(name = PRIMARY_COLLECTION) {
  await state.ctx?.sync?.startCollection?.(name);
  return getCollection(name);
}

async function createRecord(fields = {}) {
  const collection = await ensureCollectionReady(PRIMARY_COLLECTION);
  const now = Date.now();
  const record = {
    id: fields.id || `${MODULE_ID}_${crypto.randomUUID()}`,
    title: String(fields.title || 'Untitled'),
    status: String(fields.status || 'draft'),
    created_at_ms: now,
    updated_at_ms: now,
    is_deleted: false,
    ...fields,
  };
  await collection.insert(record);
  return record;
}

async function updateRecord(id, patch = {}) {
  const collection = await ensureCollectionReady(PRIMARY_COLLECTION);
  const doc = await collection.findOne(id).exec();
  if (!doc) throw new Error(`Record not found: ${id}`);
  const next = { ...patch, updated_at_ms: Date.now() };
  if (typeof doc.incrementalPatch === 'function') {
    await doc.incrementalPatch(next);
  } else {
    await doc.patch(next);
  }
}

async function softDeleteRecord(id) {
  await updateRecord(id, { is_deleted: true });
}
```

Rules:

```text
primary keys must be stable strings and match the schema maxLength
created_at_ms is set once; updated_at_ms changes on every mutation
delete is usually a tombstone (`is_deleted: true`) unless local patterns prove hard delete is expected
await the write before updating success UI
after writing, render from the reactive subscription, not from a detached optimistic copy only
```

## First Runnable Slice

Build in this order for new greenfield modules:

```text
1 phase tracker with source inventory, few-shot map, porting map, and current status
  - source module: docs/business-os-<module>-implementation-plan.md
  - runtime-installed module: active task/bench tracker or final implementation note, not a Markdown file inside installed-modules/<module>
2 module.json, collections.schema.json, schema.js, icon.svg, locales/de.json, locales/en.json
3 validate module.json and collections.schema.json with JSON.parse
4 index.html, index.css, index.js with mount(ctx), one collection subscription, one create/edit path
5 one automation button that dispatches an existing command or a newly implemented native-backed command
6 README and tests; tests import shared `.mjs` helpers and JSON/text files, not `../index.js` or `../schema.js`
7 conformance, forbidden-pattern scan, real-shell proof
```

Do not create optional `core/`, `commands/`, `views/`, or large helper folders
before the first slice mounts and mutates one record. Extra folders are allowed
only when they reduce real complexity after the app already runs.

## Reactive Read Pattern

Use one load plus a live subscription:

```js
async function loadRecords() {
  const collection = await ensureCollectionReady(PRIMARY_COLLECTION);
  const docs = await collection.find().exec();
  state.records = docs.map((doc) => doc.toJSON()).filter((item) => !item.is_deleted);
  render();
}

function subscribeRecords() {
  const collection = getCollection(PRIMARY_COLLECTION);
  const sub = collection.find().$.subscribe((docs) => {
    state.records = docs.map((doc) => doc.toJSON()).filter((item) => !item.is_deleted);
    render();
  });
  return () => sub.unsubscribe?.();
}
```

The module is not durable if it only reads once. The unmount cleanup returned
from `mount(ctx)` must unsubscribe.

## Persistence Proof

For direct CRUD, proof requires all of these:

```text
collection appears in module.json
collection schema appears in collections.schema.json
module resolves the collection from ctx.db
create/edit/delete path awaits insert/patch/upsert
UI refreshes from collection subscription
page reload still shows the changed record
no HTTP request carried the Business OS record data
```

For command/projection persistence, proof requires:

```text
business_commands appears in module.json
command document is written through ctx.commandBus or the business_commands collection
Rust/native handler accepts the command type
handler writes canonical state or explicit projection records
browser observes command status/result and projected collection updates through ctx.db
unsupported command types fail instead of completing as queue-only stubs
```

## Commands

Use `business_commands` when effects need queue/audit/native handling.

Module requirements:

```text
business_commands appears in module.json collections
ctx.sync.startCollection('business_commands') is called before dispatch when needed
command has stable command_type
payload includes enough idempotency and actor/context data
native handler or documented existing handler processes the command
unsupported commands fail explicitly
```

## Form State and Finalizing Actions

Every visible control that can affect persisted data must be wired end to end:

```text
DOM control or widget event
state update or form extraction
payload field or direct CRUD patch
schema field
native validation when command-based
real-shell browser proof with a non-default value
```

Do not rely on default values or visual selection alone. A `<select>`, tab,
checkbox, toggle, editable table cell, drag/drop handle, context menu item, or
inline input is not implemented until the changed value is visible after
reload or appears in the command/projection proof.

Finalizing actions such as `post`, `send`, `lock`, `approve`, `allocate`,
`archive`, `run`, or `export` must not read stale draft state. Before the
command is dispatched, either:

```text
persist the complete current draft and await the write/result
or build the final command payload directly from the current form controls/state
```

The browser proof must include the user path that changes a field and then
immediately triggers the finalizing action without a separate manual save when
the UI offers that path.

Dispatch shape:

```js
await state.ctx.commandBus.dispatch({
  id: commandId,
  command_id: commandId,
  module: MODULE_ID,
  type: '<module>.<action>',
  command_type: '<module>.<action>',
  record_id: record.id,
  payload: {
    record_id: record.id,
    idempotency_key: commandId,
    source_module: MODULE_ID
  },
  client_context: {
    source: MODULE_ID,
    actor: currentActor()
  }
});
```

Native handler rules:

```text
validate module id and command type
validate payload shape, required fields, lifecycle state, money/unit scale, locks, and idempotency key
write only declared collections or documented external handoff state
return failed/blocked for unsupported or incomplete effects
never mark placeholder/no-op effects completed
add focused Rust tests for new handlers
```

Browser validation is usability only. Command-owned mutations must still reject
invalid or incomplete payloads in Rust/native code, including empty required
foreign keys, empty line items, invalid state transitions, negative money,
wrong cent/milli units, and unbalanced accounting effects.

## App Creator Output

Generated modules are not ready just because files exist. Harden generated
output by checking:

```text
module.json includes business_commands when CTOX tasks are visible
collections.schema.json uses ctox-business-os-module-collections-v1
index.js uses ctx.db resolver, not ctx.collections
all visible buttons have real collection writes or real command dispatch
reactive subscriptions update UI
module survives reload with persisted data
conformance and real-shell proof are present
```

README, App Store copy, module descriptions, and phase trackers must describe
only capabilities that have a real UI path, handler/write path, and proof. Put
future work under "Deferred" or "Known limitations"; never make planned
exports, recurring jobs, PDFs, AI actions, integrations, or command handlers
sound shipped.
