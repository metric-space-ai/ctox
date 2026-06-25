# Business OS App Do's And Don'ts

## Do

- Build a real app for the user's request.
- Choose three relevant shipped Business OS apps as references before coding.
- Use plain HTML fragments, CSS, and browser ESM.
- Export `mount(ctx)` from `index.js`.
- Render into `ctx.host`.
- In runtime-installed apps, load `index.html` into `ctx.host` from `mount(ctx)`
  or render the primary UI in JS. The shell does not preload runtime module
  `index.html`.
- Persist app records through shell-provided collection handles from
  `ctx.db.collection('<declared-collection-name>')`.
- Scope every runtime app collection name to the module id, for example
  `inventory_v0_1_items`, so independent generated apps and future versions do
  not collide in CTOX DB.
- Send workflow automation through `ctx.commandBus.dispatch(...)`.
- Use `business_os.chat.task` for normal AI/chat follow-up with `payload.record_snapshot`.
- Show returned `task_id`/`command_id` as a real tracking control that opens the
  CTOX Flow/Queue focus. Use `ctox.businessOs.focusTask` plus a `#ctox` hash,
  following existing shipped apps such as Matching, Research, Documents, or
  Business Chat.
- Use `ctox.ticket.local.create/comment/transition` only for real local ticket lifecycle actions.
- Keep the first version small, focused, and fully working.
- Prefer one or two panes; use modals for occasional detail work.
- Use unique function names in `index.js`; do not shadow a top-level render or
  helper function with a nested function of the same name.
- Include a visible create flow for the primary record type, especially in the
  empty state.
- Create/edit forms must include a visible Save or Submit control that actually
  completes the workflow.
- Query modals and forms from the DOM parent that actually contains them.
  `root.querySelector(...)` is wrong when the modal is a sibling of `root`.
- If you implement a custom modal or overlay with a `hidden` attribute, add the
  matching CSS rule so it cannot block clicks while hidden.
- Add tests for pure record helpers and automation command builders.
- Run `ctox business-os app validate <module-id> --installed` or `--source`.
- Run `ctox business-os app smoke <module-id> --installed` against the real
  Business OS shell and fix any dead create flow, mount error, or console error.
- Treat shipped source apps as workflow examples, not as runtime manifest
  templates. Adapt source manifests to the runtime contract.

## Don't

- Do not build a React, Next.js, Vite, bundled, or package-managed app.
- Do not create a separate HTTP, REST, IndexedDB, Postgres, or SQLite data path.
- Do not use `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, or cached DB facade handles.
- Do not call `ctx.db.registerSchemas` from app code; declare module schemas in
  `collections.schema.json` and `schema.js`.
- Do not use shared/domain collection names such as `inventory_items`,
  `contracts`, or `quality_complaints` for runtime-generated apps.
- Do not write generated runtime apps under `src/` unless the task explicitly targets a source module.
- Do not write directly to `business_commands`; use `ctx.commandBus.dispatch(...)`.
- Do not write directly to `ctox_ticket_*` projection collections.
- Do not render queue, command, or task ids as inert text when the user needs
  to track the run in CTOX.
- Do not use a full HTML document in `index.html`.
- Do not assume `index.html` is already present in `ctx.host` when `mount(ctx)`
  starts.
- Do not add decorative third panes, fake buttons, or controls without handlers.
- Do not claim a visible Create/New/Add button works until it has been clicked
  in the real Business OS shell.
- Do not copy `layout.icon_svg`, inline SVG, `store.installable`, or
  `entry: modules/...` from a source manifest into a runtime-installed app.
- Do not stop, start, restart, upgrade, bootout, disable, or otherwise manage
  the CTOX service while building an app. The service must stay running for the
  app creator, RxDB/WebRTC, validation, smoke, and E2E proof.
- Do not copy internal shell/developer apps such as App Creator, App Store,
  Browser, CTOX, Credentials, or Coding Agents as default business-app UI
  templates.
- Do not add broad settings/export/AI/bulk features unless they really work.
- Do not treat a generic empty template as the finished app.
- Do not claim success while validation is red.
