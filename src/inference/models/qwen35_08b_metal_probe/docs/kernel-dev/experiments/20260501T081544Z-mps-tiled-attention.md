# Kernel Experiment: mps-tiled-attention

## Metadata

```text
date: 20260501T081544Z
owner: michaelwelsch
subagents: n/a
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: 1e6888567 dirty untracked probe workspace
target_path: src/metal/bench.rs; vendor/metal/shaders/qwen35_08b/prefill_attention_tiled_full.metal; src/bin/memory_forensics.rs; tools/run_mps_tiled_attention_parity_sweep.sh
env_flag: CTOX_QWEN35_ATTENTION_MPS_TILED=1
```

## Run Manifest

```text
git_commit: 1e6888567
git_dirty_state: dirty - qwen35_08b_metal_probe untracked/modified plus unrelated harness files in parent repo
device: Darwin MacBook-Pro-von-Michael.fritz.box 25.2.0 Darwin Kernel Version 25.2.0: Tue Nov 18 21:09:49 PST 2025; root:xnu-12377.61.12~1/RELEASE_ARM64_T8142 arm64
macos_version: 26.2
metal_device_name: Apple M5;Metal 4
accepted_profile_path: /Users/michaelwelsch/Documents/ctox/src/inference/models/qwen35_08b_metal_probe/docs/kernel-dev/accepted_profile.env
accepted_profile_hash: 7e53ef2b3926542ce63c73e6f5e5f43b1e49926c6f71f7c0fc3478a29dbfaa9e
metalpack_path: /tmp/ctox_qwen35_08b_real_fp16.metalpack
metalpack_manifest_hash: af0ae61f0b1eec332cd886fc49046f5371d36cf8393ded5747269533e9391897
weights_hash: e218ad6265b704de41b005711c0526078c2f78af815cbfba7c079a737aca0190
binary_path: target/release/bench_metalpack_prefill_attention_core; target/release/memory_forensics; target/release/compare_attention_raw_dump
build_profile: release
full_env_dump: /tmp/ctox_qwen35_env_20260501T081544Z_mps-tiled-attention.txt
baseline_env: docs/kernel-dev/accepted_profile.env with CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
candidate_env: docs/kernel-dev/accepted_profile.env with CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8 unset and CTOX_QWEN35_ATTENTION_MPS_TILED=1
output_csv: n/a
dump_paths: /tmp/ctox_mps_tiled_attention_parity.*; /tmp/ctox_attn_accepted_p4096.bin; /tmp/ctox_attn_mps_p4096.bin
reference_impl: accepted QH4 SIMD32 exact attention; llama.cpp BF16/Metal for prefill throughput reference
```

## Hypothesis

```text
If we change:
  the exact full-attention prefill backend from custom per-query QH4 SIMD32 KV
  scan to a tiled MPSMatrix QK/PV backend with MSL SIMD32 causal softmax/combine

Then:
  attention-core median_s should fall strongly at 4096, 16384, and 32768 tokens,
  and full-prefill forensics should beat llama.cpp at those sizes

Because:
  QK and PV become matrix-shaped work that can use the Apple GPU/MPS matrix
  backend instead of streaming KV per query in a handwritten scalar/SIMD loop.
```

## Scope

```text
files_allowed_to_edit:
  src/metal/bench.rs
  vendor/metal/shaders/qwen35_08b/prefill_attention_tiled_full.metal
  src/bin/memory_forensics.rs
  tools/run_prefill_mps_tiled_projection.sh
  tools/run_mps_tiled_attention_parity_sweep.sh
  docs and research log

files_read_only:
  metalpack weights
  llama.cpp reference data

out_of_scope:
  approximate sparse/window attention
  quantized KV-cache promotion
  DeltaNet scan rewrite
```

## Expected Win

```text
primary metric:
  attention-core median_s and full_prefill_estimate_current_kernels tok/s

expected direction:
  reduce exact long-context attention core by at least 2x at p4096 and at least
  5x at p16k/p32k against accepted QH4

minimum useful win:
  full-prefill forensics beats llama.cpp at 4096, 16384, and 32768 tokens with
  FP16-tolerant raw attention dump parity
```

## Risk

```text
correctness risk:
  causal mask row mapping, GQA KV group to Q-head store mapping, attention gate
  application, MPS accumulation-order drift

performance risk:
  MPS encode overhead, scratch pack traffic, row/column stride alignment,
  small-context overhead below p512

debug risk:
  MPSMatrix is opaque to cache counters; byte buckets are logical operands, not
  hardware DRAM counters
```

