# Cache And Memory Forensics Checklist

Use this whenever an optimization claims to improve cache behavior, memory
traffic, layout, tiling, scratch, KV-cache reads, or weight streaming.

## First Principle

Literal zero cache misses is not a valid requirement for streaming weights.
The requirement is:

```text
only compulsory misses
no avoidable re-reads
no accidental scratch spill traffic
no layout/tile underfill that dominates useful work
no CPU/GPU transfer in the hot path
```

Hardware limits are part of the experiment, not background knowledge. Before a
cache or memory optimization can be trusted, capture or reference a local
roofline:

```text
tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline_<date>
```

Then report every hot op against that roofline:

```text
bandwidth_utilization
time_vs_floor
reported_effective_vs_modeled
traffic_vs_model, if hardware counter bytes exist
classification
next_probe
```

This is what turns the tools into a discovery system. The goal is to identify
whether the next optimization should attack algorithmic bytes, memory layout,
cache residency, scratch traffic, occupancy, dispatch overhead, or compute
math.

## Counter Status

Current local programmatic Metal counter access exposes `GPUTimestamp` only.
Therefore most cache rows are inferred from byte models and timing.

Every report must label cache evidence as:

```text
inferred-only
hardware-counter-backed
```

Use `hardware-counter-backed` only if the specific counter capture is named.

## Required Byte Buckets

For each hot op:

```text
unique_weight_bytes:
weight_group_stream_bytes:
logical_operand_weight_bytes:
modeled_dram_miss_bytes:
modeled_cache_hit_bytes:
reuse_opportunity:
non_weight_bytes:
scratch_write_bytes:
scratch_read_bytes:
persistent_state_bytes:
tail_underfill:
```

## Questions To Answer

```text
1. Does the working set fit the modeled cache?
2. If it fits, why might runtime still imply streaming?
3. If it streams, is that compulsory or caused by layout?
4. Does a larger tile reduce weight traffic but lower occupancy?
5. Does a smaller tile increase weight traffic but improve parallelism?
6. Are scratch writes/reads larger than the traffic saved?
7. Are we dispatching inactive blocks?
8. Are we re-reading KV for heads that share a KV head?
9. Are residuals or norms causing avoidable activation roundtrips?
10. Does the result survive a token-length sweep?
11. What fraction of measured stream and operational matmul roofline is used?
12. Did a layout sweep prove the current row/tile/chunk choice empirically?
```

## Tools

```text
target/release/cache_analysis --tokens N --decode-position N --csv
target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack N ITERS SUSTAINED_GB_S
target/release/profile_metalpack_prefill_delta_stack /tmp/ctox_qwen35_08b_real_fp16.metalpack N ITERS WARMUP LAYERS START
target/release/list_metal_counters
tools/capture_metal_trace.sh
tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline
tools/analyze_bandwidth_gap.sh --normalized normalized.txt --modeled-bytes BYTES --sustained-gb-s N
tools/analyze_memory_forensics_gaps.sh memory_forensics_stdout.txt --markdown
tools/analyze_delta_profile_gaps.sh delta_profile_stdout.txt cache_analysis.csv --markdown
```

## Forensics Report Template

```text
op:
kernel:
selection/env:
tokens/context:

runtime:
  median_s:
  p95_s:
  effective_GB/s:

byte_model:
  unique_weight_bytes:
  weight_group_stream_bytes:
  logical_operand_weight_bytes:
  non_weight_bytes:
  scratch_bytes:
  modeled_dram_miss_bytes:
  reuse_opportunity:
  tail_underfill:

interpretation:
  compulsory_miss_floor:
  avoidable_miss_suspect:
  occupancy_suspect:
  scratch_suspect:
  CPU_overhead_suspect:

gap_analysis:
  bandwidth_utilization:
  reported_effective_vs_modeled:
  byte_model_mismatch_suspect:
  time_vs_floor:
  traffic_vs_model:
  classification:
  next_probe:

decision:
  accepted | rejected | opt-in | needs-more-data
```

## Red Flags

```text
claimed cache hit/miss without counter source
effective_GB/s computed from incomplete byte model
hidden O(T^2) attention traffic omitted
partial_acc scratch omitted
candidate wins only at tiny token count
candidate changes checksum or hidden dump
model_bytes lower but runtime worse and no occupancy explanation
```
