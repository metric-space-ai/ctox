# Delta Scan Lanes4 SharedQK Experiment

## Metadata

```text
date: 2026-05-01 10:30 CEST
owner: Codex
subagents: code-inspection only; no benchmark ownership
model: Qwen3.5-0.8B text-only probe
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: dirty research workspace
target_path: vendor/metal/shaders/qwen35_08b/prefill_deltanet_scan.metal
env_flag: CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1
```

## Run Manifest

```text
accepted_profile_path: docs/kernel-dev/accepted_profile.env
accepted_profile_hash: 9fbaabb2d5219904e92d5af877dc82aa8c9cabcc590a8f90ee2f1474c00ff8d4
metalpack_path: /tmp/ctox_qwen35_08b_real_fp16.metalpack
git_commit: 1e6888567
git_dirty_state: dirty, 29 changed/untracked paths in probe workspace
device: Apple M5
macos_version: macOS 26.2 build 25C56
metal_device_name: Apple M5, Metal 4
metalpack_manifest_hash: af0ae61f0b1eec332cd886fc49046f5371d36cf8393ded5747269533e9391897
weights_hash: e218ad6265b704de41b005711c0526078c2f78af815cbfba7c079a737aca0190
binary_path: target/release/bench_metalpack_prefill_delta3_ffn_superblock
build_profile: release
full_env_dump: n/a
baseline_env: accepted profile with MPS FFN, MPS DeltaProject, MPS DeltaOut sidecars
candidate_env: baseline + CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1
reference_impl: exact rowcache_block32 Delta scan
dump_paths: /tmp/ctox_delta_base_p4096.bin, /tmp/ctox_delta_lanes4_sharedqk_p4096.bin
```

## Hypothesis

If one SIMDgroup owns one DeltaNet state row and Q/K are loaded once into
threadgroup memory for the four rows in the threadgroup, the scan should keep
the `LANES4` SIMD reduction speedup while avoiding the long-context Q/K reload
regression from plain `LANES4`.

## Correctness Gate

```text
qwen35-08b half dump compare, tokens=4096, width=1024
mismatch_count:      3739506 / 4194304
mean_abs_error:      0.001943609
rms_error:           0.002542885
max_abs_error:       0.046875000
baseline_checksum:   195986.674956322
candidate_checksum:  195964.260885477
checksum_delta:      -22.414070845
```

This is not exact enough for accepted-profile promotion. It is an approximate
path in the same policy bucket as quantized compute: usable only behind an
explicit opt-in and later quality gates.

## Benchmark Result

Paired Delta18+FFN stack with MPS sidecars:

```text
p512:
  baseline_median_s:   0.071066604
  candidate_median_s:  0.061626000
  median_delta:       -13.2842%

p4096:
  baseline_median_s:   0.548291562
  candidate_median_s:  0.476927041
  median_delta:       -13.0158%

p16384:
  baseline_median_s:   2.198526084
  candidate_median_s:  1.917534208
  median_delta:       -12.7809%
```

Full-prefill forensics with exact MPS tiled attention:

```text
p4096:
  exact rowcache_block32:       0.853s, 4800.11 tok/s
  approx lanes4_sharedqk:       0.772s, 5306.76 tok/s

p16384:
  exact rowcache_block32:       4.000s, 4095.63 tok/s
  approx lanes4_sharedqk:       3.727s, 4396.51 tok/s

p32768:
  exact rowcache_block32:       9.684s, 3383.65 tok/s
  approx lanes4_sharedqk:       9.117s, 3594.08 tok/s
```

## Decision

```text
decision: opt-in approximate
reason: faster at 512/4096/16384 and improves full-prefill forensics, but hidden
        dump drift is too large for exact accepted-profile semantics.
```

## Learning

Plain SIMD was not sufficient: the first `LANES4` kernel was fast at p4096 but
lost most of its long-context value because each row reloaded the same Q/K data.
`LANES4_SHAREDQK` is the better architecture pattern: assign row ownership to
SIMDgroups, but make the threadgroup share the token-local Q/K vector.

Cache claim remains inferred from byte floors and runtime; this Mac exposes only
GPU timestamp counters in the current tool path, not hardware L2 miss counters.