## Correctness Gate

```text
minimum:
  checksum smoke plus raw attention dump compare

required before acceptance:
  raw attention dump compare on all six full-attention layers at p512 and p4096

thresholds:
  mean_abs_error <= 0.00015
  rms_error <= 0.00030
  max_abs_error <= 0.00400
  abs(checksum_delta) <= 2.0 at p4096
```

## Benchmark Plan

```text
baseline_env:
  CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1 from accepted profile

candidate_env:
  CTOX_QWEN35_ATTENTION_MPS_TILED=1 with QH4 unset

commands:
  tools/run_mps_tiled_attention_parity_sweep.sh --tokens 512 /tmp/ctox_qwen35_08b_real_fp16.metalpack
  tools/run_mps_tiled_attention_parity_sweep.sh --tokens 4096 /tmp/ctox_qwen35_08b_real_fp16.metalpack
  CTOX_QWEN35_ATTENTION_MPS_TILED=1 target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096|16384|32768 2 150 /tmp/ctox_qwen35_mps_ffn_sidecar /tmp/ctox_qwen35_mps_delta_project_sidecar /tmp/ctox_qwen35_mps_attention_out_sidecar /tmp/ctox_qwen35_mps_delta_out_sidecar

tokens/context:
  512
  4096
  16384
  32768

iterations: 1 for parity dumps; 2 for forensics
warmup: benchmark defaults
serial_only:
  yes
```

## Cache / Memory Model

```text
unique_weight_bytes:
  attention.core unique weights about 14 MiB, sidecar O about 24 MiB total for six attention layers
weight_group_stream_bytes:
  MPS tiled attention core removes the custom per-query KV scan as the dominant execution form
logical_operand_weight_bytes:
  MPS rows report logical operand bytes; hardware DRAM/cache counters are not exposed here
reuse_opportunity:
  large; p32768 attention logical KV visits are over 1 TiB per layer in the old byte model
non_weight_bytes:
  attention scratch dominates for long context
scratch_bytes:
  q/k/v MPS scratch plus score/prob/pv/out tile scratch
 tail_underfill:
  0 for tested powers-of-two token counts
modeled_l2_fit:
  K/V cache does not fit fully at long context; MPS tiling makes this matrix-shaped instead of query-scan shaped
```

## Decision Rule

```text
accept if:
  p512 and p4096 all-layer raw dump gates pass and p4096/p16k/p32k full-prefill
  forensics beats llama.cpp

reject if:
  any layer exceeds max_abs_error 0.004 or full-prefill p16k/p32k remains below llama.cpp

keep opt-in if:
  only small-context speedups are positive or if sidecar/full-prefill integration remains unproven
```

## Result

```text
baseline:
  median_s:
    p4096 accepted attention per layer approximately 0.120-0.145 s
    p16384 accepted attention layer 3 1.632261083 s
    p32768 accepted attention layer 3 6.649290125 s
  p95_s: n/a
  tok/s: see memory_forensics rows
  checksum: accepted raw dump reference

candidate:
  median_s:
    p4096 MPS attention per layer approximately 0.0479-0.0501 s
    p16384 MPS attention layer 3 0.326072458 s
    p32768 MPS attention layer 3 1.069706084 s
  p95_s: n/a
  tok/s:
    p4096 full-prefill forensics 3889.99 tok/s vs llama.cpp 2852.70
    p16384 full-prefill forensics 3329.39 tok/s vs llama.cpp 2065.71
    p32768 full-prefill forensics 2763.40 tok/s vs llama.cpp 1325.20
  checksum: raw dump checksum deltas within threshold

correctness:
  pass/fail: pass
  notes: p512 and p4096 all six full-attention layers pass FP16-tolerant raw dump gates

decision:
  accepted
```

## Learning

```text
what_we_learned:
  The prefill attention gap was a backend-shape problem. MPSMatrix tiled QK/PV
  plus thin MSL softmax/combine beats the custom per-query scan by 2.6x-6.2x.

wrong_assumption:
  The first bridge used row % q_tile for causal masking; the correct row mapping
  is token-major, row / heads_per_group.

dead_end:
  no

do_not_repeat:
  Do not trust a sidecar attention speedup until causal-mask and GQA-store layout
  are raw-dump checked across all full-attention layers.

retry_only_if:
  MPS encode overhead regresses, or a future native Metal matrix path exposes
  faster direct MMA with better cache-counter observability.

docs_to_update:
  RESEARCH_LOG.md
  KERNEL_DEV_HANDBOOK.md
  docs/kernel-dev/accepted_profile.env
```
