# Delta Gated Norm SIMD32x4 Experiment

## Metadata

```text
date: 2026-05-01 10:40 CEST
owner: Codex
subagents: code-inspection only; no benchmarks
model: Qwen3.5-0.8B text-only probe
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: dirty research workspace
target_path: vendor/metal/shaders/qwen35_08b/prefill_deltanet_gated_norm.metal
env_flag: CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1
```

## Run Manifest

```text
git_commit: 1e6888567
git_dirty_state: dirty research workspace
device: Apple M5
macos_version: macOS 26.2 build 25C56
metal_device_name: Apple M5, Metal 4
accepted_profile_path: docs/kernel-dev/accepted_profile.env
accepted_profile_hash: 9fbaabb2d5219904e92d5af877dc82aa8c9cabcc590a8f90ee2f1474c00ff8d4
metalpack_path: /tmp/ctox_qwen35_08b_real_fp16.metalpack
metalpack_manifest_hash: af0ae61f0b1eec332cd886fc49046f5371d36cf8393ded5747269533e9391897
weights_hash: e218ad6265b704de41b005711c0526078c2f78af815cbfba7c079a737aca0190
binary_path: target/release/bench_metalpack_prefill_delta3_ffn_superblock
build_profile: release
full_env_dump: n/a
baseline_env: accepted profile with MPS FFN, MPS DeltaProject, MPS DeltaOut sidecars
candidate_env: baseline + CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1
output_csv: /tmp/ctox_qwen35_paired_54015/combined_results.txt
dump_paths: n/a
reference_impl: 128-thread gated RMSNorm with threadgroup reduction
```

## Hypothesis

If one SIMDgroup owns a `(token, head)` gated RMSNorm and each lane processes
four columns, the separate gated-norm phase should use fewer barriers and reduce
the scan+norm prefix time without touching recurrent-state update order.

## Result

Paired Delta18+FFN stack with MPS sidecars:

```text
p512:
  baseline_median_s:   0.081951354
  candidate_median_s:  0.081622729
  median_delta:       -0.4010%
  baseline_checksum16: -0.927307
  candidate_checksum16:-0.911438

p4096:
  baseline_median_s:   0.685376292
  candidate_median_s:  0.699806146
  median_delta:        2.1054%
  baseline_checksum16: -0.927307
  candidate_checksum16:-0.911438
```

## Decision

```text
decision: rejected
reason: p4096 is slower and checksum drift appears even though only norm
        reduction order changed.
```

## Learning

Removing barriers from a small post-scan RMSNorm is not enough when it changes
the reduction order and does not attack the real recurrent-state scan cost. This
candidate is removed from the regular autotune search to avoid wasting tuning
time, but the env-gated kernel remains as a negative control.
