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
`--seconds 900` bounds the full scenario run, at most 1800. `--scenario K02`,
`K08` or `K04-ATTACH` selects one incident. `--clients 1..3` opens real independent
browser storage contexts. The selected local ICE candidate MUST be `relay`.
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
  paint. No empty first render, complete ≤1000ms. Does not claim the separate
  multi-tab K19 invariant; that needs its own scenario.
- **K04-ATTACH**: after a confirmed local checkpoint, inject the exact old ATTACH
  writer's timestamp pattern into the owned fixture table. Observe native truth
  read-only and the local browser without a demand query. A forward timestamp
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
fixture only registers/replicates leads; other synthetic tables provide store
volume without triggering business automation. This is not full app semantics.

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
