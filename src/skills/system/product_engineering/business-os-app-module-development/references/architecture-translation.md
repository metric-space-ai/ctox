# Business OS Architecture Translation And Porting Guide

Use this before building, porting, reviewing, or hardening a Business OS app.
The goal is to prevent agents from unconsciously building a Next.js, React,
REST, IndexedDB, or Postgres app inside the Business OS module folder.

## Table Of Contents

- One-screen mental model
- Lifecycle of a module
- Porting workflow
- Source inventory checklist
- Porting matrix template
- Framework translation table
- Pattern-by-pattern porting rules
- Write model decision tree
- Common agent traps
- Required architecture map in plans
- Browser proof expectations

## One-Screen Mental Model

A Business OS app is a shell-mounted, no-build browser module backed by CTOX
DB. It is not a standalone SPA with its own API server.

```text
Business OS shell
  -> opens CTOX DB in the browser
  -> registers core collections
  -> registers module collections from collections.schema.json
  -> imports modules/<module>/index.js
  -> calls mount(ctx)

module UI
  -> reads/writes through ctx.db
  -> writes commands through ctx.commandBus.dispatch(...)
  -> subscribes to collection.$ or find().$

CTOX DB data plane
  -> browser IndexedDB
  -> WebRTC replication
  -> native rxdb-rs peer
  -> runtime/business-os-rxdb.sqlite3

CTOX command/projection side
  -> business_commands document
  -> Rust handler / CTOX core state
  -> runtime/ctox.sqlite3 when core state is involved
  -> projected RxDB documents
  -> replicated back to the browser

App Creator lifecycle
  -> app command enters business_commands
  -> CTOX service creates and leases the app-build queue task
  -> coding agent writes only module files
  -> CTOX service validates the module
  -> CTOX service completes, reworks, or fails the queue and command
```

HTTP may deliver the static shell, module files, `collections.schema.json`, and
bootstrap/pairing config. HTTP must not carry Business OS records, commands,
files, module data, runtime state, or collection sync.

Source checkout and release installs differ only in where module files are
authored:

```text
source development
  -> edit src/apps/business-os/modules/<module>/*
  -> checked-in store module entry modules/<module>/index.html
  -> if install_scope is store, add src/apps/business-os/modules/registry.json catalog entry

regular CTOX release / App Creator / App Store
  -> dispatch ctox.module.save and ctox.source.save
  -> runtime-installed module files under installed-modules/<module>/
  -> installed entry installed-modules/<module>/index.html
```

Business OS app versioning is not the same as generic web-app package
versioning:

```text
0.0.x -> non-breaking UI/UX, feature, and bug-fix changes before public release
0.x.0 -> schema/database or potentially breaking changes before public release
1.0.0 -> first app line visible beyond the developer/founder
2.0.0 -> new parallel app line with a new module id/icon
x.0.0 -> every later major line is also a separate app id/icon
```

Do not use legacy `v1` manifest examples for generated apps. New installed
apps normally start at `0.1.0` when they create durable collections. If a
collection name needs a version suffix, translate `0.1.0` to a safe suffix
such as `v0_1_0`; never put SemVer dots in collection ids. The existing
`business_module_versions` timeline is a bundle rollback system, not a
complete SemVer public-release gate.

Do not assume a regular installation has a local CTOX source tree. If the work
arrives through CTOX/Business OS, use the embedded system skill by id and the
runtime module-save/source-save flow unless the prompt explicitly says this is
source development in a repository checkout.

The CTOX-native App Creator path is the primary target. A generated user app in
a normal installation belongs under `installed-modules/<module>/`, materialized
from `$CTOX_STATE_ROOT/business-os/installed-modules/<module>/` or
`runtime/business-os/installed-modules/<module>/` in an isolated app-build
root. Source modules under `src/apps/business-os/modules/<module>/` are
packaged store/template source, not the default target for user-created apps.

If an external coding agent needs the skill text outside a source checkout,
give it a release-matched GitHub URL as fallback, not a maintainer-local path:

```text
https://raw.githubusercontent.com/metric-space-ai/ctox/<release-tag>/src/skills/system/product_engineering/business-os-app-module-development/SKILL.md
```

Use `main` only when the target release tag is unknown, and call out that the
skill may be newer than the installed binary.

Do not confuse "module files exist" with "Business OS can discover the app":

```text
source store module
  -> needs modules/<module>/module.json
  -> needs modules/registry.json entry for App Store/catalog discovery

source starter/core/internal module
  -> loaded from module.json according to install_scope rules

runtime installed module
  -> loaded from installed-modules/<module>/module.json
  -> no packaged registry edit required
```

Do not confuse open CTOX work context with the app-build target. Queue IDs
shown in continuity, `current_queue_item_id`, and open-work blocks are service
context. An App Creator agent must not call queue lifecycle commands or select
a different queue row from that context. The authoritative target is the app
build block: `module_id`, `install_target`, and
`only_allowed_app_artifact_directory`.

Do not confuse existing module source with current architecture authority.
Existing modules can show mount shape, schema shape, layout density, and command
payload examples, but older compatibility code must be translated:

```text
ctx.db.raw / db.raw -> ctx.db facade collection lookup
ctx.collections -> ctx.db
window.dispatchEvent('ctox-business-os-chat-submit') -> ctx.commandBus.dispatch(...)
manual business_commands insert/upsert fallback -> commandBus only, or disabled action if unavailable
pending_sync local state -> submitted/queued/unavailable/failed UI state
bundled/fake-DOM tests -> direct local .mjs helper tests
JSON import schema.js wrapper -> browser-safe JS/ESM schema objects
decorative right pane -> modal/drawer or two-pane layout
```

If a few-shot module conflicts with this guide, the current skill and validator
win. Do not add "temporary" fallbacks to preserve a legacy pattern in a new App
Creator app.

## Lifecycle Of A Module

1. Shell opens the Business OS data plane with `createBusinessDb`.
2. Shell registers core collections.
3. Shell loads each module's `collections.schema.json` and calls
   `db.addCollections(...)`. Runtime module collections are dynamic; pure
   module collections do not require Rust schema-hash fixture regeneration.
4. Shell builds `ctx` through `createModuleContext(mod)`.
5. Module `mount(ctx)` renders into `ctx.host`, optional drawers/panes into
   `ctx.left` and `ctx.right`, and returns an unmount cleanup function.
6. Module resolves collections from `ctx.db.collection(name)`,
   `ctx.db.collections[name]`, or `ctx.db[name]`.
7. Module subscribes to `collection.$` or `collection.find().$` and cleans up
   every subscription on unmount.
8. User actions either directly mutate module-owned collections through
   `ctx.db`, or dispatch `business_commands` through `ctx.commandBus`.
9. Browser changes replicate over WebRTC. Native handlers and projection loops
   update canonical state or projection documents, which replicate back to the
   UI.

## Porting Workflow

When porting from an existing app, do not start by copying files. First convert
the source architecture into a Business OS architecture.

1. **Inventory the source app.** List screens/routes, API endpoints/actions,
   data models/tables, migrations, jobs, files, auth/roles, integrations,
   validation rules, tests, fixtures, and any browser storage.
2. **Choose the Business OS module boundary.** Decide the module id, install
   scope, authoring target (`src/apps/...` for source checkout or
   `installed-modules/...` through commands for release/runtime), dependencies,
   owned collections, read-only dependency collections, and commanded actions.
3. **Translate data models.** Map SQL tables, API DTOs, IndexedDB object
   stores, or JSON files to `collections.schema.json` collections. Decide
   module-owned direct CRUD versus CTOX-owned command/projection for each.
4. **Translate mutations.** Map each source API route, form action, server
   action, job trigger, or local-store write to either direct `ctx.db` CRUD or
   `ctx.commandBus.dispatch(...)` plus a native/pre-existing handler.
5. **Translate UI.** Map pages/routes to one Business OS module surface:
   simple one-pane, two-pane list/detail, or a modal/drawer workflow. A
   right/third pane is an exception for a live inspector, dependencies,
   approvals, or context that the user needs while working. Avoid landing
   pages, decorative dashboards, and generic three-column layouts.
