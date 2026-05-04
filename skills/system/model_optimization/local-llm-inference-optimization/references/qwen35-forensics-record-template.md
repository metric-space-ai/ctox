# Cache Forensics Record Template

Use this after an experiment claims a memory, cache, layout, scratch, KV-cache,
or weight-streaming effect. The record must distinguish inferred byte-model
evidence from hardware-counter-backed evidence.

## Metadata

```text
date:
experiment:
op:
kernel:
selection_env:
model:
metalpack:
tokens/context:
evidence_level: <inferred-only | hardware-counter-backed>
counter_source:
```

## Runtime

```text
command:
median_s:
p95_s:
effective_GB/s:
tok_s:
```

## Byte Model

```text
unique_weight_bytes:
weight_group_stream_bytes:
logical_operand_weight_bytes:
modeled_dram_miss_bytes:
modeled_cache_hit_bytes:
non_weight_bytes:
scratch_write_bytes:
scratch_read_bytes:
persistent_state_bytes:
tail_underfill:
```

## Interpretation

```text
compulsory_miss_floor:
avoidable_miss_suspect:
occupancy_suspect:
scratch_suspect:
cpu_overhead_suspect:
```

## Gap Analysis

```text
bandwidth_utilization:
reported_effective_vs_modeled:
byte_model_mismatch_suspect:
roofline_floor_s:
time_vs_floor:
actual_dram_bytes:
traffic_vs_model:
bandwidth_gap_suspect:
floor_gap_suspect:
classification:
next_probe:
```

## Conclusion

```text
decision:
  accepted | rejected | opt-in | needs-more-data
next_action:
```
