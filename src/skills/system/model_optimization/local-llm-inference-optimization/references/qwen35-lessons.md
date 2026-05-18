# Qwen3.5-0.8B Transfer Lessons

Use this reference when applying the Qwen3.5 Metal/M5 experience to another
model or platform.

## What Worked

Native Rust + kernel-code structure worked:

```text
Rust crate:
  model artifact inspection
  deterministic metalpack writer
  manifest/audit tools
  runtime buffer and scheduler code
  benchmark binaries
  MPS sidecar bridge

Kernel/backend code:
  MSL kernels for recurrence, reductions, attention glue, residuals, sampling
  MPSMatrix sidecars for large dense matrix phases
  Swift/C/CPU probes for backend feasibility
```

The winning pattern was not "one mega shader". It was an owned native runtime
with model-specific kernels, backend sidecars, and strict evidence flow.

## Key Wins

1. **Model specialization.** Hardcode real shapes and layer topology instead of
   writing generic kernels first.
2. **GPU-local decode contract.** Keep KV/recurrent state and LM-head/sampling
   on accelerator; CPU reads compact next-token output.
3. **Sidecars for dense matmul.** MPSMatrix sidecars beat continued hand tuning
   for FFN, Delta project, Attention O, and DeltaOut shapes.
4. **Exact tiled attention.** Long prefill needed a different algorithmic
   schedule, not cache-miss polishing of an already byte-floor scan.
5. **Rowcache for recurrence.** Keeping recurrent state local avoided repeated
   state DRAM streaming, but did not eliminate register/occupancy bottlenecks.
6. **Autotune with correctness gates.** Fastest observed candidate and accepted
   candidate are different fields.
7. **Reference reports.** Curated prefill/decode comparison tools prevented
   stale hand comparisons.
8. **Negative-result documentation.** Recording rejected candidates avoided
   repeating plausible but wrong layouts.

## Current Reference Outcome From Qwen3.5

Exact prefill forensics beat llama.cpp:

```text
p4096:  4801.88 tok/s vs 2852.70 = 1.683x
p16384: 4096.00 tok/s vs 2065.71 = 1.983x
p32768: 3383.73 tok/s vs 1325.20 = 2.553x
```

Approximate Delta scan was faster again but stayed opt-in due to hidden-state
drift.

Decode could beat reference on cooled tg128/tg512 runs, but sustained decode
needed stricter alternating regression matrices because thermal/order/storage
state affected long runs.

## Dead Ends

Do not repeat these blindly:

```text
tiny decode runs:
  4-token wins misled promotion decisions

generic cache-miss talk:
  compulsory misses must be modeled; "no misses" is not a real contract

rowgroup thresholding:
  block32/block64 scan variants were too close; structural change was needed

SIMD as magic:
  SIMD32 LM-head and gated norm rewrites lost despite cleaner reductions

scalar int8 dequant:
  saved bytes but lost to unpack/dequant overhead

per-layer NPU/GPU ping-pong:
  ANE/Core ML is coarse graph/backend track, not a custom Metal shader partner

framework-only path:
  MLX/CoreML/llama.cpp were references or sidecars, not the owned engine
```

## Transfer Rules

For any new model:

```text
discover architecture first:
  unusual recurrent, MoE, sliding-window, grouped-query, rope, quant, or
  multimodal paths determine the optimization plan

separate modes:
  prefill and decode have different bottlenecks and acceptance gates

select backend per op:
  dense matrix phases may belong to tensor/matrix libraries
  recurrence/reductions/sampling often need custom kernels

rebuild packer/layout:
  bigger models change weight stream, KV footprint, LM-head cost, and tile
  sweet spots

keep exact/approx separate:
  sparse/window/quant wins require quality gates

record every failure:
  the search space is too large to rely on memory
```

## API Token And Artifact Handling

When the model comes from Hugging Face, S3, a vendor registry, or a private
artifact store:

```text
store token in encrypted CTOX secret store
retrieve only for the bounded download step
record artifact hashes, not token values
never write token into logs, env dumps, manifests, or docs
```

The optimized engine should be reproducible from:

```text
model source reference
artifact hash
packer version
runtime commit
accepted profile
hardware/backend evidence
```
