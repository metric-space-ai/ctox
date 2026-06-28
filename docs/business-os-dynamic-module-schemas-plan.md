# Business OS Dynamic Module Schemas Plan

Last updated: 2026-06-11
Owner: CTOX Business OS
Status: Done

## Goal

Business OS modules that only need local-first data, RxDB/WebRTC sync, and CTOX
chat/task submission through `business_commands` must be installable without
editing Rust code, regenerating compiled schema contracts, or rebuilding CTOX.

Rust remains required only for native capabilities:

- new durable CTOX command handlers
- host filesystem or runtime access beyond existing generic file APIs
- native projections from CTOX core state
- hard domain transactions and invariants
- specialized demand loaders, stream sources, or performance-critical indexes

The WebRTC-only data boundary does not change. Module data, commands, files,
manifests, and runtime status continue to replicate only through CTOX DB.

## Problem Statement

The browser shell can already load module schemas dynamically from
`modules/<id>/schema.js` and call `db.addCollections()`. The native peer cannot:
`src/core/business_os/rxdb_peer.rs` registers collections from the compiled
`business_os_schema_contract.json`, which is included into the Rust binary.

That means a module with new collections currently forces this chain:

1. add or change module schema
2. regenerate `business_os_schema_contract.json`
3. update `business_os_schema_hashes.json`
4. rebuild and redeploy CTOX

That is the design error this plan fixes.

## Target Architecture

Core collections stay compiled and Rust-owned. Module collections move to a
runtime-loaded, declarative schema contract.

```text
Business OS module
  module.json
  collections.schema.json
  schema.js (optional compatibility/generator facade)
  index.html
  index.js
  index.css

Browser shell
  loads collections.schema.json and declarative migration_strategies
  does not fall back to schema.js for schema registration
  registers collections in ctox-rxdb-js

Native peer
  loads core compiled schema contract
  scans modules/*/collections.schema.json
  scans installed-modules/*/collections.schema.json
  validates and registers runtime module collections
  joins the same WebRTC replication room
```

Schema hash policy:

- Core collections use the retained Business OS schema hash registry.
- Runtime module collections use canonical JSON schema hashing.
- Any collection-name collision with different schema content is a hard error.
- Modules must not override required/core collections.

## Progress Summary

| Phase | Status | Summary |
| --- | --- | --- |
| P0 | Done | Problem confirmed: browser is dynamic, native peer is build-time coupled. |
| P1 | Done | Long-form docs document the runtime schema model, native boundary, and WebRTC-only data plane. |
| P2 | Done | Native fixture is narrowed to native-owned modules; runtime module schemas are merged separately and projections use the merged contract. |
| P3 | Done | `collections.schema.json` format and generator exist; CI checks generated JSON freshness. |
| P4 | Done | Native peer scans module JSON at runtime and focused tests cover runtime-only collection registration and repair. |
| P5 | Done | Browser shell loads `collections.schema.json` directly and requires JSON for schema registration. |
| P6 | Done | App Creator and blank installed-module scaffolds emit JSON schemas; generated apps use RxDB CRUD and `business_commands`. |
| P7 | Done | Packaged modules declare schemas and browser migrations in JSON; `schema.js` is no longer a runtime fallback. |
| P8 | Done | CI guards, focused runtime tests, and fallback ownership/removal criteria are in place. |

Status vocabulary: `Not started`, `In progress`, `Blocked`, `Done`.

## Phases

### P0 - Forensic Confirmation

Status: Done

Findings:

- `src/apps/business-os/app.js::registerModuleSchemas()` dynamically imports
  module `schema.js` and registers collections in the browser.
- `src/core/business_os/rxdb_peer.rs::collection_creators()` uses
  `business_os_schema_contract.json` through `include_str!`.
- `business_os_schema_contract.json` is generated from a fixed module list in
  `src/core/rxdb/tools/build_business_os_schema_contract.mjs`.
- Installed modules can appear in the module catalog, but new native collections
  are not available unless the Rust-side compiled schema contract knows them.

Exit criteria:

- [x] Browser and native schema registration paths identified.
- [x] Build-time coupling documented.
- [x] Required/core collection boundary identified.

### P1 - Architecture Decision And Documentation

Status: Done

Work:

- Update `docs/ctox-rxdb.md` with the runtime module schema model.
- Update `src/apps/business-os/README.md` module contract.
- Update `src/apps/business-os/ARCHITECTURE.md` target architecture.
- Add explicit native-capability boundary: declarative apps need no Rust;
  native capabilities still do.

