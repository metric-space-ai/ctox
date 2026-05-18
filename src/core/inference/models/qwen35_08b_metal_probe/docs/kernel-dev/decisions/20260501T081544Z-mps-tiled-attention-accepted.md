# Kernel Decision Record - MPS Tiled Attention Accepted

## Decision

```text
date: 2026-05-01 10:15 CEST
experiment: docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md
decision: accepted
accepted_env: CTOX_QWEN35_ATTENTION_MPS_TILED=1; remove CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
rejected_env: n/a
```


## Strict Field Index

```text
one_sentence: Promote exact MPS tiled prefill attention because all six attention layers pass p512/p4096 raw-dump gates and sidecar full-prefill forensics beats llama.cpp at 4096, 16384, and 32768 tokens.
tokens/context: 512, 4096, 16384, 32768
iterations: 1 parity, 2 forensics
warmup: benchmark defaults
baseline_command: tools/run_mps_tiled_attention_parity_sweep.sh --tokens 4096 /tmp/ctox_qwen35_08b_real_fp16.metalpack
candidate_command: CTOX_QWEN35_ATTENTION_MPS_TILED=1 target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096|16384|32768 2 150 /tmp/ctox_qwen35_mps_ffn_sidecar /tmp/ctox_qwen35_mps_delta_project_sidecar /tmp/ctox_qwen35_mps_attention_out_sidecar /tmp/ctox_qwen35_mps_delta_out_sidecar
forensics_record: docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md
search_based: no
autotune_record: n/a
correctness_gate: p512 and p4096 raw attention dump comparison passes on layers 3,7,11,15,19,23 with max_abs_error <= 0.003906250 and mean_abs_error <= 0.000142806.
token_sweep_gate: p4096, p16384, and p32768 full-prefill forensics beats llama.cpp BF16/Metal references.
reference_comparison: exact MPS tiled forensics reaches 3889.99, 3329.39, and 2763.40 tok/s versus llama.cpp 2852.70, 2065.71, and 1325.20 tok/s.
next_experiment: Delta18+FFN sidecar bottleneck: scan/gated-norm/out orchestration and custom recurrent state work now dominate exact prefill.
hidden_mean_abs_error: 0.000113671
hidden_rms_error: 0.000230015
hidden_max_abs_error: 0.003906250
```

## Summary

```text
one_sentence:
  Promote exact MPS tiled prefill attention because all six attention layers pass
  p512/p4096 raw-dump gates and sidecar full-prefill forensics beats llama.cpp at
  4096, 16384, and 32768 tokens.
```

## Learning Capture

```text
hypothesis:
  Replacing the custom QH4 per-query KV scan with MPSMatrix tiled QK/PV plus MSL
  SIMD32 causal softmax/combine makes exact prefill attention matrix-shaped and
  removes the long-context bottleneck.

actual_result:
  MPS tiled attention is 2.6x-3.0x faster per layer at p4096 and 5x-6.2x faster
  at p16k/p32k layer-3 measurements. Full-prefill forensics with MPS sidecars
  beats llama.cpp at all three reference sizes.

failure_mode:
  The first synthetic bridge had an incorrect causal-mask row mapping.

root_cause:
  MPS Q rows are token-major with four Q heads per KV group, so query_row is
  row / heads_per_group, not row % q_tile.

do_not_repeat:
  Do not promote attention sidecars from speed-only evidence. Require raw dumps
  across all full-attention layers.

retry_only_if:
  A native Metal matrix backend or quantized KV path can beat MPS tiled attention
  while preserving exact or accepted-error semantics.
```

## Evidence

```text
model: Qwen3.5-0.8B text-only prefill full-attention layers
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512 and 4096 parity; 4096, 16384, 32768 full-prefill forensics
iterations: 1 parity, 2 forensics
rounds: n/a
warmup: benchmark defaults
baseline_command: tools/run_mps_tiled_attention_parity_sweep.sh --tokens 4096 /tmp/ctox_qwen35_08b_real_fp16.metalpack with accepted QH4 baseline
candidate_command: CTOX_QWEN35_ATTENTION_MPS_TILED=1 target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096|16384|32768 2 150 /tmp/ctox_qwen35_mps_ffn_sidecar /tmp/ctox_qwen35_mps_delta_project_sidecar /tmp/ctox_qwen35_mps_attention_out_sidecar /tmp/ctox_qwen35_mps_delta_out_sidecar
search_based: no
autotune_record: n/a
forensics_record: docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md
```

## Performance

```text
p4096 attention layer sweep:
  layer 3:  accepted 0.141636750 s, mps 0.048119584 s, speedup 2.94x
  layer 7:  accepted 0.144612208 s, mps 0.050074458 s, speedup 2.89x
  layer 11: accepted 0.144005875 s, mps 0.048715666 s, speedup 2.96x
  layer 15: accepted 0.142654416 s, mps 0.047907250 s, speedup 2.98x
  layer 19: accepted 0.140805834 s, mps 0.048427917 s, speedup 2.91x
  layer 23: accepted 0.127766167 s, mps 0.048660042 s, speedup 2.63x

full-prefill forensics with MPS sidecars:
  p4096:  1.053 s, 3889.99 tok/s, 1.36x llama.cpp
  p16384: 4.921 s, 3329.39 tok/s, 1.61x llama.cpp
  p32768: 11.858 s, 2763.40 tok/s, 2.09x llama.cpp
```

## Correctness

```text
checksum: FP16-tolerant raw attention dumps, not bitwise identical
hidden_mean_abs_error: 0.000113671
hidden_rms_error: 0.000230015
hidden_max_abs_error: 0.003906250
checksum_delta: -1.409674823
```

## Memory / Cache Interpretation

```text
modeled_bytes_delta:
  old QH4 path scanned KV per query in a custom loop; MPS tiled path performs
  QK/PV as matrix backend work and reports logical operand bytes above likely
  DRAM traffic.

weight_stream_delta:
  attention O sidecar streams O weights through MPS; QKV projection remains real
  prefill prepare path.

scratch_delta:
  MPS Q/K/V scratch plus score/prob/pv/out tile scratch added; outweighed by
  matrix backend throughput.

cache_miss_claim:
  inferred-only. Hardware cache counters are not exposed in this runner; p4096,
  p16k, and p32k forensics use byte-floor and DRAM-equivalent inference.
```

## Why This Decision Is Safe

```text
correctness_gate:
  p512 and p4096 raw attention dump comparison passes on layers 3,7,11,15,19,23
  with max_abs_error <= 0.003906250 and mean_abs_error <= 0.000142806.

integrated_path_gate:
  memory_forensics uses the integrated real attention-core path, not only the
  synthetic bridge benchmark.

reference_comparison:
  p4096/p16k/p32k full-prefill forensics beats llama.cpp BF16/Metal references.

tooling_gate:
  cargo release builds, prefill_reference_report, parity sweep, accepted-profile
  validator, and kernel-dev doctor pass.
```

## Follow-Up

```text
next_experiment:
  Delta18+FFN sidecar bottleneck: scan/gated-norm/out orchestration and custom
  recurrent state work now dominate exact prefill.

cleanup:
  Keep QH4 SIMD32 path as fallback/negative control; accepted profile should use
  MPS tiled attention by default on MPS-capable Apple GPUs.

docs_to_update:
  RESEARCH_LOG.md, KERNEL_DEV_HANDBOOK.md, accepted_profile.env

handbook_update_required: yes
```
