# Kernel Dev Wiki

Operational templates for Qwen3.5 Metal kernel research.

Use these files together with:

```text
../../RESEARCH_LOG.md
../../KERNEL_DEV_HANDBOOK.md
```

## Current Outcome

The kernel-dev docs now treat reference comparison as a first-class artifact.
Use the report tools before claiming a new state:

```text
tools/prefill_reference_report.py
tools/run_decode_regression_matrix.sh --iterations 3 --rounds 2 <metalpack-dir>
```

Current documented status:

```text
exact prefill:
  p4096/p16384/p32768 beats llama.cpp BF16/Metal in the current forensics row

approximate prefill:
  faster again with lanes4_sharedqk, but hidden-state drift keeps it opt-in

decode:
  cooled tg128/tg512 can beat llama.cpp, but decode promotion still requires
  alternating serial regression matrices because sustained runs are sensitive to
  thermal/order/storage/sync state
```

This distinction matters for the docs:

```text
accepted-profile win:
  exact or explicitly accepted quality contract, decision record, promotion gate

forensics win:
  useful directional evidence, but not necessarily the runtime default

approximate win:
  speed evidence only until quality/error budgets are accepted
```

## Files

```text
EXPERIMENT_TEMPLATE.md
  Fill this before implementing a kernel or layout experiment.

DECISION_RECORD_TEMPLATE.md
  Fill this after measurements to record accept/reject/opt-in decisions,
  including negative learnings and retry conditions for rejected paths.

BENCHMARK_PROTOCOL.md
  Use this to run reproducible serial benchmarks without contaminating results.

HARDWARE_BACKEND_GRID.md
  Required hardware/backend planning surface. Use it before adding kernels so
  each candidate targets GPU MSL SIMDgroup, GPU MPS Matrix, Metal 4 tensor,
  CPU NEON/SME, Core ML/ANE, or a justified hybrid path.

CACHE_FORENSICS_CHECKLIST.md
  Use this when the hypothesis involves memory layout, cache misses, scratch,
  weight streaming, or token tiling.

FORENSICS_RECORD_TEMPLATE.md
  Fill this after a measurement claims a cache, memory, layout, scratch, or
  weight-streaming effect.

AUTOTUNE_RECORD_TEMPLATE.md
  Fill this after a parameter search over layouts, tiles, chunks, dispatch
  shapes, or env-gated implementation variants.

ACCEPTED_PROFILE_UPDATE_TEMPLATE.md
  Fill this only after an accepted decision passes the promotion gate.

MEASUREMENT_RECORD_TEMPLATE.md
  Fill this to link a captured benchmark run directory to an experiment.

FLAG_LIFECYCLE_TEMPLATE.md
  Use this for every new `CTOX_QWEN35_*` flag before it can become a default.

accepted_profile.env
  Source of truth for the conservative accepted baseline env flags.
```

## Tools

