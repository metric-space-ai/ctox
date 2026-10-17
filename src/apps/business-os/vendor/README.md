# Vendored Runtime Libraries

Business OS runs without `npm install` or a bundler during normal development.
Runtime libraries in this directory are committed as browser-ready ESM bundles:
one `.mjs` file per library.

Libraries used by exactly one app live in that app (`modules/<id>/vendor/`,
Shell v4 isolation rule R1, `v4/tools/isolation/REPORT.md`): lexical → notes,
research-graph (+ provenance, licenses) → research, event-calendar + rrule →
calendar, document-format → documents, store-shelf + three → app-store,
chat-ui → coding-agents, easy-email-editor → mail, jszip →
customer-modules/outbound-lead-generation. Libraries shared by several apps are
Shell standard libraries addressed as `ctox:lib/<name>@<major>/…`
(`v4/tools/isolation/standard-libs.json`): ctox-office@1, monaco@0.

Current runtime files:

- `superdoc.mjs`: bundled SuperDoc runtime for DOCX editing.
- `superdoc.css`: SuperDoc stylesheet loaded by the documents module.

CTOX Sync Engine is not vendored here anymore. The active package-manager-free browser
runtime lives at `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`, with source
and tests next to that artifact.

Build scripts for generated bundles live in `src/scripts/vendor-builds/`.
The Research Graph bundle (now in `modules/research/vendor/`) is rebuilt explicitly with
`node src/scripts/vendor-builds/build-research-graph.mjs --install`; npm is a
build-time source fetcher only and is never required by the Business OS
browser runtime.
