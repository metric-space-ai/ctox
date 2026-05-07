# CTOX Business Basic Stack

This template is the vanilla business application surface for CTOX-managed
workspaces. It is intentionally separate from the CTOX core installer and
upgrade path.

CTOX core owns the daemon, local SQLite runtime state, skills, queue, tickets,
verification, and agent orchestration. A generated business repository owns the
Next.js application, Postgres schema, module code, customer customizations, and
deployment configuration.

## Installation

Install this template through the CTOX business-stack skill, not through the
core installer or core upgrade path:

```sh
python3 skills/system/product_engineering/business-stack/scripts/install_business_stack.py \
  --target <target-dir> \
  --init-git
```

The installer copies this template into a separate customer-owned repository,
excludes build artifacts, creates `.env` from `.env.example`, and writes
`.ctox-business-install.json`. That manifest records that the generated repo is
customizable product code and must not be overwritten by CTOX core upgrades.

See `CUSTOMIZATION.md` after generation for the local ownership and upgrade
rules.

From the CTOX core repository, the installer can be smoke-tested with:

```sh
python3 skills/system/product_engineering/business-stack/scripts/test_install_business_stack.py
```

## Modules

- Sales: CRM, accounts, contacts, leads, opportunities, tasks, and pipeline.
- Marketing: public website, materials, campaigns, research, and commerce
  surfaces.
- Operations: projects, work items, boards, wiki, meetings, documents, and
  day-to-day execution.
- Business: products, customers, invoices, ledger, receipts, payments,
  bookkeeping exports, and reporting.

All modules share the same shell, login entry, design system, bug reporting,
audit model, and CTOX bridge.

## Public Website Boundary

The Business OS is an internal application. Its pages and internal module APIs
are protected by the Business OS access guard. The public website should live in
its own repository and deployment, using Marketing / Website as the management
surface.

This template includes `public-website-repo/` as a starter for that separate
repository. It consumes only published public page metadata from:

```http
GET /api/public/website/pages
```

All internal APIs remain behind login:

```text
/app/*
/api/sales/*
/api/marketing/*
/api/operations/*
/api/business/*
/api/ctox/*
```

Business OS login can be deployed in three modes:

```env
CTOX_BUSINESS_OS_ACCESS_MODE=local
CTOX_BUSINESS_OS_ACCESS_MODE=hybrid
CTOX_BUSINESS_OS_ACCESS_MODE=website
```

- `local`: only the Business OS login cookie can enter `/app/*`.
- `hybrid`: local Business OS login and signed website sessions with the
  Business OS role can enter.
- `website`: only signed website sessions with the Business OS role can enter.

For website-login integration, configure the same secret on both deployments:

```env
# Business OS
CTOX_WEBSITE_AUTH_SECRET=<shared-secret>
CTOX_WEBSITE_SESSION_COOKIE=ctox_website_session
CTOX_BUSINESS_OS_ROLE=business_os_user
CTOX_BUSINESS_OS_ADMIN_ROLE=business_os_admin

# Public website
WEBSITE_AUTH_SECRET=<shared-secret>
BUSINESS_OS_URL=https://business.example.com
NEXT_PUBLIC_BUSINESS_OS_URL=https://business.example.com
```

Normal website customers must not receive `business_os_user`,
`business_os_admin`, `business_os:access`, or `business_os:admin` in their
signed website session.

## Business OS Module Coverage

Each internal submodule now renders a concrete working view instead of a generic
fallback board. Starter data is deliberately useful for a new B2B service or SaaS
business while still being safe to replace during customer customization.

Sales routes:

```text
/app/sales/pipeline
/app/sales/accounts
/app/sales/contacts
/app/sales/leads
/app/sales/tasks
```

Marketing routes:

```text
/app/marketing/website
/app/marketing/assets
/app/marketing/campaigns
/app/marketing/competitive-analysis
/app/marketing/research
/app/marketing/commerce
```

Operations routes:

```text
/app/operations/projects
/app/operations/work-items
/app/operations/boards
/app/operations/planning
/app/operations/knowledge
/app/operations/meetings
```

Business routes:

```text
/app/business/customers
/app/business/products
/app/business/invoices
/app/business/ledger
/app/business/receipts
/app/business/payments
/app/business/bookkeeping
/app/business/reports
```

CTOX control routes:

```text
/app/ctox/runs
/app/ctox/queue
/app/ctox/knowledge
/app/ctox/bugs
/app/ctox/sync
```

