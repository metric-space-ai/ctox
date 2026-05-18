# Hardware Backend Grid

This grid is the required planning surface before adding more Qwen3.5 kernels.
Every candidate must map to a concrete hardware/backend column and show why its
layout feeds that backend efficiently.

## Captured Platform

```text
capture: /tmp/ctox_qwen35_hardware_grid_current/hardware_feature_matrix.md
machine: MacBook Pro Mac17,2
chip: Apple M5
cpu: 10 cores, 4 performance + 6 efficiency
gpu: Apple M5, 10 cores
memory: 32 GB unified
Metal: Metal 4
CPU feature sysctls:
  FEAT_SME=1
  FEAT_SME2=1
  FEAT_SME2p1=1
  FEAT_BF16=1
  FEAT_EBF16=1
  FEAT_I8MM=1
  FEAT_DotProd=1
  FEAT_FP16=1
  FEAT_FHM=1
  SME_F16F32=1
  SME_I8I32=1
public Metal counters:
  GPUTimestamp only
```

## Backend Columns

| Backend | Hardware feature | Best candidate ops | Current status | Promotion rule |
|---|---|---|---|---|
| GPU MSL SIMDgroup | 32-lane SIMT/SIMDgroup, threadgroup memory, simd reductions | DeltaOut, attention reductions, custom recurrence | Active | Keep only when measured near backend floor and better than MPS/CPU alternatives |
| GPU MPS Matrix | Apple GPU matrix backend via `MPSMatrixMultiplication` | Prefill Gate+Up, FFN Down, broad dense GEMM | Proven faster in raw p4096 probes | Integrate or match before spending more time on slower MSL matmul schedules |
| GPU Metal 4 Tensor/MPP | Metal 4 tensor/machine-learning backend | Future tensorized matmul/quantized blocks | Not integrated | Add probe before claiming M5 tensor accelerator use |
| CPU NEON | 128-bit vector units | Small control transforms, pack validation, fallback | Not benchmarked as hot path | Must beat GPU path for chosen op or stay out of token hot path |
| CPU SME/I8MM/BF16 | CPU matrix extension and int8/BF16 features visible in sysctl | Static packing, coarse fallback, possible quant probes | SME2 smoke, repeated MOPA, and Qwen-shape-near I8 tile stream probes exist; not in model path | Benchmark separately; do not assume GPU speedup from CPU SME |
| Core ML/ANE | Core ML scheduler, ANE placement if supported | Coarse full graph/prefill/vision experiments | Ruled out for current crate decode hot path | Only use with placement evidence and no per-token graph churn |

## Hardware Backend Shootout p512

Source:

```text
tools/run_hardware_backend_shootout.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 512 3 \
  /tmp/ctox_qwen35_hardware_backend_current3

tools/analyze_hardware_backend_shootout.py \
  /tmp/ctox_qwen35_hardware_backend_current3/shootout.md
```

Result:

```text
sme2_runtime_available: yes
sme2_compile_available: yes
sme2_smoke_executes: yes
sme2_disassembly_has_smstart: yes
sme2_i8_mopa_executes: yes
sme2_used_by_current_model_hotpath: no

CPU quant probe:
  shape: tokens=512 rows=3584 k=1024
  i8_median_s latest:        0.088641875
  i8_effective_tops latest:  0.042
  q4_unpack_median_s latest: 0.110470791
  q4_effective_tops latest:  0.034
  note: CPU probe is thermally/noise sensitive; use it as backend-order evidence, not a final roofline

SME2 I8 MOPA probe:
  streaming_vector_bytes: 64
  za_rows_s32: 16
  disassembly: smopa za0.s, p0/m, p0/m, z0.b, z1.b
  hotpath_status: microkernel_probe_not_model_path

MPSMatrix fp16 GEMM:
  gate/up single latest:    1.449 TFLOPS
  gate+up combined latest: 4.903 TFLOPS
  ffn down latest:         4.096 TFLOPS

Core ML / ANE:
  status: ruled_out
  reason: no .mlmodel/.mlpackage/.mlmodelc artifact and coremltools unavailable
```

Decision:

```text
Do not claim SME2 or ANE use in the current pipeline.

SME2 is present on the M5 and visible to clang, but the current CPU probe uses
NEON DotProd-style intrinsics and explicitly reports no model-hotpath SME2 use.
The SME2 smoke probe executes streaming-mode/ZA code and disassembly contains
`smstart`, `zero {za}`, and `smstop`. The MOPA probe additionally emits
`smopa za0.s`, proving int8 SME outer-product code generation and execution.
This proves toolchain/runtime viability, not model acceleration. MPSMatrix is
still the measured fast matrix backend for Qwen GEMM shapes. Core ML / ANE
remains a separate coarse graph experiment and is not available for the current
Metalpack hot path.
```

## Hardware Backend Shootout p512 with SME2 Tile Stream

Source:

```text
tools/run_hardware_backend_shootout.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 512 3 \
  /tmp/ctox_qwen35_hardware_backend_sme2_tile_20260501T061620Z

tools/analyze_hardware_backend_shootout.py \
  /tmp/ctox_qwen35_hardware_backend_sme2_tile_20260501T061620Z/shootout.md
```

Result:

