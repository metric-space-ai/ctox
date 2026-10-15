# Sync v4 real-daemon incident scenarios

Only new synthetic tenants. Never point this runner at an installed/customer root.
The launcher creates a fresh lane artifact directory and a private network namespace,
starts a checksum-verified CTOX binary, local signaling and coturn, and retires its
own process groups. No record HTTP bridge, no mocked query/replication/readiness.

## Build and run

From the assigned clone, use the canonical GPU lane. Build a supplied immutable
revision (the build helper makes a detached worktree and retains a SHA receipt):

```sh
~/.codex/bin/gpu-build-run.sh --owner 01a087a0-169a \
  --task v4-e2e-harness --target-key v4-v4-e2e-harness --host auto --jobs 2 \
  --src "$PWD" -- bash -lc \
  'node scripts/sync-v4/e2e-build.mjs --revision 4794af03d --output "$BUILD_LANE_BIN/../artifacts/<new-build>"'
```

On a host with namespace permission, Playwright/Chromium and coturn, run:

```sh
node scripts/sync-v4/e2e-run.mjs --build /absolute/build.json \
  --profile small --clients 1 --rtt 150 --loss 1 \
  --modules-root /absolute/node_modules --turn-bin /absolute/turnserver \
  --turn-lib /absolute/library/directory
```

The command itself must be inside `gpu-build-run.sh`. gpu4 supports the required
`sudo -n unshare --net` in the accepted transport bench; gpu3 may build the binary
but cannot run this namespace helper without a specific authorized setup. Copy
only this producer's binary + matching source assets and rewrite the manifest
paths with `e2e-export.mjs`/`e2e-import.mjs`; retain original SHA and custody.
Never borrow another daemon or bypass namespace isolation to get a green run.
`--seconds 900` bounds the full scenario run, at most 2100. `--scenario K02`,
`K08` or `K04-ATTACH` selects one incident. `--clients 1..3` opens real independent
browser storage contexts. The selected local ICE candidate MUST be `relay`.
Optional `--warm-rtt 20` establishes the real locally-held-data precondition at
20ms before restoring the requested `--rtt` for the measured scenario. This is
not a cold-sync performance result at the measurement RTT. Both changes retain
kernel qdisc evidence; an ICE consent RTT sampled during warm-up may be older.
`--seed-after-start yes` prepares empty fixture tables/schemas first, starts the
real daemon, then populates the same deterministic dataset before opening the
browser. Default is population before daemon startup. The result records the
chosen lifecycle so historical-store boot and population of a running instance
cannot be confused.
Each run freezes its own runner files and records their hashes before starting;
later source transfers cannot change a running scenario. Namespace cleanup also
retires detached descendants if the coordinator crashes.

## What is measured

- **K02**: a complete list is present in actual IndexedDB, then same-context
  reload. Wall time includes navigation, DB open, app query and paint. Complete
  visible list ≤1000ms, no missing/duplicate IDs. Renderer is a small fixture
  consuming the real app DB; it is not Outbound UI acceptance.
- **K08**: hold local rows, release the warm-up lease, first Shell `openModule`
  of a genuine runtime-installed test app. Measure its real `ctx.db` query and
  paint and actual DOM actionability without an input click. A mounted list
  behind the Shell loading cover is not visible acceptance. No empty first render, complete ≤1000ms. Does not claim the separate
  multi-tab K19 invariant; that needs its own scenario.
- **K04-ATTACH**: after a confirmed local checkpoint, inject the exact old ATTACH
  writer's timestamp pattern into the owned fixture table. Observe native truth
  read-only and the local browser without a demand query. After each injection,
  invoke the ordinary production checkpointed replication pull and record its
  actual native replies. This separates a cursor omission from a missing live
  notification. A forward timestamp
  write to the SAME row is a mandatory positive control. Missing control yields
  **inconclusive**, not proof of the checkpoint bug. SQL reproduces the writer
  mechanism; it does not claim a call of the private Rust function.

A measured red threshold is different from `not_run` setup and `inconclusive`
control. Old-main success is recorded honestly; the catalog's named pre-fix
revision must be used before claiming a fixed regression.

`small` is a quick wiring profile. `thesen-like` holds ~1.2GB of deterministic
synthetic envelopes shaped after the measured volume groups (including 231k
history tombstones). No customer records or executable command rows are copied.
Manifest contains content SHA and per-collection alive/deleted/byte totals. The
fixture replicates leads. Other synthetic tables are registered demand-only in a
separate unopened volume module, providing store volume without business automation. This is not full app semantics.

