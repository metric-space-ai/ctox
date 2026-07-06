# CTOX Native Business OS

This is the native, no-build Business OS surface for CTOX.

The app is served by the Rust runtime and is intentionally made of editable
HTML, JavaScript, and CSS files. Vendored libraries live in `vendor/`; normal
module work must not require `npm install`, a bundler, or a framework runtime.

## Module Contract

Each module owns a small directory:

```text
modules/<id>/
  module.json
  schema.js
  index.html
  index.js
  index.css
```

`module.json` uses `install_scope` to decide how the app is shipped:

- `core`: always included and not removable (`ctox`, `tickets`, `desktop`, `app-store`, `knowledge`, `reports`).
- `starter`: included on first installation as the standard workspace pack (`documents`, `spreadsheets`, `calendar`, `notes`).
- `store`: discoverable in the App Store and installed into `installed-modules/` on demand.
- `internal`: shipped for shell-owned workflows but hidden from normal launchers.
- `sample`: ignored by the runtime; used only for checked-in example `installed-modules/`.
- `installed`: runtime-installed via the App Creator/App Store into
  `runtime/business-os/installed-modules/` (git-ignored, survives upgrades).
- `local`: operator-placed, git-ignored dev/customer modules (see below).

### Local Modules (git-ignored dev and customer apps)

`runtime/business-os/local-modules/<id>/` is the third module location:
hand-developed apps that must never land in the public repo — private test
modules or per-customer apps (e.g. a `sellify` app). `runtime/` is already
git-ignored, so no `.gitignore` entry per app is needed. Dropping the module
directory there IS the install: the runtime discovers it, projects it into
the module catalog with `source: "local"` / `install_scope: "local"`, serves
its files under `/local-modules/<id>/…`, registers its
`collections.schema.json` collections, and shows it as a launcher tab on this
instance. The app-store install/uninstall lifecycle does not manage local
modules; deleting the directory removes the app.

A local module uses the exact same contract as any other module
(`module.json` with `entry: "local-modules/<id>/index.html"` and
`install_scope: "local"`, `schema.js` + `collections.schema.json` with
module-scoped collection names, `mount(ctx)`, kit classes, tokens). Validate
with:

```sh
node src/apps/business-os/scripts/validate-app-module.mjs <id> --local
```

Local mode enforces the same structural, data-boundary, and design rules as
installed mode; only the business-behavior requirements (mandatory automation
command, create affordance) are relaxed for quick test apps.

The shell loads module manifests through the native Rust API and mounts modules
as plain browser modules. React may be embedded for menus, settings, and dense
forms, but the working views should remain direct, inspectable ESM.

### Writing A Module

The shell imports `index.js` and calls `mount(ctx)`; the context carries
everything a module is allowed to touch: `host`, `db`, `sync`, `commandBus`,
`eventBus`, `contextMenu`, `notifications`, `windowManager`, drawer openers,
`locale`, and `permissions` (full pinned contract:
`docs/business-os-module-context.md`). `mount` returns an unmount/cleanup
function.

Styling comes from two shell-owned layers that are loaded once for every app:

- `app.css` defines the primitive design tokens (`--bg`, `--surface`,
  `--surface-2`, `--line`, `--text`, `--muted`, `--accent`, `--danger`,
  `--panel-radius`, `--control-radius`, `--font-sans`, `--font-mono`, ...).
- `shared/base.css` is the module base kit — the construction set every app
  builds its frame and controls from: `.ctox-workspace` (the standard 3-pane
  grid wired to the shell column resizers), `.ctox-pane`, `.ctox-pane-body`,
  `.ctox-pane-band`, the pane header blocks (`.ctox-pane-actions`,
  `.ctox-pane-icon`, `.ctox-pane-search`, `.ctox-pane-filter`,
  `.ctox-pane-tabs`/`.ctox-pane-tab`), `.ctox-toolbar`, `.ctox-button`,
  `.ctox-input`, `.ctox-chip`, `.ctox-card`, `.ctox-list`,
  `.ctox-table`/`.ctox-table-wrap`, `.ctox-fields`, `.ctox-modal`,
  `.ctox-avatar`, `.ctox-choice`, `.ctox-empty`, `.ctox-badge`, plus derived
  semantic tokens (`--line-strong`, `--success`, `--warning`). Modules use
  these classes directly — no import needed — and keep `index.css` for what
  is genuinely module-specific. Module-local color names should alias shell
  tokens (see `modules/customers/index.css`), which makes light/dark theming
  automatic. `modules/conversations/` is the layout reference.