6. **Translate validation and invariants.** UI validation may improve UX, but
   command-owned or regulated effects must also validate in Rust/native code.
7. **Build the smallest durable slice.** Prove one create/edit/delete or one
   command/projection flow through the real shell before adding secondary UI,
   optional helper folders, or broad test harness work.
8. **Add broader features only after proof.** Every source feature must be
   mapped, implemented, deferred, or rejected with a reason.
9. **Verify in the real shell.** Unit tests are supporting evidence; readiness
   requires browser proof through the actual Business OS shell and CTOX DB path.

## One-Shot App Creator Scope Minimization

For a new App Creator module, the safest first pass is usually a single
module-owned collection that represents the primary work item. This is not a
database-design ideal; it is the minimum reliable Business OS slice that can be
created, validated, mounted, and then evolved.

Translate common multi-table instincts like this:

```text
Inventory tables: items, locations, batches, pick_lists
Business OS first pass: inventory_records
  fields: kind, sku, name, location_name, batch_code, quantity, min_level,
          expires_at_ms, pick_status, owner, notes

Projects tables: projects, milestones, time_entries, invoices
Business OS first pass: project_records
  fields: kind, customer_name, project_name, pricing_model, milestone_name,
          budget_cents, actual_cents, billing_status, risk, due_at_ms

Contracts tables: contracts, slas, renewal_terms, notices
Business OS first pass: contract_records
  fields: customer_name, contract_title, sla_level, renewal_at_ms,
          cancellation_deadline_ms, status, owner, notes

Quality tables: complaints, findings, audits, evidence
Business OS first pass: quality_records
  fields: kind, title, customer_name, severity, state, due_at_ms,
          evidence_summary, audit_ref, owner, notes
```

Only split the first pass into additional persisted collections when the user
explicitly needs independently editable objects and you update every contract
surface in one edit: `module.json`, `collections.schema.json`, `schema.js`,
`core/records.mjs`, `core/automation.mjs`, `index.js`, locales, and tests. A
helper that references `items`, `locations`, or `batches` while the manifest
and schema still only declare `<module>_records` is an invalid app, even if a
custom helper test is green.

Never reference `business_commands` from runtime helpers or permission probes.
`module.json` may list `business_commands` as a shell dependency, but app code
must create automation through `ctx.commandBus.dispatch(...)` only.

## Source Inventory Checklist

Create this inventory in the plan before implementation:

```text
Screens/routes/pages:
Components/views:
API routes/server actions/RPC methods:
Database tables/models/migrations:
IndexedDB/localStorage/sessionStorage usage:
Background jobs/cron/queues/schedulers:
Files/uploads/downloads/PDFs/exports:
Auth, roles, tenant or governance rules:
External integrations and webhooks:
Validation and domain invariants:
Existing tests/fixtures/smoke flows:
Source environment variables/config:
Package dependencies that would not run as plain browser ESM:
Libraries that must be vendored as local browser ESM or deferred:
```

For every item, record one of:

```text
ported now
ported later/deferred
rejected/not applicable
replaced by existing Business OS capability
```

Unclassified source behavior is a blocker because agents otherwise silently
drop backend rules, jobs, validation, or edge-case screens during the port.

## Porting Matrix Template

Put this table in the plan:

```text
Source item | Source stack pattern | Business OS target | Write model | Files/API used | Native handler needed | Proof required | Status | Deferred/rejected reason
```

Minimum source items to include:

```text
each route/page
each API/server action
each table/model/object store
each background job or scheduler
each file/artifact flow
each privileged or finalizing action
each external integration
each source test fixture or business invariant
```

Example:

```text
POST /api/invoices/:id/post | Next.js API route + Postgres transaction | ctx.commandBus.dispatch('invoices.invoice.post') -> Rust handler -> accounting_invoices/accounting_journal_entries projection | Command/projection | index.js, business_commands, src/core/business_os/invoices.rs | yes | post visible, journal visible, reload preserves both, duplicate post idempotent | active | no HTTP endpoint
```

