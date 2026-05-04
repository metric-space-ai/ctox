---
name: local-llm-inference-optimization
description: Optimize local LLM inference engines for a specific model and platform. Use when CTOX must improve prefill, decode, attention, recurrent state, quantized matmul, memory layout, CPU/GPU/NPU backend use, cache/bandwidth behavior, or reference performance against llama.cpp, MLX, Core ML, TensorRT, vLLM, or another local inference baseline.
metadata:
  short-description: Build native Rust+kernel LLM engines per machine
---

# Local LLM Inference Optimization

Use this skill to build and optimize a hardware-first **local LLM inference
engine** for the machine CTOX is installed on. The deliverable is native engine
code plus platform kernel code, not only a benchmark report or framework
tuning.

This skill was distilled from the Qwen3.5-0.8B Metal/M5 work. That effort used
an owned Rust crate, Metal kernels, MPS sidecars, packers, probes, and benchmark
binaries; it eventually beat llama.cpp prefill by using hardware/backend
discovery, model specialization, exact tiled attention, recurrent state
forensics, and strict promotion gates.

## Required Deliverable Shape

Build toward this artifact shape unless the user explicitly asks for analysis
only:

```text
native runtime:
  Rust crate or Rust module owned by CTOX
  model config parser
  tokenizer / token IO bridge
  safetensors/GGUF/model artifact loader
  weight packer and manifest
  accepted-profile env/config
  preallocated runtime buffers
  command/scheduler plan
  benchmark and reference binaries

kernel/backend code:
  Metal/CUDA/Vulkan/ROCm/CPU-kernel sources as appropriate
  matrix/tensor backend sidecars where they beat custom kernels
  CPU SIMD/SME probes only when relevant
  Core ML/ANE/NPU graph probes only as coarse backend tracks

runtime contract:
  prefill path
  decode path
  GPU/accelerator-local KV or recurrent state
  GPU/accelerator-local LM-head + sampling where possible
  CPU reads compact next-token output, not hidden states or full logits
```

Frameworks such as MLX, llama.cpp, Core ML, MPSGraph, TensorRT, or vLLM are
references or backend components. They are not a substitute for the owned CTOX
runtime unless the user explicitly changes the goal.

## Non-Negotiable Output Contract

The optimization is incomplete until the repository contains both:

- native Rust host/runtime code that owns model loading, packed artifacts,
  scheduling, benchmark entrypoints, and profile selection
- platform kernel/backend code for the target machine, for example MSL/Metal,
  CUDA, Vulkan compute, HIP/ROCm, CPU SIMD/SME C/assembly, or a native sidecar
  bridge to a platform matrix/tensor API

Python, notebooks, shell scripts, MLX, llama.cpp, Core ML, PyTorch, or other
framework-only prototypes may be used as references, converters, probes, or
temporary sidecars. They do not satisfy the deliverable by themselves.

## Core Loop

Always work in this loop:

```text
model/platform discovery
  -> reference baseline
  -> correctness reference
  -> roofline and byte model
  -> operator bottleneck ranking
  -> backend/layout hypothesis
  -> env-gated candidate
  -> isolated benchmark
  -> integrated benchmark
  -> correctness gate
  -> cache/forensics record
  -> accept, reject, or keep opt-in
```

Do not skip the loop because a candidate looks obviously faster.

## CTOX Self-Build Contract

When CTOX is asked to build an optimized engine for its own installed machine,
drive the work as a local self-build pipeline:

1. Store external model-provider credentials with `ctox secret put` or
   `ctox secret intake`; use only secret handles such as
   `credentials/HUGGINGFACE_TOKEN` in logs, docs, manifests, and reports.
2. Download or locate model artifacts through a bounded tool step, then record
   artifact paths, revisions, hashes, and licenses without exposing credential
   values.
3. Generate or update the Rust-native engine crate/module for that
   model/platform pair.
4. Generate or update platform kernel/backend sources and sidecar packers.
5. Build the packer, reference runner, prefill benchmark, decode benchmark,
   operator benchmarks, and forensics tools.
6. Run hardware feature probes on the installed machine before choosing
   quantization, SIMD/matrix APIs, storage modes, or kernel layouts.
7. Autotune candidates against the local roofline and reference runtime.
8. Promote one accepted profile only when it passes correctness,
   prefill/decode, cache/byte-model, and regression gates.

The result must be reproducible from repository code plus local secret-store
handles and local model artifacts. Do not require pasted API tokens, ad hoc
environment exports, or manual notebook execution for the accepted path.

## First Response Checklist

When this skill triggers, first identify:

- model family, exact checkpoint, architecture, quantization, context length
- platform: CPU, GPU, NPU/ANE/TPU, memory size, runtime APIs, OS version
- target mode: prefill, decode, batch, streaming, long-context, multimodal
- reference: llama.cpp, MLX, Core ML, vLLM, TensorRT, custom baseline
- correctness contract: exact, approximate, quantized, sparse/windowed
- performance target: tok/s, latency, p95, energy, memory, max context
- access path for model artifacts: local path, Hugging Face repo, S3, registry,
  or another source

If any of these are unknown, discover them from local artifacts and official
hardware/runtime docs before implementing kernels.

If a model download needs an API token, use CTOX secret-management primitives.
Do not print tokens, commit tokens, or write them to normal workspace files.

## Required Artifacts

Create or reuse these artifacts in the model project:

