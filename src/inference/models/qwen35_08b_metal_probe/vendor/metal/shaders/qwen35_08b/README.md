# qwen35_08b Metal Shaders

Placeholder for owned Metal shader sources used by the Qwen3.5-0.8B probe.

The first planned shaders are:

```text
bench_stream_read.metal
bench_stream_write.metal
matvec_fp16_1024.metal
lm_head_argmax_fp16.metal
deltanet_step_fp16.metal
```

No shader in this directory is production support until it has a verifier and
a benchmark entry in `docs/qwen35-08b-metal-research-log.md`.
