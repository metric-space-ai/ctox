# ctox-rxdb-js (CTOX DB browser runtime) — agent guardrails

Read `docs/ctox-rxdb.md` before changing anything here. This runtime is the
WebRTC-ONLY data plane between Business OS and the CTOX daemon. Every rule
below exists because an agent broke it in good faith and shipped a regression.

## Hard rules

1. **No HTTP fallback — ever.** Business OS collections, commands, files,
   manifests and runtime status replicate ONLY over RxDB/WebRTC (root
   `README.md`, "Data Boundary"). If sync looks broken, fix it inside the
   WebRTC stack. An HTTP bridge is a regression, not a feature.
2. **Never patch `dist/ctox-rxdb-js.mjs` directly.** It is built from
   `src/index.mjs`. Edit src, then rebuild with the pinned command:
   ```
   npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs \
     --bundle --format=esm \
     --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs \
     "--banner:js=// CTOX DB app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."
   ```
   and bump the `?v=` cache-buster in `shared/db.js` and `shared/sync.js`
   (both identical — a mismatch loads a second bundle copy and duplicates
   peers). App modules import the bundle through the shell facade, not
   directly, so they carry no buster of their own.
3. **No npm / bare / `node:` imports** in `src/*.mjs`. The runtime is
   package-manager-free; only relative imports are allowed.
4. **Never hand-edit `*-contract.generated.mjs`** (or the Rust twins). Wire
   contracts are generated from `src/core/rxdb/tests/fixtures/*.json` via
   `src/core/rxdb/tools/build_webrtc_*_contract.mjs`.
5. **`schema.mjs` hash registry mirrors the Rust fixture**
   (`src/core/business_os/business_os_schema_hashes.json`). Regenerate, don't
   guess hashes.
6. **Run and keep green:** `node src/apps/business-os/rxdb/tests/run-all.mjs`.
   A red test is a finding. Never delete or weaken a test to make the suite
   pass; if a contract changed on purpose, update the pin in the same commit.

## What protects these rules

`tests/data-plane-guard-smoke.mjs` (no HTTP/npm/env, ratcheted),
`tests/contract-drift-smoke.mjs` (generated files match fixtures),
`tests/bundle-reproducible-smoke.mjs` (dist rebuilds byte-identical from src),
`tests/schema-hash-registry-smoke.mjs`, and the guards under
`src/apps/business-os/scripts/` and `src/core/rxdb/tools/`. If one of them
blocks you, the guard is right and your change is the problem.
