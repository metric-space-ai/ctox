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
Place local browser ESM helpers under `lib/*.mjs` and vendored browser ESM
helpers under `vendor/*.mjs`; import them with relative paths. Do not add
package-manager manifests, lockfiles, or dependency directories.
When a helper's export surface changes, change the imported helper URL too
(for example with a versioned helper filename such as `lib/records-v2.mjs`) so
an already-open browser cannot keep an old ESM module in cache while loading a
new `index.js`.

## Runtime Shape

- `index.html` is the app's local HTML fragment. It must not contain `<!doctype>`, `<html>`, `<head>`, `<body>`, `<link>`, `<script>`, `<meta>`, `<title>`, or inline `<style>`.
- `index.css` is plain CSS scoped under a module root class.
- `index.js` is browser ESM and exports `mount(ctx)`.
- Runtime apps must include `icon.svg` and set `"icon": "icon.svg"` in
  `module.json`. Do not use remote icons, `icon_url`, `icon_path`, or inline SVG
  fields in runtime manifests.
- `mount(ctx)` renders into `ctx.host`, wires handlers, subscribes to data, and returns optional cleanup.
- For runtime-installed apps, `mount(ctx)` must load `index.html` itself, or
  render an equivalent primary UI into `ctx.host` itself. Do not assume the
  Business OS shell has already inserted `index.html` into `ctx.host`.
- Query DOM references from the container that actually contains those
  elements. If dialogs or forms are siblings of the module root section, query
  them from `ctx.host`, not from the inner root section.
- Use only local relative ESM imports or shipped browser ESM files. Do not add package-manager dependencies.
- Keep local ESM helper imports cache-safe. If `index.js` starts importing a
  new export from a helper, bump the helper filename/import path or otherwise
  force a fresh module URL, then validate in a real browser reload.

## Shell Layout And Theme

- Runtime-installed business apps must set `module.json` `layout.shell` to
  `"full-workspace"`. The generic shell `Kontext` and `Themen` panes are for
  shell diagnostics, not for normal generated business-app UX.
- Build the app's own information architecture inside `ctx.host`. Use a focused
  single workspace, a two-pane workbench when the left pane contains real
  navigable business records, and modals/drawers for occasional details. Do not
  leave empty, decorative, or duplicate side columns.
- The shell already supplies the global header, app switcher, active app
  identity, version/source controls, account controls, and chat. The module may
  add at most one compact app-level command/header row for local filters and
  primary actions. Do not stack multiple app headers such as category/title
  hero, version bar, date strip, metrics strip, and filter row before the real
  work surface.
- The app must inherit light/dark theme from the Business OS shell. Use shell
  tokens such as `var(--bg)`, `var(--surface)`, `var(--surface-2)`,
  `var(--text)`, `var(--text-strong)`, `var(--muted)`, `var(--line)`,
  `var(--accent)`, and `var(--accent-soft)` for backgrounds, borders, text, and
  controls.
- Do not set `color-scheme` in app CSS, do not hard-code a dark-only or
  light-only palette for the root work surface, and do not rely on white text on
  fixed dark backgrounds. Status colors may use domain-specific accents only
  when normal surfaces/text still come from Business OS tokens.
- Do not define Business OS tokens on `:root`, `html`, or `body` in app CSS.
  Workspace admins can customize both light and dark token values. Apps consume
  tokens; the shell owns the palette. See `design-guide.md`.
- Before delivery, visually check light, dark, and one custom-brand fixture.

## Workflow Ergonomics

- Build the common path as the shortest interaction, not as a generic CRUD form.
- For booking, scheduling, shift, parking, availability, capacity planning, or
  other date/slot domains, include a calendar or date-strip view. The visible
  slot card/row should provide one-click claim/release/book actions for the
  normal case.
- Use forms, modals, or drawers for optional details, setup, admin changes, and
  exceptional edits. Do not make a normal user open a modal just to claim or
  release a visible slot.