```text
sme2_runtime_available: yes
sme2_compile_available: yes
sme2_smoke_executes: yes
sme2_disassembly_has_smstart: yes
sme2_i8_mopa_executes: yes
sme2_i8_tile_executes: yes
sme2_used_by_current_model_hotpath: no

CPU quant probe:
  shape: tokens=512 rows=3584 k=1024
  i8_median_s:        0.037717958
  q4_unpack_median_s: 0.058795250

SME2 I8 tile stream probe:
  shape: tokens=512 rows=3584 k=1024
  streaming_vector_bytes: 64
  za_rows_s32:           16
  m_tiles/n_tiles/k:     32 / 224 / 16 blocks
  mopa_per_run:          114688
  best_s:                0.000512500
  mopa_per_s_best:       223781463.415
  stream_gb_s_best:      42.966
  disassembly:           smopa + ZA st1w stores
  hotpath_status:        tile_probe_not_model_path

MPSMatrix fp16 GEMM:
  gate/up single:        5.758 TFLOPS
  gate+up combined:      5.215 TFLOPS
  ffn down:              5.423 TFLOPS

Core ML / ANE:
  status: ruled_out
```

Decision:

```text
SME2 has now advanced from availability-only to a panel-streaming feasibility
probe with real `smopa` and ZA-store instruction evidence. It is still not a
Qwen matmul backend and must not be promoted into the model path yet.

The p512 evidence favors GPU/MPSMatrix for dense projection shapes. The next
SME2 step, if pursued, must be a layout-correct INT8/Q4 matmul packer and
correctness harness, not more smoke testing. Until that exists and beats the
GPU matrix backend for a specific operation, SME2 remains a research backend.
```

## Static INT8 Metal Matmul Autotune p512

Source:

```text
tools/run_static_int8_matmul_autotune.sh 512 3584 3 1 \
  | tee /tmp/ctox_qwen35_static_int8_autotune_512.txt

tools/analyze_static_int8_autotune.py \
  /tmp/ctox_qwen35_static_int8_autotune_512.txt \
  --reference-median-s 0.000652708
```

Result:

```text
candidate_count: 9

best:
  row_tile:                16
  quant_group_size:        128
  col_tile:                256
  median_s:                0.029514792
  p95_s:                   0.036120125
  effective_visible_gb_s:  0.411

worst:
  row_tile:                4
  quant_group_size:        256
  median_s:                0.094780208

best_vs_worst_speedup:     3.211
best_vs_MPS_single_proj:   0.022x
```

Decision:

```text
Keep the row_tile=16 shader extension and autotune tooling as evidence that
layout/schedule parameters matter materially. Do not promote static INT8 Metal
matmul into the model path: the best measured candidate is still far slower
than the p512 MPSMatrix single-projection reference.

This rejects the current scalar-dequant INT8 Metal schedule more strongly than
before. The next quantized matmul attempt must use a different consumption path:
SIMDgroup/tensor-style dotting, MPS/Core ML quant backend, or CPU SME2 with a
layout-correct packed kernel.
```

## Static INT8 SIMD32 Probe p512

Source:

```text
tools/run_static_int8_matmul_autotune.sh 512 3584 3 1 \
  | tee /tmp/ctox_qwen35_static_int8_autotune_simd32_512.txt

tools/analyze_static_int8_autotune.py \
  /tmp/ctox_qwen35_static_int8_autotune_simd32_512.txt \
  --reference-median-s 0.000652708
```

Result:

```text
candidate_count: 18

best overall:
  kernel:                 scalar
  row_tile:               16
  quant_group_size:       64
  median_s:               0.032614458
  p95_s:                  0.037204834

best simd32:
  kernel:                 simd32
  row_tile:               4
  quant_group_size:       64
  median_s:               0.049373958
  p95_s:                  0.055636208

best_vs_MPS_single_proj:  0.020x
```

Decision:

```text
Reject the first SIMD32 static INT8 matmul variant. It removes the old
threadgroup-memory reduction and maps one SIMDgroup to one output row, but it
does not improve the kernel. The likely causes are still scalar dequant loads,
poor reuse of X across rows, and too many small per-row reductions.

This is a useful negative result: using SIMDgroup reductions is necessary in
many kernels, but not sufficient. The quantized matmul path needs a packing and
compute strategy that reuses input tiles across many rows and maps to real
matrix/tensor hardware, not merely one row per SIMDgroup.
```

## Prefill Reference Report

Source:

```text
tools/prefill_reference_report.py \
  | tee /tmp/ctox_qwen35_prefill_reference_report.txt
```

Result:

```text
exact_mps_deltaout:
  p4096:  3112.46 tok/s vs llama.cpp 2852.70 = 1.091x
  p16384: 1396.40 tok/s vs llama.cpp 2065.71 = 0.676x
  p32768:  786.90 tok/s vs llama.cpp 1325.20 = 0.594x

halfdot_full_context:
  p16384: 2102.59 tok/s vs llama.cpp 2065.71 = 1.018x
  p32768: 1105.53 tok/s vs llama.cpp 1325.20 = 0.834x

window_halfdot:
  p16384 win4096: 2982.59 tok/s = 1.444x
  p16384 win8192: 2154.04 tok/s = 1.043x
  p32768 win4096: 2778.54 tok/s = 2.097x
  p32768 win8192: 1921.66 tok/s = 1.450x
  p32768 win16384: 1301.65 tok/s = 0.982x
```

Decision:

```text
The exact long-prefill gap is now concentrated in long-context attention.
Approximate sparse/window modes can beat llama.cpp, but they are not accepted
profile wins. Further exact work should target K/V traffic and attention
algorithm structure without semantic windowing; static INT8 matmul tuning is
not on the critical path.
```

## Exact Attention Traffic And Dense QK Matrix Probe

Sources:

```text
tools/exact_attention_traffic_report.py \
  --tokens 4096,8192,16384,32768 \
  --sustained-gb-s 174 \
  | tee /tmp/ctox_qwen35_exact_attention_traffic_report.txt

tools/run_mps_matrix_probe.sh 4096 4096 256 3 1
tools/run_mps_matrix_probe.sh 8192 8192 256 3 1
tools/run_mps_matrix_probe.sh 16384 16384 256 2 1

tools/analyze_attention_qk_mps_probe.py \
  /tmp/ctox_qwen35_attention_qk_mps_manual/report.md
```

