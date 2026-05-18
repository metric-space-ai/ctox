# Kernel Decision Record - QKV/Z RG4 A-Shared Rejected

## Decision

```text
date: 2026-04-30 22:25 CEST
experiment: docs/kernel-dev/experiments/20260430T170513Z-prefill-gap-forensics.md
decision: rejected
accepted_env: n/a
rejected_env: CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED=1
```

## Summary

```text
one_sentence:
  Reject the QKV/Z RG4 A-shared project kernel as a default because it improves
  only p4096 and regresses p512 and p16384 in paired alternating sweeps.
```

## Learning Capture

```text
hypothesis:
  Four row-groups per threadgroup can share one staged 128-token A tile and
  reduce repeated q_half loads in the QKV/Z projection.

actual_result:
  The candidate is checksum-stable but not performance-stable across token
  lengths.

failure_mode:
  Barrier/threadgroup-memory overhead and lower occupancy erase the A-load
  savings outside p4096.

root_cause:
  inferred - project is not primarily limited by q_half device reloads after
  QKV/Z128; pressure and scheduling dominate.

do_not_repeat:
  Do not promote Project A-staging candidates from a single p4096 win.

retry_only_if:
  future hardware counters show q_half reload misses dominate QKV/Z projection.
```

## Evidence

```text
model: Qwen3.5-0.8B text-only DeltaNet+FFN stack
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512, 4096, 16384 prefill token sweeps
iterations: 2
warmup: 0
baseline_command: tools/compare_delta_stack_candidate.sh --candidate-env CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED=1 --tokens <N> --rounds 2 --iterations 2 --warmup 0
candidate_command: CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED=1 via compare_delta_stack_candidate.sh
forensics_command: n/a
forensics_record: n/a
search_based: yes
autotune_record: /tmp/ctox_qwen35_paired_qkvz_rg4_ashared.txt
```

## Performance

```text
512:
  baseline_median_s: 0.195956042
  candidate_median_s: 0.201707562
  median_delta_percent: +2.9351

4096:
  baseline_median_s: 1.694889771
  candidate_median_s: 1.677932833
  median_delta_percent: -1.0005

16384:
  baseline_median_s: 6.839253979
  candidate_median_s: 7.079493792
  median_delta_percent: +3.5127
```

## Correctness

```text
checksum: unchanged -0.910950
hidden_mean_abs_error: n/a
hidden_rms_error: n/a
hidden_max_abs_error: n/a
checksum_delta: 0.000000000
greedy_tokens: n/a
logits_check: n/a
```

## Memory / Cache Interpretation

```text
modeled_bytes_delta:
  not yet represented in byte model; the candidate targets repeated q_half
  device reloads rather than weight-stream bytes

scratch_delta:
  adds 128x8 half threadgroup A tile and two threadgroup barriers per K step

cache_miss_claim:
  inferred-only; paired sweep does not justify a hardware miss claim
```

## Why This Decision Is Safe

```text
correctness_gate:
  checksum unchanged, but hidden dump was not needed because the candidate was
  rejected for performance instability

integrated_path_gate:
  full Delta18+FFN paired sweep regresses at p512 and p16384

token_sweep_gate:
  failed

reference_comparison:
  no default behavior change
```

## Follow-Up

```text
next_experiment:
  Luce-inspired two-phase chunked DeltaNet scan and 128-token residual MMA for
  down/out

cleanup:
  keep the env-gated kernel as an opt-in negative/control candidate

docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md

handbook_update_required: yes
```