```text
RESEARCH_LOG.md
KERNEL_DEV_HANDBOOK.md or model-specific optimization handbook
docs/kernel-dev/accepted_profile.env
docs/kernel-dev/EXPERIMENT_TEMPLATE.md
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/FORENSICS_RECORD_TEMPLATE.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/CACHE_FORENSICS_CHECKLIST.md
docs/kernel-dev/QUANT_PIPELINE_TEMPLATE.md
tools/run_accepted_profile.*
tools/reference_report.*
tools/run_decode_regression_matrix.*
tools/capture_roofline_baseline.*
src/bin/pack_weights.* or equivalent
src/bin/bench_* reference and engine benchmarks
vendor/<backend>/ or kernels/<backend>/ kernel sources
```

If the project has no equivalent, create the smallest useful version before
optimizing.

## Workflow

1. **Freeze the model shape.** Extract config, layer topology, hidden sizes,
   head counts, recurrent state shapes, KV-cache layout, vocab/LM-head shape,
   dtype policy, and special operators. Treat these as kernel ABI.
2. **Create the native engine skeleton.** Add the Rust loader, packer,
   manifest, runtime buffer model, scheduler, and kernel build/loading path
   before chasing advanced kernels. The first skeleton can be slow, but it must
   own the full dataflow.
3. **Measure the reference.** Run realistic prefill and decode lengths. Four
   generated tokens are smoke only; they are not promotion evidence.
4. **Build correctness references.** Capture logits, greedy tokens, hidden
   dumps, state/cache dumps, and CPU/operator references where possible.
5. **Capture hardware facts.** Measure local stream bandwidth, matrix
   throughput, feature flags, backend availability, and thermal sensitivity.
6. **Classify every hot operator.** For each phase, compute modeled bytes,
   expected roofline floor, p50/p95 runtime, dispatch count, scratch traffic,
   and correctness risk.
7. **Choose the backend per operator.** Use native matrix/tensor libraries for
   large dense GEMM-like work when they win; use custom kernels for recurrence,
   reductions, online softmax, sampling, and layout-specific fusions.
8. **Implement candidates behind flags.** Never replace the accepted path
   directly. Make candidate selection explicit and reproducible.
9. **Benchmark isolated and integrated.** Isolated wins identify mechanisms;
   integrated wins decide whether the runtime improved.
10. **Autotune layouts empirically.** Sweep tile sizes, group sizes, vector
   width, chunk length, row layout, storage mode, and backend choice. Cache the
   best per platform only after gates pass.
11. **Record negative results.** Rejected candidates must explain hypothesis,
    evidence, failure mode, root cause, do-not-repeat, and retry conditions.
12. **Promote only with evidence.** Accepted defaults need correctness,
    full-path performance, p95 stability, token/context sweep, and rollback
    conditions.

## Promotion Rules

Use these hard rules:

- A local kernel win is not a runtime win.
- A prefill win does not imply a decode win.
- An approximate win is not an exact accepted-profile win.
- A quantized path needs an explicit error budget and quality gate.
- A single sample is not acceptance evidence.
- A candidate that wins only when measured first is not accepted.
- A backend feature exists only when local probes show it is available and
  useful for this operator.
- Cache-miss claims require either hardware counters or a clearly labeled byte
  model plus timing evidence.
- Framework calls are allowed inside the native runtime only as explicit backend
  sidecars with measured integration cost and correctness gates.
- Model artifact API tokens must live in the CTOX secret store or equivalent
  encrypted local secret mechanism, never in logs or source files.

## Subagents

Use subagents for read-only work when it speeds up discovery:

- codebase/operator inventory
- paper or official-doc research
- candidate risk review
- reference implementation comparison
- negative-result summarization

Do not let subagents run benchmarks or performance tests. Keep all benchmark
runs serial on the main thread to preserve thermal and GPU state.

## References

Load these only when needed:

- [mac-mlx-metal-handbook-map.md](references/mac-mlx-metal-handbook-map.md):
  map for the complete Mac/MLX/Metal/MPS/Qwen3.5 handbook material now bundled
  with this skill.
- [qwen35-metal-kernel-dev-handbook.md](references/qwen35-metal-kernel-dev-handbook.md):
  full Qwen3.5 Metal Kernel Dev Handbook, copied into the skill as the detailed
  Apple Silicon learning base.
- [research-logbook-system.md](references/research-logbook-system.md):
  how to preserve original tuning logs as reusable lookup knowledge for each
  optimized model.
- [qwen35-research-log-index.md](references/qwen35-research-log-index.md):
  lookup index into the original Qwen3.5 chronological tuning log.
- [qwen35-research-log.md](references/qwen35-research-log.md):
  original Qwen3.5-0.8B Metal research log; search or index it before loading
  large sections.
- [qwen35-hardware-backend-grid.md](references/qwen35-hardware-backend-grid.md):
  full Apple Silicon hardware/backend feature grid and backend selection record.
- [method-playbook.md](references/method-playbook.md): full optimization
  procedure and artifact expectations.
- [measurement-gates.md](references/measurement-gates.md): benchmark,
  correctness, cache-forensics, and promotion gates.
- [backend-selection.md](references/backend-selection.md): CPU/GPU/NPU,
  matrix/tensor backend, custom-kernel, quantization, and sparsity selection.
- [qwen35-lessons.md](references/qwen35-lessons.md): concrete transferable
  lessons and dead ends from the Qwen3.5-0.8B Metal work.

Operational Qwen3.5 kernel-dev templates are also bundled as `qwen35-*` files
in `references/`. Use them as source templates when creating a new model's
`docs/kernel-dev/` tree.