```text
../../tools/new_kernel_experiment.sh <slug> [metalpack-dir]
  Scaffold a timestamped experiment record with a reproducibility run manifest.

../../tools/validate_kernel_experiment.sh [--strict] <experiment.md>
  Validate required fields. Use --strict before an experiment supports an
  accept/reject decision.

../../tools/kernel_dev_doctor.sh [--strict-experiments]
  Check the kernel-dev wiki/tooling layer itself. Does not run benchmarks.

../../tools/run_accepted_profile.sh <command> [args...]
  Run a command with the conservative accepted baseline profile.

../../tools/validate_accepted_profile.sh [accepted_profile.env]
  Validate accepted profile syntax, CTOX_QWEN35_* scope, and duplicate flags.

../../tools/run_measurement_pack.sh [--dry-run] [--capture] <pack> <metalpack-dir>
  Run standardized smoke/candidate/acceptance/long-context measurement packs.

../../tools/capture_roofline_baseline.sh [--output-dir DIR]
  Capture local stream and operational matmul roofline probes. Use the generated
  roofline.env values as sustained-bandwidth inputs for gap analysis.

../../tools/capture_hardware_feature_matrix.sh [output-dir]
  Capture the local Mac model, chip, GPU cores, Metal support, CPU ISA feature
  sysctls, and available Metal counter sets. Use this before promoting kernels
  or changing backend assumptions.

../../tools/run_hardware_backend_shootout.sh <metalpack-dir> [tokens] [iterations] [output-dir]
  Run a serial backend evidence pack: hardware feature matrix, CPU quant/SIMD
  probe with SME/SME2 compile-feature disclosure, MPSMatrix GEMM probes, and
  Core ML / ANE artifact availability. This separates "hardware exists" from
  "current pipeline uses it".

../../tools/analyze_hardware_backend_shootout.py <shootout.md>
  Summarize SME2 availability/use status, CPU quant timings, best MPSMatrix
  throughput, and whether Core ML / ANE is measurable.

../../tools/run_sme2_smoke_probe.sh
  Compile and execute a minimal ACLE SME/SME2 smoke probe. It verifies
  streaming-mode and ZA-zero code can run on the CPU and emits disassembly
  evidence such as `smstart`, `zero {za}`, and `smstop`. This does not make SME2
  part of the model hot path by itself.

../../tools/run_sme2_mopa_probe.sh [repeats] [iterations] [warmup]
  Compile and execute a minimal SME2 int8 MOPA probe. It verifies `smopa`
  generation and execution for ZA int8 outer-product accumulation. This is a
  microkernel evidence point, not a complete Qwen matmul backend.

../../tools/run_sme2_i8_tile_probe.sh [tokens] [rows] [k] [iterations] [warmup]
  Compile and execute a Qwen-shape-near SME2 int8 tile stream probe. It streams
  token/output/K-shaped panels through `smopa`, stores ZA rows, and reports
  MOPA/s plus modeled stream GB/s. This is still not a layout-correct Qwen
  matmul backend; it is a stronger CPU SME2 feasibility probe than the minimal
  repeated-MOPA smoke test.

../../tools/run_static_int8_matmul_autotune.sh [tokens] [rows] [iterations] [warmup]
  Serially sweep the static INT8 Metal matmul probe across row tiles and quant
  group sizes. Use it as a lightweight layout/schedule discovery tool before
  changing the accepted profile or writing another fixed INT8 kernel.

../../tools/analyze_static_int8_autotune.py <output> [--reference-median-s S]
  Summarize a static INT8 autotune run, report the best/worst candidate, and
  optionally compare the best candidate against a known reference median.

../../tools/prefill_reference_report.py
  Print the current curated prefill projections against llama.cpp BF16/Metal.
  It separates exact-ish sidecar, approximate precision, and sparse-window rows
  so approximate speedups are not confused with accepted-profile wins.

../../tools/run_decode_regression_matrix.sh [--sizes 128,512] [--iterations N] [--rounds N] <metalpack-dir>
  Run the serial decode regression guard across accepted, no-Split-K, rowcache,
  and no-Split-K+rowcache variants. Use at least --iterations 3 --rounds 2 for
  promotion decisions; tg4 is not promotion evidence. Add --storage-sweep to
  include shared-weight variants and --sync-sweep to include async command
  queuing variants.

../../tools/exact_attention_traffic_report.py [--tokens csv] [--sustained-gb-s B]
  Model exact qh4 prefill attention K/V traffic for query-block sizes and list
  curated measured exact candidates. Use this before writing another attention
  kernel; byte wins are only useful if the schedule survives register pressure.

../../tools/run_attention_qk_mps_probe.sh [tokens-csv] [iterations] [warmup] [output-dir]
  Serially benchmark dense per-head QK matrix shapes `tokens x tokens x 256`
  through MPSMatrix. This is only upper-bound evidence for a future tiled exact
  attention path; it materializes dense scores and does not perform softmax/V.

../../tools/analyze_attention_qk_mps_probe.py <report.md>
  Summarize QK MPS probe output and estimate the cost of eight Q heads across
  six full-attention layers.

../../tools/plan_tiled_attention.py [--tokens N] [--q-tiles csv] [--k-tiles csv]
  Plan scratch score-tile sizes, Q/K/V tile bytes, and causal tile-pair counts
  for a future exact tiled QK-softmax-V prototype.

../../tools/run_tiled_attention_qk_mps_prototype.sh [tokens] [q_tile] [k_tile] [iterations] [warmup]
  Compile and run a synthetic MPSMatrix QK tile prototype. It repeatedly encodes
  causal QK tile GEMMs in one command buffer and reports encode rate and TFLOPS.
  This proves tile/encode feasibility only; it has no softmax, V accumulation,
  real Q/K slicing, or accepted-profile semantics.

../../tools/run_tiled_attention_qk_mps_grid.sh [tokens] [iterations] [warmup] [output]
  Serially sweep the first exact-attention QK tile grid. Keep this run isolated
  from other benchmarks because command-buffer encode overhead is part of the
  measurement.

../../tools/analyze_tiled_attention_qk_mps_grid.py <output>
  Rank tiled QK MPS candidates by median wall time and expose causal tile-pair
  counts, effective TFLOPS, MPS encode rate, and a QK-only projection for eight
  Q heads across six full-attention layers.

../../tools/run_tiled_attention_full_mps_prototype.sh [tokens] [q_tile] [k_tile] [iterations] [warmup] [heads_per_group] [matrix_origins] [quality_check]
  Compile and run the synthetic full tiled-attention stage prototype:
  MPSMatrix QK, SIMD32 block-softmax update, MPSMatrix P*V, and online combine.
  It defaults to `heads_per_group=4` for Qwen GQA and `matrix_origins=1` to
  emulate real Q/K/V tile slicing without copy kernels. `quality_check=1`
  compares sparse output points against a CPU exact reference. It is still
  synthetic data and not an accepted-profile implementation.

../../tools/run_prefill_attention_backend_matrix.sh [--sizes 4096,16384,32768] <metalpack-dir>
  Serially compare the real accepted prefill attention-core path with the Rust
  MPS tiled QK-softmax-PV prototype. This is not an accepted-profile benchmark:
  the MPS tiled side is synthetic Qwen-layout bridge evidence that packs and
  measures both KV groups, but it still does not include real QKV projection,
  O projection, or full hidden-dump parity.

../../tools/run_prefill_mps_tiled_projection.sh [--sizes 4096,16384,32768] <metalpack-dir>
  Measure the accepted QH4 attention core and the opt-in exact MPS tiled
  attention core, then project full-prefill impact for Qwen's six full-attention
  layers. Use this after changing tiled attention, MPS tile sizes, or accepted
  attention flags.

../../tools/run_mps_matrix_probe.sh [m] [n] [k] [iterations] [warmup]
  Compile and run a Swift/MPS fp16 GEMM probe for M5 matrix-backend comparison.
  Use this to compare Apple framework matrix throughput against handwritten MSL
  SIMDgroup/MMA kernels for projection, FFN, and LM-head shapes.

../../tools/run_mps_ffn_block_probe.sh [tokens] [hidden] [intermediate] [iterations] [warmup]
  Run a hybrid MPSMatrix + MSL SwiGLU FFN block probe. This tests whether MPS
  matrix speed survives a realistic Gate+Up -> SwiGLU -> Down command-buffer
  composition.

../../tools/run_mps_ffn_metalpack_probe.sh <metalpack-dir> [layer] [tokens] [iterations] [warmup]
  Run the same hybrid FFN block using real Qwen metalpack layer weights,
  converted once from fp16_row_tiled to MPS-compatible matrix layout.

target/release/pack_mps_ffn_sidecar <source.metalpack-dir> <output.mps-ffn-dir>
  Build a persistent MPS-compatible FFN sidecar pack with per-layer
  gate_up[1024,7168] and down[3584,1024] fp16 row-major matrices.

../../tools/run_mps_ffn_sidecar_probe.sh <mps-ffn-sidecar-dir> [layer] [tokens] [iterations] [warmup]
  Run the hybrid MPS FFN block directly from the persistent sidecar without
  per-run transposition from fp16_row_tiled.

target/release/bench_mps_ffn_sidecar_runtime <mps-ffn-sidecar-dir> [layer] [tokens] [iterations] [warmup]
  Run the same persistent FFN sidecar through the Rust runtime's C-ABI MPS
  bridge, encoding MPS Gate+Up, MSL SwiGLU, and MPS Down in one command buffer.

../../tools/run_mps_deltanet_project_probe.sh [tokens] [hidden] [qkv_rows] [z_rows] [iterations] [warmup]
  Run a synthetic MPSMatrix probe for the combined DeltaNet QKV+Z projection
  shape x[tokens,1024] * qkvz[1024,8192].

target/release/pack_mps_delta_project_sidecar <source.metalpack-dir> <output.mps-delta-project-dir>
  Build a persistent MPS-compatible DeltaNet projection sidecar with only the
  18 DeltaNet layers and qkvz[1024,8192] fp16 row-major matrices.

../../tools/run_mps_deltanet_project_sidecar_probe.sh <mps-delta-project-sidecar-dir> [layer] [tokens] [iterations] [warmup]
  Run the real Qwen DeltaNet QKV+Z projection from the persistent sidecar
  without per-run transposition from fp16_row_tiled.

../../tools/estimate_mps_ffn_prefill_impact.py [--full-s S] [--llama-tok-s T]
  Estimate p4096 model-wide prefill impact from replacing all 24 FFN matrix
  phases with the measured MPS sidecar block time.

../../tools/quant_error_gate.py <measurement.txt> --max-abs N --mean-abs N [--speedup-min N]
  Validate approximate or quantized candidates against explicit numerical drift
  and speedup thresholds. Use this instead of treating quantization as either
  bitexact or unbounded-error.

../../tools/validate_quant_pipeline.py [--strict] <record.md>
  Validate a QUANT_PIPELINE_TEMPLATE.md record and reject hot-loop conversion
  hazards such as f32->f16->f32, per-token requantization, or materialized full
  dequant tensors in strict mode. Strict mode also requires a concrete target
  compute backend, hardware-feature evidence, and a layout/prefetch contract.

../../tools/validate_metalpack_quant_manifest.py [--strict] <metalpack-dir-or-manifest>
  Validate fp16 row-tiled and static quantized metalpack payload sizes against
  row_tile, col_tile, quant_group_size, and quant_value_bits. Strict mode
  requires at least one quantized entry.

../../tools/run_quant_delta_scan_gate.sh [tokens] [chunk] [iterations] [warmup] [output-dir]
  Compare exact f32x4 and quantized f16x4 DeltaNet chunk-scan prototypes,
  produce real max/mean error metrics, and run quant_error_gate.py.

target/release/bench_metalpack_prefill_delta3_ffn_superblock <metalpack-dir> [start-layer] [tokens] [iterations] [warmup] [delta-layer-count] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-delta-out-sidecar-dir]
  Run the integrated DeltaNet+FFN stack. When the optional MPS FFN sidecar is
  supplied, the benchmark cuts the command buffer into MSL DeltaOut, MPS FFN,
  and MSL fp16 residual phases to test real pipeline integration. When the
  optional MPS Delta project sidecar is also supplied, DeltaNet QKV+Z uses the
  MPS matrix backend and a materialized or direct qkvz bridge before the
  existing MSL conv/scan path. The optional MPS DeltaOut sidecar replaces the
  DeltaNet out projection with MPSMatrix plus an explicit residual add.

../../tools/compare_delta_stack_candidate.sh --candidate-env KEY=VALUE [--mps-ffn-sidecar DIR] [--mps-delta-project-sidecar DIR] [--mps-delta-out-sidecar DIR]
  Alternate accepted-profile and candidate DeltaNet+FFN stack runs to reduce
  thermal/order bias. Pass the MPS sidecars when judging current prefill
  candidates; otherwise the tool measures an obsolete non-sidecar path. Use
  --candidate-reset-tuning-env when testing mutually-exclusive tile/layout
  flags; otherwise accepted-profile flags can remain set and hide the candidate.

target/release/autotune_metalpack_prefill_delta_stack <metalpack-dir> [tokens] [iterations] [warmup] [delta-layer-count] [start-layer] [passes] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-delta-out-sidecar-dir]
  Run serial coordinate-descent tuning for the DeltaNet+FFN stack. Pass all
  three sidecars when tuning the current prefill pipeline; otherwise the search
  optimizes a stale non-sidecar path.

../../tools/run_delta_scan_family_sweep.sh [--tokens N[,N...]]
  Run serial, reset-based scan-family comparisons on the current MPS sidecar
  pipeline. Use this before claiming a scan-layout win; it includes exact
  rowcache variants and the approximate lanes4_sharedqk negative/fast control.

target/release/bench_metalpack_prefill_delta_scan <metalpack-dir> [layer] [tokens] [iterations] [warmup] [validate_tokens]
  Run the isolated recurrent DeltaNet scan benchmark. The output includes the
  selected kernel, grid/thread shape, modeled bytes, tok/s inputs for sweep
  tools, and synthetic CPU reference errors.

../../tools/run_delta_scan_isolated_sweep.sh [--tokens N[,N...]]
  Run serial isolated DeltaNet scan sweeps outside projection/FFN/attention
  noise. Use this when tuning scan cache/layout/SIMD variants; compare by
  median_s, tok/s, and vs_block32, not by a single naive GB/s number.

target/release/memory_forensics <metalpack-dir> [tokens=512] [iterations=3] [sustained-gb-s=90] [mps-ffn-sidecar-dir] [mps-delta-project-sidecar-dir] [mps-attention-out-sidecar-dir] [mps-delta-out-sidecar-dir]
  Run model-component forensics and full-prefill estimates. Optional sidecar
  arguments route the Delta18+FFN row through the integrated MPS FFN/QKVZ and
  optional DeltaOut path, route Attention O through MPS when supplied, and keep
  full-prefill estimates aligned with the active backend. Use
  CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1 only for approximate
  SIMD32 Delta scan forensics; it is not an exact accepted-profile row.

target/release/cache_analysis --tokens N --sustained-gb-s B [--csv]
  Emit the cache/memory byte model, including
  attention.prefill_kv_stream for exact qh4 long-prefill Attention. Use this
  before claiming a cache-miss optimization: if measured runtime is already near
  the modeled compulsory K/V stream floor, the next exact win must reduce bytes
  or change the schedule, not merely reshuffle the same stream.

../../tools/run_attention_window_quality_sweep.sh <metalpack-dir> <tokens> [layer] [iterations] [mps-attention-out-sidecar-dir] [windows_csv]
  Run serial exact-vs-window attention benchmarks. It dumps exact qh4 SIMD32
  vec8 attention once, runs each
  CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW candidate, and compares raw
  attention drift with mean_abs, rms, max_abs, checksum_delta, and mismatch
  count.

../../tools/profile_attention_variant_stages.sh <metalpack-dir> <tokens> <layer> <iterations> <candidate-env KEY=VALUE> [mps-attention-out-sidecar-dir]
  Run serial cumulative stage profiles for accepted qh4 SIMD32 vec8 and one
  candidate attention variant. It reports prepare_s and attention_only_s so a
  candidate loss can be attributed to prepare/packing or the attention hot loop.

../../tools/run_matrix_backend_shootout.sh <metalpack-dir> [tokens] [iterations] [output-dir]
  Run an accepted-profile-aware MPS-vs-MSL matrix backend shootout for Qwen
  projection, FFN, and DeltaOut shapes.

../../tools/analyze_matrix_backend_grid.py <shootout.md>
  Summarize MPS-vs-MSL ratios and select the next matrix backend priority.

../../tools/run_cpu_quant_probe.sh [tokens] [rows] [k] [iterations] [warmup]
  Build and run a C/NEON DotProd baseline for int8 and q4-unpack CPU quant
  layouts, while disclosing I8MM/BF16/SME/SME2 compile-feature availability.
  This is a CPU backend-column probe, not a model hotpath by itself.

target/release/bench_static_int8_matmul [tokens] [rows] [iterations] [warmup] [quant_group_size]
  Synthetic probe for the static int8_row_tiled pack format. It consumes packed
  int8 weights directly and performs only lane-local in-dot dequantization.
  Treat it as a kernel-schedule probe, not as an accepted model path.

../../tools/capture_measurement_output.sh [--accepted-profile] [--output-dir DIR] [--label LABEL] -- <command> [args...]
  Run one measurement under an exclusive local lock and capture stdout, stderr,
  normalized fields, exit code, command, git state, and accepted-profile hash.

../../tools/new_measurement_record.sh <experiment.md> <capture-dir> <kind> [slug]
  Link a captured measurement directory to an experiment.

../../tools/validate_measurement_record.sh [--strict] <measurement.md>
  Validate captured measurement references and normalized runtime fields.

../../tools/list_measurement_records.sh [--markdown]
  List generated measurement records and validation status.

../../tools/update_measurement_index.sh
  Regenerate docs/kernel-dev/measurements/INDEX.md from measurement records.

../../tools/list_kernel_experiments.sh [--markdown]
  List generated experiment records and default/strict validation status.

../../tools/update_kernel_experiment_index.sh
  Regenerate docs/kernel-dev/experiments/INDEX.md from generated records.

../../tools/new_kernel_decision.sh <experiment.md> <decision> [slug]
  Scaffold a decision record from an experiment record.

../../tools/validate_kernel_decision.sh [--strict] <decision.md>
  Validate required decision evidence fields. Use --strict before promoting a
  candidate into accepted_profile.env or closing a rejected path.

../../tools/check_kernel_promotion.sh <decision.md>
  Block promotion unless an accepted decision and its referenced experiment both
  pass strict validation.

../../tools/check_autotune_defaults.sh [accepted-profile.env]
  Compare autotune_metalpack_prefill_delta_stack --print-baseline-env with the
  accepted profile for all DeltaNet+FFN autotuner-managed flags.

../../tools/new_cache_forensics_record.sh <experiment.md> <op> [slug]
  Scaffold a cache/memory forensics record linked to an experiment.

../../tools/validate_cache_forensics.sh [--strict] <forensics.md>
  Validate byte-model, runtime, evidence-level, and interpretation fields.

../../tools/fill_forensics_record_from_output.sh <benchmark-output.txt> <forensics.md>
  Fill extractable runtime fields from existing benchmark stdout.

../../tools/analyze_bandwidth_gap.sh --normalized normalized.txt --modeled-bytes BYTES
  Classify a captured benchmark against a byte-model floor and sustained
  bandwidth assumption. Use --cache-csv plus --op to pull modeled bytes from
  target/release/cache_analysis --csv output.

../../tools/analyze_memory_forensics_gaps.sh <memory-forensics-stdout.txt>
  Parse target/release/memory_forensics output into a ranked per-scope gap
  table without rerunning benchmarks.

../../tools/analyze_delta_profile_gaps.sh <profile-stdout.txt> <cache-analysis.csv>
  Combine DeltaNet+FFN prefix profiler output with modeled bytes to rank
  phase-level roofline gaps.

../../tools/compare_delta_stack_candidate.sh --candidate-env KEY=VALUE
  Alternating paired comparison between accepted-profile and candidate
  DeltaNet+FFN stack runs. Use this before promoting sub-percent wins.

../../tools/list_cache_forensics.sh [--markdown]
  List generated cache/memory forensics records and validation status.

../../tools/update_cache_forensics_index.sh
  Regenerate docs/kernel-dev/forensics/INDEX.md from forensics records.

../../tools/new_autotune_record.sh <experiment.md> <parameter-family> [slug]
  Scaffold an autotune evidence record linked to an experiment.

../../tools/validate_autotune_record.sh [--strict] <autotune.md>
  Validate search-space, best-candidate, correctness, and token-sweep evidence.

../../tools/list_autotune_records.sh [--markdown]
  List generated autotune records and validation status.

../../tools/update_autotune_index.sh
  Regenerate docs/kernel-dev/autotune/INDEX.md from autotune records.

../../tools/normalize_benchmark_output.sh <benchmark-output.txt>
  Normalize existing benchmark stdout into canonical evidence fields.

../../tools/fill_autotune_record_from_output.sh <autotune-output.txt> <autotune-record.md>
  Fill extractable autotune fields from existing autotuner stdout.

../../tools/show_kernel_evidence_bundle.sh <decision.md>
  Show default/strict validation status for a decision and its linked experiment,
  forensics, and autotune records.

../../tools/propose_accepted_profile_update.sh <decision.md> [slug]
  Create an accepted-profile update proposal after the promotion gate passes.
  It does not edit accepted_profile.env.

../../tools/validate_accepted_profile_update.sh [--strict] <profile-update.md>
  Validate accepted-profile update proposals and re-check promotion in strict mode.

../../tools/list_accepted_profile_updates.sh [--markdown]
  List accepted-profile update proposals and validation status.

../../tools/update_accepted_profile_update_index.sh
  Regenerate docs/kernel-dev/profile-updates/INDEX.md from proposals.

../../tools/list_kernel_decisions.sh [--markdown]
  List generated decision records and default/strict validation status.

../../tools/update_kernel_decision_index.sh
  Regenerate docs/kernel-dev/decisions/INDEX.md from decision records.
```