Exact qh4 K/V traffic model:

```text
p32768 qh4/qblk1 exact:
  K/V stream:       1024.03 GiB
  byte floor @174:  6319.225 ms
  measured:         near byte floor

p32768 hypothetical qblk2/qblk4/qblk8:
  qblk2 K/V stream: 512.03 GiB = 0.500x qblk1
  qblk4 K/V stream: 256.03 GiB = 0.250x qblk1
  qblk8 K/V stream: 128.03 GiB = 0.125x qblk1
```

Dense QK MPS probe:

```text
tokens  qk_one_head_s  TFLOPS  qk_8_heads_s  qk_6_layers_s
4096    0.001797917    4.778   0.014383336   0.086300016
8192    0.005539041    6.203   0.044312328   0.265873968
16384   0.015537875    8.845   0.124303000   0.745818000
```

Decision:

```text
The exact qh4/qblk1 scan is already near its K/V byte floor, so local cache-miss
cleanup cannot close p32k. Query-blocking has the right byte model but has so
far lost to register pressure. Dense MPS QK shows enough matrix-backend headroom
to justify a future tiled exact attention prototype: QK matrix/tensor compute,
online or block softmax, and V accumulation without materializing full T x T
scores for long contexts.
```

Tiled prototype planning:

```text
tools/plan_tiled_attention.py --tokens 16384 \
  --q-tiles 64,128,256,512 \
  --k-tiles 256,512,1024,2048

recommended first grid:
  q_tile: 128..256
  k_tile: 512..1024

examples:
  q_tile=128 k_tile=512:
    score tile:        0.125 MiB per Q head
    causal tile pairs: 2112
    K/V tile:          0.500 MiB

  q_tile=256 k_tile=1024:
    score tile:        0.500 MiB per Q head
    causal tile pairs: 544
    K/V tile:          1.000 MiB
```

## Tiled QK MPS Prototype Grid

Goal:

```text
Measure whether a tiled exact-attention QK schedule can use Apple matrix
hardware without exploding command-encode overhead before implementing the full
QK-softmax-V path.
```

Prototype contract:

```text
tools/tiled_attention_qk_mps_prototype.swift

synthetic repeated QK tile GEMM
MPSMatrix fp16 input/output
one command buffer per sample
causal tile-pair count modeled from tokens/q_tile/k_tile
no real Q/K slicing
no softmax
no V accumulation
no accepted-profile semantics
```

Commands:

```text
tools/run_tiled_attention_qk_mps_grid.sh 4096 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_4096.txt

tools/analyze_tiled_attention_qk_mps_grid.py \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_4096.txt

tools/run_tiled_attention_qk_mps_grid.sh 8192 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_8192.txt

tools/analyze_tiled_attention_qk_mps_grid.py \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_8192.txt

tools/run_tiled_attention_qk_mps_grid.sh 16384 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_16384.txt

tools/analyze_tiled_attention_qk_mps_grid.py \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_16384.txt

tools/run_tiled_attention_qk_mps_grid.sh 32768 2 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_32768.txt

tools/analyze_tiled_attention_qk_mps_grid.py \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_32768.txt
```

Measured:

```text
p4096 best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=40
  median_s=0.001742500
  effective_tflops=3.081
  mps_encodes_per_s=22955.524

p8192 best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=144
  median_s=0.006061125
  effective_tflops=3.189
  mps_encodes_per_s=23757.966

p16384 best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=544
  median_s=0.016984125
  effective_tflops=4.299
  mps_encodes_per_s=32029.910

p32768 best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=2112
  median_s=0.059598042
  effective_tflops=4.756
  mps_encodes_per_s=35437.406
  projected_qk_8_heads_6_layers_s=2.860706016
```

Decision:

```text
Keep q_tile=256/k_tile=1024 as the first full tiled-attention prototype shape.
Smaller tiles lose too much to encode overhead and poor effective TFLOPS. This
does not yet prove a faster exact prefill path because softmax, V accumulation,
real packed Q/K/V access, and command-buffer structure still need to be built.
It does prove that the next exact path should not be another qblk1 K/V scan.
```

## Full Tiled Attention MPS Prototype

Goal:

```text
Move from QK-only tile timing to the real exact-attention stage sequence:
QK tile GEMM, block softmax update, P*V tile GEMM, and online output combine.
```

Prototype contract:

```text
tools/tiled_attention_full_mps_prototype.swift

synthetic repeated Q/K/V tiles
MPSMatrix fp16 QK
SIMD32 row-wise block softmax update
MPSMatrix fp16 P*V
online m/l/out combine
heads_per_group=4 for Qwen GQA qh4
MPSMatrix origins emulate real Q/K/V tile slicing without copy kernels
global output store per Q block
sparse CPU reference quality gate for sampled rows/dims
not accepted-profile semantics yet
```

Measured with corrected online combine, global output store, `q_tile=256`,
`k_tile=1024`, `heads_per_group=4`, `matrix_origins=1`:

```text
p4096:
  median_s=0.009491333
  effective_tflops_qk_plus_pv=4.525
  effective_gb_s_modeled_tile_traffic=26.515

p8192:
  median_s=0.026230208
  effective_tflops_qk_plus_pv=5.895
  effective_gb_s_modeled_tile_traffic=34.539

p16384:
  median_s=0.094818167
  effective_tflops_qk_plus_pv=6.160
  effective_gb_s_modeled_tile_traffic=36.096

p32768:
  median_s=0.338463542
  effective_tflops_qk_plus_pv=6.700
  effective_gb_s_modeled_tile_traffic=39.258
  projected_2_kv_groups_6_layers_s=4.061562504
```

