# ctox-qwen35-08b-metal-probe

Research crate for the Qwen3.5-0.8B Metal optimization probe.

This crate exists to validate the CTOX strategy on the smallest relevant
Qwen3.5 hybrid model before any lessons are applied to the 27B or 35B crates.
It is intentionally self-contained and not wired into the root CTOX runtime.

## Contract

```text
model:       Qwen/Qwen3.5-0.8B
mode:        text-only first
batch:       1
path:        decode first
backend:     Metal GPU hot path
ANE/NPU:     separate Core ML benchmark track
CPU:         orchestration only
```

This is still a research prototype, but the real Qwen3.5-0.8B layered Metal
decode path now has captured greedy parity against MLX for the raw-token probe.

## Current Reference Status

The current documented reference comparison is no longer "below llama.cpp" for
prefill:

```text
exact prefill forensics:
  p4096:   4801.88 tok/s vs llama.cpp 2852.70 = 1.683x
  p16384:  4096.00 tok/s vs llama.cpp 2065.71 = 1.983x
  p32768:  3383.73 tok/s vs llama.cpp 1325.20 = 2.553x

approximate Delta scan fast control:
  p4096:   5305.70 tok/s vs llama.cpp 2852.70 = 1.860x
  p16384:  4396.03 tok/s vs llama.cpp 2065.71 = 2.128x
  p32768:  3594.16 tok/s vs llama.cpp 1325.20 = 2.712x

cooled decode measurements:
  tg128: 55.91 tok/s vs llama.cpp 52.98 = 1.055x
  tg512: 55.66 tok/s vs llama.cpp 44.77 = 1.243x
```

Interpretation:

```text
prefill:
  exact forensics beats llama.cpp at 4k/16k/32k

decode:
  can beat llama.cpp, but still needs robust alternating tg128/tg512 acceptance
  runs before another decode default is promoted

approximate rows:
  useful speed controls, not accepted-profile wins unless quality gates accept
  the drift
```

Source commands:

