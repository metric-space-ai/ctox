# Quant Pipeline Record Template

Use this before implementing or promoting a quantized path. The goal is to
prevent accidental conversion-heavy pipelines such as `f32 -> f16 -> f32` that
save bytes in one buffer but lose the win through repeated conversion.

## Metadata

```text
date:
candidate_id:
owner:
op_family:
model:
metalpack:
status: proposed|implemented|measured|accepted|rejected
```

## Static Format

```text
target_compute_backend:
  gpu_msl_simdgroup|gpu_mps_matrix|gpu_metal4_tensor|cpu_neon|cpu_sme|coreml_ane|hybrid|unknown

hardware_quant_reason:
  <which backend operation is expected to be fastest for this quant format>

hardware_feature_evidence:
  <local feature capture/probe and external doc/source references>

source_checkpoint_dtype:
packed_storage_dtype:
runtime_input_dtype:
runtime_accumulator_dtype:
runtime_output_dtype:
state_or_cache_dtype:
quantization_time: offline|load-time|per-token|per-dispatch
dequantization_policy: none|in-dot-only|materialize-full-tensor|other
repack_policy: none|load-time|per-token|per-dispatch

layout_order:
  <row-major/tile-major/group-major/page-major description>

group_stride_bytes:
  <number/formula>

prefetch_or_speculation_contract:
  <why sequential lanes/threadgroups/CPU SME loads see the next data package>
```

## Allowed Conversions

```text
checkpoint_to_pack:
  <one-time conversion description>

pack_to_runtime:
  <zero-copy or one-time upload description>

inside_kernel:
  <must not materialize full dequant tensors; name exact lane/tile conversion>

between_kernels:
  <same dtype carried through, or exact reason for boundary conversion>
```

## Forbidden Pattern Check

```text
f32_to_f16_to_f32_hot_loop:
  yes|no

materialized_dequant_tensor:
  yes|no

per_token_requantization:
  yes|no

per_dispatch_repacking:
  yes|no
```

## Error Budget

```text
approximate_mode:
  yes|no

max_abs_error_limit:
mean_abs_error_limit:
logit_drift_required:
greedy_token_drift_required:
quality_smoke_required:
```

## Performance Gate

```text
baseline:
candidate:
tokens_or_contexts:
speedup_min:
roofline_expected_change:
cache_expected_change:
```

## Decision

```text
decision:
accept_reject_reason:
next_action:
```
