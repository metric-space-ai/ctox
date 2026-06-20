# Business OS Architecture Translation

Use this when your instinct is to build a familiar web app. Translate that instinct into the Business OS app model.

## Mental Model

```text
Business OS shell
  imports index.js
  calls mount(ctx)
  provides ctx.host, ctx.db, ctx.commandBus

Business OS app
  renders plain HTML/CSS/browser ESM into ctx.host
  stores records through ctx.db
  starts automation through ctx.commandBus.dispatch(...)

CTOX data plane
  browser CTOX DB/RxDB
  WebRTC replication
  native CTOX peer
  local runtime storage owned by CTOX
```

HTTP can serve static shell and module files. HTTP is not the app data bridge.

## Translation Table

| Familiar pattern | Business OS equivalent |
| --- | --- |
| Next.js page or route | `index.html` fragment plus `index.js` `mount(ctx)` |
| React component state | plain JS state inside `mount(ctx)` or local helpers |
| REST API for records | `ctx.db` collection reads/writes |
| Server action / background job | `ctx.commandBus.dispatch(...)` |
| Postgres table | module-owned CTOX DB collection declared in module files |
| IndexedDB/localStorage | do not use; use `ctx.db` for durable app records |
| npm package | only use local browser ESM already shipped with the app |
| build step | none; files must run directly in the browser |
| dashboard route tree | one focused workbench, usually one or two panes plus modal/drawer |

## Reference Apps

Before coding, choose the three best shipped Business OS apps yourself. Pick by:

- similar business workflow
- similar data shape
- similar UI shape
- similar automation need

Use `ctox business-os app references --json` if you need a local catalog. Do not treat any named app as mandatory; `customers`, `shiftflow`, and `outbound` are examples, not a fixed set.

## Porting Discipline

When adapting a known app idea:

1. Name the main business record.
2. Choose the smallest useful collection shape.
3. Choose one focused UI workflow.
4. Add one real automation action.
5. Leave future dashboards, exports, AI buttons, and bulk operations out unless they are fully implemented.

The app should feel like a Business OS work surface, not a generic SaaS landing page or a React demo.