Exit criteria:

- [x] Plan and generator comments say module data schemas are runtime metadata,
  not Rust build inputs.
- [x] Docs preserve the WebRTC-only data boundary.
- [x] Plan lists what still requires backend/native code.

### P2 - Contract Split

Status: Done

Work:

- Rename or introduce a core-only compiled contract for required/native
  collections.
- Keep required collections pinned:
  `business_module_catalog`, `ctox_runtime_settings`, `business_commands`,
  `ctox_queue_tasks`, `desktop_files`, `desktop_file_chunks`, and other
  native-owned projections.
- Define runtime module schema aggregation rules:
  first matching identical schema wins; divergent duplicate schemas fail.
- Define collision policy against core collections.

Exit criteria:

- [x] Core/native collections remain available before module scanning.
- [x] Module collections are represented separately from the compiled native
  schema fixture at runtime.
- [x] Duplicate and collision behavior is specified and guarded for module JSON.
- [x] Retained native fixture should be narrowed further as legacy native
  projections are retired.

### P3 - Module Schema JSON Format And Generator

Status: Done

Work:

- Introduce `collections.schema.json` beside each `module.json`.
- Define a no-code JSON shape:

  ```json
  {
    "schema_format": "ctox-business-os-module-collections-v1",
    "collections": {
      "example_records": {
        "version": 0,
        "primaryKey": "id",
        "type": "object",
        "properties": {
          "id": { "type": "string", "maxLength": 180 },
          "title": { "type": "string" },
          "updated_at_ms": { "type": "number" }
        },
        "required": ["id", "title", "updated_at_ms"],
        "indexes": ["updated_at_ms"],
        "additionalProperties": true
      }
    }
  }
  ```

- Add a generator that imports current `schema.js` files and writes
  `collections.schema.json`.
- Keep `schema.js` as a compatibility facade during migration.

Exit criteria:

- [x] Generator produces stable JSON for all migratable modules.
- [x] Generated JSON contains no functions or executable code.
- [x] Existing module schema parity is preserved by conformance guard.

### P4 - Native Runtime Schema Loader

Status: Done

Work:

- Add a loader under `src/core/business_os/` that scans:
  `src/apps/business-os/modules/*/collections.schema.json` and
  `runtime/business-os/installed-modules/*/collections.schema.json`.
- Validate collection names, primary keys, indexes, schema versions, and JSON
  shape before registering.
- Merge core and module collection creators.
- Support runtime module collections in `repair_optional_rxdb_collection_schema_drift`.
- Decide first implementation behavior for newly installed collections:
  peer restart is acceptable for V1; hot reload can follow.

Exit criteria:

- [x] Native peer registers a module-only collection not present in the compiled
  core contract.
- [x] A bad module schema fails with actionable diagnostics.
- [x] Required/core collection failures still abort peer bring-up.
- [x] Runtime module collections are included in schema-drift repair.
- [x] Optional/module collection failures remain isolated by tolerant native
  collection bring-up.

### P5 - Browser Runtime Source Of Truth

Status: Done

Work:

- Update `registerModuleSchemas()` to load `collections.schema.json` first.
- Require `collections.schema.json` for runtime schema registration.
- Keep browser migrations declarative in `collections.schema.json` metadata.
- Keep module sync lazy and collection-priority behavior inside CTOX DB.

Exit criteria:

- [x] Browser and native read the same JSON schema for migrated modules.
- [x] Missing `collections.schema.json` fails loudly instead of silently falling
  back to `schema.js`.
- [x] Unknown/custom module collections use canonical JSON schema hashes.

### P6 - App Creator And App Store

Status: Done

Work:

- Update App Creator generated output:
  `module.json`, `collections.schema.json`, `schema.js`, UI files, locales.
- Update install-template and installed-module paths to copy schema JSON.
- Add UI validation that every declared collection has a schema.
- Make generated apps use `business_commands` for chats/tasks instead of any
  module-specific backend bridge.
- Add a generator guard that keeps emitted modules on JSON schemas, shell DB
  handles, RxDB CRUD, and `business_commands`.

Exit criteria:

- [x] A generated app emits a new `collections.schema.json` without Rust edits.
- [x] The generated app can create/read/update records through RxDB.
- [x] The generated app can submit a CTOX chat/task through `business_commands`.

### P7 - Existing Module Migration

Status: Done

Batch order:

