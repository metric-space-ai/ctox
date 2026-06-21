# Green Checklist

Use this before claiming a Business OS app is done.

- The target directory is correct for runtime or source mode.
- Three relevant shipped Business OS apps were chosen and inspected.
- The app is vanilla HTML/CSS/browser ESM with no build step.
- `index.js` exports `mount(ctx)`.
- `mount(ctx)` renders the HTML fragment into `ctx.host`.
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
- Automation uses `ctx.commandBus.dispatch(...)`.
- Chat/AI actions use `business_os.chat.task` with `payload.record_snapshot`; real ticket lifecycle actions use `ctox.ticket.*`.
- The UI has no decorative panes or dead controls.
- The empty state lets the user create at least one primary business record.
- Primary Create/New/Add controls are clicked in the real Business OS shell and
  reveal a usable dialog, form, or save flow.
- Hidden modals, drawers, and overlays really stop intercepting clicks when hidden.
- Core workflows implemented in the UI actually work.
- Tests cover record helper behavior and automation payloads.
- `ctox business-os app validate <module-id> --installed` or `--source` passes.
- `ctox business-os app smoke <module-id> --installed` passes.