Quality gate:

```text
tools/run_tiled_attention_full_mps_prototype.sh 512 128 256 2 1 4 1 1
  quality_checked_points=64
  quality_mean_abs_error=0.000087432
  quality_max_abs_error=0.000277638

tools/run_tiled_attention_full_mps_prototype.sh 1024 256 512 2 1 4 1 1
  quality_checked_points=64
  quality_mean_abs_error=0.000077426
  quality_max_abs_error=0.000253379
```

Decision:

```text
Promote this from pure planning to the next implementation track. The p32k
projection is not a final benchmark because it still uses synthetic Q/K/V data,
but MPSMatrix origins now exercise real tile addressing without copy kernels.
The online combine now includes the required exp(tile_m-next_m) PV scaling and
passes a sparse CPU reference gate at p512/p1024. The next work item is
integrating this path into the Rust benchmark and comparing its full attention
dump against the accepted qh4 kernel.
```

## p4096 Matrix Backend Result

Source:

```text
tools/run_matrix_backend_shootout.sh /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 3 /tmp/ctox_qwen35_matrix_backend_grid_p4096
tools/analyze_matrix_backend_grid.py /tmp/ctox_qwen35_matrix_backend_grid_p4096/shootout.md
```

Result:

```text
gate_up_combined:
  MPS raw GEMM: 0.005159167 s
  MSL integrated MMA: 0.019215459 s
  MSL/MPS ratio: 3.725x slower
  winner: MPS

ffn_down:
  MPS raw GEMM: 0.002741708 s
  MSL integrated MMA: 0.038315541 s
  MSL/MPS ratio: 13.975x slower
  winner: MPS

delta_out:
  MPS raw GEMM: 0.001801625 s
  MSL integrated gated-norm+out: 0.001510125 s
  MSL/MPS ratio: 0.838x
  winner: current MSL path
```

Decision:

```text
The next prefill performance milestone is not another hand-tuned scalar
quantized matmul. It is either:

1. integrate MPS/Metal-matrix execution for Gate+Up and FFN Down while solving
   layout/intermediate interop, or
2. write an MSL SIMDgroup/MMA schedule that matches the MPS backend envelope.

DeltaOut should stay on the current MSL path for now.
```

## Hybrid MPS FFN Block Probe

Source:

```text
tools/run_mps_ffn_block_probe.sh 4096 1024 3584 3 1
```

Operation:

```text
MPSMatrixMultiplication:
  x[tokens,1024] * gate_up[1024,7168] -> gate_up_out[tokens,7168]

MSL SwiGLU:
  silu(gate) * up -> act[tokens,3584]

MPSMatrixMultiplication:
  act[tokens,3584] * down[3584,1024] -> out[tokens,1024]
```

Result:

```text
p4096 hybrid MPS+MSL FFN block:
  median_s: 0.009343500
  effective_tflops: 9.653
  visible_gb_s: 13.579

current MSL probes for the same two large FFN matrix phases:
  gate/up MSL MMA: 0.019215459 s
  down MSL MMA:    0.038315541 s
  sum:             0.057531000 s

hybrid block vs MSL sum:
  ~6.16x faster before real metalpack-layout interop costs
```

Real metalpack-weight variant:

```text
tools/run_mps_ffn_metalpack_probe.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 3 1

layout conversion:
  one-time host-side fp16_row_tiled -> MPS row-major/transposed FFN buffers

median_s: 0.009666167
effective_tflops: 9.331

real-metalpack MPS block vs MSL sum:
  0.057531000 / 0.009666167 ~= 5.95x faster
```

Persistent MPS FFN sidecar:

```text
target/release/pack_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar

pack time:
  1.394 s

sidecar size:
  528482304 bytes / 0.492 GiB

layout:
  gate_up per layer: [1024, 7168] fp16 row-major
  down per layer:    [3584, 1024] fp16 row-major

tools/run_mps_ffn_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar 0 4096 3 1

median_s: 0.008587583
effective_tflops: 10.503

sidecar MPS block vs MSL sum:
  0.057531000 / 0.008587583 ~= 6.70x faster
```

Rust runtime MPS bridge:

```text
target/release/bench_mps_ffn_sidecar_runtime \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar 0 4096 3 1

backend:
  Rust C-ABI MSL RMSNorm + MPSMatrix + MSL SwiGLU + persistent sidecar

median_s: 0.009669208
p95_s:    0.010470041
effective_tflops: 9.328

no-norm control:
  median_s: 0.009181208

runtime bridge vs current MSL FFN sum:
  0.057531000 / 0.009669208 ~= 5.95x faster
```

p4096 full-prefill impact estimate:

```text
tools/estimate_mps_ffn_prefill_impact.py

baseline_full_s:          3.364000000
baseline_tok_s:           1217.60
llama_tok_s:              2852.70
msl_ffn_per_layer_s:      0.057531000
mps_ffn_per_layer_s:      0.008587583
ffn_layers:               24
total_saved_s:            1.174642008
projected_full_s:         2.189357992
projected_tok_s:          1870.87
projected_vs_llama_gap_x: 1.525
remaining_seconds_to_llama: 0.753525272
```

Runtime-bridge impact estimate:

```text
tools/estimate_mps_ffn_prefill_impact.py --mps-ffn-s 0.009669208

projected_full_s:         2.215316992
projected_tok_s:          1848.95
projected_vs_llama_gap_x: 1.543
remaining_seconds_to_llama: 0.779484272
```

Integrated Delta18+FFN stack with MPS FFN sidecar:

