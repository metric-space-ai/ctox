---
name: business-stack
description: Use when CTOX installs, initializes, upgrades, or customizes the separate CTOX Business Basic Stack repository with Sales, Marketing, Operations, and Business modules.
metadata:
  short-description: Install and customize the separate CTOX Business Basic Stack
cluster: product_engineering
---

# Business Stack

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when the owner asks CTOX to create or modify the CTOX Business
Basic Stack.

The business stack is a generated customer-owned repository. It is not the CTOX
core repository and must not be overwritten by `ctox install` or `ctox upgrade`.

## Core Rule

CTOX core owns the daemon, SQLite runtime state, skills, queues, ticket state,
verification, and orchestration.

The generated business repository owns the Next.js app, Postgres schema,
module code, public website content, deployment configuration, and all customer
customization.

## Vanilla Template

The source template lives in:

```text
templates/business-basic/
```

It contains:

- `apps/web`: Next.js app shell with public entry, login, and internal module
  routes.
- `packages/db`: Postgres/Drizzle schema baseline.
- `packages/ctox-bridge`: event bridge contract between business data and CTOX.
- `packages/ui`: shared navigation model, design tokens, theme CSS, shell, and
  layout components.
- `modules/sales`: CRM module contract.
- `modules/marketing`: website, campaigns, assets, research, commerce contract.
- `modules/operations`: projects, work items, boards, wiki, meetings contract.
- `modules/business`: products, invoices, bookkeeping, reporting contract.

## Mission/Vision Bootstrap Contract

When a new Business OS tenant has a company name plus mission and vision, CTOX
should initialize the tenant with useful placeholder data so every module opens
as a working surface rather than an empty shell.

Use a thin central bootstrap orchestrator with module-owned demo generators:

- The orchestrator owns tenant inputs: company name, mission, vision, locale,
  industry hints, deployment mode, and whether demo data should be generated.
- Each module owns its own seed function and prompt/instruction contract:
  Marketing creates website/page/campaign/research placeholders, Sales creates
  campaigns/pipeline/leads/offers/customers placeholders, Operations creates
  projects/work items/knowledge/meetings placeholders, Business creates products,
  invoices, reports, exports, and bookkeeping placeholders, and CTOX creates
  queue/task/bug-report examples.
- Module seeds must be idempotent and write only through the Postgres runtime
  APIs or Drizzle seed helpers. Use stable `externalId`/`ctoxSyncKey` values so
  repeated bootstraps update placeholders instead of duplicating them.
- The central orchestrator may call every module seed, but it must not contain
  one giant cross-module content blob. Cross-module links are passed as typed
  references such as campaign-to-lead, offer-to-customer, or customer-to-project.
- The module seed files may use the mission and vision to shape copy, naming,
  next actions, research prompts, campaign criteria, and operating assumptions,
  but they must never hard-code a real tenant's private customer data.

This keeps the business stack maintainable: module behavior changes next to the
module, while installation still has one obvious "bootstrap demo tenant" action.

Recommended generated entrypoints:

```text
apps/web/lib/bootstrap-demo.ts
apps/web/lib/marketing-seed.ts
apps/web/lib/sales-seed.ts
apps/web/lib/operations-seed.ts
apps/web/lib/business-seed.ts
apps/web/lib/ctox-seed.ts
apps/web/app/api/settings/bootstrap-demo/route.ts
```

The bootstrap endpoint should accept:

```json
{
  "companyName": "Example GmbH",
  "mission": "What the company exists to do.",
  "vision": "What future state the company wants to create.",
  "mode": "empty | demo | guided",
  "locale": "en | de"
}
```

Modes:

- `empty`: create organization/settings only.
- `demo`: generate placeholder records for every module from mission and vision.
- `guided`: generate a reviewable bootstrap plan first, then write selected
  module placeholders after approval.

Tenant-specific real data belongs in the tenant Postgres database or external
object storage references. It must not be committed back into the CTOX core repo
or the vanilla template. Demo data in the CTOX template must stay generic and
synthetic.

## Installation Pattern

Create the business stack as a separate Git repo with the bundled installer:

```sh
python3 skills/system/product_engineering/business-stack/scripts/install_business_stack.py \
  --target <target-dir> \
  --init-git
```

Use `--dry-run` first when the target is unclear. The installer refuses to
write into a non-empty directory, excludes build artifacts such as
`node_modules` and `.next`, copies `.env.example` to `.env`, and writes:

```text
.ctox-business-install.json
```

That manifest marks the generated repository as customer-owned and records that
CTOX core upgrades must never overwrite it in place.

Then configure Postgres and run migrations from the generated repo.

Do not add the generated repo as a mutable subdirectory of CTOX core unless the
owner explicitly asks for a monorepo experiment.

Do not run this installer automatically from `ctox install` or `ctox upgrade`.
Those commands may suggest installing or updating the business stack, but the
business repository is created and changed only through an explicit business
stack action.

Verify the installer itself from the CTOX core repo with:

```sh
python3 skills/system/product_engineering/business-stack/scripts/test_install_business_stack.py
```

The smoke test covers dry-run behavior, non-empty target rejection, excluded
build artifacts, manifest ownership, `.env` creation, and `--init-git`.

## Public Entry Contract

The root page `/` must always render.

The vanilla template must not ship opinionated landing-page content. It only
wires the public Next.js surface with:

- one discreet login button in the top-right corner
- an empty public content area for customer-specific website customization

Internal applications start behind `/login` and `/app`.

