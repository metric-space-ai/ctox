# Qwen3.5 Metal Kernel Dev Handbook

This handbook condenses the working knowledge from `RESEARCH_LOG.md` into a
repeatable engineering playbook. It is meant as a lookup document for the
Qwen3.5-0.8B probe first, then as the source of lessons for larger 27B/35B
work.

## Quick Index

Use this document by question:

```text
What are we building?               -> North Star
What shape are kernels allowed for?  -> Fixed Shape Contract
What is a valid optimization?        -> What Counts As Progress
How do we measure?                   -> Measurement Discipline
How do we prevent fake wins?         -> Correctness Gates
How do we reason about cache misses? -> Cache Miss Reality / Memory Forensics
How do we tune layouts?              -> Autotuning Method
What has worked?                     -> Accepted Patterns So Far
What failed?                         -> Rejected Or Risky Patterns
Where are we vs llama.cpp?           -> Current Reference Status
What did llama.cpp teach us?          -> llama.cpp Transfer Lessons
What should be done next?            -> Planning Loop
What template should I use?          -> docs/kernel-dev/
```

Operational templates live in `docs/kernel-dev/`:

```text
docs/kernel-dev/EXPERIMENT_TEMPLATE.md
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/FORENSICS_RECORD_TEMPLATE.md
docs/kernel-dev/AUTOTUNE_RECORD_TEMPLATE.md
docs/kernel-dev/ACCEPTED_PROFILE_UPDATE_TEMPLATE.md
docs/kernel-dev/MEASUREMENT_RECORD_TEMPLATE.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/CACHE_FORENSICS_CHECKLIST.md
docs/kernel-dev/FLAG_LIFECYCLE_TEMPLATE.md
docs/kernel-dev/accepted_profile.env
tools/new_kernel_experiment.sh
tools/validate_kernel_experiment.sh
tools/kernel_dev_doctor.sh
tools/run_accepted_profile.sh
tools/validate_accepted_profile.sh
tools/run_measurement_pack.sh
tools/capture_roofline_baseline.sh
tools/capture_measurement_output.sh
tools/new_measurement_record.sh
tools/validate_measurement_record.sh
tools/list_measurement_records.sh
tools/update_measurement_index.sh
tools/list_kernel_experiments.sh
tools/update_kernel_experiment_index.sh
tools/new_kernel_decision.sh
tools/validate_kernel_decision.sh
tools/check_kernel_promotion.sh
tools/new_cache_forensics_record.sh
tools/validate_cache_forensics.sh
tools/fill_forensics_record_from_output.sh
tools/analyze_bandwidth_gap.sh
tools/analyze_memory_forensics_gaps.sh
tools/analyze_delta_profile_gaps.sh
tools/list_cache_forensics.sh
tools/update_cache_forensics_index.sh
tools/new_autotune_record.sh
tools/validate_autotune_record.sh
tools/list_autotune_records.sh
tools/update_autotune_index.sh
tools/normalize_benchmark_output.sh
tools/fill_autotune_record_from_output.sh
tools/show_kernel_evidence_bundle.sh
tools/propose_accepted_profile_update.sh
tools/validate_accepted_profile_update.sh
tools/list_accepted_profile_updates.sh
tools/update_accepted_profile_update_index.sh
tools/list_kernel_decisions.sh
tools/update_kernel_decision_index.sh
```

## Current Accepted Defaults

The accepted profile is intentionally conservative. It is the last known
configuration that balances speed and correctness without hidden-state drift:

The source of truth is `docs/kernel-dev/accepted_profile.env`.

Do not replace these defaults with a faster autotune candidate unless the
candidate becomes `accepted_selection` after the correctness gate and a token
sweep.

Use:

```text
tools/run_accepted_profile.sh <command> [args...]
```

when a benchmark must run against the conservative baseline.

## Current Reference Status

As of the latest documented measurements, the Qwen3.5-0.8B probe has crossed
the main reference milestone for **prefill** and has a plausible but less
stable **decode** lead.

Exact prefill forensics versus llama.cpp BF16/Metal:

```text
p4096:
  CTOX exact MPS tiled forensics: 4801.88 tok/s
  llama.cpp reference:            2852.70 tok/s
  ratio:                          1.683x

p16384:
  CTOX exact MPS tiled forensics: 4096.00 tok/s
  llama.cpp reference:            2065.71 tok/s
  ratio:                          1.983x

p32768:
  CTOX exact MPS tiled forensics: 3383.73 tok/s
  llama.cpp reference:            1325.20 tok/s
  ratio:                          2.553x
```

Approximate prefill fast control:

```text
CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1

p4096:  5305.70 tok/s = 1.860x llama.cpp
p16384: 4396.03 tok/s = 2.128x llama.cpp
p32768: 3594.16 tok/s = 2.712x llama.cpp
```

This approximate row is **not** accepted-profile evidence. It is useful because
it proves the SIMD/cache shape has real speed potential, but hidden-state drift
keeps it behind an explicit approximation/quality gate.

Decode status:

```text
cooled clean tg128:
  CTOX accepted: 55.91 tok/s
  llama.cpp:     52.98 tok/s
  ratio:         1.055x

cooled clean tg512:
  CTOX accepted: 55.66 tok/s
  llama.cpp:     44.77 tok/s
  ratio:         1.243x
```

Decode is not as mature as prefill. Earlier long serial matrices showed
thermal/measurement-state regressions below reference. The rule is therefore:

```text
prefill:
  reference beaten in the current exact forensics row

decode:
  can beat reference, but promotion still requires serial alternating tg128/tg512
  matrices with hardware state, storage mode, and sync policy recorded
```

Current source tools:

```text
tools/prefill_reference_report.py
tools/run_decode_regression_matrix.sh --iterations 3 --rounds 2 <metalpack>
```

## North Star

The product target is not "one impressive shader". It is a measured GPU-local
inference pipeline:

```text
CPU:
  load/pack weights
  encode command buffers
  write the input token
  read one next_token

Metal GPU:
  embedding
  DeltaNet / Attention / FFN layers
  KV cache and DeltaNet recurrent state
  final norm
  LM-head argmax/sampling

ANE/NPU:
  separate Core ML experiments only
  no per-layer Metal <-> Core ML ping-pong in decode
```

The rule for decode remains: one CPU synchronization per generated token, not
one synchronization per layer or operator.

## Fixed Shape Contract

Do not write generic kernels first. Qwen3.5-0.8B is deliberately fixed:

```text
hidden size:            1024
vocab / embedding:      248,320
layers:                 24
layout:                 [D, D, D, A] x 6
DeltaNet layers:        18
Attention layers:       6
FFN intermediate:       3584
Attention:              8 Q heads, 2 KV heads, head_dim 256
DeltaNet:               16 QK/V heads, head_dim 128
```

Treat these constants as part of the kernel ABI. If a shape changes, the
packer, manifests, dispatches, benchmarks, and cache model must all change
together.

## What Counts As Progress

A kernel change is not progress unless it passes this loop:

```text
hypothesis
  -> isolated kernel or runtime patch
  -> correctness gate
  -> integrated full-path benchmark
  -> byte/cache forensics
  -> compare against baseline/reference
  -> log decision: accept, reject, or keep opt-in
```

The log must record failures. Rejected experiments are valuable because they
prevent repeating plausible but wrong ideas.

## Knowledge Capture Rule

Every kernel-learning must be written down, including dead ends. The research
log is not only a success journal; it is the memory of the optimization search.
A failed idea is valuable when it explains which assumption was wrong.

Record all of these:

```text
accepted wins
rejected candidates
opt-in candidates
failed correctness gates
failed pipeline creation / register-pressure cases
byte-model mismatches
roofline gaps that contradict intuition
layout/tile/chunk sweeps, including losers
invalid benchmark conditions
reference-implementation differences
paper/research ideas that did not transfer to Metal/Qwen3.5
```

Every negative result should answer:

```text
hypothesis:
  what did we expect?

evidence:
  command, env, tokens, iterations, roofline, measurement/forensics/autotune record

failure_mode:
  slower runtime | p95 instability | checksum drift | hidden/logit mismatch |
  pipeline compile failure | bandwidth underuse | scratch explosion |
  occupancy/register pressure | CPU orchestration overhead

root_cause:
  measured, inferred, or unknown

do_not_repeat:
  what exact pattern should future work avoid?

retry_only_if:
  what would need to change before the idea is worth trying again?
```

If a candidate is rejected without this information, the work is incomplete.
If the rejection changes general strategy, update this handbook as well as
`RESEARCH_LOG.md`.