1. `matching` as the blueprint module.
2. `customers`, because it is a representative business-data module.
3. `documents`, `spreadsheets`, `notes`, and `calendar`.
4. `research`, `knowledge`, `outbound`, `shiftflow`, `buchhaltung`, `iot`.
5. Native-near modules: `ctox`, `desktop`, `tickets`, `reports`, `app-store`,
   `creator`, and `browser`.

Migration rule:

- Move pure module data schemas to `collections.schema.json`.
- Keep core/native projection schemas in the core contract.
- Keep `schema.js` as an optional compatibility/generator facade where
  practical.
- Do not introduce HTTP data paths.

Exit criteria:

- [x] Every migrated packaged module passes module conformance.
- [x] Every packaged module collection is either core/native-owned or declared
  in JSON.
- [x] Pure data collections no longer require Rust contract entries.
- [x] Remove `schema.js` fallbacks after migrationStrategies have a declarative
  home or are retired.

### P8 - Guards, Tests, And Legacy Cleanup

Status: Done

New or updated guards:

- `module.json.collections[]` must exist in `collections.schema.json` or be a
  declared core collection.
- Module schemas must not override core collections.
- Duplicate module collection schemas must be byte/canonical equivalent.
- Browser and native canonical schema hashes must match for runtime modules.
- Installed test module with a new collection must replicate over WebRTC without
  a Rust contract entry.
- App Creator output must include JSON schemas, RxDB CRUD through the shell DB
  handle, and CTOX task submission through the shell command bus.

Existing required gates:

- `node src/core/rxdb/tools/build_business_os_module_schema_files.mjs`
- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-declarative-migrations.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- Focused native peer tests for dynamic schema loading.

Exit criteria:

- [x] Runtime module schema guard is in CI.
- [x] Legacy build-time generator is explicitly scoped to native-owned schema
  fixtures, not the general app install path.
- [x] App Creator generated-module guard was replaced by the Business OS app
  validator and skill contract.
- [x] `schema.js` fallback has an owner and removal criteria, or is removed.

## Verification Notes

2026-06-11:

- `node --check` passed for:
  `src/apps/business-os/app.js`,
  `src/apps/business-os/shared/declarative-migrations.js`,
  `src/apps/business-os/modules/matching/ui/businessOsDataSource.js`,
  `src/apps/business-os/scripts/assert-module-conformance.mjs`, and
  `src/apps/business-os/scripts/assert-declarative-migrations.mjs`.
- `node src/core/rxdb/tools/build_business_os_module_schema_files.mjs` passed:
  generated module schema files are current.
- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs` passed.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `node src/apps/business-os/scripts/assert-module-conformance.mjs` passed:
  23 modules.
- `node src/apps/business-os/scripts/assert-declarative-migrations.mjs` passed:
  23 modules.
- Business OS app validation replaced the old generated-module guard.
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed: 39/39.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml --quiet` passed:
  239 + 30 tests.
- `cargo check --bin ctox` passed with existing warnings.
- `cargo test --bin ctox runtime_module_schema --quiet` passed: 3 tests.
- `cargo test --bin ctox business_record_projection --quiet` passed: 6 tests.
- `cargo test --bin ctox projection_upsert --quiet` passed: 4 tests.
- `git diff --check` passed.

## Deployment Double-Check

2026-06-11:

- `ctox upgrade --dev` fetches the configured GitHub release channel's `main`
  branch source archive and builds it as a source-mode release on the instance
  where the command is run. Local uncommitted changes are not included; this
  refactor must be committed and pushed to `main` before that path can deploy
  it elsewhere.
- The command is per-instance. It does not fan out to every CTOX installation
  by itself; each target instance must run `ctox upgrade --dev` or be upgraded
  through an explicit remote provisioning/orchestration path.
- Source-mode upgrades copy the release tree, build the new `ctox` binary,
  switch the managed `current` symlink, refresh wrappers/service units, restart
  `ctox.service`, and restart active Business OS web-shell units so static
  assets are served from the new release.
- Business OS packaged modules and their `collections.schema.json` files are
  part of the source release under `src/apps/business-os`. The native resolver
  and module catalog prefer the release source over stale runtime app roots;
  this is covered by
  `cargo test --bin ctox module_catalog_prefers_release_source_over_stale_runtime_app_root --quiet`.
- Ad-hoc/generated installed apps are still instance-local unless they are
  packaged into the Git source tree or installed separately on that instance.
