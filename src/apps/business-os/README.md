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

The shell loads module manifests through the native Rust API and mounts modules
as plain browser modules. React may be embedded for menus, settings, and dense
forms, but the working views should remain direct, inspectable ESM.

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
