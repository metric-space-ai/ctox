# Business OS App Do's And Don'ts

## Do

- Build a real app for the user's request.
- Choose three relevant shipped Business OS apps as references before coding.
- Use plain HTML fragments, CSS, and browser ESM.
- Use structured file-edit tools for app files. In CTOX Codex runs, use
  `apply_patch`; use shell commands for inspection, validation, and tests.
- Include `icon.svg` and set `"icon": "icon.svg"` in the runtime
  `module.json`.
- Export `mount(ctx)` from `index.js`.
- Render into `ctx.host`.
- Put `data-context-record-id`/`-record-type`/`-label` on every record row/card/tree node so the shell right-click hands the agent that record (see module-contract "Agent Context").
- In runtime-installed apps, load `index.html` into `ctx.host` from `mount(ctx)`
  or render the primary UI in JS. The shell does not preload runtime module
  `index.html`.
- Persist app records through shell-provided collection handles from
  `ctx.db.collection('<declared-collection-name>')`.
- Scope every runtime app collection name to the module id, for example
  `inventory_v0_1_items`, so independent generated apps and future versions do
  not collide in CTOX Sync Engine.
- Send workflow automation through `ctx.commandBus.dispatch(...)`.
- Use `business_os.chat.task` for normal AI/chat follow-up with `payload.record_snapshot`.
- Show returned `task_id`/`command_id` as a real tracking control that opens the
  CTOX Flow/Queue focus. Use `ctox.businessOs.focusTask` plus a `#ctox` hash,
  following existing shipped apps such as Matching, Research, Documents, or
  Business Chat.
- Use `ctox.ticket.local.create/comment/transition` only for real local ticket lifecycle actions.
- Keep the first version small, focused, and fully working.
- For runtime-installed business apps, set `module.json` `layout.shell` to
  `full-workspace` so the app owns the visible central Business OS work area.
- Optimize the primary user path for direct action. Booking, parking,
  scheduling, shift, and availability apps should expose a calendar/date-strip
  or equivalent slot view with one-click claim/release/book actions.
- Prefer one or two panes inside the app only when both panes contain real
  business workflow content; use modals for occasional detail work.
- Style the app with Business OS theme tokens such as `--bg`, `--surface`,
  `--surface-2`, `--text`, `--muted`, `--line`, and `--accent`, and verify the
  result in light, dark, and one custom-brand fixture.
- Build the frame and every recurring control from the preloaded
  `shared/base.css` kit classes (design-guide "Component Kit"): pane headers
  (`.ctox-pane-header`/`.ctox-pane-band` + kicker/title + `.ctox-pane-actions`),
  `.ctox-pane-search`/`.ctox-pane-filter`, `.ctox-button`/`.ctox-pane-icon`,
  `.ctox-input`/`.ctox-select`, `.ctox-table`, `.ctox-fields`, `.ctox-badge`,
  `.ctox-chip`, `.ctox-modal`, `.ctox-empty`. The static check fails an app
  that renders no kit classes.
- Make header/list primary actions compact icon buttons (`.ctox-pane-icon`)
  with `aria-label` and `title`, using `ctx.getActionIcon('<name>')` glyphs.
  Text buttons belong in toolbars, forms, and modal footers only.
- Treat standalone vanilla apps as portable only when storage and automation
  already sit behind `mount(ctx)`, `ctx.db.collection(...)`, and
  `ctx.commandBus.dispatch(...)`.
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

- Do not render record rows, cards, or tree nodes without
  `data-context-record-id`/`-record-type`/`-label`. A record list the agent
  cannot right-click into is an incomplete app, not a finished one.
- Do not build your own right-click/context menu or a `ctox:context-action`
  bridge; the shell owns the right-click -> agent flow.
- Do not build a React, Next.js, Vite, bundled, or package-managed app.
- Do not create a separate HTTP, REST, IndexedDB, Postgres, or SQLite data path.
- Do not use `ctx.db[name]`, `ctx.db.collections`, direct
  `ctx.db.<collection>` property access, or cached DB facade handles.
