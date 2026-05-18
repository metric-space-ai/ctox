# Delta Scan Rowcache Block Auto Decision

## Decision

```text
date: 2026-05-01 10:54 CEST
experiment: docs/kernel-dev/experiments/20260501T085442Z-delta-scan-rowcache-block-auto.md
decision: rejected
accepted_env: n/a
rejected_env: CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
```

## Summary

```text
one_sentence:
  Reject rowcache_block_auto because it is exact but regresses p4096 and p16384
  in the current paired sidecar pipeline.
```

## Learning Capture

```text
hypothesis:
  Use block32 for short prompts and block64 for longer prompts.

actual_result:
  p512 improved by 3.11%, but p4096 regressed by 1.49% and p16384 regressed by
  1.77%.

failure_mode:
  p95/runtime instability and size-dependent regression

root_cause:
  inferred - block32/block64 are too close; rowgroup size is not the main scan
  limiter after row_state caching.

do_not_repeat:
  Do not promote token-threshold micro-choices from a single positive size.

retry_only_if:
  A longer repeated sweep with controlled thermals shows a stable multi-size
  win, or rowgroup choice becomes part of a broader scan rewrite.
```

## Evidence

```text
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512, 4096, 16384
iterations: 1
warmup: 1
baseline_command: tools/compare_delta_stack_candidate.sh --candidate-reset-tuning-env ...
candidate_command: CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1 via compare tool
forensics_command: n/a
forensics_record: n/a
search_based: no
autotune_record: n/a
```

## Performance

```text
baseline_median_s:
  p512:   0.075132958
  p4096:  0.591575917
  p16384: 2.373350500

candidate_median_s:
  p512:   0.072797500
  p4096:  0.600406791
  p16384: 2.415476166

median_delta_percent:
  p512:   -3.1084
  p4096:   1.4928
  p16384:  1.7749
```

## Correctness

```text
checksum:
  exact checksum16 parity in all measured rows
hidden_mean_abs_error: n/a
hidden_rms_error:      n/a
hidden_max_abs_error:  n/a
checksum_delta:        0
greedy_tokens:         not run
logits_check:          not run
```

## Memory / Cache Interpretation

```text
modeled_bytes_baseline: unchanged
modeled_bytes_candidate: unchanged
weight_stream_delta: none
scratch_delta: rowgroup shape only
tail_underfill: unchanged
dram_equivalent_bytes: inferred-only
cache_miss_claim:
  inferred-only
```

## Why This Decision Is Safe

```text
correctness_gate:
  checksum exact, but performance fails.
integrated_path_gate:
  measured in Delta18+FFN MPS-sidecar stack.
token_sweep_gate:
  p512, p4096, p16384.
reference_comparison:
  no accepted-profile change.
```

## Follow-Up

```text
next_experiment:
  Structural exact scan rewrite or approximate/quantized scan quality gates.
cleanup:
  Keep env-gated auto mode as an opt-in negative control.
docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md
handbook_update_required: yes
```
