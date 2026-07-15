# Vendored Runtime Libraries

Business OS runs without `npm install` or a bundler during normal development.
Runtime libraries in this directory are committed as browser-ready ESM bundles:
one `.mjs` file per library.

Current runtime files:

- `research-graph.mjs`: pinned browser ESM closure for the Web Research
  force-directed 2D/3D graph surface. Its complete package inventory, hashes,
  origins, and licenses are recorded in `research-graph.provenance.json` and
  `research-graph.LICENSES.txt`.
- `document-format.mjs`: DOCX import, Markdown import/export, and document text
  extraction helpers for the documents module.
- `superdoc.mjs`: bundled SuperDoc runtime for DOCX editing.
- `superdoc.css`: SuperDoc stylesheet loaded by the documents module.

CTOX Sync Engine is not vendored here anymore. The active package-manager-free browser
runtime lives at `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`, with source
and tests next to that artifact.

Build scripts for generated bundles live in `src/scripts/vendor-builds/`.
The Research Graph bundle is rebuilt explicitly with
`node src/scripts/vendor-builds/build-research-graph.mjs --install`; npm is a
build-time source fetcher only and is never required by the Business OS
browser runtime.
