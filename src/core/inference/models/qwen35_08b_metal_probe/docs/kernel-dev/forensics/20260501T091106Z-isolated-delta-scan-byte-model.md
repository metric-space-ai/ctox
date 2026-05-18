# Cache Forensics: isolated-delta-scan-byte-model

Generated: 20260501T091106Z

# Cache Forensics Record Template

Use this after an experiment claims a memory, cache, layout, scratch, KV-cache,
or weight-streaming effect. The record must distinguish inferred byte-model
evidence from hardware-counter-backed evidence.

## Metadata

```text
date: 20260501T091106Z
experiment: docs/kernel-dev/experiments/20260501T090838Z-delta-scan-isolated-sweep.md
op: deltanet_scan
kernel: qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128
selection_env: plain; rowcache; rowcache_direct; rowcache_block64; rowcache_block32; rowcache_block_auto; lanes4_sharedqk_approx
model: Qwen3.5-0.8B
metalpack: /tmp/ctox_qwen35_08b_real_fp16.metalpack
tokens/context: 512,4096,16384
evidence_level: inferred-only
counter_source: n/a
```

## Runtime

```text
command: tools/run_delta_scan_isolated_sweep.sh --tokens 512,4096,16384 --rounds 2 --iterations 3 --warmup 2 --validate-tokens 8 --output-dir /tmp/ctox_qwen35_scan_isolated_20260501T_continue
median_s: 0.0544857
p95_s: 0.056241792
effective_GB/s: 6.235
tok_s: 300702
```

## Byte Model

```text
unique_weight_bytes: 0
weight_group_stream_bytes: 0
logical_operand_weight_bytes: 51877249024
modeled_dram_miss_bytes: 339738624
modeled_cache_hit_bytes: 51537510400
non_weight_bytes: 339738624
scratch_write_bytes: 0
scratch_read_bytes: 0
persistent_state_bytes: 1048576
tail_underfill: 0
```

## Interpretation

```text
compulsory_miss_floor: q/k/v, beta/decay, output, and one persistent-state read/write are compulsory for exact rowcache scan
avoidable_miss_suspect: repeated tokenwise state streaming is avoided by rowcache; remaining gap is likely register pressure/occupancy, not compulsory DRAM traffic
occupancy_suspect: row_state[128] per worker is the dominant pressure; block32 keeps the best measured balance
scratch_suspect: none in isolated scan
cpu_overhead_suspect: low; each sample is one dispatch and one command-buffer wait
```

## Gap Analysis

```text
bandwidth_utilization: 6.235 GB/s against the reduced rowcache byte model; not directly comparable to the plain kernel's repeated-state GB/s
reported_effective_vs_modeled: rowcache_block32 p16384 uses 339738624 modeled bytes versus 51877249024 logical plain state-stream bytes
byte_model_mismatch_suspect: yes, if interpreted as raw DRAM bandwidth; no, if interpreted as lower-bound compulsory traffic
roofline_floor_s: 0.00246
time_vs_floor: 22.15
actual_dram_bytes: unknown without hardware counters
traffic_vs_model: needs Metal counter capture
bandwidth_gap_suspect: yes
floor_gap_suspect: yes
classification: inferred rowcache reuse win, remaining time dominated by recurrence/register/occupancy rather than simple DRAM streaming
next_probe: capture Metal counters for accepted rowcache_block32 and approximate lanes4_sharedqk at p16384, then test a structural exact scan-state layout rather than another rowgroup threshold
```

## Conclusion

```text
decision: needs-more-data
next_action: keep rowcache_block32 accepted, keep lanes4_sharedqk opt-in approximate, and use isolated sweep plus full-stack hidden/logit gates before any scan promotion
```
