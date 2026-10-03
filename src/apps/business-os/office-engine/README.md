# CTOX Documents and CTOX Spreadsheets

This directory contains the browser side of the two CTOX downstream forks:
**CTOX Documents** and **CTOX Spreadsheets**. Euro-Office v9.3.1 is the pinned
source ancestry and development Oracle, not the product runtime identity. The
production forks run in browser ESM capsules and do not run Euro-Office
DocumentServer, Node.js services, databases, queues, or native C++ code.

The repository-wide implementation plan and human-readable progress tracker is
[`docs/ctox-office-port-plan.md`](../../../../docs/ctox-office-port-plan.md).
`features.json` remains the machine-readable source of truth for feature status
and dependencies.

The upstream release and every submodule revision are pinned in
`upstream/euro-office-v9.3.1.json`. Fetching or updating upstream source is an
explicit maintainer operation. Normal builds and runtime startup never clone or
download upstream code.

## Boundary

- `src/forks/ctox-documents/` and `src/forks/ctox-spreadsheets/` contain the
  product manifests and Business-OS chrome owned by each fork.
- `src/runtime/ctox-documents.mjs` and
  `src/runtime/ctox-spreadsheets.mjs` are the product runtime entry points;
  shared low-level compatibility code lives in `ctox-fork-core.mjs`.
- The remaining `src/` files contain the stable ESM capsule, iframe runtime,
  RPC and CTOX bridge source.
- The explicit vendor command stages a hash inventory under
  `runtime/vendor-sources/euro-office/document-closure-audit`; it preserves the
  upstream `web-apps`/`sdkjs` layout without placing an unreviewed bulk closure
  in `src/`. Reviewed feature slices are promoted into the adapter source only
  with their port-ledger evidence.
- `features.json` is the ordered, dependency-checked port ledger.
- `oracle/` contains deterministic browser-flow specifications and validators.
- Built fork assets live under `vendor/ctox-office/` and carry generated
  provenance. The current production provenance includes the CTOX fork source
  plus the pinned
  `web-apps`/`sdkjs` document-and-spreadsheet closure; all 24 Business-parity
  feature groups carry differential evidence. A future bootstrap-only build
  remains a development artifact and may never satisfy a production gate.
- Document bytes cross the capsule through `MessageChannel`, then through the
  Business OS database and command facades. HTTP is static-asset delivery only.
- XLSX and delimited spreadsheet resources (CSV/TSV) open in CTOX
  Spreadsheets by default. The native `office.spreadsheet.prepare` path
  canonicalizes delimited text to a typed XLSX package while Files retains the
  original downloadable resource. CTOX Spreadsheets is the only spreadsheet
  viewer; there is no legacy runtime fallback.

The capsule deliberately keeps the iframe boundary. The inherited editor core
uses process-global namespaces and global CSS; the iframe prevents those
implementation details from becoming part of the public CTOX module contract.

## Shell V2 Office workspace

Both apps use the same two-column workspace: a resizable file library with
search, view switch and filter tray on the left, and the editor in the remaining
space. The Shell owns the icon and window controls; both pane headers follow its
two-row geometry. Small windows expose the library as a dismissible drawer.
There is no permanent Runbook column. Creating a blank file is a direct action,
independent of research prompts or automation.

Source creation and import acquire a chunk lease and wait for the native peer
to acknowledge the exact persisted source rows before publishing version and
file references. Source chunks reserve room for base64 and metadata within the
wire budget. A local write alone is not a successful upload; transport or
permission failures must reject creation rather than expose an unreadable file.

The capsule forwards a validated subset of the live Shell palette and theme to
both isolated editor frames. The frames use locally generated HTML rather than
navigating to a tenant HTML response that may deny framing. No document data
path or HTTP fallback is introduced.

Both frame layers use same-origin `srcdoc` HTML. The inner editor must not
navigate to a blob URL: embedded browser hosts can reject that navigation and
leave `about:blank` without emitting an iframe load error. The embedded entry
retains its explicit asset base and launch query. Browser acceptance must cover
the embedded host as well as a standalone browser; Chromium alone is not proof
of embedded-browser compatibility.

`shell-integration.browser.mjs` mounts both actual apps and editor engines,
checks library interactions and responsive/theme changes, creates blank files,
types and saves through the UI, verifies native output, then reopens the saved
version. Its database/command facades are isolated mocks; it does not verify
tenant permissions, WebRTC replication, or deployment. Run from the repository
root after building the native CLI and serving that root on port 8766:

```sh
cargo build --manifest-path src/core/office-engine/Cargo.toml --target-dir runtime/build/cargo-target
node src/apps/business-os/office-engine/shell-integration.browser.mjs
```

The lab prepares its DOCX/XLSX fixtures using the native CLI. Screenshots and
native roundtrip artifacts are written to `output/playwright/office-integration/`.

Native document inspection (`ctox office read document INPUT`) preserves
paragraph boundaries, explicit line breaks, tabs, and blank paragraphs in
`primary_text`. Formatting runs inside a paragraph are concatenated, not
reported as separate lines. Reading does not mutate the source package or
write back to a Business OS record.

DOCX export also accepts first content in paragraphs that retain empty or
formatting-only runs from an earlier save. Those templates are preserved,
including across repeated saves; fields, comments, bookmarks, and existing
text are not discarded to resolve a run-count mismatch.

To materialize the reviewed source closure from already checked-out pinned
repositories:

```sh
npm run vendor:office -- --source=/absolute/path/to/euro-office
npm run build:office
```

The source directory must contain `web-apps` and `sdkjs` checkouts at the SHAs
in the pin file. The vendor command does not clone, fetch, or update them.

For a fork-source-only change, the already verified dependency closure may be reused
without a source checkout:

```sh
node ../../../scripts/vendor-builds/build-ctox-office.mjs --reuse-verified-upstream
```

This mode verifies every staged upstream dependency against the existing
provenance and current pin before rebuilding both CTOX fork entry points,
Business-OS chrome and ESM adapters. It cannot create a production closure from
a bootstrap-only bundle and cannot modify the pinned dependency inputs.
