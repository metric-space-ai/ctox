# Business OS Module Verification

Use this reference before claiming an app or module is ready.

## Repair Order

When a generated app fails validation, repair the highest layer first and do
not work on lower layers until the higher layer is green:

```text
1. target path and off-target artifacts
2. valid module.json / collections.schema.json JSON
3. source vs installed manifest fields
4. required file set
5. collection ownership and schema parity
6. UI layout contract
7. runtime import/dependency/data-plane contract
8. index.js syntax
9. no-dependency tests
10. real-shell smoke
```

Do not modify tests to match a broken module contract. A passing custom test is
not evidence while `module_static_check.mjs` or `validate-app-module.mjs` is
red.

## Static Checks

Run the narrow checks that match the touched files:

```sh
node -e "const fs=require('fs'); for (const f of ['src/apps/business-os/modules/<module>/module.json','src/apps/business-os/modules/<module>/collections.schema.json']) JSON.parse(fs.readFileSync(f,'utf8')); console.log('module JSON OK')"
node -e "const fs=require('fs'); const f='src/apps/business-os/modules/registry.json'; if (fs.existsSync(f)) JSON.parse(fs.readFileSync(f,'utf8')); console.log('registry JSON OK')"
node --check src/apps/business-os/modules/<module>/index.js
node --test src/apps/business-os/modules/<module>/tests/*.test.mjs
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <module>
node src/skills/system/product_engineering/business-os-app-module-development/scripts/module_static_check.mjs <module> --installed  # for App Creator/runtime installed-modules targets
node src/apps/business-os/scripts/assert-module-conformance.mjs
! rg -n "ctx\\??\\.db\\??\\.raw|\\bdb\\??\\.raw\\b|ctx\\.collections|ctox\\.db|fetch\\('/api/business-os|from ['\\\"]rxdb|from ['\\\"]node:|from ['\\\"][^./]|require\\(|https?://|cdn\\." src/apps/business-os/modules/<module> --glob '*.js' --glob '*.mjs' --glob '*.html' --glob '!tests/**' --glob '!*.test.mjs'
! rg -n "import .*\\.json" src/apps/business-os/modules/<module> --glob '*.js' --glob '*.mjs' --glob '!tests/**' --glob '!*.test.mjs'
! find src/apps/business-os/modules/<module> -maxdepth 4 \( -name node_modules -o -name package.json -o -name package-lock.json -o -name yarn.lock -o -name pnpm-lock.yaml -o -name bun.lockb \) -print
! rg -n "esbuild|webpack|rollup|vite|importmap|import map|npm run build|npm install|npx " src/apps/business-os/modules/<module>/README.md docs/business-os-<module>-implementation-plan.md
! find src/apps/business-os/modules/<module> -maxdepth 4 \( -name .DS_Store -o -name Thumbs.db -o -name '_probe_*' -o -name '*.bundle.js' -o -name '*.tmp' \) -print
! rg -n "pending_sync|business_commands.*upsert|business_commands.*insert|collection\\(['\\\"]business_commands|commandBus unavailable" src/apps/business-os/modules/<module> docs/business-os-<module>-implementation-plan.md
```

The forbidden `node:` import scan is for browser runtime files only. It must
exclude tests. `node:test`, `node:assert/strict`, `node:fs/promises`, and
similar built-ins are normal in `.test.mjs` files.

If `rg` is unavailable, use a real fallback such as `grep -R` or a small Node
script. Do not print "clean" after `rg: command not found`; that is an
unrun gate.

`assert-rxdb-only.mjs` scans the Business OS module tree broadly, including
tests and README files. Do not put forbidden data-plane literals in new module
files at all, even in comments or assertions:

```text
/api/business-os
/rxdb/pull
/commands
local-only
FallbackDatabase
from 'rxdb'
```

If a test needs to assert a runtime file does not contain a forbidden pattern,
build the regex from fragments so the test file itself does not trip the guard,
or move that assertion into a local helper that avoids contiguous forbidden
strings.

