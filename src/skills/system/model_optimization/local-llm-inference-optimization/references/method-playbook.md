# Method Playbook

Use this reference when starting or restructuring an LLM inference optimization
project for a new model/platform pair.

## 1. Freeze Scope

Record these facts before kernel work:

```text
model:
  checkpoint, revision, config hash, tokenizer, vocab, tied/untied LM head

architecture:
  layer count, layer order, hidden size, MLP shape, attention heads, KV heads,
  head_dim, recurrent/stateful layers, MoE, vision/audio towers

runtime modes:
  prefill, decode, batch size, streaming, max context, multimodal stages

platform:
  CPU ISA, GPU generation, NPU/ANE availability, memory size/bandwidth,
  runtime APIs, OS/driver/compiler versions

semantics:
  exact, approximate, sparse/windowed, quantized, accepted error budget
```

If the model has unusual layers, optimize around them. For Qwen3.5-0.8B the
central special case was DeltaNet recurrence, not generic Transformer attention.

## 1a. Build The Owned Native Engine

The target artifact is native runtime code plus kernel/backend code. Do not
stop at a notebook, script, or framework-only benchmark.

Minimum Rust-native engine skeleton:

```text
crate/module:
  Cargo target or equivalent native package

model ingestion:
  config parser
  tokenizer bridge or raw-token bench path
  safetensors/GGUF/artifact reader
  API-token-backed downloader only through secret store

packer:
  deterministic packed weights
  manifest with tensor names, classes, offsets, dtypes, layouts, hashes
  backend-native sidecar packs when needed

runtime:
  device/context initialization
  pipeline/kernel cache
  accepted-profile config
  preallocated buffers
  command graph or scheduler
  prefill entry point
  decode entry point
  compact next-token output

kernels:
  platform kernel sources
  matrix/tensor backend bridge if faster
  CPU reference kernels for correctness

tools:
  pack_weights
  audit_shapes
  reference runner
  prefill benchmark
  decode benchmark
  operator microbenchmarks
  forensics/profile tools
```

For Apple/Metal this means Rust + MSL/MPS/CoreML probes. For CUDA this means
Rust or C++ host code plus CUDA kernels/cuBLAS/TensorRT probes. For CPU-only it
means Rust plus SIMD/AMX/SME/oneDNN or vendor kernels.

Do not accept a pure Python, pure shell, or pure framework integration as the
final product. Those paths are valid only as references, converters, probes, or
backend sidecars embedded behind the Rust-owned runtime boundary.

## 2. Build the Baseline Grid

Create a reference table:

```text
prefill:
  prompt sizes: 512, 4096, 16384, 32768, model-specific max target

decode:
  output lengths: 128, 512, and the product's realistic generation length

metrics:
  median_s, p95_s, tok/s, memory, energy if available, correctness state

references:
  llama.cpp / MLX / Core ML / vLLM / TensorRT / vendor runtime
```

Never claim performance from four decode tokens. Short runs prove execution and
maybe parity; they do not prove throughput.

## 3. Build the Correctness Ladder

Use progressively stronger gates:

```text
checksum smoke
operator CPU reference
hidden-state dump
logits comparison
greedy token parity
state/cache parity over long context
task-quality gate for approximate or quantized paths
```

Keep exact and approximate profiles separate. Approximate paths can be
valuable, but they need explicit quality/error budgets.

## 4. Build the Roofline

Measure local ceilings:

```text
sustained stream bandwidth
large matrix/tensor throughput
small matvec/GEMV throughput
reduction/softmax throughput
CPU quant/SIMD throughput if CPU is considered
NPU/ANE placement if a graph runtime can expose it
```

Then classify each operator:

```text
modeled bytes
modeled FLOPs
arithmetic intensity
byte floor
time_vs_floor
bandwidth utilization
dispatch count
scratch traffic
tail underfill
```

When a kernel is already near its byte floor, do not spend time on cache-miss
cleanup. Reduce algorithmic bytes, change backend, fuse passes, quantize, or
change the schedule.

## 5. Rank the Bottlenecks

Profile by phase:

```text
embedding
input norm
QKV / recurrent projections
attention or recurrent mixer
output projection
MLP gate/up/down
final norm
LM head
sampling
CPU orchestration
```

The biggest phase is not always the next target. Pick the phase whose gap to
its roofline or reference is both large and actionable.

## 6. Choose Candidate Families

Use this order:

1. Remove CPU roundtrips and full-logit readbacks.
2. Make the native engine run the whole model path, even if slow.
3. Fix data layout and packing.
4. Use platform matrix/tensor APIs for dense GEMM-like phases.
5. Fuse memory-bound adjacent ops when it removes real traffic.
6. Add custom kernels for recurrence, reductions, online softmax, sampling, and
   layout-specific glue.
7. Autotune tile/chunk/layout parameters.
8. Quantize only after the exact path and quality gates exist.
9. Add sparse/windowed approximations only with explicit quality gates.

## 7. Run the Promotion Loop

For every candidate:

```text
create experiment record
state hypothesis and changed layout/backend/math
implement behind a flag
run isolated benchmark
run integrated benchmark
run correctness gates
run cache/forensics model
compare against reference and accepted profile
record accept/reject/opt-in
update handbook if the result changes strategy
```

Promotion requires:

```text
correctness gate pass
median and p95 win across relevant sizes
no hidden storage/sync/thermal regression
reference comparison
rollback trigger
accepted-profile update record
```

## 8. Transfer to Larger Models

Before applying a small-model lesson to 27B/35B:

```text
recompute memory capacity and bandwidth floors
re-evaluate LM-head/vocab traffic
re-evaluate KV-cache or recurrent-state footprint
re-run hardware backend shootout
repeat autotuning for token tiles and quant group size
separate exact and approximate quality gates again
```

A schedule that wins on the small model may fail when weights, KV cache,
dispatch count, or memory pressure scale.
