# Kernel Decision Record - Rowcache Block32 Accepted

## Decision

```text
date: 2026-04-30 22:05 CEST
experiment: docs/kernel-dev/experiments/20260430T170513Z-prefill-gap-forensics.md
decision: accepted
accepted_env: CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1
rejected_env: n/a
```

## Summary

```text
one_sentence:
  Promote DeltaNet scan rowcache_block32 because it is hidden-dump bitexact and
  wins the paired alternating Delta18+FFN sweep at 512, 4096, and 16384 tokens.
```

## Learning Capture

```text
hypothesis:
  Smaller row blocks reduce per-threadgroup pressure in the rowcache scan while
  preserving the exact rowcache arithmetic order.

actual_result:
  block32 is bitexact and shows a small paired-order win across the tested token
  lengths.

failure_mode:
  Unpaired baseline-then-candidate sweeps were noisy enough to contradict the
  p4096 result.

root_cause:
  inferred - block32 lowers pressure enough to offset duplicated Q/K staging,
  but the gain is small and sensitive to run-order noise.

do_not_repeat:
  Do not make sub-percent scan decisions from unpaired sweeps.

retry_only_if:
  a future recurrence rewrite or hardware profile changes the scan pressure
  tradeoff enough to justify a new paired sweep.
```

## Evidence

```text
model: Qwen3.5-0.8B text-only DeltaNet+FFN stack
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512, 4096, 16384 prefill token sweeps
iterations: 2
rounds: 2 alternating baseline/candidate
warmup: 0
candidate_command: tools/compare_delta_stack_candidate.sh --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1 --tokens 512,4096,16384 --rounds 2 --iterations 2
search_based: yes
autotune_record: /tmp/ctox_qwen35_paired_block32_sweep.txt
```

## Performance

```text
512:
  baseline_median_s: 0.190638166
  candidate_median_s: 0.189972708
  median_delta_percent: -0.3491

4096:
  baseline_median_s: 1.493337584
  candidate_median_s: 1.484006187
  median_delta_percent: -0.6249

16384:
  baseline_median_s: 6.015355376
  candidate_median_s: 5.999566792
  median_delta_percent: -0.2625
```

## Correctness

```text
checksum: unchanged -0.910950
hidden_mean_abs_error: 0.000000000
hidden_rms_error: 0.000000000
hidden_max_abs_error: 0.000000000
checksum_delta: 0.000000000
greedy_tokens: n/a
logits_check: n/a
```

## Memory / Cache Interpretation

```text
modeled_bytes_delta:
  no major modeled byte reduction; this is primarily an occupancy/register
  pressure and scheduling change inside the scan recurrence

weight_stream_delta:
  none expected

scratch_delta:
  Q/K staging is duplicated across more threadgroups per head

cache_miss_claim:
  inferred-only; no hardware counter capture in this record
```

## Why This Decision Is Safe

```text
correctness_gate:
  p4096 final hidden dump mismatch_count=0, max_abs_error=0, checksum_delta=0

integrated_path_gate:
  full Delta18+FFN paired sweep improves at 512, 4096, and 16384 tokens

reference_comparison:
  bitexact against previous accepted hidden dump; no model semantics change

tooling_gate:
  accepted_profile validator, autotune-default drift guard, kernel-dev doctor,
  and cache_model tests pass after promotion
```

## Follow-Up

```text
next_experiment:
  recurrence-level scan math, double-buffered Q/K staging, and projection
  register-pressure forensics

cleanup:
  keep block64 as opt-in candidate; keep rejected scan variants as negative
  controls for the autotuner and docs

docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md, accepted_profile.env

handbook_update_required: yes
```
