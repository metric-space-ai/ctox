# CTOX Performance Probe

This directory contains local operator diagnostics for CTOX performance work.
The probe is intentionally outside the daemon hot path: it does not start
`ctox-real`, does not mutate runtime state, and samples idle CPU before running
any optional `ctox status` commands.

Typical idle evidence run:

```sh
python3 src/tools/perf/ctox_perf_probe.py \
  --root /Users/michaelwelsch/Documents/ctox.nosync \
  --cpu-samples 300 \
  --cpu-interval 1 \
  --status-samples 20 \
  --pretty > runtime/build/ctox-perf-probe.json
```

Release-gate idle run with hard budgets:

```sh
python3 src/tools/perf/ctox_perf_probe.py \
  --root /Users/michaelwelsch/Documents/ctox.nosync \
  --assert-idle \
  --cpu-samples 600 \
  --cpu-interval 1 \
  --status-samples 20 \
  --max-heartbeat-delta 'rxdb_sqlite.changed_documents_since_calls=0' \
  --pretty > runtime/build/ctox-idle-gate.json
```

Installed daemon Gate A/B/C workflow:

```sh
python3 src/tools/perf/ctox_installed_idle_gate.py \
  --root /Users/michaelwelsch/Documents/ctox.nosync \
  --ctox-command ctox \
  --release
```

The installed workflow runs `ctox upgrade --dev`, resolves the installed
`ctox-real` PID, writes `release-identity.json` with the source git commit,
branch and status, install manifest, `current` symlink target,
installed/current `ctox-real` hashes, `ctox version`, process command
path/hash and process start time, stores artifacts under
`runtime/perf/installed-idle-*`, runs Gate A passive idle without `ctox status`,
runs Gate B with status polling as separate load while sampling
CPU/DB/heartbeat deltas, and then runs Gate C
`ctox process-mining spawn-liveness`. A real run fails before Gate A when the
sampled PID cannot be tied to the newly installed release. Release-root layouts
without a shared `bin/ctox-real` launcher are valid when the sampled process
hash matches the `current` release binary hash. For a local command/artefact
dry check without upgrading or sampling the daemon:

```sh
python3 src/tools/perf/ctox_installed_idle_gate.py \
  --dry-run \
  --skip-upgrade \
  --skip-gate-c \
  --pid 12345 \
  --gate-a-seconds 1 \
  --gate-b-seconds 1
```

`--assert-idle` exits with status 1 when a budget is exceeded. Default budgets
are average CPU <= 2 percent, p95 CPU <= 5 percent, status p95 <= 100 ms,
total SQLite file growth <= 0 bytes during the CPU sampling window, no positive
growth for any discovered SQLite main/WAL/SHM/journal file, no extra
`ctox-real` process candidates, no positive page/freelist/dbstat/RxDB
row/payload/tombstone deltas from database metric snapshots, no active native
loop row work, no native loop errors, and no hot native SQLite query, count,
write, stream, statement elapsed, writer-lock wait/held, or writer-fallback
deltas. Normal SQLite query fallback calls, fallback rows visited,
indexed-candidate fallback calls, and too-broad fallback aborts are included in
those heartbeat budgets, including the per-collection, per-operator, and
collection/operator attribution maps. Post-file-access idle also fails on
external SQLite poll reads (`external_poll_data_version_reads`,
`external_poll_changed_table_reads`) and on `changed_documents_since` result
drain deltas, so chunk/change-stream backlog cannot masquerade as idle. It also
requires the native peer
heartbeat file to be
present, fresh,
from the sampled PID, `running=true`, `replicationUp=true`, and to expose the
expected performance counter schemas. It also fails on new
`ticket_sync_runs` or `communication_sync_runs` rows during the CPU sampling
window, and on any
daemon-side service status request delta recorded in
`runtime/service-performance.status.json`. That service-performance artifact
includes process PID/boot identity, IPC status counters, and HTTP status
counters; passive idle fails on missing artifacts, wrong PID, boot-ID changes,
counter resets, or status-request growth. Additional
`--max-heartbeat-delta GLOB=VALUE` and
`--max-sync-run-delta GLOB=VALUE` flags can pin exact counters for a scenario.
Gate B in the installed workflow intentionally creates status load, so it skips
the service-performance artifact delta while keeping status latency and
status-load assertions separate, and it requires the status-poll load to show
up in the daemon `status_requests.total_requests` counter.

The file-growth window starts after the probe's read-only SQLite pre-sampling
diagnostics. This avoids counting a SQLite `-shm` file materialized by the
probe setup as daemon idle growth; any main/WAL/SHM/journal growth during the
actual CPU sampling window still fails the default idle budget.

For DB-size diagnostics only:

```sh
python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --pretty
```

For a CPU-only sample that does not inspect SQLite files:

```sh
python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 60 --cpu-interval 1 --pretty
```

The CPU sampler targets the running process directly through `ps`/`pgrep` and
does not call `ctox status`. It records every `pgrep -x ctox-real` candidate,
aggregates candidate CPU, samples the selected process group plus descendants,
and treats additional `ctox-real` candidates as an idle-gate failure so stale
or child work cannot burn CPU outside the selected PID budget. Status latency
is sampled afterwards and reported separately so status polling cannot be
confused with daemon idle CPU. The probe also reads
`runtime/business-os-rxdb-peer.status.json` before and after CPU sampling and
reports numeric deltas for native peer loop counters and SQLite runtime
counters when the installed daemon exposes them. Under `--assert-idle`,
`--max-heartbeat-age-ms` defaults to 30 seconds and stale, missing,
schema-mismatched, wrong-PID, or `replicationUp=false` heartbeat snapshots fail
the run. It also reads
`runtime/service-performance.status.json`; under `--assert-idle --skip-status`
the default budget requires zero service status request deltas, which catches
external `ctox status` pollers during a passive idle sample. Runtime DB
discovery includes the known CTOX SQLite files plus `runtime/*.sqlite3` and
`runtime/*.db`, and file growth is reported per main/WAL/SHM/journal component.
When DB diagnostics are enabled, the probe also snapshots page/freelist,
`dbstat`, RxDB collection row/data/tombstone, and sampled desktop chunk metrics
before and after CPU sampling; `--max-db-metric-delta GLOB=VALUE` can pin those
deltas explicitly.