- Do not add a generic "Report to CTOX", "An CTOX melden", queue, AI, or
  command-bus button as a default app affordance. Add visible automation only
  when the user requested it or the domain workflow clearly needs it.
- If an automation action is visible, it must dispatch a real command, show
  failure/success honestly, and expose a trackable `task_id`/`command_id` result
  when the command creates one.

## Workflow Ergonomics

- Build the common path as the shortest interaction, not as a generic CRUD form.
- For booking, scheduling, shift, parking, availability, capacity planning, or
  other date/slot domains, include a calendar or date-strip view. The visible
  slot card/row should provide one-click claim/release/book actions for the
  normal case.
- For physical resources and time slots, enforce the real-world invariant in
  the click path. A person, vehicle, room, desk, device, or other constrained
  asset must not be claimable into two overlapping slots just because two
  buttons are visible.
- Use forms, modals, or drawers for optional details, setup, admin changes, and
  exceptional edits. Do not make a normal user open a modal just to claim or
  release a visible slot.
- Do not add a generic "Report to CTOX", "An CTOX melden", queue, AI, or
  command-bus button as a default app affordance. Add visible automation only
  when the user requested it or the domain workflow clearly needs it.
- If an automation action is visible, it must dispatch a real command, show
  failure/success honestly, and expose a trackable `task_id`/`command_id` result
  when the command creates one.

## Data

- Module records persist through shell-provided collection handles from
  `ctx.db.collection('<declared-collection-name>')`.
- Get every module collection handle directly from the shell, for example
  `const records = ctx.db.collection('<collection>');`.
- Do not use legacy collection fallbacks such as `ctx.db[name]`,
  `ctx.db.collections`, direct `ctx.db.<collection>` property access, cached DB
  facade handles, or any app-owned fallback data path.
- For small first versions, read a module-owned collection with
  `await records.find().exec()`, convert docs with `toJSON()`, then filter and
  sort in plain JavaScript.
- Do not create a separate REST, HTTP, IndexedDB, Postgres, SQLite, localStorage, or sessionStorage data path.
- For a first app, prefer one module-owned collection for the main business object.
- Runtime app collection names must be scoped to the module id. Use
  `<module_id>_<record_name>` after replacing any non-collection-safe
  characters with underscores. Example: module `contracts_v0_1` owns
  `contracts_v0_1_contracts`, not shared names such as `contracts` or
  `contract_records`.
- If you add collections, list them in `module.json`, declare them in `collections.schema.json`, and export matching schemas from `schema.js`.
- Runtime-installed module collections are registered into the native CTOX Sync Engine
  peer from `collections.schema.json`. If a collection is missing from either
  `module.json`, `collections.schema.json`, or `schema.js`, browser and native
  persistence will disagree.
- Do not call `ctx.db.registerSchemas` from app code. Declare schemas in the
  module files and let the Business OS shell/native peer register them.
- Do not reuse domain-level collection names across generated apps or bench
  runs. Reuse can create native/browser schema drift and make WebRTC sync look
  random.
- Keep browser and native schema shapes aligned: `schema.js` and
  `collections.schema.json` must use the same collection names, versions,
  primary keys, required fields, property names, and property types.
- Persist values in the type declared by the schema. For dates, either store
  ISO date strings in `*_date` fields or declare numeric millisecond fields as
  `*_date_ms`. Do not put `Date.parse(...)` numbers into fields declared as
  `string`, and do not return `null` for fields declared only as `number`.

## Standalone Portability

- A standalone vanilla app can be a good starting point when it already exports
  `mount(ctx)` and keeps storage/automation behind the Business OS context.
- Standalone previews may use the mock context and token CSS under
  `assets/standalone/`, but production Business OS modules must receive the real
  shell-provided `ctx`.
- Do not port package-manager setup, standalone auth, HTTP APIs, localStorage
  persistence, or app-owned sync into Business OS. Replace those boundaries with
  `ctx.db.collection(...)` and `ctx.commandBus.dispatch(...)`.
