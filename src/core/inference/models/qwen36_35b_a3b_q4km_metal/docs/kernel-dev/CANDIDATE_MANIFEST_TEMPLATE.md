# Kernel Candidate Manifest Template

Use this record for hand-written, autotuned, or OpenEvolve-style generated
kernel/layout candidates before they enter paired benchmark comparison.

```text
date: <YYYY-MM-DD HH:MM TZ>
candidate_id: <stable id>
source: manual|autotune|openevolve|paper|llama.cpp|luce|other
owner: <main-thread|subagent-id|human>
status: proposed|implemented|measured|accepted|rejected|abandoned
kernel_or_file: <path/function>
env_flag: <env flag or n/a>
intended_bottleneck: <phase/op>
baseline_profile: <accepted_profile hash or n/a>
```

## Hypothesis

```text
expected_win:
  <memory traffic / cache hit / dispatch / occupancy / math improvement>

cache_or_scratch_hypothesis:
  <what should improve in cache/scratch behavior>

risk:
  <numerical drift / barriers / register pressure / p95 / memory growth>
```

## Search / Mutation Space

```text
parameters:
  <tile sizes, vec width, chunk size, layout, split-k, threadgroup size>

fixed_constraints:
  <dtype, exact/approximate mode, model shape, context limits>

target_compute_backend:
  gpu_msl_simdgroup|gpu_mps_matrix|gpu_metal4_tensor|cpu_neon|cpu_sme|coreml_ane|hybrid|unknown

hardware_feature_contract:
  <SIMD width, matrix unit, MPS op, SME/I8MM/BF16 feature, or why unknown>

hardware_feature_evidence:
  <capture_hardware_feature_matrix path, Apple docs, local probe, or n/a>

layout_prefetch_contract:
  <contiguous stream, row/col tile order, group stride, page/block order>

speculative_access_hypothesis:
  <why adjacent lanes/threadgroups/CPU prefetchers see the next needed data>

quantization_lifecycle_state:
  n/a|proposed|calibrated|packed|gated|accepted|rejected

static_pack_artifact:
  <path/hash or n/a>

calibration_dataset:
  <dataset/hash or n/a>

scale_granularity:
  n/a|per_tensor|per_channel|group_32|group_64|group_128|other

dequant_location:
  n/a|none|in_dot_only|materialized_tensor

fallback_dtype:
  <f16|bf16|f32|int8|q4|n/a>

error_budget_record:
  <path or n/a>

generated_by:
  <script/model/prompt/commit or n/a>
```

## Correctness Gate

```text
gate_type:
  checksum|hidden_dump|logits|greedy_tokens|operator_ref

max_abs_error:
  <number or n/a>

checksum_delta:
  <number or n/a>

approximate_mode:
  yes|no

quality_drift_required:
  yes|no
```

## Benchmark Plan

```text
tokens_or_contexts:
  <512,4096,16384,...>

iterations:
  <n>

warmup:
  <n>

paired_order:
  yes|no

reference:
  accepted_profile|llama.cpp|MLX|operator_cpu|other
```

## Result

```text
median_delta_percent:
  <number or n/a>

p95_delta_percent:
  <number or n/a>

roofline_class:
  bandwidth-bound|compute-bound|dispatch-bound|unknown

metal_error_stats:
  <compile/runtime/validation failures>

regressions:
  <token/context/operator regressions>
```

## Decision

```text
decision:
  accept|reject|keep-opt-in|needs-more-data

accept_reject_reason:
  <short reason>

next_action:
  <next step>
```