```text
tools/prefill_reference_report.py
tools/run_decode_regression_matrix.sh --iterations 3 --rounds 2 \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

## Research Knowledge Base

- `RESEARCH_LOG.md` is the chronological lab notebook with measurements,
  rejected hypotheses, dead ends, invalid measurements, and decision records.
- `KERNEL_DEV_HANDBOOK.md` is the distilled kernel-development playbook:
  architecture rules, cache/memory methodology, accepted/rejected optimization
  patterns, autotuning discipline, and the current definition of done.
- `docs/kernel-dev/` contains operational templates for new experiments,
  benchmark protocols, decision records, and cache/memory forensics.
- `tools/new_kernel_experiment.sh` scaffolds timestamped experiment records
  with a reproducibility run manifest.
- `tools/validate_kernel_experiment.sh` checks experiment records before coding
  and in strict mode before accept/reject decisions.
- `tools/kernel_dev_doctor.sh` validates the kernel-dev wiki/tooling layer
  without running performance benchmarks.
- `docs/kernel-dev/accepted_profile.env` and `tools/run_accepted_profile.sh`
  centralize the conservative accepted baseline environment.
- `tools/validate_accepted_profile.sh` checks accepted-profile syntax and
  guards the baseline env scope.
- `tools/check_autotune_defaults.sh` checks that the DeltaNet+FFN autotuner
  baseline flags have not drifted away from `accepted_profile.env`.
- `tools/run_measurement_pack.sh` runs standardized smoke/candidate/acceptance
  measurement packs.
- `tools/compare_delta_stack_candidate.sh` alternates accepted-profile and
  candidate DeltaNet+FFN stack runs to reduce order/thermal bias for small wins.
- `tools/capture_roofline_baseline.sh` captures local stream and operational
  matmul roofline probes into a reusable baseline directory.
- `tools/capture_measurement_output.sh` runs one measurement under a local lock
  and captures raw stdout/stderr plus normalized evidence fields.
- `tools/new_measurement_record.sh` links captured measurement directories to
  experiment records.
- `tools/list_kernel_experiments.sh` summarizes generated experiment records
  and their validation status.
- `tools/update_kernel_experiment_index.sh` regenerates the experiment index.
- `tools/new_kernel_decision.sh` scaffolds decision records from experiments.
- `tools/validate_kernel_decision.sh` checks decision records before promotion
  or final rejection.
- `tools/check_kernel_promotion.sh` blocks accepted-profile changes unless the
  accepted decision and its experiment both pass strict validation.
- `tools/new_cache_forensics_record.sh` and `tools/validate_cache_forensics.sh`
  turn cache/memory/layout claims into strict byte-model evidence records.
- `tools/fill_forensics_record_from_output.sh` transfers existing benchmark
  stdout into cache-forensics runtime fields.
- `tools/analyze_bandwidth_gap.sh` classifies normalized benchmark output
  against modeled compulsory DRAM bytes and a sustained-bandwidth roofline.
- `tools/analyze_memory_forensics_gaps.sh` ranks the per-scope gaps emitted by
  `target/release/memory_forensics`.
- `tools/analyze_delta_profile_gaps.sh` combines DeltaNet+FFN prefix profiling
  with `cache_analysis --csv` to rank phase-level roofline gaps.
- `tools/run_mps_ffn_sidecar_probe.sh` and
  `tools/run_mps_deltanet_project_sidecar_probe.sh` compare persistent
  MPS-compatible sidecar layouts against handwritten MSL matrix paths before
  more custom kernel tuning is attempted.
- `target/release/bench_mps_ffn_sidecar_runtime` verifies that the FFN sidecar
  speed survives the Rust runtime bridge and one-command-buffer composition.
- `target/release/bench_metalpack_prefill_delta3_ffn_superblock` accepts an
  optional MPS FFN sidecar argument and optional MPS Delta project sidecar
  argument to test integrated DeltaNet+FFN pipeline speed, including
  command-buffer phase cuts, MPS QKV+Z, MPS FFN, and fp16 residual output.
- `target/release/memory_forensics` accepts the same sidecar pair for the
  Delta18+FFN row so full-prefill estimates and cache/byte buckets stay aligned
  with the active backend.
- `target/release/pack_mps_attention_out_sidecar` and
  `target/release/bench_mps_attention_out_sidecar_runtime` package and validate
  the Attention O projection as an MPSMatrix sidecar before it is integrated
  into `bench_metalpack_prefill_attention_core`.
- `target/release/pack_mps_delta_out_sidecar` packages DeltaNet `out_proj`
  weights in MPSMatrix layout; the DeltaNet+FFN stack benchmark accepts it as a
  third sidecar after MPS FFN and MPS Delta project.
- `target/release/bench_metalpack_prefill_attention_core` accepts an optional
  MPS Attention O sidecar argument and supports
  `CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP={norm,project,prepare,attention}` for
  cumulative stage profiling.
- `CTOX_QWEN35_MPS_QKVZ_DIRECT=1` makes the integrated MPS QKV+Z path feed
  Conv/Split and GatedNorm directly from the combined qkvz matrix instead of
  materializing legacy qkv/z split buffers.
- `CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1` is an approximate opt-in Delta
  scan candidate: one SIMDgroup owns one recurrent-state row, while Q/K are
  loaded once into threadgroup memory for the four rows in the threadgroup. It
  improves MPS-sidecar prefill forensics, but hidden-state drift blocks exact
  accepted-profile promotion.
- `CTOX_QWEN35_ATTENTION_MPS_TILED=1` is the current accepted exact attention
  backend. It uses MPSMatrix QK/PV tiles plus MSL softmax/combine glue instead
  of the older custom per-query QH4 scan.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1` remains available as a fallback
  attention scan schedule: one SIMDgroup owns a full 256-dim head by processing
  8 dimensions per lane, avoiding per-key threadgroup reductions.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096=1` is an approximate,
  opt-in local-window attention candidate for long-prefill speed experiments;
  it is not part of the exact accepted profile.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW=<tokens>` is the parameterized
  version of that sparse/window candidate. Use
  `tools/run_attention_window_quality_sweep.sh` to measure speed and raw
  attention drift against the exact SIMD32 path before treating any window as
  acceptable.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV=1` is a bitexact
  K/V-cache layout probe for the accepted qh4 SIMD32 attention schedule. It is
  currently rejected for promotion because serial p8192/p16384 measurements were
  slower than separate K and V buffers.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV=1` is an approximate int8
  K/V-cache probe with one fp16 scale per token/KV-head. It reduces modeled K/V
  bytes but is currently rejected for promotion because scalar int8 dequant in
  the hot loop was slower than FP16 K/V.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V=1` keeps K in FP16 and
  quantizes only V to int8+scale. It has lower drift than int8 K/V, but is also
  rejected for promotion because the current scalar int8 consumption path is
  still slower than FP16.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4=1` consumes the same
  int8 V layout through packed 32-bit loads and SIMD broadcast. It is bitexact
  to scalar int8 V but rejected because broadcast/unpack is slower.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC=1` is a full-context
  approximate attention candidate that keeps online-softmax `m/l` in FP32 but
  stores q/k/gate/value accumulation in half. It is a strong approximate speed
  candidate, but is not exact.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT=1` extends HALFACC by using
  half score partials for q*k. It is currently the strongest full-context
  approximate attention candidate, but still requires explicit model-quality
  gates and is not part of the accepted exact profile.
