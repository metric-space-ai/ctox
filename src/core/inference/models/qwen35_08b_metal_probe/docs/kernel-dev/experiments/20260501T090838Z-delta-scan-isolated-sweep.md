# Kernel Experiment: delta-scan-isolated-sweep

Generated: 20260501T090838Z

# Kernel Experiment Template

Copy this into `RESEARCH_LOG.md` before implementing a nontrivial kernel,
layout, runtime, or autotuning change.

## Metadata

```text
date: 20260501T090838Z
owner: michaelwelsch
subagents:
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
baseline_commit_or_state: 1e6888567
target_path: src/metal/bench.rs; src/bin/bench_metalpack_prefill_delta_scan.rs; tools/run_delta_scan_isolated_sweep.sh
env_flag: CTOX_QWEN35_DELTA_SCAN_ROWCACHE* / CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK
```

## Run Manifest

Required for reproducibility:

```text
git_commit: 1e6888567
git_dirty_state: ?? ./
device: Darwin MacBook-Pro-von-Michael.fritz.box 25.2.0 Darwin Kernel Version 25.2.0: Tue Nov 18 21:09:49 PST 2025; root:xnu-12377.61.12~1/RELEASE_ARM64_T8142 arm64
macos_version: 26.2
metal_device_name: Apple M5;Metal 4
accepted_profile_path: /Users/michaelwelsch/Documents/ctox/src/inference/models/qwen35_08b_metal_probe/docs/kernel-dev/accepted_profile.env
accepted_profile_hash: 9fbaabb2d5219904e92d5af877dc82aa8c9cabcc590a8f90ee2f1474c00ff8d4
metalpack_path: /tmp/ctox_qwen35_08b_real_fp16.metalpack
metalpack_manifest_hash: af0ae61f0b1eec332cd886fc49046f5371d36cf8393ded5747269533e9391897
weights_hash: e218ad6265b704de41b005711c0526078c2f78af815cbfba7c079a737aca0190
binary_path: target/release/bench_metalpack_prefill_delta_scan
build_profile: release
full_env_dump: /tmp/ctox_qwen35_env_20260501T090838Z_delta-scan-isolated-sweep.txt
baseline_env: /Users/michaelwelsch/Documents/ctox/src/inference/models/qwen35_08b_metal_probe/docs/kernel-dev/accepted_profile.env
candidate_env: plain; rowcache; rowcache_direct; rowcache_block64; rowcache_block32; rowcache_block_auto; lanes4_sharedqk_approx
output_csv: /tmp/ctox_qwen35_scan_isolated_20260501T_continue/raw.tsv
dump_paths: /tmp/ctox_qwen35_20260501T090838Z_delta-scan-isolated-sweep_*.bin
reference_impl: MLX + llama.cpp
```

## Hypothesis

```text
If we change:
  the measurement surface from integrated Delta-stack timing to isolated
  recurrent DeltaNet scan timing, and report kernel/dispatch/bytes metadata

Then:
  exact scan layout candidates can be compared without projection/FFN/attention
  noise, and approximate SIMD candidates can be measured as explicit controls

Because:
  the current full-prefill p4096 profile attributes roughly 40% of Delta-stack
  time to scan+norm; rowgroup/layout candidates differ by only a few percent in
  the integrated path, so they need a dedicated forensic surface
```

The hypothesis must be falsifiable by a benchmark and a correctness gate.

## Scope

```text
files_allowed_to_edit:
  - src/metal/bench.rs
  - src/bin/bench_metalpack_prefill_delta_scan.rs
  - tools/run_delta_scan_isolated_sweep.sh

files_read_only:
  - vendor/metal/shaders/qwen35_08b/prefill_deltanet_scan.metal
  - docs/kernel-dev/accepted_profile.env

out_of_scope:
  - accepted-profile promotion
  - approximate-path promotion
  - model-quality decisions
```

## Expected Win

```text
primary metric:
  median_s, tok/s, vs_block32, max_abs_error_out, max_abs_error_state

expected direction:
  identify exact scan layouts that beat accepted rowcache_block32 across
  512/4096/16384, or prove that the fast lane candidate is still approximate

minimum useful win:
  >= 3% mean median_s win versus rowcache_block32 at every measured token size,
  exact scan validation, and no worse integrated p95
```

## Risk

