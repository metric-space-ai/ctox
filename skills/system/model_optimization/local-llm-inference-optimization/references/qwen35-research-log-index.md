# Qwen3.5 Research Log Index

Use this index before loading the full `qwen35-research-log.md`.

The full log is the original chronological development record for the
Qwen3.5-0.8B Metal/Mac optimization. It is intentionally verbose and includes
smoke tests, failed candidates, regressions, benchmark commands, promotion
records, and tool-building steps.

## Core Setup

```text
line 9:    Strategy
line 27:   Fixed Shape Contract
line 51:   Gates
line 71:   2026-04-29 first Metal smoke and early decode skeleton
line 2036: Metal bandwidth and FP16 matvec baselines
line 2079: real Qwen3.5 artifact pack and layered decode
```

## Decode Construction

```text
line 698:  Metalpack attention operator slice
line 777:  Metalpack DeltaNet operator slice
line 862:  D/D/D/A superblock
line 957:  full 24-layer pattern scheduler
line 1007: layer-specific 24-layer binding API
line 1263: attention KV-cache surface
line 1331: multi-step attention KV-cache smoke
line 1374: full 24-layer multi-step state/cache smoke
line 1907: compact next-token output
line 2464: greedy parity restored and MLX baseline beaten
line 2577: FFN gate/up/SwiGLU dispatch fusion
line 2636: DeltaNet qkv/z/b/a dispatch fusion
line 2700: attention q/k/v dispatch fusion
line 2763: projection residual writeback fusion
line 2830: LM-head rowtile argmax fusion
line 15985: decode regression lesson
line 16136: decode regression follow-up
```

## Prefill Construction

```text
line 3305: first real CTOX prefill-state-build path
line 3378: sync/storage experiments and first batched prefill kernel
line 3475: batched prefill projection on real weights
line 3532: batched FFN gate/up prefill block
line 3606: batched FFN down projection
line 3672: GPU-local batched FFN prefill block
line 3727: batched DeltaNet projection block
line 3897: batched DeltaNet recurrent state scan
line 3975: batched DeltaNet gated norm/out/full block chain
line 4749: first GPU-local DeltaNet+FFN prefill layer-pair
line 5151: 18 DeltaNet+FFN layer stack
line 5419: attention FFN and QKV projection prefill
line 5507: first real attention prefill core
line 16230: prefill attention backend matrix
line 16355: real MPS tiled attention core integration
line 16517: full-prefill forensics with MPS tiled attention
line 16594: MPS tiled attention promoted
```

## Memory, Cache, And Forensics

```text
line 4153: cache/miss analysis layer
line 4430: DeltaNet out-proj tok4 SIMD path and cache miss rule
line 4596: Metal counter availability on this Mac
line 5578: memory forensics tooling
line 6115: DOWN_MMA16 retest and cache-inference tooling
line 6759: cache forensics byte buckets
line 7233: ideal-reuse cache forensics
line 8773: decode split-K scratch forensics
line 10010: cache forensics records
line 10837: roofline-first tooling rule
line 16972: isolated Delta scan sweep and corrected byte model
```

## SIMD, SME, Quantization, Hardware Backend

```text
line 4068: SIMDgroup prefill RMS projection
line 4263: FFN gate/up tok4 SIMD path
line 4351: FFN down tok4 SIMD path
line 5922: SIMDgroup attention reduction
line 8791: NPU/ANE math and quantization check
line 12528: hardware-first correction and M5 feature gate
line 12642: MPS Matrix backend probe
line 12727: quantized candidate error gate
line 12781: backend shootout and quant Delta gate tools
line 12881: static quantization pipeline rule
line 12927: static quantized metalpack layouts
line 12989: static int8 matmul probe
line 13119: hardware backend grid and p4096 matrix shootout
line 15178: hardware backend shootout, SME2 and ANE status
line 15383: SME2 I8 tile stream probe
line 15478: static INT8 Metal matmul autotune
line 15551: static INT8 SIMD32 matmul variant
```

## External Research / Transfer

```text
line 8810: why llama.cpp was faster than current CTOX probe
line 11772: Luce prefill lessons and QKV/Z RG4 A-shared rejection
line 11869: OpenEvolve tuning lessons
line 11920: native sparse attention applicability
line 11997: long-context attention optimization taxonomy
line 12062: llama.cpp prefill strategy transfer
line 12157: OpenEvolve kernel discovery lessons
```

## Tooling And Process

```text
line 8959: superblock prefix profiler and first autotuner
line 9063: autotuner correctness gate and CSV forensics
line 9232: kernel dev handbook knowledge base
line 9283: operational kernel dev templates
line 9433: kernel dev doctor
line 9467: accepted profile source of truth
line 9574: standard measurement pack runner
line 9748: decision record tooling
line 9949: accepted-profile promotion gate
line 10123: autotune evidence records
line 10298: evidence bundle inspector
line 10362: accepted profile proposal gate
line 10427: accepted profile validator
line 10623: captured measurement runs
line 10729: measurement records
line 10914: negative learning must be preserved
line 17086: documentation audit after beating reference prefill
```

## Search Patterns

```text
rg -n "accepted|promoted|rejected|regression" qwen35-research-log.md
rg -n "llama.cpp|MLX|MPS|Metal|SIMD|SME|ANE|Core ML" qwen35-research-log.md
rg -n "prefill|decode|attention|DeltaNet|FFN|LM-head" qwen35-research-log.md
rg -n "cache|forensics|bandwidth|byte model|roofline" qwen35-research-log.md
```
