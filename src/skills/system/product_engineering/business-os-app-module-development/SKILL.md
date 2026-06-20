---
name: business-os-app-module-development
description: Use whenever CTOX, Business OS, App Creator, App Store, chat, CLI, or an inbound Business OS workflow asks an agent to build, modify, repair, review, or install a CTOX Business OS app/module. The agent builds the app itself as no-build vanilla HTML/CSS/browser ESM, persists through the shell-provided CTOX DB/RxDB handle, sends automation through commandBus, studies shipped Business OS app examples, and validates the result with CTOX app validation.
metadata:
  short-description: Build runnable CTOX Business OS app modules with vanilla ESM, CTOX DB persistence, and command-bus automation.
---

# Business OS App Module Development

Build the app. Do not create a plan, a skill file, or a generic web app.

## Mental Model

A CTOX Business OS app is mostly a normal no-build browser module:

- `index.html` is an HTML fragment, not a full document.
- `index.css` is plain CSS scoped to the module UI.
- `index.js` is browser ESM and exports `mount(ctx)`.
- There is no package manager, no bundler, no framework requirement, and no compile step.

The CTOX-specific parts are small and important:

- App files for runtime-created apps live under `runtime/business-os/installed-modules/<module-id>/` in the installed app root.
- The Business OS shell calls `mount(ctx)` and provides `ctx.host`, `ctx.db`, `ctx.commandBus`, module metadata, and shell services.
- App data persists through the shell-provided CTOX DB/RxDB collection handle. Do not create a separate IndexedDB, Postgres, SQLite, REST, or HTTP data path.
- Workflow automation goes through `ctx.commandBus.dispatch(...)`, normally as a `business_os.chat.task` command with enough record context for CTOX to continue the work.

## First Steps

1. First choose the three best existing shipped Business OS apps as references for this request.
   Pick them by similar workflow, data shape, and UI shape. Example choices might include `customers`, `shiftflow`, `outbound`, `tickets`, `notes`, `invoices`, `documents`, or `app-store`, depending on the requested app.
   If you need a local catalog, run:

```sh
ctox business-os app references --json
```

2. Inspect those three apps before implementing.
3. Identify the target:
   - runtime app: `runtime/business-os/installed-modules/<module-id>/`
   - source app: `src/apps/business-os/modules/<module-id>/`
4. Turn the user's request into a small working Business OS app:
   - primary record type
   - collection name and fields
   - list/detail or single-workspace UI
   - create/edit/archive or equivalent core workflow
   - one useful automation command
5. Implement the app files.
6. Run validation:

```sh
ctox business-os app validate <module-id> --installed
```

Use `--source` only when the task explicitly targets a checked-in source module.

## File Contract

Runtime-created apps should keep this simple structure:

```text
module.json
collections.schema.json
schema.js
index.html
index.css
index.js
icon.svg
core/records.mjs
core/automation.mjs
locales/en.json
locales/de.json
tests/*.test.mjs
```

Use `core/records.mjs` for pure record shaping, filtering, summaries, and domain calculations. Use `core/automation.mjs` for command builders. Keep `index.js` focused on mounting, DOM wiring, collection reads/writes, and dispatching commands.

## Mount Pattern

Use this shape unless the existing source module has a stronger local pattern:

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
  // Wire DOM, subscriptions, persistence, and actions here.
  return () => {
    ctx.host.innerHTML = '';
  };
}
```

`index.html` is inserted into the Business OS shell. Keep it as a fragment: no `<!doctype>`, `<html>`, `<head>`, `<body>`, `<link>`, `<script>`, or inline `<style>`.

## Data Pattern

Use the collection exposed by the shell:

```js
function collectionFrom(ctx, name) {
  return ctx?.db?.collection?.(name) || ctx?.db?.collections?.[name] || ctx?.db?.[name] || null;
}
```

For a first version, prefer one module-owned collection representing the main business object. Add more collections only when the workflow really needs separate persisted objects.

## Automation Pattern

Every App Creator business app should include at least one useful CTOX automation. For example, a selected record can create a follow-up chat task:

```js
const command = {
  type: 'business_os.chat.task',
  command_type: 'business_os.chat.task',
  module: MODULE_ID,
  record_id: record.id,
  payload: {
    title: `Follow up: ${record.title}`,
    instruction: 'Review this record and continue the normal CTOX workflow.',
    source_module: MODULE_ID,
    source_collection: COLLECTION_NAME,
    record_snapshot: record,
  },
};
await ctx.commandBus.dispatch(command);
```

Do not write directly to `business_commands` from app code. The shell command bus is the supported interface.

## Chat And Ticket Automation

Use `business_os.chat.task` for the normal intelligent workflow: ask CTOX to review a record, draft a reply, check a renewal, prepare a report, or continue work in the normal chat/task flow. Always include `payload.record_snapshot` so the worker has the current business context.

Use a `ctox.ticket.*` command only when the app is explicitly creating or updating a real CTOX ticket. Dispatch it through the same command bus:

```js
await ctx.commandBus.dispatch({
  type: 'ctox.ticket.local.create',
  command_type: 'ctox.ticket.local.create',
  module: MODULE_ID,
  record_id: record.id,
  inbound_channel: MODULE_ID,
  payload: {
    title: `Ticket: ${record.title}`,
    body: record.description || '',
    status: 'open',
    priority: record.priority || 'normal',
    source_module: MODULE_ID,
    source_collection: COLLECTION_NAME,
    source_record_id: record.id,
  },
});
```

Other local ticket lifecycle commands include `ctox.ticket.local.comment` and `ctox.ticket.local.transition`. Do not write ticket projection collections directly.

## UI Guidance

Keep the app focused. A reliable small app is better than a decorative dashboard.

- Use one pane for simple workflows or two panes for list/detail work.
- Use a modal or drawer for occasional detail work.
- Add a third persistent pane only when the user explicitly needs live side context.
- Every visible button must have real behavior.
- Avoid fake AI/export/bulk/settings controls unless fully implemented.

## Tests

Add focused Node tests for pure helpers and command builders:

- import `../core/records.mjs`
- import `../core/automation.mjs`
- assert record shaping, filtering, summaries, or domain calculations
- assert the automation command type and concrete record facts in `payload.record_snapshot`

Browser shell behavior is checked by CTOX validation and smoke tests; do not make tests depend on a fake framework build.

## Final Checklist

Before claiming success:

- Target path is correct for runtime or source mode.
- You chose and inspected three relevant shipped app examples.
- App is vanilla HTML/CSS/browser ESM.
- `mount(ctx)` renders into `ctx.host`.
- Data reads/writes use the shell-provided `ctx.db`.
- Automation uses `ctx.commandBus.dispatch(...)`.
- UI has no decorative panes or dead controls.
- Tests cover helpers and automation payloads.
- `ctox business-os app validate <module-id> --installed` or `--source` is green.

If validation fails, fix the app. Do not work around the validator by editing tests to hide a real contract issue.

## References

Load these only when needed:

- `references/dos-and-donts.md` for the short Business OS app rules.
- `references/green-checklist.md` for the final done checklist.
- `references/module-contract.md` for exact file, manifest, and schema details.
- `references/architecture-translation.md` when porting from React, Next.js, REST, SQL, IndexedDB, or another familiar architecture.