- If a deployment uses a separate ctox.dev/static WebDeploy shell instead of
  the CTOX instance serving the shell, `ctox upgrade --dev` updates the CTOX
  instance and native peer, not that separate web deployment. The static shell
  host must be deployed through its own pipeline.
- Remote validation on managed instances completed:
  - `example`: `ctox upgrade --dev` completed and activated
    `branch-main-20260611T213221Z`; backup:
    `/home/ubuntu/.local/state/ctox/backups/update-20260611T213225Z`.
    Postcheck: `ctox.service=active`, Business OS ok, WebRTC transport,
    native RxDB peer running, `replicationUp=true`, peer error total `0`,
    module count `13`.
  - `SKF`: `ctox upgrade --dev` completed and activated
    `branch-main-20260611T213221Z`; backup:
    `/home/ubuntu/.local/state/ctox/backups/update-20260611T213225Z`.
    Postcheck: `ctox.service=active`, Business OS ok, WebRTC transport,
    native RxDB peer running, `replicationUp=true`, peer error total `0`,
    module count `12`.
  - The remote validation built GitHub `main` as fetched by the instances. It
    does not include local uncommitted/untracked refactor files until those are
    committed and pushed.

## Implementation Slices

Recommended PR order:

1. Docs and tests that encode the desired behavior.
2. JSON schema generator and conformance guard.
3. Native runtime schema loader with a synthetic installed-module test.
4. Browser JSON-first schema loading.
5. App Creator/App Store output changes.
6. Module migration batches.
7. Legacy cleanup.

## Risks And Mitigations

| Risk | Mitigation |
| --- | --- |
| Schema drift between browser and native | Both sides read `collections.schema.json`; add canonical hash tests. |
| Malformed installed module breaks required sync | Treat module collections as optional; keep required core collection failures fatal. |
| Duplicate collection definitions diverge | Hard-fail divergent duplicates with module names in diagnostics. |
| Existing modules rely on JS-only migration strategies | Store browser migrations as declarative `migration_strategies` in `collections.schema.json`. |
| Runtime reload is complex | Accept peer restart for V1; add hot reload only after correctness is proven. |
| Agents weaken WebRTC guardrails to move faster | Keep existing RxDB-only and no-HTTP guards mandatory. |

## Progress Log

| Date | Update |
| --- | --- |
| 2026-06-11 | P0 completed from code inspection. Plan file created. |
| 2026-06-11 | Added `collections.schema.json` generator and generated JSON contracts for 23 packaged modules. |
| 2026-06-11 | Browser schema registration now loads JSON schemas directly; `schema.js` remains only as an optional compatibility/generator facade. |
| 2026-06-11 | Native peer now merges `modules/*/collections.schema.json` and `installed-modules/*/collections.schema.json` at runtime. |
| 2026-06-11 | Runtime-only module collection registration and schema-drift repair are covered by focused Rust tests. |
| 2026-06-11 | Module conformance guard now requires JSON schemas, JS parity, duplicate-schema parity, mount export, and locales. |
| 2026-06-11 | App Creator and blank installed-module scaffolds now emit `collections.schema.json`. |
| 2026-06-11 | Long-form docs now preserve the WebRTC-only boundary and document the runtime-schema/native-capability split. |
| 2026-06-11 | Generated App Creator modules now use shell RxDB handles for CRUD and dispatch CTOX chat tasks through `business_commands`; CI guards the template. |
| 2026-06-11 | `schema.js` runtime fallback was removed after declarative migration metadata moved into `collections.schema.json`. |
| 2026-06-11 | Native business-record projection now derives its collection list from the merged runtime schema contract for the app root. |
| 2026-06-11 | Native schema/hash fixtures were narrowed to retained native-owned modules; non-native packaged modules now rely on `collections.schema.json`. |
| 2026-06-11 | Browser schema registration and Matching's standalone data source no longer import `schema.js`; declarative migrations now live in `collections.schema.json`. |
| 2026-06-11 | CI now validates declarative migrations alongside module conformance and App Creator generated output. |
| 2026-06-11 | Deployment double-check added: `ctox upgrade --dev` deploys committed GitHub `main` per instance and restarts the Business OS web shell, but does not globally fan out or update separate ctox.dev WebDeploy shells. |
| 2026-06-11 | Remote `ctox upgrade --dev` validation completed on `example` and `SKF`; both activated `branch-main-20260611T213221Z` and passed Business OS/WebRTC/RxDB peer postchecks. |