- See `standalone-porting.md` before converting a standalone app.

## Automation

- Visible automation actions must call `ctx.commandBus.dispatch(...)`.
- The normal intelligent workflow command is `business_os.chat.task`.
- Use `business_os.chat.task` for follow-up chats, AI review, drafting, renewal checks, reports, and normal CTOX task continuation.
- Include both `type` and `command_type`, set `module`, set `record_id`, and include enough `payload.record_snapshot` context for CTOX to continue the workflow.
- Every visible automation that creates a CTOX command or queue task must expose
  a visible tracking affordance in the originating record. Store the returned
  `task_id` and `command_id`, and provide a button/link that sets
  `sessionStorage["ctox.businessOs.focusTask"]` and opens `#ctox` or
  `#ctox?task_id=<task_id>&command_id=<command_id>`. Do not render queue/task
  ids as dead text only.
- Use `ctox.ticket.local.create`, `ctox.ticket.local.comment`, or `ctox.ticket.local.transition` only when the app is intentionally creating or updating a real local CTOX ticket.
- Do not write directly to `business_commands` from app code.
- Do not write directly to `ctox_ticket_*` projection collections from app code.

## Agent Context (Right-Click)

- The shell owns the right-click -> agent flow. A capture-phase `contextmenu`
  handler in `src/apps/business-os/app.js` opens a "Chat to CTOX" popover and
  hands the agent `{ module, column, record_type, record_id, label, deep_link,
  selected_text, clicked_text }`. Do not build a per-app context menu or
  app-owned event bus for this flow.
- On the OUTERMOST element of every record (list row, card, table row, tree
  node), set `data-context-record-id`, `data-context-record-type`, and
  `data-context-label`. The shell walks ancestors, so child buttons inside the
  row need nothing extra. Without it the agent gets only the clicked text, not a
  record handle, and cannot tell which record the user meant.
- The shell also resolves any `data-*-id` attribute as a fallback (record type
  derived from the attribute name), so a module's own domain id such as
  `data-shift-id` works -- but the explicit trio above is preferred because it
  pins a clean type and human label.
- The agent learns `column = left | center | right` only when a pane ancestor
  matches a `*-left` / `*-right` / `*-sidebar` class or carries
  `data-left-content` / `data-right-content`. Mark side panes accordingly;
  otherwise everything reports as `center`.
- Canonical reference: `src/apps/business-os/ARCHITECTURE.md`
  ("Right-Click -> Agent Context").

Example -- annotate the record container, HTML-string or `dataset` form:

```js
// HTML string (escape values with the module's helper)
`<button class="ticket-row"
   data-context-record-id="${esc(t.id)}"
   data-context-record-type="ticket"
   data-context-label="${esc(t.title || t.id)}">...</button>`

// createElement
row.dataset.contextRecordId = t.id;
row.dataset.contextRecordType = 'ticket';
row.dataset.contextLabel = t.title || t.id;
```

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
  "icon": "icon.svg",
  "version": "0.1.0",
  "collections": ["<module_collection>"],
  "layout": { "shell": "full-workspace", "center": "module workspace" }
}
```

Use existing shipped apps as concrete examples, but adapt them to the requested app. If an old app conflicts with this contract or the validator, the current contract wins.

Do not copy these source-module manifest fields into runtime-installed apps:

- `layout.icon_svg` or any inline SVG markup. Put SVG markup in `icon.svg`.
- `icon_url`, `icon_path`, or remote icon references. Runtime apps use local
  `"icon": "icon.svg"`.
- `store.installable`, `store.editable_after_install`, or source-store distribution flags.
- `entry: modules/<id>/index.html`. Runtime apps use `installed-modules/<module-id>/index.html`.
- `layout.right` unless the app truly needs a persistent third pane and `layout.third_pane_justification` explains the workflow need.