## Measurement Discipline

Benchmarks must be serial when comparing performance. Do not let subagents run
benchmarks in parallel; they can inspect code, propose changes, or read papers.

Every hardware target has finite compute throughput and memory bandwidth. A
kernel is not optimized in the abstract; it is optimized relative to the
measured limits of the current Mac, OS, Metal runtime, thermal state, model
shape, and data layout. Therefore every serious optimization phase must start
from a local roofline baseline:

```text
stream bandwidth roof:       sustained_stream_GB_s
prefill matmul proxy roof:   operational_prefill_matmul_GB_s
decode/matvec proxy roof:    operational_matvec_GB_s
compute proxy roof:          add when a stable MMA/FMA-only probe exists
```

Use:

```text
tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline_<date>
```

The resulting `roofline.env` is the input for gap analysis. Do not judge a
kernel by raw milliseconds alone. For each hot operator classify:

```text
bandwidth_utilization = effective_GB/s / sustained_stream_GB_s
time_vs_floor         = median_s / modeled_byte_floor_s
traffic_vs_model      = actual_counter_bytes / modeled_bytes, if available
layout_sensitivity    = result of token_tile / row_tile / chunk sweeps
```

This is the discovery loop that finds real flaws. If a kernel is far below the
memory roof while its byte model is correct, investigate occupancy, tail
underfill, dispatch fragmentation, scratch traffic, or layout. If it is near
the byte floor but still slower than reference, the next win must reduce
algorithmic bytes: bigger token reuse, fewer passes, fusion, quantization,
FlashAttention-style tiling, or a different DeltaNet scan algorithm.

Use median and p95. Single samples are only smoke tests. For acceptance:

```text
warmup:      at least 1
iterations:  at least 3, preferably 7 for close calls
tokens:      sweep more than one length
```

Small decode tests are not a performance claim. Four tokens only prove that a
path runs and maybe preserves greedy parity. Real prefill/decode claims need
longer contexts and realistic output lengths.

## Correctness Gates

Performance numbers are meaningless if the math changed.

Use progressively stronger gates:

```text
1. checksum smoke
2. full hidden-state dump comparison
3. logits comparison
4. greedy token-stream parity
5. long-context state/cache parity
```

The autotuner now separates:

```text
best_selection:
  fastest observed candidate

accepted_selection:
  fastest candidate only if the hidden-dump correctness gate passes;
  otherwise conservative baseline
```

Default hidden-dump gate thresholds:

```text
mean_abs_error       <= 0.0005
rms_error            <= 0.0010
max_abs_error        <= 0.0100
abs(checksum_delta)  <= 1.0
```

A 4096-token candidate that was about 7.7% faster failed this gate:

```text
mean_abs_error   0.001899509
rms_error        0.002485653
max_abs_error    0.062500000
checksum_delta  -16.460019886
```

Decision: faster but not acceptable as default.

## Cache Miss Reality

"No cache misses" is the right instinct but the wrong literal contract.

For streaming weights, compulsory misses are unavoidable. A full LM head or
large projection must read weights from memory at least once unless those
weights are already resident. The real engineering target is:

```text
compulsory misses:      accepted and modeled
avoidable re-reads:     eliminate
scratch spill traffic:  minimize
layout underfill:       measure and tune
CPU/GPU roundtrips:     eliminate from hot path
```

The local Metal counter path currently exposes `GPUTimestamp` only, not direct
L2/cache-miss counters. Therefore cache analysis is a combined method:

```text
measured runtime
  + modeled bytes
  + DRAM-equivalent bytes
  + tail underfill
  + Xcode/Metal trace where available
```

Do not claim measured hardware L2 miss rates unless a real counter source was
captured.

## Memory Forensics

Every hot op should be evaluated in byte buckets:

```text
unique_weight_bytes
weight_group_stream_bytes
logical_operand_weight_bytes
reuse_opportunity
non_weight_bytes
weight_reuse_floor_bytes
persistent/cache-resident floor
tail underfill
```

Good questions per op:

```text
Are weights streamed once per useful token tile?
Are hidden activations written only when needed by a later dispatch?
Does scratch fit modeled cache?
Does a larger tile reduce weight traffic but reduce occupancy?
Does a smaller tile increase dispatch/weight traffic but improve parallelism?
```

The correct answer is empirical. A first MMA128 probe looked ambiguous, but the
integrated p4096 hidden-dump gate and p512/p4096/p16384 sweep later showed
QKV/Z MMA128 was a real accepted win. Do not promote or reject large tiles from
single isolated impressions; use the integrated path and byte model together.

Keep autotuner defaults synchronized with `docs/kernel-dev/accepted_profile.env`.
If those drift, the autotuner may report an `accepted_selection` that no longer
matches the actual baseline and every later coordinate decision becomes
suspect. Use `tools/check_autotune_defaults.sh` after changing either the
accepted profile or autotuner baseline candidates.

For sub-percent candidate wins, use `tools/compare_delta_stack_candidate.sh`
instead of trusting a single baseline-then-candidate sweep. It alternates run
order so thermal drift and scheduler state are less likely to masquerade as a
kernel improvement.

Decode has an additional promotion rule. A 1-token or 4-token win is only a
microbenchmark result; it is not evidence that the decode path improved. Before
making any decode flag the default or adding it to an accepted profile, measure
at least tg128 and tg512 end-to-end against the current accepted profile and the
llama.cpp reference. Use `tools/run_decode_regression_matrix.sh` with at least
`--iterations 3 --rounds 2`, because long decode runs are sensitive to thermal
and scheduler drift. Record Split-K, rowcache, scratch bytes, dispatch count,
and token/s together. A path that wins at tg4 but loses at tg128/tg512 is a
regression, not a mega-kernel improvement.

Do not use one boolean default for all decode contexts unless the sweep proves
it. Split-K, KV-cache compression, rowcache, and approximate attention paths
need scenario thresholds. A mega-kernel policy is allowed to branch by context
length and output length; it is not allowed to hide a slower path behind a
single global "optimized" flag.

DeltaNet scan has a special warning. The accepted rowcache scan is byte-light
but not cheap: it carries a full `row_state[128]` per thread, loops serially
over tokens, stages Q/K through threadgroup memory, and uses barriers every
token. A low byte count does not imply a fast scan.

Rejected scan patterns:

```text
lanes4 / lanes4_ordered:
  faster or alternative state layout attempts, but hidden dumps drifted.
  Ordered threadgroup scratch did not fix the arithmetic drift and was slower.

rowcache_direct:
  removed q_s/k_s threadgroup staging and barriers by reloading Q/K from device
  memory, but drifted and was slower. Do not trade Q/K staging for direct
  per-row Q/K reloads in this recurrence.

rowcache_gated_norm:
  bit-exact and useful as an opt-in correctness reference, but slower in the
  integrated p4096 stack. Do not promote fusion just because it removes a
  scratch write/read; scan-loop register/barrier cost can dominate.
```

Rejected project/cache patterns:

```text
QKV/Z RG4 A-shared:
  stages one 128-token A tile in threadgroup memory and lets four row-groups
  share it. This looked attractive because QKV/Z projection still dominates
  absolute time, but paired evidence rejected it: +2.94% at p512, -1.00% at
  p4096, +3.51% at p16384. The likely cause is barrier/threadgroup-memory
  pressure and lower occupancy. Keep it opt-in only as a negative/control
  candidate.
```

Accepted scan patterns:

```text
rowcache_block32:
  accepted after a paired alternating sweep, not after the first unpaired token
  sweep. It preserves rowcache arithmetic and hidden dumps were bitexact.
  The gain is small, so future changes must use paired comparison before
  promotion or rollback.
```

Correct but not promoted scan patterns:

```text
rowcache_block64:
  preserve rowcache arithmetic by splitting rows into smaller threadgroups.
  Hidden dumps are bitexact, but block64 showed only sub-percent gains and was
  not promoted.
```

The next viable scan optimization must change the recurrence/blocking
mathematics while preserving stack-level equality, or it must introduce a
stronger accepted numerical contract. More low-level staging variants are not
enough by themselves.

Luce prefill lesson:

```text
Do not copy the decode megakernel structure into prefill. Luce treats prefill
as a separate path: large GEMM-like projections plus a chunked DeltaNet scan.
The transferable idea is a two-phase chunk scan that exposes token parallelism
and processes state slices locally, not another single-token-style serial scan
staging tweak.
```

OpenEvolve lesson for autotuning:

```text
Automated kernel search must not optimize a vague combined score. The evaluator
must expose:
  direct speedup ratio vs accepted profile
  p95 / variance penalty
  correctness margin
  cache/roofline class and time-vs-floor
  candidate family metadata
  compile/safety failure class

Use quality-diversity search only after these signals are reliable. Otherwise
the search can produce safe-looking candidates that overfit one benchmark and
regress the integrated path.
```

OpenEvolve GPU-kernel-discovery rule:

```text
Use OpenEvolve-style search as a dev-tool pattern:
  generated candidates
  multi-scenario evaluator
  hard correctness gate
  regression accounting
  hardware-aware prompts
  structured accept/reject records

Do not use it as a substitute for architecture analysis. The candidate space
must be seeded with the measured bottleneck:
  attention:
    vec<T,8>, two-pass exact softmax, GQA KV layout, split/reduce variants
  DeltaNet:
    chunk size, state-slice layout, reduction tree, row/col state layout,
    scratch lifetime, state propagation, and uBatch schedule
```

Candidate manifest:

```text
candidate_id:
kernel_or_file:
intended_bottleneck:
changed_layout:
tokens_or_contexts:
correctness_gate:
median_delta:
p95_delta:
roofline_class:
cache_or_scratch_hypothesis:
metal_error_stats:
accept_reject:
```

Promotion rule:

```text
An evolved candidate cannot be accepted from a single benchmark win. It needs
paired measurement, correctness, p95, and token/context sweep evidence just
like a hand-written candidate.
```

SIMDgroup-first rule:

```text
For Apple GPU kernels in this project, SIMDgroup structure is a design
primitive, not a cleanup step.

Use SIMDgroup algorithms first for:
  head-dim dots and reductions
  RMSNorm / layernorm statistics
  attention score reductions and online softmax
  DeltaNet row-state dot products
  LM-head row reductions and top-k/argmax
  sampling reductions

For 128-wide heads, prefer:
  32 SIMD lanes
  4 columns per lane
  lane-local vec4/state4
  simd_sum for the 128-wide reduction

Use threadgroup memory only when data must cross SIMDgroups or when a tile is
reused enough to pay for barriers.

Avoid as the first performance candidate:
  128 threads
  threadgroup scratch partials
  multiple barriers per reduction

The scratch version can remain as a correctness/control path.
```

Hardware-first rule:

```text
Kernel optimization starts with the actual device, not with a generic Apple
Silicon assumption.

Before promoting a kernel candidate, capture:
  tools/capture_hardware_feature_matrix.sh /tmp/ctox_qwen35_hardware_<date>
  tools/run_hardware_backend_shootout.sh <metalpack-dir> <tokens> <iterations> <output-dir>
  tools/analyze_hardware_backend_shootout.py <output-dir>/shootout.md
  tools/run_sme2_smoke_probe.sh
  tools/run_sme2_mopa_probe.sh <repeats> <iterations> <warmup>
  tools/run_sme2_i8_tile_probe.sh <tokens> <rows> <k> <iterations> <warmup>
  tools/run_static_int8_matmul_autotune.sh <tokens> <rows> <iterations> <warmup>
  tools/analyze_static_int8_autotune.py <output> [--reference-median-s S]
  tools/prefill_reference_report.py
  tools/exact_attention_traffic_report.py [--tokens csv] [--sustained-gb-s B]
  tools/run_attention_qk_mps_probe.sh [tokens-csv] [iterations] [warmup] [output-dir]
  tools/analyze_attention_qk_mps_probe.py <report.md>
  tools/plan_tiled_attention.py [--tokens N] [--q-tiles csv] [--k-tiles csv]
  tools/run_tiled_attention_qk_mps_prototype.sh <tokens> <q_tile> <k_tile> <iterations> <warmup>
  tools/run_tiled_attention_qk_mps_grid.sh <tokens> <iterations> <warmup> <output>
  tools/analyze_tiled_attention_qk_mps_grid.py <output>
  tools/run_tiled_attention_full_mps_prototype.sh <tokens> <q_tile> <k_tile> <iterations> <warmup> <heads_per_group> <matrix_origins> <quality_check>
  tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline_<date>

Required facts for this M5 machine:
  chip: Apple M5
  CPU cores: 10 total, 4 performance + 6 efficiency
  GPU cores: 10
  unified memory: 32 GB
  Metal: Metal 4
  CPU ISA sysctls: SME=1, SME2=1, BF16=1, I8MM=1, DotProd=1
  public Metal counters in this probe: GPUTimestamp only

Implications:
  SIMD availability is not proof of speed. Every SIMD, MMA, tensor, or
  quantized candidate must show measured wall-time, p95, and roofline movement.
  Cache misses cannot be asserted from public counters here; label cache
  evidence as modeled/timing-derived unless a named counter source exists.
  M5 GPU Neural Accelerators / Metal 4 tensor APIs are a separate backend track.
  Handwritten MSL `simdgroup_multiply_accumulate` kernels are useful, but they
  are not automatically proof that the new M5 tensor hardware is saturated.

Backend selection rule:
  GPU Metal 4 tensor / MPS / MPSGraph probes:
    large dense matmuls, quantized matmuls, LM head, FFN, projections
  MSL SIMDgroup kernels:
    DeltaNet recurrence, reductions, online softmax, sampling, layout-specific
    fusions that tensor APIs cannot express
  CPU SME:
    packing, validation, possible coarse fallback probes; never assume it wins
    a token hot path without a local roofline-backed measurement
```

M5 matrix backend probe rule:

```text
For every large dense matmul family, compare the handwritten MSL kernel against
an Apple framework matrix backend before spending more time on bespoke MSL:

  tools/run_mps_matrix_probe.sh <m> <n> <k> <iterations> <warmup>
  tools/run_mps_ffn_block_probe.sh <tokens> <hidden> <intermediate> <iterations> <warmup>
  tools/run_mps_ffn_metalpack_probe.sh <metalpack-dir> <layer> <tokens> <iterations> <warmup>
  target/release/pack_mps_ffn_sidecar <source.metalpack-dir> <output.mps-ffn-dir>
  tools/run_mps_ffn_sidecar_probe.sh <mps-ffn-sidecar-dir> <layer> <tokens> <iterations> <warmup>
  target/release/bench_mps_ffn_sidecar_runtime <mps-ffn-sidecar-dir> <layer> <tokens> <iterations> <warmup>
  tools/run_mps_deltanet_project_probe.sh <tokens> <hidden> <qkv_rows> <z_rows> <iterations> <warmup>
  target/release/pack_mps_delta_project_sidecar <source.metalpack-dir> <output.mps-delta-project-dir>
  tools/run_mps_deltanet_project_sidecar_probe.sh <mps-delta-project-sidecar-dir> <layer> <tokens> <iterations> <warmup>
  tools/estimate_mps_ffn_prefill_impact.py
  target/release/bench_metalpack_prefill_delta3_ffn_superblock <metalpack-dir> <start-layer> <tokens> <iterations> <warmup> <delta-layer-count> <mps-ffn-sidecar-dir> <mps-delta-project-sidecar-dir>
  target/release/memory_forensics <metalpack-dir> <tokens> <iterations> <sustained-gb-s> <mps-ffn-sidecar-dir> <mps-delta-project-sidecar-dir>
  tools/run_hardware_backend_shootout.sh <metalpack-dir> <tokens> <iterations> <output-dir>
  tools/analyze_hardware_backend_shootout.py <shootout.md>
  tools/run_sme2_smoke_probe.sh
  tools/run_sme2_mopa_probe.sh <repeats> <iterations> <warmup>
  tools/run_sme2_i8_tile_probe.sh <tokens> <rows> <k> <iterations> <warmup>
  tools/run_static_int8_matmul_autotune.sh <tokens> <rows> <iterations> <warmup>
  tools/analyze_static_int8_autotune.py <output> [--reference-median-s S]
  tools/prefill_reference_report.py
  tools/exact_attention_traffic_report.py [--tokens csv] [--sustained-gb-s B]
  tools/run_attention_qk_mps_probe.sh [tokens-csv] [iterations] [warmup] [output-dir]
  tools/analyze_attention_qk_mps_probe.py <report.md>
  tools/plan_tiled_attention.py [--tokens N] [--q-tiles csv] [--k-tiles csv]
  tools/run_tiled_attention_qk_mps_prototype.sh <tokens> <q_tile> <k_tile> <iterations> <warmup>
  tools/run_tiled_attention_qk_mps_grid.sh <tokens> <iterations> <warmup> <output>
  tools/analyze_tiled_attention_qk_mps_grid.py <output>
  tools/run_tiled_attention_full_mps_prototype.sh <tokens> <q_tile> <k_tile> <iterations> <warmup> <heads_per_group> <matrix_origins> <quality_check>
  tools/run_matrix_backend_shootout.sh <metalpack-dir> <tokens> <iterations> <output-dir>
  tools/analyze_matrix_backend_grid.py <shootout.md>
  tools/run_cpu_quant_probe.sh <tokens> <rows> <k> <iterations> <warmup>

Qwen3.5 shape mapping:
  projection / qkv / z:       m=tokens, n=rows, k=1024
  FFN gate+up combined:       m=tokens, n=7168, k=1024
  FFN down:                   m=tokens, n=1024, k=3584
  Delta out:                  m=tokens, n=1024, k=2048
  LM head shortlist/full:     m=tokens_or_1, n=vocab_tile_or_vocab, k=1024

Interpretation:
  MPS/Metal tensor throughput is not automatically usable inside the fused
  Qwen pipeline because fusions, layouts, quantized packing, and command-buffer
  composition matter. But if the framework backend is materially faster for
  the raw GEMM shape, handwritten MSL should not be the default assumption.

Integrated sidecar lesson:
  Standalone MPS probes are necessary but insufficient. A dense matrix family
  becomes actionable only after the same sidecar is measured inside the real
  command timeline: MSL producer, end compute encoder, MPSMatrix encode on the
  same command buffer, MSL bridge/consumer, drift check, and memory_forensics
  with sidecar-aware byte buckets. Once weight_stream/unique falls near 1.0x,
  stop tuning the old handwritten matmul and move to the next byte/stall
  bucket: layout bridges, scan/gated-norm, DeltaOut, or attention.

qkvz-direct rule:
  If a framework backend returns a wider fused matrix output, first try making
  downstream MSL consumers stride-aware before copying the output into legacy
  split tensors. The QKVZ direct bridge beat the split bridge at p4096 because
  Conv/Split and GatedNorm could read qkvz[tokens,8192] directly without a
  separate global split pass.

re-sweep rule:
  Rejected kernels are not permanent facts. After a layout/backend change,
  re-sweep previous rejects whose cost model changed. Lanes4 scan was slower in
  the old path, but after MPS QKVZ direct it beat rowcache_block32 for ScanNorm.

model-wide estimator rule:
  Once a backend replacement is proven for a repeated layer family, apply it to
  all equivalent model rows in memory_forensics. Leaving attention-layer FFNs on
  the old MSL row after proving MPS FFN sidecars for Delta-layer FFNs creates an
  artificially pessimistic full-prefill estimate.
```

