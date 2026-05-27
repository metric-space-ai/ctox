# Vendored Runtime Libraries

Business OS runs without `npm install` or a bundler during normal development.
Runtime libraries in this directory are committed as browser-ready ESM bundles:
one `.mjs` file per library.

Current runtime files:

- `document-format.mjs`: DOCX import, Markdown import/export, and document text
  extraction helpers for the documents module.
- `superdoc.mjs`: bundled SuperDoc runtime for DOCX editing.
- `superdoc.css`: SuperDoc stylesheet loaded by the documents module.

CTOX DB is not vendored here anymore. The active package-manager-free browser
runtime lives at `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`, with source
and tests next to that artifact.

Build scripts for generated bundles live in `src/scripts/vendor-builds/`.