- `CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT=<tokens>` combines
  local-window sparse attention with HALFDOT precision. It is currently the
  fastest approximate long-prefill candidate, but it changes model semantics by
  dropping old K/V positions and is not part of the accepted exact profile.
- `CTOX_QWEN35_ATTENTION_QH4_SPLITK{64,128,256,512}=1` are exact-ish qh4
  Split-K probes with log-sum-exp combine. They fix the old Partial-QBLK2 GQA
  overread, but remain rejected because full partial-accumulator scratch traffic
  is slower than the accepted qh4 SIMD32 vec8 path.
- `tools/profile_attention_variant_stages.sh` compares accepted qh4 SIMD32
  attention against one candidate and reports prepare versus attention-only
  timing, so quant/layout losses can be attributed to packing or the hot loop.
- `tools/new_autotune_record.sh` and `tools/validate_autotune_record.sh`
  capture search-space, best-candidate, and correctness evidence for automated
  tuning runs.
- `target/release/cache_analysis --tokens N --sustained-gb-s B` now includes
  `attention.prefill_kv_stream`, the exact qh4 prefill Attention K/V byte-floor
  row. Use it to separate unavoidable streaming from real cache-miss/layout
  bugs.
- `tools/run_hardware_backend_shootout.sh` and
  `tools/analyze_hardware_backend_shootout.py` capture whether SME/SME2,
  I8MM/BF16, MPSMatrix, and Core ML / ANE are available and measurable. Use
  them before claiming a kernel uses CPU SME2, M5 GPU matrix acceleration, or
  ANE.
- `tools/run_sme2_smoke_probe.sh` compiles and runs a minimal ACLE SME/SME2
  smoke probe and emits disassembly evidence. It proves CPU SME2 code can run,
  not that the model hot path is using SME2.
- `tools/run_sme2_mopa_probe.sh` compiles and runs a minimal SME2 int8 MOPA
  probe and emits `smopa` disassembly evidence. It proves the CPU backend can
  execute SME2 outer-product code, not that Qwen uses it yet.
- `tools/run_sme2_i8_tile_probe.sh` compiles and runs a Qwen-shape-near SME2
  int8 tile stream probe. It adds panel streaming, ZA stores, MOPA/s, and
  modeled stream bandwidth evidence, but it is still not a layout-correct model
  matmul backend.
- `tools/run_static_int8_matmul_autotune.sh` sweeps the static INT8 Metal
  matmul probe across row tiles and quant group sizes so layout choices are
  measured instead of guessed.
- `tools/analyze_static_int8_autotune.py` summarizes that sweep and makes the
  best/worst schedule delta explicit.
- `tools/prefill_reference_report.py` prints the current curated p4096/p16k/p32k
  prefill projections against llama.cpp and keeps exact vs approximate wins
  separate.