Quantized candidate rule:

```text
Quantization is allowed and expected to change numerics. CTOX optimization must
accept bounded inaccuracy when it buys real speed. The requirement is not
bitexactness; the requirement is an explicit quality budget and a static
quantized pipeline.

Do not build conversion-heavy fake quantization:
  bad:
    f32 -> f16 -> f32 inside the hot loop
    q4 -> f32 materialized tensor -> f16 output every token
    per-token requantization of weights or cache

  good:
    checkpoint -> one-time static packed format
    packed format uploaded once
    kernels consume that format directly
    any unpack/dequant happens only lane-local/tile-local inside the dot or
    recurrence, without materializing a full dequant tensor

Examples:
  f16 -> f16 -> f16:
    acceptable for pure low-precision path if reductions/accumulators are part
    of the declared quality budget

  q4/int8 packed weights -> int/f16 accumulators:
    acceptable if the packed representation is static and the kernel consumes
    it directly

  f16 state surface read as half and immediately converted to float:
    diagnostic candidate only unless the surrounding pipeline avoids repeated
    dtype boundary costs

Every quantized mode must have a `QUANT_PIPELINE_TEMPLATE.md` record.
Use `tools/validate_quant_pipeline.py --strict <record>` before measuring it.
Use `tools/validate_metalpack_quant_manifest.py [--strict] <metalpack>` to
prove that packed byte counts match row tiles, col tiles, group size, and value
bits before a quantized pack can be used as benchmark evidence.

Quantization is selected from the platform backend, not from compression ratio
alone. Before implementing a quantized candidate, record the target backend:
GPU MSL SIMDgroup, GPU MPS matrix, GPU Metal 4 tensor, CPU NEON, CPU SME,
Core ML/ANE, or hybrid. The record must name the fastest expected primitive
for that dtype on this Mac and the evidence for it. A Q4 layout that cannot
feed a fast hardware primitive is only smaller, not necessarily faster.

Layout must also serve the hardware access pattern. For CPU NEON/SME this
means contiguous groups that allow prefetch/speculative fetch to land on the
next matrix panel. For GPU SIMDgroup/MMA paths this means adjacent lanes and
threadgroups read adjacent packed groups/scales without divergent address
arithmetic. If the next data package is not predictable from the current group
stride, the layout is not ready for promotion.

Static pack format status:
  The pack plan and metalpack manifest now understand:
    int8_row_tiled
    int4_groupwise_row_tiled

  The int8 writer emits, per row and `quant_group_size` inside each col tile:
    f16 scale
    int8 quantized values

  The int4 writer emits, per row and `quant_group_size` inside each col tile:
    f16 scale
    packed signed 4-bit values, two values per byte

  `quant_group_size` is part of the payload contract, not only manifest
  metadata. It must be nonzero, no larger than `col_tile`, divide `col_tile`,
  and be even for int4. A kernel that assumes one scale per full col tile is a
  different layout and must not be benchmarked as `int4_groupwise_row_tiled`.

  These formats are intentionally distinct from fp16_row_tiled. Existing FP16
  kernels must reject them until a matching static-quantized kernel consumes the
  format directly.

Rejected first int8 schedule:
  `qwen35_08b_prefill_matmul_int8_row_tiled_k1024_f32` proved the static format
  can be consumed directly, but the row/threadgroup schedule is not competitive.
  Compression alone is not enough. Quantized matmul candidates must also map to
  M5 matrix/tensor hardware or a demonstrably efficient SIMDgroup schedule.

Minimum gate:
  tools/quant_error_gate.py <measurement.txt> \
    --max-abs <limit> \
    --mean-abs <limit> \
    --baseline-key <baseline_time_key> \
    --candidate-key <candidate_time_key> \
    --speedup-min <ratio>

Promotion cannot rely on max/mean error alone. For model-visible paths, add:
  hidden-dump drift across p512 / p4096 / p16384
  logit drift
  greedy-token divergence
  at least one task or prompt-level quality smoke if logits drift is nontrivial

Error budget examples:
  internal recurrent state candidate:
    max_abs <= 2e-5, mean_abs <= 1e-6, plus hidden/logit gate
  matmul fp16/MMA candidate:
    max_abs <= 2e-3, mean_abs <= 5e-5, plus hidden/logit gate
  int8/int4 weight candidate:
    accuracy loss is expected; budget must be calibrated per layer family and
    quality set, then optimized on a speed/quality Pareto curve
```

Sparse attention / NSA rule:

```text
Native Sparse Attention is not a bitexact replacement for a model's existing
full attention layers. It combines compressed, selected-block, and sliding
window attention and expects model/routing support. For Qwen3.5-0.8B, treat it
as an approximate long-context experiment only.

Use it when:
  attention.core dominates at long context
  the evaluation explicitly allows logits/token drift
  selected block count, block size, and window size are autotuned

Do not use it when:
  measuring accepted-profile correctness
  trying to close the current DeltaNet-prefill gap
  comparing bitexact hidden dumps against the full-attention baseline
```

Long-context attention taxonomy:

```text
Accepted-reference path:
  exact attention kernels and exact KV memory layout changes only.
  Examples: FlashAttention-style tiling, Flash-Decoding/Split-K for decode,
  PagedAttention-style allocation/paging that does not drop tokens.

Approximate long-context path:
  dynamic sparse prefill, selected KV blocks, KV pruning, attention sinks,
  DuoAttention-style head policies, and KV quantization.

Never compare these paths without labels:
  exact path:
    hidden/logit equality budget
    llama.cpp/reference comparison is valid
  approximate path:
    logits drift and greedy-token divergence are expected metrics
    report speed-quality Pareto, not just tok/s
```

Candidate ordering for Qwen3.5-0.8B:

```text
1. Exact DeltaNet prefill chunk scan, because measured Delta18+FFN dominates.
2. Exact attention tiling/Split-K when attention.core becomes the long-context
   floor bottleneck.
3. Optional approximate sparse/quantized KV mode after exact baselines are
   stable and evaluation includes quality drift.
```

## Layout And Tile Decision Matrix

