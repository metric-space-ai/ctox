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

## 2. `assert-module-conformance.mjs` → `assert-module-conformance.v4.mjs`

| # | V2 invariant | v4 |
|---|---|---|
| M1 | schema-covers-collections (module.json collections declared in schema.js) | checked against `collections.schema.json` (§16.13); leftover `schema.js` (ctox, conversations, outbound) still counted |
| M2 | schema-import (schema.js imports in Node) | checked: `collections.schema.json` exists and parses as `{collections}`; leftover `schema.js` must still import |
| M3 | schema-parity (same collection → identical definition) | checked, fingerprints from `collections.schema.json` (+ leftover schema.js) |
| M4 | mount-export | checked unchanged |
| M5 | mount-signature `mount(ctx)` | checked with the v4 contract `mount(root, api, { restored } = {})` (kernel/index.d.ts AppMount, PORTING.md R1-mount); `mount(ctx)` is now red |
| M6 | no-db-raw | checked unchanged |
| M7 | no-indexeddb | checked unchanged |
| M8 | css-no-root-tokens | checked unchanged |
| M9 | css-no-shell-token-redefinition | checked unchanged |
| M10 | css-no-self-referential-token | checked unchanged |
| M11 | css-no-cdn-import | checked unchanged |
| M12 | locales de/en | checked unchanged |
| M13 | db-isolation-inventory (V2 guard, spawned) | V2 guard runs unchanged; only accepted finding: `desktop_apps: stale inventory entry for <id>` when `<id>` moved to `modules/<id>` (desktop-apps app.js gone, module.json + v4 manifest present: browser, creator, explorer, file-viewer). Plus v4 form: `check-isolation.mjs --rule R2` must exit 0 |

13 invariants: 13 checked in v4 (M1/M2/M3/M5/M13 adapted to v4 sources), 0 NO_V4.
Counter-checks (each → exit 1): K-M1 `ctox_ticket_cases` renamed in tickets collections.schema.json;
K-M2 notes collections.schema.json unparseable; K-M3 file-viewer `desktop_files` schema drift vs cv-print-builder;
K-M4 news `mount(ctx)`; K-M5 `ctx.db.raw` in news; K-M6 `:root { --news-x }` in news CSS; K-M7 news `locales/en.json`
removed; K-M8 non-ported stale desktop_app `ghost-app` in the inventory; K-M9 tickets manifest names `news` as owner of
`ctox_crew_members` (R2).
Note: `check-isolation.mjs` without `--rule` is red before and after this change (R1 6, R3 80, R4 149 findings) —
those are v4-only rules without a V2 counterpart, not part of this gate.

## 3. `assert-command-consumer-inventory.mjs` → `assert-command-consumer-inventory.v4.mjs`