Apply the same rule to dependency-management and bundler terms. Do not write
comments, test names, assertions, README prose, or README shell commands that
contain exact strings such as `esbuild`, `webpack`, `rollup`, `vite`,
`node_modules`, `package.json`, `package-lock`, `importmap`, `import map`,
`npm install`, or `npx` as negative proof. Build regexes from fragments or run
that scanner from outside the module tree.

If `node --check` treats `.js` as CommonJS because the isolated bench copy lacks
the repo's normal ESM package context, do not claim a syntax failure solely from
that. Instead run the repo's existing module tests, the conformance guard, or a
syntax check in an ESM-capable context. A missing ESM package context is a bench
harness problem; invalid JSON, missing files, missing `mount(ctx)`, and
Business-OS forbidden patterns are still hard module failures.

Do not run `npm install`, `npx`, or create package files just to make a module
test pass. New Business OS modules are no-build browser ESM modules. If an
existing reference test depends on a package that is absent in the current
workspace, write a no-dependency test for pure helpers or mark that proof
blocked instead of installing dependencies.

Do not invoke `npx --yes`, transient bundler installs, or a bundler import from
module tests. If a test imports `esbuild`, Vite, Rollup, Webpack, or another
package-managed build tool, the test is part of the failure even when it
passes.

The bundled `module_static_check.mjs` is the preferred source-checkout scanner
for app-module structure, registry visibility, no-build ESM runtime imports,
and forbidden negative-proof strings. Run it from the repository root and do
not copy its rule literals into the module's own tests or README files.
It must be the last source-checkout gate before completion. Run it after
module tests, README, phase-plan, and source edits. If any file under
`src/apps/business-os/modules/<module>/` or
`docs/business-os-<module>-implementation-plan.md` changes after a green run,
the green evidence is stale and the checker must be rerun.

For CTOX-native release/App Creator work, distinguish command-path proof from
source-checkout proof. A runtime-installed module must show evidence that the
Business OS command or App Creator action completed, wrote the expected
installed module files, and mounted in the live Business OS shell. The worker
must have received or inferred `business-os-app-module-development` as the
required/suggested skill, and must have been able to inspect the embedded skill
through CTOX or an explicitly pinned release-tag GitHub fallback. Do not claim
production readiness from source-only checks when the target was an installed
release module.

The static checker also mirrors the critical CSS conformance rule: module
styles must not define custom properties on `:root`, `html`, or `body`, and
must not redefine shell/base tokens such as `--bg`, `--surface`, `--text`,
`--accent`, `--line`, or `--panel-radius`. Scope module-local CSS variables
under the module root and only read shell tokens with `var(...)`.

If a coding-agent runtime creates its own tool directory such as `.opencode/`,
do not count that as an app dependency unless the module, its tests, or its
instructions reference it. Package artifacts inside the module tree or created
specifically to run module checks are failures.

Business OS app runtime code has no dependency-management step. External
libraries are allowed only as local browser-compatible ESM modules: either an
existing shell/repo ESM import by relative path or a reviewed vendored ESM file
checked into the module/source tree. Do not use import maps, CommonJS
`require`, bare package imports, CDN/runtime URL imports, npm/yarn/pnpm/bun
installs, or bundlers for app code. If a library is not available in that
shape, defer the feature or use an existing CTOX shell API.

Browser runtime files must not import `.json` modules. In particular, do not
write `schema.js` as a JSON import wrapper around `collections.schema.json`.
Keep `collections.schema.json` for native/runtime registration and mirror the
same schemas in browser-safe JS/ESM objects or a local `.mjs` helper.

For schema parity tests, prefer a local browser-safe helper such as
`core/schemas.mjs` or `schemas.mjs` that exports the plain schema objects.
Import that helper directly from both `schema.js` and tests. Do not build a
custom CommonJS, `node:vm`, `new Function`, or string-transform loader to
execute `schema.js` in tests. If the bench lacks an ESM package context for
`.js`, test the shared `.mjs` helper or compare `schema.js` text with
`collections.schema.json` expectations instead.

Do not document bundling as a readiness proof. `esbuild`, Vite, Webpack, Rollup,
`npm run build`, or `npx` belong to other web-app stacks unless an existing
Business OS module explicitly proves that path. Normal Business OS proof is
valid JSON, module conformance, no forbidden imports/data-plane patterns,
focused no-dependency tests, and real-shell browser proof.

