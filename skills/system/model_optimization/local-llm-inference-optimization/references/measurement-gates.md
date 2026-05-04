# Measurement And Gates

Use this reference when designing benchmark suites, promotion gates, or
cache/memory forensics for an inference optimization project.

## Benchmark Discipline

Run performance comparisons serially. Do not run GPU/CPU benchmarks in
parallel with subagents or unrelated builds.

Minimum evidence:

```text
smoke:
  one short run, correctness only

candidate:
  >= 3 iterations, >= 1 warmup, at least two token/context sizes

promotion:
  alternating accepted/candidate order
  median and p95
  realistic prefill contexts
  realistic decode output lengths
  reference comparison
  hardware state recorded
```

Record:

```text
command
env flags
git state
model artifact hash
hardware/OS/runtime version
warmup/iterations/round order
median_s and p95_s
tok/s
correctness metrics
```

## Correctness Gates

Exact promotion requires one of:

```text
bitexact hidden/logit parity
or documented tolerance that matches the existing exact baseline
```

Suggested ladder:

```text
1. operator checksum smoke
2. CPU reference for the touched operator
3. hidden-state dump
4. logits/top-k comparison
5. greedy token stream parity
6. long-context state/cache parity
```

For approximate, quantized, sparse, or windowed candidates, explicitly record:

```text
mean_abs_error
rms_error
max_abs_error
checksum_delta
logit rank changes
task-level quality if available
accepted budget
```

Never hide approximate speedups inside the exact accepted profile.

## Decode Gate

Decode promotion must use realistic output lengths:

```text
minimum:
  tg128 and tg512

preferred:
  target product generation lengths and long-context positions

variants to test:
  accepted
  attention split/no-split
  recurrent rowcache on/off
  storage mode
  sync/async command policy
  quantized KV/state if applicable
```

Do not promote decode flags from 1-token, 4-token, or single-layer wins.

## Prefill Gate

Prefill promotion must cover:

```text
short prompt
mid prompt
long prompt
model target context
```

For long-context work, distinguish:

```text
exact attention/recurrent semantics
approximate precision
sparse/windowed semantics
KV/cache pruning
prefill chunking or ubatching
```

## Cache And Memory Forensics

Classify bytes:

```text
unique_weight_bytes
weight_group_stream_bytes
logical_operand_weight_bytes
modeled_dram_miss_bytes
modeled_cache_hit_bytes
non_weight_bytes
scratch_write_bytes
scratch_read_bytes
persistent_state_bytes
tail_underfill
```

Interpretation rules:

```text
compulsory misses:
  model and accept them

avoidable re-reads:
  eliminate by layout, fusion, tiling, caching, or backend choice

scratch explosion:
  count writes and reads; split/reduce can lose despite better parallelism

low modeled bytes but slow runtime:
  suspect register pressure, occupancy, barriers, serialization, or bad backend

high effective GB/s in a naive path:
  may indicate repeated avoidable streaming, not a good kernel
```

Do not claim hardware cache-miss rates unless a named counter source was
captured. If only timing and byte models exist, label the evidence
`inferred-only`.

## Decision Records

Every accept/reject/opt-in decision must include:

```text
hypothesis
changed files or flags
baseline and candidate env
token/context sweep
median and p95
correctness result
cache/roofline interpretation
reference comparison
decision
failure mode if rejected
do_not_repeat
retry_only_if
```

Negative results are mandatory knowledge. A rejected candidate without a
root-cause note is unfinished work.
