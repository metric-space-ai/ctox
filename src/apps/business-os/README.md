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

The shell loads module manifests through the native Rust API and mounts modules
as plain browser modules. React may be embedded for menus, settings, and dense
forms, but the working views should remain direct, inspectable ESM.

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
