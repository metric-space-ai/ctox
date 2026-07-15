# Green Checklist

Use this before claiming a Business OS app is done.

- The target directory is correct for runtime or source mode.
- Three relevant shipped Business OS apps were chosen and inspected.
- The app is vanilla HTML/CSS/browser ESM with no build step.
- Runtime `module.json` sets `"icon": "icon.svg"` and the module directory
  contains a local `icon.svg`.
- Runtime `module.json` sets root `launch_kind` to `desktop-app`, writes the
  canonical `presentation` object (minimum 640×480), and keeps
  `layout.shell: windowed` only as a compatibility hint.
- The app is usable at its minimum width and responds to its window container,
  not only to the browser viewport.
- `IMPECCABLE_PREFLIGHT` passed with Product register plus the CTOX root
  `PRODUCT.md`, `DESIGN.md`, and `.impeccable/design.json` context.
- Routine controls remain compact and neutral. At most one real, domain-named
  AI/automation action is visually dominant per visible work surface.
- Real mouse dragging resizes the floating window and every visible
  `.ctox-column-resizer`; no direct style mutation is accepted as proof.
- At 360px the shell uses a mobile app sheet with Start, version/status,
  Source/Versions, close/back, chat, and task switching reachable.
- Each two-/three-pane layout preserves a visible stack/tab/drawer and return
  path for panes that cannot remain side by side.
- The app has at most one compact app-level command/header row. It does not
  repeat shell-owned app identity/version/source chrome or stack hero, metrics,
  date-strip, and filter headers before the work surface.
- `index.js` exports `mount(ctx)`.
- `mount(ctx)` loads `index.html` into `ctx.host` or renders an equivalent
  primary UI into `ctx.host`; it does not assume the shell preloaded the
  fragment.
- `index.css` is loaded by the module or otherwise available through the app contract.
- App records use declared module collections through
  `ctx.db.collection('<declared-collection-name>')`.
- No legacy DB fallback exists: no `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, cached DB facade, raw IndexedDB, HTTP,
  or app-owned sync path.
- App code does not call `ctx.db.registerSchemas`; schema registration comes
  from module metadata and the Business OS shell/native peer.
- Runtime app collection names are scoped to the module id.
- `schema.js`, `collections.schema.json`, and record helper outputs agree on
  collection names, schema versions, required fields, and property types.
- Every collection version above 0 has all intermediate JSON
  `migration_strategies`; persisted schemas were never edited in place.
- Automation uses `ctx.commandBus.dispatch(...)`.
- Chat/AI actions use `business_os.chat.task` with `payload.record_snapshot`; real ticket lifecycle actions use `ctox.ticket.*`.
- Automation results that return `task_id` or `command_id` are visible and
  clickable from the originating record, opening the CTOX Flow/Queue focus via
  `ctox.businessOs.focusTask` and `#ctox?...`.
- The app does not include a generic "Report to CTOX" / "An CTOX melden" /
  queue / AI / command-bus button unless that automation was requested or is a
  real workflow with a trackable result.
- The UI has no decorative panes or dead controls.
- Any left/right column inside the app contains real workflow content and is not
  an empty copy of shell context/topics.
- `index.css` uses Business OS theme tokens for surfaces, borders, and text,
  does not force `color-scheme`, and does not define root Business OS tokens.
- No color-bearing CSS declaration hard-codes hex/rgb theme colors; everything
  resolves through tokens or `color-mix(...)` over tokens.
- The UI is built from `shared/base.css` kit classes (pane header with
  kicker/title and `.ctox-pane-actions`, kit controls, `.ctox-table`,
  `.ctox-fields`, `.ctox-badge`, `.ctox-modal`, `.ctox-empty`) instead of
  app-local rebuilds; header primary actions are `.ctox-pane-icon` icon
  buttons with `aria-label`/`title` and `ctx.getActionIcon` glyphs.
- The app was visually checked in light and dark theme at desktop and narrow
  viewport sizes; text, buttons, cards, dialogs, and bottom actions remain
  readable and do not overlap.
- Long German, English, and unbreakable technical app names remain within a
  fixed two-line desktop icon cell and keep the full accessible name/tooltip.
- The app was visually checked against one custom-brand fixture, proving it
  consumes shell tokens instead of hard-coded root palettes.
- Every record row/card/tree node exposes `data-context-record-id`/`-record-type`/`-label` (or at least a `data-*-id`) so a right-click hands the agent the record.
- Browser proof right-clicks a record and confirms the "Chat to CTOX" popover opens showing that record (its label/id), proving the agent receives the click target.
- The empty state lets the user create at least one primary business record.
- Primary Create/New/Add controls are clicked in the real Business OS shell and
  reveal a usable dialog, form, or save flow.
- Hidden modals, drawers, and overlays really stop intercepting clicks when hidden.
- Core workflows implemented in the UI actually work.
- For booking, parking, scheduling, shift, availability, or date/slot domains,
  the common claim/release/book path works in one click from the visible
  calendar/date/slot view.
- Resource/date apps enforce domain conflicts in that one-click path, for
  example one vehicle/person/asset cannot be booked into two overlapping slots.
- Any changed local ESM helper export is loaded through a fresh helper URL
  (for example a versioned helper filename), and a browser reload proves the
  module does not fail on stale helper imports.
- Tests cover record helper behavior and automation payloads.
- `ctox business-os app validate <module-id> --installed` or `--source` passes.
- `ctox business-os app smoke <module-id> --installed` passes.
- If the app began standalone, the port removed app-owned persistence/sync and
  production code now uses shell-provided `ctx.db` and `ctx.commandBus`.
- No service lifecycle command was used during the app build:
  no `ctox stop/start/upgrade`, `launchctl`, `systemctl`, bootout, disable, or
  daemon restart.
