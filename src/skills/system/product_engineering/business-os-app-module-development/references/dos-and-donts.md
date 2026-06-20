# Business OS App Do's And Don'ts

## Do

- Build a real app for the user's request.
- Choose three relevant shipped Business OS apps as references before coding.
- Use plain HTML fragments, CSS, and browser ESM.
- Export `mount(ctx)` from `index.js`.
- Render into `ctx.host`.
- Persist app records through the shell-provided `ctx.db` collection handle.
- Send workflow automation through `ctx.commandBus.dispatch(...)`.
- Use `business_os.chat.task` for normal AI/chat follow-up with `payload.record_snapshot`.
- Use `ctox.ticket.local.create/comment/transition` only for real local ticket lifecycle actions.
- Keep the first version small, focused, and fully working.
- Prefer one or two panes; use modals for occasional detail work.
- Use unique function names in `index.js`; do not shadow a top-level render or
  helper function with a nested function of the same name.
- Include a visible create flow for the primary record type, especially in the
  empty state.
- Create/edit forms must include a visible Save or Submit control that actually
  completes the workflow.
- If you implement a custom modal or overlay with a `hidden` attribute, add the
  matching CSS rule so it cannot block clicks while hidden.
- Add tests for pure record helpers and automation command builders.
- Run `ctox business-os app validate <module-id> --installed` or `--source`.
- Treat shipped source apps as workflow examples, not as runtime manifest
  templates. Adapt source manifests to the runtime contract.

## Don't

- Do not build a React, Next.js, Vite, bundled, or package-managed app.
- Do not create a separate HTTP, REST, IndexedDB, Postgres, or SQLite data path.
- Do not write generated runtime apps under `src/` unless the task explicitly targets a source module.
- Do not write directly to `business_commands`; use `ctx.commandBus.dispatch(...)`.
- Do not write directly to `ctox_ticket_*` projection collections.
- Do not use a full HTML document in `index.html`.
- Do not add decorative third panes, fake buttons, or controls without handlers.
- Do not copy `layout.icon_svg`, inline SVG, `store.installable`, or
  `entry: modules/...` from a source manifest into a runtime-installed app.
- Do not copy internal shell/developer apps such as App Creator, App Store,
  Browser, CTOX, Credentials, or Coding Agents as default business-app UI
  templates.
- Do not add broad settings/export/AI/bulk features unless they really work.
- Do not treat a generic empty template as the finished app.
- Do not claim success while validation is red.