Each scenario is an ES module with `id`, `steps`, `criterion`, `run(h)`. The
harness supplies browser/reload, local truth, offline WebRTC packet loss and
owned daemon restart. `changeRelease(receiptPath)` checks the new binary and
switches only the owned source links/binary with the same state root.
`schemaChange(cliArgs)` invokes the revision's actual migration CLI against that
root; a scenario must then assert generation and row contents. This primitive
is not proof of a schema migration. The V4 registry migration API still needs its
scenario adapter once it is connected in integration; no fake supplies success.
Scenario files are discovered automatically; the first three are the explicitly
ordered controller slices, not a claim of complete K02/K08/K04 catalog coverage.

`result.json`, screenshots, native logs, controller/group cleanup and `result.md`
are host-private. Copy only results to the controller's private docs directory;
append the table line to private `docs/e2e-ergebnisse.md`. Reports never belong
in public repo docs. No production deployment or merge is performed here.

## Katalog-Gate (#810, E2E-K)

`tests/sync-v4/k01`, `k07`, `k11`, `k15`, `k02`, `k24` contain the scenario
contracts in that execution order. Run the controller entrypoint:

```sh
scripts/sync-v4/gate-regression.sh --build /absolute/verified/build.json \
  --profile thesen-like --rtt 150 --warm-rtt 20 --loss 1 --seconds 2100 \
  --modules-root /absolute/node_modules --turn-bin /absolute/turnserver \
  --turn-lib /absolute/library/directory
```

The entrypoint invokes the canonical GPU lane itself, with two workers and the
assigned target key. gpu4 is the default because the accepted namespace setup
requires its sudo permission; `E2E_GPU_HOST` explicitly selects another capable
host. `E2E_SOURCE_STAGE` may point to the output of `e2e-stage.mjs`: only owned
harness files are transferred, each with its source SHA256 and commit custody.
Daemon binary and static assets have a separate verified producer manifest.
Each K scenario gets a fresh private tenant and frozen harness snapshot; prior
writes, failures and fixtures do not contaminate the next scenario.

The gate prints `K-no. | red/green | evidence`, writes `gate.json` and fails if
no scenario actually ran. A measured slice, missing adapter, missing mutation
witness, or setup failure is never catalog acceptance. In this draft the
coverage gaps are explicit: no scenario yet claims full catalog acceptance.
`measuredPass` and `acceptanceComplete` are separate fields intentionally.

- K01 executes real stale browser writes and a fresh-write positive control;
  without successful control the result is inconclusive. Desktop pins, rejected
  operation archival and the third-client startup audit are still outstanding.
- K07 records browser command response percentiles under 20 MiB collection
  writes and repeated full DB queries. Optional native timing probes are kept
  separately from round-trip time. Raw DataChannel observations count text
  query RPCs and unclassified binary sends; absence cannot be inferred from a
  missing decoder. Real file-CAS upload and app/CPU matrix remain outstanding.
- K11 accepts `--upgrade-build /absolute/build.json` and `--schema-cli file.json`
  (`compatible`, `incompatible`: real revision CLI argv arrays; `database`: path
  relative to the isolated tenant root). The installer now runs the real
  `update apply --version` against a private loopback release-package fixture,
  using two checksum-verified binaries, a private managed prefix and the
  installer's own backup/maintenance/link-switch lifecycle. A nested private
  PID/mount/network namespace masks `/run` before dropping to the fixture UID;
  no host service bus or host PID can be reached. There is no restart fallback.
  The migration CLI must change the actual `_collections` generation registry;
  read-only proofs retain schema, row-ID and data hashes. Missing native
  migration bindings remain `not_run`; code or a binary switch is not proof.
- K15 runs eight continuously committing SQLite connections plus real browser
  producers, native read commands and queue CLI. `--catalog-duration full`
  selects thirty minutes, three clients via `--clients 3`, midpoint daemon
  restart. Producer startup and terminal receipts must prove the load existed;
  native harness-event/ack producers and commit/queue-age instrumentation remain.
- K02's `--catalog-duration full` selects two ten-minute variants: native writes,
  300 detail reads, five reloads and a 50 MiB quota. Wire budget is currently a
  conservative changed-byte upper bound, not exact per-checkpoint accounting.
  Actual Outbound UI, quota status/outbox assertions and the named pre-fix red
  witness remain outstanding. `--prime yes` records one explicit cold bootstrap
  before the fresh measured context; it is not a hidden retry loop.