```text
target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  <metalpack-dir> 0 <tokens> <iterations> <warmup> <delta-layer-count> \
  <mps-ffn-sidecar-dir>
```

p512 / 3 DeltaNet+FFN layers:

```text
MSL accepted profile:
  median_s: 0.034042333

MPS FFN sidecar:
  median_s: 0.022567750

speedup:
  1.51x
```

p4096 / 18 DeltaNet+FFN layers:

```text
MSL accepted profile:
  median_s: 1.701746125

MPS FFN sidecar:
  median_s: 1.180014250

speedup:
  1.44x

time_saved:
  0.521731875 s
```

Drift gate snapshot:

```text
p512 / 3 layers, MSL final hidden vs MPS-FFN final hidden:
  mean_abs_error: 0.000300663
  rms_error:      0.000444082
  max_abs_error:  0.015625000
```

Decision:

```text
Keep the integrated MPS FFN sidecar track active. It is a real pipeline win
but not a full acceptance yet: the next gate must define acceptable numerical
drift and then apply the same MPS sidecar phase cut to attention-layer FFNs or
DeltaNet QKV+Z.
```

Interpretation:

```text
The MPS FFN sidecar is necessary and large, but not enough alone. After FFN
replacement, p4096 still needs roughly 0.75 s removed to match llama.cpp.
The next backend-grid targets remain DeltaNet projection/scan and attention
core, with priority determined by measured post-FFN integration forensics.
```

Decision:

```text
This is now the primary prefill track. The next production integration should
route FFN Gate+Up and Down through an MPS/Metal-matrix-compatible layout, with
custom MSL kept for RMSNorm, SwiGLU, residual, DeltaNet recurrence, and
attention-specific work.
```

## MPS DeltaNet QKV+Z Projection Probe

Source:

```text
tools/run_mps_deltanet_project_probe.sh 4096 1024 6144 2048 3 1
```

Operation:

```text
MPSMatrixMultiplication:
  x[tokens,1024] * qkvz[1024,8192] -> qkvz_out[tokens,8192]
```

Synthetic result:

```text
p4096 MPS DeltaNet QKV+Z projection:
  median_s: 0.006537417
  effective_tflops: 10.512
  visible_gb_s: 14.115
```

Persistent MPS DeltaNet projection sidecar:

```text
target/release/pack_mps_delta_project_sidecar \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

sidecar size:
  301989888 bytes / 0.281 GiB

layout:
  qkvz per DeltaNet layer: [1024, 8192] fp16 row-major

tools/run_mps_deltanet_project_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar 0 4096 3 1

median_s: 0.006910042
effective_tflops: 9.945
visible_gb_s: 13.354
```

f32-output integration feasibility:

```text
tools/run_mps_deltanet_project_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar 0 4096 3 1 1

output_dtype: float32
median_s: 0.008769667
effective_tflops: 7.836
visible_gb_s: 18.174
```

Comparison caveat:

```text
The existing bench_metalpack_prefill_delta_project result for layer 0 at p4096
was 0.255187917 s, but that benchmark includes RMSNorm plus qkv/z/b/a projection
plumbing and is not a direct integrated-stack replacement measurement. The
MPS sidecar proves that the dense QKV+Z matrix phase itself should not remain
on the handwritten MSL path. The remaining work is runtime integration and a
post-integration full-prefill forensics run.
```

Decision:

```text
Promote DeltaNet QKV+Z to the same backend track as FFN: persistent sidecar
layout plus MPS/Metal matrix execution. Keep b/a, RMSNorm, conv, scan,
gated norm, and out projection in custom MSL until a measured backend-specific
candidate beats them.
```

## Quantization Rule

Quantization format is selected by backend evidence, not compression ratio.

```text
GPU MPS/Metal tensor:
  use the dtype/layout the backend can consume at high throughput

GPU MSL SIMDgroup:
  use packed groups only when adjacent lanes read adjacent values/scales and
  the kernel avoids scalar per-row reductions

CPU SME/I8MM:
  use panels/groups that match SME/I8MM matrix primitives and sequential
  prefetch; benchmark before routing model hot path to CPU
```

Rejected pattern:

```text
choose Q4/INT8 because it is smaller, then run a scalar dequant dot-product
that cannot use the matrix hardware.
```

Accepted pattern:

```text
choose Q4/INT8 only when the backend primitive and packed layout are known,
then prove speedup, p95, bandwidth/compute movement, and bounded numerical
drift against the accepted profile.
```

## CPU Quant Probe

Source:

```text
tools/run_cpu_quant_probe.sh 64 1024 1024 3 1
```

Result:

```text
neon_dotprod_compile_feature: 1
i8_median_s: 0.002407625
i8_effective_tops: 0.056
i8_visible_gb_s: 0.572
q4_unpack_median_s: 0.003705458
q4_unpack_effective_tops: 0.036
q4_unpack_visible_gb_s: 0.230
```

Decision:

```text
This naive CPU DotProd/q4-unpack path is not a model hotpath candidate.
It is a baseline/control probe for the CPU backend column. A future SME/I8MM
candidate must use an actual matrix-shaped primitive/panel layout and beat this
probe by a large margin before it can affect Qwen prefill/decode strategy.
```

## Integrated MPS QKV+Z + FFN Sidecars

Runtime bridge:

```text
target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 2 1 18 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar
```

Result:

```text
median_s: 0.814598250
tok_s_delta18_ffn_stack_only: 5028.3

previous MPS-FFN-only stack:
  median_s: 1.180014250

old MSL accepted stack:
  median_s: 1.701746125

speedup:
  1.45x vs MPS-FFN-only
  2.09x vs old MSL accepted stack
```

Full-prefill forensics:

```text
target/release/memory_forensics \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 2 90 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

delta18+ffn:
  median_ms: 749.961
  modeled_bytes: 7.30 GiB
  weight_stream/unique: 1.0x
  ratio_to_90GBs_floor: 8.61x

attention.core:
  median_ms: 299.930

attention.ffn:
  median_ms: 36.358

full_prefill_estimate_current_kernels:
  2.768 s
  1479.93 tok/s
```

Backend-grid interpretation:

```text
QKV+Z and FFN are now proven MPS-matrix backend wins in the integrated
Delta18 prefill path. The remaining p4096 gap is no longer explained by
repeated QKVZ/FFN weight streaming alone. The next grid cells to test are:

1. MSL stride-aware qkvz consumers that remove the materialized qkvz split.
2. MSL chunked/fused DeltaNet scan+gated-norm to reduce recurrent-state stalls.
3. MPS or custom-MSL FFN sidecar integration for the six attention layers.
4. Flash/paged/block attention prefill variants for the attention.core row.
```

## qkvz-Direct Bridge Candidate

Candidate flag:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1
```

Purpose:

```text
Consume qkvz[tokens,8192] directly from the MPS QKV+Z output in Conv/Split and
GatedNorm instead of materializing qkv_out[tokens,6144] and z_out[tokens,2048].
```

p4096 / 18 DeltaNet-layer result:

```text
direct:
  median_s: 0.679421292

split bridge:
  median_s: 0.814598250

old MSL accepted stack:
  median_s: 1.701746125

speedup:
  1.20x vs split bridge
  2.51x vs old MSL stack
```

Profile stops:

```text
project:     0.117363291 s
conv_split:  0.143619791 s
scan_norm:   0.383966584 s
delta_out:   0.527728416 s
ffn_gate_up: 0.627994000 s
full:        0.684312708 s
```

Decision:

```text
Keep qkvz-direct as a candidate for promotion after broader sweeps. It has the
same p512/3-layer drift envelope as the split bridge and removes a measured
layout-copy bottleneck. The next measured target is scan_norm, not more QKVZ
matrix work.
```

## qkvz-Direct Plus Lanes4 Scan

Candidate flags:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1
CTOX_QWEN35_DELTA_SCAN_LANES4=1
```

ScanNorm sweep, p4096 / 18 DeltaNet layers:

```text
rowcache_block32: 0.374539958 s
rowcache_block64: 0.374242083 s
rowcache_direct:  0.375005167 s
lanes4:           0.269049041 s
lanes4_ordered:   1.569576625 s
scan_gated_norm:  0.394096291 s
```

Full Delta18+FFN:

```text
direct+lanes4:
  median_s: 0.539885917

direct+rowcache_block32:
  median_s: 0.679421292

speedup:
  1.26x
```

Model-wide p4096 estimate after also using MPS FFN for attention-layer FFNs:

```text
full_prefill_estimate_current_kernels:
  2.348 s
  1744.73 tok/s

llama.cpp pp4096:
  2852.70 tok/s

remaining_gap:
  1.64x
```

Decision:

```text
Treat qkvz-direct + lanes4 as the current MPS-sidecar candidate profile. Do not
promote it into accepted_profile.env yet because that file also drives non-MPS
benchmarks; create backend-specific accepted profiles before global promotion.
```

## Attention O Sidecar And SIMD32 Vec8 Scan

Backend/tooling additions:

```text
pack_mps_attention_out_sidecar
bench_mps_attention_out_sidecar_runtime
bench_metalpack_prefill_attention_core ... [mps-attention-out-sidecar-dir]
CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP={norm,project,prepare,attention,full}
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
```

Stage finding at p4096:

```text
old qh4/qblk1 attention.core:
  full:      313.159 ms
  attention: 275.163 ms
  o_proj:     37.996 ms

MPS O sidecar:
  isolated O median: 2.203 ms
  integrated full with qh4/qblk1: 275.315 ms
```

SIMD ownership sweep with MPS O:

```text
tokens  qh4/qblk1_ms  qh4_simd32_vec8_ms
512        9.027          5.476
1024      24.509         13.234
2048      76.739         41.205
4096     275.315        131.357
```

Interpretation:

```text
The 256-thread qh4/qblk1 kernel had good GQA KV reuse but performed
threadgroup-memory reductions and barriers for every key. qh4_simd32_vec8
keeps the full 256-dim head inside one SIMDgroup by assigning 8 dimensions per
lane. That raises register work but removes per-key cross-SIMD barriers.
```

p4096 full-prefill estimate with MPS sidecars and qh4_simd32_vec8:

```text
delta18+ffn:      558.545 ms
attention.core:   132.234 ms
attention.ffn:      7.638 ms
full estimate:      1.398 s
tok/s:           2930.37
llama.cpp pp4096: 2852.70 tok/s
```

Decision:

```text
Promote CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8 into accepted_profile.env as the
env-only attention default. Keep MPS Attention O as an explicit sidecar path
until backend-specific accepted profiles exist.
```

## DeltaOut MPS Sidecar And WIN4096 Attention Candidate

MPS DeltaOut sidecar:

```text
pack_mps_delta_out_sidecar
bench_metalpack_prefill_delta3_ffn_superblock ... [mps-delta-out-sidecar-dir]
```

Measured Delta18+FFN speed:

```text
p4096:  0.561171542 s -> 0.449904125 s
p16384: 2.500954542 s -> 2.111856459 s
```

Exact full-prefill estimates with MPS DeltaOut:

```text
p4096:  3112.20 tok/s  vs llama.cpp 2852.70
p16384: 1396.42 tok/s  vs llama.cpp 2065.71
p32768:  786.89 tok/s  vs llama.cpp 1325.20
```