Use this matrix when choosing what to tune next:

| Symptom | Likely Cause | First Tool | Next Candidate |
| --- | --- | --- | --- |
| modeled bytes lower but runtime worse | register pressure, lower occupancy, bad tail underfill | `profile_metalpack_prefill_delta_stack` | smaller tile or split accumulators |
| isolated kernel wins but full path loses | dispatch interaction, scratch traffic, CPU orchestration | integrated superblock benchmark | move candidate behind full-path gate |
| p95 much worse than median | runtime variance, memory pressure, occupancy instability | 7+ iterations | reject or keep opt-in |
| checksum matches but hidden dump drifts | checksum too weak | `compare_half_dump` | add logits/greedy gate |
| long-context attention regresses | partial scratch too large or too many inactive blocks | `cache_analysis`, Split-K sweep | active-block dispatch, block-size sweep |
| short-context Split-K loses | extra dispatches exceed parallelism gain | layered decode benchmark | threshold Split-K by position |
| larger tile wins at 128 but fails at 4096 | cache/tail behavior changes with shape | token sweep | require multi-token acceptance |
| Core ML/ANE path looks promising but slow | CPU fallback or graph overhead | Core ML placement report | coarse graph only, W8A8, no token-loop ping-pong |

The useful question is not "which tile is best?" but "which tile is best for
this shape, this op, this cache behavior, and this correctness budget?"

## Autotuning Method

Manual flag flipping does not scale. Use serial coordinate descent and token
sweeps.

Autotuning is mandatory for memory layout work. A layout is not "better"
because it looks more coalesced or reduces modeled bytes in isolation. It is
better only if the empirical sweep improves the integrated path against the
local roofline and preserves correctness.

Every layout/tile search should record:

```text
row_tile
token_tile
chunk_size
threadgroup size
simdgroup mapping
scratch bytes
modeled bytes
effective_GB/s
bandwidth_utilization
time_vs_floor
p95/median stability
checksum or stronger correctness gate
```

The expected output of tooling is not just "candidate A is fastest"; it must
say which resource is blocking the next step:

```text
near byte floor              -> reduce algorithmic traffic
low bandwidth utilization    -> occupancy/layout/dispatch/scratch investigation
traffic above model          -> avoidable re-read/cache-miss bug
runtime worse with fewer bytes -> register pressure or occupancy regression
```

Current tunable families:

```text
qkvz:       mma32, mma64, mma128
delta_out:  mma32_res, mma64_res
gate_up:    mma32, mma64
down:       mma32_res, mma64_res
scan:       rowcache, lanes4, lanes4_ordered
conv:       fused, fused_tok4
```

Historical low-occupancy candidates such as qkvz `mma8` and `mma16` should
remain in discovery sweeps when investigating project-phase regressions, even
if they are known likely losers. They are useful controls: if a supposedly
better large tile loses, the small-tile rows help separate register pressure
from weight-stream reuse.

Required outputs:

```text
median_s
p95_s
effective_GB/s
sustained_stream_GB_s from roofline.env
operational_matmul_GB_s from roofline.env
modeled_bytes
bandwidth_utilization
time_vs_floor
reported_effective_vs_modeled
traffic_vs_model if hardware bytes exist
gap classification
tok/s
checksum
correctness status
accepted_selection
```

Acceptance must sweep token lengths. A candidate can pass at 128 tokens and
fail or regress at 4096+.

Minimum acceptance sweep:

```text
512
4096
16384
```

For long-context work:

```text
32768
65536
131072
```

### Autotune Command Cookbook

Quick smoke, only to verify the tooling path:

```text
target/release/autotune_metalpack_prefill_delta_stack \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 128 1 0 1 0 1
```

Serious 18-layer DeltaNet+FFN tune at 4096 tokens:

```text
target/release/autotune_metalpack_prefill_delta_stack \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 7 1 18 0 2
```

Serial token sweep:

```text
target/release/sweep_metalpack_prefill_delta_autotune \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 512,4096,16384 3 1 18 0 2
```

Long-context sweep, expensive:

```text
target/release/sweep_metalpack_prefill_delta_autotune \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 32768,65536,131072 3 1 18 0 2
```

Override the CSV destination:

```text
CTOX_QWEN35_AUTOTUNE_CSV=/tmp/qwen35_delta_tune.csv \
target/release/autotune_metalpack_prefill_delta_stack ...
```

Skip validation only for tooling smoke, never for acceptance:

```text
CTOX_QWEN35_AUTOTUNE_SKIP_VALIDATE=1 ...
```

### Autotune Acceptance Protocol

A candidate can be promoted only if:

```text
1. It is selected as best_selection.
2. It passes the hidden-dump gate and becomes accepted_selection.
3. It passes at least 512, 4096, and 16384 tokens.
4. It does not materially worsen p95.
5. Its byte/cache model explains why it is faster.
6. It preserves or improves integrated-path performance.
7. The exact env flags and results are logged.
```

If a candidate is faster but fails correctness, keep the kernel as a diagnostic
path and investigate numerical order/layout effects. Do not promote it.

## Accepted Patterns So Far

### GPU-Local State

Keep hidden state, KV cache, DeltaNet recurrent state, logits/top-k scratch, and
sampling on GPU. CPU reads only the next token.

### Full-Vocab LM-Head Argmax On GPU

The 248,320 vocab LM head is too large to copy logits to CPU. Compute logits and
argmax/top-k on GPU, return one token.

### Per-Layer Real Binding

Manifests must resolve actual layer weights:

```text
18 DeltaNet layer slots
6 Attention layer slots
24 FFN slots
per-layer input RMSNorm
per-layer post-attention RMSNorm
final RMSNorm
```

Template-prefix shortcuts are only for synthetic packs.

### Residual Fusion

Fuse residual add into projection writeback where correctness permits:

```text
token mixer out_proj + residual
FFN down_proj + residual
```

This removes separate dispatches and activation roundtrips.

### Delta Out MMA64

Accepted into `docs/kernel-dev/accepted_profile.env` after a token sweep showed
small but consistent Delta18+FFN improvement and hidden-dump equality for the
isolated p4096 candidate. `scan_lanes4` remains rejected as a default because
it is faster but fails the hidden-dump gate.

### Safe FFN Fusion

Accepted direction:

```text
RMSNorm + gate/up projection + SwiGLU staging
down projection + residual writeback
```

The two-projection FFN remains a weight-streaming problem. Tile size must be
tuned; "larger tile" is not automatically faster.

### Safe DeltaNet Fusion

Accepted direction:

```text
qkv/z/b/a projection fusion
split + q/k normalization fusion
keep recurrent state update separate unless parity proves otherwise
```

The recurrent state update is numerically fragile. Deeper fusions were rejected
when they broke greedy parity.

### Attention GQA Cache Reuse

Use qh4/GQA-aware decode attention instead of one threadgroup per Q-head. For
Qwen3.5 attention, four Q heads share each KV head. Re-reading the same KV cache
per Q-head is a real memory/cache pathology.

### Split-K / Flash-Decoding Direction

For longer contexts, split the key dimension into active blocks and combine
with stable online softmax summaries:

```text
partial: m, l, acc[head_dim]
combine: log-sum-exp merge
```

Important fix: dispatch only active key blocks for `position + 1`, not all
blocks allocated for `max_context`.

Current lesson: Split-K128 can win after active-block dispatch, but scratch
traffic and thresholds still require token/context sweeps.

## Rejected Or Risky Patterns

Rejected patterns are first-class knowledge. Keep the summary here short, but
link or name the log entry/record that contains the full measurement evidence.
Never delete a rejected path from memory just because the code is later cleaned
up.

### Per-Layer Metal/Core ML Switching

Do not put ANE/NPU in the decode hot path by switching per layer. Core ML is a
separate graph path. Stateful DeltaNet, KV-cache mutation, and per-token decode
are hostile to fine-grained ANE ping-pong.

### Full DeltaNet State-Step Fusion Without Strong Parity

Rejected because it changed token output. DeltaNet state math is sensitive to
normalization, decay, beta, and update order.

### Scan + GatedNorm Fusion

Looked plausible, measured slower. Keep disabled unless a new layout changes
the result.

### Tok4 Conv/Split Fusion

Measured as a hard regression. Disable by default.

### Blind MMA128 Promotion

MMA128 can reduce modeled weight streams but lose in wall time due to occupancy
and register pressure. It remains a candidate, not a default.

### Lanes4 Ordered Scan

