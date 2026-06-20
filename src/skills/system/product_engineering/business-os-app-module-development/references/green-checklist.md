# Green Checklist

Use this before claiming a Business OS app is done.

- The target directory is correct for runtime or source mode.
- Three relevant shipped Business OS apps were chosen and inspected.
- The app is vanilla HTML/CSS/browser ESM with no build step.
- `index.js` exports `mount(ctx)`.
- `mount(ctx)` renders the HTML fragment into `ctx.host`.
- `index.css` is loaded by the module or otherwise available through the app contract.
- App records use `ctx.db` and declared module collections.
- `schema.js`, `collections.schema.json`, and record helper outputs agree on
  collection names, schema versions, required fields, and property types.
- Automation uses `ctx.commandBus.dispatch(...)`.
- Chat/AI actions use `business_os.chat.task` with `payload.record_snapshot`; real ticket lifecycle actions use `ctox.ticket.*`.
- The UI has no decorative panes or dead controls.
- The empty state lets the user create at least one primary business record.
- Hidden modals, drawers, and overlays really stop intercepting clicks when hidden.
- Core workflows implemented in the UI actually work.
- Tests cover record helper behavior and automation payloads.
- `ctox business-os app validate <module-id> --installed` or `--source` passes.