Approximate attention candidate:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096=1
```

Attention-core timing:

```text
          exact qh4 SIMD32   win4096
p4096     0.121023500 s      0.130665875 s
p16384    1.578269541 s      0.771103375 s
p32768    6.035448458 s      1.602353875 s
```

Drift at p8192 versus exact:

```text
mean_abs_error: 0.002596090
rms_error:      0.009273482
max_abs_error:  0.756835938
```

Decision:

```text
MPS DeltaOut is a backend candidate win. WIN4096 is opt-in only: it shows the
speed potential of sparse attention, but exact long-prefill still needs a
FlashAttention-style tiled algorithm or a quality-validated KV-selection method.
```

## Parameterized Window Attention Grid

Backend:

```text
GPU MSL SIMDgroup
kernel: qh4 qblk1 SIMD32 vec8 with parameterized local KV window
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW=<tokens>
tool: tools/run_attention_window_quality_sweep.sh
```

Hardware use:

```text
one SIMDgroup owns 4 Q-heads sharing one KV-head
32 lanes * 8 half dimensions = full 256-dim head
window parameter reduces K/V cache stream length
no extra CPU sync and no CPU-side sampling/logit readback involved
```

Measured attention-core rows with MPS Attention O:

```text
p8192 exact/window4096/window8192:
  0.437737750 s / 0.347766042 s / 0.443750625 s

p16384 exact/window4096/window8192/window16384:
  1.587089250 s / 0.760995708 s / 1.205333083 s / 1.605048625 s

p32768 exact/window4096/window8192/window16384/window32768:
  6.462921750 s / 1.687524959 s / 2.964136750 s / 4.853272625 s / 6.307306750 s
```

Decision:

```text
Keep as approximate sparse-attention research grid. Do not promote to accepted
exact profile. Use it to evaluate whether the model can tolerate fixed-window
semantics or to motivate smarter KV-selection kernels.
```

## Exact Attention Cache Floor

Backend:

```text
GPU MSL SIMDgroup
kernel: qh4 SIMD32 vec8 exact prefill attention
tool: target/release/cache_analysis --tokens N --sustained-gb-s B
```

Current p32768 model:

```text
attention.prefill_kv_stream:
  modeled bytes: 1024.03 GiB per attention layer
  modeled floor at 174 GB/s: 6319.225 ms
  measured exact attention core: about 6307-6463 ms in recent runs
```

Decision:

```text
The exact long-prefill Attention gap is explained by compulsory K/V streaming,
not by an obvious cache-miss bug in qh4 SIMD32 vec8. Query blocking has to
reduce K/V bytes without triggering the qblk2 register-pressure failure, or the
pipeline needs static lower-precision KV storage.
```

## Prefill Attention Backend Matrix

Latest serial matrix:

```text
tools/run_prefill_attention_backend_matrix.sh \
  --sizes 4096,16384,32768 \
  --accepted-iters 2 \
  --tiled-iters 2 \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

Results:

```text
p4096:
  accepted real attention-core:   0.271157792 s
  MPS tiled Qwen bridge both-KV:  0.015845125 s
  ratio:                         17.11x

p16384:
  accepted real attention-core:   2.171023792 s
  MPS tiled Qwen bridge both-KV:  0.205345750 s
  ratio:                         10.57x

p32768:
  accepted real attention-core:   7.743548542 s
  MPS tiled Qwen bridge both-KV:  0.800409333 s
  ratio:                         9.67x
```

Contract:

```text
The MPS tiled row is synthetic inner-attention evidence only. The raw benchmark
packs a synthetic accepted-layout Q/K/V cache, measures both Qwen KV groups,
and includes bridge pack traffic. It is not an accepted-profile speed claim
until it consumes real Q/K/V, writes the real attention tensor, feeds the real
O projection, and passes quality gates.
```

Correctness note: the first bridge matrix used the wrong token index for the
causal mask (`row % q_tile`). The Q rows are token-major with four Q heads per
KV group, so the correct token index is `row / 4`. The corrected sparse CPU
quality check at p512/q128/k256 reports max absolute error about `2.8e-4`.

Stage profile at p16384:

```text
accepted prepare:        0.129162542 s
accepted attention only: 1.559005500 s
halfdot attention only:  0.849561833 s
```

Implication:

```text
Integrating the exact MPS tiled attention core is now the highest-leverage
exact long-prefill task. QKV prepare is not the dominant p16k stage.
```

## Interleaved K/V Layout Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV=1
semantics: exact
```

Measured:

```text
p1024: bitexact against accepted qh4 SIMD32 vec8
p8192:  0.450825334 s accepted vs 0.464868000 s interleaved
p16384: 1.837716375 s accepted vs 1.874628750 s interleaved
```

Decision:

```text
Reject for promotion. The separate K and V cache buffers are currently better
than alternating K/V halfs for the accepted SIMD32 vec8 access pattern.
```

## Int8 K/V Cache Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV=1
semantics: quantized approximate
layout: int8 K + int8 V + fp16 scale per token/KV-head
```

Measured:

```text
p1024 drift:
  mean_abs_error: 0.001691472
  rms_error:      0.002557171
  max_abs_error:  0.035156250

p8192:
  FP16 K/V accepted: 0.440077250 s
  int8 K/V:          0.468724917 s

p16384:
  FP16 K/V accepted: 1.596366667 s
  int8 K/V:          1.774528625 s
```

Decision:

```text
Reject for promotion. The byte model improves, but this MSL schedule consumes
int8 through scalar char->float conversion in the hot key loop, and that is
slower than streaming FP16 K/V.
```

## Int8 V-Only Cache Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V=1
semantics: quantized approximate
layout: FP16 K + int8 V + fp16 scale per token/KV-head
```

Measured:

```text
p1024 drift:
  mean_abs_error: 0.000575357
  rms_error:      0.000878829
  max_abs_error:  0.009765625

