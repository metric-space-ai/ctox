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
