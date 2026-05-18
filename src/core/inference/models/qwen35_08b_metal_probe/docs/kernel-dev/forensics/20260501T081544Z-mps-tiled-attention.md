# Cache Forensics: MPS Tiled Attention

## Metadata

```text
date: 20260501T081544Z
experiment: docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md
op: prefill full-attention core
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
evidence_level: inferred-only
counter_source: macOS GPUTimestamp only; no exposed L2/cache-miss counter
```

## Runtime

```text
kernel: qwen35_08b MPS tiled QK/PV + MSL SIMD32 softmax/combine
selection_env: CTOX_QWEN35_ATTENTION_MPS_TILED=1
tokens/context: 32768
command: CTOX_QWEN35_ATTENTION_MPS_TILED=1 target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 32768 2 150 /tmp/ctox_qwen35_mps_ffn_sidecar /tmp/ctox_qwen35_mps_delta_project_sidecar /tmp/ctox_qwen35_mps_attention_out_sidecar /tmp/ctox_qwen35_mps_delta_out_sidecar
median_s: 0.968732
p95_s: 0.968732
effective_GB/s: 1158.52
```

## Byte Model

```text
unique_weight_bytes: 14680064
weight_group_stream_bytes: 21474836480
logical_operand_weight_bytes: 481036337152
modeled_dram_miss_bytes: 145309671424
modeled_cache_hit_bytes: 1096040779776
non_weight_bytes: 1100809999974
scratch_write_bytes: 1099511627776
scratch_read_bytes: 1099511627776
persistent_state_bytes: 1492501135
tail_underfill: 0.0
```

## Interpretation

```text
compulsory_miss_floor: K/V and tile scratch dominate logical operands at p32768; real DRAM traffic is inferred below logical bytes because MPSMatrix/cache behavior is opaque
avoidable_miss_suspect: old QH4 per-query scan revisited K/V in a custom loop; MPS tiled path removes most of that avoidable execution cost
occupancy_suspect: low for QK/PV matrix backend; remaining MSL softmax/combine overhead is small relative to old scan
scratch_suspect: score/prob/pv/out tile scratch exists but is offset by matrix backend throughput
cpu_overhead_suspect: MPS encode overhead is acceptable at long contexts; small p512 speedup is lower because encode overhead is less amortized
decision: accepted
next_action: promote MPS tiled attention and shift optimization focus to Delta18 scan/gated-norm/out orchestration
```
