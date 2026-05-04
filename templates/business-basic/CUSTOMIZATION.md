# CTOX Business Stack Customization

This repository is generated from the CTOX Business Basic template and then
owned by the customer.

## Ownership

CTOX core owns the local SQLite runtime state, agent queue, skills,
orchestration, verification, and upgrade process.

This generated repository owns the Next.js app, Postgres schema, module code,
public website content, customer workflows, deployment configuration, and
business data model.

## Upgrade Rule

Core upgrades must never overwrite this repository in place.

Template improvements are applied as normal Git changes inside this repository:

```sh
git switch -c codex/business-stack-upgrade-<date>
```

Review the diff, adapt migrations, run checks, and merge only when the owner
accepts the product change.

## Customization Rule

Customize module behavior inside the generated repo. Keep the global app shell,
navigation model, language switching, theme state, bug reporting, context menu,
and CTOX queue bridge shared across all modules unless the owner explicitly
approves a global redesign.

Business state belongs in Postgres. CTOX runtime state stays in SQLite. Share
only integration events, deep links, bug reports, and queued instructions
through the CTOX bridge.

## UI Contract

Every internal submodule is one working view. Records, editors, prompts,
filters, logs, and create flows open in drawers or modals instead of deeper
pages.

Drawer interaction stays uniform across modules:

- first click opens the drawer
- repeated click on the same drawer target closes it
- `Close` returns to the base submodule URL
- `locale` and `theme` are preserved

Before shipping a customization, run:

```sh
pnpm test:all
```

Keep these APIs stable so CTOX can send links and queue contextual work:

```http
GET /api/ctox/navigation
GET /api/ctox/locales
GET /api/ctox/links
POST /api/ctox/queue-tasks
```

The default route shape is:

```text
/app/<module>/<submodule>?recordId=<id>&panel=record&drawer=right&locale=en&theme=light
```
