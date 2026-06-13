# Business OS App Architecture Porting Guide

Use this reference when building or modifying CTOX Business OS app modules. It translates common web-app assumptions into the native Business OS architecture.

## Mental Model

A Business OS app is not a standalone web app. It is a no-build browser ESM module loaded by the Business OS shell. The shell owns the window, session, layout, database runtime, command bus, notifications, context menus, drawers, and synchronization.

The module owns:

- its manifest and module-local files
- its module-owned collection schemas
- its DOM rendering under `ctx.host`
- subscriptions to shell-provided collection handles
- UI event handlers
- command payload builders for automation
- cleanup of listeners, timers, and subscriptions

The module does not own:

- a server
- a package manager
- a React/Next.js runtime
- a database connection string
- HTTP business-data APIs
- upstream RxDB plugins
- a separate IndexedDB database
- global shell design tokens

## Known Good Existing Modules

Read at least three before coding. Prefer these combinations:

```text
CRUD app: modules/notes
Customer-linked business data: modules/customers
Planning/date workflows: modules/shiftflow
Automation-heavy workflows: modules/outbound
App creation/install flows: modules/creator and modules/app-store
Complex finance object model: modules/buchhaltung or modules/invoices when present
```

For each module, inspect:

```sh
sed -n '1,220p' src/apps/business-os/modules/<id>/module.json
sed -n '1,220p' src/apps/business-os/modules/<id>/collections.schema.json
sed -n '1,220p' src/apps/business-os/modules/<id>/schema.js
rg -n "export function mount|commandBus|business_commands|upsert|find\\(|subscribe|modal|drawer|data-" src/apps/business-os/modules/<id>
find src/apps/business-os/modules/<id> -maxdepth 3 -type f | sort
```

## Translation From Familiar Frameworks

| Familiar pattern | Business OS app equivalent |
|---|---|
| Next.js app route | `index.html` plus `index.js` `mount(ctx)` |
| React state store | module-local JS state object, collection subscriptions, render functions |
| Server action | `ctx.commandBus.dispatch(...)` business command |
| REST endpoint | not for business data; use CTOX DB WebRTC collections or command bus |
| Postgres migration | `schema.js` plus `collections.schema.json` collection definition |
| ORM model | JSON schema plus pure normalization/validation helpers |
| Background job | CTOX queue task created from a `business_commands` record |
| Webhook/ticket | normal CTOX chat/ticket command payload |
| npm package | existing shipped ESM vendor file or browser API |
| build step | not allowed |
| global CSS theme | shell tokens and module-local CSS only |

## Data Plane

Business OS uses CTOX DB:

- Browser runtime: `ctox-rxdb-js`
- Native peer: `rxdb-rs`
- Persistence path on CTOX side: the native Business OS RxDB SQLite store managed by CTOX
- Sync: WebRTC only
- App-facing access: the `ctx.db` facade from the Business OS shell

Do not use or mention `ctox.db` as a real API. Do not create a local SQLite database. Do not use raw IndexedDB. Do not call HTTP pull/push/command/status endpoints for business data.

## Collection Creation

Collections are created by module registration, not by ad hoc table creation from app code.

When an app needs a new collection:

1. Choose a stable collection name, usually snake_case and domain-scoped.
2. Add it to `module.json` `collections`.
3. Add the JSON schema to `schema.js` `collections`.
4. Mirror it in `collections.schema.json`.
5. Add migration strategy entries when increasing schema versions.
6. Use the shell-provided collection handle at runtime.

The Business OS shell and native peer register collections from these schema artifacts. If a collection is missing from `schema.js`, it may silently fail to sync. If `collections.schema.json` is missing, the runtime/module tooling may not know how to create or validate the collection on install.

## Shell Collections

Some collections are owned by the shell:

```text
business_commands
ctox_queue_tasks
business_module_catalog
ctox_runtime_settings
```

A module may list these in `module.json` if it uses them. Do not redeclare them as module-owned collections unless an existing module uses that exact compatibility pattern and the conformance guard accepts it. Prefer declaring only module-owned collections in new schemas and using `ctx.commandBus` for command writes.

## UI Surfaces

Business OS is an operational workspace, not a landing page. Start small:

- left pane for lists, filters, scopes, or navigation
- center pane for the primary workbench and selected record
- modal or drawer for create/edit flows
- right pane only when there is a persistent separate context, inspector, assistant, or audit stream

Avoid the "always three columns" failure mode. If the third column is only summaries, tips, or extra buttons, remove it.

Every visible affordance must have a working chain:

```text
button/control
-> event handler
-> validation
-> collection mutation or command dispatch
-> visible state update
-> test/smoke assertion
```

## Automation Payloads

Each new app should include at least one automation action that creates a normal CTOX work item. Good examples:

- contracts: "create renewal follow-up"
- inventory: "create reorder task"
- projects: "create budget risk review"
- subscriptions: "create churn-risk follow-up"
- quality/compliance: "create audit/remediation ticket"

The payload must include enough record facts for the receiving CTOX flow to act without scraping the UI.

Use `business_os.chat.task` for chat/work items unless there is a specific existing `ctox.ticket.*` or module command pattern to follow.

## Validation Strategy

Minimum validation for one-shot app creation:

```sh
node --test <module-test-files>
node src/apps/business-os/scripts/assert-module-conformance.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
rg -n "ctx\\.db\\.raw|ctox\\.db|indexedDB|/rxdb/pull|/commands|fetch\\(|require\\(|package\\.json|node_modules" <module-dir>
```

If the app is generated under `installed-modules/<id>` in a runtime install, run targeted module tests and the forbidden-pattern scan against that directory, then use the full guard suite on a source checkout before upstreaming it.

## Common Agent Failures

Avoid these exact failures:

- creating a skill file instead of the app
- creating a "harness trace" or "skill trace" file as the deliverable
- using a plan/doc as the output when code was requested
- adding `package.json` because the model thinks a web app needs dependencies
- importing upstream `rxdb`
- writing `ctx.db.raw`
- inventing `ctox.db`
- adding HTTP fallback APIs
- making a third pane full of static hints
- adding buttons that only set status text
- writing tests that prove helper functions but not the visible actions
- declaring collections in only one of `module.json`, `schema.js`, or `collections.schema.json`
- using fixed seed ids that collide on repeated runs