Existing Business OS modules may have legacy esbuild/fake-DOM tests. For a new
greenfield app, treat those as rejected few-shot patterns. Write direct ESM pure
helper tests instead. Do not import `esbuild` or generate bundle/probe files in
the new module tree.

Greenfield automation proof must use `ctx.commandBus.dispatch`. Do not prove a
standard CTOX follow-up by inserting a `pending_sync` document into
`business_commands`; if commandBus is absent, the action is unavailable or the
real-shell proof is blocked.

Do not use `pending_sync` as a module-local status enum, CSS class, UI label,
README explanation, or fallback commandBus result. Use neutral module-local
labels such as `submitted`, `queued`, `unavailable`, or `failed`; let
commandBus/native code own raw command document states.

Do not leave editor or sed repair artifacts in the module tree. `*.bak`,
`*.orig`, `*.rej`, `*.tmp`, `*.bundle.js`, `*.bundle.mjs`, `_probe_*`,
package files, and dependency directories are failures, not harmless leftovers.

For no-dependency tests, put reusable pure helpers in one browser-safe ESM file
that Node can import directly, preferably `core/<module>.mjs`, and import that
same helper from `index.js`. Do not maintain duplicate `.js` and `.mjs` copies
of the same domain logic. Do not import `esbuild` or another bundler in a new
module test unless the repo already makes that dependency available for the
specific test pattern. A test that only passes after adding package artifacts is
a failure, not proof.

The same no-loader rule applies to schemas: a complex test harness that rewrites
`schema.js`, imports `node:vm`, or evaluates transformed source with
`new Function` is not proof. It is a signal that the schemas should be moved to
a shared local `.mjs` helper and imported directly.

Do not build an elaborate fake RxDB/WebRTC runtime for a greenfield module
test. Pure helper tests plus the conformance guard are better evidence than a
buggy fake database. If you need mount-level coverage, use the smallest facade
that matches the shell contract (`ctx.db.collection()` and `ctx.db.collections`)
and never require or expose `db.raw`.

When tests live in `tests/`, resolve sibling files from the module root:

```js
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const moduleRoot = resolve(testDir, '..');
```

Then read `resolve(moduleRoot, 'module.json')`,
`resolve(moduleRoot, 'collections.schema.json')`, and so on. A test that reads
`tests/module.json` is wrong.

Do not add optional fake-DOM or mount tests after a green pure-helper and
conformance pass unless the fake shell is stable and the new test passes. A
broken optional test is a failure even if the module itself was previously
green. Avoid overriding read-only browser globals such as `globalThis.crypto`;
inject deterministic ids through helper options instead.

For source-checkout `install_scope: "store"` modules, verify App Store/catalog
discoverability:

```sh
node - <<'NODE'
const fs = require('fs');
const mod = JSON.parse(fs.readFileSync('src/apps/business-os/modules/<module>/module.json','utf8'));
const catalog = JSON.parse(fs.readFileSync('src/apps/business-os/modules/registry.json','utf8'));
const entry = (catalog.modules || []).find((item) => item.id === mod.id);
if (!entry) throw new Error(`registry missing ${mod.id}`);
if (entry.entry !== mod.entry) throw new Error(`registry entry mismatch for ${mod.id}`);
for (const name of mod.collections || []) {
  if (!(entry.collections || []).includes(name)) throw new Error(`registry missing collection ${name}`);
}
console.log('module registry entry OK');
NODE
```

Do not run this registry check for runtime-installed `installed-modules`
targets unless the task explicitly edits the packaged registry.

For normal modules, verify `collections.schema.json` does not redeclare
shell-registered collections, `schema.js` does not re-export them as browser
schema keys, and every non-shell collection listed in `module.json` has a
schema:

