# Green Checklist

Use this before claiming a Business OS app is done.

- The target directory is correct for runtime or source mode.
- Three relevant shipped Business OS apps were chosen and inspected.
- The app is vanilla HTML/CSS/browser ESM with no build step.
- `index.js` exports `mount(ctx)`.
- `mount(ctx)` renders the HTML fragment into `ctx.host`.
- `index.css` is loaded by the module or otherwise available through the app contract.
- App records use `ctx.db` and declared module collections.
- Automation uses `ctx.commandBus.dispatch(...)`.
- The UI has no decorative panes or dead controls.
- Core workflows implemented in the UI actually work.
- Tests cover record helper behavior and automation payloads.
- `ctox business-os app validate <module-id> --installed` or `--source` passes.