Two hard UI conventions on top of the kit:

- Pane-header primary actions are compact `.ctox-pane-icon` buttons with an
  `aria-label` and `title` — never wide text buttons. Text buttons
  (`.ctox-button`, `.is-primary`, `.is-danger`) belong in toolbars, forms,
  and modal footers.
- Action glyphs come from the shared set: `ctx.getActionIcon(name)` in
  JS-generated markup (names via `listActionIcons()` in `shared/icons.js`);
  static SVGs in `index.html` follow the same style (24 viewBox,
  `stroke="currentColor"`, stroke-width 1.8, no fills). Do not invent
  per-module icon styles.

Known pitfalls the conformance guard
(`scripts/assert-module-conformance.mjs`, run in CI) catches early:

- every collection in `module.json` must be declared in `schema.js` — a
  missing declaration does not error, it silently never replicates
- schema version bumps need `migrationStrategies` exported from `schema.js`
- `mount(ctx)` is the only supported signature; do not unwrap `ctx.db.raw`
  (raw handles go stale when the data plane recovers from schema drift)
- module CSS must not write tokens on `:root`, redefine shell/base tokens, or
  `@import` remote stylesheets/fonts
- ship `locales/de.json` + `locales/en.json` and load them through
  `shared/i18n.js`

## Data Runtime Contract

Business OS modules use **CTOX DB**, the CTOX-owned browser data runtime backed
by `ctox-rxdb-js` and the native `rxdb-rs` peer. It is RxDB-derived, but it is
not upstream npm `rxdb` and not a drop-in replacement for arbitrary RxDB
plugins.

Module code must use database, collection, sync, and command handles supplied
by the Business OS shell context. Do not import `rxdb` or `rxdb/plugins/...`
from a module. The app-facing compatibility contract is
`ctox-db-business-os-v1`.

The Tickets app is the reference core module for native CTOX capability
projection. It reads only replicated `ctox_ticket_*` collections and writes
durable `ctox.ticket.*` command documents through `business_commands`; it does
not use a module-local HTTP command bridge.

## Layout Contract

Every module should preserve the same operating pattern:

- left pane: navigation, filters, source context, or queue scope
- center pane: primary workbench and selected record list
- right pane: topic context, inspector, activity, or assistant thread
- left drawer: global/module navigation and setup
- bottom drawer: selected items from the center workbench
- right drawer: focused topic details from the right pane

This keeps user workflows and CTOX prompt context stable across modules.

### Deliberately Free-Form Apps

The kit is the default, not a cage. When a workflow explicitly needs it, an
app may ship a fully custom web UI (`layout.shell: "full-workspace"` or a
windowed desktop app) and skip the pane grid entirely. Two things still hold:

- Tokens and conformance rules apply unchanged — a free-form app still theams
  with the workspace.
- Data still flows through the shell context (`ctx.db`, `ctx.commandBus`).
  An app-external persistence layer cuts the app off from every other app —
  no shared records, no right-click record context, no CTOX automation — and
  is the explicit exception, not a convenience. Integrating an external data
  source belongs on the CTOX side (backend sync into replicated collections),
  not in the browser app.

## First Blueprint

`modules/matching/` is the first migrated module blueprint. It uses
the Business Basic color tokens from the former Next.js app and ports the
NinjaWorkflowTool Matching view as the first concrete matching
example:

- companies in the left pane
- job postings and match actions in the center pane
- candidates in the right pane
- job details from the left drawer
- candidate profiles from the right drawer
- match evidence and queued skill commands from the bottom drawer

The generalization contract for configurable importers, parsers, structure
prompts, and scoring criteria lives in
`modules/matching/REQUIREMENT_MATCHING.md`.

## Trademark Notice

CTOX and CTOX Business OS are names used by this project. The repository
license grants rights to the covered source code, but it does not grant
trademark rights or permission to present modified versions as official CTOX
products. Forks and redistributed builds should use their own product branding
unless they have separate permission to use the CTOX name, logos, or service
marks.