| # | V2 invariant | v4 |
|---|---|---|
| C1 | dispatch_consumers: exact inventory of files dispatching commands (`commandBus.dispatch`) | checked as two frozen inventories in the script: (a) V2 bus regex — 13 files (app.js, office bridge, shared/{app-actions,business-chat,react-settings,universal-importer}, v4/shell/workjet/* = v2-compat debt L-42); (b) v4 path `api.command(` — 43 files; every app file needs a v4 manifest |
| C2 | projection_readers: exact inventory of `business_commands` readers | checked: V2 regex ∪ `api.query/get/subscribe/watch/find/findOne('business_commands')` — 20 files; every app reader must declare `business_commands` in its v4 manifest (collections or foreign_collections read) |
| C3 | legacy_projection_waiters = [] | checked unchanged |
| C4 | direct_intent_writers = [] | checked: V2 regex ∪ `api.put/insert/upsert('business_commands')` |

Scan scope = V2 scope plus exclusions for test/tooling locations: `node_modules/`, `*/tests/`, `*/fixtures/`
(e.g. `v4/tools/guards/fixtures/tree/**`, `modules/mail/tests/mail-browser-qa.v4.mjs`), `kernel/testkit/`, `v4/tools/`.
4 invariants: 4 checked in v4, 0 NO_V4.
Counter-checks (each → exit 1): K-C1a `commandBus.dispatch` added to news; K-C1b `api.command` added to
shared/crew-presence.js; K-C1c tickets stops dispatching (missing); K-C2a news reads `business_commands`;
K-C2b importer manifest no longer declares `business_commands`; K-C3 `function waitForCommandProjection(`;
K-C4a `api.put('business_commands')` in news; K-C4b `db.business_commands.insert(` in shared.
Observation (not a gate failure, same as V2 semantics): `modules/credentials/index.js:521` modifies an existing
command document with `api.patch('business_commands', …)` (local redaction, V4-PORT OFFEN-2); declared in its manifest
with ops read+write. V2 never treated modification as an intent write either.

## 4. `assert-ctox-crew-map.mjs` → `assert-ctox-crew-map.v4.mjs`

V2 failure: the ported ctox app imports `ctox:shared/crew-presence.js` etc.; Chromium refuses the unknown scheme
(CORS "Cross origin requests are only supported for protocol schemes …"), the fixture never sets `__ctoxCrewReady`
and every scenario times out after 30 s.

| # | V2 invariant | v4 |
|---|---|---|
| K1 | only the selected task plus running tasks stand on the map | checked unchanged |
| K2 | exactly one creature marked selected | checked unchanged |
| K3 | waiting creature sleeps at `queued`; working creature works at `running` | checked unchanged |
| K4 | fresh durable tool turn → finite `tool` impulse; waiting creature gets none | checked unchanged |
| K5 | gesture settles back into the working pose | checked unchanged |
| K6 | visible working crew is animated; reduced motion stops all motion | checked unchanged |
| K7 | failed creature at `model-failed` with X eyes | checked unchanged |
| K8 | selection reachable by mouse and keyboard | checked unchanged |
| K9 | chat task-id deep link, shared identity colour, reference badge (no creature copy) | checked unchanged (chat is still `shared/business-chat.js`, debt L-32) |
| K10 | one being per member; crew-bar seat shows portrait while body is on the map; presence events | checked unchanged |
| K11 | member with two running tasks stands once with count 2 | checked unchanged |
| K12 | identity colour stable across a clean reload; no console/network errors | checked unchanged |

Differences are loading only: `ctox:shared/` bound to `/src/apps/business-os/shared/` when serving JS/CSS (as
`v4/blob-loader.js` / `v4/frame/lab/frame.test.mjs`), ctox app imported without `?v=`, output dir `ctox-crew-map-v4`.
12 invariants: 12 checked, 0 NO_V4. Run: exit 0, 7 scenarios, ~17 s (screenshots looked at: map, three nodes,
crew bar, X eyes on the failed creature).
Counter-checks (each → exit 1): K-K1 slot no longer gets `is-selected` ("selected creature is not marked");
K-K2 crew-motion ignores the reduced-motion change ("reduced motion must stop all creature motion").

## 5. Workflows: steps per line

Line = `${{ github.base_ref || github.ref_name }}` (PR: target branch; push: branch). `release.yml` is tag-triggered
(`v*`), where `ref_name` is the tag: its new step `Select CI line` sets `release/v2` when the tagged commit is an
ancestor of `origin/release/v2` (incl. commits before the split = V2 code), else `main`.
Combined multi-command steps had their V2 commands moved into a V2-line step and a v4-line step; every other
command in those steps is unchanged and stays where it was.

| Workflow | Trigger change | main / v4 / PRs against them | release/v2 |
|---|---|---|---|
| `ci.yml` | push + pull_request `branches: [main]` → `[main, release/v2]` | `Business OS RxDB-only contract` (rest) → `… (v4 line)`: `--check` + run `assert-rxdb-only.v4.mjs`, `assert-module-conformance.v4.mjs`, `assert-command-consumer-inventory.v4.mjs` | `Business OS RxDB-only contract` (rest) → `… (V2 line)`: `assert-rxdb-only.mjs`, `assert-module-conformance.mjs`, `assert-command-consumer-inventory.mjs` |
| `release.yml` | none (tags `v*`) | `Business OS production contract checks` (rest) → `Select CI line` → `… (v4 line)`: `assert-rxdb-only.v4.mjs`, `assert-module-conformance.v4.mjs` | same rest → `Select CI line` → `… (V2 line)`: `assert-rxdb-only.mjs`, `assert-module-conformance.mjs` |
| `iot-soak.yml` | push `branches: [main]` → `[main, release/v2]`; paths + `assert-rxdb-only.v4.mjs` (pull_request and push) | `Run IoT module + RxDB-only contract checks` (iot + app-store tests) → `RxDB-only contract, no HTTP bridge (v4 line)`: `assert-rxdb-only.v4.mjs` | same → `… (V2 line)`: `assert-rxdb-only.mjs` |
| `crew-ui-acceptance.yml` | none (pull_request without branch filter, workflow_dispatch) | `Module and chat behavior` (ctox test.js, tickets) → `Chat behavior (v4 line)`: same three files with `--test-skip-pattern "Web Research oeffnet den Chat"` → `Research chat handoff (v4 line)`: `node src/apps/business-os/scripts/research-chat-handoff.v4.mjs` (does not exist yet → red) … `Shell and data boundary contracts` (rest) → `RxDB-only and crew map contracts (v4 line)`: `assert-rxdb-only.v4.mjs`, `assert-ctox-crew-map.v4.mjs` | `Module and chat behavior` (rest) → `Chat behavior (V2 line)`: `node --test shared/business-chat.test.mjs shared/react-settings.test.mjs shared/shell-crew-health.test.mjs` … `Shell and data boundary contracts` (rest) → `… (V2 line)`: `assert-rxdb-only.mjs`, `assert-ctox-crew-map.mjs` |
| `mail-ui-acceptance.yml` | none (pull_request without branch filter, workflow_dispatch) | `Mail browser behavior (v4 line)`: `mail-browser-qa.v4.mjs` with `CTOX_APP_API_STUB` = in-repo stub `modules/ctox/tests/fixtures/app-api-stub` (pin 68b83a3), evidence `mail-qa-v4/` uploaded | `Mail browser behavior`: `mail-browser-qa.mjs` (unchanged step, now `if: release/v2`) |
| `business-os-shell-release.yml` | none | `Verify main release source`: 0.2.x must be an ancestor of `origin/main` | 0.1.x must be an ancestor of `origin/release/v2`; unknown series fails |

Pre-existing reds, not touched (red before the port as well): `check-office-skill-gating`, `assert-ctox-data-state`,
`assert-context-menu-lifecycle`.

## 6. Verification (2026-10-16, branch `shell-v4/followup-1`)

- All four `.v4.mjs` exit 0 from the repo root.
- YAML: `/usr/bin/python3 -c "import yaml,sys; …" .github/workflows/*.yml` → 27 files OK (Homebrew python3 has no PyYAML).
- `node /Volumes/tmp/rel-runs/wf-gates.cjs .github/workflows/business-os-shell-release.yml "$PWD" 0.2.0-beta.1
  /Volumes/tmp/rel-runs/civ4-02` → failed=0. The runner starts at `Select release gate series`, so it does not
  evaluate the ancestry check in `Verify main release source`. That check was tested separately in a temp repo:
  0.1.x on release/v2 → 0, 0.1.x on main → 1, 0.2.x on main → 0, 0.2.x on release/v2 → 1, 0.3.0 → 1.
  `release.yml` `Select CI line` was also tested: a tag on the release/v2 tip gives release/v2, a tag on the main tip
  gives main, a tag before the split gives release/v2.
- `mail-browser-qa.v4.mjs` with the in-repo stub (`CTOX_APP_API_STUB=modules/ctox/tests/fixtures/app-api-stub`)
  → exit 0.
- `node --test --test-skip-pattern "Web Research oeffnet den Chat" shared/business-chat.test.mjs
  shared/react-settings.test.mjs shared/shell-crew-health.test.mjs` → 142/142 pass. Without the skip, that
  test fails, as expected.