## Framework Translation Table

| Familiar instinct | Do not do in Business OS | Business OS equivalent |
|---|---|---|
| Next.js route/page | Do not create `app/`, route files, API routes, or server components. | `module.json` declares `entry`; shell imports `modules/<module>/index.js`; export `mount(ctx)`. |
| React component tree | Do not add React/Vite/bundler or a package-managed component stack. | Plain browser ESM and DOM rendering, usually one `index.js` plus optional local view modules. |
| `useEffect(fetch(...), [])` | Do not fetch Business OS data over HTTP. | `await collection.find().exec()` plus `collection.$` or `find().$` subscription with cleanup. |
| React Query / SWR | Do not invent REST cache invalidation. | CTOX DB collection subscriptions are the live data source; WebRTC replication supplies freshness. |
| Server Action / API mutation | Do not POST to `/api/business-os/...` for records or commands. | `ctx.commandBus.dispatch(...)` for command/projection flows; direct `ctx.db` CRUD only for module-owned local records. |
| Prisma/Drizzle migration | Do not create SQL migrations for browser module data. | Declare collections in `collections.schema.json`; keep `module.json.collections` in sync. |
| Postgres table | Do not assume a module owns SQL tables. | Browser sees RxDB collections; native document store is `runtime/business-os-rxdb.sqlite3`. Core daemon state may live in `runtime/ctox.sqlite3` and be projected. |
| Foreign-key table dependency | Do not list a peer-module collection in `module.json` and then omit its schema. | Either defer the dependency and store a text/reference field, or include an identical schema definition so schema-parity passes. Shell-registered collections (`business_commands`, `ctox_queue_tasks`, `business_module_catalog`, `ctox_runtime_settings`) are the exception and must not be redeclared. |
| IndexedDB/localStorage app store | Do not call `indexedDB`, `localStorage`, or `sessionStorage` for Business OS data. | Use `ctx.db`; the shell owns IndexedDB and recovery. |
| WebSocket/SSE live updates | Do not open a custom live-data socket. | Use CTOX DB WebRTC replication plus collection subscriptions. Use `ctx.eventBus` only for local shell/cross-module signals, not persistence. |
| Backend cron/job | Do not run durable business jobs from browser timers. | Create `business_commands` and native/CTOX handlers; schedule durable work in CTOX where needed. |
| Queue/task completion | Do not call `ctox queue ack/complete/release/fail` or write queue rows from the app agent. | Let the CTOX service validator complete or rework the queue after the app passes validation. |
| File upload API | Do not add an HTTP file API. | Use `desktop_files` / `desktop_file_chunks` and the appropriate CTOX command/materialization path. |
| Auth middleware | Do not create module-local auth/session storage. | Use `ctx.session`, `ctx.governance`, and native policy gates for privileged commands. |
| npm package import | Do not use dependency management, import maps, bare imports, CommonJS, CDN runtime imports, generated app bundles, or `node:*` in browser modules. Do not prepare package-manager setup as a later activation step. | Use plain ESM, local relative `.js`/`.mjs` modules, existing shell/runtime APIs, or vendored browser ESM source committed with the module/repo. If the library is not available in that shape, defer the feature. |
| Global CSS variables | Do not put module theme variables on `:root`, `html`, or `body`, and do not redefine shell/base tokens. | Scope module-local tokens under `[data-<module>-root]` or `.<module>-app`; read shell tokens with `var(--text)` / `var(--surface)` but never assign those shared names. |
| Existing esbuild test pattern | Do not copy legacy bundler/fake-DOM tests into a new greenfield module. | Put pure helpers in one local `.mjs` file and import them directly from both `index.js` and `tests/*.test.mjs`. |
| Schema.js loader workaround | Do not build CommonJS, `node:vm`, `new Function`, or source-transform loaders so tests can execute `schema.js`. | Put plain schema objects in a shared local browser-safe `schemas.mjs` helper imported by `schema.js` and tests, or use simple JSON/text parity checks. |
| Manual command queue fallback | Do not upsert `pending_sync` docs into `business_commands` from a greenfield app when commandBus is missing. | Dispatch `business_os.chat.task` through `ctx.commandBus.dispatch`; if unavailable, disable/report the action and mark shell proof blocked. |
| Optimistic success toast | Do not mark success because a local command doc was written. | Wait for authoritative command status/result or projected collection change, then render from `ctx.db`. |
| `schema.js` as runtime truth | Do not rely on `schema.js` alone. | Runtime source is `collections.schema.json`; `schema.js` is compatibility/generator facade. |

