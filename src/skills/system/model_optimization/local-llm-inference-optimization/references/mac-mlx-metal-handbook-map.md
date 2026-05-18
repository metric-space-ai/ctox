# Mac / MLX / Metal Handbook Map

Use this map when the task is about Apple Silicon, MLX, Metal, MPS, Core ML,
ANE/NPU, SIMD, SME, quantization, cache forensics, or the Qwen3.5-0.8B transfer
lessons.

## Full Handbook

Read `qwen35-metal-kernel-dev-handbook.md` first when the user asks for the
complete Mac/Metal learning base. It is the full Qwen3.5 kernel-dev handbook
copied into this skill, not a summary.

Important sections in that handbook:

```text
Current Reference Status
North Star
Measurement Discipline
Correctness Gates
Cache Miss Reality
Memory Forensics
Hardware Feature Discovery Rule
Backend Shootout Rule
Quantized Candidate Rule
Layout And Tile Decision Matrix
Autotuning Method
Accepted Patterns So Far
Rejected Or Risky Patterns
llama.cpp Transfer Lessons
Prefill Strategy
Decode Strategy
CPU Runtime Strategy
ANE/NPU Strategy
SIMD Ownership Rule
Transfer Rules For 27B/35B
Exact Versus Approximate Attention Rule
Sparse/Window Attention Tuning Rule
Exact Attention Byte-Floor Rule
Delta Scan SIMD Lessons
Tool Surface Rule
Isolated Scan Forensics Rule
```

## Hardware And Backend Grid

Read `qwen35-hardware-backend-grid.md` when choosing among:

- MSL SIMDgroup kernels
- MPS/MPSMatrix sidecars
- Metal 4 tensor or matrix APIs
- CPU NEON/SME/SME2/I8MM probes
- Core ML / ANE coarse graph experiments
- quantized formats and backend-specific layouts

## Original Research Log

Read `qwen35-research-log-index.md` before opening the full log. It points into
`qwen35-research-log.md`, the original chronological Qwen3.5 optimization log.

Use the full log for forensic lookup:

- whether a candidate was already tried
- why a promising path was rejected
- exact benchmark commands and flags from the original run
- regressions caused by measurement state, thermal state, CPU/GPU sync, or
  sidecar integration
- when a result became accepted profile instead of merely "fast once"

Use `research-logbook-system.md` when adding logs for a new model.

## Operational Templates

Use these files when creating the same documentation and evidence surface for a
new model:

```text
qwen35-kernel-dev-readme.md
qwen35-benchmark-protocol.md
qwen35-cache-forensics-checklist.md
qwen35-experiment-template.md
qwen35-decision-record-template.md
qwen35-forensics-record-template.md
qwen35-autotune-record-template.md
qwen35-accepted-profile-update-template.md
qwen35-candidate-manifest-template.md
qwen35-flag-lifecycle-template.md
qwen35-measurement-record-template.md
qwen35-quant-pipeline-template.md
qwen35-accepted-profile.env
```

## Transfer Rule

For a new model, do not copy Qwen-specific shape constants blindly. Copy the
method:

```text
hardware feature discovery
  -> reference baseline
  -> model-shape ABI
  -> native Rust runtime
  -> platform kernels/sidecars
  -> byte model and roofline
  -> correctness ladder
  -> cache/forensics records
  -> autotuned accepted profile
```
