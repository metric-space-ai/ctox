# Delta Gated Norm SIMD32x4 Decision

## Decision

```text
date: 2026-05-01 10:40 CEST
experiment: docs/kernel-dev/experiments/20260501T084028Z-delta-gated-norm-simd32x4.md
decision: rejected
accepted_env: n/a
rejected_env: CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1
```

## Summary

```text
one_sentence:
  Reject gated_norm_simd32x4 because it is slower at p4096 and introduces
  checksum drift.
```

## Learning Capture

```text
hypothesis:
  A SIMDgroup-owned gated RMSNorm should reduce barrier overhead after Delta
  scan.

actual_result:
  p512 improved only 0.40%, p4096 regressed 2.11%, and checksum changed from
  -0.927307 to -0.911438.

failure_mode:
  slower runtime and correctness drift

root_cause:
  inferred - the separate norm is not the dominant enough cost, and SIMDgroup
  reduction order changes the normalized output.

do_not_repeat:
  Do not optimize small post-scan reductions before proving their isolated share
  dominates the integrated prefix.

retry_only_if:
  A broader exact scan+norm redesign removes a dispatch or reuses scan-local
  partials without extra global traffic.
```

## Evidence

```text
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512, 4096
iterations: 2
warmup: 1
baseline_command: tools/compare_delta_stack_candidate.sh ... MPS sidecars
candidate_command: CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1 tools/compare_delta_stack_candidate.sh ... MPS sidecars
forensics_command: n/a
forensics_record: n/a
search_based: no
autotune_record: n/a
```

## Performance

```text
baseline_median_s:
  p512:  0.081951354
  p4096: 0.685376292

candidate_median_s:
  p512:  0.081622729
  p4096: 0.699806146

median_delta_percent:
  p512:  -0.4010
  p4096:  2.1054
```

## Correctness

```text
checksum:
  baseline_checksum16:  -0.927307
  candidate_checksum16: -0.911438
hidden_mean_abs_error: n/a
hidden_rms_error:      n/a
hidden_max_abs_error:  n/a
checksum_delta:        n/a
greedy_tokens:         not run
logits_check:          not run
```

## Memory / Cache Interpretation

```text
modeled_bytes_baseline: unchanged
modeled_bytes_candidate: unchanged
weight_stream_delta: none
scratch_delta: lower threadgroup reduction scratch/barriers
tail_underfill: unchanged
dram_equivalent_bytes: inferred-only
cache_miss_claim:
  inferred-only
```

## Why This Decision Is Safe

```text
correctness_gate:
  checksum drift already fails the cheap gate.
integrated_path_gate:
  measured in the integrated Delta18+FFN MPS-sidecar stack.
token_sweep_gate:
  p512 and p4096 measured; p4096 regression is enough to reject.
reference_comparison:
  no accepted-profile change.
```

## Follow-Up

```text
next_experiment:
  Focus on the recurrent scan itself or exact MPS/SME-backed projection paths.
cleanup:
  Remove from regular autotune candidate list; keep env-gated code as negative control.
docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md
handbook_update_required: yes
```