p8192:
  FP16 K/V accepted: 0.427998250 s
  int8 V-only:       0.461115375 s

p16384:
  FP16 K/V accepted: 1.599017250 s
  int8 V-only:       1.723293583 s
```

Decision:

```text
Reject for promotion. Keeping K in FP16 avoids score drift and reduces error,
but scalar int8 V consumption is still slower than FP16 V streaming.
```

## Int8 V PACK4/Broadcast Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4=1
semantics: same as int8 V-only
layout: same byte layout, consumed as packed uint words
```

Measured:

```text
p1024:
  bitexact versus scalar int8 V-only

p8192:
  FP16 K/V:      0.444675333 s
  int8 V:        0.473095250 s
  int8 V PACK4:  0.559044000 s

p16384:
  FP16 K/V:      1.615702958 s
  int8 V:        1.750226125 s
  int8 V PACK4:  2.104403417 s
```

Decision:

```text
Reject for promotion. Broadcast/unpack is worse than scalar int8 loads for this
SIMD32 ownership pattern.
```

## HALFACC Attention Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC=1
semantics: full-context approximate
layout: existing FP16 K/V cache
compute: half q/k/gate/acc, float m/l and exp path
```

Measured:

```text
p8192:
  accepted: 0.440715500 s
  HALFACC:  0.288347334 s

p16384:
  accepted: 1.588621708 s
  HALFACC:  1.013771083 s

p32768:
  accepted: 6.697672416 s
  HALFACC:  4.333045291 s
```

Drift:

```text
p8192 mean_abs_error: 0.002941105
p8192 rms_error:      0.006426603
p8192 max_abs_error:  0.150390625
```

Decision:

```text
Keep as strong approximate candidate. It preserves full context and gives a
real hotloop speedup, but it is not exact and still needs task-quality gates
before any product/default use.
```

## HALFDOT Attention Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT=1
semantics: full-context approximate
layout: existing FP16 K/V cache
compute: half q/k/gate/acc and half score partials, float m/l and exp path
```

Measured:

```text
p8192:
  HALFACC: 0.280871667 s
  HALFDOT: 0.261224500 s

p16384:
  HALFACC: 1.013771083 s
  HALFDOT: 0.919213792 s

p32768:
  HALFACC: 4.333045291 s
  HALFDOT: 4.130694917 s
```

Drift:

```text
p8192 mean_abs_error: 0.002944298
p8192 rms_error:      0.006426372
p8192 max_abs_error:  0.153320312
```

Model-wide estimate with existing MPS DeltaOut path:

```text
p16384: 2102.59 tok/s vs llama.cpp pp16384 2065.71
p32768: 1105.53 tok/s vs llama.cpp pp32768 1325.20
```

Decision:

```text
Keep as strongest approximate full-context attention candidate. It reaches the
p16k reference target in projection but remains approximate and does not close
p32k. The next exact long-context target should be a structural attention
schedule, not another scalar K/V quantization variant.
```

## WINDOW_HALFDOT Attention Probe

Backend:

```text
GPU MSL SIMDgroup
flag: CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT=<tokens>
semantics: approximate sparse attention plus approximate precision
layout: existing FP16 K/V cache
compute: bounded K/V visits, half score/value accumulation, float m/l
```

Measured:

```text
p16384:
  win4096: 0.536036292 s attention-core
  win8192: 0.888198292 s attention-core

p32768:
  win4096:  1.156241041 s attention-core
  win8192:  2.032697458 s attention-core
  win16384: 3.386395417 s attention-core
```

Projected model-wide prefill:

```text
p16384 win4096: 2982.59 tok/s vs llama.cpp 2065.71
p16384 win8192: 2154.04 tok/s vs llama.cpp 2065.71
p32768 win4096: 2778.54 tok/s vs llama.cpp 1325.20
p32768 win8192: 1921.66 tok/s vs llama.cpp 1325.20
p32768 win16384: 1301.65 tok/s vs llama.cpp 1325.20
```

Decision:

```text
Keep as fastest approximate long-prefill candidate. It is not eligible for the
exact accepted profile because it drops old context and adds half-precision
drift. It is useful for a separate approximate/speed profile after model-quality
evaluation.
```

## qh4 Split-K Exact Attention Probe

Backend:

```text
GPU MSL SIMDgroup
flags:
  CTOX_QWEN35_ATTENTION_QH4_SPLITK64=1
  CTOX_QWEN35_ATTENTION_QH4_SPLITK128=1
  CTOX_QWEN35_ATTENTION_QH4_SPLITK256=1
  CTOX_QWEN35_ATTENTION_QH4_SPLITK512=1
semantics: full-context, softmax-order drift only
layout: existing FP16 K/V cache plus partial_m/partial_l/partial_acc scratch
```

Measured:

```text
p4096 accepted qh4 SIMD32 vec8: 0.127131375 s
p4096 splitk64:                0.197536708 s
p4096 splitk128:               0.172208250 s
p4096 splitk256:               0.151201042 s
p4096 splitk512:               0.142699708 s

p8192 accepted qh4 SIMD32 vec8: 0.450240125 s
p8192 splitk256:               0.540268584 s
p8192 splitk512:               0.525104833 s

p16384 accepted qh4 SIMD32 vec8 previous run: 1.588621708 s
p16384 splitk512:                        1.939569000 s
```

Decision:

```text
Reject for promotion. qh4 Split-K proves that GQA-aware split-K is the right
version of the old partial idea, but the full partial_acc scratch and combine
pass erase the parallelism win. Future exact attention work should avoid
materializing [query, head, key_block, head_dim] partial output.
```
