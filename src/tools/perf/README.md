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
separately so status polling cannot be confused with daemon idle CPU.