- `tools/run_decode_regression_matrix.sh` runs the serial tg128/tg512 decode
  guard across accepted, no-Split-K, rowcache, and no-Split-K+rowcache variants.
  Use it before promoting decode defaults; tg4 is only a smoke/forensics run.
  Optional `--storage-sweep` and `--sync-sweep` add storage-mode and CPU-sync
  variants.
- `tools/exact_attention_traffic_report.py` models exact qh4 attention K/V
  traffic for qblk candidates and lists measured rejected/accepted variants.
- `tools/run_attention_qk_mps_probe.sh` and
  `tools/analyze_attention_qk_mps_probe.py` measure dense QK matrix-backend
  throughput as evidence for a future tiled exact attention path.
- `tools/plan_tiled_attention.py` sizes score tiles and causal tile-pair counts
  for that future exact tiled attention prototype.
- `tools/run_tiled_attention_qk_mps_prototype.sh`,
  `tools/run_tiled_attention_qk_mps_grid.sh`, and
  `tools/analyze_tiled_attention_qk_mps_grid.py` test tile/encode overhead
  before building the full tiled QK-softmax-V kernel.
- `tools/run_tiled_attention_full_mps_prototype.sh` measures the synthetic
  qh4 tiled QK -> SIMD32 block softmax -> P*V -> online-combine path, exposing
  whether the full exact-attention architecture can beat the old K/V scan. It
  uses MPSMatrix origins to emulate real tile slicing without copy kernels.
- `tools/run_prefill_attention_backend_matrix.sh` compares the real accepted
  prefill attention-core path with the synthetic Rust MPS tiled exact-attention
  prototype. The output is a prioritization signal, not an accepted-profile
  speed claim. The tiled side now uses a synthetic Qwen-layout bridge, packs
  both KV groups, and measures the full GQA attention inner loop before real
  QKV/O projection integration.
- `tools/run_prefill_mps_tiled_projection.sh` measures accepted QH4 attention
  and the opt-in exact MPS tiled attention core live, then projects the
  full-prefill impact of replacing Qwen's six full-attention layers. This is a
  projection gate. After the p512/p4096 all-layer parity sweep, exact MPS tiled
  attention is now the accepted prefill attention backend.
- `tools/normalize_benchmark_output.sh` and
  `tools/fill_autotune_record_from_output.sh` transfer existing benchmark stdout
  into evidence records without rerunning benchmarks.
- `tools/show_kernel_evidence_bundle.sh` summarizes a decision's linked
  experiment, cache-forensics, autotune, and promotion status.
- `tools/propose_accepted_profile_update.sh` creates a review artifact for
  accepted-profile env changes after the promotion gate passes.

## First Gates

```text
cargo test
cargo run --bin qwen35-08b-metal-research
cargo run --release --bin bench_stream -- 16 5
cargo run --release --bin bench_matvec -- 3584 10
cargo run --release --bin bench_matvec_tiled -- 3584 10
cargo run --release --bin bench_lm_head -- full 3
cargo run --release --bin bench_lm_head_tiled -- full 3
cargo run --release --bin bench_decode_skeleton -- full 3 107
cargo run --release --bin bench_mega_synthetic -- 8192 1 24 107
cargo run --release --bin bench_rms_matvec -- 3584 10
cargo run --release --bin bench_rms_matvec_tiled -- 3584 10
cargo run --release --bin bench_deltanet -- 20
cargo run --release --bin bench_mega_pattern -- 32768 1 107
cargo run --bin inspect_artifacts -- /path/to/Qwen3.5-0.8B
cargo run --release --bin pack_weights -- /path/to/Qwen3.5-0.8B /tmp/qwen35_08b_fp16.metalpack
cargo run --release --bin bench_metalpack_lm_head -- /tmp/qwen35_08b_fp16.metalpack 3
cargo run --release --bin bench_metalpack_matvec -- /tmp/qwen35_08b_fp16.metalpack mlp.gate 10
cargo run --release --bin bench_metalpack_decode_skeleton -- /tmp/qwen35_08b_fp16.metalpack 107 3
cargo run --release --bin bench_metalpack_decode_projection -- /tmp/qwen35_08b_fp16.metalpack self_attn.o_proj 107 3
cargo run --release --bin bench_metalpack_decode_attention -- /tmp/qwen35_08b_fp16.metalpack model.layers.0.self_attn 107 3
cargo run --release --bin bench_metalpack_decode_deltanet -- /tmp/qwen35_08b_fp16.metalpack model.layers.0 107 3
cargo run --release --bin bench_metalpack_decode_ffn -- /tmp/qwen35_08b_fp16.metalpack model.layers.0.mlp 107 3
cargo run --release --bin bench_metalpack_decode_ffn_stack -- /tmp/qwen35_08b_fp16.metalpack 6 model.layers.0.mlp 107 3
cargo run --release --bin bench_metalpack_decode_superblock -- /tmp/qwen35_08b_fp16.metalpack model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 3 1
cargo run --release --bin bench_metalpack_decode_superblock -- /tmp/qwen35_08b_fp16.metalpack model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 1 6
cargo run --release --bin bench_metalpack_decode_layered_pattern -- /tmp/qwen35_08b_fp16.metalpack model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 1
cargo run --release --bin make_synthetic_metalpack -- /tmp/ctox_qwen35_08b_synth_true_shape.metalpack 8192 1
cargo run --bin audit_shapes -- /tmp/ctox_qwen35_08b_synth_true_shape.metalpack
cargo run --release --bin bench_metalpack_decode_attention_steps -- /tmp/ctox_qwen35_08b_synth_true_shape.metalpack model.layers.3.self_attn 107 4 1 4
cargo run --release --bin bench_metalpack_decode_layered_pattern -- /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4
```