## Workflow

```text
1. Run tools/new_kernel_experiment.sh <slug> [metalpack] or copy EXPERIMENT_TEMPLATE.md.
2. Implement the smallest env-gated candidate.
3. Run tools/validate_kernel_experiment.sh <record> before coding.
4. Capture or refresh the hardware feature matrix with tools/capture_hardware_feature_matrix.sh when hardware, OS/runtime, or backend assumptions change.
5. Capture or refresh the device roofline with tools/capture_roofline_baseline.sh when hardware, OS, thermal state, or backend assumptions change.
6. Run BENCHMARK_PROTOCOL.md and capture real commands with tools/capture_measurement_output.sh.
7. Link captured runs with tools/new_measurement_record.sh and validate them.
8. Run CACHE_FORENSICS_CHECKLIST.md for memory-sensitive changes.
9. Fill FLAG_LIFECYCLE_TEMPLATE.md for new env-gated paths.
10. For quantized candidates, fill QUANT_PIPELINE_TEMPLATE.md and run tools/validate_quant_pipeline.py --strict <record>.
11. Run tools/validate_kernel_experiment.sh --strict <record> before decision.
12. Record the outcome with DECISION_RECORD_TEMPLATE.md. Rejections must include
    failure_mode, root_cause, do_not_repeat, and retry_only_if.
13. Run tools/list_kernel_experiments.sh to inspect open records.
14. Run tools/new_cache_forensics_record.sh for memory/cache/layout claims.
15. Run tools/validate_cache_forensics.sh --strict <forensics> before decision.
16. Use tools/normalize_benchmark_output.sh and tools/fill_forensics_record_from_output.sh to transfer runtime fields from existing stdout.
17. Run tools/analyze_bandwidth_gap.sh, tools/analyze_memory_forensics_gaps.sh, or tools/analyze_delta_profile_gaps.sh before choosing the next prefill target.
16. Run tools/new_autotune_record.sh for parameter searches.
17. Run tools/validate_autotune_record.sh --strict <autotune> before using a searched candidate.
18. Use tools/fill_autotune_record_from_output.sh to transfer existing autotuner stdout into records.
19. Run tools/new_kernel_decision.sh after a measurement reaches a decision.
20. Link strict evidence in the decision's forensics_record and, if search_based: yes, autotune_record fields.
21. Run tools/validate_kernel_decision.sh --strict <decision> before promotion.
22. Run tools/show_kernel_evidence_bundle.sh <decision> to inspect all linked evidence.
23. Run tools/check_kernel_promotion.sh <decision> before changing accepted_profile.env.
24. Run tools/propose_accepted_profile_update.sh <decision> to create the review artifact.
25. Run tools/update_kernel_experiment_index.sh, tools/update_measurement_index.sh, tools/update_cache_forensics_index.sh, tools/update_autotune_index.sh, tools/update_kernel_decision_index.sh, and tools/update_accepted_profile_update_index.sh after record changes.
26. Promote only if the decision is accepted and the handbook criteria are met.
27. Record dead ends in RESEARCH_LOG.md and update KERNEL_DEV_HANDBOOK.md when
    the failure changes strategy.
28. Run tools/kernel_dev_doctor.sh after editing this knowledge base.
```

Subagents may help fill in code-reading and literature sections. They must not
run performance benchmarks.

## Scaffold Command

```text
tools/new_kernel_experiment.sh <slug> [metalpack-dir]
```

The scaffold fills date, owner, git state, macOS/device information, accepted
profile path/hash, metalpack hashes, output CSV/dump defaults, and an env dump
path. It also regenerates `docs/kernel-dev/experiments/INDEX.md`.
