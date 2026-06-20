# Business OS App Module Contract

Use this when you need exact file and runtime details.

## Target Paths

- Runtime-created apps live under `runtime/business-os/installed-modules/<module-id>/` in the installed Business OS root.
- Source modules live under `src/apps/business-os/modules/<module-id>/` only when the task explicitly targets checked-in source.
- Do not put user-created runtime apps under `src/`; source modules are packaged store/templates.

## Required Files

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

Keep extra files rare. Use extra local ESM helpers only when the app would otherwise become harder to read.

## Runtime Shape

- `index.html` is a fragment inserted by the shell. It must not contain `<!doctype>`, `<html>`, `<head>`, `<body>`, `<link>`, `<script>`, `<meta>`, `<title>`, or inline `<style>`.
- `index.css` is plain CSS scoped under a module root class.
- `index.js` is browser ESM and exports `mount(ctx)`.
- `mount(ctx)` renders into `ctx.host`, wires handlers, subscribes to data, and returns optional cleanup.
- Use only local relative ESM imports or shipped browser ESM files. Do not add package-manager dependencies.

## Data

- Module records persist through the shell-provided `ctx.db` collection handle.
- Get collection handles from the shell, for example
  `const records = ctx.db.collection?.('<collection>') || ctx.db.collections?.<collection> || ctx.db?.<collection>;`.
- For small first versions, read a module-owned collection with
  `await records.find().exec()`, convert docs with `toJSON()`, then filter and
  sort in plain JavaScript.
- Do not create a separate REST, HTTP, IndexedDB, Postgres, SQLite, localStorage, or sessionStorage data path.
- For a first app, prefer one module-owned collection for the main business object.
- If you add collections, list them in `module.json`, declare them in `collections.schema.json`, and export matching schemas from `schema.js`.
- Runtime-installed module collections are registered into the native CTOX DB
  peer from `collections.schema.json`. If a collection is missing from either
  `module.json`, `collections.schema.json`, or `schema.js`, browser and native
  persistence will disagree.
- Keep browser and native schema shapes aligned: `schema.js` and
  `collections.schema.json` must use the same collection names, versions,
  primary keys, required fields, property names, and property types.
- Persist values in the type declared by the schema. For dates, either store
  ISO date strings in `*_date` fields or declare numeric millisecond fields as
  `*_date_ms`. Do not put `Date.parse(...)` numbers into fields declared as
  `string`, and do not return `null` for fields declared only as `number`.

## Automation

- Visible automation actions must call `ctx.commandBus.dispatch(...)`.
- The normal intelligent workflow command is `business_os.chat.task`.
- Use `business_os.chat.task` for follow-up chats, AI review, drafting, renewal checks, reports, and normal CTOX task continuation.
- Include both `type` and `command_type`, set `module`, set `record_id`, and include enough `payload.record_snapshot` context for CTOX to continue the workflow.
- Use `ctox.ticket.local.create`, `ctox.ticket.local.comment`, or `ctox.ticket.local.transition` only when the app is intentionally creating or updating a real local CTOX ticket.
- Do not write directly to `business_commands` from app code.
- Do not write directly to `ctox_ticket_*` projection collections from app code.

## Versioning

- `0.0.x`: UI/UX, feature, and bug-fix changes without data-shape changes.
- `0.x.0`: schema/database or potentially breaking changes.
- `1.0.0`: first release visible beyond the developer/founder/admin audience.
- `2.0.0` and later major versions: new parallel app line with its own module id and icon, so legacy production versions can keep running.

New durable runtime apps normally start at `0.1.0`. Do not use `v1`, `0.0.0`, or a `v` prefix.

## Manifest Basics

For a runtime-installed app, `module.json` normally uses:

```json
{
  "id": "<module-id>",
  "entry": "installed-modules/<module-id>/index.html",
  "install_scope": "installed",
  "version": "0.1.0",
  "collections": ["<module_collection>"]
}
```

Use existing shipped apps as concrete examples, but adapt them to the requested app. If an old app conflicts with this contract or the validator, the current contract wins.

Do not copy these source-module manifest fields into runtime-installed apps:

- `layout.icon_svg` or any inline SVG markup. Put SVG markup in `icon.svg`.
- `store.installable`, `store.editable_after_install`, or source-store distribution flags.
- `entry: modules/<id>/index.html`. Runtime apps use `installed-modules/<module-id>/index.html`.
- `layout.right` unless the app truly needs a persistent third pane and `layout.third_pane_justification` explains the workflow need.