```text
correctness risk:
  recurrent update order, row ownership, synthetic inputs hiding model-state
  drift, short validation windows missing long-sequence accumulation drift

performance risk:
  row_state[128] register pressure, underfilled 32/64-row groups, dispatch
  overhead dominating short token counts, GB/s estimates becoming misleading
  when rowcache converts repeated DRAM state streaming into register/cache reuse

debug risk:
  Metal shader timing variance and hidden full-stack drift that does not appear
  in isolated synthetic q/k/v validation
```

## Correctness Gate

```text
minimum:
  checksum smoke plus max_abs_error_out/state for synthetic scan inputs

required before acceptance:
  hidden dump compare
  logits compare
  greedy token parity
  long-context state/cache parity if touching attention or recurrence

thresholds:
  mean_abs_error <= 0.0 for exact hidden-dump promotion
  rms_error <= 0.0 for exact hidden-dump promotion
  max_abs_error <= 0.0 for exact hidden-dump promotion
  abs(checksum_delta) <= 0.0 for exact hidden-dump promotion
```

## Benchmark Plan

```text
baseline_env:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1

candidate_env:
  no scan env
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT=1
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64=1
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
  CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1

commands:
  cargo build --release --bin bench_metalpack_prefill_delta_scan
  tools/run_delta_scan_isolated_sweep.sh --tokens 512,4096,16384 --rounds 2 --iterations 3 --warmup 2 --validate-tokens 8 --output-dir /tmp/ctox_qwen35_scan_isolated_20260501T_continue

tokens/context:
  512
  4096
  16384

iterations: 3
warmup: 2
serial_only:
  yes
```

## Cache / Memory Model

```text
unique_weight_bytes:
weight_group_stream_bytes:
logical_operand_weight_bytes:
reuse_opportunity: recurrent state row reused across all tokens in rowcache kernels
non_weight_bytes: q/k/v fp16 streams, beta/decay fp32 streams, out fp32 stream
scratch_bytes: none in isolated scan
tail_underfill: plain/rowcache use 16 head threadgroups; block32 uses 4x16; block64 uses 2x16; lanes4 uses 32x16 with 4-lane column ownership
modeled_l2_fit: full recurrent state is 1,048,576 bytes; one head state is 65,536 bytes
```

## Decision Rule

```text
accept if:
  an exact candidate beats rowcache_block32 by >= 3% across 512/4096/16384 and
  passes hidden-dump parity in full stack

reject if:
  exact candidates only reshuffle rowgroup size and fail to beat block32
  robustly

keep opt-in if:
  approximate SIMD lane candidates are faster but fail full-stack hidden/logit
  gates
```

## Result

Fill after running:

```text
baseline:
  rowcache_block32 p512 mean_median_s: 0.00182112
  rowcache_block32 p4096 mean_median_s: 0.0135023
  rowcache_block32 p16384 mean_median_s: 0.0544857
  checksum: 0.038058

candidate:
  lanes4_sharedqk p512 mean_median_s: 0.00125546, vs_block32: 1.45056
  lanes4_sharedqk p4096 mean_median_s: 0.00996406, vs_block32: 1.35509
  lanes4_sharedqk p16384 mean_median_s: 0.040948, vs_block32: 1.33061
  checksum: 0.038058

correctness:
  pass/fail: mixed
  notes: isolated synthetic validation passes at validate_tokens=8 and 512, but prior full-stack hidden dump for lanes4_sharedqk showed mean_abs 0.001943609 and checksum_delta -22.414070845

decision:
  no exact promotion; keep lanes4_sharedqk opt-in approximate
```

## Learning

Fill this even when the experiment fails.

```text
what_we_learned:
  Isolated scan timing shows rowcache_block32 remains the strongest exact scan
  family member in this sweep. The approximate lanes4_sharedqk kernel is
  structurally faster by 1.33-1.45x in scan-only timing, but synthetic scan
  validation is not enough to promote it because the full-stack hidden state
  previously drifted.

wrong_assumption:
  A single effective_GB/s number is not comparable across plain and rowcache
  scan kernels. Plain intentionally streams state per token and reports a high
  modeled GB/s, while rowcache removes that DRAM traffic and must be judged by
  time/tok_s plus a byte model specific to the rowcache reuse pattern.

dead_end:
  no

do_not_repeat:
  Do not promote scan layout changes from integrated stack deltas alone, and do
  not treat short synthetic validation as equivalent to full hidden/logit parity.

retry_only_if:
  a new exact state-layout/math kernel changes more than rowgroup size, or an
  approximate path has an explicit quantization/error acceptance gate

docs_to_update:
  RESEARCH_LOG.md
  KERNEL_DEV_HANDBOOK.md if this changes the strategy
```