Current code fixes the model shape, experiment gates, and the first owned Metal
stream read/write, matvec, full-vocab LM-head argmax, and one-sync decode
skeleton smoke benchmarks. It also includes the first synthetic single-dispatch
megakernel shape plus a Qwen-pattern `[D,D,D,A]x6` single-dispatch path. The
metalpack-backed decode slices now cover embedding -> LM-head, embedding ->
projection -> LM-head, embedding -> FFN -> LM-head, repeated FFN stacks, and a
single-token attention operator slice with packed Q/K/V/O projections. The
DeltaNet slice follows the same route: packed qkv/z/b/a/out operators with
recurrent state kept GPU-local. The first packed scheduler slice executes the
Qwen `[D,D,D,A]` superblock shape with FFN after every mixer in one command
buffer, and the same scheduler can run all six superblocks for the full
24-layer topology. A separate layered-pattern path now resolves full manifests
by `layer` and `TensorClass` into all 18 DeltaNet, 6 attention, and 24 FFN
slots, with a template-prefix fallback kept for small synthetic packs.
The scheduler path now accepts the Qwen3.5 GQA attention shapes directly:
Q projection rows 2048 or 2056 with head gate, K/V rows 512, and O projection
K=2048. `make_synthetic_metalpack` creates a compact alias pack that exercises
the full 24-layer manifest and these true operator shapes without materializing
all duplicated layer weights. The attention dispatch also owns K/V cache
buffers and an online-softmax cache loop; the current benchmark route still
defaults to `position=0`/`max_context=1` but exposes both parameters on the
attention, superblock, and layered-pattern benchmark CLIs. Long-context parity
remains a separate gate.
`bench_metalpack_decode_attention_steps` drives a multi-step attention slice
through one persistent KV-cache allocation and reads only `next_token` between
positions. The layered-pattern benchmark uses its optional trailing `steps`
argument to run the full 24-layer path over multiple generated tokens while
keeping DeltaNet state and all six attention KV caches alive inside the same
benchmark sequence.

The next bridge toward real weights is `inspect_artifacts`: it reads a local
HF-style `config.json` plus safetensors headers, validates the config against
the fixed 0.8B shape, classifies tensors by Qwen subsystem, and emits the first
Metal pack plan. `pack_weights` writes a deterministic `.metalpack` directory
with `manifest.json` and tiled `weights.bin`. Real language-layer kernels,
reference token capture, and Core ML/ANE probing are now represented in the
research gates. The Core ML/ANE path is ruled out for this crate unless a
separate `.mlmodel`/`.mlpackage` artifact or converter path is added.
