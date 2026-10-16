# CI per line: V2 checks on `release/v2`, v4 counterparts everywhere else (E37)

Decision E37: no check is switched off. Every workflow picks the line with
`${{ github.base_ref || github.ref_name }}`. On `release/v2` the V2 steps run unchanged;
on every other line (main, v4/*, PRs against them) the v4 counterpart of the same step runs.
V2 test files stay byte-identical. A v4 counterpart checks the SAME invariant against the v4
source; an invariant without meaning in v4 is listed in the script header with a reason.

Measured baseline (2026-10-16, branch `shell-v4/followup-1`, V2 scripts run unchanged):

| V2 script | Result on this branch | First cause |
|---|---|---|
| `assert-rxdb-only.mjs` | crash ENOENT | `modules/desktop/schema.js` deleted by the port |
| `assert-module-conformance.mjs` | red | every ported module: no `schema.js`, `mount(root, api, …)`; plus db-isolation-inventory stale desktop_apps |
| `assert-command-consumer-inventory.mjs` | red | dispatch_consumers drift (V2 consumers moved to `api.command`) |
| `assert-ctox-crew-map.mjs` | red (timeout) | the ported ctox module imports `ctox:shared/…`; the V2 harness serves no such scheme, `__ctoxCrewReady` never set |

Inventory per script: see the sections below (written as each counterpart lands).

## 1. `assert-rxdb-only.mjs` → `assert-rxdb-only.v4.mjs`

Copy of the V2 script with three documented differences (header of the v4 file).

| # | V2 invariant | v4 |
|---|---|---|
| R1 | shell build key: `APP_BUILD` = `index.html` `app.js?v=`, app.css/base.css links, stylesheet fallback | checked unchanged (app.js is still the shell entry) |
| R2 | relative static imports resolve and named imports are exported | checked; roots + `kernel/`, `v4/frame`, `v4/shell`, `v4/source`; resolver follows `export * from`, `export const {a} =`, `export const A=1,B=2` (V2 resolver false-positives on `shared/permissions.js`, `research/tests/v4-fixture.mjs`, `kernel/framing.mjs`) |
| R3 | advanced status interface (app.js/sync.js/db.js markers) | checked unchanged |
| R4 | file chunk integrity | checked against v4 sources: `modules/desktop/collections.schema.json` per collection, `shared/file-integrity.js`, `universal-importer.js`, `rxdb_peer.rs` unchanged; viewer only `api.files.get`, explorer only `api.files.put`, no DataURL, no chunk access; kernel `fetchBlob` sha256 = ref, `files.put` ref = sha256 |
| R4' | viewer/explorer import `shared/file-integrity.js?v=`; explorer chunks + hashes itself | NO_V4: apps load no shell path and no `?v=` (§10.2a Nr. 4, isolation R1); chunking/hashing is kernel transport — hash invariant checked at the kernel |
| R5 | active Notes module not on legacy Notesnook build | checked unchanged |
| R6 | sync warmup does not block boot, catalog seed policy | checked unchanged |
| R7 | connection smoke requires advanced-status boot budget | checked unchanged |
| R8 | login does not default to admin, pairing order | checked unchanged |
| R9 | CTOX Sync Engine branding (db.js, manifest, docs) | checked unchanged |
| R10 | server.rs HTTP data APIs gated, control-plane allowlist has no data path | checked unchanged |
| R11 | subscription auth starts through business_commands | checked unchanged |
| R12 | 11 forbidden HTTP/fallback patterns + no upstream `rxdb` import | checked on all V2 roots + `kernel/**`, `v4/**` (tests/lab/fixtures/testkit of the added roots skipped) |

Total 13 invariant groups: 12 checked in v4 (one with v4 sources), 1 partial NO_V4 (R4').
Counter-checks (mirror `/Volumes/tmp/civ4-mirror`, each mutation → exit 1): K-R1 schema field renamed;
K-R2 `FRAME_BYTES` export renamed; K-R3 kernel hash-mismatch reject removed; K-R4 viewer reads
`desktop_file_chunks`; K-R5 explorer `readAsDataURL`; K-R6 `/api/business-os/records` in `v4/shell/app-host.js`;
K-R7 `import … from "rxdb"` in kernel; K-R8 `APP_BUILD` removed from app.js.
