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
  runtime module schemas from collections.schema.json
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
| AI assistant action | `business_os.chat.task` through `ctx.commandBus.dispatch(...)` with `payload.record_snapshot` |
| Create a real local ticket | `ctox.ticket.local.create` through `ctx.commandBus.dispatch(...)` |
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

The reference catalog is not a copy-paste manifest template. Many shipped
source apps are packaged shell modules and may contain source-only fields such
as `layout.icon_svg`, `store.installable`, `entry: modules/...`, or a persistent
third pane. For runtime-created apps, follow the module contract and validator:
use `icon.svg`, `installed-modules/<module-id>/index.html`, no store install
flags, and no third pane unless the workflow truly needs it.

Avoid internal shell/developer tools as default references for business apps.
App Creator, App Store, Browser, CTOX, Credentials, and Coding Agents are useful
only when building a similar shell/developer control surface.

## Porting Discipline

When adapting a known app idea:

1. Name the main business record.
2. Choose the smallest useful collection shape.
3. Choose one focused UI workflow.
4. Add one real automation action.
5. Leave future dashboards, exports, AI buttons, and bulk operations out unless they are fully implemented.

The app should feel like a Business OS work surface, not a generic SaaS landing page or a React demo.

## Data Plane Discipline

For runtime-created apps, every module-owned collection must exist in all three
places: `module.json` `collections`, `collections.schema.json`, and `schema.js`.
The browser registers `schema.js`; the native CTOX DB peer registers
`collections.schema.json`. If those disagree, the app may appear in the shell
but fail when reading, writing, or syncing records.

For a small app, prefer whole-collection reads (`find().exec()`) and plain
JavaScript filtering/sorting. Reach for selector/sort queries only when the
collection is declared correctly and the app actually needs query-window
behavior.