Do not remove the top-right login entry during marketing customization.

## Unified UI/UX Contract

The modules are development boundaries, not separate visual products.

Global UI/UX belongs in `packages/ui`:

- `src/theme/theme.css`: CSS variables and default theme.
- `src/theme/tokens.ts`: typed design tokens.
- `src/theme/modes.ts`: global light/dark theme mode registry.
- `src/components/*`: shared shell and layout primitives.
- `src/i18n/*`: locale registry, shell messages, and language-switching helpers.
- `src/navigation/model.ts`: module, submodule, and deep-link registry.

Module work may customize functionality, fields, workflows, and local content.
It must not fork the shell, navigation model, spacing scale, typography, or
core interaction patterns unless the owner explicitly asks for a global redesign.

Per-module design changes should be token overrides, not new design systems:

```css
[data-module="sales"] {
  --module-accent: #23665f;
}
```

Keep the default experience visually quiet and consistent across all modules.

The authenticated workspace should feel like a modern operating-system app:
quiet, dense, and functional. Avoid marketing-page composition, visual noise,
decorative backgrounds, oversized hero sections, and module-specific visual
languages inside the app.

Light and dark mode are global shell state. Modules must not implement their
own theme switchers. Deep links may include `theme=light|dark`.

Item actions should be available through the shared right-click context-menu
primitive where that matches the view. The context menu is a productivity
shortcut, not the only path to an action; important actions still need visible
buttons, drawers, or keyboard-accessible controls.

The right-click menu must always include `Prompt CTOX`. Every context-menu item
must expose structured context metadata:

```text
data-context-module
data-context-submodule
data-context-record-type
data-context-record-id
data-context-label
```

When the user submits a prompt from the context menu, create a CTOX queue task
with the instruction plus the clicked or selected items. The task must be
specific enough for CTOX to know exactly which module, submodule, and record(s)
the user means without asking for clarification.

## Language Contract

Language switching is global. The workspace shell owns the language switcher,
and every module must use the shared locale registry instead of implementing a
module-local language picker.

The default locale is English. The registry starts with English and German:

```text
packages/ui/src/i18n/locales.ts
packages/ui/src/i18n/messages.ts
```

To add another language, extend `localeRegistry`, add shared shell messages, and
then add module-specific translation files next to the module code. Do not fork
the shell for a language.

CTOX can inspect available languages through:

```http
GET /api/ctox/locales
```

Deep links may include `locale=<code>`:

```text
/app/sales/accounts?locale=en&theme=light&recordId=<id>&panel=record&drawer=right
```

## Navigation And Link Contract

The internal app uses a two-level top navigation. Every working view must be
reachable with at most two navigation clicks:

1. module row: Sales, Marketing, Operations, Business, CTOX
2. submodule row: sections for the active module

Navigation stops at the submodule. A submodule route must immediately render a
working view such as a kanban board, table, inbox, wiki list, report grid, or
calendar. Do not add deeper navigable app pages for records or actions.

The registry lives in `packages/ui/src/index.ts`. Do not hard-code the same
module paths in several places when customizing. Update the registry first, then
let pages and API routes read from it.

All functional detail below a submodule is expressed as UI state inside the
same view:

- right drawer for record details, editing, comments, and agent context
- bottom drawer for timelines, logs, bulk actions, and long-running jobs
- left-bottom drawer for filters, quick create, and secondary tools

Use query parameters for shareable state rather than deeper routes:

```text
/app/sales/accounts?recordId=<id>&panel=record&drawer=right
```

Drawer targets must toggle like OS panels. The first click opens the drawer;
clicking the same target again returns to the base submodule URL while
preserving `locale` and `theme`. The visible `Close` action must do the same.
Do not introduce record-detail pages for normal module work.

CTOX needs REST-addressable deep links for outbound mail, tickets, and status
updates. Preserve these endpoints:

```http
GET /api/ctox/navigation
GET /api/ctox/locales
GET /api/ctox/links?module=sales&submodule=accounts&recordId=<id>&drawer=right&locale=en&theme=light
GET /api/ctox/links/sales/accounts/<id>?locale=en&theme=light
POST /api/ctox/queue-tasks
```

The canonical UI route pattern is:

```text
/app/<module>/<submodule>?recordId=<id>&panel=record&drawer=right&locale=en&theme=light
```

If a customization changes routes, update the registry and link API together so
CTOX-generated mail links keep working.

## Upgrade Pattern

Never overwrite customer code in place.

To update a generated business repo, create a branch in that repo and propose a
normal Git diff:

```sh
git switch -c codex/business-stack-upgrade-<date>
```

Apply template changes selectively, adapt migrations, run checks, and explain
what changed. Let the owner merge or reject the upgrade like any other product
change.

## Customization Pattern

Treat the generated business repo as the product source of truth.

- Read existing module code before changing it.
- Keep module boundaries explicit.
- Add Postgres migrations for business data changes.
- Mirror only integration events to CTOX; do not move CTOX core runtime state
  into Postgres.
- Keep bug reports and agent work linked through the CTOX bridge.
- Preserve the public entry and internal app shell unless the owner explicitly
  approves a replacement.

## Verification

After installation or customization, start the web app and run:

```sh
pnpm test:all
```

This runs TypeScript, UI contract checks, Operations coverage, full module
route/API/queue smoke tests, and the production build. Interactive QA should
also cover at least one drawer toggle per module, right-click `Prompt CTOX`,
bug report markup, language/theme preservation, and any module-specific editor
or axis controls changed in the task.