- Do not call `ctx.db.registerSchemas` from app code; declare module schemas in
  `collections.schema.json` and `schema.js`.
- Do not use shared/domain collection names such as `inventory_items`,
  `contracts`, or `quality_complaints` for runtime-generated apps.
- Do not write generated runtime apps under `src/` unless the task explicitly targets a source module.
- Do not create or edit app source files through shell heredocs, `cat >`, `tee`,
  Python file writers, or Node file writers when a structured file-edit tool is
  available.
- Do not write directly to `business_commands`; use `ctx.commandBus.dispatch(...)`.
- Do not add generic "Report to CTOX", "An CTOX melden", queue, AI, or
  command-bus buttons unless the user asked for that workflow or the app
  dispatches a real command and shows a trackable result.
- Do not write directly to `ctox_ticket_*` projection collections.
- Do not render queue, command, or task ids as inert text when the user needs
  to track the run in CTOX.
- Do not use a full HTML document in `index.html`.
- Do not assume `index.html` is already present in `ctx.host` when `mount(ctx)`
  starts.
- Do not add decorative third panes, fake buttons, or controls without handlers.
- Do not make users open a form or modal for a frequent one-click action such as
  claiming, releasing, booking, or marking availability on a visible slot.
- Do not build stacked app chrome. The Business OS shell already has the global
  header and app/version controls; a module gets at most one compact commandbar
  before the actual work surface.
- Do not put category/title hero blocks, duplicate app names, version bars,
  metrics strips, date strips, and filters into separate header rows. Fold the
  minimum controls into one compact commandbar or make the calendar/work grid
  the first real surface.
- Do not allow physically impossible duplicate claims. Resource apps must block
  overlaps such as one vehicle/person/asset being booked into two slots at the
  same time.
- Do not change an ESM helper's exports while leaving the import URL unchanged.
  Browsers cache modules by URL; use a versioned helper filename/import path
  when `index.js` depends on new helper exports.
- Do not leave runtime-installed apps in the generic shell side-pane layout
  where users see only `Kontext` and `Themen` columns around the app.
- Do not create empty left/right app columns just because reference apps have
  them. Every pane must have a real repeated workflow use.
- Do not hard-code a dark-only app surface, force `color-scheme`, or ship CSS
  that becomes unreadable when the shell switches between light and dark theme.
- Do not hard-code hex/rgb theme colors on surfaces, text, borders, or accents
  anywhere in app CSS; the static check fails color-bearing declarations that
  do not resolve through Business OS tokens.
- Do not rebuild kit components locally (own button/badge/table/modal/search
  CSS that mirrors `shared/base.css`), and do not put wide text buttons into
  pane headers where the standard is a `.ctox-pane-icon` icon button.
- Do not define Business OS tokens on `:root`, `html`, or `body`; workspace
  branding owns those token values.
- Do not port standalone package-manager setup, auth, HTTP APIs, localStorage
  persistence, or app-owned sync into Business OS.
- Do not claim a visible Create/New/Add button works until it has been clicked
  in the real Business OS shell.
- Do not copy `layout.icon_svg`, inline SVG, `store.installable`, or
  `entry: modules/...` from a source manifest into a runtime-installed app.
- Do not use `icon_url`, `icon_path`, remote icons, or inline SVG icon fields in
  a runtime-installed app manifest.
- Do not stop, start, restart, upgrade, bootout, disable, or otherwise manage
  the CTOX service while building an app. The service must stay running for the
  app creator, RxDB/WebRTC, validation, smoke, and E2E proof.
- Do not copy internal shell/developer apps such as App Creator, App Store,
  Browser, CTOX, Credentials, or Coding Agents as default business-app UI
  templates.
- Do not add broad settings/export/AI/bulk features unless they really work.
- Do not treat a generic empty template as the finished app.
- Do not claim success while validation is red.
