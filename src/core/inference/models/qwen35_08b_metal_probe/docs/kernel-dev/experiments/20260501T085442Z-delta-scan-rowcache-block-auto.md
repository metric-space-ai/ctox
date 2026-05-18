# Delta Scan Rowcache Block Auto Experiment

## Metadata

```text
date: 2026-05-01 10:54 CEST
owner: Codex
subagents: none
model: Qwen3.5-0.8B text-only probe
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: dirty research workspace
target_path: src/metal/bench.rs
env_flag: CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
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
baseline_env: accepted profile with rowcache_block32
candidate_env: baseline tuning reset + CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
output_csv: /tmp/ctox_qwen35_paired_77590/combined_results.txt
dump_paths: n/a
reference_impl: accepted rowcache_block32
```

## Hypothesis

If rowcache_block64 is slightly better at longer prompts but rowcache_block32 is
slightly better at short prompts, a token-aware auto selector should use block32
below 4096 tokens and block64 from 4096 tokens onward.

## Result

Fair paired result with `--candidate-reset-tuning-env`:

```text
p512:
  baseline_median_s:   0.075132958
  candidate_median_s:  0.072797500
  median_delta:       -3.1084%
  checksum:            exact

p4096:
  baseline_median_s:   0.591575917
  candidate_median_s:  0.600406791
  median_delta:        1.4928%
  checksum:            exact

p16384:
  baseline_median_s:   2.373350500
  candidate_median_s:  2.415476166
  median_delta:        1.7749%
  checksum:            exact
```

## Decision

```text
decision: rejected / keep opt-in only
reason: exact but not robust; p4096 and p16384 regress in the current paired run.
```

## Learning

Do not promote token-threshold micro-choices from one noisy positive point.
Block32 vs block64 is close enough that thermal/order variance can flip the
winner. Exact scan work now needs a more structural change than rowgroup-size
selection.