Rejected. It tried to preserve rowcache-like summation order with threadgroup
scratch and tx==0 serial reductions while keeping the low-register lanes4 state
layout. It was slower than rowcache and still failed hidden-dump correctness.
Do not repeat this exact barrier-heavy ordered reduction strategy.

### Copying Full Logits To CPU

Never do this in the target decode path. It creates bandwidth traffic and a hard
sync exactly where latency matters most.

## llama.cpp Transfer Lessons

The original reference advantage was not magic and not ANE. The parts worth
copying are architectural. Even now that exact prefill forensics beats
llama.cpp, these remain the main lessons for 27B/35B transfer.

llama.cpp has:

```text
model-specific Qwen3.5 / Qwen3Next graph integration
fused/chunked/autoregressive DeltaNet modes
ggml_gated_delta_net in Metal
mature FlashAttention / GGML_OP_FLASH_ATTN_EXT
broad quantized matmul coverage
ubatching and preplanned graph scheduling
preallocated buffers and mature pipeline caching
fewer accidental CPU-side lookup/dispatch costs
```

The prefill lesson to copy from llama.cpp is specific:

```text
token routing:
  n_tokens == 1:
    autoregressive or fused autoregressive DeltaNet
  n_tokens > 1:
    chunked or fused chunked DeltaNet

physical batching:
  tune n_ubatch / physical prompt chunk size independently from logical context
  length. Long prompt support is not the same thing as one enormous GPU batch.

recurrent-vs-attention memory:
  recurrent state and attention KV cache need different preparation/splitting
  rules. Treating Qwen3.5 as a normal all-attention transformer loses the main
  structure of the model.

Metal attention:
  mature implementations use multiple specialized FlashAttention variants
  (pad/block/core/vector/reduce), not a single generic kernel.
```

The main CTOX gaps before the current prefill win were:

```text
chunked/fused DeltaNet prefill math
model-grade prefill attention
quantized weight paths with in-dot dequantization
runtime graph planning and pipeline caching
autotuned memory layouts with correctness gates
```

## Current Performance Interpretation

The current probe has real wins:

```text
captured greedy parity against MLX for the raw-token probe
GPU-local next-token output
real per-layer bindings
dispatch reductions through safe fusion
qh4 GQA cache reuse
first Split-K decode path
autotuner with correctness gate
MPS sidecars for FFN, Delta project, Attention O, and DeltaOut
exact MPS tiled prefill attention
prefill forensics above llama.cpp at 4k/16k/32k
```

The remaining work is no longer "catch llama.cpp in prefill at all"; it is to
turn the current measured wins into a robust, mode-aware runtime policy and to
make the same method transfer to larger models:

```text
decode needs stable tg128/tg512 acceptance matrices, not just cooled wins
DeltaNet exact scan remains the main custom recurrent bottleneck
quantized matmul/dequant paths are not yet accepted-profile quality
runtime graph planning remains prototype-grade
cache/memory forensics are mostly modeled rather than true L2 counter backed
approximate paths need explicit quality budgets before they can count
```

This matters for planning: do not keep optimizing a local kernel after the
integrated reference has moved. The next decision must be based on the current
gap table: exact Delta scan, sustained decode policy, quantized matrix backend,
or 27B/35B transfer constraints.

## Prefill Strategy

Decode recurrence is sequential by nature; prefill is where chunked/parallel
math matters.

High-value prefill work:

```text
1. DeltaNet chunked/fused scan
2. prefill attention with mature FlashAttention-style tiling
3. FFN matmul tile autotuning
4. memory-layout sweeps over token lengths
5. correctness gates at 512/4096/16384+
```

Do not over-optimize isolated decode attention if prefill DeltaNet+FFN dominates
the end-to-end gap.

Current exact-attention integration rule:

```text
tools/run_prefill_attention_backend_matrix.sh
```

compares the real accepted attention-core path against the synthetic Rust MPS
tiled QK-softmax-PV prototype. Treat this as prioritization evidence only. The
MPS tiled row is not accepted-profile performance until it consumes real Q/K/V
buffers, writes the real attention tensor, feeds the real O projection, and
passes a hidden-dump or sparse-output quality gate. The current bridge packs a
synthetic accepted-layout Q/K/V cache and measures both Qwen KV groups, so the
ratio is stronger than the older one-KV projection. A large p16k/p32k ratio
means integration is worth doing; it does not by itself prove a model speedup.

For the integrated opt-in MPS tiled path, use:

```bash
tools/run_prefill_mps_tiled_projection.sh --sizes 4096,16384,32768 \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

This tool live-measures accepted QH4 attention and
`CTOX_QWEN35_ATTENTION_MPS_TILED=1`, then projects the six full-attention layers
inside the current exact full-prefill baseline. Treat it as a promotion gate:
it shows whether the full-prefill gap should close. As of the
20260501T081544Z decision, MPS tiled attention is the accepted prefill attention
backend after p512/p4096 all-layer raw-dump parity and p4096/p16k/p32k
full-prefill forensics wins against llama.cpp.

### llama.cpp Transfer Rule

Copy these ideas:

```text
chunked DeltaNet math
capability-gated fused DeltaNet path
uBatch/chunk-size scheduling
separate recurrent-state memory handling
specialized exact attention variants
preplanned graph/runtime resources
```

Do not copy these blindly:

```text
generic graph abstraction
all-model tensor layouts
benchmark numbers without matching dtype/quantization/context
approximate sparse/KV tricks into exact-reference comparisons
```

The CTOX target is still a hardcoded Qwen3.5 Metal path. llama.cpp is the
reference for what algorithmic structure must exist; it is not the final shape
of the CTOX runtime.

## Decode Strategy

Decode needs low latency and persistent state:

```text
one command buffer per token initially
GPU-local recurrent/KV state
GPU-local LM-head argmax/sampling
no CPU reads of hidden/logits/cache
no per-layer runtime decisions on CPU if they can be preplanned
```

At short context, decode can be dominated by matvec/LM-head. At long context,
attention KV streaming and Split-K scratch become visible.

## CPU Runtime Strategy

CPU optimization is not optional. It appears as GPU underutilization when the
shader is waiting for command construction, pipeline lookup, or synchronization.

Rules:

```text
precompile and cache pipeline states
avoid per-token pipeline lookup where possible
preplan static layer dispatch order
reuse buffers and argument layouts
avoid CPU-visible reads except next_token and optional diagnostics
keep experimental branch checks outside hot paths when disabled
```

Known lesson: Split-K pipeline lookup was happening even when qh4 was used for
short contexts. Moving lookup inside the active branch improved the real path.

## ANE/NPU Strategy

Metal shaders do not run on the Neural Engine. ANE means Core ML.

Useful ANE experiments:

```text
isolated linear/FFN blocks
W8A8 quantized Core ML blocks
prefill-sized coarse graphs
vision encoder or other coarse non-token-loop work
operation placement and fallback reports
```

Reject ANE for a path if:

```text
stateful DeltaNet falls back to CPU
KV-cache mutation falls back to CPU
per-token graph overhead dominates
Metal/Core ML transfer creates bubbles
```

## Engineering Rules

### Build From Reference Truth

Always keep a reference:

```text
MLX for token/logit parity
llama.cpp for performance reference
CPU reference kernels for local operator math
dump/compare tools for hidden tensors
```

### Optimize Integrated Paths

Standalone microbenchmarks are useful but can lie after the real scheduler
changes. Promote only after the integrated 24-layer path or superblock path
confirms the result.

### Measure The CPU Too

One real bug was resolving Split-K pipeline states even when Split-K was not
used for the current token. That was CPU orchestration overhead, not shader
math. Pipeline lookup, command encoding, and branch placement matter.

### Prefer Opt-In Experimental Flags

New risky paths should be env-gated first:

```text
CTOX_QWEN35_...
```

Default only after:

```text
correctness gate passes
integrated benchmark wins
token/context sweep does not regress
research log records decision
```

### Keep The Log Executable

Every log entry should include enough command/output context to reproduce:

```text
command
model/metalpack
env flags
tokens/context/steps
median/p95
checksum or parity
decision
next step
```

## Current Tool Map

Core diagnostics:

```text
bench_stream
cache_analysis
memory_forensics
list_metal_counters
compare_half_dump
compare_attention_raw_dump
```

Model/artifact tools:

```text
inspect_artifacts
pack_weights
audit_shapes
make_synthetic_metalpack
```

Prefill tools:

```text
bench_metalpack_prefill_delta3_ffn_superblock
profile_metalpack_prefill_delta_stack
autotune_metalpack_prefill_delta_stack
sweep_metalpack_prefill_delta_autotune
bench_metalpack_prefill_attention_core
bench_metalpack_prefill_ffn_block
```

Decode tools:

```text
bench_metalpack_decode_layered_pattern
bench_metalpack_decode_attention
bench_metalpack_decode_attention_steps
bench_metalpack_decode_deltanet
bench_metalpack_lm_head
```

External reference:

```text
llama.cpp llama-bench
tools/mlx_reference.py
tools/coreml_ane_probe.py
tools/run_hardware_backend_shootout.sh
tools/analyze_hardware_backend_shootout.py
tools/run_sme2_smoke_probe.sh
tools/run_sme2_mopa_probe.sh
tools/run_sme2_i8_tile_probe.sh
tools/run_static_int8_matmul_autotune.sh
tools/analyze_static_int8_autotune.py
tools/prefill_reference_report.py
tools/exact_attention_traffic_report.py
tools/run_attention_qk_mps_probe.sh
tools/analyze_attention_qk_mps_probe.py
tools/plan_tiled_attention.py
tools/capture_metal_trace.sh
```

## Subagent Policy

Subagents are useful, but benchmark discipline is more important.

Allowed subagent work:

```text
read code and summarize callsites
inspect external references
compare algorithms
propose candidate kernels
review docs
look for missing correctness gates
```

Not allowed for subagents during performance work:

```text
running benchmarks
running token sweeps
running competing tests that perturb thermals or shared GPU state
editing the same hot files without clear ownership
```

The main thread owns all performance measurements.

## Hypothesis Template

Use this for each new kernel idea:

```text
Hypothesis:
  <one sentence, falsifiable>

