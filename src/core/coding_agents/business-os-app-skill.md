# Business OS app skill

You are editing the source of a **Business OS app module** inside CTOX. Business
OS apps are not generic web pages — they follow a fixed shell/runtime contract.
Your edits are applied as versioned commits to the module's synced source; there
is no host filesystem and no build step. Keep changes minimal, idiomatic, and
consistent with the conventions below.

## Module layout

A module directory contains:

- `module.json` — manifest (`id`, `title`, `entry`, permissions). Do not rename
  `id`. Keep `entry` pointing at `index.html`.
- `index.html` — markup. No `<!doctype>`, `<html>`, `<head>`, or `<body>` build
  wrapper concerns; author the module body per the existing file.
- `index.js` — the app logic. Its default/entry export is a `mount(ctx)`
  function (see below). ES module, no bundler, no `npm` imports — the shell does
  not run a package manager. Use only browser-native ESM and what `ctx` provides.
- `index.css` — module styles. **Use the shared kit**, do not hand-roll layout.
- `locales/` — i18n JSON (e.g. `en.json`, `de.json`). Every user-facing string
  goes through i18n; never hardcode display text in markup or JS.
- `tests/` (or `test.js`) — the module's checks. When you change behavior, update
  the tests to the new contract rather than deleting or weakening them.

## The `mount(ctx)` contract

The shell calls `mount(ctx)` with a context object. Never invent your own data or
sync path — everything comes through `ctx`:

- Database handles are **delivered by the shell** through `ctx`. Do NOT import
  `rxdb`, do NOT open your own database, do NOT invent a sync path.
- Read/write app data only through the collection handles `ctx` provides. These
  sync over CTOX's WebRTC/RxDB mesh.
- `ctx` exposes helpers such as i18n (`ctx.t(...)`), action icons
  (`ctx.getActionIcon(...)`), the current actor/permissions, and command
  dispatch. Reuse the exact helper names already used in the file.
- Server-authoritative decisions (permissions, persistence, projections) stay on
  the CTOX side. Browser helpers may mirror UX state but are never the source of
  truth for policy or persistence.

## The kit (styling)

Business OS ships a standard component kit in `base.css` (the "Baukasten",
sections §1–18). ALL modules are migrated onto it.

- Use the kit's classes (shell, cards, buttons, form controls, tables, toolbars)
  and the design tokens (CSS custom properties) — never hardcode colors, spacing,
  or typography as raw values.
- Match the surrounding module's existing class usage. If the file uses a kit
  card/toolbar pattern, extend that pattern; do not introduce a parallel bespoke
  layout.
- A validator enforces kit + token usage. Off-kit CSS is a finding, not a style
  choice.

## Data boundary (hard rule)

Business OS data is **not an HTTP API surface**. Collections, module runtime
data, commands, files, and status sync through CTOX over WebRTC/RxDB — never
through `fetch`/HTTP to a CTOX endpoint. Do not add an HTTP data bridge or
fallback. If you need data, use the `ctx` collection handles.

## Command dispatch

Mutations that change server state go through the shared `business_commands`
control plane (a `ctx`-provided dispatch), gated by server-side policy. Do not
add a UI-only gate for a server mutation, and do not bypass the command path.

## Working style

- Make the smallest change that satisfies the task. Preserve public API,
  `data-*` hooks, class names, and i18n keys that other code depends on.
- Keep the module snappy: only data loads async; the UI itself renders
  immediately. Loaders should be invisible in normal operation.
- Prefer editing existing functions over adding parallel ones. Read the file
  first; write code that reads like the code already there.
