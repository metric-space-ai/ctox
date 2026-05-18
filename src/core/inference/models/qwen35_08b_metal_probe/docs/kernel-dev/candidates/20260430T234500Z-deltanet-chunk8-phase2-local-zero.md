# Kernel Candidate Manifest - deltanet-chunk8-phase2-local-zero

```text
date: 2026-04-30 23:45 CEST
candidate_id: deltanet-chunk8-phase2-local-zero
source: manual
owner: main-thread
status: measured
kernel_or_file: vendor/metal/shaders/qwen35_08b/prefill_deltanet_chunk_phase1.metal::phase2_local_zero + phase3_propagate + simd32x4 variants
env_flag: n/a
intended_bottleneck: DeltaNet prefill scan+norm structural replacement
baseline_profile: 2e63086c55ece30a62be5856d9c3f559aa3041f70be69db944cdb68561dfcc9a
```

## Hypothesis

```text
expected_win: prove complete chunked DeltaNet recurrence correctness before replacing serial rowcache scan

cache_or_scratch_hypothesis: expose scratch lifetime bugs before aggressive state-slice layouts

risk: reduction scratch races, barrier overhead, p95 inflation, excessive chunk-state writes
```

## Search / Mutation Space

```text
parameters: chunk=4|8|16|32 state_mode=f32|f16|f32x4 heads=16 head_dim=128 phase2-one-threadgroup-per-chunk-head-row phase3-one-threadgroup-per-head-row threads=32|128

fixed_constraints: exact mode Qwen3.5-0.8B DeltaNet dimensions full initial-state propagation

generated_by: manual
```

## Correctness Gate

```text
gate_type: operator_ref

max_abs_error: f32x4 full_out=0.000000076 full_state=0.000000007 at 2048 tokens

checksum_delta: n/a

approximate_mode: no

quality_drift_required: no
```

## Benchmark Plan

```text
tokens_or_contexts: 32,128,512,2048

iterations: 1-3

warmup: 0-1

paired_order: no

reference: operator_cpu
```

## Result

```text
median_delta_percent: not-compared

p95_delta_percent: not-compared

roofline_class: unknown

metal_error_stats: initial validation failed due threadgroup reduction scratch lifetime race; fixed with post-read barrier; phase3 full serial comparison passes

regressions: still isolated and not integrated; f16 state cuts memory but adds drift; f32x4 is best tested full_path at 2048 chunk32=0.017705625s
```

## Decision

```text
decision: needs-more-data

accept_reject_reason: correct full chunked recurrence but not an integrated performance candidate

next_action: integrate f32x4 schedule into DeltaNet stack candidate and compare against accepted rowcache with paired sweep
```