Expected win:
  <less weight traffic / fewer dispatches / less scratch / better occupancy>

Risk:
  <numerical drift / register pressure / cache underfill / CPU overhead>

Implementation:
  <kernel/runtime files and env flag>

Correctness gate:
  <checksum / dump / logits / greedy tokens>

Benchmark:
  <tokens, iterations, warmup, reference>

Decision rule:
  accept if <criteria>; reject if <criteria>
```

If the hypothesis cannot be falsified with a benchmark and a correctness gate,
it is not ready to implement.

## Definition Of Done For A Kernel Optimization

A kernel/layout optimization is done only when:

```text
1. It has an explicit hypothesis.
2. It is env-gated while experimental.
3. It passes operator-level correctness.
4. It passes integrated-path correctness.
5. It wins median and does not worsen p95 materially.
6. It survives token/context sweep.
7. Its byte/cache model explains the win.
8. It is documented in RESEARCH_LOG.md.
9. It is added to this handbook if it changes strategy.
```

## Planning Loop

Use this loop for every work session:

```text
1. Read latest log entry and current accepted defaults.
2. Pick the largest measured bottleneck, not the most interesting kernel.
3. Form one falsifiable hypothesis.
4. Build the smallest tool or kernel needed to test it.
5. Run serial measurements.
6. Compare against MLX/llama.cpp/reference.
7. Accept, reject, or keep opt-in.
8. Update the log and handbook if the rule generalizes.
```

The important cultural rule: failed hypotheses are not failures if they are
measured, logged, and used to narrow the search space.

## SIMD Ownership Rule

Do not equate maximum thread count with maximum SIMD utilization. For Qwen3.5
attention prefill, the faster path is:

```text
one SIMDgroup per query and GQA KV group
32 lanes
8 head dimensions per lane
4 Q heads per KV head
no threadgroup-memory dot-product reduction
no per-key threadgroup barriers
```

This beat the previous 256-thread qh4/qblk1 kernel because the old kernel used
all head-dimension threads but paid cross-SIMD reductions through threadgroup
memory for every key. If the operation's reduction width fits inside one SIMD
group after assigning multiple elements per lane, test that schedule before
building larger threadgroups.

Practical checklist:

```text
1. Count synchronization points per logical key/tile.
2. Check whether a single SIMDgroup can own the full reduction by vectorizing
   each lane over multiple dimensions.
3. Compare register pressure against removed barriers, not just theoretical
   load bytes.
4. Validate FP16 drift against the previous accepted kernel.
5. Promote only if the byte model and the stage profiler both explain the win.
```

## Transfer Rules For 27B/35B

Do not blindly copy 0.8B tile sizes to larger models. Copy the method.

Likely transferable:

```text
GPU-local decode state
CPU one-sync-per-token target
full-logits-never-to-CPU rule
correctness-gated autotuning
byte/cache forensic model
accepted/rejected decision logging
subagent no-benchmark policy
Core ML/ANE as separate coarse-graph experiment
```

Needs retuning:

```text
row/col tile sizes
MMA accumulator width
Split-K block size and threshold
quant group size
KV-cache quantization layout
FFN and LM-head packing
activation scratch placement
```

Likely more important at 27B/35B:

```text
quantized weights with in-dot dequantization
KV-cache compression
prefill chunking
graph scheduling / command reuse
memory bandwidth roofline
thermal stability and long-run p95
```

The large-model rule is simple: if 0.8B cannot produce a measured,
correctness-gated win for a technique, do not assume it will scale into a win.

## Exact Versus Approximate Attention Rule

Long-context attention optimizations must be labeled as exact or approximate.
A kernel that changes the visible K/V set, such as a fixed recent-token window,
can be useful, but it cannot be compared to llama.cpp as an exact replacement
without quality metrics.

Minimum evidence for approximate attention:

```text
1. Speed against the exact local kernel at several token lengths.
2. Tensor drift against exact attention on a representative prompt length.
3. A task-quality metric before promotion: perplexity, retrieval, or targeted
   long-context eval.
4. Explicit opt-in flag name that exposes the approximation.
```

Current example:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096=1
  fast at p16384/p32768
  not accepted by default
  p8192 drift: rms 0.00927, max 0.75684
```

## Sparse/Window Attention Tuning Rule

Sparse attention is a performance/quality grid, not a single optimization flag.
For a fixed local window, the runtime scales roughly with visible K/V count
instead of full causal T^2, but the dropped K/V positions change model
semantics.

Required workflow:

```text
1. Run the exact qh4 SIMD32 vec8 path and dump raw attention.
2. Sweep candidate windows with tools/run_attention_window_quality_sweep.sh.
3. Record median_s, effective GB/s, mean_abs, rms, max_abs, checksum_delta.
4. Reject or keep opt-in based on quality budget, not just tok/s.
5. Only compare against llama.cpp as a replacement if semantics are exact or
   task-quality loss is explicitly accepted.
```

Current parameterized flag:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW=<tokens>
```

Observed long-prefill rule of thumb:

```text
p32768 window4096:
  very fast, projected full prefill beats llama.cpp, but RMS drift is 0.03549.

p32768 window16384:
  lower drift, but projected full prefill remains slower than llama.cpp.
```

That means fixed windows alone are not the final answer. The next sparse track
needs smarter KV selection, head-specific policies, or task-aware acceptance
tests.

## Exact Attention Byte-Floor Rule

For long prefill, the first question is whether the measured exact kernel is
above the compulsory K/V stream floor.

Use:

```text
target/release/cache_analysis --tokens <N> --sustained-gb-s <measured>
```

Required interpretation:

```text
attention.prefill_kv_stream
  logical_bytes:
    naive per-Q-head K/V reads
  modeled_dram_miss_bytes:
    qh4 GQA-aware K/V stream, where one KV head feeds four Q heads
  modeled_hit_rate:
    expected reuse from GQA grouping, not a real hardware counter
```

Example:

```text
p32768 exact qh4:
  modeled_dram_miss_bytes ~= 1024 GiB per attention layer
  measured attention core ~= byte floor at ~174 GB/s
```

If measured runtime is close to this floor, micro-optimizing cache misses will
not close the llama.cpp long-prefill gap. The remaining exact options are:

```text
1. true query-block K/V reuse without register-pressure collapse
2. exact lower-precision KV storage/layout that halves compulsory bytes
3. hardware matrix/tensor attention schedule that changes compute balance
```

Rejected exact micro-optimization:

```text
late-gate qh4 SIMD32 vec8
  bitexact
  slower at p8192
  reverted
```

Rejected exact layout probe:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV=1
  bitexact
  slower at p8192/p16384
  keep as opt-in sweep evidence, not accepted
```

Practical lesson:

```text
Cache-friendly is empirical. Interleaving K/V looked plausible because each
online-softmax step consumes K and V for the same key. The measured result was
worse, likely because separate contiguous K and V streams coalesce better for
the 32-lane * 8-dim ownership pattern.
```