## Pattern-By-Pattern Porting Rules

### Pages And Routes

Source pages/routes become module views, tabs, panes, or drawers. Do not create
new browser routes unless the shell already has a route for the module.

```text
list route     -> left pane filter/list or center table
detail route   -> center detail/editor with data-context-* attributes
settings route -> right inspector or settings tab inside the module
wizard route   -> drawer/modal/workflow panel, backed by real persisted state
```

Do not mechanically translate a familiar SaaS dashboard into three columns,
cards, counters, and action rails. Business OS apps are operational module
surfaces. Start from the smallest proven local layout:

```text
small module        -> one-pane table/list with inline create/edit or modal
normal work queue   -> two-pane list/detail or table/detail
context-heavy work  -> optional right inspector only when populated by live data
secondary workflow  -> modal or drawer, not permanent clutter
```

Each visible button, tab, filter, counter, and panel must either read live data
or perform a proven durable mutation/command. If the feature is deferred, keep
it in the phase plan only and remove the UI affordance.

For a small greenfield app, do not allocate a permanent right pane for KPIs,
generic automation status, or summary cards. Those normally belong in the
center detail, a modal, or a bottom drawer. A right/third pane needs an
explicit right-pane proof in the plan:

```text
Right pane live object:
Why center/modal/drawer is insufficient:
Controls in the pane:
Data source for each control:
Existing module pattern reused:
Browser proof required:
```

Without that proof, use one pane or two panes.

Use small references first. Large modules such as `customers`, `outbound`,
`ctox`, `buchhaltung`, and `notes` are valid source material only for targeted
line ranges. They are not templates for new greenfield app scope, layout, or
file size. For many new modules, the correct translation is:

```text
desktop -> minimal shell mount shape
calendar -> compact schema/test-hook shape
tickets or outbound targeted lines -> existing command dispatch shape
iot targeted lines -> two-pane operational CSS only
customers targeted lines -> command builders/context metadata only
```

### API Endpoints And Server Actions

Every source endpoint must become one of:

```text
direct ctx.db CRUD for module-owned local records
ctx.commandBus.dispatch(...) for audited/native/cross-module/finalizing effects
existing shell/Business OS capability
deferred/rejected item with no visible shipped affordance
```

There is no Business OS replacement that says "keep the API endpoint for now"
for record data.

For automation requirements phrased as "trigger a ticket", "open a chat", or
"create a work item", first look for an existing CTOX command path. Generic
follow-ups should usually dispatch `business_os.chat.task`. Ticket-specific
commands require confirming the existing native handler and payload shape before
using them. A newly invented `<module>.*` command is not implemented unless the
native handler and tests are added in the same change.

### SQL Models, ORM Entities, And Migrations

Port tables/models to JSON schemas in `collections.schema.json`. Keep
`module.json.collections` identical to the collections the module reads or
writes. Use integer cents, millisecond timestamps, stable string primary keys,
and tombstone deletes unless an existing local pattern proves otherwise.

Do not create SQL migrations for module data. SQL stores are native runtime
implementation details, not the module-facing app database.

### React State And Component Effects

Port component state to explicit module state and DOM event handlers. A control
is not ported until a non-default user change flows into persisted state or a
command payload and is visible after reload.

Port `useEffect` data loads to:

```text
initial collection.find().exec()
plus collection.$ or collection.find().$ subscription
plus unmount cleanup
```

### IndexedDB, Local Storage, And Client Caches

Source apps often use IndexedDB/localStorage for drafts, preferences, or local
offline records. In Business OS:

```text
business data      -> ctx.db collections
durable commands   -> business_commands through ctx.commandBus
shell-only UI state -> in-memory module state unless an existing shell API owns it
pairing bootstrap  -> shell/runtime only, not module code
```

### Background Jobs And Schedulers

Browser timers are not durable jobs. Port cron/queue/scheduler behavior to
CTOX-owned commands, native handlers, or existing CTOX scheduling surfaces.
Expose job state through replicated collections or command results.

### Files, PDFs, Imports, And Exports

Do not add upload/download HTTP APIs for Business OS data. Use
`desktop_files`, `desktop_file_chunks`, and command/projection flows. A PDF,
XML export, import, email attachment, or generated file is not shipped until
its file index/payload path and reload proof are clear.

### Auth, Roles, Tenants, And Governance

Use `ctx.session`, `ctx.governance`, module governance metadata, and native
policy checks. Do not create module-local auth middleware, token storage, or
role tables unless the Business OS governance model explicitly owns them.

### External Integrations

Email, Teams, WhatsApp, Jami, payment providers, OCR, ELSTER, DATEV, webhooks,
and AI approvals should be command/projection flows. The browser may collect
input and display status, but native/CTOX code owns durable execution,
approval, retry, idempotency, and external side effects.

## Write Model Decision Tree

Before coding, classify every collection:

```text
Does the browser module own the record and is the effect local/reversible?
  yes -> Module-owned direct CRUD through ctx.db
  no  -> Command/projection flow through business_commands and native handler

Does the action post, lock, approve, send, import, export, schedule, allocate,
archive, touch files, touch another module, or need audit/idempotency?
  yes -> Command/projection flow

Does the collection start with ctox_* or represent runtime/users/tickets/module
catalog/channel state/queue/tasks/knowledge/core state?
  yes -> CTOX-owned projection; browser is read-only except command docs
```

The plan must name the write model for every collection the app reads or
writes. Ambiguous ownership is a blocker.

## Common Agent Traps

These mistakes usually come from generic web-app habits:

```text
creating API routes because "the frontend needs data"
using fetch('/api/business-os/...') for CRUD or commands
using IndexedDB/localStorage directly because "it is a local-first app"
building React/Vite/Next.js scaffolding for a no-build module
declaring schema.js but forgetting collections.schema.json
adding collections to module.json without matching schema definitions
writing projection collections directly instead of command documents
unwrapping ctx.db.raw and keeping stale collection handles across data-plane recovery
reading data once and expecting the shell to refresh it
dispatching a command but not proving authoritative status/projection after reload
returning native ok/completed for no-op or future-work handlers
advertising App Store capabilities that are only stubs or planned phases
patching rxdb/dist directly or changing wire constants on one side
```

## Required Architecture Map In Plans

Every non-trivial app plan must include this table before implementation. For
ports, this table is in addition to the source inventory and porting matrix.

```text
Generic pattern/assumption | Business OS equivalent | Files/API used | Proof required | Rejected shortcut
```

Minimum rows:

```text
Data loading
Data mutation
Schema/migration
Live updates
Finalizing commands
Files/artifacts if any
Auth/governance if any
External integrations if any
```

Example row:

```text
REST create invoice endpoint | ctx.commandBus.dispatch('invoices.invoice.create') -> native handler -> accounting_invoices projection | index.js, business_commands, src/core/business_os/invoices.rs | command result + invoice visible after reload | no /api/business-os/invoices POST
```

## Browser Proof Expectations

Real-shell proof must exercise architecture, not just rendering:

```text
open the actual Business OS shell
open/install the module through the shell path
confirm collections.schema.json registered before mount-dependent actions
change a non-default form value
create or update data through the selected write model
for commands, wait for authoritative result/projection
confirm the changed record appears from ctx.db subscription
reload and confirm it is still visible
confirm no Business OS data moved through HTTP
confirm unsupported commands fail instead of completing
confirm console/network errors do not show sync or schema failures
```

If the app cannot pass this proof, it may be a scaffold or local draft, but it
is not ready.
