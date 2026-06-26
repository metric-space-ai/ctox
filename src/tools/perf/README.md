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

`--assert-idle` exits with status 1 when a budget is exceeded. Default budgets
are average CPU <= 2 percent, p95 CPU <= 5 percent, status p95 <= 100 ms,
total SQLite file growth <= 0 bytes during the CPU sampling window, no active
native loop row work, no native loop errors, and no hot native SQLite query,
count, write, stream, or writer-fallback deltas. Additional
`--max-heartbeat-delta GLOB=VALUE` flags can pin exact native heartbeat
counters for a scenario.

For DB-size diagnostics only:

```sh
python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --pretty
```

For a CPU-only sample that does not inspect SQLite files:

```sh
python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 60 --cpu-interval 1 --pretty
```

The CPU sampler targets the running process directly through `ps`/`pgrep` and
does not call `ctox status`. Status latency is sampled afterwards and reported
separately so status polling cannot be confused with daemon idle CPU. The probe
also reads `runtime/business-os-rxdb-peer.status.json` before and after CPU
sampling and reports numeric deltas for native peer loop counters and SQLite
runtime counters when the installed daemon exposes them.