The module APIs read starter records from Postgres when `DATABASE_URL` points to
a real database. If the relevant module tables are empty and
`CTOX_BUSINESS_AUTO_SEED` is not `false`, the app idempotently seeds the vanilla
starter records first. Without a configured Postgres URL, the same records are
served from local seed files.

The APIs expose starter records and queue intended mutations into CTOX:

```http
GET  /api/sales
POST /api/sales/<resource>
GET  /api/marketing
POST /api/marketing/<resource>
GET  /api/operations
POST /api/operations/<resource>
GET  /api/business
POST /api/business/<resource>
GET  /api/ctox/<resource>
GET  /api/ctox/queue-tasks
POST /api/ctox/queue-tasks
GET  /api/ctox/bug-reports
POST /api/ctox/bug-reports
```

Run the full route/API/queue smoke test against a running dev server:

```sh
pnpm test:business-stack
```

Run the accounting persistence smoke test against a local Postgres server:

```sh
pnpm test:accounting-db
```

The test creates a temporary database, applies every Drizzle SQL migration,
persists an accounting setup and invoice workflow snapshot, checks journal /
ledger readback, and verifies that posted journal entries and ledger rows are
blocked from mutation. Set `DATABASE_ADMIN_URL` when the default
`postgres://$USER@localhost:5432/postgres` admin connection is not valid.

Run the complete local verification suite against a running dev server:

```sh
pnpm test:all
```

`test:all` runs TypeScript, the UI contract smoke test, the Operations smoke
test, the full Business Stack route/API/queue smoke test, and the production
Next.js build. Keep this command green before handing a customized stack back
to the owner.

## UI/UX Platform Contract

The four modules are development boundaries, not separate products. The app
must feel like one unified workspace.

Global UI/UX belongs in `packages/ui`:

- `src/theme/theme.css`: CSS variables and default theme.
- `src/theme/tokens.ts`: typed design tokens for code-driven styling.
- `src/theme/modes.ts`: global light/dark theme mode registry.
- `src/components/*`: shared shell and layout components.
- `src/i18n/*`: locale registry, shell messages, and language-switching
  helpers.
- `src/navigation/model.ts`: module, submodule, and deep-link registry.

Module code may customize behavior and local content. It should not fork the
global shell, navigation, spacing scale, typography, or core interaction
patterns.

Per-module design overrides are allowed only through scoped tokens such as
`[data-module="sales"] { --module-accent: ... }`. This keeps local emphasis
possible without breaking the unified product feel.

The default interaction model should feel like a modern operating-system app:
quiet, dense, and functional. Avoid marketing-page composition inside the
authenticated workspace. Avoid visual noise, decorative backgrounds, oversized
hero sections, and module-specific visual languages.

Light and dark mode are global shell state. Modules must not implement their
own theme switchers. Deep links may include `theme=light|dark`.

Item actions should be available through the shared right-click context-menu
primitive where that matches the view. The context menu is a productivity
shortcut, not the only path to an action; important actions still need visible
buttons, drawers, or keyboard-accessible controls.

The right-click menu must always include a `Prompt CTOX` action. Every item that
can open this menu must expose structured context metadata:

- `data-context-module`
- `data-context-submodule`
- `data-context-record-type`
- `data-context-record-id`
- `data-context-label`

When the user prompts CTOX from the menu, the app creates a queue task through:

```http
POST /api/ctox/queue-tasks
```

The payload includes the instruction plus the clicked or selected items, so CTOX
knows exactly which module, submodule, and record context the user means.

## Language Contract

Language switching is global. The workspace shell owns the language switcher,
and every module must use the shared locale registry instead of implementing a
module-local language picker.

The default locale is English. The registry starts with English and German in
`packages/ui/src/i18n`.
Additional languages are added by extending `localeRegistry` and the shared
message dictionaries, then adding module-specific translations next to the
module code.

CTOX can inspect available languages through:

```http
GET /api/ctox/locales
```

Deep links may include `locale=<code>`:

```text
/app/sales/accounts?locale=en&theme=light&recordId=<id>&panel=record&drawer=right
```

## Navigation Contract

The internal app uses a two-level top navigation. Every working view must be
reachable with at most two navigation clicks:

1. module row: Sales, Marketing, Operations, Business, CTOX
2. submodule row: context-specific sections for the selected module

Navigation stops at the submodule. A submodule route must immediately render a
working view such as a kanban board, table, inbox, wiki list, report grid, or
calendar. Do not add deeper navigable app pages for records or actions.

The navigation model is centralized in `packages/ui/src/index.ts`. Module pages,
REST link generation, and future email/link tooling should read from that
registry instead of duplicating paths.

All functional detail below a submodule is expressed as UI state inside the
same view:

- right drawer for record details, editing, comments, and agent context
- bottom drawer for timelines, logs, bulk actions, and long-running jobs
- left-bottom drawer for filters, quick create, and secondary tools

This keeps all modules structurally identical while still allowing rich
workflows.

Drawer links must behave like operating-system panels:

- first click opens the target drawer inside the current submodule
- repeated click on the same target returns to the base submodule URL
- the drawer `Close` action also returns to the base submodule URL
- locale and theme query parameters are preserved
- modules must not create deeper record pages for normal daily work

The current interactive QA pass covers this contract across Sales, Marketing,
Operations, Business, and CTOX, plus right-click `Prompt CTOX`, bug reporting,
language/theme preservation, competitive-analysis axis controls, score-model
editing, ranking scroll, own-product highlighting, and scrape-decision prompts.

## Deep-Link API

CTOX can create stable links for outbound mail and other communication through
the app API:

```http
GET /api/ctox/navigation
GET /api/ctox/locales
GET /api/ctox/links?module=sales&submodule=accounts&recordId=<id>&drawer=right&locale=en&theme=light
GET /api/ctox/links/sales/accounts/<id>?locale=en&theme=light
POST /api/ctox/queue-tasks
```

The returned link points to the canonical UI route:

```text
/app/<module>/<submodule>?recordId=<id>&panel=record&drawer=right&locale=en&theme=light
```

Business customizations may add more submodules, but should preserve this
registry-backed link contract.

## Operations Stack

Operations is the first fully wired vanilla module slice. It follows the
Business OS rule that a submodule has one working view and all record work
happens through drawers, context menus, and queue-backed actions.

Seed data lives in:

```text
apps/web/lib/operations-seed.ts
```

The seed company is `Acme Operations`, a generic B2B SaaS / service-delivery
starter business. It includes realistic starter projects, work items, boards,
planning milestones, knowledge pages, meetings, decisions, action items,
customers, and people.

Operations routes:

```text
/app/operations/projects
/app/operations/work-items
/app/operations/boards
/app/operations/planning
/app/operations/knowledge
/app/operations/meetings
```

Submodule behavior:

- `Projects`: project tree, active work, risk/sync rail, project drawer
- `Work Items`: dense work table, status/priority/assignee drill-ins, editable work-item drawer
- `Boards`: WIP-limited kanban columns with per-column quick create
- `Planning`: timeline, calendar pressure, risk signals
- `Knowledge`: wiki/document/runbook index with linked work and queue-backed sync
- `Meetings`: meetings, notes, decisions, actions, linked work, queue-backed extraction

All clickable records and action controls expose `data-context-*` metadata so
right-click `Prompt CTOX` can queue a task with exact module, submodule, record,
and label context. Create, update, sync, and extraction flows stay in drawers or
context panels; normal Operations work does not create deeper navigable pages.

Operations REST API:

```http
GET  /api/operations
GET  /api/operations/projects
GET  /api/operations/work-items
GET  /api/operations/knowledge
GET  /api/operations/meetings
POST /api/operations/<resource>
```

`POST /api/operations/<resource>` does not blindly mutate local seed data. It
queues the intended create/update/sync/extract action into CTOX and emits a
sync event with a canonical deep link. This preserves the separation between
the vanilla template and CTOX-driven customization.

Supported mutation actions:

```text
create | update | delete | sync | extract | reschedule
```

Postgres schema definitions are defined in:

```text
packages/db/src/schema.ts
```

The schema includes Sales CRM records, Marketing workspace records, Operations
projects/work/wiki/meetings, Business ERP records, competitive-analysis scrape
state, organizations, and CTOX sync events. Set
`CTOX_BUSINESS_AUTO_SEED=false` to disable automatic vanilla seeding and keep the
database strictly externally provisioned.

The root Drizzle config is:

```text
drizzle.config.ts
```

Generate and run migrations with:

```sh
pnpm db:generate
pnpm db:migrate
```

Run the Operations smoke test against a running dev server:

```sh
pnpm test:operations
```

## Public Entry Contract

The public root route must always render, but the vanilla template does not
ship opinionated landing-page content. It only wires the public Next.js surface
and exposes a discreet login button in the top-right corner.

The login entry is part of the public shell contract and must survive website
customization.

## Ownership Contract

CTOX upgrades must not overwrite a generated business repository.

Business stack updates are proposed as normal Git changes inside the generated
business repository:

```sh
ctox business check-updates
ctox business propose-upgrade
```

The generated repo should keep its own commits, branches, migrations, and
deployment lifecycle.