Rejected quantized K/V probe:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV=1
  one fp16 scale per token/KV-head
  int8 K and int8 V consumed directly by the attention loop
  no full dequant tensor
  slower at p8192/p16384 than FP16 K/V
```

Quantization rule:

```text
Reducing bytes is not sufficient. The quantized format must match a fast
hardware consumption path. Scalar int8 loads plus float conversion in the inner
key loop can lose even when the byte model improves.
```

Rejected V-only quantization probe:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V=1
  K stays FP16
  V becomes int8 with one fp16 scale per token/KV-head
  lower drift than int8 K/V
  still slower than FP16 K/V at p8192/p16384
```

Attention quantization rule update:

```text
Do not assume memory-byte reduction beats FP16. For the current qh4 SIMD32
schedule, scalar int8 consumption loses even when only V is quantized. Future
attempts need packed/vectorized consumption or a hardware matrix path.
```

Rejected PACK4 int8 V probe:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4=1
  bitexact versus scalar int8 V
  slower than scalar int8 V
  much slower than FP16 K/V
```

PACK4 lesson:

```text
Reducing load instructions by loading one 32-bit word for four lanes did not
help. The simd_broadcast and bit-unpack path is too expensive inside the
per-key loop. Prefer ownership/layouts where each lane can consume its data
directly, or move quantized attention to a backend with native dot/unpack
support.
```

HALFACC candidate:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC=1
  full causal context
  q/k/gate/value accumulation in half
  m/l online softmax state in float
  approximate, not exact
  p8192/p16384/p32768 attention-core speedup about 1.5x
```

HALFACC rule:

```text
When a kernel is near the byte floor but still has heavy register/accumulator
state, test reduced accumulator precision before inventing more scalar
quantized memory formats. Promote only with explicit model-quality gates.
```

HALFDOT candidate:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT=1
  full causal context
  q/k/gate/value accumulation in half
  q*k score partial in half
  m/l online softmax state in float
  approximate, not exact
  p8192/p16384/p32768 attention-core speedup over HALFACC about 1.05-1.10x
```

HALFDOT rule:

```text
Precision is a schedule parameter. On the current SIMD32 attention kernel,
half score partials provide a real hotloop win with roughly the same raw
attention drift class as HALFACC. Treat this as an approximate profile only:
it may beat llama.cpp at p16k, but p32k still needs structural attention work.
```

WINDOW_HALFDOT candidate:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT=<tokens>
  local-window sparse attention
  half q/k/gate/value accumulation
  half q*k score partial
  m/l online softmax state in float
  approximate, not exact
```

WINDOW_HALFDOT rule:

```text
Use this only as an explicit approximate long-context mode. It can beat the
llama.cpp pp16k/pp32k reference projections because it reduces K/V visits, but
it is not a cache-miss fix for exact attention. It changes model semantics by
dropping old keys/values and must be validated with task-quality gates.
```

## Delta Scan SIMD Lessons

Exact accepted Delta scan baseline:

```text
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1
  exact scalar row accumulation
  row state cached in thread registers
  accepted profile
```

Approximate SIMD candidate:

```text
CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1
  one SIMDgroup owns one recurrent-state row
  32 lanes
  4 state columns per lane
  Q/K loaded once into threadgroup memory for four rows
  faster at p512/p4096/p16384, but not exact
```

Measured stack effect with MPS sidecars:

```text
p512:   -13.2842% median_s
p4096:  -13.0158% median_s
p16384: -12.7809% median_s
```

Correctness gate:

```text
p4096 hidden dump:
  mean_abs_error: 0.001943609
  rms_error:      0.002542885
  max_abs_error:  0.046875000
```

Delta scan rule:

```text
SIMD width is not the optimization by itself. SIMD row ownership must be paired
with a data-sharing layout, otherwise the kernel can reload the same token-local
Q/K vectors and lose its long-context benefit. In recurrent code, SIMDgroup
reductions also change accumulation order, so they belong to an approximate or
quantized profile unless full logits/greedy quality gates explicitly accept the
drift.
```

Do not promote `LANES4` or `LANES4_SHAREDQK` into the exact accepted profile
without a model-level quality gate. Use it as a fast approximate path for
quantization-style experiments.

Guardrail:

```text
Do not combine CTOX_QWEN35_DELTA_SCAN_GATED_NORM with block/direct/lanes4 scan
variants or CTOX_QWEN35_MPS_QKVZ_DIRECT unless a matching fused kernel exists.
The dispatch shape and z-buffer ownership differ.
```

Rejected standalone norm probe:

```text
CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1
  one SIMDgroup per token/head
  four columns per lane
  lower barrier count than 128-thread tree reduction
  p4096 slower and checksum drift
```

Gated-norm rule:

```text
Do not chase small post-scan reductions before the recurrent scan itself is
under control. SIMD32x4 changed the norm reduction order and did not produce an
integrated win. Prioritize scan state math/layout, not isolated RMSNorm barrier
cleanup.
```

## Tool Surface Rule

All performance tools must expose the same backend/sidecar surface as the
benchmark they wrap.

```text
bench_metalpack_prefill_delta3_ffn_superblock:
  accepts MPS FFN, DeltaProject, DeltaOut sidecars

compare_delta_stack_candidate:
  must pass the same sidecars for candidate comparisons

profile_metalpack_prefill_delta_stack:
  must pass the same sidecars for phase attribution

autotune_metalpack_prefill_delta_stack:
  must pass the same sidecars for coordinate search

memory_forensics:
  must pass the same sidecars for full-prefill estimates
```

If one tool lacks a backend argument, its output is not comparable to the
current accepted pipeline.

Scan-family comparisons:

```text
tools/run_delta_scan_family_sweep.sh --tokens 512,4096,16384
```

Use this before claiming a Delta scan layout win. It runs reset-based serial
comparisons for rowcache, direct, block64, block32, auto, and the approximate
lanes4_sharedqk fast control.

For mutually-exclusive env flags, compare tools need a reset mode:

```text
tools/compare_delta_stack_candidate.sh --candidate-reset-tuning-env
```

Without reset, accepted flags such as `QKVZ_MMA128` can remain active and hide a
candidate like `QKVZ_MMA64`. Treat any comparison of mutually-exclusive flags
without reset as invalid.

Rejected exact rowgroup auto probe:

```text
CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
  block32 below 4096 tokens
  block64 from 4096 tokens
  exact checksum
  p4096/p16384 regressed in paired sidecar measurement
```

Rowgroup rule:

```text
Do not expect rowgroup-size thresholding to close the scan gap. Block32 and
block64 are too close and noisy. Exact scan optimization needs a structural
change to state update math/layout or a backend that can consume the recurrence
more efficiently.
```

## Isolated Scan Forensics Rule

The recurrent DeltaNet scan must be measured both integrated and isolated.
Integrated Delta-stack timing answers "does this help the real pipeline?";
isolated scan timing answers "did the scan layout itself improve?"

Use:

```text
tools/run_delta_scan_isolated_sweep.sh --tokens 512,4096,16384
```

The tool reports:

```text
kernel
grid / threads
bytes_moved_estimate
median_s
tok_s
vs_block32
max_abs_error_out/state
```

Interpretation rule:

```text
Do not compare plain-scan and rowcache-scan by one naive effective_GB/s number.
Plain scan streams the recurrent state per token and can report high modeled
GB/s. Rowcache deliberately removes that repeated DRAM traffic, so the useful
metrics are median_s, tok/s, speedup versus accepted rowcache_block32, and a
separate byte model that distinguishes compulsory traffic from avoided state
streaming.
```

Latest isolated sweep:

```text
p512:
  rowcache_block32:       0.00182112 s, 281145 tok/s, 1.00x
  lanes4_sharedqk_approx: 0.00125546 s, 407819 tok/s, 1.45x

p4096:
  rowcache_block32:       0.0135023 s, 303357 tok/s, 1.00x
  lanes4_sharedqk_approx: 0.00996406 s, 411077 tok/s, 1.36x

p16384:
  rowcache_block32:       0.0544857 s, 300702 tok/s, 1.00x
  lanes4_sharedqk_approx: 0.0409480 s, 400117 tok/s, 1.33x
```

Decision:

```text
rowcache_block32 remains the exact accepted scan baseline.
lanes4_sharedqk remains an opt-in approximate fast control.
```

Correctness rule:

```text
Short synthetic scan validation is not enough for promotion. A scan candidate
that passes isolated q/k/v validation can still drift in full-stack hidden
state. Promotion requires hidden dump, logits, and greedy parity for exact
profiles, or an explicit quantization/approximation acceptance gate.
```