- K24 uses the delivered Shell tool, exact `run.mjs` and app v1/v2 bytes pinned
  per file in `tests/sync-v4/k24/ORIGIN.json` (upstream `1ab7ceb03`). The adapter
  `k24/execute.mjs` retains every PASS/OFFEN/FAIL criterion verbatim. Bind uses
  MemoryCoreTransport and cannot establish native acceptance. Its private
  static server serves the additional `/rxdb/src/` module dependency of #823;
  before/after hashes of this fixture-only adaptation are recorded. No data
  frame or command is intercepted. Native URL mode permits only an explicitly
  marked isolated loopback instance; no WELSCH tunnel is contacted.

## E2E-K3 revision selection and comparison

```sh
scripts/sync-v4/gate-regression.sh --target v3 --build /absolute/old/build.json
scripts/sync-v4/gate-regression.sh --target v4=v4/cutover
scripts/sync-v4/gate-regression.sh --target v4=<immutable-sha> \
  --build /absolute/verified/build.json --kernel-revision <sha-of-823>
```

V3 reference values are frozen in `k-support/baseline-v3.json`; reruns never
rewrite them. The gate writes `comparison.json` and `comparison.md` alongside
`gate.json`: V3 value, V4 value, target, red/green/not_run and evidence. A missing
revision/build/browser/native adapter yields exit 2 and `not_run`, never red as
if the product had been measured, and never a green empty gate. V4 default
build host is auto; V3/Chromium uses gpu4 because its namespace permission is
required. A supplied producer is rehashed and must match the requested SHA.

The V4 gate builds/verifies the requested daemon and executes an isolated native
bootstrap/Chromium/forced-TURN preflight. It freezes a separate immutable #823
kernel archive and records its SHA; `--kernel-archive` plus
`--kernel-archive-sha256` can supply the same tracked assets to a sparse lane
stage without a Git object database. Credentials stay in the browser; evidence
contains only endpoint locality, identities, phases and connection statistics.
No V3 app collection/query/pull is substituted for a V4 scenario.
The probe document is served by the actual daemon, not `route.fulfill`:
Chromium classifies a synthetic document differently for local-network access.
`k-support/v4-document-mutation.mjs` restores that former document path and
requires `ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS`/no RTC connection, followed
by a real served document with a selected TURN relay. This is a transport
bring-up invariant, not a V4 application acceptance claim.


**Open:** native host-owned hello/device enrollment, canonical scopes and package
verification still need to be connected to the browser adapter. Preflight alone
does not satisfy any K scenario: missing authority is `not_run` with the actual
phase and logs, never a fabricated signature or MemoryCoreTransport success.
T8/Z9 and full K11 migration remain open.

K24 keeps the delivered upstream tool byte-identical. The platform adapter brings
the actual upload page to the foreground before selecting its directory, and
observes the Shell's real completed upload if its FileList has already reset.
`k24/chooser-mutation.mjs` removes that foreground step in a private runner copy:
the real upload must stall; restoration must complete. Bind criteria remain
PASS/OFFEN/FAIL exactly as upstream reports and cannot count as native success.

`k11/installer-smoke.mjs <old receipt> <new receipt> <new artifact prefix>` runs
the actual supported installer in private PID/mount/network namespaces, verifies
its active binary/backup and compares synthetic row hashes read-only before and
after. It explicitly records `migrationExecuted:false` and never claims K11
acceptance. The real compatible/incompatible publication path is still required.
`prepare-install-source.mjs` can supplement a minimal portable source snapshot
with an explicitly SHA-pinned tracked archive from the same revision; the
original producer and portable package remain untouched.

The controller owns one admission attempt, bounded to 40 minutes; no successor
polling chain is created. Expiry only retires its own verified queued group;
admitted jobs are not treated as queued and are not cancelled by this timer.

`--lead-profile demand-only --scenario K08` runs the lease/window slice with an
actual demand-only collection. It is not the historical Outbound startup code
from before `1773924d1`; a green result does not reproduce or close that incident.

`tests/sync-v4/k-support/cancellation.mjs` cancels a real daemon/browser run, verifies bounded
namespace/group retirement and retains the receipt. Signal escalation is
bounded to five seconds. Oracle mutation copies prove missing rows, zero
scenarios and incomplete coverage each turn a guard red, then restored green.
No mutation changes product code or a running daemon.

Native revision builds use a per-revision subdirectory below the admitted target:
absolute CMake source paths from one detached worktree cannot poison another.
Existing source, producer artifacts and earlier caches remain intact.
