# Kernel Experiment Template

Copy this into `RESEARCH_LOG.md` before implementing a nontrivial kernel,
layout, runtime, or autotuning change.

## Metadata

```text
date:
owner:
subagents:
model:
metalpack:
baseline_commit_or_state:
target_path:
env_flag:
```

## Run Manifest

Required for reproducibility:

```text
git_commit:
git_dirty_state:
device:
macos_version:
metal_device_name:
accepted_profile_path:
accepted_profile_hash:
metalpack_path:
metalpack_manifest_hash:
weights_hash:
binary_path:
build_profile:
full_env_dump:
baseline_env:
candidate_env:
output_csv:
dump_paths:
reference_impl:
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

## Learning

Fill this even when the experiment fails.

```text
what_we_learned:
  -

wrong_assumption:
  -

dead_end:
  yes | no

do_not_repeat:
  -

retry_only_if:
  -

docs_to_update:
  RESEARCH_LOG.md
  KERNEL_DEV_HANDBOOK.md if this changes the strategy
```
