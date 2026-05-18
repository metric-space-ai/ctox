# Delta Scan Lanes4 SharedQK Decision

## Decision

```text
date: 2026-05-01 10:30 CEST
experiment: docs/kernel-dev/experiments/20260501T083010Z-delta-scan-lanes4-sharedqk.md
decision: opt-in
accepted_env: n/a
opt_in_env: CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1
rejected_env: promote-to-exact-accepted-profile
```

## Summary

```text
one_sentence:
  Keep lanes4_sharedqk as an approximate SIMD32 Delta scan path because it wins
  about 13% on the Delta18+FFN stack but introduces measurable hidden-state drift.
```

## Learning Capture

```text
hypothesis:
  Reusing Q/K through threadgroup memory should preserve the SIMD row-reduction
  win and avoid the plain lanes4 long-context reload problem.

actual_result:
  p512/p4096/p16384 stack medians improved by 13.28%, 13.02%, and 12.78%.
  Full-prefill forensics improved from 4800.11 to 5306.76 tok/s at p4096,
  from 4095.63 to 4396.51 tok/s at p16384, and from 3383.65 to 3594.08 tok/s
  at p32768.

failure_mode:
  correctness drift

root_cause:
  inferred - SIMDgroup reduction changes the Delta recurrence accumulation
  order compared with the exact scalar row loop.

do_not_repeat:
  Do not promote SIMD reductions in recurrent state code as exact merely because
  checksum smoke looks close; require full hidden dump comparison.

retry_only_if:
  A quantized/approx profile is explicitly being tuned, or model-level quality
  gates show the drift is acceptable.
```

## Evidence

```text
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512, 4096, 16384, 32768 forensics
iterations: paired stack 2 except p16384 quick check; forensics 1
warmup: 1
baseline_command: tools/compare_delta_stack_candidate.sh ... MPS sidecars
candidate_command: CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1 tools/compare_delta_stack_candidate.sh ... MPS sidecars
forensics_command: CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1 tools/run_accepted_profile.sh target/release/memory_forensics ...
forensics_record: /tmp/ctox_forensics_sharedqk_p4096.txt, /tmp/ctox_forensics_sharedqk_p16384.txt, /tmp/ctox_forensics_sharedqk_p32768.txt
search_based: no
autotune_record: n/a
```

## Performance

```text
baseline_median_s:
  p512:   0.071066604
  p4096:  0.548291562
  p16384: 2.198526084

candidate_median_s:
  p512:   0.061626000
  p4096:  0.476927041
  p16384: 1.917534208

median_delta_percent:
  p512:   -13.2842
  p4096:  -13.0158
  p16384: -12.7809
```

## Correctness

```text
checksum:
  baseline_checksum16:  -0.927307
  candidate_checksum16: -0.919556
hidden_mean_abs_error: 0.001943609
hidden_rms_error:      0.002542885
hidden_max_abs_error:  0.046875000
checksum_delta:        -22.414070845
greedy_tokens:         not run
logits_check:          not run
```

## Memory / Cache Interpretation

```text
modeled_bytes_baseline: inferred from benchmark byte model
modeled_bytes_candidate: inferred from benchmark byte model
weight_stream_delta: none
scratch_delta: +256 threadgroup floats per threadgroup for shared Q/K
tail_underfill: unchanged
dram_equivalent_bytes: inferred from runtime and bandwidth assumption
cache_miss_claim:
  inferred-only
```

## Why This Decision Is Safe

```text
correctness_gate:
  full hidden dump shows drift; therefore no exact-profile promotion.
integrated_path_gate:
  Delta18+FFN stack and full-prefill forensics both run.
token_sweep_gate:
  p512, p4096, p16384 stack and p4096/p16384/p32768 forensics measured.
reference_comparison:
  full-prefill forensics stays above llama.cpp, but this row is approximate.
```

## Follow-Up

```text
next_experiment:
  Quality-gate approximate Delta scan against logits/greedy decode, or build an
  exact SIMD-friendly scan that preserves accumulation order.
cleanup:
  Keep CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK as opt-in.
docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md, README.md, prefill_reference_report.py
handbook_update_required: yes
```