```sh
MODULE=<module> node --input-type=module <<'NODE'
import { readFileSync } from 'node:fs';

const moduleId = process.env.MODULE;
const root = `src/apps/business-os/modules/${moduleId}`;
const mod = JSON.parse(readFileSync(`${root}/module.json`, 'utf8'));
const schemas = JSON.parse(readFileSync(`${root}/collections.schema.json`, 'utf8'));
const schemaJs = readFileSync(`${root}/schema.js`, 'utf8');
const shell = ['business_module_catalog', 'ctox_runtime_settings', 'business_commands', 'ctox_queue_tasks'];

for (const name of shell) {
  if (schemas.collections?.[name]) throw new Error(`shell collection redeclared: ${name}`);
  const keyPattern = new RegExp(`(?:^|[,{}]\\s*)(?:['"]${name}['"]|${name})\\s*:`, 'm');
  if (keyPattern.test(schemaJs)) throw new Error(`shell collection exported from schema.js: ${name}`);
}

const shellSet = new Set(shell);
for (const name of mod.collections || []) {
  if (!shellSet.has(name) && !schemas.collections?.[name]) {
    throw new Error(`non-shell collection missing from schema: ${name}`);
  }
}

console.log('schema coverage OK');
NODE
```

The module's own tests should enforce the same list exactly. A test that says
`business_commands` is allowed in `collections.schema.json` or `schema.js` is
wrong for a new greenfield module. A test that lists peer-module collections in
`module.json` but does not require them in `collections.schema.json` is also
wrong; either declare the exact same schema as the owning module or defer that
dependency.

If App Creator generated output changed, also run:

```sh
node src/apps/business-os/scripts/assert-app-creator-generated-module.mjs
```

Local JSON, Node, conformance, and grep/rg checks are ordinary read/test gates.
Do not stop at a questionnaire or approval prompt for those checks in an
environment that already permits local commands. If the tool surface truly
blocks them, mark the proof phase `blocked` and do not finish as complete.

Real-shell browser proof is required for production readiness, but do not
mutate a regular installed CTOX release to fake source-checkout proof. If proof
would require copying module files into `~/.local/lib/ctox/current`, editing the
installed release registry, or otherwise touching a non-workspace install, mark
the browser-proof phase `blocked` and report the safe proof path instead. Do
not ask the user a questionnaire for that mutation during a bench or source
checkout run.

If schemas or RxDB contracts changed, run the project-specific gates:

```sh
node src/core/rxdb/tools/build_business_os_schema_contract.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml
cargo test --bin ctox native_all_schema_hashes_match_browser_contract_fixture
cargo check
```

When `src/apps/business-os/rxdb/src/schema.mjs` changes, rebuild the browser
bundle using the pinned command from `docs/ctox-rxdb.md` and bump required cache
busters together. Never patch `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`
directly.

For new or changed native command handlers:

```sh
cargo test --bin ctox --no-fail-fast <module-or-handler-filter>
cargo check --bin ctox
```

## Fake Affordance Scan

Run:

```sh
rg -n "TODO|placeholder|mock|fake|not implemented|coming soon|setMessage\\(|alert\\(|stub" src/apps/business-os/modules/<module>
rg -n "onclick=|addEventListener|data-.*action|button|submit|drag|drop|context-action|commandBus.dispatch|insert\\(|patch\\(" src/apps/business-os/modules/<module>
```

Every visible action hit needs:

```text
UI event
real collection write or command dispatch
schema support
native/pre-existing handler when command-based
test proof
browser proof
```

If the chain is missing, remove or hide the affordance.

For forms, editable tables, filters that affect writes, and finalizing actions,
also verify:

```text
each persisted input/select/toggle has a state or payload binding
browser proof changes at least one non-default value and sees it after reload
post/send/lock/approve/run uses the current edited values, not a stale draft
native handler rejects the same invalid required fields and lifecycle states the UI rejects
```

For automation actions, additionally verify:

```text
the UI dispatches an existing CTOX command type such as business_os.chat.task, or the change includes a native handler
the automation does not write directly to ctox_ticket_* projection collections
the automation result shown in the UI is based on command dispatch/status, not only a local audit row
module copy does not claim "ticket created" unless the command/projection path is proven
```

## Real-Shell Browser Proof

Browser proof must use the actual Business OS shell, not a standalone HTML file
unless the module is intentionally standalone.

The proof must cover:

```text
open Business OS shell
open or install the module through the expected path
module mounts without console errors
declared dependency blocker appears if a required dependency is absent
real data loads from ctx.db
create/edit/delete or the core command works
reload preserves persisted state
reactive subscription updates visible UI
command status/result is visible when a command is used
right-click Prompt CTOX works when context metadata is present
no failed network requests for Business OS record data
```

Browser proof that only checks route rendering is not enough.

## Persistence Browser Proof

For direct CTOX DB CRUD, the browser proof must include this sequence:

```text
open real Business OS shell
open the target module
create a record with a unique smoke id
wait until the record is visible from the reactive UI
reload the shell
open the same module again
confirm the same smoke id is still visible
edit the record
wait until the edited field is visible
reload again
confirm the edited value is still visible
delete or tombstone the smoke record if the module supports cleanup
confirm no Business OS record data moved through HTTP requests
```

For command/projection persistence, the proof must include:

```text
dispatch the command from the real module UI
observe a non-empty command id and queue/task status
wait for command status/result to replicate back
confirm the target projected collection changed in the UI
reload the shell
confirm the projection is still visible
confirm unsupported commands do not complete as success
```

## Review Checklist

Inspect for these failure classes:

```text
port lacks source inventory for screens/routes, APIs/actions, data models, jobs, files, auth/governance, integrations, and tests
source features are neither mapped to Business OS targets nor explicitly deferred/rejected
source API/server-action pattern was copied as an HTTP data route instead of direct ctx.db CRUD or command/projection
source SQL/ORM migration assumptions were copied instead of collections.schema.json/module.json collection contracts
module expects ctx.collections instead of ctx.db
module invents ctox.db or uses a global database handle
module reads ctx.db.raw instead of live facade collections
missing collection.$ subscriptions
source store module missing `src/apps/business-os/modules/registry.json` catalog entry
runtime-installed module edits packaged registry unnecessarily
module.json omits read/write dependency collections
collections.schema.json redeclares shell/core dependency collections such as business_commands instead of limiting itself to module-owned collections
schema.js exports shell/core dependency collections such as business_commands instead of mirroring only module-owned schemas
collections.schema.json missing or out of sync
schema.js and collections.schema.json drift
module.json or collections.schema.json is invalid JSON, often caused by unescaped inline SVG
module.json contains layout.icon_svg copied by hand instead of using icon.svg or validated manifest JSON
module.json contains layout.right without an explicit layout.third_pane_justification
index.html or index.css contains right-pane/right-column/right-resizer/data-*-right/three-column layout copied from another module without a real persistent-context workflow
index.css defines `:root` custom properties or redefines shell/base design tokens instead of scoping module-local variables under the module root
unknown <module>.* command falls through to generic queue behavior
module invents a module-local automation command without native handler and still claims normal CTOX work/ticket flow
stub command returns ok/completed
tests assert wrong field names or wrong unit scale
test setup created package.json, package-lock.json, node_modules, or .opencode/node_modules artifacts
test setup imports index.js or schema.js directly from Node, or through a `data:text/javascript` URL/base64 source string, instead of testing shared `.mjs` helpers plus JSON/text parity
test setup imports a bundler only to compensate for `.js` ESM context instead of using one `.mjs` helper
test setup rewrites or transform-loads schema.js instead of importing a shared `.mjs` schema helper or doing simple text/JSON parity checks
test setup's fake DB fixture is broken, too complex, or asserts `db.raw`
optional fake-DOM/mount test is broken, flaky, or overrides read-only globals
form control changes are visible but not persisted or dispatched
finalizing action posts/sends/runs stale draft data after visible unsaved edits
native handler trusts browser-only validation for required fields or money/accounting rules
native handler writes fields downstream schemas do not read
posted/locked records can be changed through generic edit paths
README or phase tracker says done while browser proof or native gates are absent
phase tracker marks required files, handlers, tests, or shell proof done before those files or evidence exist
README, module.json, or App Store copy advertises handlers/features that are stubs, deferred, or unproven
duplicate stale todo rows in phase tracker
```

## Completion Evidence

Final response must include:

```text
module:
changed files:
owned collections:
dependency collections:
direct CRUD actions:
commanded actions:
native handlers:
tests:
browser proof:
blocked or deferred items:
```

If a required proof could not run, say exactly why and keep the relevant phase
blocked or needs proof.
