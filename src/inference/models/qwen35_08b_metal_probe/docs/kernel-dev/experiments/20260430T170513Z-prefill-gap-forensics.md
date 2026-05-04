# Kernel Experiment: prefill-gap-forensics

Generated: 20260430T170513Z

# Kernel Experiment Template

Copy this into `RESEARCH_LOG.md` before implementing a nontrivial kernel,
layout, runtime, or autotuning change.

## Metadata

```text
date: 20260430T170513Z
owner: michaelwelsch
subagents:
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: 5081442bf
target_path:
env_flag:
```

## Run Manifest

Required for reproducibility:

```text
git_commit: 5081442bf
git_dirty_state: ?? ./
device: Darwin 7a2bc49e-c674-48c4-8100-61d695ac9b31.fritz.box 25.2.0 Darwin Kernel Version 25.2.0: Tue Nov 18 21:09:49 PST 2025; root:xnu-12377.61.12~1/RELEASE_ARM64_T8142 arm64
macos_version: 26.2
metal_device_name: Apple M5;Metal 4
accepted_profile_path: /Users/michaelwelsch/Documents/ctox/src/inference/models/qwen35_08b_metal_probe/docs/kernel-dev/accepted_profile.env
accepted_profile_hash: fea814a42ac1bfebce567a5c4a0ac090524c4def8fb97fa7670f28abbc91de3c
metalpack_path: /tmp/ctox_qwen35_08b_real_fp16.metalpack
metalpack_manifest_hash: af0ae61f0b1eec332cd886fc49046f5371d36cf8393ded5747269533e9391897
weights_hash: e218ad6265b704de41b005711c0526078c2f78af815cbfba7c079a737aca0190
binary_path:
build_profile: release
full_env_dump: /tmp/ctox_qwen35_env_20260430T170513Z_prefill-gap-forensics.txt
baseline_env: /Users/michaelwelsch/Documents/ctox/src/inference/models/qwen35_08b_metal_probe/docs/kernel-dev/accepted_profile.env
candidate_env:
output_csv: /tmp/ctox_qwen35_20260430T170513Z_prefill-gap-forensics.csv
dump_paths: /tmp/ctox_qwen35_20260430T170513Z_prefill-gap-forensics_*.bin
reference_impl: MLX + llama.cpp
```

## Hypothesis

```text
If we change:
  <kernel/layout/runtime behavior>

Then:
  <median/p95/runtime/byte traffic should improve>

Because:
  <specific memory, math, dispatch, or occupancy reason>
```

The hypothesis must be falsifiable by a benchmark and a correctness gate.

## Scope

```text
files_allowed_to_edit:
  -

files_read_only:
  -

out_of_scope:
  -
```

## Expected Win

```text
primary metric:
  median_s | p95_s | tok/s | effective_GB/s | modeled_bytes

expected direction:
  <for example: reduce QKV/Z weight stream by N%, remove one dispatch per layer>

minimum useful win:
  <for example: >= 3% median improvement and no p95 regression>
```

## Risk

```text
correctness risk:
  <normalization order, accumulation order, cache update ownership, quant error>

performance risk:
  <register pressure, occupancy, tail underfill, scratch traffic, CPU overhead>

debug risk:
  <hard-to-dump state, non-determinism, thermal variance>
```

## Correctness Gate

```text
minimum:
  checksum smoke

required before acceptance:
  hidden dump compare
  logits compare
  greedy token parity
  long-context state/cache parity if touching attention or recurrence

thresholds:
  mean_abs_error <=
  rms_error <=
  max_abs_error <=
  abs(checksum_delta) <=
```

## Benchmark Plan

```text
baseline_env:
  -

candidate_env:
  -

commands:
  -

tokens/context:
  512
  4096
  16384

iterations:
warmup:
serial_only:
  yes
```

## Cache / Memory Model

```text
unique_weight_bytes:
weight_group_stream_bytes:
logical_operand_weight_bytes:
reuse_opportunity:
non_weight_bytes:
scratch_bytes:
tail_underfill:
modeled_l2_fit:
```

## Decision Rule

```text
accept if:
  -

reject if:
  -

keep opt-in if:
  -
```

## Result

Fill after running:

```text
baseline:
  median_s:
  p95_s:
  tok/s:
  checksum:

candidate:
  median_s:
  p95_s:
  tok/s:
  checksum:

correctness:
  pass/fail:
  notes:

decision:
  accepted | rejected | opt-in
```
