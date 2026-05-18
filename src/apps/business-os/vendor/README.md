# Vendored Runtime Libraries

Business OS runs without `npm install` or a bundler during normal development.
Runtime libraries in this directory are committed as browser-ready ESM bundles:
one `.mjs` file per library.

Current runtime files:

- `document-format.mjs`: DOCX import, Markdown import/export, and document text
  extraction helpers for the documents module.
- `rxdb-bundle.mjs`: RxDB, Dexie storage, and WebRTC replication exports.
- `superdoc.mjs`: bundled SuperDoc runtime for DOCX editing.
- `superdoc.css`: SuperDoc stylesheet loaded by the documents module.

Build scripts for generated bundles live in `src/scripts/vendor-builds/`.
