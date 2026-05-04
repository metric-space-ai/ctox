# Qwen3.5-0.8B Metal Research Log

Canonical log for the Qwen3.5-0.8B Metal optimization probe.

The goal is to prove or disprove the CTOX-owned Metal decode strategy on the
smallest relevant Qwen3.5 hybrid model before applying any lessons to 27B or
35B.

## Strategy

```text
GPU:
  Metal decode hot path
  hidden/KV/DeltaNet state/LM-head/argmax stay GPU-local

ANE/NPU:
  separate Core ML benchmark track
  no per-layer switching between Metal and Core ML

CPU:
  orchestration only
  write input token
  read next token
  target: one sync per generated token
```

## Fixed Shape Contract

```text
model:                   Qwen/Qwen3.5-0.8B
parameters:              0.8B
hidden size:             1024
vocab / embedding:       248320 padded
layers:                  24
layout:                  [D, D, D, A] x 6
DeltaNet layers:         18
Full-attention layers:   6
FFN intermediate:        3584
attention Q heads:       8
attention KV heads:      2
attention head dim:      256
attention RoPE dim:      64
DeltaNet QK/V heads:     16 / 16
DeltaNet head dim:       128
native context:          262144
LM output:               248320, tied to token embedding
```

The same constants live in `src/model_shape.rs`.

## Gates

```text
shape-contract:              passed
metal-device-and-bandwidth:  started
hf-artifact-inspection:      started
metalpack-writer:            started
fp16-matvec-1024:            started
gpu-local-lm-head-argmax:    started
tiled-lm-head-contract:      started
packed-matvec-contract:      started
deltanet-step-kernel:        pending
full-decode-greedy-parity:   pending
one-cpu-sync-per-token:      started
synthetic-single-dispatch:   passed
qwen-pattern-single-dispatch: passed
mlx-baseline-beaten:         pending
ane-coreml-baseline:         pending
```

## 2026-04-29

Created:

```text
src/inference/models/qwen35_08b_metal_probe/
```

Verified:

```text
cargo test
  4 passed

cargo run --bin qwen35-08b-metal-research
  layer mix: 18 DeltaNet, 6 full attention
  LM-head FP16 estimate: 485.00 MiB
  approximate FP16 weight estimate: 1.49 GiB
```

First Metal smoke:

```text
command:
  cargo run --release --bin bench_stream -- 16 5

result:
  bytes: 16777216
  iterations: 5
  median_s: 0.001040083
  p95_s: 0.001404250
  effective_gb_s_read_plus_write: 32.26
  checksum: 2723777225
```

Interpretation:

```text
This proves the local owned Metal path works: shader compile, metallib link,
library load, pipeline lookup, dispatch, and CPU wait.

This is not a sustained roofline benchmark yet. It uses shared buffers and a
small 16 MiB input. The next benchmark must add larger buffers, private-storage
variants, stable iteration counts, and device metadata.
```

Matvec smoke:

```text
command:
  cargo run --release --bin bench_matvec -- 3584 10

result:
  shape: [3584 x 1024] @ [1024]
  iterations: 10
  median_s: 0.000344416
  p95_s: 0.002207250
  effective_gb_s_weights_plus_io: 21.36
  checksum16: -55.036552
```

LM-head argmax smoke:

```text
command:
  cargo run --release --bin bench_lm_head -- 8192 5

result:
  shape: [8192 x 1024] @ [1024]
  iterations: 5
  median_s: 0.000852334
  p95_s: 0.000996375
  effective_gb_s_weights_plus_pairs: 19.84
  next_token: 107
  score: 2.418061
```

Full-vocab LM-head argmax:

```text
command:
  cargo run --release --bin bench_lm_head -- full 3

result:
  shape: [248320 x 1024] @ [1024]
  iterations: 3
  median_s: 0.006741292
  p95_s: 0.007429042
  effective_gb_s_weights_plus_pairs: 76.03
  next_token: 107
  score: 2.418061
```

Interpretation:

```text
The LM-head path now satisfies the key structural rule for decode: the CPU does
not read full logits. Metal writes GPU-side (score, token_id) pairs and reduces
them to a single token in the same command-buffer sequence.

This is still a synthetic shared-buffer benchmark, not a tuned production
kernel. The next optimization target is reducing per-row dispatch overhead and
packing weights for coalesced reads before attempting a persistent interpreter.
```

Decode skeleton:

```text
command:
  cargo run --release --bin bench_decode_skeleton -- 8192 5 107

result:
  pipeline: token -> embedding_gather -> lm_head_argmax -> next_token
  shape: [8192 x 1024] tied embedding/lm_head
  input_token: 107
  iterations: 5
  median_s: 0.000685708
  p95_s: 0.001363708
  effective_gb_s_weights_plus_pairs: 24.66
  next_token: 107
  score: 85.641479
```

Full-vocab decode skeleton:

```text
command:
  cargo run --release --bin bench_decode_skeleton -- full 3 107

result:
  pipeline: token -> embedding_gather -> lm_head_argmax -> next_token
  shape: [248320 x 1024] tied embedding/lm_head
  input_token: 107
  iterations: 3
  median_s: 0.006587667
  p95_s: 0.008891541
  effective_gb_s_weights_plus_pairs: 77.80
  next_token: 107
  score: 85.641479
```

Interpretation:

```text
This is the first one-sync token pipeline shape:

CPU writes token_in
GPU gathers the tied embedding row
GPU computes the full-vocab LM-head dot products
GPU reduces to one next_token
CPU reads next_token

It still has no language layers. Its value is structural: it proves the
runtime form that the full mega-pipeline must preserve.
```

Synthetic single-dispatch megakernel:

```text
command:
  cargo run --release --bin bench_mega_synthetic -- 1024 1 24 107

result:
  pipeline: token -> embedding -> 24 synthetic RMS/matvec layers -> lm_head_argmax -> next_token
  shape: vocab=1024 hidden=1024
  iterations: 1
  median_s: 0.003610167
  p95_s: 0.003610167
  estimated_gb_s_weight_stream: 15.10
  next_token: 127
  score: 15.923923
```

Larger synthetic vocab:

```text
command:
  cargo run --release --bin bench_mega_synthetic -- 8192 1 24 107

result:
  pipeline: token -> embedding -> 24 synthetic RMS/matvec layers -> lm_head_argmax -> next_token
  shape: vocab=8192 hidden=1024
  iterations: 1
  median_s: 0.005230209
  p95_s: 0.005230209
  estimated_gb_s_weight_stream: 16.04
  next_token: 127
  score: 15.923923
```

Interpretation:

```text
The first actual single-dispatch megakernel shape now exists. One Metal
threadgroup owns the 1024-wide hidden state, runs 24 synthetic RMS/matvec
layers, scans the LM head, reduces to next_token, and returns only the token.

This is intentionally not the final performant kernel. It serializes too much
inside one threadgroup and uses synthetic dense layers instead of Qwen3.5's
DeltaNet/attention/MLP operators. Its purpose is to lock down the execution
contract and provide a replacement target for real fused Qwen operators.
```

Fused RMS + matvec:

```text
command:
  cargo run --release --bin bench_rms_matvec -- 3584 10

result:
  shape: [3584 x 1024] @ [1024]
  iterations: 10
  median_s: 0.000715417
  p95_s: 0.001954500
  effective_gb_s_weight_norm_input_io: 30.80
  checksum16: -191.748123
```

DeltaNet recurrent step:

```text
command:
  cargo run --release --bin bench_deltanet -- 20

result:
  heads: 16
  head_dim: 128
  state_bytes: 1048576
  iterations: 20
  median_s: 0.000170208
  p95_s: 0.000258083
  effective_gb_s_state_estimate: 18.55
  checksum32: -79414.585938
```

Qwen-pattern single-dispatch megakernel:

```text
command:
  cargo run --release --bin bench_mega_pattern -- 8192 1 107

result:
  pipeline: token -> embedding -> [D,D,D,A]x6 synthetic operators -> lm_head_argmax
  shape: vocab=8192 hidden=1024
  layers: 24
  median_s: 0.002720125
  p95_s: 0.002720125
  estimated_gb_s_weight_stream: 17.02
  next_token: 92
  score: 59.999268
```

Larger Qwen-pattern run:

```text
command:
  cargo run --release --bin bench_mega_pattern -- 32768 1 107

result:
  pipeline: token -> embedding -> [D,D,D,A]x6 synthetic operators -> lm_head_argmax
  shape: vocab=32768 hidden=1024
  layers: 24
  median_s: 0.006734500
  p95_s: 0.006734500
  estimated_gb_s_weight_stream: 21.82
  next_token: 92
  score: 59.999268
```

Interpretation:

```text
The single-dispatch path now follows Qwen3.5-0.8B's layer schedule:
[D,D,D,A] x 6. The D slices are stateful recurrent vector updates and the A
slices are RMS/matvec projections. This is closer to the final execution shape
than the earlier all-matvec synthetic megakernel.

Still missing for a real Qwen decode:
  real safetensors/packed weights
  real DeltaNet matrix-state rule in the megakernel
  real Gated Attention QKV/RoPE/KV update
  real SwiGLU FFN gate/up/down
  full-vocab tiled LM-head without duplicating tied embedding memory
  parity against HF/MLX greedy tokens
```

Artifact inspector and first metalpack writer:

```text
added:
  src/artifacts.rs
  src/pack_plan.rs
  src/metalpack.rs
  src/bin/inspect_artifacts.rs
  src/bin/pack_weights.rs

contract:
  read local HF config.json
  read safetensors headers without loading all tensor data
  validate extracted config fields against Qwen3.5-0.8B shape
  classify tensors into embedding, LM-head, MLP, attention, DeltaNet, norm
  write deterministic manifest.json + weights.bin
  tile FP16 matrices as row_tile x col_tile blocks
```

Verification:

```text
command:
  cargo test

result:
  7 passed
```

Tiled LM-head contract:

```text
command:
  cargo run --release --bin bench_lm_head_tiled -- full 3

result:
  shape: [248320 x 1024] @ [1024]
  tile: rows=8 cols=256
  packed_bytes: 508559360
  iterations: 3
  median_s: 0.005411625
  p95_s: 0.006071458
  effective_gb_s_packed_weights_plus_pairs: 94.71
  next_token: 107
  score: 2.418061
```

Interpretation:

```text
The LM-head path now has a real packed-memory contract. The metalpack writer
emits row-tiled FP16 matrices and the tiled LM-head kernel consumes the same
layout directly. The first per-row tiled kernel was slower than row-major; the
row-tile kernel now computes eight vocab rows per threadgroup and is faster
than the earlier row-major full-vocab smoke on this benchmark.

This still uses synthetic weights. The missing bridge is loading a real local
Qwen3.5-0.8B artifact directory, validating actual tensor names, writing the
metalpack, and feeding the packed LM-head/embedding into decode.
```

Metalpack load-to-GPU smoke:

```text
temporary synthetic artifact:
  /tmp/ctox_qwen35_08b_synth_hf
  model.embed_tokens.weight: F16 [8192, 1024]

commands:
  cargo run --release --bin pack_weights -- --allow-incomplete \
    /tmp/ctox_qwen35_08b_synth_hf \
    /tmp/ctox_qwen35_08b_synth.metalpack

  cargo run --release --bin bench_metalpack_lm_head -- \
    /tmp/ctox_qwen35_08b_synth.metalpack 3

result:
  entries: 1
  packed_bytes: 16777216
  tensor: model.embed_tokens.weight
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  median_s: 0.000495375
  p95_s: 0.000897833
  effective_gb_s_packed_weights_plus_pairs: 34.14
  next_token: 155
  score: 4.224833
```

Interpretation:

```text
The bridge safetensors -> metalpack -> manifest loader -> GPU tiled LM-head is
now present. This is still a synthetic incomplete artifact, but it validates the
runtime handoff shape that the real Qwen3.5-0.8B pack must use.
```

Packed projection kernels:

```text
added:
  qwen35_08b_matvec_rowtiles_fp16_tiled_k1024_f32
  qwen35_08b_rms_matvec_rowtiles_fp16_tiled_k1024_f32
  bench_matvec_tiled
  bench_rms_matvec_tiled
  bench_metalpack_matvec

command:
  cargo run --release --bin bench_matvec_tiled -- 3584 10

result:
  shape: [3584 x 1024]
  tile: rows=8 cols=256
  packed_bytes: 7340032
  median_s: 0.000424709
  p95_s: 0.000994625
  effective_gb_s_packed_weights_plus_io: 17.32
  checksum16: -55.036552

command:
  cargo run --release --bin bench_rms_matvec_tiled -- 3584 10

result:
  shape: [3584 x 1024]
  tile: rows=8 cols=256
  packed_bytes: 7340032
  median_s: 0.000350458
  p95_s: 0.001104917
  effective_gb_s_packed_weight_norm_input_io: 21.00
  checksum16: -191.748123
```

Metalpack matvec smoke:

```text
command:
  cargo run --release --bin bench_metalpack_matvec -- \
    /tmp/ctox_qwen35_08b_synth.metalpack embed 5

result:
  tensor: model.embed_tokens.weight
  class: token_embedding
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  packed_bytes: 16777216
  median_s: 0.000524125
  p95_s: 0.002048417
  effective_gb_s_packed_weights_plus_io: 32.08
  checksum16: -50.453457
```

Metalpack decode skeleton:

```text
added:
  qwen35_08b_embedding_gather_fp16_tiled_k1024
  bench_metalpack_decode_skeleton

command:
  cargo run --release --bin bench_metalpack_decode_skeleton -- \
    /tmp/ctox_qwen35_08b_synth.metalpack 107 3

result:
  embedding: model.embed_tokens.weight
  lm_head: model.embed_tokens.weight
  input_token: 107
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  median_s: 0.000821000
  p95_s: 0.001439833
  effective_gb_s_packed_lm_head_plus_pairs: 20.60
  next_token: 348
  score: 92.076958
```

Interpretation:

```text
The decode skeleton can now run from the same row-tiled metalpack layout as the
projection kernels. The CPU writes one token and reads one token; hidden state,
embedding row, LM-head scores, and reductions stay on GPU. This is still a
language-layer-free skeleton, but the storage contract is no longer synthetic
row-major.
```

Metalpack decode plus one packed projection:

```text
added:
  qwen35_08b_hidden_f32_to_fp16_k1024
  run_decode_one_projection_tiled_with_weights
  bench_metalpack_decode_projection

temporary synthetic artifact:
  /tmp/ctox_qwen35_08b_synth_hf_proj
  model.embed_tokens.weight: F16 [8192, 1024]
  model.layers.0.self_attn.o_proj.weight: F16 [1024, 1024]

contract fix:
  all FP16 row-tiled matrices now use row_tile=8 and col_tile=256
  this keeps embedding, projection, and LM-head address math compatible inside
  a single decode command buffer

command:
  cargo run --release --bin bench_metalpack_decode_projection -- \
    /tmp/ctox_qwen35_08b_synth_proj.metalpack self_attn.o_proj 107 3

result:
  embedding: model.embed_tokens.weight
  projection: model.layers.0.self_attn.o_proj.weight
  lm_head: model.embed_tokens.weight
  input_token: 107
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  median_s: 0.001079458
  p95_s: 0.001086875
  effective_gb_s_projection_lm_head_pairs: 17.61
  next_token: 1145
  score: 13146.418945
```

Interpretation:

```text
This is the first metalpack-backed path with a real layer-shaped operator
between embedding and LM-head:

  token
  -> tiled embedding gather
  -> packed RMSNorm + [1024,1024] projection
  -> F32-to-FP16 hidden cast
  -> tiled LM-head argmax

It is not a complete Qwen layer. It proves that packed projection tensors can
sit inside the same one-command-buffer decode shape. The next replacement is to
turn this single projection slot into a real attention/DeltaNet/FFN slice.
```

Metalpack decode plus FFN slice:

```text
added:
  qwen35_08b_swiglu_f32_to_fp16_i3584
  qwen35_08b_matvec_rowtiles_fp16_tiled_k3584_f32
  run_decode_ffn_tiled_with_weights
  bench_metalpack_decode_ffn

temporary synthetic artifact:
  /tmp/ctox_qwen35_08b_synth_hf_ffn
  model.embed_tokens.weight: F16 [8192, 1024]
  model.layers.0.mlp.gate_proj.weight: F16 [3584, 1024]
  model.layers.0.mlp.up_proj.weight: F16 [3584, 1024]
  model.layers.0.mlp.down_proj.weight: F16 [1024, 3584]

command:
  cargo run --release --bin bench_metalpack_decode_ffn -- \
    /tmp/ctox_qwen35_08b_synth_ffn.metalpack model.layers.0.mlp 107 3

result:
  embedding: model.embed_tokens.weight
  gate: model.layers.0.mlp.gate_proj.weight
  up: model.layers.0.mlp.up_proj.weight
  down: model.layers.0.mlp.down_proj.weight
  lm_head: model.embed_tokens.weight
  input_token: 107
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  median_s: 0.001765166
  p95_s: 0.002267041
  effective_gb_s_ffn_lm_head_pairs: 22.08
  next_token: 1145
  score: 4980365.000000
```

Interpretation:

```text
This is the first metalpack-backed FFN slice in the decode command-buffer
shape:

  token
  -> tiled embedding gather
  -> packed RMSNorm + gate_proj
  -> packed RMSNorm + up_proj
  -> SwiGLU
  -> packed down_proj
  -> F32-to-FP16 hidden cast
  -> tiled LM-head argmax

It is still not numerically representative because the test weights are
synthetic and unnormalized, but the dataflow now matches the Qwen MLP structure
at the operator level.
```

Repeated FFN stack in one command buffer:

```text
added:
  run_decode_repeated_ffn_tiled_with_weights
  bench_metalpack_decode_ffn_stack

command:
  cargo run --release --bin bench_metalpack_decode_ffn_stack -- \
    /tmp/ctox_qwen35_08b_synth_ffn.metalpack 6 model.layers.0.mlp 107 3

result:
  ffn_layers: 6
  shape: [8192 x 1024]
  tile: rows=8 cols=256
  median_s: 0.003685709
  p95_s: 0.003881625
  effective_gb_s_repeated_ffn_lm_head_pairs: 40.51
  next_token: 1145
  score: 5784412.500000

command:
  cargo run --release --bin bench_metalpack_decode_ffn_stack -- \
    /tmp/ctox_qwen35_08b_synth_ffn.metalpack 24 model.layers.0.mlp 107 1

result:
  ffn_layers: 24
  median_s: 0.010740750
  effective_gb_s_repeated_ffn_lm_head_pairs: 50.88
  next_token: 1145
  score: 5784412.500000
```

Interpretation:

```text
This is the first Superblock-style command-buffer pipeline: embedding once,
then N packed FFN slices back-to-back, then one tiled LM-head argmax. There is
still no CPU roundtrip between slices. It is not a single Metal dispatch, but
it is the stable Metal-safe form of the megakernel strategy: dispatch
boundaries provide global ordering while all activations stay GPU-local.

The next missing Qwen operators are DeltaNet and attention. The FFN path is now
good enough to be used as the repeated feed-forward half of future D/A layer
slices.
```

## 2026-04-29 - Metalpack Attention Operator Slice

Added:

```text
vendor/metal/shaders/qwen35_08b/attention_single_token.metal
run_decode_attention_tiled_with_weights
bench_metalpack_decode_attention
research gate: metalpack-decode-plus-attention = passed
```

Command-buffer shape:

```text
token
-> tiled embedding gather
-> packed RMSNorm + q_proj
-> packed RMSNorm + k_proj
-> packed RMSNorm + v_proj
-> single-token attention combine
-> packed o_proj
-> F32-to-FP16 hidden cast
-> tiled LM-head argmax
```

Synthetic artifact:

```text
/tmp/ctox_qwen35_08b_synth_attn_hf
  model.embed_tokens.weight                  [8192, 1024]
  model.layers.0.self_attn.q_proj.weight     [1024, 1024]
  model.layers.0.self_attn.k_proj.weight     [1024, 1024]
  model.layers.0.self_attn.v_proj.weight     [1024, 1024]
  model.layers.0.self_attn.o_proj.weight     [1024, 1024]

/tmp/ctox_qwen35_08b_synth_attn.metalpack
  entries: 5
  packed_bytes: 25165824
  tile: rows=8 cols=256
```

Run:

```text
cargo run --release --bin bench_metalpack_decode_attention -- \
  /tmp/ctox_qwen35_08b_synth_attn.metalpack model.layers.0.self_attn 107 3
```

Result:

```text
qwen35-08b metalpack decode + attention benchmark
input_token: 107
shape: [8192 x 1024]
tile: rows=8 cols=256
iterations: 3
median_s: 0.000598042
p95_s: 0.000704625
effective_gb_s_attention_lm_head_pairs: 42.34
next_token: 0
score: 0.000000
```

Interpretation:

```text
This is not yet full Qwen attention. It intentionally proves the next
metalpack-backed GPU-local operator slice: packed Q/K/V projections, a
single-token attention combine, packed O projection, and GPU LM-head argmax in
one command buffer.

The missing production pieces are still RoPE, grouped query/head layout,
KV-cache update/read, and online softmax over prior context. Those belong in
the next attention milestone. The useful result here is that the Q/K/V/O
projection traffic now uses the same tiled pack layout as FFN and LM-head, so
the D/D/D/A scheduler can start consuming both FFN and attention slices without
changing the weight container.
```

## 2026-04-29 - Metalpack DeltaNet Operator Slice

Added:

```text
qwen35_08b_matvec_rowtiles_fp16_tiled_k2048_f32
qwen35_08b_deltanet_split_qkv_f32_to_fp16_h16d128
qwen35_08b_deltanet_activate_beta_gate_h16
qwen35_08b_deltanet_apply_z_gate_f32_to_fp16_k2048
run_decode_deltanet_tiled_with_weights
bench_metalpack_decode_deltanet
research gate: metalpack-decode-plus-deltanet = passed
```

Command-buffer shape:

```text
token
-> tiled embedding gather
-> packed RMSNorm + in_proj_qkv
-> packed RMSNorm + in_proj_z
-> packed RMSNorm + in_proj_b
-> packed RMSNorm + in_proj_a
-> split q/k/v into half vectors
-> beta/gate activation
-> recurrent DeltaNet state update
-> z-gated DeltaNet output
-> packed out_proj
-> F32-to-FP16 hidden cast
-> tiled LM-head argmax
```

Synthetic artifact:

```text
/tmp/ctox_qwen35_08b_synth_delta_hf
  model.embed_tokens.weight                  [8192, 1024]
  model.layers.0.in_proj_qkv.weight          [6144, 1024]
  model.layers.0.in_proj_z.weight            [2048, 1024]
  model.layers.0.in_proj_b.weight            [16, 1024]
  model.layers.0.in_proj_a.weight            [16, 1024]
  model.layers.0.out_proj.weight             [1024, 2048]

/tmp/ctox_qwen35_08b_synth_delta.metalpack
  entries: 6
  packed_bytes: 37814272
  tile: rows=8 cols=256
```

Run:

```text
cargo run --release --bin bench_metalpack_decode_deltanet -- \
  /tmp/ctox_qwen35_08b_synth_delta.metalpack model.layers.0 107 3
```

Result:

```text
qwen35-08b metalpack decode + deltanet benchmark
input_token: 107
shape: [8192 x 1024]
delta_width: 2048 heads=16 head_dim=128
tile: rows=8 cols=256
iterations: 3
median_s: 0.001925417
p95_s: 0.002727209
effective_gb_s_deltanet_lm_head_pairs: 20.83
next_token: 214
score: 33908.570312
```

Interpretation:

```text
This is now the first metalpack-backed D-layer slice. The recurrent matrix
state lives in a GPU buffer and is updated by the Metal DeltaNet kernel; the CPU
does not observe q/k/v, state, hidden activations, or logits.

It is still not full Qwen DeltaNet parity. The remaining pieces are the exact
HF causal conv update, exact a/b/dt/A_log gating math, exact gated RMSNorm, and
numerical comparison against a reference implementation. The dataflow and
buffer ownership are now close enough to wire into a D/D/D/A layer scheduler.
```

## 2026-04-29 - Metalpack D/D/D/A Superblock

Added:

```text
run_decode_superblock_tiled_with_weights
bench_metalpack_decode_superblock
research gate: metalpack-ddda-superblock = passed
```

Command-buffer shape:

```text
token
-> tiled embedding gather
-> D mixer
-> FFN
-> D mixer
-> FFN
-> D mixer
-> FFN
-> A mixer
-> FFN
-> tiled LM-head argmax
```

The first version intentionally reuses one packed DeltaNet slice, one packed
attention slice, and one packed FFN slice. That keeps the milestone focused on
the scheduler and buffer-lifetime problem: no CPU readback between the four
mixers, no CPU readback between FFNs, hidden activations ping-pong only between
GPU buffers, and only the final token/score is read by the CPU.

Synthetic artifact:

```text
/tmp/ctox_qwen35_08b_synth_superblock_hf
  model.embed_tokens.weight                  [8192, 1024]
  model.layers.0.in_proj_qkv.weight          [6144, 1024]
  model.layers.0.in_proj_z.weight            [2048, 1024]
  model.layers.0.in_proj_b.weight            [16, 1024]
  model.layers.0.in_proj_a.weight            [16, 1024]
  model.layers.0.out_proj.weight             [1024, 2048]
  model.layers.3.self_attn.q_proj.weight     [1024, 1024]
  model.layers.3.self_attn.k_proj.weight     [1024, 1024]
  model.layers.3.self_attn.v_proj.weight     [1024, 1024]
  model.layers.3.self_attn.o_proj.weight     [1024, 1024]
  model.layers.0.mlp.gate_proj.weight        [3584, 1024]
  model.layers.0.mlp.up_proj.weight          [3584, 1024]
  model.layers.0.mlp.down_proj.weight        [1024, 3584]

/tmp/ctox_qwen35_08b_synth_superblock.metalpack
  entries: 13
  packed_bytes: 68222976
  tile: rows=8 cols=256
```

Run:

```text
cargo run --release --bin bench_metalpack_decode_superblock -- \
  /tmp/ctox_qwen35_08b_synth_superblock.metalpack \
  model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 3
```

Result:

```text
qwen35-08b metalpack decode D/D/D/A superblock benchmark
input_token: 107
shape: [8192 x 1024]
superblocks: 1
pattern: D+FFN D+FFN D+FFN A+FFN
tile: rows=8 cols=256
iterations: 3
median_s: 0.004365250
p95_s: 0.006631584
effective_gb_s_superblock_lm_head_pairs: 41.90
next_token: 190
score: 0.004276
```

Interpretation:

```text
This is the first stable form of the Qwen3.5 Metal mega-pipeline. It is not a
single Metal dispatch and not numerically complete yet, but it has the intended
runtime shape: the CPU encodes a static sequence once per token, the GPU owns
the model-local hot path for a full [D,D,D,A] block, and the CPU only observes
the sampled token after LM-head argmax.

The next scheduler milestone is scaling this from one reused superblock to six
superblocks with distinct layer prefixes and then replacing placeholder
DeltaNet/attention math with exact Qwen parity kernels.
```

## 2026-04-29 - Full 24-Layer Pattern Scheduler

Added:

```text
bench_metalpack_decode_superblock optional superblocks argument
separate DeltaNet state slots per D-layer in repeated superblocks
research gate: metalpack-full-pattern-24layer-scheduler = passed
```

Run:

```text
cargo run --release --bin bench_metalpack_decode_superblock -- \
  /tmp/ctox_qwen35_08b_synth_superblock.metalpack \
  model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 1 6
```

Result:

```text
qwen35-08b metalpack decode D/D/D/A superblock benchmark
input_token: 107
shape: [8192 x 1024]
superblocks: 6
pattern: D+FFN D+FFN D+FFN A+FFN repeated 6 times
tile: rows=8 cols=256
iterations: 1
median_s: 0.015260792
p95_s: 0.015260792
effective_gb_s_superblock_lm_head_pairs: 66.33
next_token: 244
score: 0.004407
```

Interpretation:

```text
This is the first full-topology Qwen3.5-0.8B decode scheduler in the probe:
18 D-style mixer slots, 6 A-style mixer slots, 24 FFN slots, one LM-head, one
CPU synchronization at the end. The weights are still reused from one
synthetic D slice, one synthetic A slice, and one synthetic FFN slice, so this
is not a real model run. It does prove the command-buffer structure and state
allocation pattern needed for a real 24-layer metalpack execution.

Next, the runtime needs to bind distinct layer-prefix entries from a real or
complete synthetic pack instead of reusing the same slice, then run exact parity
operators.
```

## 2026-04-29 - Layer-Specific 24-Layer Binding API

Added:

```text
DeltaLayerTiled
AttentionLayerTiled
FfnLayerTiled
run_decode_layered_pattern_tiled_with_weights
bench_metalpack_decode_layered_pattern
research gate: metalpack-layer-specific-24layer-binding = passed
```

The previous full-pattern scheduler accepted one DeltaNet slice, one attention
slice, and one FFN slice plus a repeat count. This milestone adds the runtime
shape needed by a real model pack:

```text
delta_layers:    18 independent layer entries
attention_layers: 6 independent layer entries
ffn_layers:      24 independent layer entries
```

The current benchmark still fills those arrays from the existing synthetic
template entries to avoid writing a >1 GiB synthetic full-weight pack during
this step. The Metal execution path does not use a repeat-count shortcut: it
walks the 24-layer Qwen pattern and indexes the corresponding layer array slot
for every D, A, and FFN dispatch.

Run:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_superblock.metalpack \
  model.layers.0 model.layers.3.self_attn model.layers.0.mlp 107 1
```

Result:

```text
qwen35-08b metalpack decode layered 24-layer pattern benchmark
input_token: 107
shape: [8192 x 1024]
layers: delta=18 attention=6 ffn=24
tile: rows=8 cols=256
iterations: 1
median_s: 0.016812584
p95_s: 0.016812584
effective_gb_s_layered_pattern_lm_head_pairs: 60.21
next_token: 244
score: 0.004407
```

Interpretation:

```text
This is the binding shape required for a real Qwen3.5-0.8B metalpack. The next
step is not another scheduler refactor; it is resolving real pack entries by
layer id and feeding these arrays with distinct tensors from either a complete
synthetic pack or an actual local Qwen3.5-0.8B artifact.
```

## 2026-04-29 - Auto Layer Resolver

Added:

```text
bench_metalpack_decode_layered_pattern auto binding mode
fallback mode for small template packs
research gate: metalpack-auto-layer-resolver = passed
```

The layered-pattern benchmark now first tries to resolve a full manifest by
metadata:

```text
key: layer id + TensorClass
DeltaNet layers:   layers where layer % 4 != 3
Attention layers:  layers where layer % 4 == 3
FFN layers:        all layers 0..23
```

If the manifest is incomplete, it falls back to the previous template-prefix
mode. That keeps the small synthetic packs useful while making a complete
`.metalpack` executable without hand-written prefix lists.

To test the auto path without generating a >1 GiB full synthetic pack, an alias
manifest was created at:

```text
/tmp/ctox_qwen35_08b_synth_full_alias.metalpack
entries: 187
weights.bin: reused from /tmp/ctox_qwen35_08b_synth_superblock.metalpack
```

Run:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_full_alias.metalpack ignored ignored ignored 107 1
```

Result:

```text
qwen35-08b metalpack decode layered 24-layer pattern benchmark
binding_mode: auto-layer-id
input_token: 107
shape: [8192 x 1024]
layers: delta=18 attention=6 ffn=24
tile: rows=8 cols=256
iterations: 1
median_s: 0.011087333
p95_s: 0.011087333
effective_gb_s_layered_pattern_lm_head_pairs: 91.30
next_token: 244
score: 0.004407
```

Interpretation:

```text
The runtime can now consume the same manifest structure a real full Qwen3.5-0.8B
pack should emit. The remaining blocker to a real model run is no longer
scheduler binding; it is exact operator parity and shape support for the real
Qwen3.5 projection tensors if they differ from the current placeholder kernels.
```

Next:

```text
1. Run inspect_artifacts and pack_weights on a real local Qwen3.5-0.8B HF directory.
2. Add runtime buffer binding from metalpack entries into private/shared Metal buffers.
3. Replace DeltaNet placeholder math with exact causal-conv/gated-delta rule.
4. Replace attention placeholder with real QKV/RoPE/KV/online-softmax path.
5. Resolve real Qwen3.5 tensor shapes against the current placeholder assumptions.
6. Feed real packed embedding/LM-head into the decode skeleton.
7. Add reference-token capture contract against HF/MLX.
```

## 2026-04-29 - Shape Audit and Qwen GQA Attention Shapes

Added:

```text
audit_shapes
make_synthetic_metalpack
research gates:
  metalpack-shape-audit = passed
  synthetic-true-shape-metalpack = passed
  metalpack-qwen-gqa-attention-shapes = passed
```

The audit now compares three things for every relevant tensor:

```text
expected Qwen3.5 shape
current Metal kernel-supported shape
actual metalpack manifest shape
```

The old alias pack intentionally failed only on the 6 attention layers:

```text
q expected [2048,1024] or [2056,1024], actual [1024,1024]
k/v expected [512,1024], actual [1024,1024]
o expected [1024,2048], actual [1024,1024]
```

The runtime and shader path were then changed to accept the actual Qwen GQA
contract:

```text
attention q projection:     2048 or 2056 rows from hidden 1024
attention k/v projection:   512 rows from hidden 1024
attention combine scratch:  2048 half values
attention o projection:     hidden 1024 rows from K=2048
```

`make_synthetic_metalpack` writes one compact `weights.bin` with one template
slice per shape and a full 187-entry manifest. Layer entries alias those
template slices by offset, so the scheduler sees the real full-layer metadata
without writing a full synthetic 0.8B pack.

Build true-shape synthetic pack:

```text
cargo run --release --bin make_synthetic_metalpack -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack 8192 1
```

Result:

```text
entries: 187
packed_bytes: 70336512
vocab_rows: 8192
attention_q_rows: 2056
```

Audit:

```text
cargo run --bin audit_shapes -- /tmp/ctox_qwen35_08b_synth_true_shape.metalpack

summary: supported=188 placeholder=0 missing=0 unsupported=0
```

Attention slice:

```text
cargo run --release --bin bench_metalpack_decode_attention -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack model.layers.3.self_attn 107 1

median_s: 0.001596584
effective_gb_s_attention_lm_head_pairs: 17.18
next_token: 5301
score: 9.360816
```

Full 24-layer layered scheduler:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1

binding_mode: auto-layer-id
layers: delta=18 attention=6 ffn=24
median_s: 0.016331833
effective_gb_s_layered_pattern_lm_head_pairs: 62.76
next_token: 5842
score: 0.097068
```

Interpretation:

```text
The full packed scheduler no longer depends on the old hidden x hidden
attention placeholder shape. It can execute the Qwen3.5 24-layer topology with
real GQA projection dimensions, one command buffer per token, GPU-local
scratch/state, and CPU-visible output limited to the argmax pair.
```

Still not done:

```text
The GQA combine kernel is shape-correct but still a single-token placeholder.
It expands the current V head to the four Q heads and applies the optional
head gate. It does not yet implement RoPE, KV-cache history, online softmax,
or exact Qwen attention parity.

Next hard blocker:
replace the placeholder GQA combine with real RoPE + KV-cache update + online
softmax over cached history, then capture greedy reference tokens.
```

## 2026-04-29 - Attention KV Cache Surface

Added:

```text
research gate:
  attention-kv-cache-online-softmax-surface = passed
```

The GQA attention combine dispatch now has the runtime surface required for
stateful decode:

```text
inputs:
  q_f32 [2048 or 2056]
  k_f32 [512]
  v_f32 [512]

GPU state:
  k_cache [attention_layer, context, 512] half
  v_cache [attention_layer, context, 512] half

output:
  attention half [2048]
```

The kernel writes the current K/V vectors into cache, applies a first RoPE path
to Q/K, and computes the attention output with an online-softmax loop over the
cache positions. The attention, superblock, and layered-pattern benchmark CLIs
now expose `decode_position` and `max_context`; the current validation still
uses position 0, so this validates the command/buffer/kernel contract without
claiming long-context parity yet.

Re-run after the KV-cache surface change:

```text
cargo run --release --bin bench_metalpack_decode_attention -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack model.layers.3.self_attn 107 1 0 4

median_s: 0.001240666
effective_gb_s_attention_lm_head_pairs: 22.12
next_token: 5301
score: 9.360670
```

Full 24-layer path:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4

binding_mode: auto-layer-id
layers: delta=18 attention=6 ffn=24
median_s: 0.016062792
effective_gb_s_layered_pattern_lm_head_pairs: 63.81
next_token: 5842
score: 0.066540
```

Remaining Attention work:

```text
1. Run multi-token decode steps so position > 0 reads previously written cache.
2. Validate RoPE theta and layout against the real Qwen3.5 implementation.
3. Compare attention outputs against a captured reference before using real tokens.
4. Keep KV cache persistent across generated tokens instead of per-benchmark call.
```

## 2026-04-29 - Multi-Step Attention KV Cache Smoke

Added:

```text
bench_metalpack_decode_attention_steps
research gate:
  attention-multistep-kv-cache-smoke = passed
```

This benchmark allocates the attention K/V cache once, then runs token positions
`0..steps` sequentially. After every token it reads only the argmax token and
score pair, writes the next token back to the shared token buffer, and keeps the
GPU cache allocation alive for the next position.

Run:

```text
cargo run --release --bin bench_metalpack_decode_attention_steps -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack model.layers.3.self_attn 107 4 1 4
```

Result:

```text
steps: 4
max_context: 4
median_s: 0.004363417
effective_gb_s_attention_sequence_lm_head_pairs: 25.15
tokens: [5301, 510, 510, 510]
last_score: 3.890219
```

Interpretation:

```text
The attention slice now exercises position > 0 with a persistent cache inside a
single benchmark sequence. This is still a slice-level smoke test, not full
language parity. The next bridge is the same multi-step contract for the full
24-layer D/D/D/A scheduler, with persistent DeltaNet state and per-attention
KV caches across generated tokens.
```

## 2026-04-29 - Full 24-Layer Multi-Step State/Cache Smoke

Added:

```text
run_decode_layered_pattern_tiled_sequence_with_weights
bench_metalpack_decode_layered_pattern optional steps argument
research gate:
  layered-24layer-multistep-state-cache-smoke = passed
```

The layered-pattern runtime now has a sequence path. It allocates all model
buffers once, keeps the 18 DeltaNet recurrent state buffers and six attention
KV-cache regions alive, and then runs the full `[D,D,D,A]x6` scheduler for
`steps` generated tokens. Between token positions, CPU reads only the argmax
token/score pair and writes the next token back to the shared token buffer.

Run:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4
```

Result:

```text
binding_mode: auto-layer-id
layers: delta=18 attention=6 ffn=24
steps: 4
max_context: 4
median_s: 0.058846750
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 67.75
tokens: [5698, 5698, 5698, 5301]
last_score: 0.113124
```

Interpretation:

```text
The full synthetic true-shape 24-layer path now has the lifecycle shape needed
for batch-1 autoregressive decode: one buffer setup, one command-buffer decode
per token, GPU-local recurrent/cache state across token positions, and only
next-token readback between positions.
```

Next:

```text
1. Replace placeholder DeltaNet math with exact Qwen causal-conv/gated-delta rule.
2. Validate RoPE and attention numerics against reference tensors.
3. Run the true-shape path against real Qwen3.5-0.8B packed weights.
4. Capture greedy reference tokens and compare generated token sequence.
```

## 2026-04-29 - DeltaNet State-Param Audit

Added:

```text
synthetic metalpack RawState entries:
  A_log
  dt_bias
  conv1d.weight
  conv1d.bias

research gate:
  deltanet-state-param-audit = passed
```

The shape audit now distinguishes the implemented DeltaNet projection surface
from the still-placeholder recurrent math. The synthetic true-shape pack now
contains the per-Delta-layer decay and causal-conv state parameters, and the
audit marks them explicitly as `KernelPlaceholder` until the Metal decode path
actually consumes them.

Rebuilt synthetic pack:

```text
cargo run --release --bin make_synthetic_metalpack -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack 8192 1

entries: 259
packed_bytes: 70398080
vocab_rows: 8192
attention_q_rows: 2056
```

Audit:

```text
cargo run --bin audit_shapes -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack

summary: supported=188 placeholder=72 missing=0 unsupported=0
```

Interpretation:

```text
The core true-shape packed projection/LM-head surface remains supported. The
72 placeholders are intentional: 18 DeltaNet layers × 4 state-param entries.
This prevents the 24-layer scheduler from being counted as numerically complete
until A_log, dt_bias, and causal Conv1D are wired into the Metal kernels.
```

Verification:

```text
cargo fmt && cargo test
cargo run --bin qwen35-08b-metal-research

research gates: 22 passed, 8 pending
```

## 2026-04-29 - DeltaNet Beta/Decay Activation Kernel

Added:

```text
Metal kernel:
  qwen35_08b_deltanet_activate_beta_decay_h16

Benchmark CLI:
  bench_deltanet_decay

research gate:
  deltanet-decay-activation-kernel = passed
```

This replaces the previous DeltaNet "gate as sigmoid(a)" experiment with a
separate exact activation surface for the recurrent decay inputs:

```text
beta  = sigmoid(b)
decay = exp(-exp(A_log) * softplus(a + dt_bias))
```

The kernel is still isolated from the full 24-layer scheduler. That is
intentional: the next scheduler change should bind real `A_log` and `dt_bias`
buffers from the metalpack rather than hiding synthetic constants inside the
decode path.

Run:

```text
cargo run --release --bin bench_deltanet_decay -- 20
```

Result:

```text
heads: 16
iterations: 20
median_s: 0.000341750
p95_s: 0.000555959
max_abs_error_beta: 0.000000119
max_abs_error_decay: 0.000000060
checksum: 23.594406
```

Verification:

```text
cargo fmt && cargo test
cargo run --bin qwen35-08b-metal-research

research gates: 23 passed, 8 pending
```

## 2026-04-29 - Layered DeltaNet Decay + Conv1D Binding

Added:

```text
Metal kernels:
  qwen35_08b_deltanet_activate_beta_decay_h16
  qwen35_08b_deltanet_causal_conv1d_update_silu_c6144_k4

layered scheduler bindings:
  A_log / dt_bias raw F32 buffers
  conv1d.weight / conv1d.bias raw F16 buffers
  persistent per-Delta-layer Conv1D state

research gates:
  layered-deltanet-decay-param-binding = passed
  layered-deltanet-conv1d-state-binding = passed
```

The full 24-layer layered scheduler now routes each DeltaNet layer as:

```text
RMS+in_proj_qkv
-> causal Conv1D state update + SiLU
-> Q/K/V split
-> beta/decay activation from b/a/A_log/dt_bias
-> recurrent f32 state step
-> z gate
-> out projection
```

The synthetic true-shape audit now has no remaining shape-surface placeholders:

```text
cargo run --bin audit_shapes -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack

summary: supported=260 placeholder=0 missing=0 unsupported=0
```

Full 24-layer multi-step smoke with the new DeltaNet bindings:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

binding_mode: auto-layer-id
layers: delta=18 attention=6 ffn=24
steps: 4
max_context: 4
median_s: 0.064603375
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 61.71
tokens: [5, 5, 5, 5]
last_score: 0.122193
```

Verification:

```text
cargo clean && cargo test
cargo run --bin qwen35-08b-metal-research

research gates: 25 passed, 8 pending
```

Remaining correctness gap:

```text
The DeltaNet recurrent kernel still needs numerical parity against a captured
reference, especially q/k RMS scaling, exact gated RMSNorm after recurrent
output, residual/norm placement, and real-weight greedy-token parity.
```

## 2026-04-30 - DeltaNet Step CPU Reference Gate

Added:

```text
bench_deltanet CPU reference comparison:
  GPU out buffer vs CPU recurrent step
  GPU mutated f32 state vs CPU mutated state

research gate:
  deltanet-step-kernel = passed

new open gate:
  deltanet-recurrent-multistep-stability = pending
```

Single-step run:

```text
cargo run --release --bin bench_deltanet -- 1

heads: 16
head_dim: 128
state_bytes: 1048576
iterations: 1
median_s: 0.000134292
effective_gb_s_state_estimate: 23.52
max_abs_error_out: 0.000000075
max_abs_error_state: 0.000000007
checksum32: -0.277799
```

The one-step recurrent update now matches the CPU reference within float noise.
The same synthetic vector repeated 20 times is not stable:

```text
cargo run --release --bin bench_deltanet -- 20

max_abs_error_out: 2176.000000000
max_abs_error_state: 5888.000000000
```

Interpretation:

```text
The Metal kernel implements the single-step rule correctly. Multi-step drift
must be tested with the realistic decode sequence after q/k RMS normalization,
causal Conv1D, and bounded decay from A_log/dt_bias; the current repeated
synthetic vector is a stress case, not language-parity evidence.
```

## 2026-04-30 - DeltaNet Q/K L2Norm + Gated RMSNorm Binding

Reference alignment:

```text
HF Qwen3.5 DeltaNet decode applies:
  query = l2norm(query) * (1 / sqrt(head_dim))
  key   = l2norm(key)
  core  = recurrent_gated_delta_rule(query, key, value, decay, beta)
  out   = RMSNorm(core) * norm.weight * SiLU(z)
```

Added Metal kernels:

```text
qwen35_08b_deltanet_qk_l2norm_scale_h16d128
qwen35_08b_deltanet_gated_rmsnorm_f32_to_fp16_h16d128
```

Metalpack/schema changes:

```text
DeltaNet norm.weight is now an explicit RawState tensor:
  model.layers.N.mixer.norm.weight
  class: delta_state_param
  dtype: F32
  shape: [128]

synthetic true-shape metalpack:
  entries: 277
  packed_bytes: 70398592

shape audit:
  summary: supported=278 placeholder=0 missing=0 unsupported=0
```

24-layer sequence smoke:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

binding_mode: auto-layer-id
layers: delta=18 attention=6 ffn=24
steps: 4
median_s: 0.057898666
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 68.86
tokens: [7465, 7465, 7465, 7465]
last_score: 0.118732
```

Research gates:

```text
layered-deltanet-qk-l2norm-binding = passed
layered-deltanet-gated-rmsnorm-binding = passed
```

## 2026-04-30 - BF16 Real-Weight Packing Guard

Issue:

```text
Real Qwen checkpoints may store projection matrices and state vectors as BF16.
Metal half kernels cannot consume raw BF16 bits as FP16.
```

Added:

```text
metalpack writer:
  F16 row-tiled matrices: direct FP16 tile copy
  BF16 row-tiled matrices: convert each BF16 value to FP16 while tiling

layered decode loader:
  float RawState accepts F32/F16/BF16 and uploads f32 buffers
  half RawState accepts F16/BF16/F32 and uploads FP16 buffers

pack plan:
  real pack_weights now blocks if any DeltaNet layer is missing:
    A_log
    dt_bias
    conv1d.weight
    conv1d.bias
    norm.weight
```

Validation:

```text
cargo test
  includes BF16 -> FP16 packed embedding fixture

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

median_s: 0.062558959
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 63.73
tokens: [7465, 7465, 7465, 7465]
```

Research gate:

```text
metalpack-writer = passed
```

## 2026-04-30 - GPU Residual Add Binding

Issue:

```text
The layered scheduler was still writing token-mixer and FFN projections as the
next hidden state. Qwen decoder layers require residual + projection after the
token mixer and again after the FFN.
```

Added:

```text
qwen35_08b_residual_add_f32_to_fp16_k1024

layered 24-layer scheduler:
  attention o_proj -> residual add
  DeltaNet out_proj -> residual add
  FFN down_proj -> residual add
```

Validation:

```text
cargo test

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

median_s: 0.053098125
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 75.08
tokens: [114, 541, 110, 25]
last_score: 2.597090
```

Research gate:

```text
layered-decoder-residual-add-binding = passed
```

## 2026-04-30 - Per-Layer RMSNorm Binding

Issue:

```text
The layered scheduler still used one synthetic norm vector for every layer.
Real Qwen decode needs each decoder layer's input_layernorm before the token
mixer and post_attention_layernorm before the FFN.
```

Added:

```text
full layer binding:
  model.layers.N.input_layernorm.weight
  model.layers.N.post_attention_layernorm.weight

runtime:
  DeltaNet/attention projections use layer.input_norm
  FFN gate/up projections use layer.post_norm
  loader converts Qwen RMSNorm weights to effective (1 + weight) FP16 vectors

pack plan:
  pack_weights blocks if either per-layer RMSNorm tensor is missing
```

Validation:

```text
synthetic true-shape metalpack:
  entries: 326
  packed_bytes: 70400640
  binding_mode: auto-layer-id

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

median_s: 0.057647708
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 69.16
tokens: [114, 541, 110, 25]
last_score: 2.540340
```

Research gate:

```text
layered-decoder-layernorm-binding = passed
```

## 2026-04-30 - Final RMSNorm Before LM Head

Issue:

```text
The layered scheduler scored LM-head logits directly from the last layer output.
Qwen requires final RMSNorm before the LM head.
```

Added:

```text
qwen35_08b_rmsnorm_hidden_fp16_k1024

layered 24-layer scheduler:
  last hidden -> final RMSNorm -> LM-head row-tiled score -> GPU argmax

bench loader:
  reads model.norm.weight as FinalNorm
  converts Qwen RMSNorm weight to effective (1 + weight) FP16 vector
```

Validation:

```text
cargo test

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

median_s: 0.060599708
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 65.79
tokens: [77, 114, 114, 977]
last_score: 1.194886
```

Research gate:

```text
gpu-final-rmsnorm-before-lm-head = passed
```

## 2026-04-30 - Compact Next-Token Output

Issue:

```text
The layered sequence runtime still read the final argmax score/id pair buffers
after each token. That was fine for benchmarking, but it did not satisfy the
runtime target: the decode loop should consume only next_token per generated
token.
```

Added:

```text
qwen35_08b_argmax_pair_to_token_score

layered 24-layer scheduler:
  LM-head score pairs -> GPU pair reductions -> compact out_token/out_score

sequence runtime:
  reads only out_token inside the autoregressive step loop
  reads out_score once after the sequence for benchmark reporting
```

Validation:

```text
cargo test

cargo run --bin qwen35-08b-metal-research
research gates: 34 passed, 6 pending

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_synth_true_shape.metalpack ignored ignored ignored 107 1 0 4 4

median_s: 0.051114625
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 78.00
tokens: [77, 114, 114, 977]
last_score: 1.194886
```

Research gates:

```text
gpu-local-lm-head-argmax = passed
one-cpu-sync-per-token = passed
```

## 2026-04-30 - Attention RoPE/KV CPU Reference

Issue:

```text
The layered scheduler used the GQA RoPE/KV-cache attention kernel, but that
kernel had only smoke coverage. It needed a deterministic numerical check
before treating the attention path as a reliable part of the Qwen pipeline.
```

Added:

```text
bench_attention_reference

coverage:
  synthetic q/k/v traces over multiple decode positions
  persistent GPU KV cache
  CPU reference for RoPE, GQA head mapping, sigmoid head gate, softmax, and FP16 cache/output quantization
```

Validation:

```text
cargo run --release --bin bench_attention_reference -- 4 4

steps: 4
max_context: 4
heads: q=8 kv=2 dim=256
max_abs_err: 0.00000000
tolerance: 0.00250000
```

Research gate:

```text
attention-rope-kv-reference = passed
```

## 2026-04-30 - DeltaNet Normalized Multistep Stability

Issue:

```text
The old repeated-vector DeltaNet stress check used unnormalized Q/K and reused
the same trace for many iterations. That remains useful as a stress case, but it
does not represent the decode path after Q/K L2 normalization and decay binding.
```

Added:

```text
bench_deltanet_stability

coverage:
  20-step changing synthetic trace
  per-head Q L2Norm + rsqrt(head_dim) scaling
  per-head K L2Norm
  decay factors < 1.0
  CPU reference with old-state snapshot semantics
```

Validation:

```text
cargo run --release --bin bench_deltanet_stability -- 20

steps: 20
heads: 16
head_dim: 128
max_abs_error_out: 0.000000000
max_abs_error_state: 0.000000000
tolerance: 0.000500000
```

Research gate:

```text
deltanet-recurrent-multistep-stability = passed
```

## 2026-04-30 - Metal Bandwidth and FP16 Matvec Baselines

Purpose:

```text
Close the two baseline measurement gates that the Qwen3.5 Metal pipeline uses
as roofline context: raw stream bandwidth and a synthetic 1024-wide FP16
projection matvec.
```

Validation:

```text
cargo run --release --bin bench_stream -- 64 5

bytes: 67108864
iterations: 5
median_s: 0.001486292
p95_s: 0.002258834
effective_gb_s_read_plus_write: 90.30

cargo run --release --bin bench_matvec -- 3584 5

shape: [3584 x 1024] @ [1024]
iterations: 5
median_s: 0.000675167
effective_gb_s_weights_plus_io: 10.90

cargo run --release --bin bench_matvec_tiled -- 3584 8 256 5

shape: [3584 x 1024] @ [1024]
tile: rows=8 cols=256
median_s: 0.000623125
effective_gb_s_packed_weights_plus_io: 11.81
```

Research gates:

```text
metal-device-and-bandwidth = passed
fp16-matvec-1024 = passed
```

## 2026-04-30 - Real Qwen3.5-0.8B Artifact Pack and Layered Decode

Purpose:

```text
Move the probe from compact synthetic metalpacks to the real Qwen/Qwen3.5-0.8B
text model artifact, then prove that the full 24-layer GPU-local scheduler can
bind real tensor names, real BF16 source weights packed as FP16, and the real
embedding/LM-head shape.
```

Artifact:

```text
model_dir:
  /Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B

metalpack:
  /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

Inspection:

```text
cargo run --release --bin inspect_artifacts -- \
  /Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B

shape_compatible: yes
safetensor_shards: 1
tensors: 488
tensor_bytes: 1746882752 (1.63 GiB)
pack_plan_blocking_warnings: 0
```

Pack:

```text
cargo run --release --bin pack_weights -- \
  /Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack

entries: 488
packed_bytes: 1746882752 (1.63 GiB)
```

Shape audit:

```text
cargo run --release --bin audit_shapes -- /tmp/ctox_qwen35_08b_real_fp16.metalpack

widths:
  attn_q=2048
  attn_q_plus_head_gate=4096
  attn_kv=512
  deltanet=2048

summary:
  supported=260
  placeholder=0
  missing=18
  unsupported=0

note:
  the 18 missing entries are optional DeltaNet conv1d.bias tensors. The runner
  binds a zero bias vector for those layers.
```

Decode smoke:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 0 1 1

binding_mode: auto-layer-id
embedding: model.language_model.embed_tokens.weight
lm_head: model.language_model.embed_tokens.weight
steps: 1
shape: [248320 x 1024]
layers: delta=18 attention=6 ffn=24
tile: rows=8 cols=256
median_s: 0.016040292
effective_gb_s_layered_pattern_lm_head_pairs: 96.36
next_token: 5387
score: 10.666454
```

Multi-step decode:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 0 4 4

steps: 4
median_s: 0.064190417
effective_gb_s_layered_pattern_sequence_lm_head_pairs: 94.56
tokens: [12105, 3397, 108703, 2379]
last_score: 12.334301
```

Implementation notes:

```text
attention:
  real q_proj output is [4096 x 1024], i.e. 2048 Q channels plus a
  per-dimension 2048-channel gate. The Metal attention kernel now supports that
  layout and still keeps the old scalar-per-head gate fallback for synthetic
  traces.

rope:
  Qwen3.5 config uses rope_theta=10000000. The attention reference and Metal
  shader now use that theta.

conv bias:
  Qwen3.5 DeltaNet conv1d.bias is absent in the real artifact. The scheduler
  treats it as optional zero bias instead of a pack-plan blocker.
```

Research gate:

```text
hf-artifact-inspection-and-pack-plan = passed
```

## 2026-04-30 - Real Parity Corrections: DeltaNet State and Attention Q/Gate Layout

Issue:

```text
The real-metalpack decode was executable but not semantically aligned with MLX.
The first captured MLX raw-token reference for prompt token [107] was:

  MLX greedy tokens: [198, 2, 220, 16]

Earlier Metal runs did not match even the first token, so the next work item was
operator-level parity, not performance tuning.
```

Fixes:

```text
Attention RoPE:
  MLX Qwen3.5 uses RoPE traditional=false, which rotates the first half of the
  rotary slice against the second half. The Metal shader and CPU attention
  reference now use that half-split convention instead of even/odd pairs.

Attention q_proj layout:
  q_proj rows are grouped per head as [query_256, gate_256]. The Metal attention
  Q RMSNorm, RoPE input, and sigmoid gate lookup now use the per-head interleaved
  layout instead of assuming [all_queries, all_gates].

Attention q_norm/k_norm:
  q_norm.weight and k_norm.weight are now classified as layer_norm entries in
  the pack plan and bound into the layered scheduler.

DeltaNet state:
  the recurrent state is [head, Dv, Dk]. The shader now computes the output from
  state[dv, dk] * q[dk] instead of the transposed state[dk, dv]. The decay is
  also applied before kv_mem, matching MLX gated_delta_update.

DeltaNet gated RMSNorm:
  linear_attn.norm.weight is now read with the same 1 + weight sanitization used
  for Qwen3.5 RMSNorm weights.
```

Validation:

```text
cargo run --release --bin bench_attention_reference -- 4 4

max_abs_err: 0.00001526
tolerance: 0.00250000

cargo run --release --bin bench_deltanet_stability -- 20

max_abs_error_out: 0.000000000
max_abs_error_state: 0.000000000
tolerance: 0.000500000

cargo test

result:
  passed
```

Real metalpack:

```text
rm -rf /tmp/ctox_qwen35_08b_real_fp16.metalpack
cargo run --release --bin pack_weights -- \
  /Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack

entries: 488
packed_bytes: 1746882752 (1.63 GiB)

cargo run --release --bin audit_shapes -- /tmp/ctox_qwen35_08b_real_fp16.metalpack

summary:
  supported=260
  placeholder=0
  missing=18
  unsupported=0
```

Current parity:

```text
MLX raw-token greedy reference:
  prompt: [107]
  tokens: [198, 2, 220, 16]
  warm tok/s runs: 38.16, 42.30, 42.16
  reproducible command:
    uv run --with mlx-lm python tools/mlx_reference.py --prompt-token 107 --max-tokens 4 --runs 2

Metal real layered scheduler:
  prompt: [107]
  steps: 1
  next_token: 198

Metal real layered scheduler:
  prompt: [107]
  steps: 4
  tokens: [198, 64, 198, 64]
  median_s: 0.099227042
  effective tok/s: ~40.31
```

Research gates:

```text
real-first-token-greedy-parity = passed
full-decode-greedy-parity = pending
mlx-baseline-beaten = pending
```

Next diagnostic:

```text
The first token now matches MLX, but multi-token greedy parity still fails. Since
fresh MLX single-token calls for token 198 produce token 2 while Metal produces
198, the remaining gap is not only cache carry-over. The next step is to dump
and compare layer outputs for token 198, starting with layer 0 DeltaNet+FFN.
```

## 2026-04-30 01:49 CEST - F32 hidden-state parity pass

Correction after the DeltaNet gated RMSNorm audit:

```text
linear_attn.norm.weight:
  dtype: F32
  interpretation: raw weight, not 1 + weight

q_norm/k_norm/input/post/final RMSNorm BF16 weights:
  interpretation: 1 + raw weight
```

After applying the raw F32 gated-norm semantics, CPU/Metalpack layer traces are
closer to MLX through the early layers, but the full-logit top-2 order for token
107 changes:

```text
CPU/Metalpack top candidates:
  [220, 198, 11, 271, 13]

MLX top candidates:
  [198, 220, 11, 271, 13]
```

Implemented a debug F32 hidden-state path for the full layered scheduler:

```text
F32 hidden buffers:
  embedding gather -> F32 hidden
  RMS+projection reads F32 hidden
  residual add writes F32 hidden
  final RMSNorm writes F32 hidden
  LM-head score reads F32 hidden

FP16 remains:
  packed weights
  DeltaNet q/k/v transient vectors
  attention cache
  SwiGLU activations
```

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000
```

Current real-model parity:

```text
MLX cached decode:
  command:
    uv run --with mlx-lm python tools/mlx_reference.py --prompt-token 107 --max-tokens 4 --runs 2
  tokens:
    [198, 2, 220, 16]
  warm tok/s:
    ~32.9

Metal layered F32-hidden decode:
  command:
    cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4
  tokens:
    [220, 16, 24, 24]
  median_s:
    0.101108083 for 4 tokens
```

Gate update:

```text
real-first-token-greedy-parity = pending
full-decode-greedy-parity = pending
mlx-baseline-beaten = pending
```

Conclusion:

```text
The F32 hidden-state path fixes the previous scheduler type-risk but does not
restore MLX greedy parity. The remaining problem is semantic precision or one
still-mismatched op, not command-buffer orchestration. Next step: add a focused
single-token logit/top-k diagnostic for the final norm + LM head and then bisect
layers with GPU readback against the CPU/Metalpack reference.
```

## 2026-04-30 02:00 CEST - Single-step state reset correction

Found and fixed a benchmark bug in the single-step layered runner:

```text
bug:
  DeltaNet recurrent state and Conv1D state were zeroed once before warmup.
  Timed single-step iterations then inherited warmup state.

fix:
  reset DeltaNet recurrent state and Conv1D state before every warmup and
  measured iteration, matching the sequence runner's reset behavior.
```

This invalidates the earlier apparent first-token parity. With clean state:

```text
Metal single-step:
  next_token: 220
  score: 10.741610
  median_s: 0.026386542

MLX first-token top-k:
  token 198: 10.75
  token 220: 10.6875
  token 11:   9.125
  token 271:  8.875
  token 13:   8.6875
```

Updated `tools/mlx_reference.py` with `--top-k` so the first-token logit order
can be captured reproducibly:

```text
uv run --with mlx-lm python tools/mlx_reference.py \
  --prompt-token 107 --max-tokens 1 --runs 1 --top-k 5
```

Current conclusion:

```text
real-first-token-greedy-parity remains pending.
The Metal path is close enough that tokens 198 and 220 are the only competing
first-token candidates, but the logit order is still reversed.
```

## 2026-04-30 01:57 CEST - Greedy parity restored and MLX baseline beaten

Root cause found in the DeltaNet Q/K normalization:

```text
MLX reference:
  q = (head_dim ** -1)   * rms_norm(q)
  k = (head_dim ** -0.5) * rms_norm(k)

Previous Metal:
  used rsqrt(sum(x^2) + eps)

Corrected Metal:
  uses rsqrt(sum(x^2) + head_dim * eps)
```

This matters because Qwen3.5-0.8B's first-token top-2 logits for raw token 107
are very close:

```text
MLX:
  token 198: 10.75
  token 220: 10.6875
```

Validation after the fix:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 0 4 1 5

Metal clean-state single-step:
  next_token: 198
  score: 10.721777
  cpu_lm_head_top_logits:
    [(198, 10.721774), (220, 10.712102), (11, 9.26336), (271, 8.875678), (13, 8.772468)]

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal 4-token decode:
  tokens: [198, 2, 220, 16]
  median_s: 0.101230166
  throughput: ~39.51 tok/s

uv run --with mlx-lm python tools/mlx_reference.py --prompt-token 107 --max-tokens 4 --runs 3

MLX 4-token decode:
  tokens: [198, 2, 220, 16]
  throughput: 35.44-36.98 tok/s
```

Gate update:

```text
real-first-token-greedy-parity = passed
full-decode-greedy-parity = passed
mlx-baseline-beaten = passed
ane-coreml-baseline = pending
```

Current state:

```text
The real Qwen3.5-0.8B Metal layered mega-pipeline is now correct for the
captured greedy raw-token decode and beats the local MLX baseline on the same
prompt. Remaining work is no longer basic parity; it is fusion, stable benchmark
coverage, and optional ANE/Core ML baseline measurement.
```

## 2026-04-30 01:58 CEST - ANE/Core ML baseline ruled out for this crate

Probe command:

```text
python3 tools/coreml_ane_probe.py
```

Result:

```text
status: ruled_out
coremltools_available: false
repo_coreml_artifacts: []
model_coreml_artifacts: []
```

Rule-out reason:

```text
No .mlmodel/.mlpackage/.mlmodelc artifact exists in the repo or local model
directory. The current crate has no Core ML converter path, and Qwen3.5 decode
depends on stateful DeltaNet/gated_delta_update plus attention KV cache. ANE
therefore remains outside the Metal decode hot path for this prototype.
```

Gate update:

```text
ane-coreml-baseline = passed as ruled_out
all current research gates = passed
```

## 2026-04-30 02:01 CEST - FFN gate/up/SwiGLU dispatch fusion

Implemented the first real dispatch-reduction pass in the full layered Metal
decode path:

```text
Before, per FFN:
  RMSNorm + gate_proj
  RMSNorm + up_proj
  SwiGLU activation
  down_proj
  residual_add

After, per FFN:
  RMSNorm + gate_proj + up_proj + SwiGLU activation
  down_proj
  residual_add
```

New kernel:

```text
qwen35_08b_ffn_gate_up_swiglu_rowtiles_f32_tiled_k1024_i3584
```

It computes the RMS scale once per row tile, streams both gate/up weights in
the same dispatch, reduces both accumulators, and writes the FP16 SwiGLU
activation directly for the existing down-projection kernel.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.061835125 for 4 tokens

Metal throughput:
  ~64.69 tok/s

MLX reference:
  [198, 2, 220, 16]
  57.42-61.69 tok/s
```

Impact:

```text
The fused FFN path preserves greedy parity and improves the measured 4-token
real decode from ~39.5 tok/s to ~64.7 tok/s on the same prompt.
```

## 2026-04-30 02:21 CEST - DeltaNet qkv/z/b/a dispatch fusion

Implemented the next real dispatch-reduction pass in the full layered Metal
decode path:

```text
Before, per DeltaNet layer:
  RMSNorm + qkv projection
  RMSNorm + z projection
  RMSNorm + beta/b projection
  RMSNorm + decay/a projection

After, per DeltaNet layer:
  RMSNorm + qkv/z/b/a projections in one dispatch
```

New kernel:

```text
qwen35_08b_deltanet_qkv_z_b_a_rms_project_f32_tiled_k1024
```

The kernel maps the threadgroup grid over the four projection row-tile ranges,
uses the projection-local tile index for packed-weight addressing, and writes
the existing `qkv_f32`, `z_f32`, `beta_raw`, and `gate_raw` buffers consumed by
the downstream DeltaNet conv/state kernels.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.061436250 for 4 tokens

Metal throughput:
  ~65.11 tok/s

MLX reference:
  [198, 2, 220, 16]
  56.46-59.50 tok/s
```

Impact:

```text
The fused DeltaNet projection path preserves greedy parity and removes three
Metal dispatches from each of the 18 DeltaNet layers per generated token.
```

## 2026-04-30 02:33 CEST - Attention q/k/v dispatch fusion

Implemented the matching dispatch-reduction pass for the 6 full-attention
layers in the real layered Metal decode path:

```text
Before, per attention layer:
  RMSNorm + q projection
  RMSNorm + k projection
  RMSNorm + v projection

After, per attention layer:
  RMSNorm + q/k/v projections in one dispatch
```

New kernel:

```text
qwen35_08b_attention_q_k_v_rms_project_f32_tiled_k1024
```

The kernel uses one projection-local packed-weight tile index for each of the
`q`, `k`, and `v` row-tile ranges, then writes the existing `attn_q_f32`,
`attn_k_f32`, and `attn_v_f32` buffers used by the Q/K RMSNorm and RoPE/KV
cache kernel.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.061283625 for 4 tokens

Metal throughput:
  ~65.27 tok/s

MLX reference:
  [198, 2, 220, 16]
  54.28-59.68 tok/s
```

Impact:

```text
The fused attention projection path preserves greedy parity and removes two
Metal dispatches from each of the 6 attention layers per generated token.
```

## 2026-04-30 02:51 CEST - Projection residual writeback fusion

Implemented residual-add writeback fusion for the real layered Metal decode
path:

```text
Before:
  token-mixer out_proj -> hidden_f32
  residual_add(input, hidden_f32) -> next hidden

  FFN down_proj -> hidden_f32
  residual_add(input, hidden_f32) -> next hidden

After:
  token-mixer out_proj + residual_add -> next hidden
  FFN down_proj + residual_add -> next hidden
```

New kernels:

```text
qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k2048_f32
qwen35_08b_matvec_residual_rowtiles_fp16_tiled_k3584_f32
```

The fused kernels keep the same row-tiled matvec layout, but add the residual
input inside the `tid == 0` row writeback. The older standalone projection and
residual kernels remain in place for slice/superblock diagnostics.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.061294375 for 4 tokens

Metal throughput:
  ~65.26 tok/s

MLX reference:
  [198, 2, 220, 16]
  53.86-57.77 tok/s
```

Impact:

```text
The fused writeback path preserves greedy parity and removes the standalone
residual-add dispatch after each of the 24 token-mixer projections and 24 FFN
down projections.
```

## 2026-04-30 03:06 CEST - LM-head rowtile argmax fusion

Reduced the LM-head argmax reduction workload in the real layered Metal decode
path:

```text
Before:
  LM head writes one score/id pair per vocab row
  global argmax reduces 248,320 candidates

After:
  LM head writes one best score/id pair per vocab row tile
  global argmax reduces 31,040 candidates
```

New kernel:

```text
qwen35_08b_lm_head_argmax_rowtiles_f32_tiled_k1024
```

The kernel still streams the full tied embedding/LM-head weights, but performs
the local 8-row argmax inside the row-tile dispatch. This removes most of the
intermediate score/id writes and one global reduction pass for the 248,320-row
vocabulary.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.058752875 for 4 tokens

Metal throughput:
  ~68.08 tok/s

MLX reference:
  [198, 2, 220, 16]
  54.73-57.25 tok/s
```

Impact:

```text
The rowtile argmax path preserves greedy parity and reduces the first global
LM-head argmax stage from full-vocab candidates to row-tile candidates.
```

## 2026-04-30 03:24 CEST - DeltaNet fused state-step attempt rejected

Tested a deeper DeltaNet fusion that combined:

```text
split qkv f32 -> fp16
q/k L2 normalization
beta/decay activation
recurrent DeltaNet state update
```

Candidate kernel:

```text
qwen35_08b_deltanet_step_fused_qkv_norm_decay_f32_state
```

Result:

```text
Metal tokens with fused step:
  [198, 12, 220, 16]

Expected MLX / accepted Metal tokens:
  [198, 2, 220, 16]
```

I first suspected the parallel Q/K norm reduction order, then changed the fused
kernel to compute Q/K sums serially in the same order as the existing
`qwen35_08b_deltanet_qk_l2norm_scale_h16d128` kernel. The token mismatch
persisted:

```text
Metal tokens with serial-norm fused step:
  [198, 12, 220, 16]
```

Decision:

```text
Do not bind the fused DeltaNet state-step kernel into the real decode path.
The scheduler is rolled back to the known-correct split -> qk_norm ->
activate -> step sequence. The candidate kernel remains in the shader tree for
future tensor-diff debugging.
```

Post-rollback validation:

```text
cargo test
  passed

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.058752875 for 4 tokens
```

## 2026-04-30 03:43 CEST - Attention q/k norm + RoPE/KV fusion

Fused attention Q/K RMSNorm into the existing RoPE/KV-cache/online-attention
kernel for the real layered Metal decode path:

```text
Before, per attention layer:
  q/k RMSNorm dispatch
  RoPE + KV-cache update + online attention dispatch

After, per attention layer:
  q/k RMSNorm + RoPE + KV-cache update + online attention in one dispatch
```

New kernel:

```text
qwen35_08b_attention_norm_rope_cache_gqa8_kv2_d256_to_fp16
```

The fused kernel recomputes Q/K RMSNorm inside each attention head threadgroup
using the same reduction structure as the prior standalone
`qwen35_08b_attention_qk_rmsnorm_f32_h8_kv2_d256` kernel, applies RoPE to the
normalized Q/K values, updates the KV cache, and runs the existing online
softmax path.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.059317750 for 4 tokens

Metal throughput:
  ~67.43 tok/s

MLX reference:
  [198, 2, 220, 16]
  56.85-61.96 tok/s
```

Impact:

```text
The fused attention norm path preserves greedy parity and removes the
standalone q/k RMSNorm dispatch from each of the 6 full-attention layers.
```

## 2026-04-30 04:02 CEST - DeltaNet split + q/k norm fusion

Fused the safe part of the DeltaNet state preparation path in the real layered
Metal decode scheduler:

```text
Before, per DeltaNet layer:
  split qkv f32 -> q/k/v fp16
  q/k L2 normalization
  beta/decay activation
  recurrent DeltaNet step

After, per DeltaNet layer:
  split qkv f32 -> normalized q/k fp16 + v fp16
  beta/decay activation
  recurrent DeltaNet step
```

New kernel:

```text
qwen35_08b_deltanet_split_qkv_norm_f32_to_fp16_h16d128
```

Unlike the rejected fused state-step attempt, this kernel deliberately keeps the
Q/K norm sums serial per head, matching the old
`qwen35_08b_deltanet_qk_l2norm_scale_h16d128` accumulation order. It only
removes the separate split dispatch and leaves beta/decay plus recurrent state
update on the known-correct path.

Validation:

```text
cargo test
  passed

cargo run --release --bin bench_attention_reference -- 4 4
  max_abs_err: 0.00001526

cargo run --release --bin bench_deltanet_stability -- 20
  max_abs_error_out: 0.000000000
  max_abs_error_state: 0.000000000

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.059360125 for 4 tokens

Metal throughput:
  ~67.39 tok/s

MLX reference:
  [198, 2, 220, 16]
  56.71-60.75 tok/s
```

Impact:

```text
The fused DeltaNet split/norm path preserves greedy parity and removes one
dispatch from each of the 18 DeltaNet layers.
```

## 2026-04-30 04:16 CEST - DeltaNet fused decay-step attempt rejected

Tested a narrower DeltaNet fusion than the earlier full state-step attempt:

```text
beta/decay activation
recurrent DeltaNet state update
```

Candidate kernel:

```text
qwen35_08b_deltanet_step_fused_decay_f32_state
```

This variant kept the accepted `q/k/v` split+norm path and preserved the
original state-update barrier inside the step kernel. It still changed greedy
decode:

```text
Metal tokens with fused decay-step:
  [198, 12, 220, 16]

Expected MLX / accepted Metal tokens:
  [198, 2, 220, 16]
```

Decision:

```text
Do not bind qwen35_08b_deltanet_step_fused_decay_f32_state into the real
decode path. Keep the candidate kernel for tensor-diff debugging, but retain
the known-correct activate -> step sequence in the scheduler.
```

Post-rollback validation:

```text
cargo test
  passed

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 4 0 4 4

Metal tokens:
  [198, 2, 220, 16]

Metal median_s:
  0.058994083 for 4 tokens
```

## 2026-04-30 04:10 CEST - llama.cpp BF16 reference comparison

Built and ran the local `llama.cpp` reference with Metal enabled:

```text
llama.cpp commit:
  15fa3c4

binary:
  /Users/michaelwelsch/Downloads/llama.cpp/build/bin/llama-bench

device:
  Apple M5, Metal backend

reference model:
  unsloth/Qwen3.5-0.8B-GGUF
  Qwen3.5-0.8B-BF16.gguf
```

Conversion note:

```text
The local HF -> GGUF converter recognizes Qwen3.5 weights, but currently fails
at tokenizer pre-tokenizer detection. The benchmark therefore uses the published
BF16 GGUF reference instead of a locally converted file.
```

Reference prefill + decode benchmark:

```text
build/bin/llama-bench \
  --hf-repo unsloth/Qwen3.5-0.8B-GGUF \
  --hf-file Qwen3.5-0.8B-BF16.gguf \
  -ngl 99 -p 512 -n 128 -r 3 -o json

llama.cpp pp512:
  avg_ts = 3243.24 tok/s

llama.cpp tg128:
  avg_ts = 52.98 tok/s
```

Short decode smoke benchmark, not a performance target:

```text
build/bin/llama-bench \
  --hf-repo unsloth/Qwen3.5-0.8B-GGUF \
  --hf-file Qwen3.5-0.8B-BF16.gguf \
  -ngl 99 -p 1 -n 4 -r 5 -o json

llama.cpp tg4:
  avg_ts = 51.02 tok/s
```

Current CTOX Metal decode comparison:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 3 0 128 128

CTOX Metal tg128:
  median_s = 2.602157750
  throughput = 49.19 tok/s
  effective GB/s = 74.27
```

Interpretation:

```text
Prefill:
  not implemented in the CTOX Metal probe yet, so there is no CTOX prefill
  number to compare against llama.cpp pp512.

Decode:
  short 4-token CTOX probe is only a correctness/smoke signal. Do not use it
  as a meaningful performance comparison.

  longer 128-token decode is currently slightly slower than llama.cpp tg128
    CTOX:      49.19 tok/s
    llama.cpp: 52.98 tok/s

Decision:
  Treat llama.cpp as the current decode bar for the 128-token benchmark. The
  CTOX path has shown lower per-token overhead in the short decode case, but it
  has not yet beaten llama.cpp on the steadier tg128 reference run.
```

## 2026-04-30 04:31 CEST - long-context benchmark correction

The earlier `tg4` comparison is not a useful performance result. Realistic
evaluation needs long prompt lengths and nontrivial output decode length.

`llama.cpp` BF16/Metal reference, prompt-only matrix:

```text
build/bin/llama-bench \
  --hf-repo unsloth/Qwen3.5-0.8B-GGUF \
  --hf-file Qwen3.5-0.8B-BF16.gguf \
  -ngl 99 -p 4096,16384,32768,65536,131072 -n 512 \
  -r 1 -b 2048 -ub 512 -o json

pp4096:
  2852.70 tok/s

pp16384:
  2065.71 tok/s

pp32768:
  1325.20 tok/s

pp65536:
  701.26 tok/s

pp131072:
  349.74 tok/s

tg512 from the same run:
  44.77 tok/s
```

`llama.cpp` BF16/Metal combined long-context run:

```text
build/bin/llama-bench \
  --hf-repo unsloth/Qwen3.5-0.8B-GGUF \
  --hf-file Qwen3.5-0.8B-BF16.gguf \
  -ngl 99 -pg 131072,512 \
  -r 1 -b 2048 -ub 512 -o json

p131072 + n512:
  avg_ns = 416742939417
  avg_ts = 315.74 tok/s
```

Current CTOX long-output decode measurement:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 0 512 512

CTOX tg512 from empty state:
  median_s = 63.697588792
  throughput = 8.04 tok/s
  effective GB/s = 12.17

This number is invalid as a standalone comparison because it was captured while
the 128k llama.cpp reference benchmark was still running.
```

CTOX limitation:

```text
The current CTOX Metal probe does not implement true prefill. Its sequence
runner autoregressively generates tokens and updates KV/Delta state one token at
a time. It cannot currently run a semantically valid p131072+n512 benchmark
because there is no prompt-token input path that fills the 128k KV-cache and
DeltaNet recurrent states without sampling each intermediate token.
```

Decision:

```text
The current CTOX path is not competitive against llama.cpp for realistic
long-output or long-context workloads. The next required milestone is a real
prefill/state-build runner, followed by a decode runner that starts at an
existing prompt length and generates 512+ tokens against populated KV/Delta
state.
```

## 2026-04-30 05:22 CEST - first real CTOX prefill-state-build path

Implemented the first semantically valid prefill/decode sequence path in the
layered-pattern benchmark:

```text
prefill:
  run N-1 prompt tokens through all 24 layers
  update DeltaNet recurrent state and attention KV-cache
  skip final RMSNorm, LM-head, argmax, and CPU token readback

first decode token:
  run LM-head/argmax on the last prompt token position

remaining decode:
  feed generated token IDs through the normal decode path
```

The benchmark CLI now treats argument 7 (`decode_position`) as `prefill_steps`
for sequence runs and labels it that way in output.

Validation:

```text
cargo check
  passed

cargo test
  passed

cargo run --release --bin qwen35-08b-metal-research
  research gates: 50 passed, 0 pending

cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 0 4 4

tokens:
  [198, 2, 220, 16]
```

Corrected standalone decode measurement without concurrent llama.cpp load:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 3 0 512 512

CTOX tg512:
  median_s = 10.721247500
  throughput = 47.76 tok/s
```

First semantically valid prefill/decode measurement:

```text
cargo run --release --bin bench_metalpack_decode_layered_pattern -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored 107 1 512 513 1

CTOX p512+n1:
  median_s = 8.851784292
  effective GB/s = 58.11
```

Interpretation:

```text
Standalone decode is roughly at llama.cpp tg512 level.

Prefill is not competitive. It is now semantically measurable, but still runs
as serial batch-1 weight streaming. Matching llama.cpp long-context performance
requires a batch prefill path that reuses weights across prompt tokens instead
of executing one matvec-style token step at a time.
```

## 2026-04-30 06:18 CEST - sync/storage experiments and first batched prefill kernel

Negative results first:

```text
Private read-only weight buffers:
  implemented FFI support and made it env-gated via
  CTOX_QWEN35_PRIVATE_WEIGHTS=1

  result:
    tg512 regressed to roughly 44 tok/s in the tested sequence path

  decision:
    keep private-buffer upload capability for experiments
    keep shared buffers as the default for this probe

Decode command buffers without per-token CPU wait:
  result:
    tg512 regressed to median_s = 12.645565083
    throughput ~= 40.5 tok/s

  decision:
    restore per-token wait for decode
    CPU sync is not the current decode bottleneck
```

Small valid improvement:

```text
include_lm_head=false prefill dispatches now honor wait_until_completed=false.

CTOX p512+n1:
  before: median_s = 8.851784292
  after:  median_s = 8.456265500

Interpretation:
  useful but not decisive; serial batch-1 prefill remains the core problem.
```

Built the first true batched prefill building block:

```text
kernel:
  qwen35_08b_prefill_rms_matmul_rowtiles_tok2_fp16_tiled_k1024_f32

operation:
  token-block RMSNorm + tiled projection
  x: [tokens x 1024]
  W: [3584 x 1024]
  y: [tokens x 3584]
  token_tile = 2

benchmark:
  cargo run --release --bin bench_prefill_rms_matmul -- 512 3584 5

result:
  median_s = 0.023281333
  p95_s = 0.023702959
  effective GB/s estimate = 81.12

single-token reference:
  cargo run --release --bin bench_rms_matvec_tiled -- 3584 10
  median_s = 0.000533125

serial estimate:
  512 * 0.000533125s = 0.272960000s

speedup of the batched prefill projection building block:
  0.272960000 / 0.023281333 ~= 11.7x
```

Token tile autotune:

```text
token_tile = 4:
  Metal pipeline creation failed, likely due to threadgroup memory/register use.

token_tile = 3:
  compiled and ran, but regressed:
    p512 projection median_s = 0.029445208

token_tile = 2:
  best current candidate:
    p512 projection median_s = 0.023281333
```

Conclusion:

```text
The project now has a measurable path out of serial prefill. The next milestone
is integrating this token-block projection pattern into real model prefill:
  1. batched DeltaNet input projections
  2. token-block FFN gate/up projections
  3. scan/state update across the token block
  4. attention prefill over token blocks
```

## 2026-04-30 06:37 CEST - batched prefill projection on real Qwen weights

Added:

```text
bench_metalpack_prefill_projection
```

This runs the same token-block RMSNorm + projection kernel against real
metalpack tensors. The local Qwen pack records source dtype BF16, but the
layout is `fp16_row_tiled`; the benchmark therefore validates layout/shape and
reads the packed 16-bit payload.

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_projection -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack gate_proj 512 5

tensor:
  model.language_model.layers.0.mlp.gate_proj.weight

shape:
  tokens=512 [3584 x 1024]

median_s:
  0.023093750

effective GB/s estimate:
  81.77
```

```text
cargo run --release --bin bench_metalpack_prefill_projection -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack in_proj_qkv 512 5

tensor:
  model.language_model.layers.0.linear_attn.in_proj_qkv.weight

shape:
  tokens=512 [6144 x 1024]

median_s:
  0.039499875

effective GB/s estimate:
  81.92
```

Interpretation:

```text
The batched prefill projection kernel is no longer synthetic-only. It runs on
real Qwen3.5-0.8B layer weights and holds the same performance envelope. This
is the first concrete replacement for serial batch-1 prefill projections.
```

## 2026-04-30 06:52 CEST - batched FFN gate/up prefill block on real Qwen weights

Added:

```text
kernel:
  qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok2_fp16_tiled_k1024_i3584

benchmark:
  bench_metalpack_prefill_ffn_gate_up
```

The first direct token-tile-2 FFN gate/up kernel used 8 output rows per
threadgroup and failed during Metal pipeline creation. The fix was to keep the
packed weight layout at `row_tile=8`, but compute only 4 output rows per
threadgroup. This reduces threadgroup memory while preserving compatibility
with the existing packed weights.

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_ffn_gate_up -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

layer:
  0

operation:
  post_attention_layernorm + gate_proj + up_proj + SwiGLU

shape:
  tokens=512 hidden=1024 intermediate=3584

median_s:
  0.043209625

effective GB/s estimate:
  87.11
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_gate_up -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.076416625

effective GB/s estimate:
  98.51
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_gate_up -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 512 5

median_s:
  0.039007375

effective GB/s estimate:
  96.49
```

Interpretation:

```text
This is the second real batched prefill replacement:
  previous: one projection over a token block
  now: fused FFN gate/up/SwiGLU over a token block

Against the old single-token projection building block, FFN gate/up prefill is
now roughly an order of magnitude faster for p512. The next missing FFN piece
is the batched down projection from [tokens x 3584] back to [tokens x 1024].
```

## 2026-04-30 07:08 CEST - batched FFN down projection on real Qwen weights

Added:

```text
kernel:
  qwen35_08b_prefill_down_matmul_rowtiles_tok2_fp16_tiled_k3584_f32

benchmark:
  bench_metalpack_prefill_down
```

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_down -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

tensor:
  model.language_model.layers.0.mlp.down_proj.weight

shape:
  tokens=512 rows=1024 cols=3584

median_s:
  0.010133292

effective GB/s estimate:
  186.00
```

```text
cargo run --release --bin bench_metalpack_prefill_down -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.021605708

effective GB/s estimate:
  174.47
```

Current measurable FFN prefill block estimate for one layer:

```text
p512 gate/up/SwiGLU:
  0.043209625s

p512 down projection:
  0.010133292s

p512 FFN block subtotal:
  0.053342917s
```

Interpretation:

```text
The FFN prefill path now has both main batched pieces on real model weights:
  1. post_attention_layernorm + gate/up + SwiGLU
  2. down projection

This still needs integration into a state-carrying model prefill runner, but the
major serial FFN matvec pattern is now replaced by token-block kernels.
```

## 2026-04-30 07:22 CEST - GPU-local batched FFN prefill block

Added:

```text
benchmark:
  bench_metalpack_prefill_ffn_block

command buffer:
  1. post_attention_layernorm + gate/up + SwiGLU
  2. down projection

CPU readback:
  only final checksum after completion
```

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

shape:
  tokens=512 hidden=1024 intermediate=3584

median_s:
  0.057727959

effective GB/s estimate:
  97.79
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.116487708

effective GB/s estimate:
  96.92
```

Interpretation:

```text
This is the first realistic GPU-local FFN prefill block benchmark. The
activation stays on the GPU between gate/up and down projection. It is slightly
slower than the sum of separately timed pieces, but it measures the execution
shape needed by a full prefill runner.

The next hard part is no longer FFN projection throughput. The remaining
prefill blockers are DeltaNet block-state scan and attention prefill/KV writes.
```

## 2026-04-30 07:39 CEST - batched DeltaNet projection block on real Qwen weights

Added:

```text
benchmark:
  bench_metalpack_prefill_delta_project

command buffer:
  1. input_layernorm + in_proj_qkv
  2. input_layernorm + in_proj_z
  3. input_layernorm + in_proj_b
  4. input_layernorm + in_proj_a
```

This uses the existing token-block RMSNorm+projection kernel and keeps all four
projection outputs on the GPU. It does not yet run causal conv, q/k norm, decay
activation, recurrent state update, z-gated norm, or out projection.

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_delta_project -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

shape:
  tokens=512 hidden=1024 qkv_rows=6144 z_rows=2048 gate_rows=16

median_s:
  0.054445417

effective GB/s estimate:
  79.66
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_project -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.110959208

effective GB/s estimate:
  78.17
```

Interpretation:

```text
The largest pure weight-streaming part of a DeltaNet layer now has a real
batched prefill path. The next milestone is not another projection kernel; it
is the token-block DeltaNet state path:
  causal conv over [tokens x 6144]
  q/k normalization
  beta/decay activation
  recurrent state scan over the token block
  z-gated norm
  out projection
```

## 2026-04-30 07:55 CEST - batched DeltaNet causal conv on real Qwen weights

Added:

```text
kernel:
  qwen35_08b_prefill_deltanet_causal_conv1d_silu_c6144_k4

benchmark:
  bench_metalpack_prefill_delta_conv
```

The kernel processes one channel per GPU thread and loops over the token block
inside that thread. It reads/writes the 3-token convolution state and applies
the 4-wide causal convolution plus SiLU. The local Qwen3.5 pack has no
`conv1d.bias`, so the benchmark uses a zero bias buffer.

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_delta_conv -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

shape:
  tokens=512 channels=6144 kernel_width=4

median_s:
  0.000664000

effective GB/s estimate:
  38.10
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_conv -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.001008208

effective GB/s estimate:
  50.06
```

Interpretation:

```text
The causal conv is not the DeltaNet prefill bottleneck. At p512 it is below
1 ms, while the DeltaNet projection block is roughly 54 ms. The next expensive
piece is the recurrent DeltaNet state update over the token block.
```

## 2026-04-30 08:12 CEST - batched DeltaNet q/k/v prepare on real state params

Added:

```text
kernels:
  qwen35_08b_prefill_deltanet_split_qkv_norm_tok_f32_to_fp16_h16d128
  qwen35_08b_prefill_deltanet_activate_beta_decay_tok_h16

benchmark:
  bench_metalpack_prefill_delta_prepare
```

This prepares the recurrent state update inputs over a token block:

```text
qkv float [tokens x 6144]
  -> q half [tokens x 2048], L2 normalized per head and scaled
  -> k half [tokens x 2048], L2 normalized per head
  -> v half [tokens x 2048]

b/a float [tokens x 16] + real A_log/dt_bias
  -> beta float [tokens x 16]
  -> decay float [tokens x 16]
```

Real Qwen measurements:

```text
cargo run --release --bin bench_metalpack_prefill_delta_prepare -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.000637542

effective GB/s estimate:
  29.81
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_prepare -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.000688584

effective GB/s estimate:
  55.20
```

Interpretation:

```text
DeltaNet q/k/v prepare is not the bottleneck either. The remaining expensive
DeltaNet prefill piece is now isolated: the recurrent state scan over
[tokens x heads x 128 x 128].
```

## 2026-04-30 08:38 CEST - batched DeltaNet recurrent state scan

Added:

```text
kernel:
  qwen35_08b_prefill_deltanet_scan_f32_state_tok_h16d128

benchmark:
  bench_metalpack_prefill_delta_scan
```

Kernel shape:

```text
one threadgroup per DeltaNet head
128 threads per head
sequential token loop inside the kernel
state remains GPU-local as float [16 x 128 x 128]
inputs are q/k/v half [tokens x 2048] and beta/decay float [tokens x 16]
output is float [tokens x 2048]
```

Real-model invocation path with synthetic prepared inputs:

```text
cargo run --release --bin bench_metalpack_prefill_delta_scan -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 128 5

median_s:
  0.001910166

effective GB/s estimate:
  212.18
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_scan -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.003535083

effective GB/s estimate:
  458.59
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_scan -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.007381125

effective GB/s estimate:
  439.27
```

Validation:

```text
validate8 max_abs_error_out:
  1e-8

validate8 max_abs_error_state:
  1.5e-8
```

Interpretation:

```text
The recurrent scan is now blockwise GPU-local and no longer the dominant
DeltaNet prefill bottleneck. For p512, projection is still roughly 54 ms while
scan is roughly 3.5 ms. Next target: gated norm + out projection, then chain
project -> conv -> prepare -> scan -> gated norm -> out projection without CPU
readback.
```

## 2026-04-30 09:08 CEST - batched DeltaNet gated norm, out projection, and full block chain

Added:

```text
kernels:
  qwen35_08b_prefill_deltanet_gated_rmsnorm_tok_h16d128_f32_to_fp16
  qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok2_fp16_tiled_k2048_f32

benchmarks:
  bench_metalpack_prefill_delta_out
  bench_metalpack_prefill_delta_block
```

The full DeltaNet block benchmark chains the current blockwise GPU path in one
command buffer:

```text
RMSNorm + qkv/z/b/a projections
  -> causal conv
  -> q/k/v split + q/k norm + beta/decay activation
  -> recurrent scan
  -> gated RMSNorm
  -> out projection
```

No CPU readback occurs between these stages.

Gated norm + out projection:

```text
cargo run --release --bin bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.016644250

effective GB/s estimate:
  65.41
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.016552792

effective GB/s estimate:
  131.54
```

Full DeltaNet block:

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 128 5

median_s:
  0.017262625
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.071055917

effective GB/s estimate:
  99.17
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.142178417

effective GB/s estimate:
  99.12
```

Interpretation:

```text
The block is now GPU-local across the DeltaNet stages, but the measured p512
time is still dominated by packed FP16 projection streaming. The recurrent scan
is only a few milliseconds; the practical next optimization target is the
projection/out-projection matmul kernel family, not the recurrent update.
```

## 2026-04-30 09:31 CEST - SIMDgroup prefill RMS projection path

Added:

```text
kernel:
  qwen35_08b_prefill_rms_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32
```

The new path uses SIMDgroup reductions and `token_tile=4` for the RMSNorm +
projection kernels. It is now the default for prefill RMS projections; the old
token-tile-2 path remains available with:

```text
CTOX_QWEN35_PREFILL_RMS_TOK2=1
```

Correctness note:

```text
The first SIMD4 version had a bad RMS broadcast across SIMDgroups. It produced
a wrong full-block checksum and was fixed before being made the default.
The corrected SIMD4 projection checksum matches the token-tile-2 projection
path for the same benchmark input.
```

Real Qwen measurements:

```text
CTOX_QWEN35_PREFILL_RMS_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_projection -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack in_proj_qkv 512 3

token_tile:
  2

median_s:
  0.043352833
```

```text
CTOX_QWEN35_PREFILL_RMS_SIMD4=1 cargo run --release --bin \
  bench_metalpack_prefill_projection -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack in_proj_qkv 512 5

token_tile:
  4

median_s:
  0.022790166
```

Full DeltaNet block after enabling SIMD4 by default:

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.040453875

checksum:
  -3.185984
```

Best observed full DeltaNet block run with the SIMD4 path:

```text
CTOX_QWEN35_PREFILL_RMS_SIMD4=1 cargo run --release --bin \
  bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.037640167
```

Interpretation:

```text
The SIMDgroup projection kernel roughly halves the qkv projection time and cuts
the full DeltaNet block p512 time from about 71 ms to about 38-40 ms. This is
real progress, but the block is still far from the memory roofline needed to
beat llama.cpp prefill end-to-end.
```

## 2026-04-30 10:02 CEST - cache/miss analysis layer

Added:

```text
module:
  src/cache_model.rs

benchmark/tool:
  cargo run --release --bin cache_analysis

trace helper:
  tools/capture_metal_trace.sh
```

Policy change:

```text
The optimization target is zero avoidable cache misses, not literal zero cache
misses. Literal zero is impossible for compulsory reads of weights or KV data
that are not already resident. The required standard is:

  actual trace misses <= modeled unavoidable compulsory/streaming miss floor
```

The cache model now reports, per operation:

```text
working_set_bytes
logical_bytes
modeled_unavoidable_dram_miss_bytes
modeled_cache_hit_bytes
modeled_hit_rate
residency against a configurable modeled L2 size
required counter checks
```

Example p512 command:

```text
cargo run --release --bin cache_analysis -- \
  --tokens 512 \
  --decode-position 4096 \
  --modeled-l2-mib 32 \
  --sustained-gb-s 90
```

Top modeled p512 miss floors:

```text
ffn.gate_up_swiglu:
  unavoidable miss floor: 3.51 GiB
  modeled hit rate:       50.0%
  next action:            tok4/SIMD gate/up kernel

ffn.down:
  unavoidable miss floor: 1.76 GiB
  modeled hit rate:       49.9%
  next action:            tok4/SIMD k3584 down kernel

delta.project.qkv:
  unavoidable miss floor: 1.51 GiB
  modeled hit rate:       74.8%
  next action:            row_tile/private-buffer tuning

delta.out_proj:
  unavoidable miss floor: 1.00 GiB
  modeled hit rate:       49.9%
  next action:            tok4/SIMD k2048 out projection

lm_head:
  unavoidable miss floor: 486.90 MiB
  modeled hit rate:       0.0%
  next action:            GPU-local top-k already required; later quantize/shortlist
```

Metal System Trace automation:

```text
tools/capture_metal_trace.sh /tmp/qwen35_delta_block_p128_script.trace \
  target/release/bench_metalpack_prefill_delta_block \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 128 1
```

The trace helper now records and exports a TOC successfully:

```text
return-exit-status:
  0

template:
  Metal System Trace

GPU counter tables present:
  gpu-counter-info
  gpu-counter-value
  metal-gpu-counter-profile
  metal-gpu-counter-intervals
```

Limitation found:

```text
The default xctrace Metal System Trace records `Counter Set: (null)` on this
machine, so it exposes GPU timeline/counter table plumbing but not yet useful
cache-hit/cache-miss counters. The next step is to identify or create a Metal
counter profile that includes memory/cache counters, then compare actual misses
against the cache model's unavoidable miss floor.
```

## 2026-04-30 10:23 CEST - FFN gate/up tok4 SIMD path

Added:

```text
kernel:
  qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok4_simd_fp16_tiled_k1024_i3584
```

The old FFN gate/up path remains available with:

```text
CTOX_QWEN35_FFN_GATE_UP_TOK2=1
```

Cache-analysis reason:

```text
For p512, ffn.gate_up_swiglu had the largest modeled unavoidable miss floor:
  3.51 GiB with token_tile=2

Increasing token_tile to 4 cuts the required weight-stream miss floor roughly
in half for gate/up, while preserving the checksum.
```

Measurements:

```text
CTOX_QWEN35_FFN_GATE_UP_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_ffn_gate_up -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 3

token_tile:
  2

median_s:
  0.041830375

checksum:
  0.339415
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_gate_up -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

token_tile:
  4

median_s:
  0.023473959

checksum:
  0.339415
```

Full FFN block:

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 10

median_s:
  0.038862250

checksum:
  0.432987
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 10

median_s:
  0.057477500

checksum:
  0.432987
```

Interpretation:

```text
The FFN gate/up miss floor and runtime are materially improved. The next cache
target is the remaining token_tile=2 down projection, followed by DeltaNet and
attention k2048 out projections.
```

## 2026-04-30 10:40 CEST - FFN down tok4 SIMD path

Added:

```text
kernel:
  qwen35_08b_prefill_down_matmul_rowtiles_tok4_simd_fp16_tiled_k3584_f32
```

The old down path remains available with:

```text
CTOX_QWEN35_DOWN_TOK2=1
```

Measurements:

```text
CTOX_QWEN35_DOWN_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_down -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 3

token_tile:
  2

median_s:
  0.008911584

checksum:
  0.867992
```

```text
cargo run --release --bin bench_metalpack_prefill_down -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

token_tile:
  4

median_s:
  0.007793167

checksum:
  0.867992
```

Full FFN block after gate/up tok4 + down tok4:

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 10

median_s:
  0.034183125

checksum:
  0.432987
```

```text
cargo run --release --bin bench_metalpack_prefill_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 10

median_s:
  0.068907500

checksum:
  0.432987
```

Interpretation:

```text
The down tok4 path is checksum-stable and improves p512, but the gain is much
smaller than gate/up because the previous down kernel was already relatively
fast. Further improvement needs row_tile/autotuning and trace-backed cache
counter validation, not just increasing token_tile.
```

## 2026-04-30 11:15 CEST - DeltaNet out-proj tok4 SIMD path and cache miss rule

Cache rule for this project:

```text
No kernel is allowed to have avoidable cache misses.

For streaming operators whose working set is larger than cache, literal zero
misses is impossible because the first read of streamed weights/KV/logits is
compulsory. The enforceable rule is therefore:

trace_misses <= modeled_unavoidable_compulsory_streaming_floor

Any miss traffic above that floor is a bug or a tuning target.
```

Added:

```text
kernel:
  qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok4_simd_fp16_tiled_k2048_f32

runtime switch:
  default: tok4 SIMD
  CTOX_QWEN35_DELTA_OUT_TOK2=1: old tok2 reference path
```

Isolated gated-norm + DeltaNet out-proj:

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

token_tile:
  2

median_s:
  0.007912292

checksum:
  0.985206
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 8

token_tile:
  4

median_s:
  0.004493667

checksum:
  0.985206
```

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

token_tile:
  2

median_s:
  0.014686500

checksum:
  0.985206
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 8

token_tile:
  4

median_s:
  0.009301708

checksum:
  0.985206
```

Full DeltaNet block:

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 3

project_token_tile:
  4

out_token_tile:
  2

median_s:
  0.041612334

checksum:
  -3.185984
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 6

project_token_tile:
  4

out_token_tile:
  4

median_s:
  0.033231042

checksum:
  -3.185984
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 6

project_token_tile:
  4

out_token_tile:
  4

median_s:
  0.067897875

checksum:
  -3.185984
```

Cache model after the change:

```text
cargo run --release --bin cache_analysis -- \
  --tokens 512 --decode-position 4096 --modeled-l2-mib 32 --sustained-gb-s 90

delta.out_proj:
  token_tile: 4
  modeled_unavoidable_stream: 516.00 MiB
  modeled_hit_rate: 74.9%

attention.o_proj:
  token_tile: 4-shape target
  modeled_unavoidable_stream: 516.00 MiB
  modeled_hit_rate: 74.9%
```

Interpretation:

```text
This removes one avoidable weight re-read from the DeltaNet block. The next
mandatory step is not another paper estimate: collect Metal memory/cache
counters and compare trace misses against the modeled unavoidable floor.
```

## 2026-04-30 11:35 CEST - Metal counter availability on this Mac

Added:

```text
bin:
  list_metal_counters
```

Command:

```text
cargo run --release --bin list_metal_counters
```

Observed:

```text
metal counter sampling support:
  stage: true
  draw: false
  dispatch: false
  tile_dispatch: false
  blit: false

counter_sets:
  1

set[0]:
  timestamp

counter[0]:
  GPUTimestamp
```

The `xctrace --instrument "Metal GPU Counters"` path was also tested with a
hard time limit. Instruments selected:

```text
Counter Set:
  Performance Limiters
```

but the GPU service reported:

```text
Selected counter profile is not supported on target device
```

and the exported `gpu-counter-info` / `gpu-counter-value` tables contained
schemas only, not counter rows.

Interpretation:

```text
This Mac does not expose public per-dispatch memory/cache hit/miss counters to
this probe. The cache policy still remains enforceable, but through a modeled
unavoidable miss floor plus measured kernel time/effective GB/s, not through
literal hardware L2 miss counters.

For each operation, the acceptance test becomes:

1. working set classification: fit-model vs stream
2. modeled unavoidable bytes
3. measured runtime
4. effective GB/s
5. action if effective GB/s is materially below the stream/fit floor

If Apple/Xcode exposes a private or GUI-only counter profile for this machine,
that can be plugged into the same floor comparison later.
```

## 2026-04-30 11:55 CEST - Attention out-proj tok4 SIMD benchmark

Added:

```text
bin:
  bench_metalpack_prefill_attention_out

path:
  attention.o_proj uses the same k2048 tok4 SIMD matmul kernel as DeltaNet out
```

Measurements on layer 3:

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_attention_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 512 5

token_tile:
  2

median_s:
  0.007732667

checksum:
  0.278367
```

```text
cargo run --release --bin bench_metalpack_prefill_attention_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 512 5

token_tile:
  4

median_s:
  0.005402167

checksum:
  0.278366
```

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 cargo run --release --bin \
  bench_metalpack_prefill_attention_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 1024 5

token_tile:
  2

median_s:
  0.014665250

checksum:
  0.278367
```

```text
cargo run --release --bin bench_metalpack_prefill_attention_out -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 1024 5

token_tile:
  4

median_s:
  0.009418458

checksum:
  0.278366
```

Interpretation:

```text
The attention output projection now has the same cache-floor reduction as
DeltaNet output projection. This does not solve end-to-end prefill yet, but it
removes another repeated weight-streaming source from the realistic prefill
path.
```

## 2026-04-30 12:35 CEST - First GPU-local DeltaNet + FFN prefill layer-pair

Added:

```text
kernel:
  qwen35_08b_prefill_residual_add_f32_to_fp16_k1024

bin:
  bench_metalpack_prefill_delta_ffn_block
```

This benchmark runs a real DeltaNet layer followed by its FFN in one Metal
command buffer:

```text
DeltaNet projections
causal conv
DeltaNet split/activation/scan
gated norm
DeltaNet out projection
residual add -> FP16 hidden
FFN gate/up/swiglu
FFN down
residual add -> FP16 hidden
```

There is no CPU synchronization between DeltaNet and FFN.

Default tok4/SIMD path:

```text
cargo run --release --bin bench_metalpack_prefill_delta_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

project_token_tile:
  4

out_token_tile:
  4

ffn_token_tile:
  4

down_token_tile:
  4

median_s:
  0.066651208

checksum:
  -2.954582
```

```text
cargo run --release --bin bench_metalpack_prefill_delta_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.134259167

checksum:
  -2.954582
```

Old tok2 comparison:

```text
CTOX_QWEN35_DELTA_OUT_TOK2=1 \
CTOX_QWEN35_DOWN_TOK2=1 \
CTOX_QWEN35_FFN_GATE_UP_TOK2=1 \
cargo run --release --bin bench_metalpack_prefill_delta_ffn_block -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 3

project_token_tile:
  4

out_token_tile:
  2

ffn_token_tile:
  2

down_token_tile:
  2

median_s:
  0.095618833

checksum:
  -2.954575
```

Interpretation:

```text
The token-4/SIMD cache-floor reduction survives integration into a GPU-local
DeltaNet+FFN layer-pair. The gain versus old token-2 subpaths is about 30% at
p512. The next integration step is a 3x DeltaNet+FFN superblock, then the
attention+FFN block, then [D,D,D,A] superblocks.
```

Storage note:

```text
Read-only buffers used by integrated paths now default to private Metal
storage, with CTOX_QWEN35_SHARED_WEIGHTS=1 as the explicit comparison path.
```

Repeated p512/p1024 measurements after rebuild showed shared and private
storage effectively tied for this benchmark:

```text
default private p512:
  0.050534958

shared override p512:
  0.050549917

default private p1024:
  0.101104333

shared override p1024:
  0.100807000
```

Interpretation:

```text
Private storage is kept as the correct GPU-only default, but it is not counted
as a proven speedup for this specific DeltaNet+FFN prefill block. The stable
gain here is from token-4 weight reuse and GPU-local integration, not from
storage mode.
```

## 2026-04-30 06:59 CEST - 3x DeltaNet+FFN Superblock and Row-Tile Check

Goal:

```text
Move from one integrated DeltaNet+FFN layer-pair to a real three-layer
DeltaNet superblock:

  [D+FFN, D+FFN, D+FFN]

The benchmark loads real Qwen3.5-0.8B layers 0, 1, and 2 from the metalpack and
encodes all three layer-pairs into one Metal command buffer. There is no CPU
sync between the three layer-pairs.
```

Implemented:

```text
src/bin/bench_metalpack_prefill_delta3_ffn_superblock.rs
src/metal/bench.rs:
  PrefillDeltaFfnLayerWeights
  PrefillDelta3FfnSuperblockBenchResult
  run_prefill_delta3_ffn_superblock_with_weights
```

The row-tile autotuning gate was also made explicit:

```text
CTOX_QWEN35_PACK_ROW_TILE
CTOX_QWEN35_PACK_COL_TILE
```

The first row4 attempt exposed a real limitation: the SIMD projection/out/down
kernels had row8 assumptions in their packed-load/store lanes. The token-4 SIMD
kernels were corrected to support `row_tile <= 8`; `row_tile=4` now runs
correctly, but is slower for this superblock.

Measurements, default row8 metalpack:

```text
cargo run --release --bin bench_metalpack_prefill_delta3_ffn_superblock -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

median_s:
  0.146751709

effective_gb_s_delta3_ffn_superblock_estimate:
  146.85
```

```text
cargo run --release --bin bench_metalpack_prefill_delta3_ffn_superblock -- \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 1024 5

median_s:
  0.295470042

effective_gb_s_delta3_ffn_superblock_estimate:
  145.87
```

Large prefill scaling, default row8:

```text
p4096:
  median_s 1.236608541
  effective_gb_s 139.41

p8192:
  median_s 2.500755917
  effective_gb_s 137.88

p16384:
  median_s 5.008199208
  effective_gb_s 137.69

p32768:
  median_s 10.018361209
  effective_gb_s 137.67

p65536:
  median_s 20.477873250
  effective_gb_s 134.70

p131072:
  median_s 40.944220459
  effective_gb_s 134.74
```

Row4 comparison:

```text
CTOX_QWEN35_PACK_ROW_TILE=4 cargo run --release --bin pack_weights -- \
  /Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B \
  /tmp/ctox_qwen35_08b_real_fp16_row4.metalpack

cargo run --release --bin bench_metalpack_prefill_delta3_ffn_superblock -- \
  /tmp/ctox_qwen35_08b_real_fp16_row4.metalpack 0 512 5

median_s:
  0.221138375

effective_gb_s:
  97.45
```

```text
cargo run --release --bin bench_metalpack_prefill_delta3_ffn_superblock -- \
  /tmp/ctox_qwen35_08b_real_fp16_row4.metalpack 0 1024 5

median_s:
  0.445523125

effective_gb_s:
  96.74
```

Interpretation:

```text
row8 remains the correct layout for the large projection/out/down kernels.
row4 is useful only for the specialized FFN gate/up kernel, where the shader
already computes four output rows per threadgroup while indexing an arbitrary
packed row_tile.

The 3x superblock confirms GPU-local integration, but it is not yet a
reference-class prefill implementation. Scaling is stable through 128k tokens,
but the modeled effective bandwidth plateaus around 135-147 GB/s. The next
real bottleneck is avoidable activation scratch traffic and underfilled
matmul memory throughput, not CPU synchronization.
```

Cache/miss policy update:

```text
Literal zero cache misses is not physically possible for streamed model weights
and activation ranges larger than cache. The enforceable target is zero
avoidable misses: measured/modelled traffic must not exceed the compulsory
streaming floor for each operation. On this Mac, public Metal counters expose
only GPUTimestamp, so the current cache analysis uses byte-floor modelling plus
per-kernel timing until a device/counter profile with cache-hit data is
available.
```

## 2026-04-30 07:18 CEST - tok8 Prefill Kernel Experiment Rejected

Hypothesis:

```text
Increase the heavy prefill kernels from token_tile=4 to token_tile=8 to reuse
each streamed weight tile across more tokens and reduce compulsory weight
reloads.
```

Implemented experimental kernels:

```text
qwen35_08b_prefill_rms_matmul_rowtiles_tok8_simd_fp16_tiled_k1024_f32
qwen35_08b_prefill_deltanet_out_matmul_rowtiles_tok8_simd_fp16_tiled_k2048_f32
qwen35_08b_prefill_down_matmul_rowtiles_tok8_simd_fp16_tiled_k3584_f32
qwen35_08b_prefill_ffn_gate_up_swiglu_row4_tok8_simd_fp16_tiled_k1024_i3584
```

Result:

```text
tok4 all kernels, p512:
  0.146750625

tok8 all kernels, p512:
  0.233153542

only ffn_gate_up tok8:
  0.173949959

only projection/rms tok8:
  0.179134958

only ffn_down tok8:
  0.165076666

only delta_out tok8:
  0.155503209
```

Interpretation:

```text
tok8 reduces modeled weight reloads, but loses more to register pressure and
lower occupancy than it gains from cache/streaming reuse. tok4 remains the
default. tok8 kernels are kept as explicit experiment paths via:

  CTOX_QWEN35_PREFILL_RMS_TOK8=1
  CTOX_QWEN35_FFN_GATE_UP_TOK8=1
  CTOX_QWEN35_DOWN_TOK8=1
  CTOX_QWEN35_DELTA_OUT_TOK8=1

Do not treat larger token tiles as automatically better on Apple GPU; occupancy
has to be part of the cache/miss model.
```

## 2026-04-30 07:31 CEST - fused residual output kernels

Goal:

```text
Remove the global F32 scratch and separate residual-add dispatch after:

  DeltaNet out projection
  FFN down projection

The fused kernels accumulate the projection and write the residual-added FP16
hidden state directly.
```

Implemented:

```text
qwen35_08b_prefill_deltanet_out_matmul_residual_rowtiles_tok4_simd_fp16_tiled_k2048
qwen35_08b_prefill_down_matmul_residual_rowtiles_tok4_simd_fp16_tiled_k3584
```

Single DeltaNet+FFN layer-pair:

```text
p512 before stable:
  0.050534958

p512 fused residual:
  0.049248625

p1024 before stable:
  0.101104333

p1024 fused residual:
  0.099074583
```

3x DeltaNet+FFN superblock:

```text
p512 before:
  0.146751709

p512 fused residual:
  0.147128666

p1024 before:
  0.295470042

p1024 fused residual:
  0.296188208
```

Interpretation:

```text
The fusion helps the isolated layer-pair slightly, but does not improve the
3-layer superblock. In the superblock the dominant cost remains the projection
and FFN matrix streams; the removed residual dispatches are below the noise
floor once three real layer-pairs are chained in one command buffer.

The fused kernels are kept because they reduce scratch surface and are neutral
to slightly positive locally, but they are not counted as a reference-closing
optimization.
```

## 2026-04-30 07:43 CEST - 18 DeltaNet+FFN Layer Stack

Goal:

```text
Run all 18 real Qwen3.5-0.8B DeltaNet layers, each with its FFN, in one
GPU-local command buffer:

  0,1,2,4,5,6,8,9,10,12,13,14,16,17,18,20,21,22

This intentionally excludes the 6 full-attention layers. It measures whether
the DeltaNet-specialized path scales across the complete DeltaNet side of the
model without CPU bubbles.
```

Implemented:

```text
run_prefill_delta_ffn_stack_with_weights

bench_metalpack_prefill_delta3_ffn_superblock now accepts:
  [delta-layer-count]

Example:
  cargo run --release --bin bench_metalpack_prefill_delta3_ffn_superblock -- \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 3 1 18
```

Measurements:

```text
18 DeltaNet+FFN layers, p512:
  median_s 0.880873666
  effective_gb_s 146.78

18 DeltaNet+FFN layers, p1024:
  median_s 1.772145084
  effective_gb_s 145.92

18 DeltaNet+FFN layers, p4096:
  median_s 7.445820250
  effective_gb_s 138.92
```

Interpretation:

```text
The full DeltaNet side scales almost exactly linearly from the 3-layer
superblock. This is good for CPU/command-buffer overhead: there is no extra
bubble when chaining all 18 DeltaNet+FFN layer-pairs in one command buffer.

It is bad for performance headroom: there is no hidden cross-layer cache reuse
or fusion gain. The current prefill implementation is still a batched matvec
path. To compete with llama.cpp prefill, the next major change must be a
GEMM-shaped prefill path or a much more aggressive producer/consumer pipeline,
not more command-buffer fusion.
```

## 2026-04-30 07:58 CEST - SIMDgroup MMA FFN-down Prototype

Goal:

```text
Test whether Apple SIMDgroup matrix operations can replace the current
batched-matvec FFN down projection:

  act[tokens, 3584] x down_weight[1024, 3584]^T -> out[tokens, 1024]
```

Implemented experimental kernel:

```text
qwen35_08b_prefill_down_mma8x8_fp16_tiled_k3584_f32

Enabled only with:
  CTOX_QWEN35_DOWN_MMA=1
```

Isolated FFN-down measurements:

```text
p512 current tok4:
  0.005175167

p512 MMA 8x8:
  0.004422791

p1024 current tok4:
  0.010222208

p1024 MMA 8x8:
  0.009158167
```

Integrated FFN-block measurement:

```text
p512 current FFN block:
  0.024184083
  checksum16 0.432987

p512 FFN block with DOWN_MMA:
  0.036322042
  checksum16 0.337831
```

Interpretation:

```text
The isolated MMA kernel is faster, but it is not accepted into the main path:
inside the FFN block it is slower and produces a different checksum. The likely
problem is matrix orientation/layout interaction with the existing packed
row-tiled weights and simdgroup_store/load conventions.

Action:
  keep the MMA kernel behind CTOX_QWEN35_DOWN_MMA for further debugging
  do not use it for DeltaNet+FFN stack or reference comparisons
  next MMA step must add explicit max-error validation against the tok4 kernel
  before any integration attempt
```

Superseded correction:

```text
The first integrated FFN-block result above was invalid. The integration
accidentally ran the Gate/Up producer with the 32-thread MMA threadgroup size
instead of the required 256-thread group. After fixing that dispatch bug,
DOWN_MMA is correct and mildly faster:

p512 FFN block:
  baseline 0.024154708 s
  DOWN_MMA 0.023309417 s
  checksum16 identical: 0.432987

p1024 FFN block:
  baseline 0.048247667 s
  DOWN_MMA 0.046405167 s
  checksum16 identical: 0.432987

Isolated down compare, p512:
  baseline_median_s 0.005199417
  mma_median_s      0.004316917
  max_abs_error     0.000001848
  mean_abs_error    0.000000044
```

## 2026-04-30 08:38 CEST - DeltaNet+FFN Projection Cache Fixes

Goal:

```text
Remove avoidable cache misses and hidden-state rereads in the DeltaNet+FFN
prefill superblock.
```

Implemented:

```text
1. Optional DeltaNet input projection split:
   CTOX_QWEN35_PROJECT_SPLIT_NORM=1

   Old:
     RMSNorm+matmul qkv
     RMSNorm+matmul z
     RMSNorm+matmul b
     RMSNorm+matmul a

   New:
     RMSNorm once -> half scratch
     plain matmul qkv/z/b/a from normalized scratch

2. Optional FFN Gate/Up MMA:
   CTOX_QWEN35_FFN_GATE_UP_MMA=1

   RMSNorm once -> half scratch
   SIMDgroup MMA accumulates gate and up in parallel
   SwiGLU is written directly as half activation

3. DOWN_MMA remains optional:
   CTOX_QWEN35_DOWN_MMA=1
```

Single DeltaNet+FFN layer-pair, layer 0:

```text
p512:
  default                              0.049229417 s
  split project norm                   0.046049417 s
  split project norm + DOWN_MMA        0.045112417 s
  + FFN_GATE_UP_MMA                    0.032186500 s

p1024:
  default                              0.098836250 s
  split project norm                   0.092566250 s
  split project norm + DOWN_MMA        0.090223916 s
  + FFN_GATE_UP_MMA                    0.064461209 s
```

3x DeltaNet+FFN superblock:

```text
p512:
  default                              0.147018625 s
  split project norm + DOWN_MMA        0.134787708 s
  + FFN_GATE_UP_MMA                    0.096230083 s

p1024:
  default                              0.295407084 s
  split project norm + DOWN_MMA        0.270192375 s
  + FFN_GATE_UP_MMA                    0.192882375 s
```

18x DeltaNet+FFN stack:

```text
p512:
  default                              0.880983791 s
  split project norm + DOWN_MMA
    + FFN_GATE_UP_MMA                  0.576156625 s

p1024:
  default                              1.775197750 s
  split project norm + DOWN_MMA
    + FFN_GATE_UP_MMA                  1.155681750 s

p4096:
  default                              7.430657166 s
  split project norm + DOWN_MMA        6.628482834 s
  split project norm + DOWN_MMA
    + FFN_GATE_UP_MMA                  4.740564875 s

p8192:
  split project norm + DOWN_MMA        13.906410292 s
  split project norm + DOWN_MMA
    + FFN_GATE_UP_MMA                  10.129036792 s
```

Gate/Up MMA validation against the previous Gate/Up kernel:

```text
p512:
  baseline_median_s 0.019130459
  mma_median_s      0.006295208
  max_abs_error     0.001953125
  mean_abs_error    0.000025076

p1024:
  baseline_median_s 0.038102000
  mma_median_s      0.012222958
  max_abs_error     0.001953125
  mean_abs_error    0.000025065
```

Interpretation:

```text
This is the first cache-analysis-driven prefill speedup that materially changes
the stack result. The main win did not come from CPU synchronization or command
buffer changes; it came from removing repeated normalization/read traffic and
turning the largest FFN producer into an MMA-shaped kernel.

The current optimized 18x DeltaNet+FFN path is still not a full Qwen3.5 prefill:
the 6 attention layers and their FFNs are not included in this stack benchmark.
The next reference-closing work must implement the attention-layer FFN with the
same split-norm + Gate/Up-MMA + Down-MMA path, then add the attention projection
and online attention kernels.
```

## 2026-04-30 09:10 CEST - Attention FFN and QKV Projection Prefill

Implemented:

```text
1. General prefill FFN block now supports:
   CTOX_QWEN35_FFN_GATE_UP_MMA=1
   CTOX_QWEN35_DOWN_MMA=1

   This makes the optimized FFN path available to the 6 Full-Attention layers,
   not only to the DeltaNet+FFN superblock.

2. Attention Q/K/V prefill projection benchmark:
   RMSNorm once -> q/k/v matmuls from normalized hidden scratch

3. Experimental K=1024 projection MMA:
   CTOX_QWEN35_PROJECT_MMA=1
```

Attention-layer FFN, layer 3:

```text
p512:
  baseline FFN block                   0.024272750 s
  Gate/Up-MMA + Down-MMA               0.010368167 s

p1024:
  baseline FFN block                   0.048271042 s
  Gate/Up-MMA + Down-MMA               0.020527416 s
```

Attention q/k/v projection, layer 3:

```text
p512:
  separate RMS+q, RMS+k, RMS+v sum     0.012644667 s
  split norm + SIMD matmul             0.010268667 s
  split norm + PROJECT_MMA             0.006079917 s

p1024:
  separate RMS+q, RMS+k, RMS+v sum     0.024645084 s
  split norm + SIMD matmul             0.020371166 s
  split norm + PROJECT_MMA             0.011940917 s
```

Attention out projection, layer 3:

```text
p512:                                  0.004192084 s
p1024:                                 0.006582708 s
```

Rejected experiment:

```text
Using CTOX_QWEN35_PROJECT_MMA=1 globally for DeltaNet split projections is not
acceptable:

18x DeltaNet+FFN stack, all other optimizations enabled:
  p512 without PROJECT_MMA              0.576156625 s
  p512 with PROJECT_MMA                 1.630143042 s

  p1024 without PROJECT_MMA             1.155681750 s
  p1024 with PROJECT_MMA                3.260357250 s

  p4096 without PROJECT_MMA             4.740564875 s
  p4096 with PROJECT_MMA                13.145578250 s

PROJECT_MMA is therefore only a useful candidate for large attention q/k/v-like
K=1024 projections for now. It must not be enabled for the DeltaNet stack.
```

Current prefill cost picture:

```text
The optimized FFN path is now usable for all 24 layers.
The 18 DeltaNet+FFN stack is materially faster.
The 6 attention-layer FFNs are materially faster.
The attention q/k/v and o projections now have isolated optimized measurements.

Still missing for a full Qwen3.5 prefill comparison:
  q/k normalization
  RoPE
  online causal attention over prefill sequence
  attention output residual integration
  full 24-layer command-buffer assembly
```

## 2026-04-30 09:42 CEST - First Real Attention Prefill Core

Implemented:

```text
q/k/v projection from normalized hidden
q/k per-head RMSNorm
RoPE for q/k
half KV cache for the whole prefill sequence
causal online softmax attention
gate application from q_proj gate half
o_proj
```

Kernel files:

```text
vendor/metal/shaders/qwen35_08b/prefill_attention.metal
```

Benchmark:

```text
bench_metalpack_prefill_attention_core
```

Layer 3 attention core with q/k/v PROJECT_MMA:

```text
p128:
  0.006422042 s

p512:
  0.028702584 s

p1024:
  0.097579667 s

p2048:
  0.323304041 s

p4096:
  1.121225417 s
```

Interpretation:

```text
This is now a real Full-Attention prefill core, not just q/k/v or o-projection
microbenching. It is also clearly not the final architecture.

The current attention kernel maps one threadgroup to one (query token, q-head)
and loops over all visible keys inside that threadgroup. That is correct enough
for first integration and captures q/k norm, RoPE, causal masking and gate
application, but the O(T^2) work is not tiled across query/key blocks like a
FlashAttention-style kernel.

At p4096, one attention layer already costs 1.12 s. Six attention layers would
cost roughly 6.7 s before their FFNs. This alone prevents beating llama.cpp
prefill. The next architecture change must be blockwise tiled attention:

  query block x key block
  per-block max/sum/value partials
  cross-block online softmax combine
  no materialized score matrix
  q/k/v cache in token-major or block-major layout chosen for coalesced loads

The current kernel remains useful as a correctness/performance baseline for the
next tiled attention implementation.
```

## 2026-04-30 09:34 CEST - Memory Forensics Tooling

Implemented:

```text
src/bin/memory_forensics.rs
```

Purpose:

```text
Run the current real Metal prefill component benchmarks as one forensics pass.
Parse median time and benchmark GB/s.
Compute a byte-floor model per component.
Estimate excess bytes/time against an explicit sustained-bandwidth assumption.
Expose when benchmark-internal byte accounting misses hidden traffic.
```

Important hardware-counter finding:

```text
cargo run --release --bin list_metal_counters

Available counter set on this Mac:
  GPUTimestamp only

No public L2/cache-miss counter is exposed through the current Metal counter
API path here. Therefore cache-miss analysis in this probe is currently:

  measured kernel time
  + explicit byte movement model
  + inferred excess over streaming floor
  + optional Xcode/Metal trace capture outside the CLI path
```

This means "no cache misses" cannot be a literal measurable requirement on
this setup. The actionable target is:

```text
zero avoidable misses:
  no unnecessary global scratch writes
  no repeated normalization reads when split norm can be used
  no full logits readback
  no untiled O(T^2) KV streaming when blockwise reuse is possible
```

Forensics command:

```text
cargo build --release --bins
target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 512 3 150
target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 1024 2 150
target/release/memory_forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack 2048 1 150
```

The `150 GB/s` value is an explicit working roofline assumption for these
component kernels, not a hardware claim. It is used to make excess-memory
forensics comparable across runs.

Results:

```text
p512:
  delta18+ffn       0.705745 s, 147.27 GB/s forensic,  96.80 GiB model
  attention.core    0.029618 s,  77.71 GB/s forensic,   2.14 GiB model
  attention.ffn     0.011674 s, 121.39 GB/s forensic,   1.32 GiB model
  full estimate     0.953 s,   536.97 tok/s

p1024:
  delta18+ffn       1.403451 s, 148.11 GB/s forensic, 193.59 GiB model
  attention.core    0.093340 s,  72.32 GB/s forensic,   6.29 GiB model
  attention.ffn     0.021906 s, 129.39 GB/s forensic,   2.64 GiB model
  full estimate     2.095 s,   488.80 tok/s

p2048:
  delta18+ffn       2.837707 s, 146.50 GB/s forensic, 387.17 GiB model
  attention.core    0.315080 s,  70.11 GB/s forensic,  20.57 GiB model
  attention.ffn     0.045158 s, 125.53 GB/s forensic,   5.28 GiB model
  full estimate     4.999 s,   409.67 tok/s
```

Key forensic finding:

```text
The DeltaNet+FFN stack is now near the current streaming floor.
The remaining prefill blocker is not CPU sync.
The remaining prefill blocker is not the optimized FFN path.
The current blocker is the attention core.
```

The attention benchmark's original internal byte estimate undercounted the
hidden O(T^2) KV traffic. The new forensics tool adds this explicitly:

```text
attention_t2_stream_floor per attention layer:
  p512:   1.00 GiB
  p1024:  4.00 GiB
  p2048: 16.01 GiB
```

Interpretation:

```text
The current attention prefill kernel performs one threadgroup per
(query token, q-head) and streams all previous K/V for that query. That makes
cache behavior structurally bad for long prefill because the same K/V blocks
are reloaded for many nearby queries.

The next kernel cannot be a small tweak of this layout. It must be a tiled
attention kernel:

  query block x key block
  K/V tile loaded once for multiple query rows
  per-query online max/sum/value partials
  second-stage combine or in-kernel block accumulation
  no materialized score matrix

Only after this change does it make sense to re-run the full prefill estimate.
```

## 2026-04-30 09:34 CEST - Attention Query-Block Experiments

Implemented experimental attention kernels:

```text
CTOX_QWEN35_ATTENTION_QBLK4=1
  qwen35_08b_prefill_attention_causal_qblk4_gqa8_kv2_d256_to_fp16

CTOX_QWEN35_ATTENTION_QBLK2=1
  qwen35_08b_prefill_attention_causal_qblk2_gqa8_kv2_d256_to_fp16

CTOX_QWEN35_ATTENTION_QBLK2X512=1
  qwen35_08b_prefill_attention_causal_qblk2x512_gqa8_kv2_d256_to_fp16
```

Intent:

```text
Reuse K/V loads across adjacent query tokens instead of reloading the same
K/V vector for every single query token.
```

Results:

```text
p512 attention core:
  baseline per-query     0.029869291 s, checksum -0.697315
  QBLK2 serial           0.031112125 s, checksum -0.697315
  QBLK4 serial           0.033754042 s, checksum -0.697315
```

Rejected:

```text
QBLK2 and QBLK4 are correct but slower.

They reduce K/V load count, but they serialize multiple query reductions in one
threadgroup and lose too much parallelism. This proves that "fewer memory
loads" alone is not sufficient if the kernel loses occupancy and adds barriers.
```

Dispatch bug found and fixed:

```text
The first QBLK implementation accidentally applied the query-block dispatch to
the prepare_qk_rope_v kernel. That skipped prepare work for some tokens and
made QBLK2X512 appear faster but incorrect.

Fix:
  prepare_qk_rope_v always dispatches as (tokens, q_heads) x head_dim.
  only the attention kernel dispatches by query block.
```

QBLK2X512 after dispatch fix:

```text
p2 attention core, PROJECT_MMA off:
  baseline per-query     0.000580000 s, checksum -0.697334
  QBLK2X512              0.000336833 s, checksum -0.697334

p512 attention core:
  baseline per-query     0.030085000 s, checksum -0.697315
  QBLK2X512              0.036463375 s, checksum -0.697315

p1024 attention core:
  baseline per-query     0.094988500 s, checksum -0.697315
  QBLK2X512              0.118900000 s, checksum -0.697315

p2048 attention core:
  baseline per-query     0.325545083 s, checksum -0.697315
  QBLK2X512              0.408040666 s, checksum -0.697315
```

Interpretation:

```text
QBLK2X512 is now correct, but slower for realistic token counts. The tiny p2
case wins because launch and duplicated K/V loads dominate. At real prefill
sizes, the larger 512-thread threadgroup plus explicit threadgroup-memory K/V
staging loses occupancy/latency enough to offset the saved K/V reads.

This rejects simple query-block reuse as the next path. The next attention
kernel must tile both query and key blocks and combine online softmax partials,
instead of only grouping adjacent queries in one threadgroup.
```

Next action:

```text
Build a true two-stage blockwise attention prefill:
  stage 1: query_block x key_block partial max/sum/value
  stage 2: combine partials per query/head
  measure K/V tile reuse and scratch traffic with memory_forensics
```

## 2026-04-30 09:48 CEST - Raw Attention Forensics and Partial Attention Rejection

Implemented:

```text
CTOX_QWEN35_ATTENTION_RAW_DUMP=/tmp/file.bin
  Dumps the raw attention output buffer before o_proj.

compare_attention_raw_dump
  Compares two raw half dumps and reports first mismatch, token/head/lane,
  mismatch count, max abs error, mean abs error and checksum.
```

Added experimental two-stage partial attention:

```text
CTOX_QWEN35_ATTENTION_PARTIAL_QBLK2=1

stage 1:
  qblk2 x kblk64 partial m/l/acc

stage 2:
  combine partial m/l/acc per query/head
```

Correctness:

```text
p512 raw attention dump compare, baseline vs PARTIAL_QBLK2:
  elements          1,048,576
  mismatch_count    466
  mean_abs_error    0.000000060
  max_abs_error     0.000976562
  first mismatch    token 73, head 6, lane 81

p512 raw attention dump compare, baseline vs QBLK2X512:
  mismatch_count    0
  mean_abs_error    0
  max_abs_error     0
```

Interpretation:

```text
QBLK2X512 is bit-identical after the dispatch fix, but slower.
PARTIAL_QBLK2 is numerically close but not bit-identical because it changes
softmax accumulation order across key blocks. The error is within a small FP16
tolerance, but performance is the deciding issue here.
```

Performance:

```text
p64, PROJECT_MMA off:
  baseline per-query     0.003479625 s
  PARTIAL_QBLK2          0.002606750 s

p512, PROJECT_MMA on:
  baseline per-query     0.025721417 s
  PARTIAL_QBLK2          0.048636333 s

p1024, PROJECT_MMA on:
  baseline per-query     0.082997833 s
  PARTIAL_QBLK2          0.172698833 s

p2048, PROJECT_MMA on:
  baseline per-query     0.292039333 s
  PARTIAL_QBLK2          0.635094584 s
```

Rejected:

```text
PARTIAL_QBLK2 is not a viable prefill architecture for this model on this Mac.
It wins only in tiny contexts. At realistic prefill sizes, partial_acc scratch
traffic and second-stage combine dominate.

The next attention attempt must avoid large per-query x key-block x head_dim
scratch. A viable path needs either:

  1. in-kernel block accumulation with enough resident work to avoid deadlock,
     or
  2. smaller compressed partials that do not write full head_dim accumulators
     per key block,
     or
  3. a different prefill strategy that leans on MLX/Core ML for prefill while
     keeping the custom Metal path focused on decode.
```

Sweep tool:

```text
attention_variant_sweep
```

Command:

```text
target/release/attention_variant_sweep \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 512,1024 3 1
```

Result:

```text
p512:
  baseline        28.310 ms
  qblk2           29.152 ms
  qblk4           30.667 ms
  qblk2x512       34.038 ms
  partial_qblk2   53.447 ms

p1024:
  baseline        91.391 ms
  qblk2           96.800 ms
  qblk4          102.885 ms
  qblk2x512      116.287 ms
  partial_qblk2  195.423 ms
```

Decision:

```text
Keep the per-query attention core as the current valid baseline.
Do not use QBLK2/QBLK4/QBLK2X512/PARTIAL_QBLK2 for the main path.
The next architecture must target a qualitatively different reduction of
attention traffic, not just grouping adjacent queries.
```

## 2026-04-30 SIMDgroup Attention Reduction

Implemented:

```text
qwen35_08b_prefill_attention_causal_simdreduce_gqa8_kv2_d256_to_fp16
```

This keeps the valid per-query attention architecture but replaces the per-key
256-lane threadgroup reduction:

```text
old:
  write 256 partials
  threadgroup barrier
  8-step threadgroup reduction
  barrier per step

new:
  simd_sum inside each 32-lane SIMDgroup
  write 8 SIMDgroup partials
  one SIMDgroup reduces those 8 values
  far fewer barriers per key
```

Correctness:

```text
p512 raw attention dump compare, baseline vs SIMDREDUCE:
  elements          1,048,576
  mismatch_count    648
  mean_abs_error    0.000000029
  max_abs_error     0.000976562
  first mismatch    token 6, head 1, lane 67

The raw output is not bit-identical because the reduction order changes, but
the difference is within a tiny FP16 tolerance. Final checksum remained stable
in the attention-core benchmark.
```

Attention variant sweep:

```text
p512:
  old baseline     27.811 ms
  SIMDREDUCE       17.335 ms

p1024:
  old baseline     91.405 ms
  SIMDREDUCE       48.661 ms

p2048:
  old baseline    317.030 ms
  SIMDREDUCE      151.725 ms
```

Decision:

```text
SIMDREDUCE is now the default attention core.
The old barrier-heavy reduction is retained behind:
  CTOX_QWEN35_ATTENTION_NO_SIMDREDUCE=1
```

Memory forensics after SIMDREDUCE:

```text
p512:
  delta18+ffn       0.695567 s, 149.42 GB/s forensic,  96.79 GiB model
  attention.core    0.016627 s, 138.43 GB/s forensic,   2.14 GiB model
  attention.ffn     0.011848 s, 119.61 GB/s forensic,   1.32 GiB model
  full estimate     0.866 s,   590.94 tok/s

p1024:
  delta18+ffn       1.406576 s, 147.78 GB/s forensic, 193.59 GiB model
  attention.core    0.050060 s, 134.85 GB/s forensic,   6.29 GiB model
  attention.ffn     0.025448 s, 111.37 GB/s forensic,   2.64 GiB model
  full estimate     1.860 s,   550.65 tok/s

p2048:
  delta18+ffn       2.818363 s, 147.51 GB/s forensic, 387.18 GiB model
  attention.core    0.149916 s, 147.36 GB/s forensic,  20.57 GiB model
  attention.ffn     0.048730 s, 116.33 GB/s forensic,   5.28 GiB model
  full estimate     4.010 s,   510.69 tok/s
```

Interpretation:

```text
This is the first successful attention-core optimization. The attention kernel
is now near the same measured streaming roofline as the DeltaNet+FFN stack at
p2048.

This does not solve long-context prefill by itself: O(T^2) KV traffic remains
the mathematical scaling problem. But the previous attention implementation was
also wasting time on reduction barriers; that avoidable overhead is now largely
removed.
```

## 2026-04-30 Selective DeltaNet Projection MMA

Problem:

```text
The earlier global CTOX_QWEN35_PROJECT_MMA=1 experiment was rejected because it
made the DeltaNet stack much slower. That experiment used the K=1024 MMA kernel
for qkv, z, b and a projections.

The new hypothesis was that only the large qkv/z projections should use MMA;
the tiny b/a projections should stay on the SIMD row-tile path.
```

Implemented:

```text
Default:
  DeltaNet qkv + z projections use:
    qwen35_08b_prefill_matmul_mma8x8_fp16_tiled_k1024_f32

  DeltaNet b + a projections keep:
    qwen35_08b_prefill_matmul_rowtiles_tok4_simd_fp16_tiled_k1024_f32

Opt-out:
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA=1
```

Measured 3x DeltaNet+FFN stack:

```text
p512:
  previous optimized      0.117354666 s
  selective qkv/z MMA     0.091447500 s

p1024:
  previous optimized      0.233855459 s
  selective qkv/z MMA     0.184082708 s
```

Measured 18x DeltaNet+FFN stack:

```text
p512:
  previous optimized      0.697370958 s
  selective qkv/z MMA     0.543710916 s

p1024:
  previous optimized      1.400430541 s
  selective qkv/z MMA     1.041862333 s
```

Memory forensics after selective qkv/z MMA and SIMDREDUCE attention:

```text
p512:
  delta18+ffn       0.521769 s, 162.15 GB/s forensic,  78.79 GiB model
  attention.core    0.022421 s, 102.65 GB/s forensic,   2.14 GiB model
  attention.ffn     0.010492 s, 135.07 GB/s forensic,   1.32 GiB model
  full estimate     0.719 s,   711.85 tok/s

p1024:
  delta18+ffn       1.058953 s, 159.79 GB/s forensic, 157.59 GiB model
  attention.core    0.054304 s, 124.31 GB/s forensic,   6.29 GiB model
  attention.ffn     0.021932 s, 129.23 GB/s forensic,   2.64 GiB model
  full estimate     1.516 s,   675.30 tok/s

p2048:
  delta18+ffn       2.183142 s, 155.01 GB/s forensic, 315.17 GiB model
  attention.core    0.166404 s, 132.76 GB/s forensic,  20.57 GiB model
  attention.ffn     0.042148 s, 134.49 GB/s forensic,   5.28 GiB model
  full estimate     3.434 s,   596.31 tok/s

p4096:
  delta18+ffn       4.482514 s, 150.99 GB/s forensic, 630.33 GiB model
  attention.core    0.540152 s, 145.41 GB/s forensic,  73.15 GiB model
  attention.ffn     0.085926 s, 131.94 GB/s forensic,  10.56 GiB model
  full estimate     8.239 s,   497.15 tok/s
```

Interpretation:

```text
Selective MMA fixed the bad global PROJECT_MMA result and produced a real
DeltaNet stack improvement. The model is still far from llama.cpp prefill:

  llama.cpp pp4096 reference: 2852.70 tok/s
  current CTOX estimate:       497.15 tok/s

The remaining gap is mostly weight reuse. Even with qkv/z MMA, the current
DeltaNet+FFN path still streams weights at small token tiles. To close the gap,
the next work must increase token-block reuse for the large linear operators,
not merely reduce dispatch overhead.
```

## 2026-04-30 10:06 CEST - DOWN_MMA16 Re-test and Cache-Inference Tooling

Problem:

```text
The first DOWN_MMA16 experiment appeared much slower than DOWN_MMA8, but static
inspection found a benchmark/runtime bug:

  prefill_down_matmul_kernel()
      selected qwen35_08b_prefill_down_mma16x8...

  prefill_down_threadgroup_threads()
      only checked CTOX_QWEN35_DOWN_MMA
      did not check CTOX_QWEN35_DOWN_MMA16

Result:
  DOWN_MMA16 was launched with 256 threads instead of 32.
  That allowed multiple SIMDgroups in a threadgroup to duplicate the same
  simdgroup-matrix work and overwrite the same output.

The previous DOWN_MMA16 rejection was therefore invalid.
```

Fix:

```text
prefill_down_threadgroup_threads():
  CTOX_QWEN35_DOWN_MMA=1   -> 32 threads
  CTOX_QWEN35_DOWN_MMA16=1 -> 32 threads
```

Re-test, full FFN block with GateUp MMA enabled:

```text
p512:
  down8    median_s 0.011989000
  down16   median_s 0.010008208

p1024:
  down8    median_s 0.022687708
  down16   median_s 0.018322708
```

Re-test, 18x DeltaNet+FFN stack:

```text
p512:
  down8    median_s 0.539459292
  down16   median_s 0.507874333

p1024:
  down8    median_s 1.086048834
  down16   median_s 1.022277250
```

Decision:

```text
DOWN_MMA16 is no longer rejected.
memory_forensics now uses:
  CTOX_QWEN35_DOWN_MMA16=1
  CTOX_QWEN35_FFN_GATE_UP_MMA=1
  CTOX_QWEN35_PROJECT_SPLIT_NORM=1

GateUp-MMA16 remains lower priority. Static analysis says the naive 16-token
GateUp variant would likely double live accumulator state versus GateUp-MMA8
and create much higher register pressure. If tested, it must be isolated behind
a separate flag and compared before stack integration.
```

Updated memory forensics with DOWN_MMA16:

```text
p512:
  delta18+ffn       0.509906 s, 157.63 GB/s forensic,  74.86 GiB model
  attention.core    0.017454 s, 131.87 GB/s forensic,   2.14 GiB model
  attention.ffn     0.009725 s, 121.57 GB/s forensic,   1.10 GiB model
  full estimate     0.673 s,   760.80 tok/s

p1024:
  delta18+ffn       1.018684 s, 157.80 GB/s forensic, 149.71 GiB model
  attention.core    0.050656 s, 133.27 GB/s forensic,   6.29 GiB model
  attention.ffn     0.018357 s, 128.81 GB/s forensic,   2.20 GiB model
  full estimate     1.433 s,   714.70 tok/s

p2048:
  delta18+ffn       2.075806 s, 154.88 GB/s forensic, 299.42 GiB model
  attention.core    0.153995 s, 143.46 GB/s forensic,  20.57 GiB model
  attention.ffn     0.041121 s, 115.01 GB/s forensic,   4.40 GiB model
  full estimate     3.246 s,   630.83 tok/s

p4096:
  delta18+ffn       4.134189 s, 155.54 GB/s forensic, 598.87 GiB model
  attention.core    0.530639 s, 148.02 GB/s forensic,  73.15 GiB model
  attention.ffn     0.082914 s, 114.07 GB/s forensic,   8.81 GiB model
  full estimate     7.816 s,   524.09 tok/s
```

Cache/miss forensics additions:

```text
memory_forensics now reports a second line per scope:

  dram_equiv:
      median_s * sustained_bandwidth

  cache_or_model_overcount_lb:
      max(0, model_bytes - dram_equiv) / model_bytes

  unmodeled_or_stall:
      max(0, dram_equiv - model_bytes)

  unique_weights:
      parsed packed_bytes from the benchmark output

  modeled/unique:
      model_bytes / unique_weights

These are inference metrics, not hardware L2/SLC counters. The current Mac
counter path exposes GPUTimestamp only. The goal is to separate:

  1. logical model bytes that must be served by cache or are overcounted
  2. time not explained by the byte model at the assumed sustained bandwidth
  3. excessive weight re-streaming caused by small token tiles
```

Example p1024:

```text
delta18+ffn:
  dram_equiv                  147.27 GiB
  cache_or_model_overcount_lb   1.6%
  unmodeled_or_stall             0 B
  unique_weights             739.12 MiB
  modeled/unique             207.4x

attention.core:
  dram_equiv                    7.06 GiB
  cache_or_model_overcount_lb   0.0%
  unmodeled_or_stall          793.57 MiB
  unique_weights              14.00 MiB
  modeled/unique             459.9x

attention.ffn:
  dram_equiv                    2.90 GiB
  cache_or_model_overcount_lb   0.0%
  unmodeled_or_stall          718.63 MiB
  unique_weights              21.00 MiB
  modeled/unique             107.4x
```

Research notes for next optimization hypotheses:

```text
FlashAttention:
  The relevant idea is IO-aware exact attention: tile Q/K/V and use online
  softmax so the full attention matrix is never materialized in global memory.
  Our SIMDREDUCE attention is a partial local version of this idea, but long
  prefill still pays O(T^2) KV traffic.
  Source: https://arxiv.org/abs/2205.14135

Gated DeltaNet:
  The model family explicitly combines gating for memory erasure with delta
  updates for targeted memory modification. The optimization-relevant part is
  the parallel/chunkwise training algorithm and equivalence to recurrent state.
  Source: https://proceedings.iclr.cc/paper_files/paper/2025/hash/4904fad153f6434a7bcf04465d4be2cc-Abstract-Conference.html

DeltaNet parallelization:
  The DeltaNet sequence-parallel work uses hardware-efficient representations
  for delta-rule linear transformers. For this project, the useful direction is
  not changing model math, but looking for chunk summaries/prefix composition
  that can reduce recurrent prefill serialization.
  Source: https://arxiv.org/abs/2406.06484

Fast Weight Programmer view:
  Linear attention and DeltaNet can be read as fast-weight memory updates. This
  is useful for deriving equivalent state-update groupings and deciding which
  state can be kept local in a persistent/chunked kernel.
  Source: https://arxiv.org/abs/2102.11174

Open-TQ-Metal:
  Recent Apple-Silicon-specific work reports fused compressed-domain attention
  with int4 KV-cache and direct attention on compressed KV data. This is most
  relevant for long-context attention and later decode KV-cache work, not for
  the current DeltaNet weight-stream bottleneck.
  Source: https://arxiv.org/abs/2604.16957
```

## 2026-04-30 10:06 CEST - Project-MMA16 and Delta-Out-MMA16 Experiments

Project-MMA16 hypothesis:

```text
The selective QKV/Z DeltaNet projection path was still using MMA8. QKV and Z
are large K=1024 matrix-vector/matrix-block products without activation
side-effects, so a 16-token MMA variant should improve weight reuse in the
same way DOWN_MMA16 did, but with lower risk than GateUp-MMA16.
```

Implemented:

```text
New kernel:
  qwen35_08b_prefill_matmul_mma16x8_fp16_tiled_k1024_f32

Runtime behavior:
  Delta QKV/Z projection uses MMA16 automatically when:
    tokens % 16 == 0
    row_tile == 8
    CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA is not set

  Fallback:
    CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8=1 forces the older MMA8 path.
```

Measured 18x DeltaNet+FFN stack, with Down16 + GateUp-MMA:

```text
p512:
  Project-MMA8 forced      0.491625542 s
  Project-MMA16 default    0.409341167 s

p1024:
  Project-MMA8 path        0.986317208 s
  Project-MMA16 path       0.818559167 s

checksums:
  unchanged at -0.915833
```

Updated memory forensics with Project-MMA16 + Down16:

```text
p512:
  delta18+ffn       0.423492 s, 166.98 GB/s forensic,  65.86 GiB model
  attention.core    0.017509 s, 131.45 GB/s forensic,   2.14 GiB model
  attention.ffn     0.009792 s, 120.74 GB/s forensic,   1.10 GiB model
  full estimate     0.587 s,   871.79 tok/s

p1024:
  delta18+ffn       0.847940 s, 166.79 GB/s forensic, 131.71 GiB model
  attention.core    0.049886 s, 135.32 GB/s forensic,   6.29 GiB model
  attention.ffn     0.019478 s, 121.39 GB/s forensic,   2.20 GiB model
  full estimate     1.264 s,   810.04 tok/s

p2048:
  delta18+ffn       1.706557 s, 165.74 GB/s forensic, 263.42 GiB model
  attention.core    0.163921 s, 134.77 GB/s forensic,  20.57 GiB model
  attention.ffn     0.036513 s, 129.52 GB/s forensic,   4.40 GiB model
  full estimate     2.909 s,   703.98 tok/s

p4096:
  delta18+ffn       3.441759 s, 164.36 GB/s forensic, 526.84 GiB model
  attention.core    0.531707 s, 147.72 GB/s forensic,  73.15 GiB model
  attention.ffn     0.081654 s, 115.83 GB/s forensic,   8.81 GiB model
  full estimate     7.122 s,   575.13 tok/s
```

Interpretation:

```text
This is a real improvement, but still not close to llama.cpp pp4096.

llama.cpp pp4096 reference: 2852.70 tok/s
current CTOX p4096 estimate: 575.13 tok/s

The Project-MMA16 change validates the current direction: increase token-block
weight reuse on the large linear operators first. It does not yet solve the
overall architecture gap.
```

Delta-Out-MMA16 experiment:

```text
New kernel:
  qwen35_08b_prefill_deltanet_out_mma16x8_fp16_tiled_k2048_f32

Integration:
  CTOX_QWEN35_DELTA_OUT_MMA16=1
  computes the DeltaNet output projection into an f32 buffer, then runs the
  existing residual-add f32->fp16 kernel.

Measured 18x DeltaNet+FFN stack:

p512:
  default delta-out residual tok4     0.392742583 s
  delta-out MMA16 experiment          0.355617625 s

p1024:
  default delta-out residual tok4     0.785680417 s
  delta-out MMA16 experiment          0.711098334 s

checksum16:
  default      -0.915833
  MMA16 exp    -0.910950
```

Decision:

```text
Delta-Out-MMA16 is performance-positive but not promoted yet.
The checksum shift is small and probably comes from different FP32 accumulation
order, but it needs a full final-hidden dump compare before it can become part
of the default optimized path.
```

## 2026-04-30 10:28 CEST - Final-Hidden Dumps, Attention-Out Bugfix, and b/a Fusion

Dev-tool additions:

```text
Added final hidden dump for the DeltaNet+FFN stack:
  CTOX_QWEN35_DELTA_STACK_FINAL_DUMP=/tmp/file.bin

Added generic FP16 dump compare:
  compare_half_dump <baseline.bin> <candidate.bin> <tokens> <width>

The tool reports:
  mismatch_count
  mean_abs_error
  rms_error
  max_abs_error
  checksum_delta
  first mismatch token/col
```

Delta-Out-MMA16 correctness check:

```text
p512, 1 DeltaNet+FFN layer:
  mismatch_count    22,596 / 524,288
  mean_abs_error    0.000003281
  rms_error         0.000027441
  max_abs_error     0.000976562
  checksum_delta    0.009828806

p512, 3 DeltaNet+FFN layers:
  mismatch_count    318,536 / 524,288
  mean_abs_error    0.000229833
  rms_error         0.000366526
  max_abs_error     0.015625000
  checksum_delta   -0.236306906

p512, 18 DeltaNet+FFN layers:
  mismatch_count    467,606 / 524,288
  mean_abs_error    0.001866446
  rms_error         0.002441499
  max_abs_error     0.035156250
  checksum_delta    7.602805197
```

Interpretation:

```text
Delta-Out-MMA16 is not bit-identical, but the error starts at normal FP16
rounding scale and grows through repeated layers. This is consistent with
different FP32 accumulation order. It still needs end-to-end greedy/logit
validation before production decode, but it is acceptable for the optimized
research candidate path.
```

Attention-Out argument bug:

```text
Found a real correctness bug in the attention core output projection.

The out-projection kernel expects:
  buffer(3) tokens
  buffer(4) rows
  buffer(5) row_tile
  buffer(6) col_tile
  buffer(7) n_col_tiles

The attention core was passing:
  buffer(3) hidden
  buffer(4) row_tile
  buffer(5) col_tile
  buffer(6) n_col_tiles

This made the attention out-projection benchmark output unreliable. Fixed the
argument order and re-measured.
```

Attention core after fix:

```text
p512:
  project MMA8 + out tok4      0.017106959 s
  project MMA16 + out tok4     0.014705291 s
  project MMA16 + out MMA16    0.012567166 s
  checksum16                  -0.870190 / -0.870191

p1024:
  project MMA8 + out tok4      0.047418875 s
  project MMA16 + out tok4     0.042013083 s
  project MMA16 + out MMA16    0.037027166 s
  checksum16                  -0.870190 / -0.870191
```

Decision:

```text
Attention Project-MMA16 is now the default inside attention core when
tokens % 16 == 0. It can be forced back with:
  CTOX_QWEN35_ATTENTION_PROJECT_MMA8=1

Attention out-projection uses Delta-Out-MMA16 when the optimized candidate env
is active:
  CTOX_QWEN35_DELTA_OUT_MMA16=1

All MMA16 out paths now dispatch with 32 threads and guard against invalid
token tails.
```

Updated memory forensics with optimized candidate path:

```text
p512:
  delta18+ffn       0.358743 s
  attention.core    0.012374 s
  attention.ffn     0.008847 s
  full estimate     0.486 s, 1053.35 tok/s

p1024:
  delta18+ffn       0.715450 s
  attention.core    0.037461 s
  attention.ffn     0.019160 s
  full estimate     1.055 s, 970.45 tok/s

p2048:
  delta18+ffn       1.431813 s
  attention.core    0.126765 s
  attention.ffn     0.040238 s
  full estimate     2.434 s, 841.47 tok/s

p4096:
  delta18+ffn       2.877491 s
  attention.core    0.464761 s
  attention.ffn     0.069495 s
  full estimate     6.083 s, 673.35 tok/s
```

Comparison:

```text
llama.cpp pp4096 reference: 2852.70 tok/s
current optimized candidate:  673.35 tok/s

The gap is still about 4.2x at p4096.
```

b/a projection + beta/decay fusion:

```text
Implemented:
  qwen35_08b_prefill_deltanet_ba_project_activate_tok4_h16_k1024

First placement was wrong:
  It read q_half after q_half had already been overwritten by Q from the
  qkv split. This produced wrong checksum:
    fused wrong checksum16: -0.214172
    baseline checksum16:    -0.910950

Fixed placement:
  Run fused b/a projection immediately after the QKV/Z projections and before
  conv/split overwrites q_half.
```

Measured 18x DeltaNet+FFN stack:

```text
p512:
  fused b/a activate      0.356341500 s, checksum16 -0.910950
  old b/a + activation    0.357302708 s, checksum16 -0.910950

p1024:
  fused b/a activate      0.717659625 s, checksum16 -0.910950
  old b/a + activation    0.719507208 s, checksum16 -0.910950
```

Decision:

```text
The fused b/a activation path is correct but only marginally faster. Keep it
as a small default micro-optimization with opt-out:
  CTOX_QWEN35_DELTA_BA_FUSED_NO=1

This is not the missing mega-kernel lever. The remaining dominant issue is
still large-linear weight reuse and the unfused block structure around the
DeltaNet/FFN stack.
```

## 2026-04-30 10:33 CEST - GateUp-MMA16 and Forensics Cleanup

Implemented GateUp-MMA16:

```text
New kernel:
  qwen35_08b_prefill_ffn_gate_up_mma16x8_normed_fp16_tiled_k1024_i3584

Selection:
  CTOX_QWEN35_FFN_GATE_UP_MMA16=1 -> token tile 16
  CTOX_QWEN35_FFN_GATE_UP_MMA=1   -> old token tile 8

Integrated into:
  standalone FFN gate/up benchmark
  full FFN block benchmark
  optimized 18x DeltaNet+FFN stack
  memory_forensics optimized candidate path
```

Measured isolated FFN block:

```text
p512:
  GateUp-MMA8 + Down-MMA16     0.009022083 s
  GateUp-MMA16 + Down-MMA16    0.006264709 s
  checksum16                  0.398366

p1024:
  GateUp-MMA8 + Down-MMA16     0.017569041 s
  GateUp-MMA16 + Down-MMA16    0.012794292 s
```

Measured 18x DeltaNet+FFN stack:

```text
p512:
  GateUp-MMA8     0.336717167 s
  GateUp-MMA16    0.293108458 s
  checksum16     -0.910950

p1024:
  GateUp-MMA8     0.671585750 s
  GateUp-MMA16    0.584885791 s
  checksum16     -0.910950
```

Updated memory forensics after GateUp-MMA16:

```text
p512:
  full estimate     0.397 s, 1288.40 tok/s

p1024:
  full estimate     0.868 s, 1180.19 tok/s

p2048:
  full estimate     2.010 s, 1018.85 tok/s

p4096:
  delta18+ffn       2.328988 s
  attention.core    0.419814 s
  attention.ffn     0.050939 s
  full estimate     5.154 s, 794.80 tok/s
```

Comparison:

```text
llama.cpp pp4096 reference: 2852.70 tok/s
current optimized candidate:  794.80 tok/s

The gap is still about 3.6x at p4096.
```

Forensics cleanup:

```text
cache_model.rs now describes the active MMA16 planning model instead of the
older tok4/tok8 model.

bench_metalpack_prefill_gate_up_mma_compare now selects the same MMA8/MMA16
kernel family as the production path instead of being hardwired to MMA8. This
prevents stale dev-tool measurements from hiding or inventing regressions.
```

Decision:

```text
GateUp-MMA16 is accepted for the optimized research candidate. It is a real
speedup, but not enough to beat llama.cpp. The next macro target remains
DeltaNet+FFN block-level fusion and reduction of scratch/dispatch traffic,
because delta18+ffn is still the dominant p4096 component.
```

## 2026-04-30 10:47 CEST - Rejected Scan + Gated RMSNorm Fusion

Implemented an opt-in fused DeltaNet scan + gated RMSNorm kernel:

```text
New kernel:
  qwen35_08b_prefill_deltanet_scan_gated_norm_f32_state_tok_h16d128

Flag:
  CTOX_QWEN35_DELTA_SCAN_GATED_NORM=1

Intent:
  remove the global F32 delta write/read between:
    qwen35_08b_prefill_deltanet_scan_f32_state_tok_h16d128
    qwen35_08b_prefill_deltanet_gated_rmsnorm_tok_h16d128_f32_to_fp16
```

Correctness:

```text
p512, 1 DeltaNet+FFN layer, optimized candidate otherwise unchanged:
  mismatch_count    0 / 524,288
  mean_abs_error    0.000000000
  rms_error         0.000000000
  max_abs_error     0.000000000
  checksum_delta    0.000000000
```

Performance:

```text
p512, 18 DeltaNet+FFN layers:
  baseline scan + gated norm    0.293036916 s
  fused scan_gated_norm         0.298114208 s

p1024, 18 DeltaNet+FFN layers:
  baseline scan + gated norm    0.584950000 s
  fused scan_gated_norm         0.595442875 s
```

Decision:

```text
Rejected as default. Keep the kernel behind the opt-in flag for future
forensics, but do not include it in the optimized candidate path.

Reason:
  The fusion removes a global F32 scratch write/read, but it moves the gated
  RMSNorm reduction into the token-sequential scan threadgroup. The separate
  gated-norm dispatch has much more token-level parallelism, so the fused path
  loses more parallelism than it saves in memory traffic.

Lesson:
  "Less memory movement" is not automatically faster when the fusion changes
  the parallelism shape. The forensics model must distinguish scratch-traffic
  savings from lost parallel work.
```

## 2026-04-30 10:58 CEST - Cache Forensics Byte Buckets

Updated `memory_forensics` to stop reporting `model_bytes / unique_weights` as
if it were a cache hit metric.

New per-scope fields:

```text
unique_weight_bytes
weight_group_stream_bytes
logical_operand_weight_bytes
reuse_opportunity_bytes/rate
non_weight_bytes
token tile summary
token group summary
max tail underfill
```

Reason:

```text
The previous `modeled/unique` number mixed non-weight traffic into the
weight-reuse ratio. That overstated cache conclusions after MMA16 tiling.

The new metric separates:
  logical per-token weight operands
  token-tiled weight stream groups
  non-weight activation/state/attention traffic
  inferred DRAM-equivalent bytes from measured time and assumed bandwidth
```

Validated p1024 optimized candidate forensics:

```text
delta18+ffn:
  median                         563.486 ms
  model_bytes                    102.46 GiB
  dram_equiv at 100 GB/s          52.48 GiB
  unique_weights                 739.12 MiB
  weight_stream/unique            64.0x
  weights_stream                  46.41 GiB
  logical_weight_operands        739.12 GiB
  reuse_opportunity              692.72 GiB / 93.7%
  non_weight                      56.06 GiB

attention.core:
  median                          33.044 ms
  model_bytes                      4.91 GiB
  unique_weights                  14.00 MiB
  weight_stream/unique            64.0x
  weights_stream                 896.00 MiB
  non_weight                       4.04 GiB

attention.ffn:
  median                          12.304 ms
  model_bytes                      1.33 GiB
  unique_weights                  21.00 MiB
  weight_stream/unique            64.0x
  weights_stream                   1.31 GiB
  non_weight                      15.00 MiB
```

Important interpretation:

```text
The tool still does not have hardware L2 miss counters. It now reports
hardware-cache conclusions as lower-bound inference only, and reports token
tiling reuse separately from cache hits.

For p1024, delta18+ffn is still split roughly between tiled weight stream and
non-weight state/scratch traffic. That explains why pure matmul kernel work is
no longer sufficient.
```

## 2026-04-30 11:06 CEST - Research Check: Gated DeltaNet and Apple Memory

External research checked:

```text
Gated Delta Networks: Improving Mamba2 with Delta Rule
  NVIDIA / ICLR 2025
  https://research.nvidia.com/publication/2025-04_gated-delta-networks-improving-mamba2-delta-rule

A Persistent-State Dataflow Accelerator for Memory-Bound Linear Attention Decode on FPGA
  arXiv 2603.05931, March 2026
  https://arxiv.org/abs/2603.05931

Open-TQ-Metal: Fused Compressed-Domain Attention for Long-Context LLM Inference on Apple Silicon
  arXiv 2604.16957, April 2026
  https://arxiv.org/abs/2604.16957

Profiling Large Language Model Inference on Apple Silicon: A Quantization Perspective
  arXiv 2508.08531, August 2025
  https://arxiv.org/abs/2508.08531

ONNX Linear Attention / Recurrent State-Update operator discussion
  https://github.com/onnx/onnx/issues/7689
```

Implications for this implementation:

```text
1. Gated DeltaNet is the right optimization target for Qwen3.5-like hybrids.
   The literature frames the mechanism as useful exactly because it replaces
   growing KV state with fixed recurrent state, but decode becomes dominated
   by how often that state is moved.

2. The FPGA paper states the hard version of the problem: batch-1 Gated
   DeltaNet decode is memory-bound on GPUs when the recurrent state must be
   round-tripped through external memory. The accelerator wins by keeping the
   recurrent state on chip and pipelining the recurrence.

3. The ONNX operator discussion matches our local failure modes: decomposing
   the recurrent update into many kernel launches and global intermediates is
   structurally uncompetitive. Our next real decode target should be a
   persistent-state DeltaNet decode kernel, not just more prefill matmul
   tuning.

4. Open-TQ-Metal is relevant for the six full-attention layers and long
   contexts. It supports the plan to treat long-context attention as fused
   compressed-domain KV work, separate from the DeltaNet recurrence work.

5. The Apple Silicon quantization profiling paper reinforces the current
   caution: lower precision is not automatically faster; dequantization,
   cache residency, and memory bandwidth need separate measurement.
```

## 2026-04-30 11:21 CEST - DeltaNet Scan Rowcache

Implemented an opt-in row-cache scan kernel:

```text
New kernel:
  qwen35_08b_prefill_deltanet_scan_rowcache_f32_state_tok_h16d128

Flag:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1

Contract:
  Same buffers as the baseline scan:
    q, k, v, beta, decay, recurrent_state, out_tokens, tokens

Intent:
  Each thread owns one `(head,row)` state row.
  It loads 128 float state values once before the token loop,
  updates that row locally for all tokens,
  writes the final row state once after the token loop.
```

Correctness:

```text
Isolated scan p512:
  max_abs_error_out_validate8      0.000000010
  max_abs_error_state_validate8    0.000000015
  checksum32                       0.038058

18x DeltaNet+FFN stack:
  checksum16 unchanged             -0.910950
```

Performance:

```text
Isolated scan p512:
  baseline scan                    0.003679042 s
  rowcache scan                    0.003541250 s

18x DeltaNet+FFN stack p512:
  baseline optimized candidate     0.282215375 s
  rowcache candidate               0.272547875 s

18x DeltaNet+FFN stack p1024:
  baseline optimized candidate     0.563256541 s
  rowcache candidate               0.543946416 s
```

Forensics after accepting rowcache into the optimized candidate:

```text
p1024:
  delta18+ffn median               543.313 ms
  delta18+ffn model_bytes           48.50 GiB
  delta18+ffn floor@100GB/s        520.766 ms
  delta18+ffn ratio                  1.04x
  full estimate                      0.816 s, 1255.66 tok/s

p4096:
  delta18+ffn median              2167.846 ms
  delta18+ffn model_bytes          193.88 GiB
  delta18+ffn floor@100GB/s       2081.783 ms
  delta18+ffn ratio                  1.04x
  full estimate                      4.915 s, 833.34 tok/s
```

Comparison:

```text
llama.cpp pp4096 reference:        2852.70 tok/s
current optimized candidate p4096:  833.34 tok/s
gap:                               ~3.42x slower
```

Decision:

```text
Accept rowcache into the optimized research candidate path for prefill
forensics. Keep it env-gated in the benchmark/runtime code because it uses:
  thread float row_state[128]

That is 512 bytes private storage per thread. It is likely to spill or reduce
occupancy on some Apple GPUs. On the current Mac, the measured full-stack
effect is positive, and the byte model now treats recurrent-state traffic as
initial read + final write instead of per-token read/write.
```

Lesson:

```text
This is the first optimization in this phase that directly attacks recurrent
state movement without destroying scan parallelism. It also changes the
forensics conclusion: Delta18+FFN prefill is now close to the modeled
weight-stream floor. The next large gap is no longer "one more DeltaNet
scratch fusion"; it is persistent-state decode and long-context attention.
```

## 2026-04-30 11:34 CEST - Decode DeltaNet Step Rowcache

Implemented a single-token DeltaNet step row-cache kernel:

```text
New kernel:
  qwen35_08b_deltanet_step_rowcache_f32_state

Flag:
  CTOX_QWEN35_DECODE_DELTA_ROWCACHE=1

Contract:
  Same buffers as baseline qwen35_08b_deltanet_step_f32_state.

Intent:
  Apply the same row-local recurrent-state idea to decode:
    read one 128-wide state row
    update locally for one token
    write the final row once
```

Validation:

```text
Updated bench_deltanet_stability so it can select the rowcache kernel and
report per-step timing on a normalized, non-exploding trace.

100-step normalized trace:
  baseline kernel:
    median_step_s           0.000237208
    p95_step_s              0.000258083
    max_abs_error_out       0.000000000
    max_abs_error_state     0.000000001

  rowcache kernel:
    median_step_s           0.000222667
    p95_step_s              0.000257833
    max_abs_error_out       0.000000000
    max_abs_error_state     0.000000000
```

Rejected measurement:

```text
bench_deltanet with 50 repeated unnormalized synthetic steps is not a valid
correctness/performance source. The baseline itself diverges to enormous
errors and huge checksums, so those timings are not used for decisions.
```

Decision:

```text
Keep decode rowcache as an opt-in experimental kernel. It is correct on the
normalized stability trace and slightly faster there, but it still uses a
large private array:
  thread float row_state[128]

It should not be promoted into full decode until the real BF16/F16 metalpack
loader mismatch in bench_metalpack_decode_deltanet is fixed and the full
decode path can be measured with real model weights.
```

## 2026-04-30 11:49 CEST - Real BF16 Metalpack Decode Unblocked

Fixed `bench_metalpack_decode_deltanet` for the real Qwen3.5 metalpack:

```text
Problem:
  manifest dtype is BF16 for embedding and DeltaNet weights
  layout is fp16_row_tiled
  writer already converts BF16 source data to FP16 packed bytes

Fix:
  accept F16/BF16 manifest dtype for fp16_row_tiled decode weights
  default layer prefix changed to:
    model.language_model.layers.0
```

Real single-DeltaNet-layer + LM-head decode:

```text
baseline:
  median_s       0.008089916
  next_token     3599
  score          55837.890625

decode rowcache:
  median_s       0.006842708
  next_token     3599
  score          55837.890625
```

Real 24-layer layered decode, one token:

```text
baseline:
  median_s       0.019940042
  next_token     198
  score          10.721777

decode rowcache:
  median_s       0.017825083
  next_token     198
  score          10.721777
```

Real 24-layer layered decode, longer output:

```text
64 decode tokens:
  baseline       1.126606041 s  -> 56.81 tok/s
  rowcache       1.116511667 s  -> 57.32 tok/s

512 decode tokens:
  baseline       9.592729500 s  -> 53.37 tok/s
  rowcache       9.576664000 s  -> 53.46 tok/s
```

Comparison:

```text
llama.cpp standalone tg512 reference: 44.77 tok/s
current layered decode rowcache:       53.46 tok/s
```

Decision:

```text
Decode-only is now above the llama.cpp standalone decode reference on this
probe. This does not mean the project is finished: prefill is still much
slower than llama.cpp pp4096/pp128k. Continue on prefill, especially full
attention and the remaining DeltaNet/FFN weight-stream path.
```

## 2026-04-30 12:18 CEST - Attention qblk2 SIMD-Reduce and Forensics Fix

Implemented a new prefill attention variant:

```text
kernel:
  qwen35_08b_prefill_attention_causal_qblk2_simdreduce_gqa8_kv2_d256_to_fp16

env:
  CTOX_QWEN35_ATTENTION_QBLK2_SIMDREDUCE=1

idea:
  process two adjacent query tokens per threadgroup
  share each K/V stream load across the two queries
  use SIMD-group reductions instead of threadgroup-tree reductions
```

Corrected a benchmark-selection bug found by static subagent review:

```text
Bug:
  old qblk flags could select query_block=2/4 but still choose the default
  single-query SIMD-reduce pipeline, because SIMD-reduce was tested before
  the explicit qblk kernels.

Fix:
  explicit attention variant flags are now mutually exclusive
  pipeline selection checks qblk2_simd/qblk2x512/qblk4/qblk2 before default
  SIMD-reduce

Impact:
  previous old-qblk comparison rows are discarded
  qblk2_simd rows remain valid because that variant already had priority
```

p1024 corrected sweep:

```text
baseline       79.126 ms
qblk2          81.909 ms
simdreduce     40.430 ms
qblk2_simd     38.563 ms
qblk4          89.257 ms
qblk2x512     101.652 ms
partial_qblk2 173.223 ms

checksum for all variants: -0.870190
decision: qblk2_simd is the best tested attention core variant at p1024
```

p4096 memory forensics after enabling qblk2_simd:

```text
iterations: 3
sustained bandwidth assumption: 100 GB/s

delta18+ffn:
  median_ms      2368.090
  model_bytes    193.88 GiB
  floor_ms       2081.788
  ratio          1.14x
  eff_GB/s       87.91

attention.core:
  median_ms      416.910
  model_bytes    35.65 GiB
  floor_ms       382.772
  ratio          1.09x
  eff_GB/s       91.81

attention.ffn:
  median_ms      53.415
  model_bytes    5.31 GiB
  floor_ms       56.999
  ratio          0.94x
  eff_GB/s       106.71

full_prefill_estimate_current_kernels:
  5.190 s
  789.20 tok/s
```

Comparison to fixed llama.cpp reference:

```text
llama.cpp pp4096 reference:          2852.70 tok/s
current modeled full prefill p4096:   789.20 tok/s
gap:                                  3.62x slower
```

Forensics conclusion:

```text
No accepted claim of hardware L2/cache-miss counters yet.
This Mac path exposes GPUTimestamp only in the current tooling, so cache
miss rows are still inferred from byte floors and DRAM-equivalent bytes.

The qblk2 SIMD attention core is close to its modeled byte floor. Delta18+FFN
is also close to its modeled weight-stream floor. The remaining prefill gap is
therefore structural: too much effective weight/KV streaming, not merely one
bad reduction kernel.
```

Next accepted optimization direction:

```text
1. Reduce DeltaNet/FFN weight streaming beyond token-tile=16.
2. Investigate compressed/block attention for long prefill, because qblk2
   only halves part of the K/V reuse problem and O(T^2) traffic remains.
3. Improve hardware-counter tooling where possible; if Metal/Xcode counters
   are not programmatically exposed, keep the byte-floor forensic path as the
   portable baseline and document it clearly.
```

## 2026-04-30 12:37 CEST - Ideal-Reuse Cache Forensics Added

Ran the local Metal counter inventory:

```text
metal counter sampling support:
  stage: true
  draw: false
  dispatch: false
  tile_dispatch: false
  blit: false

counter_sets:
  timestamp counters=1
  GPUTimestamp
```

Conclusion:

```text
The current programmatic Metal path still cannot report hardware L2/cache-miss
counters. Do not claim measured cache-hit/miss counters. Continue using
timestamp timings plus explicit byte models.
```

Forensics tool update:

```text
memory_forensics now prints two floors:

1. current algorithmic stream model
   - what the current kernels are expected to move because of token tiles,
     repeated weight groups, and qblk2 attention KV streaming

2. ideal_reuse_floor
   - what would remain if the op achieved ideal cache/persistent reuse of
     weights/KV inside the prefill block

The difference is printed as:
  stream/cache-miss budget above ideal
```

p4096 single-run example after the tool update:

```text
delta18+ffn:
  model_bytes                         193.89 GiB
  ideal_reuse_floor                     8.98 GiB
  stream/cache-miss budget above ideal 184.91 GiB (95.4%)

attention.core:
  model_bytes                          35.65 GiB
  ideal_reuse_floor                   174.00 MiB
  stream/cache-miss budget above ideal  35.48 GiB (99.5%)

attention.ffn:
  model_bytes                           5.31 GiB
  ideal_reuse_floor                    81.00 MiB
  stream/cache-miss budget above ideal   5.23 GiB (98.5%)
```

Interpretation:

```text
The current kernels are close to their own streaming byte model, but far from
an ideal reuse floor. This explains why simply removing local reduction
overhead cannot close the llama.cpp prefill gap.

The next large prefill optimization must change data reuse:
  - persistent/tile-major schedules for FFN and DeltaNet projections
  - larger effective token blocks without register/local-memory collapse
  - compressed/block attention or a schedule that keeps KV resident instead
    of replaying it as O(T^2) memory traffic
```

## 2026-04-30 13:02 CEST - FFN MMA32 Weight-Reuse Candidate

Implemented two experimental token_tile=32 MMA kernels:

```text
qwen35_08b_prefill_ffn_gate_up_mma32x8_normed_fp16_tiled_k1024_i3584
qwen35_08b_prefill_down_mma32x8_fp16_tiled_k3584_f32

env:
  CTOX_QWEN35_FFN_GATE_UP_MMA32=1
  CTOX_QWEN35_DOWN_MMA32=1
```

Rejected measurement:

```text
An initial p4096 MMA16-vs-MMA32 comparison was accidentally run in parallel.
Those numbers are discarded and not used.
```

Serial FFN block measurements:

```text
p1024:
  MMA16 median_s   0.012643833
  MMA32 median_s   0.009063125
  checksum16       0.398366 both

p4096:
  MMA16 median_s   0.052970000
  MMA32 median_s   0.040815667
  checksum16       0.398366 both
```

Decision:

```text
Accept MMA32 for FFN Gate/Up and Down in optimized forensics.
It cuts FFN weight groups from 256 to 128 at p4096 and wins despite higher
register pressure.
```

p4096 memory forensics with MMA32 FFN:

```text
iterations: 3

delta18+ffn:
  median_ms      2124.487
  model_bytes    146.63 GiB
  floor_ms       1574.458
  ratio          1.35x
  eff_GB/s       74.11

attention.core:
  median_ms       420.577
  model_bytes      35.65 GiB
  floor_ms        382.772
  ratio           1.10x
  eff_GB/s        91.01

attention.ffn:
  median_ms        40.949
  model_bytes       2.68 GiB
  floor_ms         28.816
  ratio            1.42x
  eff_GB/s         70.37

full_prefill_estimate_current_kernels:
  4.894 s
  837.00 tok/s
```

Static review correction after this entry:

```text
The new floor must not be read as a measured hardware cache-hit floor.
It is now labeled:
  ideal/persistent reuse floor

Also corrected attention.core forensic bytes to include the hidden input and
normed hidden streams that bench.rs already counted.

Corrected p4096 single-run sanity output after the fix:
  attention.core model_bytes              35.66 GiB
  attention.core ideal/persistent floor  190.00 MiB
  full_prefill_estimate_current_kernels  4.910 s / 834.14 tok/s
```

Comparison:

```text
previous p4096 forensics estimate:   789.20 tok/s
current p4096 forensics estimate:    837.00 tok/s
llama.cpp pp4096 reference:         2852.70 tok/s
remaining gap:                        3.41x slower
```

Interpretation:

```text
This is a real improvement against repeated FFN weight streaming, but still
far short of the reference. The largest remaining model-byte buckets are now:
  - DeltaNet/FFN stack: 146.63 GiB
  - attention core:     35.65 GiB

Next candidates:
  - MMA32 for DeltaNet QKV/Z and Delta-Out where register pressure allows it
  - tile-major/persistent scheduling to reuse weights beyond token_tile=32
  - attention schedule that reduces qblk2 O(T^2) KV replay
```

## 2026-04-30 13:42 CEST - DeltaNet MMA32 Projection and Out-Proj

Implemented additional MMA32 candidates:

```text
qwen35_08b_prefill_matmul_mma32x8_fp16_tiled_k1024_f32
qwen35_08b_prefill_deltanet_out_mma32x8_fp16_tiled_k2048_f32

env:
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32=1
  CTOX_QWEN35_DELTA_OUT_MMA32=1
```

Also fixed the full-stack Delta-Out MMA activation:

```text
Problem:
  CTOX_QWEN35_DELTA_OUT_MMA32 was selectable by the kernel helper but was not
  considered by prefill_deltanet_out_mma_enabled().

Effect:
  Early full-stack Delta-Out-MMA32 measurements were actually fallback residual
  path measurements with misleading token-tile metadata.

Fix:
  prefill_deltanet_out_mma_enabled() now recognizes MMA32
  full-stack delta_out_mma now validates against the selected out_token_tile
```

Dev-tool improvement:

```text
bench_metalpack_prefill_delta_out now prints checksum_sparse in addition to
checksum16, because checksum16 only covered the beginning of the output and
was too weak for kernel candidate validation.
```

Serial isolated Delta-Out p4096:

```text
MMA16:
  median_s          0.013106875
  checksum16        0.985208
  checksum_sparse   9.997503

MMA32:
  median_s          0.009770459
  checksum16        0.985208
  checksum_sparse   9.997503
```

Serial Delta18+FFN stack p4096:

```text
previous accepted stack:
  QKV/Z auto-MMA16 + Delta-Out-MMA16 + FFN-MMA32
  median_s          2.247957584
  checksum16       -0.910950

Delta-Out-MMA32 only:
  median_s          2.186139292
  checksum16       -0.910950

QKV/Z-MMA32 only:
  median_s          1.933641333
  checksum16       -0.910950

QKV/Z-MMA32 + Delta-Out-MMA32:
  median_s          1.886456125
  checksum16       -0.910950
```

Decision:

```text
Accept QKV/Z-MMA32 and Delta-Out-MMA32 in the optimized forensics path.
The largest gain comes from QKV/Z, not Delta-Out.
```

p4096 memory forensics after DeltaNet MMA32:

```text
iterations: 3

delta18+ffn:
  median_ms      1890.551
  model_bytes    101.63 GiB
  floor_ms       1091.226
  ratio          1.73x
  eff_GB/s       57.72

attention.core:
  median_ms       447.822
  model_bytes      35.66 GiB
  floor_ms        382.940
  ratio           1.17x

attention.ffn:
  median_ms        40.530
  model_bytes       2.68 GiB
  floor_ms         28.817
  ratio            1.41x

full_prefill_estimate_current_kernels:
  4.821 s
  849.68 tok/s
```

Comparison:

```text
best prior p4096 estimate:     ~837 tok/s
current p4096 estimate:        ~850 tok/s
llama.cpp pp4096 reference:    2852.70 tok/s
remaining gap:                 ~3.36x slower
```

Interpretation:

```text
MMA32 reduces modeled Delta18+FFN bytes sharply:
  before DeltaNet MMA32: 146.63 GiB
  after DeltaNet MMA32:  101.63 GiB

Measured runtime improves, but effective GB/s falls because register pressure
and dispatch/local overhead become more visible. The next step is not simply
MMA64; it needs a schedule that improves reuse without collapsing occupancy.
```

## 2026-04-30 14:20 CEST - Attention qblk4 Batched SIMD-Reduce

Implemented two Attention prefill candidates:

```text
qwen35_08b_prefill_attention_causal_qblk4_simdreduce_gqa8_kv2_d256_to_fp16
qwen35_08b_prefill_attention_causal_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16

env:
  CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE=1
  CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

Rejected candidate:

```text
qblk4_simd was correct but still slower than qblk2_simd.
Reason: it shared K/V across four queries, but still performed reduction
barriers once per query.

p1024:
  qblk2_simd    52.589 ms
  qblk4_simd    54.292 ms
```

Accepted candidate:

```text
qblk4_batch computes all four query dot products per key, stores all SIMD
partials, then performs one batched cross-SIMD reduction phase. This cuts
barrier phases per key from query_block-scaled to roughly constant.
```

Corrected p1024 sweep:

```text
baseline        98.601 ms
qblk2          106.327 ms
simdreduce      53.058 ms
qblk2_simd      52.589 ms
qblk4_simd      54.292 ms
qblk4_batch     47.973 ms
qblk4          108.985 ms
qblk2x512      126.400 ms
partial_qblk2  231.811 ms

all checksums: -0.870190
```

Corrected p4096 sweep:

```text
baseline       1136.323 ms
qblk2          1155.966 ms
simdreduce      550.125 ms
qblk2_simd      487.415 ms
qblk4_simd      506.450 ms
qblk4_batch     433.838 ms
qblk4          1191.808 ms
qblk2x512      1491.204 ms
partial_qblk2  3411.221 ms

all checksums: -0.870190
```

Decision:

```text
Accept qblk4_batch as the optimized Attention-Core variant.
```

p4096 memory forensics with qblk4_batch:

```text
iterations: 3

delta18+ffn:
  median_ms      1897.855
  model_bytes    101.63 GiB

attention.core:
  median_ms       388.943
  model_bytes      19.66 GiB
  non_weight       16.16 GiB
  attention groups 1024@4

attention.ffn:
  median_ms        43.453
  model_bytes       2.68 GiB

full_prefill_estimate_current_kernels:
  4.492 s
  911.80 tok/s
```

Comparison:

```text
previous p4096 estimate:      849.68 tok/s
current p4096 estimate:       911.80 tok/s
llama.cpp pp4096 reference:  2852.70 tok/s
remaining gap:                 3.13x slower
```

Interpretation:

```text
This is the first Attention change that actually reduces the macro p4096
estimate. The model bytes for one Attention-Core layer dropped from about
35.66 GiB to 19.66 GiB, but runtime ratio rose because the kernel is now more
barrier/register/occupancy-limited than pure memory-bandwidth-limited.

Next Attention direction:
  - reduce qblk4_batch per-key barrier overhead further
  - test qblk8 only if the batched barrier structure is preserved
  - investigate block/compressed attention because O(T^2) replay still remains
```

## 2026-04-30 11:36 CEST - Rejected qblk8, Rowcache+GatedNorm, and Cross-QHead Attention Candidates

Goal:

```text
Continue the Qwen3.5-0.8B Metal mega-pipeline work with strict serial
benchmarks and stronger cache/memory forensics. No subagent ran tests or
benchmarks; subagents only did static code inspection.
```

Validation:

```text
cargo fmt && cargo test
result: pass

cargo fmt && cargo build --release --bins
result: pass
```

Forensics tool update:

```text
memory_forensics now separates:
  - weight-reuse floor
  - optional persistent/cache-resident floor
  - Attention KV per-query logical bytes
  - qblk logical bytes
  - qblk-saved bytes
  - cross-QHead reuse opportunity
  - unique KV cache-resident floor

Important: this Mac exposes GPUTimestamp only, so these are inferred byte
floors and DRAM-equivalent diagnostics, not hardware L2/cache-miss counters.
```

p4096 forensics after the reporting fix:

```text
iterations: 3
sustained BW assumption: 100 GB/s

delta18+ffn:
  median_ms                 1917.304
  model_bytes               101.64 GiB
  weight-reuse floor          8.98 GiB
  stream budget above floor  92.65 GiB

attention.core:
  median_ms                  385.411
  model_bytes                19.66 GiB
  weight-reuse floor         16.18 GiB
  persistent/cache floor    190.00 MiB

attention.ffn:
  median_ms                   39.003
  model_bytes                  2.68 GiB
  weight-reuse floor          81.00 MiB

full_prefill_estimate_current_kernels:
  4.464 s
  917.61 tok/s
```

Attention KV forensic snapshot at p4096:

```text
per_query_logical KV:       64.02 GiB
qblk4 logical KV:           16.02 GiB
qblk4 saved:                48.00 GiB
cross-QHead reuse possible: 12.01 GiB
unique KV cache floor:       8.00 MiB
cache residency gap:         4.00 GiB
```

Candidate: qblk8 batched SIMD-reduce Attention

Implementation:

```text
Added opt-in kernel:
  qwen35_08b_prefill_attention_causal_qblk8_simdreduce_batch_gqa8_kv2_d256_to_fp16

Flag:
  CTOX_QWEN35_ATTENTION_QBLK8_SIMDREDUCE_BATCH=1
```

Measurements:

```text
p1024, iterations=3:
  qblk4_batch: 49.638 ms in the corrected sweep
  qblk8_batch: 51.718 ms

p4096, iterations=3:
  qblk4_batch: 460.599 ms
  qblk8_batch: 482.253 ms

checksums:
  all -0.870190
```

Decision:

```text
Reject qblk8 for the long-context path. It reduces modeled KV loads but
register/barrier pressure dominates.
```

Candidate: Delta scan rowcache + gated norm fusion

Implementation:

```text
Added opt-in kernel:
  qwen35_08b_prefill_deltanet_scan_rowcache_gated_norm_f32_state_tok_h16d128

Enable with:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
  CTOX_QWEN35_DELTA_SCAN_GATED_NORM=1
```

Measurements:

```text
p4096 Delta18+FFN stack, iterations=3:

existing rowcache path:
  median_s: 1.897848792
  checksum16: -0.910950

rowcache+gated_norm fusion:
  median_s: 1.973894166
  checksum16: -0.910950
```

Decision:

```text
Reject as default. It is correct, but the extra per-token RMS reduction inside
the state scan is slower than writing/reading the Delta F32 scratch and running
the separate gated norm kernel.
```

Candidate: 2QHeads x qblk4 batched SIMD-reduce Attention

Implementation:

```text
Added opt-in kernel:
  qwen35_08b_prefill_attention_causal_qh2_qblk4_simdreduce_batch_gqa8_kv2_d256_to_fp16

Flag:
  CTOX_QWEN35_ATTENTION_QH2_QBLK4_SIMDREDUCE_BATCH=1
```

Measurements:

```text
p1024, iterations=3:
  qh2_qblk4_batch median_s: 0.047674167
  checksum16: -0.870190

p4096, iterations=3:
  qh2_qblk4_batch median_s: 0.505392417
  checksum16: -0.870190

comparison:
  qblk4_batch p4096 recent median_s: 0.460598709
```

Decision:

```text
Reject for the long-context default. It slightly helps p1024 but loses at p4096.
The cross-QHead KV reuse opportunity is real, but this direct two-head fusion
hits register/occupancy pressure before it converts into wall-clock speed.
Keep as opt-in research path.
```

Current accepted default profile remains:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_FFN_GATE_UP_MMA32=1
CTOX_QWEN35_DOWN_MMA32=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

## 2026-04-30 11:49 CEST - Accepted Direct Conv+Split/QK-Norm Fusion

Goal:

```text
Eliminate the large DeltaNet conv_out F32 scratch tensor between causal conv
and split/QK-norm.
```

Implementation:

```text
New opt-in kernels:
  qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok_f32_to_fp16_h16d128
  qwen35_08b_prefill_deltanet_conv_state_update_c6144_k4

Flag:
  CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
```

Design:

```text
The fused conv+split kernel computes Q/K/V conv values directly from qkv_out
and the initial conv_state, applies the same half-rounding before Q/K norms as
the old split kernel, then writes q_half/k_half/v_half directly.

The conv_state update is intentionally a separate dispatch. Updating conv_state
inside the fused per-token dispatch would race with earlier token threadgroups
that still need the initial state.
```

Numerical contract:

```text
checksums stayed identical:
  checksum16: -0.910950
```

p1024 Delta18+FFN stack:

```text
baseline accepted profile:
  median_s: 0.472418500

conv+split fused:
  median_s: 0.446036750

relative gain:
  5.58%
```

p4096 Delta18+FFN stack:

```text
iterations=3

baseline accepted profile:
  median_s: 1.834580708

conv+split fused:
  median_s: 1.790714667

relative gain:
  2.39%
```

p4096 repeat:

```text
iterations=5

baseline accepted profile:
  median_s: 1.827253250

conv+split fused:
  median_s: 1.792085708

relative gain:
  1.92%
```

Decision:

```text
Accept as current default. This is not the ideal token-blocked conv fusion yet:
the direct fused kernel reads up to four qkv_out taps per output instead of
streaming a per-channel history through a token block. Despite that, it is
correct and repeatably faster because it removes the conv_out F32 write/read
and one large split input read path.
```

p4096 memory forensics with conv+split fused default:

```text
delta18+ffn:
  median_ms:    1799.189
  model_bytes:   97.14 GiB
  non_weight:     3.76 GiB

attention.core:
  median_ms:     377.799
  model_bytes:    19.66 GiB

attention.ffn:
  median_ms:      40.927
  model_bytes:     2.68 GiB

full_prefill_estimate_current_kernels:
  4.312 s
  950.01 tok/s
```

Note:

```text
The absolute p4096 forensics run was slower than the previous best residual-in-
MMA run, likely due current runtime/thermal variance. Candidate acceptance is
based on direct back-to-back baseline-vs-fused measurements in the same run
window.
```

Current accepted default profile:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA32=1
CTOX_QWEN35_DOWN_MMA32=1
CTOX_QWEN35_DOWN_MMA32_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

## 2026-04-30 11:56 CEST - Rejected tok4 Conv+Split Fusion as Default

Goal:

```text
Improve the accepted direct conv+split fusion by processing four tokens per
threadgroup, keeping the 4-tap Conv history in registers. This should reduce
qkv_out replay compared with the direct per-token fused kernel.
```

Implementation:

```text
New opt-in kernel:
  qwen35_08b_prefill_deltanet_conv_split_qkv_norm_tok4_f32_to_fp16_h16d128

Flag:
  CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4=1
```

Correctness:

```text
checksum16 stayed identical:
  -0.910950
```

p1024 Delta18+FFN stack:

```text
direct conv+split fused:
  median_s: 0.449989958

tok4 conv+split fused:
  median_s: 0.450563000
```

p4096 Delta18+FFN stack:

```text
direct conv+split fused:
  median_s: 1.817080625

tok4 conv+split fused:
  median_s: 1.814976042
```

Decision:

```text
Reject tok4 as default. It is correct, but the p4096 gain is about 0.1% and
inside normal run noise. The expected QKV-replay reduction is mostly consumed
by extra register/control pressure. Keep it as an opt-in research path.
```

## 2026-04-30 11:43 CEST - Accepted Residual-in-MMA for Delta-Out and FFN-Down

Goal:

```text
Remove the separate residual-add dispatches after Delta-Out MMA32 and FFN-Down
MMA32 without guessing simdgroup matrix element coordinates.
```

Implementation:

```text
New opt-in kernels:
  qwen35_08b_prefill_deltanet_out_mma32x8_residual_fp16_tiled_k2048_f32
  qwen35_08b_prefill_down_mma32x8_residual_fp16_tiled_k3584_f32

Flags:
  CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
  CTOX_QWEN35_DOWN_MMA32_RESIDUAL=1
```

Store strategy:

```text
Load the residual tile as simdgroup_half8x8, add it to the
simdgroup_float8x8 accumulator through thread_elements(), then store the final
half tile directly. This preserves the same matrix layout as simdgroup_store
and avoids manual lane-to-(token,row) mapping.
```

Build note:

```text
First attempt used out_mat.thread_elements() as a loop bound and failed MSL
compilation. Fixed to the same constant count used by existing GateUp MMA
kernels:
  for i in 0..2 thread elements
```

p4096 Delta18+FFN stack measurements:

```text
iterations=3

baseline accepted profile:
  median_s: 1.672791959
  checksum16: -0.910950

Delta-Out residual-in-MMA only:
  median_s: 1.665564125
  checksum16: -0.910950

Down residual-in-MMA only:
  median_s: 1.664338167
  checksum16: -0.910950

both residual-in-MMA:
  median_s: 1.661137333
  checksum16: -0.910950
```

Repeat with 5 iterations:

```text
baseline:
  median_s: 1.671469458

both residual-in-MMA:
  median_s: 1.661656250

relative gain:
  0.59%
```

Decision:

```text
Accept both residual-in-MMA flags into the current default profile. The gain is
small but repeatable, checksum-identical, and removes two dispatches plus F32
scratch write/read per Delta layer.
```

p4096 memory forensics with residual-in-MMA default:

```text
delta18+ffn:
  median_ms:    1662.477
  model_bytes:  100.52 GiB
  non_weight:     7.14 GiB

attention.core:
  median_ms:     393.645
  model_bytes:    19.66 GiB

attention.ffn:
  median_ms:      35.894
  model_bytes:     2.68 GiB

full_prefill_estimate_current_kernels:
  4.240 s
  966.10 tok/s

llama.cpp pp4096 reference:
  2852.70 tok/s

remaining p4096 gap:
  2.95x slower
```

Current accepted default profile:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA32=1
CTOX_QWEN35_DOWN_MMA32=1
CTOX_QWEN35_DOWN_MMA32_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

## 2026-04-30 12:12 CEST - QKV/Z64 and Down64 Weight-Streaming Experiments

Goal:

```text
Reduce repeated prefill weight streaming by increasing selected MMA token tiles
from 32 to 64, while keeping correctness and avoiding register/occupancy cliffs.
```

Implemented opt-in kernels:

```text
qwen35_08b_prefill_matmul_mma64x8_fp16_tiled_k1024_f32
qwen35_08b_prefill_down_mma64x8_fp16_tiled_k3584_f32
qwen35_08b_prefill_down_mma64x8_residual_fp16_tiled_k3584_f32
```

New flags:

```text
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
CTOX_QWEN35_DOWN_MMA64=1
CTOX_QWEN35_DOWN_MMA64_RESIDUAL=1
```

Down64 solo result:

```text
p1024 Delta18+FFN, iterations=3:
  Down32 default: 0.448311542s
  Down64 solo:    0.461300458s
  checksum16:     -0.910950

p4096 Delta18+FFN, iterations=3:
  Down32 default: 1.854963750s
  Down64 solo:    1.772406334s
  checksum16:     -0.910950

p4096 repeat, iterations=5:
  Down32 default: 1.787977500s
  Down64 solo:    1.837541875s
```

Decision:

```text
Reject Down64 as a standalone default. The checksum is correct and the byte model
is attractive, but the wall-clock result is not stable enough by itself.
```

QKV/Z64 result:

```text
p1024 Delta18+FFN, iterations=3:
  QKV/Z64 + Down32: 0.427688833s
  checksum16:       -0.910950

p4096 Delta18+FFN, iterations=5:
  QKV/Z64 + Down32: 1.740720292s
  Down32/QKV32 baseline measured after: 1.790863375s
  checksum16:       -0.910950
```

QKV/Z64 + Down64 combined result:

```text
p4096 Delta18+FFN, iterations=5:
  run 1:       1.698723708s
  run 2:       1.706707166s
  checksum16: -0.910950

p1024 Delta18+FFN, iterations=5:
  0.410841667s
  checksum16: -0.910950
```

Attention FFN block check:

```text
p4096 layer 3 FFN block, iterations=5:
  GateUp32 + Down32: 0.039252583s
  GateUp32 + Down64: 0.038608416s
  checksum16:        0.398366
```

Decision:

```text
Accept QKV/Z64 + Down64 as the current optimized profile. Down64 is not accepted
alone, but it is accepted in the combined profile because repeated p4096 and p1024
Delta18+FFN stack measurements are checksum-identical and faster.
```

p4096 memory forensics with QKV/Z64 + Down64 default:

```text
delta18+ffn:
  median_ms:    1716.783
  model_bytes:    71.26 GiB
  weight_stream:  67.50 GiB
  non_weight:      3.76 GiB
  groups:
    qkvz=64@64
    b/a=1024@4
    out=128@32
    gate_up=128@32
    down=64@64

attention.core:
  median_ms:     378.199
  model_bytes:    19.66 GiB

attention.ffn:
  median_ms:      39.825
  model_bytes:     2.25 GiB
  groups:
    gate_up=128@32
    down=64@64

full_prefill_estimate_current_kernels:
  4.225s
  969.48 tok/s
```

Important measurement caveat:

```text
The Mac still exposes only GPUTimestamp programmatically. Cache-hit/cache-miss
rows in memory_forensics are inferred DRAM-equivalent byte floors, not direct L2
hardware counter measurements.
```

Current accepted default profile:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA32=1
CTOX_QWEN35_DOWN_MMA64=1
CTOX_QWEN35_DOWN_MMA64_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

## 2026-04-30 12:58 CEST - Accepted qh4/qblk1 Attention Core

Goal:

```text
Improve the long-prefill attention core after qblk8 and qh2/qblk4 were rejected.
Use full GQA KV sharing over all 4 Q-heads per KV-head, but avoid the query-block
register pressure that hurt qh2/qblk4.
```

Implemented:

```text
qwen35_08b_prefill_attention_causal_qh4_qblk1_simdreduce_batch_gqa8_kv2_d256_to_fp16
CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH=1
```

## 2026-04-30 13:12 CEST - Online Attention Research Check

User challenge:

```text
Maybe the current attention path is not using the optimal math. Verify with
online paper research, not only local benchmarking.
```

Sources reviewed:

```text
FlashAttention:
  https://arxiv.org/abs/2205.14135

FlashAttention-2:
  https://arxiv.org/abs/2307.08691

Flash-Decoding:
  https://princeton-nlp.github.io/flash-decoding/

PagedAttention / vLLM:
  https://arxiv.org/abs/2309.06180

Open-TQ-Metal:
  https://arxiv.org/abs/2604.16957

GQA:
  https://aclanthology.org/2023.emnlp-main.298.pdf
```

Findings:

```text
1. FlashAttention is exact attention, but its core advantage is IO-aware
   Q/K/V tiling and online softmax, not merely using larger query blocks.

2. FlashAttention-2's key lesson for us is work partitioning:
   split work across thread blocks to raise occupancy, reduce non-matmul FLOPs,
   and reduce shared/threadgroup-memory communication.

3. Flash-Decoding is directly relevant to decode and very long contexts:
   split the KV sequence into chunks, compute local online-softmax partials,
   then combine chunks with log-sum-exp. Our earlier partial attention did this
   too expensively because it wrote full head_dim partial_acc tensors.

4. GQA confirms that sharing K/V across query-head groups is a real architectural
   memory-bandwidth optimization. Our qh4/qblk1 kernel exploits exactly this
   for Qwen3.5's 8 Q heads / 2 KV heads.

5. PagedAttention is mainly memory-management and batching infrastructure. It is
   important for serving and long decode memory layout, but it is not a single
   sequence p4096 prefill speedup by itself.

6. Open-TQ-Metal is the most directly relevant Apple-Silicon paper: it argues
   for fused compressed-domain attention on Metal, i.e. compute attention
   directly over compressed KV without materializing a full dequant matrix.
```

Consequence for CTOX:

```text
The accepted qh4/qblk1 attention kernel is a good GQA reuse step, but it is not
the final attention math. The next real candidates are:

1. Flash-Decoding-style split-K attention for decode and very long contexts:
   stage 1 writes only m/l plus either compressed per-key-block summaries or a
   smaller partial output; stage 2 combines with log-sum-exp. Avoid the earlier
   rejected partial_acc design that wrote full [query, head, block, head_dim].

2. Fused compressed-domain KV attention for long contexts:
   INT8/INT4 KV cache, dot-product directly on compressed K/V in Metal, no global
   dequant tensor. This is probably mandatory for 128K.

3. For prefill, qh4/qblk1 remains current default until a split-K/block attention
   design beats it at p4096 and p16k+ without exploding scratch traffic.
```

Current attention status:

```text
accepted:
  CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH=1

not enough:
  larger query blocks alone
  qblk8
  qh2/qblk4
  old partial_qblk2 with full partial_acc writes

next math-heavy task:
  redesign partial/split-K attention so it writes minimal softmax summaries and
  uses log-sum-exp combination without full partial_acc scratch traffic.
```

## 2026-04-30 13:20 CEST - Rejected Delta-Out64 as Default

Goal:

```text
Try the smaller remaining DeltaNet weight-streaming lever: Delta-Out from
MMA32 residual-in-MMA to MMA64 residual-in-MMA.
```

Implemented opt-in:

```text
qwen35_08b_prefill_deltanet_out_mma64x8_residual_fp16_tiled_k2048_f32
CTOX_QWEN35_DELTA_OUT_MMA64=1
```

Measurements:

```text
p1024 Delta18+FFN stack, iterations=5:
  Delta-Out64: 0.400417084s
  previous best with Delta-Out32: 0.395547250s
  checksum16: -0.910950

p4096 Delta18+FFN stack, iterations=5:
  Delta-Out64: 1.668025708s
  previous best with Delta-Out32: 1.584731750s to 1.618021542s
  checksum16: -0.910950
```

Decision:

```text
Reject Delta-Out64 as default. It is correct but slower, likely because the
larger token tile increases register pressure more than it saves weight-stream
traffic. Keep it as an opt-in forensic candidate only.
```

Dispatch fix:

```text
The attention dispatch path now uses grouped-head dispatch whenever
attention_head_groups differs from attention_q_heads, even if query_block == 1.
Without this, qh4/qblk1 launched extra empty head groups and measured the wrong
threadgroup geometry.
```

Measurements:

```text
p1024 attention.core, iterations=5:
  qblk4_batch:  0.034864875s
  qh4/qblk1:    0.029391084s
  checksum16:  -0.870191

p4096 attention.core, iterations=5:
  qblk4_batch:  0.368125833s
  qh4/qblk1:    0.318992166s
  qh4/qblk1 repeat: 0.317865167s
  checksum16:  -0.870191
```

Decision:

```text
Accept qh4/qblk1 as the optimized attention.core profile. It has the same
logical GQA KV byte floor as qblk4's query-block sharing at p4096, but the lower
register pressure and fewer useful head-groups improve real wall-clock time.
```

p4096 memory forensics with qh4/qblk1:

```text
delta18+ffn:
  median_ms:   1694.739
  model_bytes:   55.51 GiB

attention.core:
  median_ms:    301.913
  model_bytes:   19.65 GiB
  groups:
    project=256@16
    attention=8192@qh4/qblk1
    out=256@16
  KV floor:
    per_query_logical=64.02 GiB
    qh4/qblk1_logical=16.00 GiB
    saved=48.01 GiB

attention.ffn:
  median_ms:     36.480
  model_bytes:    1.37 GiB

full_prefill_estimate_current_kernels:
  3.725s
  1099.57 tok/s
```

Reference gap:

```text
llama.cpp pp4096 reference:
  2852.70 tok/s

current p4096 estimate:
  1099.57 tok/s

remaining gap:
  2.59x slower
```

Current accepted default profile:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA64=1
CTOX_QWEN35_DOWN_MMA64=1
CTOX_QWEN35_DOWN_MMA64_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
CTOX_QWEN35_ATTENTION_QH4_QBLK1_SIMDREDUCE_BATCH=1
```

## 2026-04-30 12:35 CEST - Accepted GateUp64 MMA Candidate

Goal:

```text
Halve the FFN Gate/Up weight-stream groups after QKV/Z64 and Down64 were
accepted. This is a high register-pressure experiment because gate and up are
both accumulated live before SwiGLU.
```

Implemented:

```text
qwen35_08b_prefill_ffn_gate_up_mma64x8_normed_fp16_tiled_k1024_i3584
CTOX_QWEN35_FFN_GATE_UP_MMA64=1
```

Safety fix:

```text
Added explicit MMA token-tile validation for FFN/Delta+FFN paths. 16/32/64
MMA kernels do not tail-guard, so they now require tokens % token_tile == 0
and row_tile == 8 unless a future padded path is added.
```

Measurements:

```text
p1024 Delta18+FFN stack, iterations=5:
  QKV/Z64 + GateUp64 + Down64: 0.395547250s
  previous QKV/Z64 + Down64:   0.410841667s
  checksum16:                  -0.910950

p4096 Delta18+FFN stack, iterations=5:
  run 1:       1.618021542s
  run 2:       1.584731750s
  checksum16: -0.910950

p4096 layer 3 FFN block, iterations=5:
  GateUp64 + Down64: 0.036219334s
  checksum16:        0.398366
```

Decision:

```text
Accept GateUp64 in the optimized profile. Despite high static register pressure,
the measured p1024 and p4096 Delta18+FFN stack runs are checksum-identical and
faster than the prior QKV/Z64+Down64 profile.
```

p4096 memory forensics with GateUp64:

```text
delta18+ffn:
  median_ms:      1686.124
  model_bytes:      55.51 GiB
  weights_stream:   51.75 GiB
  groups:
    qkvz=64@64
    b/a=1024@4
    out=128@32
    gate_up=64@64
    down=64@64

attention.core:
  median_ms:       390.849
  model_bytes:      19.66 GiB

attention.ffn:
  median_ms:        38.245
  model_bytes:       1.37 GiB
  groups:
    gate_up=64@64
    down=64@64

full_prefill_estimate_current_kernels:
  4.261s
  961.35 tok/s
```

Interpretation:

```text
The FFN byte model improved, but the total p4096 estimate is now dominated by
six attention.core runs. The next reference-closing work is therefore Attention
KV reuse/compression or a substantially better attention block schedule, not
more FFN matmul token tiling alone.
```

Current accepted default profile:

```text
CTOX_QWEN35_PROJECT_SPLIT_NORM=1
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA64=1
CTOX_QWEN35_DOWN_MMA64=1
CTOX_QWEN35_DOWN_MMA64_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
CTOX_QWEN35_ATTENTION_QBLK4_SIMDREDUCE_BATCH=1
```

## 2026-04-30 13:46 CEST - Decode Attention qh4 GQA Baseline Accepted

Goal: remove repeated KV work in single-token decode attention before attempting true Split-K/Flash-Decoding. The existing decode attention path launched per Q-head and reread the shared 2-KV-head cache multiple times. Added `qwen35_08b_attention_single_token_qh4_gqa8_kv2_d256_rope_cache_to_fp16`, dispatching 2 KV-head groups with 4 Q-heads per group. This is still not the final Flash-Decoding math; it is the minimal GQA cache-reuse correction.

Validation/benchmarks were run serially on the real packed model `/tmp/ctox_qwen35_08b_real_fp16.metalpack`. The benchmark loaders were updated to accept F16 or BF16 metadata because the real pack stores 16-bit payloads and the decode bench reads packed `u16` values.

Results, isolated decode attention + LM-head benchmark:

| context | old median | qh4 median | speedup | correctness |
| ---: | ---: | ---: | ---: | --- |
| 4096 | 10.675459 ms | 9.343541 ms | 1.14x | same token 153241, same score 0.000146 |
| 8192 | 14.833375 ms | 13.161292 ms | 1.13x | same token 153241, same score 0.000073 |
| 32768 | 39.360125 ms | 31.580209 ms | 1.25x | same token 153241, same score 0.000018 |

Decision: accept qh4 as the decode attention baseline and keep it enabled for decode for all future layered measurements. It fixes a real GQA memory/cache pathology but does not close the llama.cpp gap. Next step is true Split-K/Flash-Decoding for long-context decode with compact partial state (`m`, `l`, and accumulated value summaries) instead of rereading a whole 32k/128k KV range from one serial group.

Cache/memory interpretation: no direct Apple GPU L2 miss counter is exposed in the current local Metal counter list; this row is inferred from traffic. qh4 reduces redundant KV-cache reads across query heads, so the inferred DRAM-equivalent attention traffic falls most at long context. The remaining miss risk is long linear KV streaming; Split-K must improve parallelism without exploding partial-acc scratch traffic.

## 2026-04-30 13:53 CEST - Decode qh4 Made Default

Changed decode attention selection so qh4 GQA reuse is now the default path. The old per-Q-head decode attention can still be forced with `CTOX_QWEN35_DECODE_ATTENTION_NO_QH4=1` for regression tests.

Post-change check at context 4096:

| mode | median | token | score |
| --- | ---: | ---: | ---: |
| default qh4 | 8.168917 ms | 153241 | 0.000146 |
| opt-out old path | 9.231125 ms | 153241 | 0.000146 |

Decision: keep qh4 default. This prevents future full decode measurements from silently using the known slower cache-reuse pattern.

## 2026-04-30 14:08 CEST - qh4 Integrated Into Real 24-Layer Decode Path

Finding: the earlier qh4 decode kernel was only used by the isolated `bench_metalpack_decode_attention` benchmark. The real 24-layer layered decode path still used `qwen35_08b_attention_norm_rope_cache_gqa8_kv2_d256_to_fp16`, one threadgroup per Q-head. That meant the actual model path still repeated KV-cache streaming four times per KV-head.

Change: added `qwen35_08b_attention_norm_rope_cache_qh4_gqa8_kv2_d256_to_fp16`, which preserves the real attention math (Q/K head RMSNorm, RoPE, KV-cache update, gate, online softmax) but computes four Q-heads per KV-head threadgroup. Wired this into `dispatch_decode_layered_pattern_tiled_once`; qh4 is default and the old path is still available via `CTOX_QWEN35_DECODE_ATTENTION_NO_QH4=1`.

Layered 24-layer validation:

| run | default qh4 | old opt-out | correctness |
| --- | ---: | ---: | --- |
| position 0, 1 decode step, 3 iters | 20.639667 ms | 21.867583 ms | same token 198, same score 10.721777 |
| prefill_steps 127, max_context 128, decode_steps 4 | 1.858480625 s | 1.935879750 s | same token stream [107,107,107,107], last score within fp tolerance |

Decision: accepted. This is the first real cache-reuse correction inside the full 24-layer decode path. It is still not enough: at short contexts the full model is dominated by matvec/LM-head, and at long contexts we still need Split-K/Flash-Decoding for attention parallelism and cache-traffic forensics.

## 2026-04-30 14:35 CEST - Decode Split-K256 First Working Layered Path

Goal: implement the first Flash-Decoding-style Split-K path in the real 24-layer decode path, not only in an isolated benchmark. Added opt-in `CTOX_QWEN35_DECODE_ATTENTION_SPLITK256=1` with two kernels:

- `qwen35_08b_attention_norm_rope_cache_qh4_splitk256_partial_gqa8_kv2_d256`
- `qwen35_08b_attention_norm_rope_cache_qh4_splitk256_combine_gqa8_kv2_d256_to_fp16`

The partial kernel computes Q/K head RMSNorm, RoPE, writes current K/V cache, processes a 256-token key block per KV-head, and stores per-block `(m, l, acc[256])`. The combine kernel merges block summaries with the stable online-softmax recurrence and applies the attention gate.

Scratch is allocated once per runner and reused across all six Attention layers:

```text
n_key_blocks = ceil(max_context / 256)
partial_scalars = 2 KV heads * n_key_blocks * 4 Q heads per KV group
partial_m/l = partial_scalars * f32
partial_acc = partial_scalars * 256 * f32
```

Layered 24-layer measurements:

| run | qh4 default | Split-K256 | correctness |
| --- | ---: | ---: | --- |
| position 0, max_context 1, 1 step, 3 iters | 20.639667 ms | 18.164667 ms | same token 198, same score 10.721777 |
| prefill_steps 127, max_context 128, decode_steps 4 | 1.858480625 s | 1.802000958 s | same token stream, same last score within fp tolerance |
| prefill_steps 511, max_context 512, decode_steps 4 | 7.394892292 s | 6.949800042 s | same token stream, last score within fp tolerance |

Decision: Split-K256 is correct enough for continued experimentation and begins to help at real layered sequence scale. It is still opt-in because the block size and scratch traffic need a longer-context sweep. Cache/memory note: partial_acc introduces explicit scratch writes/reads, so Split-K only wins if the added parallelism and cache reuse beat that scratch traffic. This must be measured at 2k/4k/8k+ contexts before making it default.

## 2026-04-30 15:06 CEST - Decode Split-K Auto Threshold + CPU Overhead Fix

After correcting Split-K so only the key block containing the current position updates the current KV-cache slot, short-context Split-K lost its earlier apparent gain. That was the right correction: the earlier version had redundant same-value KV writes from every key-block threadgroup.

Measured after correction:

| run | qh4/default comparison | result |
| --- | ---: | --- |
| prefill_steps 127, max_context 128, decode_steps 4, forced Split-K | 1.868939208 s vs qh4 1.858480625 s | Split-K loses at one block |
| prefill_steps 511, max_context 512, decode_steps 4, forced Split-K | 7.708783250 s vs qh4 ~7.39-7.51 s | Split-K loses/neutral at two blocks |
| prefill_steps 1023, max_context 1024, decode_steps 4, forced Split-K | 14.917422084 s vs qh4 16.166961458 s | Split-K wins at 4 blocks |

A CPU-orchestration issue was also found: the layered dispatch was resolving the Split-K pipeline states even when the current token still used qh4. Moved Split-K pipeline lookup inside the Split-K branch. With automatic thresholding, the 1024-context sequence improved further:

```text
Auto threshold default min_context=1024:
    14.221996375 s
    tokens [107,107,107,107]
    last_score 19.297121
```

Implementation policy now:

```text
qh4 GQA decode: default for short contexts
Split-K256: automatic when position+1 >= 1024
CTOX_QWEN35_DECODE_ATTENTION_SPLITK256=1: force Split-K
CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK256=1: disable Split-K auto/force
CTOX_QWEN35_DECODE_ATTENTION_SPLITK256_MIN_CONTEXT=N: tune threshold
```

This is a real CPU/GPU overhead reduction: short contexts no longer pay Split-K pipeline-state lookup or extra dispatches, while long contexts get more attention parallelism. Remaining problem: `partial_acc` scratch traffic is still heavy. The next Split-K iteration should reduce scratch bytes or use a block size sweep (128/256/512/1024) and a forensic model for partial scratch write/read versus KV-cache read reuse.

Verification: `cargo test` passed after these changes.

## 2026-04-30 15:20 CEST - Cache Model Adds Decode Split-K Scratch Forensics

Added `attention.decode_splitk_scratch` to `src/cache_model.rs` so Split-K is visible in the cache/memory forensic tool. The model now separates:

- `attention.kv_cache_read`: qh4 KV-cache streaming floor.
- `attention.decode_splitk_scratch`: partial `m/l/acc` write plus combine read.

Example model output for `--tokens 1024 --decode-position 1024`:

```text
attention.kv_cache_read           workset 2.00 MiB    unavoidable 2.00 MiB
attention.decode_splitk_scratch   workset 32.25 KiB   unavoidable 32.25 KiB, modeled hit 50%
```

Interpretation: at 1k context, Split-K scratch itself is tiny and should fit modeled L2; the measured win/loss is dominated by dispatch/orchestration and parallelism, not by scratch bandwidth yet. At 128k, the scratch grows to roughly 4 MiB per Attention layer invocation for `partial_acc`, still modeled L2-fit on a 32 MiB assumption but now large enough that real GPU cache counter data or a better Metal trace proxy is required. Because local `list_metal_counters` only exposes `GPUTimestamp`, these remain modeled/inferred cache hit/miss estimates, not hardware L2 counter measurements.

Verification: `cargo test cache_model` passed.

## 2026-04-30 15:34 CEST - NPU/ANE Math and Quantization Check

Question: whether the current compute math is close to optimal, especially for the Apple Neural Engine / NPU path.

Findings:

- GPU/Metal decode math is only partially optimized. Implemented exact qh4 GQA cache reuse and a Flash-Decoding-style Split-K256 path with log-sum-exp combine. This is mathematically appropriate for exact long-context softmax attention, but still needs block-size autotuning and reduced `partial_acc` scratch traffic.
- DeltaNet prefill math is not yet optimal. Current decode uses recurrent state update directly, which is correct for single-token decode. For prefill, the stronger mathematical direction is chunkwise/parallel DeltaNet scan, because Gated DeltaNet papers formulate hardware-efficient chunk algorithms around matmul-like blocks rather than purely scalar recurrence.
- ANE/NPU is not served by Metal kernels. It requires a separate Core ML graph path. Core ML optimization guidance points to 8-bit activation + 8-bit weight quantization (W8A8) for Neural Engine latency on newer Apple hardware, and to palettization/linear quantization combinations for compressed weights.
- The likely ANE-friendly experiment is not full stateful token decode first. It is: isolated Core ML linear/FFN blocks, W8A8 quantized, benchmarked on `.cpuAndNeuralEngine` / `.all`, plus maybe prefill or vision blocks. Stateful DeltaNet recurrence, KV-cache updates, and per-token sampling remain GPU-local unless Core ML placement proves otherwise.

Decision: add a separate NPU baseline plan before claiming heterogeneous CPU/GPU/NPU optimization. Required experiments:

1. Convert isolated Qwen3.5 linear blocks and FFN blocks to Core ML.
2. Produce FP16, W8, W4-palettized, and W8A8 variants.
3. Benchmark `.cpuAndNeuralEngine`, `.cpuAndGPU`, and `.all`.
4. Check operation placement/fallbacks; reject ANE path if DeltaNet/stateful decode falls back to CPU or introduces per-token graph overhead.
5. Keep GPU decode as primary until ANE shows a measured win on a coarse subgraph.

## 2026-04-30 15:48 CEST - Why llama.cpp Is Faster Than Current CTOX Probe

Local reference source checked: `/Users/michaelwelsch/Downloads/llama.cpp`, commit `15fa3c4`.

Fundamental differences found:

1. llama.cpp already has Qwen3.5/Qwen3Next architecture integration, including SSM/Gated DeltaNet tensor classes and model graph builders. Relevant files include `src/models/qwen3next.cpp`, `src/models/qwen35moe.cpp`, and `src/models/delta-net-base.cpp`.
2. llama.cpp has explicit DeltaNet modes: autoregressive, chunked, and fused. `delta-net-base.cpp` routes to `build_delta_net_fused`, `build_delta_net_autoregressive`, or `build_delta_net_chunking`; the fused path uses `ggml_gated_delta_net`.
3. The Metal backend has a dedicated `kernel_gated_delta_net_*` pipeline family with function constants for shape specialization. This is a direct model-specific optimization we are only partially reproducing.
4. Attention uses a mature `GGML_OP_FLASH_ATTN_EXT` backend with pad/block/vector/reduce kernel variants and function-constant specialization. CTOX only recently added qh4 and first Split-K256 decode, and prefill attention is still not comparable to llama.cpp's mature flash-attention implementation.
5. Matrix multiply coverage is much broader in llama.cpp. The Metal backend instantiates simdgroup matrix kernels for BF16/F16/F32 and many quantized types (`q4_K`, `q5_K`, `q8_0`, `iq*`, `mxfp4`, etc.) for both matrix-matrix and matrix-vector/id paths.
6. llama.cpp's runtime is a preplanned ggml graph with ubatching, backend scheduling, preallocated buffers, pipeline caching, and fewer accidental CPU orchestration costs. CTOX has already found one such cost in Split-K pipeline lookup, which confirms this class of issue matters.
7. llama.cpp is not using an ANE/NPU mega-kernel here. The reference advantage is mainly mature Metal GPU execution plus model-specific graph/kernel coverage. ANE remains a separate Core ML experiment, not something llama.cpp demonstrates for this benchmark.

Implication for CTOX: the biggest missing pieces are not cosmetic fusion. They are (a) chunked/fused DeltaNet prefill math, (b) full model-grade FlashAttention/prefill attention rather than isolated qh4 fixes, (c) quantized weight paths with in-dot dequantization, and (d) a graph/runtime layer that eliminates CPU-side pipeline lookup and dispatch overhead by construction.

## 2026-04-30 16:06 CEST - Decode Split-K Block Sweep and Active-Block Fix

Implemented a generalized qh4 Split-K decode-attention path for 128/256/512/1024 key blocks and fixed a major orchestration bug: the layered decode path now dispatches only the active key blocks for the current `position + 1`, not all blocks allocated for `max_context`.

Relevant implementation points:

- `CTOX_QWEN35_DECODE_ATTENTION_SPLITK_BLOCK=128|256|512|1024` selects the partial kernel.
- Default block size is now 128.
- Split-K auto threshold now defaults to context length 1, because the active-block fix made Split-K128 faster even at short contexts in measured layered decode.
- Legacy `SPLITK256` environment flags remain accepted for compatibility, but the generic `SPLITK` names are preferred.
- The combine kernel is block-size agnostic; the partial kernel controls key-block width and current-position KV-cache update ownership.

Measured layered decode results after the active-block fix:

```text
prefill_steps 127, max_context 128, decode_steps 4
    qh4 no Split-K:       2.309343917 s
    forced Split-K128:    1.871430625 s
    default Split-K128:   1.531830042 s

prefill_steps 511, max_context 512, decode_steps 4
    qh4 no Split-K:       10.651295625 s in this run
    forced Split-K128:    8.931584250 s

prefill_steps 1023, max_context 1024, decode_steps 4
    forced Split-K128:    14.661829625 s
    forced Split-K256:    14.874407084 s
    forced Split-K512:    17.249883166 s
    forced Split-K1024:   17.847852000 s
    default Split-K128:   11.901484917 s
```

All compared runs produced the same greedy token stream `[107, 107, 107, 107]` for the tested sequence. Small last-score differences are expected from different softmax reduction order:

```text
Split-K128 @ 1024: 19.296448
Split-K256 @ 1024: 19.297586
```

Interpretation:

- The bug was a real hidden overhead source: dispatching empty future key blocks made Split-K look much worse than the algorithm itself.
- Split-K128 currently wins for this path because it increases parallelism without paying too much partial-scratch traffic.
- This still does not make the system competitive with llama.cpp. It improves long-context decode attention, but the full probe remains dominated by DeltaNet/FFN prefill and model-wide weight streaming.
- The next high-value work is not more Decode Split-K tuning. It is chunked/fused DeltaNet prefill, because llama.cpp has a dedicated fused/parallel `ggml_gated_delta_net` path while CTOX still streams through multiple intermediate DeltaNet kernels.

Verification: `cargo test cache_model` passed after the Split-K block-size/default changes.

## 2026-04-30 16:37 CEST - DeltaNet Prefill Negative Experiments

Investigated llama.cpp's fused DeltaNet implementation:

- `src/models/delta-net-base.cpp::build_delta_net_fused` routes to `ggml_gated_delta_net`.
- `ggml/src/ggml-metal/ggml-metal.metal::kernel_gated_delta_net_impl` uses `NSG = S_v / 32`, dispatches `32 x NSG` threads per group, and keeps only `NSG` state columns per thread.
- The llama.cpp kernel stores the recurrent state transposed/row-contiguous and uses `simd_sum` across the 32-lane row worker.

Ported this idea into CTOX as an opt-in scan kernel:

```text
CTOX_QWEN35_DELTA_SCAN_LANES4=1
kernel: qwen35_08b_prefill_deltanet_scan_lanes4_f32_state_tok_h16d128
shape: 32 x 4 threads per threadgroup, 4 rows per row-block
```

Measured against the current Rowcache scan:

```text
Isolated DeltaNet scan, 4096 tokens:
    rowcache: 13.145375 ms
    lanes4:   17.368959 ms

Delta18+FFN stack slice, 3 Delta+FFN layers, 4096 tokens:
    rowcache: 235.945458 ms
    lanes4:   248.993458 ms
```

Result: lanes4 is correct but slower. The current Rowcache kernel's high per-thread state register pressure is apparently still faster for this shape than the llama.cpp-style 32x4 split in the current CTOX buffer/layout/runtime.

Retested existing scan+gated-norm fusion:

```text
Delta18+FFN stack slice, 3 Delta+FFN layers, 4096 tokens:
    rowcache + separate gated norm: 235.945458 ms
    rowcache + scan gated norm:     243.047333 ms
```

Result: scan+gated-norm fusion remains slower. Avoid enabling it by default.

Retested Conv+Split Tok4:

```text
Delta18+FFN stack slice, 3 Delta+FFN layers, 4096 tokens:
    Conv+Split fused tok1: 235.945458 ms
    Conv+Split fused tok4: 774.487250 ms
```

Result: Tok4 is a hard regression and should stay disabled.

Added opt-in QKV/Z projection MMA128:

```text
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128=1
kernel: qwen35_08b_prefill_matmul_mma128x8_fp16_tiled_k1024_f32
```

Measured:

```text
Delta18+FFN, 18 Delta+FFN layers, 4096 tokens:
    QKV/Z MMA64:  1.554008459 s
    QKV/Z MMA128: 1.903848208 s
```

Result: MMA128 lowers the modeled QKV/Z weight stream but loses badly in real time, likely due to register pressure and lower occupancy from 16 live MMA accumulators. Keep it opt-in only; do not set as default.

Stable current p4096 forensics after rerun:

```text
delta18+ffn:    1541.698 ms
attention.core:  273.673 ms
attention.ffn:    30.006 ms
full estimate:  3.364 s, 1217.68 tok/s
```

Reference remains much faster:

```text
llama.cpp pp4096: 2852.70 tok/s
CTOX pp4096 estimate: 1217.68 tok/s
```

Interpretation: the easy local fusion attempts are exhausted. The remaining gap is structural: llama.cpp's model-grade graph and Metal backend get much better effective utilization for prefill. Next work should focus on tooling that breaks Delta18+FFN into measured sub-operators under one stable run, then attack the largest measured sub-op rather than flipping broad fusion flags.

## 2026-04-30 17:22 CEST - Superblock Prefix Profiler and First Autotuner

Added a real Dev-Tooling layer instead of continuing manual flag flips.

New tools:

```text
profile_metalpack_prefill_delta_stack
autotune_metalpack_prefill_delta_stack
```

`profile_metalpack_prefill_delta_stack` runs the same optimized DeltaNet+FFN superblock encoder path multiple times with prefix stops:

```text
project
conv_split
scan_norm
delta_out
ffn_gate_up
full
```

The phase time is computed by cumulative deltas. This is not hardware timestamp-zone profiling, but it is currently the best practical tool because this Mac exposes only `GPUTimestamp`, not L2/cache miss counters. It avoids the prior problem where older standalone microbenchmarks no longer matched the optimized superblock path.

Measured 1 Delta+FFN layer, 4096 tokens:

```text
phase                    cum_ms     delta_ms       share
project                  29.184       29.184       31.2%
conv/split+ba            32.780        3.596        3.8%
scan+norm                48.979       16.199       17.3%
delta out                57.617        8.639        9.2%
ffn norm+gate/up         77.834       20.217       21.6%
ffn down                 93.542       15.707       16.8%
```

Measured 18 Delta+FFN layers, 4096 tokens, baseline profile:

```text
phase                    cum_ms     delta_ms       share
project                 535.626      535.626       30.1%
conv/split+ba           585.745       50.119        2.8%
scan+norm               878.656      292.911       16.5%
delta out              1041.744      163.087        9.2%
ffn norm+gate/up       1394.297      352.553       19.8%
ffn down               1777.288      382.991       21.5%
```

Conclusion from the profiler: the largest optimization targets are now:

```text
1. QKV/Z projection
2. FFN down
3. FFN gate/up
4. Delta scan/norm
```

This explains why pure DeltaNet scan work cannot close the gap alone.

Added `autotune_metalpack_prefill_delta_stack`, a serial coordinate-descent tuner. It starts from the accepted profile and sweeps candidate families:

```text
qkvz:      mma32, mma64, mma128
delta_out: mma32_res, mma64_res
gate_up:   mma32, mma64
down:      mma32_res, mma64_res
scan:      rowcache, lanes4
conv:      fused, fused_tok4
```

The tuner runs serially, not in parallel, and ranks candidates by median time with p95 as tie-breaker. It also keeps a global incumbent because one pass of coordinate descent can regress due to interactions and benchmark noise.

First 18-layer autotune run:

```text
baseline:
    qkvz=mma64, delta_out=mma32_res, gate_up=mma64,
    down=mma64_res, scan=rowcache, conv=fused
    median 1.715703 s

best observed:
    qkvz=mma128, delta_out=mma64_res, gate_up=mma64,
    down=mma64_res, scan=lanes4, conv=fused
    median 1.605021 s during tuning
```

Longer direct recheck with 7 iterations:

```text
baseline:
    median_s 1.747005750
    p95_s    1.762286250
    checksum -0.910950

autotune candidate:
    median_s 1.613044375
    p95_s    1.636081292
    checksum -0.911804
```

Result: the autotune candidate is about 7.7% faster on the Delta18+FFN stack in this measurement. However, the checksum drift changed from `-0.910950` to `-0.911804`. This is probably accumulation/layout-order drift, but it is not enough proof. Before promoting this as the default path, the next required step is a stronger correctness harness: dump full hidden output for baseline and candidate, compute max/mean error, and check downstream greedy token stability.

Methodology decision: keep baseline defaults conservative for now. Use the autotuner to discover candidates, then require a correctness gate before changing defaults.

## 2026-04-30 17:54 CEST - Autotuner Correctness Gate and CSV Forensics

Extended `autotune_metalpack_prefill_delta_stack` so it is no longer just a
manual timing helper. The tuner now records every evaluated selection in a CSV
file and automatically validates the final global incumbent against the
conservative baseline with full hidden-state dumps.

New autotuner behavior:

```text
1. serial coordinate descent only; no parallel benchmark workers
2. every candidate row records:
   phase, family, candidate, full selection, median_s, p95_s,
   effective_GB/s, model_bytes, tok/s, checksum
3. after tuning, run baseline and best candidate with
   CTOX_QWEN35_DELTA_STACK_FINAL_DUMP
4. compare the two FP16 hidden dumps with compare_half_dump
5. print and CSV-record pass/fail with:
   mismatch_count, mean_abs_error, rms_error, max_abs_error, checksum_delta
```

Default gate thresholds:

```text
CTOX_QWEN35_AUTOTUNE_MAX_MEAN_ABS       0.0005
CTOX_QWEN35_AUTOTUNE_MAX_RMS            0.0010
CTOX_QWEN35_AUTOTUNE_MAX_ABS            0.0100
CTOX_QWEN35_AUTOTUNE_MAX_CHECKSUM_DELTA 1.0
```

The thresholds are intentionally conservative. A faster kernel/layout candidate
is allowed to be discovered, but it is not accepted as a default if it changes
the hidden state beyond the gate.

The output now separates:

```text
best_selection:
  fastest observed candidate

accepted_selection:
  best_selection only if correctness_gate passes;
  otherwise the conservative baseline selection
```

This prevents the tuning loop from silently promoting a numerically drifting
layout.

Manual full-dump comparison for the earlier 18-layer, 4096-token speed
candidate:

```text
elements:           4,194,304
mismatch_count:     3,744,501
mean_abs_error:     0.001899509
rms_error:          0.002485653
max_abs_error:      0.062500000
max_abs_token:      1060
max_abs_col:        0
baseline_checksum:  195995.820009172
candidate_checksum: 195979.359989285
checksum_delta:    -16.460019886
```

Decision: reject that 4096-token incumbent as a default for now. It is faster,
but it fails the new hidden-dump gate. The likely culprit is an accumulation
order/layout interaction around the `mma128 + lanes4` combination. It can stay
as an experimental candidate, but promotion requires downstream logits/greedy
token stability or a tighter numerically equivalent implementation.

Verification run of the new tool on a quick 1-layer, 128-token sweep:

```text
command:
  target/release/autotune_metalpack_prefill_delta_stack \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 128 1 0 1 0 1

best observed:
  qkvz=mma128, delta_out=mma64_res, gate_up=mma64,
  down=mma64_res, scan=rowcache, conv=fused
  median_s 0.004011334

correctness_gate:
  PASS
  mean_abs_error 0.000000000
  rms_error      0.000000000
  max_abs_error  0.000000000
  checksum_delta 0.000000000

history_csv:
  /var/folders/hf/x8jpryjx6xs8k0kwmkj5spkw0000gn/T/ctox_qwen35_autotune_delta_stack_3503.csv
```

This confirms the mechanism works. It also shows why small-token tests are not
enough: a candidate can pass at 128 tokens but drift at 4096 tokens. Future
autotune acceptance must run a token sweep, at minimum 512/4096/16384, before
changing defaults.

Next methodology step: integrate modeled memory/cache-miss budgets directly into
the tuner score, not only into the separate forensic tools. The score should
eventually report:

```text
median_s
p95_s
effective_GB/s
modeled_bytes
weight_stream_bytes
logical_operand_bytes
reuse_opportunity
tail_underfill
correctness status
```

This is the right direction for the cache-miss problem: because local Metal
counter access still exposes only `GPUTimestamp`, the tuner must use empirical
runtime plus an explicit byte/cache model and reject candidates that are fast
only because they changed math.

Added a serial token-sweep wrapper:

```text
sweep_metalpack_prefill_delta_autotune
```

It runs `autotune_metalpack_prefill_delta_stack` once per token length,
serially, with a separate per-token CSV and one summary CSV. This is needed
because cache behavior and tail underfill can invert between short and long
prefill shapes.

Smoke test:

```text
command:
  target/release/sweep_metalpack_prefill_delta_autotune \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 64,128 1 0 1 0 0

tokens   median_s    p95_s      tok_s      gate
64       0.003944    0.003944   16228.72   pass
128      0.004721    0.004721   27111.46   pass

summary_csv:
  /var/folders/hf/x8jpryjx6xs8k0kwmkj5spkw0000gn/T/ctox_qwen35_delta_autotune_sweep_5910/summary.csv
```

After adding `accepted_selection`, reran a smaller smoke check:

```text
target/release/sweep_metalpack_prefill_delta_autotune \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 64 1 0 1 0 0

tokens   median_s    p95_s      tok_s      gate   accepted_selection
64       0.004823    0.004823   13268.83   pass   conservative baseline
```

The smoke tests used `passes=0`, so they only verified orchestration and the
baseline correctness gate. Real acceptance sweeps should use the full candidate
space and longer token counts.

Verification:

```text
cargo check --bin autotune_metalpack_prefill_delta_stack --bin sweep_metalpack_prefill_delta_autotune
cargo build --release --bin autotune_metalpack_prefill_delta_stack --bin sweep_metalpack_prefill_delta_autotune
cargo test cache_model
```

All passed.

## 2026-04-30 18:18 CEST - Kernel Dev Handbook Knowledge Base

Created and expanded `KERNEL_DEV_HANDBOOK.md` as the distilled knowledge base
for the Qwen3.5 Metal kernel work. This is separate from the chronological
research log:

```text
RESEARCH_LOG.md:
  lab notebook, commands, measurements, failures, decisions

KERNEL_DEV_HANDBOOK.md:
  reusable engineering playbook and lookup document
```

The handbook now covers:

```text
North Star architecture
current accepted defaults
fixed Qwen3.5-0.8B shape contract
measurement discipline
correctness gates
cache-miss reality and memory forensics
layout/tile decision matrix
autotune command cookbook
autotune acceptance protocol
accepted and rejected optimization patterns
why llama.cpp is still faster
prefill/decode/CPU/ANE strategies
subagent benchmark policy
hypothesis template
definition of done
transfer rules for 27B/35B
```

Important distilled rules:

1. The target is not a romantic "single shader". It is a measured GPU-local
   pipeline with CPU orchestration and optional coarse Core ML/ANE experiments.
2. For weight streaming, literal zero cache misses is impossible. The target is
   only compulsory misses plus no avoidable re-reads, no accidental scratch
   traffic, and no CPU/GPU roundtrips in the hot path.
3. A faster candidate is not accepted unless it passes hidden/logit/token
   correctness gates and survives token-length sweeps.
4. Autotuning is required for layout and chunk choices; tile size decisions are
   empirical and shape-dependent.
5. llama.cpp's advantage is mature Metal GPU graph/kernel execution, not ANE.
6. For 27B/35B, copy the method, not Qwen3.5-0.8B tile constants.

Linked the handbook from `README.md` under `Research Knowledge Base`.

## 2026-04-30 18:31 CEST - Operational Kernel Dev Templates

Added a `docs/kernel-dev/` mini-wiki so the handbook rules become executable
workflow artifacts instead of prose only.

New files:

```text
docs/kernel-dev/README.md
docs/kernel-dev/EXPERIMENT_TEMPLATE.md
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/CACHE_FORENSICS_CHECKLIST.md
```

Purpose:

```text
EXPERIMENT_TEMPLATE:
  fill before implementing a kernel/layout/runtime hypothesis

DECISION_RECORD_TEMPLATE:
  fill after measurements to record accepted/rejected/opt-in decisions

BENCHMARK_PROTOCOL:
  serial benchmark rules, baseline profile, minimum acceptance commands,
  invalid benchmark conditions

CACHE_FORENSICS_CHECKLIST:
  byte buckets, cache evidence labels, required questions, red flags
```

Updated `KERNEL_DEV_HANDBOOK.md` and `README.md` to link these templates.

Methodology impact: new experiments should now start from a template, not from
free-form chat notes. That should reduce repeated mistakes around missing env
flags, weak correctness gates, non-comparable benchmarks, and unsupported cache
miss claims.

Static subagent review of the new knowledge base identified three remaining
reproducibility gaps, now closed:

```text
1. EXPERIMENT_TEMPLATE.md now includes a Run Manifest:
   git state, device/OS, metalpack hash, binary path, full env dump,
   baseline/candidate env, CSV paths, dump paths, reference implementation.

2. BENCHMARK_PROTOCOL.md now defines named measurement packs:
   smoke, candidate, acceptance, long-context.

3. Added FLAG_LIFECYCLE_TEMPLATE.md for every new CTOX_QWEN35_* env flag:
   status, default, compatibility, fallback path, correctness gates,
   benchmark requirements, promotion/removal criteria.
```

No benchmarks were run by the subagent; it only performed static documentation
review.

## 2026-04-30 18:36 CEST - Experiment Scaffold Tool

Added `tools/new_kernel_experiment.sh` to turn the experiment template into an
actual developer tool.

Usage:

```text
tools/new_kernel_experiment.sh <slug> [metalpack-dir]
```

It creates:

```text
docs/kernel-dev/experiments/<timestamp>-<slug>.md
```

and fills the Run Manifest with:

```text
UTC timestamp
owner
git commit
crate-scoped dirty state
macOS version
device / Metal display info
metalpack path
manifest SHA-256
weights SHA-256
env dump path
reference implementation hint
```

Also added `docs/kernel-dev/experiments/README.md` so generated experiment
records have a stable home. Smoke-tested the scaffold with the real FP16
metalpack, verified the generated manifest, and removed the smoke record again
so it is not confused with a real experiment.

Verification:

```text
bash -n tools/new_kernel_experiment.sh
tools/new_kernel_experiment.sh smoke-template-check /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

## 2026-04-30 18:41 CEST - Experiment Record Validator

Added `tools/validate_kernel_experiment.sh` to prevent incomplete experiment
records from becoming decision evidence.

Usage:

```text
tools/validate_kernel_experiment.sh <experiment.md>
tools/validate_kernel_experiment.sh --strict <experiment.md>
```

Default mode verifies the scaffold/run-manifest fields:

```text
date, owner, model, metalpack, git state, device/OS,
Metal device info, metalpack path, env dump, reference implementation
```

Strict mode additionally requires decision-grade fields:

```text
target_path
env_flag
binary_path
baseline_env
candidate_env
output_csv
no template placeholders
```

Smoke-tested with a generated record:

```text
tools/new_kernel_experiment.sh smoke-template-check /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/validate_kernel_experiment.sh <generated-record>
  validation: PASS

tools/validate_kernel_experiment.sh --strict <generated-record>
  validation: FAIL
  expected: record still had placeholders and no candidate fields
```

Removed the generated smoke record and env dump after verification. Updated
`docs/kernel-dev/README.md`, `KERNEL_DEV_HANDBOOK.md`, and `README.md` to link
the validator.

## 2026-04-30 18:47 CEST - Kernel Dev Doctor

Added `tools/kernel_dev_doctor.sh` to validate the knowledge/tooling layer
itself. It checks:

```text
required handbook/wiki/template files exist
shell scripts parse with bash -n
README and KERNEL_DEV_HANDBOOK link core tools
docs/kernel-dev/README links core templates
generated experiment records pass default validation
optional strict validation status for generated records
```

It does not run performance benchmarks.

Verification:

```text
tools/kernel_dev_doctor.sh
  required_files: 13
  experiments: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  required_files: 13
  experiments: 0
  experiments_failed_strict: 0
  validation: PASS
```

Updated `docs/kernel-dev/README.md`, `KERNEL_DEV_HANDBOOK.md`, and `README.md`
to reference the doctor tool.

## 2026-04-30 18:54 CEST - Accepted Profile Source Of Truth

Centralized the conservative accepted baseline flags in:

```text
docs/kernel-dev/accepted_profile.env
```

Added:

```text
tools/run_accepted_profile.sh
```

so benchmark and forensics commands can be run against the same accepted
baseline without copy/pasting env blocks.

Example:

```text
tools/run_accepted_profile.sh printenv CTOX_QWEN35_DELTA_SCAN_ROWCACHE
  1
```

Updated:

```text
KERNEL_DEV_HANDBOOK.md
docs/kernel-dev/README.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
README.md
tools/kernel_dev_doctor.sh
```

The doctor now requires the accepted profile and wrapper:

```text
tools/kernel_dev_doctor.sh
  required_files: 15
  experiments: 0
  validation: PASS
```

Methodology impact: accepted defaults now have one source of truth. Candidate
experiments should add env overrides in their experiment record rather than
editing or duplicating the accepted profile.

## 2026-04-30 19:02 CEST - Experiment Manifests Include Accepted Profile Hash

Improved the experiment scaffold so every generated run manifest records the
accepted baseline profile, not only the model pack.

Updated:

```text
docs/kernel-dev/EXPERIMENT_TEMPLATE.md
tools/new_kernel_experiment.sh
tools/validate_kernel_experiment.sh
docs/kernel-dev/README.md
docs/kernel-dev/experiments/README.md
```

New manifest fields:

```text
accepted_profile_path
accepted_profile_hash
```

The scaffold now also fills:

```text
baseline_env: docs/kernel-dev/accepted_profile.env
output_csv:  /tmp/ctox_qwen35_<timestamp>_<slug>.csv
dump_paths:  /tmp/ctox_qwen35_<timestamp>_<slug>_*.bin
```

The validator now checks that the accepted profile path exists and warns if the
recorded hash no longer matches the local profile file.

Smoke check:

```text
tools/new_kernel_experiment.sh smoke-profile-hash-check /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/validate_kernel_experiment.sh <generated-record>
  validation: PASS

tools/validate_kernel_experiment.sh --strict <generated-record>
  validation: FAIL
  expected because target_path, env_flag, binary_path, candidate_env, and
  hypothesis placeholders were still empty.
```

Removed the generated smoke record and env dump after verification.

Doctor:

```text
tools/kernel_dev_doctor.sh
  required_files: 15
  experiments: 0
  validation: PASS
```

Methodology impact: if the accepted default profile changes, older experiment
records can now detect that their baseline env is no longer the same file hash.

## 2026-04-30 19:09 CEST - Standard Measurement Pack Runner

Added:

```text
tools/run_measurement_pack.sh
```

It wraps the standard benchmark shapes from `BENCHMARK_PROTOCOL.md` and applies
the conservative accepted profile via `tools/run_accepted_profile.sh`.

Supported packs:

```text
smoke:
  128 tokens, 1 iter, 0 warmup, 1 Delta layer, 1 tune pass

candidate:
  4096 tokens, 3 iters, 1 warmup, 18 Delta layers, 2 passes

candidate-7:
  4096 tokens, 7 iters, 1 warmup, 18 Delta layers, 2 passes

acceptance:
  512,4096,16384 sweep, 3 iters, 1 warmup, 18 Delta layers

long-context:
  32768,65536,131072 sweep, 3 iters, 1 warmup, 18 Delta layers
```

Dry-run examples verified without running benchmarks:

```text
tools/run_measurement_pack.sh --dry-run acceptance /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/run_measurement_pack.sh --dry-run long-context /tmp/ctox_qwen35_08b_real_fp16.metalpack
```

Updated:

```text
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
tools/kernel_dev_doctor.sh
```

Doctor:

```text
tools/kernel_dev_doctor.sh
  required_files: 16
  experiments: 0
  validation: PASS
```

Methodology impact: acceptance/candidate/long-context runs now have one command
surface. This should reduce benchmark argument drift and make experiment
records easier to reproduce.

## 2026-04-30 19:16 CEST - Experiment Record Listing Tool

Added:

```text
tools/list_kernel_experiments.sh
```

It scans `docs/kernel-dev/experiments/*.md`, skips the directory README, and
prints each generated record with:

```text
record
date
env_flag
default validation status
strict validation status
decision
```

It also supports Markdown table output:

```text
tools/list_kernel_experiments.sh --markdown
```

Verified on the current empty experiment directory:

```text
tools/list_kernel_experiments.sh
  (no generated experiment records)

tools/list_kernel_experiments.sh --markdown
  | record | date | env_flag | default | strict | decision |
```

Updated:

```text
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
tools/kernel_dev_doctor.sh
```

Doctor:

```text
tools/kernel_dev_doctor.sh
  required_files: 17
  experiments: 0
  validation: PASS
```

Methodology impact: once multiple hypotheses exist, we can now audit open
records and see which are only scaffolded, which are strict-ready, and which
have a final decision.

## 2026-04-30 19:22 CEST - Experiment Index Generator

Added:

```text
tools/update_kernel_experiment_index.sh
docs/kernel-dev/experiments/INDEX.md
```

The index is generated from:

```text
tools/list_kernel_experiments.sh --markdown
```

and records the same columns as the terminal listing:

```text
record
date
env_flag
default validation status
strict validation status
decision
```

Updated:

```text
docs/kernel-dev/experiments/README.md
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
tools/kernel_dev_doctor.sh
```

Initial doctor run correctly failed because `INDEX.md` was being treated as an
experiment record. Fixed both `tools/list_kernel_experiments.sh` and
`tools/kernel_dev_doctor.sh` to skip `README.md` and `INDEX.md` inside the
experiment directory.

Verification:

```text
tools/update_kernel_experiment_index.sh
  updated: docs/kernel-dev/experiments/INDEX.md

tools/kernel_dev_doctor.sh
  required_files: 19
  experiments: 0
  validation: PASS
```

Methodology impact: experiment status now has a stable generated index in the
repo, not only terminal output.

## 2026-04-30 19:35 CEST - Decision Record Tooling

Added decision-record tooling parallel to experiment-record tooling.

New files:

```text
docs/kernel-dev/decisions/README.md
docs/kernel-dev/decisions/INDEX.md
tools/new_kernel_decision.sh
tools/list_kernel_decisions.sh
tools/update_kernel_decision_index.sh
```

Updated:

```text
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
tools/kernel_dev_doctor.sh
```

`tools/new_kernel_decision.sh` takes:

```text
tools/new_kernel_decision.sh <experiment.md> <accepted|rejected|opt-in|needs-more-data> [slug]
```

and creates:

```text
docs/kernel-dev/decisions/<timestamp>-<decision>-<slug>.md
```

It copies basic evidence fields from the experiment record and regenerates the
decision index.

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-decision-flow /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md needs-more-data smoke-decision-flow
tools/list_kernel_decisions.sh
  smoke-decision-flow ... needs-more-data
```

Removed the smoke experiment and decision records after verification, then
regenerated both indexes.

Doctor:

```text
tools/kernel_dev_doctor.sh
  required_files: 24
  experiments: 0
  validation: PASS
```

Methodology impact: accept/reject/opt-in decisions now have a structured home
instead of living only as free-form log paragraphs.

## 2026-04-30 19:32 CEST - Experiment Index Sync Enforcement

Tightened the experiment-record workflow:

```text
tools/new_kernel_experiment.sh
  now regenerates docs/kernel-dev/experiments/INDEX.md automatically

tools/kernel_dev_doctor.sh
  now regenerates the expected index in a temp file and fails if the checked-in
  INDEX.md is stale
```

Updated:

```text
docs/kernel-dev/README.md
docs/kernel-dev/experiments/README.md
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-index-sync-check /tmp/ctox_qwen35_08b_real_fp16.metalpack
  experiment: docs/kernel-dev/experiments/20260430T153255Z-smoke-index-sync-check.md
  index:      docs/kernel-dev/experiments/INDEX.md

tools/list_kernel_experiments.sh
  smoke-index-sync-check ... default=pass strict=fail decision=n/a

tools/kernel_dev_doctor.sh --strict-experiments
  experiments: 1
  experiments_valid_default: 1
  experiments_failed_strict: 1
  validation: PASS
```

The strict failure is expected for a newly scaffolded record because it still
contains placeholders and no candidate fields. Removed the smoke record and env
dump after verification, regenerated the index, and reran the doctor:

```text
tools/update_kernel_experiment_index.sh
tools/kernel_dev_doctor.sh
  experiments: 0
  validation: PASS
```

Methodology impact: the experiment index can no longer silently drift from the
actual generated records.

## 2026-04-30 17:40 CEST - Decision Record Validator

Added a strict/default validator for kernel decision records:

```text
tools/validate_kernel_decision.sh [--strict] <decision.md>
```

Default mode now verifies required decision metadata and valid decision values.
Strict mode requires the evidence fields needed before a candidate can be
promoted or a rejected path can be closed:

```text
one_sentence
tokens/context
iterations
warmup
baseline_command
candidate_command
correctness_gate
token_sweep_gate
reference_comparison
next_experiment
```

Decision-specific gates:

```text
accepted:
  accepted_env
  hidden_mean_abs_error
  hidden_rms_error
  hidden_max_abs_error

rejected:
  rejected_env
```

Integrated the validator into:

```text
tools/list_kernel_decisions.sh
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
docs/kernel-dev/decisions/INDEX.md
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-decision-validator /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md needs-more-data smoke-decision-validator
tools/validate_kernel_decision.sh docs/kernel-dev/decisions/<smoke>.md
  validation: PASS
  mode: default

tools/validate_kernel_decision.sh --strict docs/kernel-dev/decisions/<smoke>.md
  validation: FAIL
```

The strict failure is expected for a scaffolded decision because it still lacks
measured evidence. The smoke run also exposed a parser bug for field names such
as `tokens/context`; decision tooling now uses robust `awk` field extraction
instead of a slash-sensitive `sed` expression.

Cleaned up the temporary experiment, decision record, and env dump, regenerated
both indexes, and reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 25
  experiments: 0
  decisions: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  validation: PASS
```

Methodology impact: a performance result can no longer become a promoted
default just because it looks fast in one run. It needs a strict decision record
with reproducibility, correctness, token-sweep, and reference-comparison fields.

## 2026-04-30 17:42 CEST - Accepted-Profile Promotion Gate

Added a promotion gate:

```text
tools/check_kernel_promotion.sh <decision.md>
```

It blocks changes to the accepted profile unless:

```text
decision record:
  passes tools/validate_kernel_decision.sh --strict
  has decision: accepted

referenced experiment record:
  exists locally
  passes tools/validate_kernel_experiment.sh --strict
```

Updated:

```text
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-promotion-gate /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md accepted smoke-promotion-gate
tools/check_kernel_promotion.sh docs/kernel-dev/decisions/<smoke>.md
  promotion: BLOCKED
```

The block is expected because the scaffold has no strict evidence: no candidate
env, target path, binary path, correctness metrics, token/context sweep, or
accepted error thresholds.

Cleaned up the temporary records and env dump, regenerated indexes, and reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 26
  experiments: 0
  decisions: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  validation: PASS
```

Methodology impact: `accepted_profile.env` now has a guardrail. Faster flags
cannot become defaults without a strict experiment record and a strict accepted
decision record.

## 2026-04-30 17:46 CEST - Cache Forensics Records

Promoted cache/memory forensics from a checklist into first-class records.

Added:

```text
docs/kernel-dev/FORENSICS_RECORD_TEMPLATE.md
docs/kernel-dev/forensics/README.md
docs/kernel-dev/forensics/INDEX.md
tools/new_cache_forensics_record.sh
tools/validate_cache_forensics.sh
tools/list_cache_forensics.sh
tools/update_cache_forensics_index.sh
```

Integrated into:

```text
tools/kernel_dev_doctor.sh
tools/validate_kernel_decision.sh
tools/new_kernel_decision.sh
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Decision records now have:

```text
forensics_record:
```

Strict decision validation requires that field. If a forensics record is linked,
it must exist locally and pass:

```text
tools/validate_cache_forensics.sh --strict <forensics.md>
```

Forensics records require the evidence level to be explicit:

```text
inferred-only
hardware-counter-backed
```

Strict mode requires runtime, byte-model, and interpretation fields, including:

```text
median_s
p95_s
effective_GB/s
unique_weight_bytes
weight_group_stream_bytes
logical_operand_weight_bytes
modeled_dram_miss_bytes
modeled_cache_hit_bytes
scratch_write_bytes
scratch_read_bytes
tail_underfill
compulsory_miss_floor
avoidable_miss_suspect
occupancy_suspect
scratch_suspect
cpu_overhead_suspect
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-cache-forensics /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_cache_forensics_record.sh docs/kernel-dev/experiments/<smoke>.md prefill_delta_stack smoke-cache-forensics
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md accepted smoke-cache-forensics

tools/validate_cache_forensics.sh docs/kernel-dev/forensics/<smoke>.md
  validation: PASS
  mode: default

tools/validate_cache_forensics.sh --strict docs/kernel-dev/forensics/<smoke>.md
  validation: FAIL

tools/check_kernel_promotion.sh docs/kernel-dev/decisions/<smoke>.md
  promotion: BLOCKED
```

The strict failures are expected for scaffolds because no runtime, byte-model,
correctness, or linked forensics evidence has been filled.

Cleaned up temporary experiment, forensics, decision, and env dump records,
regenerated all indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 33
  experiments: 0
  decisions: 0
  forensics: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  validation: PASS
```

Methodology impact: cache-miss and memory-bandwidth claims now have an evidence
artifact with explicit byte buckets and an evidence-level label. This prevents
ambiguous claims like "cache misses were reduced" unless the modeled bytes or
named hardware counters support the statement.

## 2026-04-30 17:49 CEST - Autotune Evidence Records

Added first-class records for autotune/search evidence.

Added:

```text
docs/kernel-dev/AUTOTUNE_RECORD_TEMPLATE.md
docs/kernel-dev/autotune/README.md
docs/kernel-dev/autotune/INDEX.md
tools/new_autotune_record.sh
tools/validate_autotune_record.sh
tools/list_autotune_records.sh
tools/update_autotune_index.sh
```

Integrated into:

```text
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Strict autotune validation now requires:

```text
search_space
candidate_count
baseline_selection
best_selection
chosen_env
selection_metric
baseline_median_s / p95_s / tok_s
best_median_s / p95_s / tok_s
median_delta_percent / p95_delta_percent
correctness_gate
hidden_mean_abs_error / hidden_rms_error / hidden_max_abs_error
checksum_delta
token_sweep_gate
why_best_won
why_others_lost
risk
decision
next_action
```

If an autotune record links a cache forensics record, that record must also pass:

```text
tools/validate_cache_forensics.sh --strict <forensics.md>
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-autotune-record /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_autotune_record.sh docs/kernel-dev/experiments/<smoke>.md prefill_delta_layout smoke-autotune-record

tools/validate_autotune_record.sh docs/kernel-dev/autotune/<smoke>.md
  validation: PASS
  mode: default

tools/validate_autotune_record.sh --strict docs/kernel-dev/autotune/<smoke>.md
  validation: FAIL
```

The strict failure is expected for a scaffold because no search-space,
candidate metrics, or correctness evidence has been filled.

Cleaned up temporary experiment, autotune record, and env dump, regenerated
indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 40
  experiments: 0
  decisions: 0
  forensics: 0
  autotune: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  autotune_failed_strict: 0
  validation: PASS
```

Methodology impact: empirical layout/tile/chunk tuning now has a structured
artifact. The next performance push can generate candidates automatically, but
the winning candidate still needs search-space, correctness, token-sweep, and
forensics evidence before it can become a default.

## 2026-04-30 17:52 CEST - Decision Autotune Linkage

Connected autotune evidence to decision and promotion validation.

Updated:

```text
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
tools/new_kernel_decision.sh
tools/validate_kernel_decision.sh
tools/check_kernel_promotion.sh
docs/kernel-dev/README.md
```

Decision records now include:

```text
search_based: <yes | no>
autotune_record:
```

Strict decision validation now requires `search_based`. If `search_based: yes`,
then `autotune_record` is required and must pass:

```text
tools/validate_autotune_record.sh --strict <autotune.md>
```

If an `autotune_record` is present even when `search_based: no`, strict decision
validation still checks it. `tools/check_kernel_promotion.sh` inherits this
through strict decision validation and prints linked forensics/autotune records
on a successful promotion.

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-decision-autotune-link /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_autotune_record.sh docs/kernel-dev/experiments/<smoke>.md rowcache-layout smoke-decision-autotune-link
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md accepted smoke-decision-autotune-link

tools/validate_kernel_decision.sh docs/kernel-dev/decisions/<smoke>.md
  validation: PASS
  mode: default

tools/validate_kernel_decision.sh --strict docs/kernel-dev/decisions/<smoke>.md
  validation: FAIL

tools/check_kernel_promotion.sh docs/kernel-dev/decisions/<smoke>.md
  promotion: BLOCKED
```

The strict failure is expected because the scaffolded decision has no filled
`search_based`, no strict forensics record, no correctness metrics, and no
accepted env.

Cleaned up temporary records and env dump, regenerated indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 40
  experiments: 0
  decisions: 0
  forensics: 0
  autotune: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  autotune_failed_strict: 0
  validation: PASS
```

Methodology impact: automated layout/tile/chunk searches are now part of the
promotion chain. A searched candidate cannot become an accepted default unless
the search record itself proves what was searched, what won, what lost, and that
the winning candidate passed correctness and token-sweep gates.

## 2026-04-30 17:53 CEST - Evidence Bundle Inspector

Added:

```text
tools/show_kernel_evidence_bundle.sh <decision.md>
```

It reports default/strict validation status for:

```text
decision
referenced experiment
linked cache forensics record
linked autotune record
promotion gate
```

Updated:

```text
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-evidence-bundle /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md needs-more-data smoke-evidence-bundle
tools/show_kernel_evidence_bundle.sh docs/kernel-dev/decisions/<smoke>.md

kernel evidence bundle
decision: needs-more-data
search_based: n/a
promotion: blocked

artifact     default  strict   path
decision     pass     fail     docs/kernel-dev/decisions/<smoke>.md
experiment   pass     fail     docs/kernel-dev/experiments/<smoke>.md
forensics    missing  missing  n/a
autotune     missing  missing  n/a
```

Cleaned up temporary records and env dump, regenerated indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 41
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  autotune_failed_strict: 0
  validation: PASS
```

Methodology impact: before touching `accepted_profile.env`, one command can now
show whether the complete evidence chain is healthy or where it breaks.

## 2026-04-30 17:59 CEST - Accepted Profile Proposal Gate

Added an accepted-profile update proposal workflow.

Added:

```text
docs/kernel-dev/ACCEPTED_PROFILE_UPDATE_TEMPLATE.md
docs/kernel-dev/profile-updates/README.md
docs/kernel-dev/profile-updates/INDEX.md
tools/propose_accepted_profile_update.sh
tools/validate_accepted_profile_update.sh
tools/list_accepted_profile_updates.sh
tools/update_accepted_profile_update_index.sh
```

Updated:

```text
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

`accepted_env` / `rejected_env` in decision records are now one-line
machine-readable fields instead of bullet blocks. This matches the validator
model and lets profile-update tooling extract the proposed env change.

`tools/propose_accepted_profile_update.sh <decision.md>` does not edit
`docs/kernel-dev/accepted_profile.env`. It only creates a review artifact after:

```text
tools/check_kernel_promotion.sh <decision.md>
```

passes.

Smoke test:

```text
tools/new_kernel_experiment.sh smoke-profile-update-block /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/new_kernel_decision.sh docs/kernel-dev/experiments/<smoke>.md accepted smoke-profile-update-block
tools/propose_accepted_profile_update.sh docs/kernel-dev/decisions/<smoke>.md smoke-profile-update-block
  profile update: BLOCKED
```

The block is expected for scaffolds because strict experiment, strict decision,
correctness, forensics, and accepted-env evidence are missing. No profile-update
proposal was created and `accepted_profile.env` was not changed.

Cleaned up temporary records and env dump, regenerated indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 48
  profile_updates: 0
  validation: PASS
```

Methodology impact: accepted defaults now require a two-stage gate. First the
evidence chain must pass promotion, then a separate profile-update proposal is
created for review before `accepted_profile.env` can change.

## 2026-04-30 17:59 CEST - Accepted Profile Validator

Added:

```text
tools/validate_accepted_profile.sh [profile.env]
```

It validates:

```text
file parses as shell
every active line is export CTOX_QWEN35_<NAME>=<VALUE>
no duplicate env vars
at least one active accepted flag exists
```

Integrated into:

```text
tools/run_accepted_profile.sh
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Initial test exposed a macOS Bash 3 compatibility problem: associative arrays
are not available. Replaced duplicate detection with a temp-file plus
`sort | uniq -d` path.

Verification:

```text
tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  active_flags: 10
  sha256: fea814a42ac1bfebce567a5c4a0ac090524c4def8fb97fa7670f28abbc91de3c

tools/run_accepted_profile.sh printenv CTOX_QWEN35_DELTA_SCAN_ROWCACHE
  1

tools/kernel_dev_doctor.sh
  required_files: 49
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  profile_updates_failed_strict: 0
  validation: PASS
```

Methodology impact: baseline runs now validate the accepted profile before
source-time env injection, reducing the risk of broken shell syntax, duplicate
flags, or non-Qwen35 env contamination.

## 2026-04-30 18:06 CEST - Benchmark Output Normalizers

Added tooling to transfer existing benchmark stdout into evidence records
without rerunning benchmarks.

Added:

```text
tools/normalize_benchmark_output.sh
tools/fill_autotune_record_from_output.sh
tools/fill_forensics_record_from_output.sh
```

Integrated into:

```text
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

`tools/normalize_benchmark_output.sh <stdout.txt>` extracts canonical fields
from existing human-readable benchmark output:

```text
tokens/context
iterations
warmup
median_s
p95_s
effective_GB/s
baseline_median_s / baseline_p95_s
best_selection
accepted_selection
best_env
accepted_env
best_median_s / best_p95_s / best_tok_s
candidate_count
correctness_gate
output_csv
```

`tools/fill_autotune_record_from_output.sh` fills extractable autotune fields
only. It intentionally leaves strict evidence fields empty when stdout does not
contain them:

```text
hidden_mean_abs_error
hidden_rms_error
hidden_max_abs_error
checksum_delta
token_sweep_gate
why_best_won
why_others_lost
risk
decision
next_action
```

`tools/fill_forensics_record_from_output.sh` fills runtime fields only:

```text
tokens/context
median_s
p95_s
effective_GB/s
tok_s
command=source_output=<file>
evidence_level=inferred-only
```

It intentionally does not fill the byte model or interpretation fields.

Smoke tests used synthetic stdout only, no benchmarks:

```text
tools/normalize_benchmark_output.sh /tmp/ctox_qwen35_synth_autotune_output.txt
  baseline_median_s: 1.200000
  baseline_p95_s: 1.300000
  baseline_effective_GB/s: 120.00
  best_median_s: 0.900000000
  candidate_count: 12
```

Initial parser test caught a bug where `median=1.200000s` lost the leading
digit. Fixed by replacing substring arithmetic with split/truncate extraction.

Autotune fill smoke:

```text
tools/fill_autotune_record_from_output.sh /tmp/ctox_qwen35_synth_autotune_output.txt docs/kernel-dev/autotune/<smoke>.md
  updated: docs/kernel-dev/autotune/<smoke>.md

tools/validate_autotune_record.sh docs/kernel-dev/autotune/<smoke>.md
  validation: PASS

tools/validate_autotune_record.sh --strict docs/kernel-dev/autotune/<smoke>.md
  validation: FAIL
```

Forensics fill smoke:

```text
tools/fill_forensics_record_from_output.sh /tmp/ctox_qwen35_synth_forensics_output.txt docs/kernel-dev/forensics/<smoke>.md
  median_s: 0.012500000
  p95_s: 0.013000000
  effective_GB/s: 145.50
  tok_s: 327680.000000

tools/validate_cache_forensics.sh docs/kernel-dev/forensics/<smoke>.md
  validation: PASS

tools/validate_cache_forensics.sh --strict docs/kernel-dev/forensics/<smoke>.md
  validation: FAIL
```

The strict failures are expected because synthetic stdout is not enough to
prove hidden-state correctness, token-sweep safety, or byte/cache causality.

Cleaned up all temporary smoke records and env/stdout files, regenerated
indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 52
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  autotune_failed_strict: 0
  profile_updates_failed_strict: 0
  validation: PASS
```

Methodology impact: measurement values can now move from raw benchmark stdout
into structured evidence records with less manual copying, while the strict
validators still prevent incomplete runtime-only evidence from being promoted.

## 2026-04-30 18:10 CEST - Captured Measurement Runs

Added a serial measurement capture wrapper:

```text
tools/capture_measurement_output.sh [--accepted-profile] [--output-dir DIR] [--label LABEL] -- <command> [args...]
```

It creates one run directory containing:

```text
stdout.txt
stderr.txt
normalized.txt
manifest.txt
exit_code.txt
```

The manifest records:

```text
timestamp
label
repo root / cwd
git commit
dirty state
accepted profile path/hash
command line
stdout/stderr/normalized paths
exit code path
current CTOX_QWEN35_* env before command
```

The wrapper uses an exclusive local lock:

```text
/tmp/ctox_qwen35_measurement.lockdir
```

so a second benchmark run exits instead of contaminating timing.

Also added:

```text
tools/run_measurement_pack.sh --capture <pack> <metalpack-dir>
```

for standardized pack runs.

Updated:

```text
tools/kernel_dev_doctor.sh
tools/run_measurement_pack.sh
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Smoke test used a harmless shell command, not a GPU benchmark:

```text
tools/capture_measurement_output.sh --accepted-profile \
  --output-dir /tmp/ctox_qwen35_capture_smoke \
  --label smoke-capture -- \
  sh -c 'echo "tokens: 128"; echo "iterations: 1"; echo "median_s: 0.010000000"; echo "p95_s: 0.020000000"; echo "effective_gb_s_token_tile_weight_reuse_estimate: 123.45"; echo "checksum16: 9.5"; printenv CTOX_QWEN35_DELTA_SCAN_ROWCACHE'
```

Captured normalized output:

```text
tokens/context: 128
iterations: 1
median_s: 0.010000000
p95_s: 0.020000000
effective_GB/s: 123.45
checksum: 9.5
```

The stdout included `1` for `CTOX_QWEN35_DELTA_SCAN_ROWCACHE`, proving
accepted-profile sourcing worked. The lock directory was removed after exit.

Dry-run pack capture check:

```text
tools/run_measurement_pack.sh --dry-run --capture acceptance /tmp/ctox_qwen35_08b_real_fp16.metalpack
  measurement_pack: acceptance
  capture: 1
  command: ... sweep_metalpack_prefill_delta_autotune ... 512,4096,16384 3 1 18 0 2
```

Cleaned up the temporary smoke capture directory and reran:

```text
bash -n tools/*.sh
tools/kernel_dev_doctor.sh
  required_files: 53
  validation: PASS
```

Methodology impact: real benchmark runs now have a single safe entry point that
serializes execution and preserves raw output plus normalized evidence fields.
This closes a major reproducibility gap before the next actual performance
push.

## 2026-04-30 18:15 CEST - Measurement Records

Added first-class measurement records that link captured run directories to
experiments.

Added:

```text
docs/kernel-dev/MEASUREMENT_RECORD_TEMPLATE.md
docs/kernel-dev/measurements/README.md
docs/kernel-dev/measurements/INDEX.md
tools/new_measurement_record.sh
tools/validate_measurement_record.sh
tools/list_measurement_records.sh
tools/update_measurement_index.sh
```

Integrated into:

```text
tools/kernel_dev_doctor.sh
docs/kernel-dev/README.md
KERNEL_DEV_HANDBOOK.md
README.md
```

Measurement records store paths into a captured run directory:

```text
capture_dir
manifest
stdout
stderr
normalized
exit_code_file
```

and copy small summary fields:

```text
command
accepted_profile_hash
git_commit
git_dirty_state
tokens/context
iterations
warmup
median_s
p95_s
effective_GB/s
checksum
output_csv
measurement_kind
usable_for_decision
```

Strict validation requires:

```text
referenced files exist
exit_code == 0
tokens/context numeric
iterations numeric
median_s numeric
p95_s numeric
command/profile/git metadata present
usable_for_decision is yes or no
```

Smoke test used a harmless shell command, not a GPU benchmark:

```text
tools/new_kernel_experiment.sh smoke-measurement-record /tmp/ctox_qwen35_08b_real_fp16.metalpack
tools/capture_measurement_output.sh --accepted-profile --output-dir /tmp/ctox_qwen35_measurement_record_smoke --label smoke-measurement -- sh -c 'echo "tokens: 256"; echo "iterations: 2"; echo "median_s: 0.030000000"; echo "p95_s: 0.040000000"; echo "checksum16: 7.25"'
tools/new_measurement_record.sh docs/kernel-dev/experiments/<smoke>.md /tmp/ctox_qwen35_measurement_record_smoke/<run> smoke smoke-measurement-record
tools/validate_measurement_record.sh docs/kernel-dev/measurements/<smoke>.md
  validation: PASS
tools/validate_measurement_record.sh --strict docs/kernel-dev/measurements/<smoke>.md
  validation: PASS
```

The measurement itself is classified as `smoke` and `usable_for_decision: no`.
That is deliberate: strict measurement validation only proves the captured run
is internally well-formed, not that it is sufficient for promotion.

Cleaned up temporary experiment, measurement record, env dump, and capture
directory, regenerated indexes, then reran:

```text
tools/kernel_dev_doctor.sh
  required_files: 60
  experiments: 0
  measurements: 0
  validation: PASS

tools/kernel_dev_doctor.sh --strict-experiments
  experiments_failed_strict: 0
  measurements_failed_strict: 0
  decisions_failed_strict: 0
  forensics_failed_strict: 0
  autotune_failed_strict: 0
  profile_updates_failed_strict: 0
  validation: PASS
```

Methodology impact: captured benchmark output now has a durable record layer.
Future experiments can point to exact raw stdout/stderr/manifest files while
validators check that the referenced capture is still present and well-formed.
## 2026-04-30 19:11 CEST - Roofline-First Tooling Rule

User correction accepted and documented: a Metal kernel cannot be optimized
without knowing the current hardware's measured compute and memory limits.
Raw milliseconds are insufficient. Every serious prefill/decode optimization
must compare each hot operator against a local roofline and classify the gap.

Methodology rule added to:

```text
KERNEL_DEV_HANDBOOK.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
docs/kernel-dev/CACHE_FORENSICS_CHECKLIST.md
```

Required local roofline capture:

```text
tools/capture_roofline_baseline.sh --output-dir /tmp/ctox_qwen35_roofline_<date>
```

Required per-op gap fields:

```text
sustained_stream_GB_s
operational_prefill_matmul_GB_s
operational_matvec_GB_s
modeled_bytes
effective_GB/s
bandwidth_utilization
time_vs_floor
reported_effective_vs_modeled
traffic_vs_model, if hardware counter bytes exist
classification
next_probe
```

Current local roofline smoke capture:

```text
roofline_dir: /tmp/ctox_qwen35_roofline_current
sustained_stream_GB_s=81.600000
operational_prefill_matmul_GB_s=85.010000
operational_matvec_GB_s=11.070000
```

Current prefill p4096 forensics against the measured stream roof:

```text
delta18+ffn:
  median_ms: 1655.837
  effective_GB/s: 36.00
  bandwidth_utilization: 44.1%
  time_vs_floor: 2.50x
  classification: bandwidth-underutilization

attention.core:
  median_ms: 290.801
  effective_GB/s: 72.56
  bandwidth_utilization: 88.9%
  time_vs_floor: 1.24x
  classification: near-modeled-floor

attention.ffn:
  median_ms: 35.576
  effective_GB/s: 41.38
  bandwidth_utilization: 50.7%
  time_vs_floor: 2.17x
  classification: bandwidth-underutilization
```

Interpretation: the biggest actionable flaw is still Delta18+FFN prefill. It is
not merely "too many bytes"; it is also far below the measured memory roof for
its modeled traffic. Next work should use layout/chunk/dispatch autotuning and
operator breakdowns to explain why Delta18+FFN reaches only about 44% of the
measured stream roof while attention.core is much closer to its byte floor.

## 2026-04-30 19:18 CEST - Negative Learning Must Be Preserved

Methodology update: every kernel optimization learning must be recorded,
including dead ends. A rejected candidate is useful only if the record explains
which assumption failed and when the idea should or should not be retried.

Docs updated:

```text
KERNEL_DEV_HANDBOOK.md
docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
docs/kernel-dev/EXPERIMENT_TEMPLATE.md
docs/kernel-dev/README.md
docs/kernel-dev/BENCHMARK_PROTOCOL.md
README.md
```

Required fields for negative results:

```text
hypothesis
actual_result
failure_mode
root_cause
do_not_repeat
retry_only_if
docs_to_update
```

Examples of knowledge that must not be lost:

```text
MMA128 lowered modeled bytes but was slower due to likely register pressure.
qblk attention variants can reduce logical reuse but lose to occupancy or
scratch overhead.
Fused DeltaNet state-step variants that change token output are rejected even
if they appear faster.
Measurements captured while another benchmark is running are invalid evidence.
Literal zero cache misses is not a valid target for streamed weights; zero
avoidable misses against a modeled compulsory floor is the valid target.
```

## 2026-04-30 20:23 CEST - DeltaOut64 Accepted, Scan Lanes4 Rejected

Prefill optimization continued from the p4096 gap analysis. The new
phase-level profiler classified the Delta18+FFN phases against the local
`81.6 GB/s` stream roof:

```text
project:
  delta_ms: 556.750
  effective_GB/s: 40.14
  bandwidth_utilization: 49.2%
  time_vs_floor: 2.03x

scan+norm:
  delta_ms: 257.983
  effective_GB/s: 12.04
  bandwidth_utilization: 14.7%
  time_vs_floor: 6.78x

delta out:
  delta_ms: 213.650
  effective_GB/s: 93.29
  time_vs_floor: 0.87x

ffn down:
  delta_ms: 311.663
  effective_GB/s: 39.73
  bandwidth_utilization: 48.7%
  time_vs_floor: 2.05x
```

Tool added:

```text
tools/analyze_delta_profile_gaps.sh
```

QKV/Z tile sweep:

```text
MMA8:
  project_ms: 1554.052
  full_median_s: 2.636987208

MMA16:
  project_ms: 928.238
  full_median_s: 2.085752709

MMA32:
  project_ms: 669.937
  full_median_s: 1.816224500

MMA64:
  project_ms: 565.970
  full_median_s: 1.716559542

MMA128:
  project_ms: 555.806
  full_median_s: 1.690021250
```

Learning: qkvz MMA8/MMA16 are clear losers and are now included in the
autotuner as negative controls. MMA128 can look slightly faster in an isolated
profile, but coordinate autotune still selected MMA64 for p4096.

Autotune p4096:

```text
baseline:
  qkvz=mma64, delta_out=mma32_res, gate_up=mma64,
  down=mma64_res, scan=rowcache, conv=fused
  median_s: 1.641508

best:
  qkvz=mma64, delta_out=mma64_res, gate_up=mma64,
  down=mma64_res, scan=lanes4, conv=fused
  median_s: 1.527726583
  best_tok_s_prefill_delta_stack_only: 2681.11

correctness_gate:
  FAIL
  mean_abs: 0.001899509
  rms: 0.002485653
  max_abs: 0.062500000
  checksum_delta: -16.460019886
```

Decision: reject `scan=lanes4` as a default despite speed. It is a useful
diagnostic for the `scan+norm` bottleneck, but it changes hidden state too much.

Isolated `delta_out=mma64` hidden-dump check:

```text
mismatch_count: 0
mean_abs_error: 0.000000000
rms_error: 0.000000000
max_abs_error: 0.000000000
checksum_delta: 0.000000000
```

Accepted-profile token sweep with `CTOX_QWEN35_PROJECT_SPLIT_NORM=1`:

```text
512 tokens:
  baseline median_s:    0.203536083
  delta_out64 median_s: 0.200383125

4096 tokens:
  baseline median_s:    1.673760625
  delta_out64 median_s: 1.652849458

16384 tokens:
  baseline median_s:    5.741555125
  delta_out64 median_s: 5.623674458
```

Invalid measurement recorded: an earlier sweep omitted
`CTOX_QWEN35_PROJECT_SPLIT_NORM=1`, so it is not comparable to the accepted
profile and must not be used as promotion evidence.

Accepted profile changed:

```text
old:
  CTOX_QWEN35_DELTA_OUT_MMA32=1
  CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1

new:
  CTOX_QWEN35_DELTA_OUT_MMA64=1
```

Validation:

```text
tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  active_flags: 9
  sha256: d19c0c12b3508a318595c3b55f2a75615f7245e880f1595230ab3fb9d6998512

tools/run_accepted_profile.sh target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 3 1 18
  median_s: 1.397042291
  p95_s: 1.397881500
  effective_gb_s_delta_ffn_stack_estimate: 39.64
  checksum16: -0.910950
```

Updated forensics after DeltaOut64:

```text
full_prefill_estimate_current_kernels:
  3.391s
  1207.84 tok/s

delta18+ffn:
  median_ms: 1398.792
  effective_GB/s: 39.59
  bandwidth_utilization: 48.5%
  time_vs_floor: 2.06x

attention.core:
  median_ms: 302.930
  effective_GB/s: 69.66
  bandwidth_utilization: 85.4%
  time_vs_floor: 1.17x
```

Next target: not DeltaOut. The remaining optimization pressure is project,
scan+norm correctness-preserving math, and FFN/down underutilization. Scan is
the highest `time_vs_floor` gap, but the fast existing lanes4 path is not
correct enough; the next scan work needs a correctness-preserving algorithmic
change, not looser thresholds.

## 2026-04-30 20:34 CEST - Lanes4 Ordered Scan Rejected

Hypothesis:

```text
Keep the low-register `lanes4` state layout, but reduce dot products in the
same col=0..127 order as rowcache by writing per-column products into
threadgroup memory and letting tx==0 sum them sequentially.
```

Expected:

```text
less hidden drift than simd_sum lanes4
possibly better occupancy than rowcache
```

Implementation:

```text
kernel:
  qwen35_08b_prefill_deltanet_scan_lanes4_ordered_f32_state_tok_h16d128

flag:
  CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED=1
```

Correctness check against accepted rowcache at p4096:

```text
mismatch_count: 3741948
mean_abs_error: 0.001893514
rms_error: 0.002478927
max_abs_error: 0.046875000
checksum_delta: -19.538155496
first_mismatch_token: 0
first_mismatch_col: 1
```

Runtime:

```text
rowcache:
  median_s: 1.399820292
  effective_gb_s_delta_ffn_stack_estimate: 39.56
  checksum16: -0.910950

lanes4_ordered:
  median_s: 2.508549750
  effective_gb_s_delta_ffn_stack_estimate: 22.08
  checksum16: -0.910278
```

Decision: reject as default and as a promising scan path. It is both slower and
incorrect against the hidden-dump gate. Keep the flag as a negative control in
the autotuner/search space for now, but do not spend more time on this exact
ordered-threadgroup-memory strategy unless a lower-barrier design changes the
cost model.

Learning:

```text
wrong_assumption:
  ordered final summation would make lanes4 close enough to rowcache

actual_result:
  first-token drift remains and barriers/threadgroup scratch make it much slower

do_not_repeat:
  do not try to fix lanes4 correctness by adding per-token threadgroup scratch
  and serial tx==0 reductions

retry_only_if:
  a new scan formulation avoids both simd_sum drift and per-token barrier-heavy
  scratch reductions
```

## 2026-04-30 20:43 CEST - Cache Model Fixed for DeltaOut64

Problem:

```text
The accepted profile had been promoted to DeltaOut64, but cache_analysis and
memory_forensics still described delta.out_proj as MMA16 / out=32 in parts of
the byte model.
```

Change:

```text
src/cache_model.rs:
  added active_delta_out_token_tile()
  delta.out_proj now reports prefill_deltanet_out MMA64 residual when the
  accepted profile is sourced

src/bin/memory_forensics.rs:
  delta18+ffn byte buckets now report out=64 and groups out=64@64

tools/analyze_delta_profile_gaps.sh:
  rows that imply >115% of sustained roofline are classified as
  byte-model-overcount-or-roof-mismatch instead of being hidden as near floor
```

Validation:

```text
cargo test --release cache_model
  2 passed

tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  active_flags: 9
  sha256: d19c0c12b3508a318595c3b55f2a75615f7245e880f1595230ab3fb9d6998512
```

Updated p4096 memory forensics with sustained_stream_GB_s=81.6:

```text
delta18+ffn:
  median_ms: 1363.534
  effective_GB/s: 40.61
  bandwidth_utilization: 49.8%
  time_vs_floor: 2.01x

attention.core:
  median_ms: 323.364
  effective_GB/s: 65.26
  bandwidth_utilization: 80.0%
  time_vs_floor: 1.25x

attention.ffn:
  median_ms: 29.234
  effective_GB/s: 50.36
  bandwidth_utilization: 61.7%
  time_vs_floor: 1.62x

full_prefill_estimate_current_kernels:
  3.466s
  1181.88 tok/s
```

Delta phase gaps after the fixed model:

```text
project:
  delta_ms: 461.880
  bandwidth_utilization: 59.3%
  time_vs_floor: 1.69x

conv/split+ba:
  delta_ms: 27.430
  bandwidth_utilization: 254.3%
  classification: byte-model-overcount-or-roof-mismatch

scan+norm:
  delta_ms: 252.414
  bandwidth_utilization: 15.1%
  time_vs_floor: 6.63x

delta out:
  delta_ms: 116.744
  bandwidth_utilization: 57.1%
  time_vs_floor: 1.75x

ffn norm+gate/up:
  delta_ms: 322.080
  bandwidth_utilization: 90.0%
  time_vs_floor: 1.11x

ffn down:
  delta_ms: 208.553
  bandwidth_utilization: 72.8%
  time_vs_floor: 1.37x
```

Learning:

```text
do_not_trust:
  any roofline conclusion unless the byte model reflects the active accepted
  profile flags

next_target:
  scan+norm is the dominant phase-level outlier; the issue is likely register
  pressure, barriers, or serial recurrence rather than raw DRAM bytes

model_gap:
  conv/split+ba currently overcounts bytes or the prefix-profile boundary is not
  clean enough; do not optimize from that row until the bucket is repaired
```

## 2026-04-30 20:52 CEST - Scan Fusion/Direct-Load Candidates Rejected

Candidate 1: `rowcache_gated_norm`

```text
flag:
  CTOX_QWEN35_DELTA_SCAN_GATED_NORM=1

idea:
  fuse rowcache scan output with gated RMSNorm to remove a separate norm
  dispatch and delta scratch write/read

correctness:
  hidden dump compare at p4096 vs accepted rowcache:
    mismatch_count: 0
    mean_abs_error: 0
    rms_error: 0
    max_abs_error: 0
    checksum_delta: 0

performance p4096/p3:
  accepted rowcache median_s: 1.363142500
  rowcache_gated_norm median_s: 1.403827792
```

Decision: keep as an autotune candidate and correctness-preserving reference,
but do not promote. Fusion removed traffic but increased scan-loop cost enough
to lose overall.

Candidate 2: `rowcache_direct`

```text
flag:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT=1

idea:
  preserve row_state[128] and the same col order, but remove q_s/k_s
  threadgroup arrays and per-token barriers by reading Q/K directly from device
  memory inside the row loop

correctness p4096:
  mismatch_count: 3738849
  mean_abs_error: 0.001881573
  rms_error: 0.002464169
  max_abs_error: 0.054687500
  checksum_delta: -10.097489536
  first_mismatch_token: 1
  first_mismatch_col: 1

performance p4096/p1:
  accepted rowcache median_s: 1.375254459
  rowcache_direct median_s: 1.476861167
```

Decision: reject as default and optimization path. Keep only as a negative
control in the autotuner while the correctness gate remains active.

Learning:

```text
wrong_assumption:
  removing the q_s/k_s threadgroup barrier while keeping row_state order would
  preserve stack-level equality

actual_result:
  direct Q/K loads change enough arithmetic behavior to drift over the stack and
  are slower despite removing barriers

do_not_repeat:
  do not trade shared Q/K staging for direct per-row Q/K reloads in this scan
  formulation

next_scan_work:
  search for a mathematically equivalent recurrence/blocking transformation,
  not another low-level Q/K staging variant
```

## 2026-04-30 21:10 CEST - QKV/Z MMA128 Promoted

Finding:

```text
The extended serial autotune run exposed two things:
  1. lanes4 is still the fastest scan candidate, but still fails hidden-dump
     correctness and must not be promoted.
  2. QKV/Z MMA128 is a correctness-preserving integrated win.
```

Tooling fix:

```text
autotune default bug:
  delta_out default still pointed at mma32_res after DeltaOut64 had been
  promoted

fix:
  autotune family defaults now match accepted profile:
    QKV/Z MMA128
    DeltaOut64
    GateUp64
    Down64 residual
    rowcache scan
    fused conv/split
```

Correctness gate for QKV/Z MMA128:

```text
p4096 final hidden dump vs previous accepted profile:
  mismatch_count: 0
  mean_abs_error: 0
  rms_error: 0
  max_abs_error: 0
  checksum_delta: 0
```

Integrated p4096/p3:

```text
accepted before:
  median_s: 1.363063958
  p95_s: 1.363883125

QKV/Z MMA128:
  median_s: 1.325504959
  p95_s: 1.326096916

median_delta:
  -2.76%
```

Token sweep:

```text
512:
  accepted: 0.175855875
  qkvz128: 0.174649959

4096:
  accepted: 1.363780708
  qkvz128: 1.326008083

16384:
  accepted: 5.484693833
  qkvz128: 5.283299125
```

Accepted profile:

```text
changed:
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
to:
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128=1

new accepted profile hash:
  2ffc10a1422b9ea1581b7e1ddb40e2f168540598ad540a8c02e8f7b015bb702c
```

Updated p4096 forensics after QKV/Z128:

```text
delta18+ffn:
  median_ms: 1326.713
  effective_GB/s: 34.46
  model_bytes: 42.58 GiB
  time_vs_floor: 2.37x

full_prefill_estimate_current_kernels:
  3.435s
  1192.52 tok/s
```

Learning:

```text
accepted_win:
  larger token tile can win even when apparent effective GB/s decreases,
  because the byte model itself changes

do_not_repeat:
  do not let autotuner defaults drift from accepted_profile.env

next_target:
  scan+norm still dominates time_vs_floor at 6.36x; project remains 2.72x
  after the QKV/Z128 byte model update
```

## 2026-04-30 21:18 CEST - Autotune Default Drift Guard Added

Problem:

```text
Autotune defaults drifted from accepted_profile.env after DeltaOut64 was
promoted. The autotuner then reported an outdated accepted_selection even
though docs/kernel-dev/accepted_profile.env had already moved on.
```

Change:

```text
target/release/autotune_metalpack_prefill_delta_stack --print-baseline-env
  prints the internal conservative baseline flags without running benchmarks

tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env
  compares the autotuner-managed flags against accepted_profile.env

tools/kernel_dev_doctor.sh
  runs the drift guard when the autotune binary exists
```

Validation:

```text
tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  autotune_baseline_flags: 8

tools/kernel_dev_doctor.sh
  validation: PASS
  required_files: 65
```

Learning:

```text
do_not_repeat:
  do not rely on comments or manual memory to keep accepted profile and
  autotuner defaults synchronized

tooling_rule:
  any future accepted-profile promotion that touches autotuner-managed flags
  must pass check_autotune_defaults.sh
```

## 2026-04-30 21:45 CEST - Row-Blocked Rowcache Scan Candidates

Hypothesis:

```text
Keep rowcache arithmetic and col=0..127 summation order, but split each head
across smaller row blocks:
  rowcache_block64: 2 threadgroups/head, 64 rows per TG
  rowcache_block32: 4 threadgroups/head, 32 rows per TG

Expected effect:
  lower per-TG register pressure / occupancy pressure while preserving
  stack-level equality

Risk:
  duplicate Q/K staging and more threadgroups may erase the gain
```

Implementation:

```text
kernels:
  qwen35_08b_prefill_deltanet_scan_rowcache_block64_f32_state_tok_h16d128
  qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128

flags:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64=1
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1

status:
  opt-in autotune candidates only
```

Correctness:

```text
p4096 final hidden dump vs accepted rowcache:
  block64 mismatch_count: 0
  block64 max_abs_error: 0
  block64 checksum_delta: 0

  block32 mismatch_count: 0
  block32 max_abs_error: 0
  block32 checksum_delta: 0
```

Performance observations:

```text
block64 p4096/p7:
  accepted median_s: 1.325925042
  accepted p95_s: 1.327324417
  block64 median_s: 1.324675375
  block64 p95_s: 1.325769083

block64 sweep p2:
  512 accepted/block64:   0.174821666 / 0.174076333
  4096 accepted/block64:  1.326667500 / 1.324679500
  16384 accepted/block64: 5.300495333 / 5.275489583

block32 p4096/p7:
  accepted median_s: 1.326505584
  accepted p95_s: 1.342337583
  block32 median_s: 1.319652125
  block32 p95_s: 1.319816084

block32 sweep p2:
  512 accepted/block32:   0.175626042 / 0.174159750
  4096 accepted/block32:  1.497307750 / 1.561966000
  16384 accepted/block32: 6.010822584 / 5.934084083
```

Decision:

```text
do_not_promote_yet:
  block64 is correct and consistently slightly faster, but p4096 gain is only
  around 0.1-0.2%, below the promotion threshold

  block32 is correct and can be faster, but the token sweep showed an explicit
  p4096 regression in one run; it needs stability work before consideration

keep:
  both as opt-in candidates in the autotuner
```

Learning:

```text
actual_result:
  row blocking preserves correctness, unlike lanes4/direct variants

root_cause:
  inferred - smaller row blocks reduce per-TG pressure but duplicate Q/K
  staging and increase TG scheduling overhead

do_not_repeat:
  do not promote sub-percent scan wins without p95-stable multi-token evidence

next_scan_work:
  row blocking is a valid correctness-preserving axis, but not enough by itself;
  investigate double-buffered Q/K staging or recurrence-level chunking next
```

## 2026-04-30 22:05 CEST - Rowcache Block32 Promoted After Paired Sweep

Problem:

```text
The initial block32 evidence was contradictory:
  - p4096/p7 candidate-first looked clearly faster
  - a later unpaired token sweep showed a p4096 regression

This is exactly the kind of sub-percent candidate where run order, thermal
state, and scheduler variance can dominate a normal baseline-then-candidate
sweep.
```

Tooling added:

```text
tools/compare_delta_stack_candidate.sh
  alternates accepted-profile and candidate runs
  records per-run median/p95/checksum rows in CSV
  reports paired median deltas per token count

autotune checksum guard:
  CTOX_QWEN35_AUTOTUNE_COORD_MAX_CHECKSUM_DELTA
  default: 0.0001
  rejects coordinate candidates whose stack checksum drifts

bench output clarity:
  DeltaNet+FFN benches now print both project_tokens and qkvz_tokens so
  accepted QKV/Z128 is visible in the run header.
```

Paired block32 sweep:

```text
command:
  tools/compare_delta_stack_candidate.sh \
    --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1 \
    --tokens 512,4096,16384 \
    --rounds 2 \
    --iterations 2

512:
  baseline_median_s: 0.190638166
  candidate_median_s: 0.189972708
  median_delta_percent: -0.3491

4096:
  baseline_median_s: 1.493337584
  candidate_median_s: 1.484006187
  median_delta_percent: -0.6249

16384:
  baseline_median_s: 6.015355376
  candidate_median_s: 5.999566792
  median_delta_percent: -0.2625

checksum:
  unchanged at -0.910950
```

Promotion:

```text
accepted_env_added:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1

kept:
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
  as the parent rowcache family flag

accepted_profile_hash_before:
  2ffc10a1422b9ea1581b7e1ddb40e2f168540598ad540a8c02e8f7b015bb702c

accepted_profile_hash_after:
  2e63086c55ece30a62be5856d9c3f559aa3041f70be69db944cdb68561dfcc9a
```

Validation:

```text
tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  active_flags: 10

tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  autotune_baseline_flags: 9

bash -n tools/*.sh
tools/kernel_dev_doctor.sh
  validation: PASS
  required_files: 66

cargo test --release cache_model
  2 passed
```

Learning:

```text
actual_result:
  rowcache_block32 is a small but stable paired-order win once run-order bias is
  controlled, and it remains hidden-dump bitexact.

do_not_repeat:
  do not promote or reject sub-percent scan changes from unpaired sweeps

tooling_rule:
  any scan candidate below roughly 1% must use paired alternating comparison,
  checksum guard, and final hidden-dump comparison before profile promotion.
```

## 2026-04-30 22:25 CEST - Luce Prefill Lessons And QKV/Z RG4 A-Shared Rejection

External research:

```text
Luce separates the problem:
  prefill.cu:
    cuBLAS BF16 GEMMs plus standalone DeltaNet kernels
  kernel.cu:
    persistent decode megakernel
  prefill_megakernel.cu:
    Blackwell-only prefill megakernel path

Implication for Metal:
  do not treat prefill as the same optimization problem as batch-1 decode.
  Prefill needs chunk-parallel DeltaNet math and large GEMM-like phases, while
  decode needs persistent latency hiding.
```

Main transferable Luce prefill idea:

```text
chunked DeltaNet scan:
  phase 1:
    build per-chunk transformed recurrence terms
  phase 2:
    process state slices per head / j-split and update chunk outputs/state

Metal hypothesis:
  replace the serial rowcache scan+norm path with a two-phase chunk scan that
  exposes token parallelism and keeps state slices local enough for threadgroup
  memory/register reuse.
```

Quick Project-side experiment:

```text
candidate:
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED=1

kernel:
  qwen35_08b_prefill_matmul_mma128x8_rg4_ashared_fp16_tiled_k1024_f32

hypothesis:
  four row-groups per threadgroup share a staged A tile, reducing repeated
  q_half loads across QKV/Z row groups.

risk:
  threadgroup barriers and lower occupancy may outweigh the reduced A reloads.
```

Paired result:

```text
512:
  baseline_median_s: 0.195956042
  candidate_median_s: 0.201707562
  delta_percent: +2.9351

4096:
  baseline_median_s: 1.694889771
  candidate_median_s: 1.677932833
  delta_percent: -1.0005

16384:
  baseline_median_s: 6.839253979
  candidate_median_s: 7.079493792
  delta_percent: +3.5127

checksum:
  unchanged at -0.910950
```

Decision:

```text
rejected_as_default:
  QKV/Z RG4 A-shared

keep:
  opt-in negative/control candidate

reason:
  the only win was p4096, while both shorter and longer token lengths regressed.
  This is a pressure/barrier tradeoff, not a robust cache fix.
```

Next:

```text
primary:
  prototype a Metal two-phase chunked DeltaNet scan inspired by Luce prefill

secondary:
  evaluate 128-token residual MMA for FFN down / DeltaOut with paired sweeps
```

## 2026-04-30 22:40 CEST - OpenEvolve Tuning Lessons

External research:

```text
OpenEvolve's MLX Metal kernel example is useful mostly as a negative control
for automated kernel search.

The example targets Qwen3 GQA on Apple Silicon and explicitly lists the right
search dimensions:
  memory access patterns
  online/fused algorithms
  GQA-specific head grouping
  Apple SIMD/threadgroup behavior
  vectorization
  threadgroup memory hierarchy
  numerical stability

But after validity fixes the best evolved kernel was still 3.2% slower than
MLX baseline.
```

Failure lessons to import:

```text
do_not_repeat:
  do not optimize against an abstract combined_score
  do not run evolution without profiling/counter features
  do not trust subprocess/kernel hooks until proven active
  do not accept dtype-mismatched correctness gates
  do not let compilation failures dominate the candidate stream

required_for_ctox_autotune:
  direct speedup ratio against accepted profile
  p95 and variance penalty
  checksum plus hidden-dump/logit gate depending on blast radius
  cache/roofline features as evaluator inputs
  candidate family tags: tile, row grouping, staging, barriers, dtype, dispatch
  paired alternating measurement for small wins
  early compile/syntax filter before expensive benchmarks
```

Action:

```text
Keep our current deterministic autotuner, but evolve it toward a quality-
diversity search only after the evaluator emits real hardware/forensics
features. OpenEvolve without those features would likely rediscover the same
fragile, benchmark-specific regressions.
```

## 2026-04-30 22:55 CEST - Native Sparse Attention Applicability

External research:

```text
NSA combines:
  coarse compressed attention
  fine selected-block attention
  sliding-window attention

The reference implementation exposes:
  g_cmp
  g_slc
  g_swa
  block_counts
  block_size
  window_size

This is not a kernel-only rewrite of ordinary full attention. It is an
architecture/training-aware sparse attention mechanism.
```

Applicability to Qwen3.5-0.8B:

```text
accepted/bitexact path:
  NSA is not a drop-in replacement for Qwen3.5 full attention because the model
  was not trained with NSA gates and sparse routing. Replacing the 6 full
  attention layers would change model semantics.

performance research path:
  NSA-style ideas are still useful for an optional approximate long-context
  mode:
    sliding local window
    top-k selected KV blocks
    compressed KV summaries
    GQA-aware block batching

priority:
  not the main current prefill gap, because Qwen3.5-0.8B has only 6 full
  attention layers and Delta18+FFN remains the larger measured bottleneck.
```

Concrete CTOX experiment shape:

```text
name:
  attention_sparse_long_context_approx

first version:
  keep exact attention for short context and reference runs
  add opt-in sparse attention only for long context
  window_size: 256 / 512 / 1024
  selected_blocks: 8 / 16 / 32
  block_size: 16 / 32 / 64
  selection score:
    query dot compressed block key summary

gates:
  perf: attention.core time/floor at 8k, 32k, 128k
  quality/correctness: logits drift and greedy-token divergence vs full
  attention, not hidden-dump bitexactness

do_not_promote_to_accepted:
  unless the user explicitly accepts approximate semantics or a finetuned model
  exists for the sparse pattern.
```

Learning:

```text
Sparse attention is a model-level math trick, not just a memory-layout trick.
For CTOX it belongs in a separate approximate mode and a separate evaluation
track. It should not distract from the exact DeltaNet chunk-scan work needed
to close the current prefill gap.
```

## 2026-04-30 23:05 CEST - Long-Context Attention Optimization Taxonomy

User-provided research synthesis:

```text
exact / accepted-compatible:
  FlashAttention-style prefill attention
  Flash-Decoding / Split-K over KV length for decode
  PagedAttention-style KV memory management

approximate or model-semantics-changing:
  MInference / FlexPrefill dynamic sparse prefill
  NSA / selected-block / compressed / sliding attention
  H2O / StreamingLLM / SnapKV / PyramidKV / Quest / DuoAttention / LServe
  KV-cache pruning / retention policies

bandwidth-reducing but approximate unless calibrated:
  KIVI / KVQuant / TurboQuant-style KV quantization
```

CTOX classification:

```text
accepted-profile candidates:
  only exact kernels and exact memory-management changes:
    - FlashAttention-style tiled prefill attention
    - decode Split-K / Flash-Decoding equivalent
    - GPU-local KV layout / paging without changing visible attention set

separate long-context approximate mode:
  sparse attention, KV pruning, selected blocks, compressed KV summaries,
  streaming sinks, and low-bit KV quantization

quality gates for approximate mode:
  logits drift
  greedy-token divergence
  task-level regression where available
  long-context retrieval / needle checks
  speed vs quality Pareto curve
```

Practical priority for Qwen3.5-0.8B:

```text
current measured bottleneck:
  Delta18+FFN exact prefill, especially project and scan+norm

attention priority:
  short term:
    keep exact attention path
    use attention.core roofline only when it dominates at long context
  medium term:
    add approximate long-context attention mode with MInference/Quest/SnapKV-like
    page/block selection only after exact DeltaNet prefill work stops dominating
```

Do-not-repeat rule:

```text
Do not mix approximate sparse attention wins into llama.cpp/reference speed
comparisons unless the comparison is explicitly labeled approximate and includes
quality drift. Exact-reference benchmarks and approximate-long-context
benchmarks are different products.
```

## 2026-04-30 23:20 CEST - llama.cpp Prefill Strategy Transfer

Question:

```text
Can CTOX copy or learn the prefill strategy directly from llama.cpp instead of
only reading broader papers and megakernel repos?
```

Answer:

```text
yes, but copy the scheduling and math shape, not the generic ggml graph layer
wholesale.
```

Local llama.cpp source inspection:

```text
src/models/delta-net-base.cpp:
  build_delta_net() routes by token count and backend capability:
    n_tokens == 1:
      fused_gdn_ar or autoregressive
    n_tokens > 1:
      fused_gdn_ch or chunking

  build_delta_net_chunking() uses a real chunked prefill algorithm:
    pad token axis to chunk size
    reshape Q/K/V/beta/gate by chunk
    build cumulative decay terms
    build chunk-local K/B and K/Q products
    solve a lower-triangular system inside each chunk
    update recurrent state chunk by chunk

src/llama-batch.cpp and memory code:
  n_ubatch is a physical batch limit, not just a user-facing prompt length.
  recurrent/hybrid memory prepares recurrent batches and attention cache with
  dedicated split policies.

ggml-metal:
  FlashAttention has separate pad/block/core/vector/reduce variants and
  function-constant specialization.
```

CTOX transfer rules:

```text
directly transferable:
  1. split prefill and decode strategies
  2. use token-count routing: n_tokens==1 autoregressive, n_tokens>1 chunked
  3. tune physical prefill uBatch/chunk size separately from logical context
  4. add capability gates for fused/chunked DeltaNet kernels
  5. model recurrent state and attention KV cache as different memory systems
  6. use FlashAttention-style specialized attention variants, not one generic
     attention kernel

not directly transferable:
  copying ggml graph execution as the core CTOX architecture, because CTOX's
  intended advantage is hardcoded Qwen3.5 layout and fixed-shape kernels.
```

Why this matters:

```text
The current CTOX prefill gap is structural. The current rowcache scan is still
too serial for prompt processing. llama.cpp's chunked/fused Gated DeltaNet path
is exactly the class of missing algorithmic change.
```

Next implementation consequence:

```text
continue the exact chunked DeltaNet prototype:
  phase1:
    per chunk/head Kdot, beta-weighted lower terms, decay prefix
  phase2:
    chunk-local solve/scan and state transform
  phase3:
    state propagation across chunks and output reconstruction

add uBatch/chunk autotune:
  token lengths: 512 / 4096 / 16384 / 32768+
  chunk sizes: 8 / 16 / 32 / 64
  reject candidates with hidden/logit drift unless explicitly approximate
```

Related SwiftLM note:

```text
SwiftLM is useful mainly as an Apple-runtime hygiene reference: avoid
unnecessary command-buffer sync/flush behavior, keep CPU orchestration boring
and predictable, and treat TurboQuant/SSD-streaming style features as separate
long-context/big-model tracks rather than the first exact Qwen3.5 prefill fix.
```

## 2026-04-30 23:35 CEST - OpenEvolve Kernel Discovery Lessons

Source:

```text
https://huggingface.co/blog/codelion/openevolve-gpu-kernel-discovery
```

Important correction:

```text
OpenEvolve is inspiration for the optimization system, not a ready-made
Qwen3.5 DeltaNet prefill algorithm.
```

Useful concrete findings from the article:

```text
target:
  Qwen3 GQA attention on Apple Silicon against MLX scaled_dot_product_attention

reported method:
  25 generations
  population size 25
  5 islands
  bulletproof evaluator
  20 inference scenarios

kernel ideas:
  vec<T,8> loads/dots for 128-dim attention heads
  two-pass attention softmax:
    pass 1 max score
    pass 2 exp/sum + fused value accumulation
  GQA-specific KV-head mapping and coalesced layout

reported result:
  +12.5% average decode
  +14.4% average prefill
  100% numerical accuracy
  but with high variance and several regressions
```

CTOX learning:

```text
1. Use automated search only after the evaluator is strong enough.
2. Score candidates across token/context classes, not one hand-picked run.
3. Track regressions as first-class data, not as noise.
4. Include Metal command-buffer/memory failure statistics in candidate reports.
5. Give the search engine hardware facts and current bottleneck facts:
     SIMD/vector width candidates
     threadgroup size candidates
     GQA/DeltaNet layout structure
     measured roofline gap
     correctness tolerance
6. Keep generated candidates env-gated and never promote from a single win.
```

Immediate CTOX dev-tool consequence:

```text
add an evolve/autotune candidate manifest format:
  candidate_id
  changed kernel/function
  intended bottleneck
  token/context sweep
  correctness max_abs/checksum/logit gate
  median/p95 speedup
  roofline/cache class
  Metal error stats
  accept/reject reason
```

Kernel consequence:

```text
For attention:
  add vec<T,8> attention probes and two-pass exact softmax probes.

For DeltaNet:
  use the same evolutionary method, but the candidate space must be DeltaNet
  specific: chunk size, state-slice layout, reduction tree, scratch lifetime,
  row/col state layout, and state propagation strategy.
```

Do-not-repeat:

```text
Do not let an evolutionary search mutate large runtime surfaces before the
benchmark harness catches Metal failures, numerical drift, p95 regressions, and
context-specific regressions.
```

## 2026-04-30 23:45 CEST - DeltaNet Chunk Phase2 Local-Zero Prototype

Implemented:

```text
kernel:
  qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_h16d128

benchmark:
  src/bin/bench_deltanet_chunk_phase2.rs

purpose:
  compute each chunk/head/value-row DeltaNet recurrence starting from zero
  state; write local token outputs and the chunk-local end state.
```

Correctness debug:

```text
first failure:
  max_abs_error_out:   0.022183886
  max_abs_error_state: 0.007293127

root cause:
  threadgroup reduction scratch lifetime bug. Some lanes could enter the next
  reduction and overwrite shared partials while other lanes had not yet read
  the previous broadcast value.

fix:
  add a post-read threadgroup barrier in the reduction helper.
```

Validated result:

```text
tokens: 128
chunks: 16
median_s: 0.005212166
p95_s: 0.005691750
max_abs_error_out:   0.000000063
max_abs_error_state: 0.000000007
```

Interpretation:

```text
This is a correctness milestone, not a performance win yet. The local-zero
kernel has too many row-level reductions and writes a large per-chunk state
surface. It proves the chunk-local recurrence and exposes the next work:
state propagation across chunks plus a less barrier-heavy state-slice layout.
```

## 2026-05-01 00:05 CEST - DeltaNet Chunk Phase3 State Propagation

Math verification:

```text
extended:
  src/bin/verify_deltanet_chunk_scan_math.rs

new check:
  local-zero chunk recurrence + propagated initial-state contribution must
  equal the serial recurrence.

result:
  tokens=65 dim=32 chunk=8
  max_composed_state_abs_error: 0.000000009
  max_composed_out_abs_error:   0.000000015
```

Metal implementation:

```text
kernel:
  qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_h16d128

input:
  Q/K, beta/decay, initial recurrent state, Phase2 local-zero outputs, Phase2
  local-zero chunk end states

algorithm:
  for each head/value-row:
    keep S_in row in 128 thread lanes
    for each chunk:
      run v=0 recurrence over the chunk to propagate only the incoming state
      add propagated token output to Phase2 local-zero output
      add Phase2 chunk-local end state to form next chunk's input state
```

Full-path correctness:

```text
tokens=128:
  phase2_median_s: 0.004112500
  phase3_once_s:   0.003647750
  max_abs_error_full_out:   0.000000071
  max_abs_error_full_state: 0.000000007

tokens=512:
  phase2_median_s: 0.007492583
  phase3_once_s:   0.006111292
  max_abs_error_full_out:   0.000000071
  max_abs_error_full_state: 0.000000007
```

Interpretation:

```text
This is the first complete Metal chunked DeltaNet recurrence prototype in the
probe. It is exact against CPU serial within fp32 tolerance, but it is not yet a
performance candidate:
  - Phase2 writes chunks * heads * 128 * 128 f32 state.
  - Phase3 still performs row-level reductions across chunks.
  - The current shape roughly doubles recurrence work to prove the composition.

The next optimization should reduce the materialized chunk-state surface and
merge/reshape Phase2+Phase3 so the propagated-state contribution does not
repeat the full row reduction work.
```

## 2026-05-01 00:20 CEST - DeltaNet Chunk Size Autotune Hook

Implemented:

```text
Phase2/Phase3 chunk size is now a benchmark parameter:
  target/release/bench_deltanet_chunk_phase2 <tokens> <iterations> <warmup> <chunk>

supported chunks:
  4 / 8 / 16 / 32
```

Reason:

```text
The chunked DeltaNet path has a direct memory tradeoff:
  smaller chunks:
    more chunk boundaries
    more materialized chunk end states
    more Phase3 transitions

  larger chunks:
    less chunk-state traffic
    more work per local recurrence
    potentially worse scheduling/barrier behavior
```

Serial sweep, full Phase2+Phase3 path:

```text
tokens=512:
  chunk=8:
    state_mb: 67.11
    full_path_median_s: 0.012217000
    max_abs_error_full_out: 0.000000071

  chunk=16:
    state_mb: 33.55
    full_path_median_s: 0.011158166
    max_abs_error_full_out: 0.000000071

  chunk=32:
    state_mb: 16.78
    full_path_median_s: 0.012462792
    max_abs_error_full_out: 0.000000071

tokens=2048:
  chunk=8:
    state_mb: 268.44
    full_path_median_s: 0.047108708
    max_abs_error_full_out: 0.000000080

  chunk=16:
    state_mb: 134.22
    full_path_median_s: 0.046141208
    max_abs_error_full_out: 0.000000082

  chunk=32:
    state_mb: 67.11
    full_path_median_s: 0.047882458
    max_abs_error_full_out: 0.000000084
```

Learning:

```text
chunk=16 is the current best tested full-path tradeoff. chunk=32 reduces the
state surface but loses enough elsewhere that total time regresses. This is a
good example of why layout/cache tuning must be empirical rather than assumed.

No accepted-profile change:
  this is still an isolated exact prototype, not integrated into the real
  Delta18+FFN path and not faster than the accepted rowcache scan.
```

## 2026-05-01 00:40 CEST - SIMD32x4 DeltaNet Schedule

External learning applied:

```text
Apple Silicon kernel optimization articles emphasize:
  think in SIMDgroups, not scalar threads
  use vectorized/lane-local multi-element work
  use simd_sum and simd shuffles before threadgroup scratch
  benchmark because theoretical wins can regress
```

Implemented schedule:

```text
state_mode:
  f32x4 / simd32x4

kernel changes:
  qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_f32state_h16d128
  qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_f32state_h16d128

mapping:
  one SIMDgroup per chunk/head/value-row
  32 lanes
  4 state columns per lane
  simd_sum reduces 128-wide row dots
  no threadgroup partial scratch
  no threadgroup barriers in the row reduction path
```

Control comparison, tokens=512, chunk=16:

```text
f32 scratch/barrier schedule:
  full_path_median_s: 0.014523250
  max_abs_error_full_out:   0.000000071
  max_abs_error_full_state: 0.000000007

f32x4 SIMD schedule:
  full_path_median_s: 0.003486292
  max_abs_error_full_out:   0.000000071
  max_abs_error_full_state: 0.000000007

f16 state-surface schedule:
  full_path_median_s: 0.011103375
  max_abs_error_full_out:   0.000015746
  max_abs_error_full_state: 0.000014251
```

Longer sweep, f32x4:

```text
tokens=2048:
  chunk=8:
    full_path_median_s: 0.027457291
    max_abs_error_full_out: 0.000000076

  chunk=16:
    full_path_median_s: 0.020094917
    max_abs_error_full_out: 0.000000075

  chunk=32:
    full_path_median_s: 0.017705625
    max_abs_error_full_out: 0.000000076
```

Learning:

```text
SIMDgroup-first is mandatory for this class of operation. The old 128-thread
scratch/barrier schedule was correct but structurally slow. The f32x4 schedule
keeps four state lanes per thread in registers and uses one warp-level
reduction, which is exactly the right shape for head_dim=128.

The best chunk size changed after the schedule changed:
  old scratch schedule: chunk=16 looked best
  f32x4 schedule: chunk=32 is currently best at 2048 tokens

This confirms that layout/schedule/autotune parameters cannot be tuned in
isolation.
```

## 2026-04-30 23:05 CEST - Hardware-First Correction And M5 Feature Gate

Correction:

```text
The optimization sequence so far was too kernel-first. Some variants were
useful, but the process did not put the concrete hardware limits and feature
surface early enough in the loop.

New rule:
  no accepted-profile promotion without hardware feature capture, local
  roofline, SIMD/MMA/tensor suitability, and byte/cache forensics.
```

Local hardware capture:

```text
tool:
  tools/capture_hardware_feature_matrix.sh /tmp/ctox_qwen35_hardware_current

result:
  model: MacBook Pro Mac17,2
  chip: Apple M5
  CPU cores: 10 total, 4 performance + 6 efficiency
  GPU: Apple M5, 10 cores
  memory: 32 GB unified
  macOS: 26.2 build 25C56
  Metal support: Metal 4
  CPU feature sysctls:
    FEAT_SME=1
    FEAT_SME2=1
    FEAT_BF16=1
    FEAT_I8MM=1
    FEAT_DotProd=1
    FEAT_FP16=1
    FEAT_FHM=1
  public Metal counter sets in this probe:
    GPUTimestamp only
```

Architecture interpretation:

```text
1. The DeltaNet f32x4 naming was misleading:
     it uses one 32-lane GPU SIMDgroup and four columns per lane.
     That covers the full 128-wide head row reduction.

2. The bigger missing track is M5 matrix/tensor acceleration:
     Apple's M5 has Neural Accelerators in the GPU cores and Metal 4 tensor
     APIs. Our MSL simdgroup_matrix kernels may be useful, but we do not yet
     have proof that they saturate the new M5 tensor/matrix hardware.

3. Quantization is now an accepted research direction, not a last-stage detail:
     quantized candidates may accept bounded error if they deliver real speed.
     They need separate drift gates instead of bitexact hidden-dump gates.

4. Cache-miss claims must be precise:
     this local Metal counter path exposes timestamps only, not hardware L2
     cache hit/miss counters. Current cache evidence is modeled bytes plus
     timing/roofline movement unless a named counter source is added.
```

Immediate backend split:

```text
MSL SIMDgroup path:
  DeltaNet recurrence, online reductions, sampling, exact custom fusions

Metal 4 tensor / MPS / MPSGraph probe path:
  large dense matmuls, FFN gate/up/down, projections, LM head, quantized
  matmul candidates

CPU SME path:
  packing, validation, coarse fallback probes only after roofline measurement
```

Stack SIMD/quantization smoke:

```text
accepted rowcache, p512, 18 Delta layers:
  full_median_s: 0.203165750
  scan+norm delta_ms: 37.928

f32x4 chunk scan, p512, chunk=32:
  env:
    CTOX_QWEN35_DELTA_SCAN_CHUNK_F32X4=1
    CTOX_QWEN35_DELTA_SCAN_CHUNK_TOKENS=32
  full_median_s: 0.195112875
  scan+norm delta_ms: 41.701

f16/hstate chunk scan, p512, chunk=32:
  env:
    CTOX_QWEN35_DELTA_SCAN_CHUNK_F32X4=1
    CTOX_QWEN35_DELTA_SCAN_CHUNK_HSTATE=1
    CTOX_QWEN35_DELTA_SCAN_CHUNK_TOKENS=32
  full_median_s: 0.187216792
  scan+norm delta_ms: 32.400
```

Decision:

```text
Do not promote yet.

Reason:
  this is a promising approximate/quantized stack candidate, but it needs
  hidden/logit drift measurement, multi-token sweep, paired alternating order,
  and a quality tolerance record.

Next:
  build a quantization acceptance gate and a Metal 4 tensor/MPS matmul probe
  for M5 matrix hardware, then compare against the current MSL MMA kernels.
```

## 2026-04-30 23:20 CEST - MPS Matrix Backend Probe

Implemented:

```text
tools/mps_matrix_probe.swift
tools/run_mps_matrix_probe.sh
```

Purpose:

```text
Compare Apple framework matrix throughput against handwritten MSL
SIMDgroup/MMA kernels before assuming that further MSL tuning is the best path
for large dense matmuls on M5.
```

MPS probe results:

```text
shape: 512 x 1024 times 1024 x 3584
command:
  tools/run_mps_matrix_probe.sh 512 3584 1024 5 2
median_s: 0.000775083
effective_tflops: 4.849

shape: 512 x 1024 times 1024 x 7168
command:
  tools/run_mps_matrix_probe.sh 512 7168 1024 10 3
median_s: 0.001183500
effective_tflops: 6.351

shape: 512 x 3584 times 3584 x 1024
command:
  tools/run_mps_matrix_probe.sh 512 1024 3584 10 3
median_s: 0.001071042
effective_tflops: 3.509
```

MSL comparison context:

```text
Gate/Up MSL fallback vs MSL MMA compare:
  command:
    target/release/bench_metalpack_prefill_gate_up_mma_compare \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

  baseline_median_s: 0.023468167
  mma_median_s:      0.002544875
  max_abs_error:     0.001953125
  mean_abs_error:    0.000025076

Down MSL fallback vs MSL MMA compare:
  command:
    target/release/bench_metalpack_prefill_down_mma_compare \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 5

  baseline_median_s: 0.006394333
  mma_median_s:      0.004879250
  max_abs_error:     0.000001848
  mean_abs_error:    0.000000044
```

Interpretation:

```text
The current MSL MMA kernels are useful and far better than the old SIMD
fallbacks, but MPS shows that the raw M5 matrix backend has materially more
headroom for Qwen-sized GEMMs.

This does not mean MPS should replace the whole pipeline immediately:
  MPS numbers are raw fp16 GEMMs, while the stack needs RMSNorm, SwiGLU,
  residuals, Qwen-specific layouts, quantized weights, and command-buffer
  composition.

But it does change the engineering direction:
  large dense matmuls need a backend shootout:
    handwritten MSL SIMD/MMA
    MPS/MPSGraph or Metal 4 tensor APIs
    quantized custom MSL with in-dot dequantization

Do not spend more time on bespoke MSL dense GEMM without proving why the Apple
matrix backend cannot be used for that shape/fusion.
```

## 2026-04-30 23:35 CEST - Quantized Candidate Error Gate

Implemented:

```text
tools/quant_error_gate.py
```

Purpose:

```text
Quantized and approximate kernels are allowed, but the tolerated error must be
explicit and checked by tooling. The old binary split between bitexact and
"not acceptable" is too restrictive for quantization, while unbounded error is
not acceptable for model-visible paths.
```

Smoke gate:

```text
measurement:
  /tmp/ctox_gate_up_mma_compare.txt

command:
  tools/quant_error_gate.py /tmp/ctox_gate_up_mma_compare.txt \
    --candidate-key mma_median_s \
    --max-abs 0.002 \
    --mean-abs 0.00005 \
    --speedup-min 2.0

result:
  max_abs_error: 0.001953125 <= 0.002
  mean_abs_error: 0.000025076 <= 0.00005
  speedup: 3.2425 >= 2.0
  validation: PASS
```

Policy:

```text
This gate is necessary but not sufficient.

For model-visible promotion, also require:
  hidden-dump drift
  logit drift
  greedy-token divergence
  prompt/task smoke if logits differ materially

The next quantization work should apply this gate to:
  f16/hstate DeltaNet chunk scan
  int8 weight-only projection/FFN candidates
  int4 weight-only projection/FFN candidates
```

## 2026-04-30 23:25 CEST - Backend Shootout And Quant Delta Gate Tools

Implemented:

```text
tools/run_matrix_backend_shootout.sh
tools/run_quant_delta_scan_gate.sh
```

Fix:

```text
bench_deltanet_chunk_phase2 now prints real mean_abs_error_* metrics, so
quantized DeltaNet candidates no longer use placeholder mean error.
```

Accepted-profile-aware matrix shootout:

```text
command:
  tools/run_matrix_backend_shootout.sh \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack \
    512 5 /tmp/ctox_qwen35_matrix_backend_accepted_current

MPS raw GEMM:
  gate/up single projection shape:
    median_s: 0.000744292
  gate+up combined shape:
    median_s: 0.001424208
  FFN down shape:
    median_s: 0.000887375
  Delta out shape:
    median_s: 0.000854291

MSL accepted-profile integrated probes:
  Gate/Up fallback:
    baseline_median_s: 0.023303167
  Gate/Up MSL MMA:
    mma_token_tile: 64
    mma_median_s: 0.002988042
    max_abs_error: 0.001953125
    mean_abs_error: 0.000025076
  FFN Down MSL MMA:
    mma_median_s: 0.004711042
    max_abs_error: 0.000001848
    mean_abs_error: 0.000000044
  DeltaOut active accepted path:
    token_tile: 64
    median_s: 0.000691667
```

Interpretation:

```text
DeltaOut64 is already competitive with MPS raw GEMM for this p512 shape.
Gate/Up and especially FFN Down still have substantial M5 matrix-backend
headroom. These are now backend-shootout targets, not blind MSL-tuning targets.

The shootout is not a direct replacement proof because MPS raw GEMM does not
include RMSNorm, SwiGLU, residual, or packed Qwen layout conversion. It is a
hardware lower-bound probe that prevents wasting time on underpowered MSL
matrix kernels.
```

Quantized Delta scan gate:

```text
command:
  tools/run_quant_delta_scan_gate.sh \
    2048 32 5 2 /tmp/ctox_qwen35_quant_delta_scan_mean_current

f32x4:
  state_mb: 67.11
  full_path_median_s: 0.015587375
  max_abs_error_full_out: 0.000000076
  mean_abs_error_full_out: 0.000000005

f16x4:
  state_mb: 33.55
  full_path_median_s: 0.014116083
  max_abs_error_full_out: 0.000016058
  mean_abs_error_full_out: 0.000000371

quant gate:
  max_abs_error <= 0.00002: pass
  mean_abs_error <= 0.000001: pass
  speedup >= 1.01: pass, speedup=1.1042
```

Decision:

```text
f16x4 Delta scan remains a candidate, not accepted.

Reason:
  isolated gate now passes, but previous stack smoke was mixed and the real
  promotion target is Delta18+FFN with hidden/logit drift. Need paired
  alternating stack sweep at p512/p4096/p16384 before promotion.
```

## 2026-04-30 23:35 CEST - Static Quantization Pipeline Rule

User correction:

```text
Quantization error must be accepted when it buys speed, but quantization must
not be implemented as conversion churn.

Bad pattern:
  f32 -> f16 -> f32 in the hot path
  q4 -> full dequant tensor -> f16 every token
  per-token/per-dispatch weight or cache requantization

Target pattern:
  static packed weights/cache/state
  fixed runtime dtype contract
  lane-local or tile-local unpack/dequant only where mathematically necessary
  no materialized full dequant tensors
```

Implemented process guard:

```text
docs/kernel-dev/QUANT_PIPELINE_TEMPLATE.md
tools/validate_quant_pipeline.py
```

Consequence for current candidates:

```text
f16x4 Delta chunk scan:
  useful diagnostic and maybe an opt-in approximate path
  not a clean final quantization architecture because local_state is stored as
  half but phase3 immediately converts half -> float inside the recurrence

real next quantization targets:
  static int8/int4 weight packs for FFN/projection/LM-head families
  kernels that consume packed weights directly
  quality-budgeted logit/token/task gates

SME note:
  no Accelerate/SME path exists in the tree yet. SME is CPU-side and should be
  benchmarked separately for static packing/quantization or coarse fallback,
  not assumed to accelerate the GPU token hot path.
```

## 2026-04-30 23:45 CEST - Static Quantized Metalpack Layouts

Implemented:

```text
src/pack_plan.rs:
  PackLayout::Int8RowTiled
  PackLayout::Int4GroupwiseRowTiled
  QuantScheme::Int8Symmetric
  QuantScheme::Int4GroupwiseSymmetric
  quant_scheme / quant_group_size on PackEntry

src/metalpack.rs:
  quant_scheme / quant_group_size on MetalPackEntry
  manifest fields:
    quant_scheme
    quant_group_size
    quant_scale_dtype
    quant_value_bits
  static quantized writer:
    int8_row_tiled
    int4_groupwise_row_tiled

src/bin/inspect_artifacts.rs:
  prints quant scheme and group size in pack plan sample
```

Static format:

```text
For each row/col-tile quant group:
  scale:
    f16

  int8_row_tiled:
    int8 values, one byte per value

  int4_groupwise_row_tiled:
    signed int4 values, two values per byte
```

Why this matters:

```text
Quantization now has an artifact contract. An int8/int4 candidate can no longer
pretend to be fp16_row_tiled or silently rely on a runtime conversion pass.

The next quantized kernels must consume these layouts directly. Existing FP16
kernels should continue to reject quantized layouts until they have explicit
matching decode/unpack logic.
```

Validation:

```text
cargo test --release --lib:
  13 passed

new unit test:
  metalpack::tests::writes_static_int8_quantized_row_tiled_payload
```

## 2026-04-30 23:55 CEST - Static Int8 Matmul Probe

Implemented:

```text
vendor/metal/shaders/qwen35_08b/prefill_matmul_int8.metal
src/bin/bench_static_int8_matmul.rs
```

Purpose:

```text
First kernel that consumes the new static `int8_row_tiled` format directly:
  input activations: half
  packed weights: f16 scale + int8 values per row/col-tile group
  dequantization: only inside the dot product
  no materialized full dequant tensor
  no per-token requantization
```

Results:

```text
naive one-output-threadgroup schedule:
  command:
    target/release/bench_static_int8_matmul 512 3584 5 2
  median_s: 0.082062292
  weight_compression_ratio: 1.9845

row-tile threadgroup schedule:
  command:
    target/release/bench_static_int8_matmul 512 3584 5 2
  median_s: 0.031375041
  weight_compression_ratio: 1.9845
```

Decision:

```text
Rejected as a performance path.

Reason:
  Static quantization format is correct direction, but this MSL schedule is far
  too slow. It saves weight bytes but does not hit M5 matrix/tensor hardware and
  pays scalar dequant/reduction overhead.

Learning:
  int8/q4 only helps if the compute schedule is also hardware-native. For large
  matmuls, the next attempt should be:
    MPS/Metal 4 tensor backend if it supports the required quantized format, or
    a much more matrix-shaped SIMDgroup kernel, not one output row per
    threadgroup-family scheduling.
```

## 2026-04-30 23:49 CEST - Quant Group Payload Contract Fixed

Problem:

```text
The static quantized manifest recorded quant_group_size, but the writer still
emitted one scale per full col_tile. That made metadata and payload disagree
whenever quant_group_size != col_tile, especially for int4 groupwise layouts.
Any later Q4/INT8 kernel autotune would have benchmarked the wrong format.
```

Implemented:

```text
src/pack_plan.rs:
  int8 default quant_group_size is now explicit 256 instead of 0
  accepted env group sizes: 32 / 64 / 128 / 256 for int8, 32 / 64 / 128 for int4

src/metalpack.rs:
  write_quantized_row_tiled now writes scales and quantized values per
  quant_group_size inside each col_tile
  validates quant_group_size > 0
  validates quant_group_size <= col_tile
  validates col_tile % quant_group_size == 0
  validates int4 quant_group_size is even

vendor/metal/shaders/qwen35_08b/prefill_matmul_int8.metal:
  int8 probe kernel now consumes quant_group_size and computes the correct
  per-group scale/value offset

src/bin/bench_static_int8_matmul.rs:
  added optional quant_group_size argument
```

Validation:

```text
cargo fmt
cargo test --release --lib
  15 passed

cargo build --release --bin bench_static_int8_matmul

target/release/bench_static_int8_matmul 512 3584 3 1 256
  median_s: 0.048962542
  weight_compression_ratio: 1.9845

target/release/bench_static_int8_matmul 512 3584 3 1 64
  median_s: 0.049326125
  weight_compression_ratio: 1.9394

tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  PASS, hash 2e63086c55ece30a62be5856d9c3f559aa3041f70be69db944cdb68561dfcc9a

tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env
  PASS

tools/kernel_dev_doctor.sh
  PASS
```

Decision:

```text
Promote the static quant payload contract fix.

Do not promote the int8 matmul probe as a performance path. The corrected
kernel remains far slower than the FP16/MMA candidates; the useful result is
format correctness and a negative control for scalar in-dot dequant schedules.

Next quantized-performance attempt must be matrix-shaped from the start:
  MPS/Metal 4 tensor backend if it can consume the format,
  or a SIMDgroup/MMA-style kernel with packed int8/q4 loads and no per-row
  threadgroup reduction pattern.
```

## 2026-04-30 23:56 CEST - Hardware Backend Grid and p4096 Matrix Shootout

User correction:

```text
Optimization must be hardware-grid driven:
  1. know the exact platform
  2. prove which hardware features are actually fast for each op/quant format
  3. optimize kernels against the theoretical/backend limit
  4. treat layout as part of feeding prefetch/speculative access correctly
```

Captured platform:

```text
tools/capture_hardware_feature_matrix.sh /tmp/ctox_qwen35_hardware_grid_current

machine: MacBook Pro Mac17,2
chip: Apple M5
cpu: 10 cores, 4 performance + 6 efficiency
gpu: Apple M5, 10 cores
memory: 32 GB unified
Metal: Metal 4
CPU sysctls:
  FEAT_SME=1
  FEAT_SME2=1
  FEAT_BF16=1
  FEAT_I8MM=1
  FEAT_DotProd=1
  FEAT_FP16=1
  FEAT_FHM=1
public Metal counters:
  GPUTimestamp only
```

Added:

```text
docs/kernel-dev/HARDWARE_BACKEND_GRID.md
tools/analyze_matrix_backend_grid.py

Updated:
  docs/kernel-dev/CANDIDATE_MANIFEST_TEMPLATE.md
  docs/kernel-dev/QUANT_PIPELINE_TEMPLATE.md
  tools/validate_quant_pipeline.py
  tools/kernel_dev_doctor.sh
  KERNEL_DEV_HANDBOOK.md
```

p4096 matrix backend shootout:

```text
tools/run_matrix_backend_shootout.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  4096 3 \
  /tmp/ctox_qwen35_matrix_backend_grid_p4096

tools/analyze_matrix_backend_grid.py \
  /tmp/ctox_qwen35_matrix_backend_grid_p4096/shootout.md
```

Results:

```text
gate_up_combined:
  MPS raw GEMM:        0.005159167 s
  MSL integrated MMA:  0.019215459 s
  MSL/MPS:             3.725x slower
  winner:              MPS

ffn_down:
  MPS raw GEMM:        0.002741708 s
  MSL integrated MMA:  0.038315541 s
  MSL/MPS:             13.975x slower
  winner:              MPS

delta_out:
  MPS raw GEMM:        0.001801625 s
  MSL gated-norm+out:  0.001510125 s
  MSL/MPS:             0.838x
  winner:              current MSL path
```

Decision:

```text
The next prefill performance work must target the M5 matrix backend gap:
  Gate+Up and FFN Down are now integration/matrix-backend problems.
  DeltaOut should stay on the current MSL path.

Do not spend more effort on scalar int8/q4 row-reduction kernels for these
large dense projections unless the schedule is matrix-shaped from the start.
```

CPU quant backend-column probe:

```text
Implemented:
  tools/cpu_quant_probe.c
  tools/run_cpu_quant_probe.sh

Command:
  tools/run_cpu_quant_probe.sh 64 1024 1024 3 1

Result:
  neon_dotprod_compile_feature: 1
  i8_median_s: 0.002407625
  i8_effective_tops: 0.056
  i8_visible_gb_s: 0.572
  q4_unpack_median_s: 0.003705458
  q4_unpack_effective_tops: 0.036
  q4_unpack_visible_gb_s: 0.230
```

Interpretation:

```text
The naive CPU NEON/DotProd q4-unpack path is not a Qwen hotpath candidate.
SME/I8MM still needs a true matrix-shaped/panelized primitive probe before it
can influence the model strategy. The GPU/MPS matrix column remains the
priority for prefill Gate+Up and Down.
```

Hybrid MPS FFN block probe:

```text
Implemented:
  tools/mps_ffn_block_probe.swift
  tools/run_mps_ffn_block_probe.sh

Operation:
  MPSMatrix x gate_up -> [tokens,7168]
  MSL SwiGLU -> [tokens,3584]
  MPSMatrix x down -> [tokens,1024]

Command:
  tools/run_mps_ffn_block_probe.sh 4096 1024 3584 3 1

Result:
  median_s: 0.009343500
  effective_tflops: 9.653
  visible_gb_s: 13.579
```

Comparison against p4096 MSL probes:

```text
gate/up MSL MMA: 0.019215459 s
down MSL MMA:    0.038315541 s
sum:             0.057531000 s

hybrid MPS+MSL FFN block:
  0.009343500 s
  ~6.16x faster than current MSL gate/up + down probe sum
```

Decision:

```text
This is the first clearly promising M5 hardware-feature path for closing the
prefill gap. The next integration work is to make Qwen metalpack weights
available in an MPS-compatible matrix layout and route FFN Gate+Up/Down through
this backend while retaining custom MSL for surrounding fused ops.
```

Real metalpack FFN MPS probe:

```text
Implemented:
  tools/mps_ffn_metalpack_probe.swift
  tools/run_mps_ffn_metalpack_probe.sh

Behavior:
  reads existing metalpack manifest and weights.bin
  finds layer mlp_gate/mlp_up/mlp_down entries
  converts fp16_row_tiled weights once into MPS row-major/transposed buffers
  runs MPS Gate+Up -> MSL SwiGLU -> MPS Down

Command:
  tools/run_mps_ffn_metalpack_probe.sh \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 3 1

Result:
  median_s: 0.009666167
  effective_tflops: 9.331
```

Comparison:

```text
current p4096 MSL Gate+Up + Down probe sum:
  0.019215459 + 0.038315541 = 0.057531000 s

real-metalpack MPS FFN block:
  0.009666167 s

speedup:
  ~5.95x for the FFN block matrix phases
```

Decision:

```text
This is no longer only a synthetic backend result. Real Qwen weights retain the
MPS advantage after one-time layout conversion. Production work should add an
MPS-compatible sidecar weight layout to the pack/runtime and route prefill FFN
blocks through it.
```

## 2026-05-01 00:22 CEST - Persistent MPS FFN Sidecar Pack

Implemented:

```text
src/bin/pack_mps_ffn_sidecar.rs
tools/mps_ffn_sidecar_probe.swift
tools/run_mps_ffn_sidecar_probe.sh
```

Sidecar layout:

```text
format: ctox.qwen35_08b.mps_ffn_sidecar

per layer:
  gate_up:
    shape: [1024, 7168]
    layout: fp16 row-major for MPSMatrix
    columns 0..3583: gate_proj transposed
    columns 3584..7167: up_proj transposed

  down:
    shape: [3584, 1024]
    layout: fp16 row-major for MPSMatrix
    source: down_proj transposed
```

Pack:

```text
target/release/pack_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar

pack time:
  1.394 s

packed_bytes:
  528482304

packed_gib:
  0.492
```

Probe:

```text
tools/run_mps_ffn_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar 0 4096 3 1

median_s:
  0.008587583

effective_tflops:
  10.503
```

Comparison:

```text
current p4096 MSL Gate+Up + Down probe sum:
  0.019215459 + 0.038315541 = 0.057531000 s

persistent-sidecar MPS FFN block:
  0.008587583 s

speedup:
  ~6.70x for the FFN block matrix phases
```

Decision:

```text
This is now the primary FFN prefill integration target. The next code step is
to move from Swift probe to runtime integration: load the sidecar once, keep
the MPS-compatible FFN matrices resident, and route prefill FFN Gate+Up/Down
through MPS while keeping surrounding RMSNorm/SwiGLU/residual orchestration in
custom MSL.
```

Impact estimate:

```text
Implemented:
  tools/estimate_mps_ffn_prefill_impact.py

Command:
  tools/estimate_mps_ffn_prefill_impact.py

Inputs:
  baseline_full_s:     3.364
  llama_tok_s:         2852.70
  MSL FFN/layer:       0.057531000 s
  MPS FFN/layer:       0.008587583 s
  FFN layers:          24

Output:
  total_saved_s:            1.174642008
  projected_full_s:         2.189357992
  projected_tok_s:          1870.87
  projected_vs_llama_gap_x: 1.525
  remaining_seconds_to_llama: 0.753525272
```

Interpretation:

```text
The MPS FFN sidecar is a major necessary win, but it does not close the
llama.cpp prefill gap alone. After integrating it, the remaining p4096 target
is about 0.75 s, so the next bottleneck hunt must shift to DeltaNet
projection/scan and attention core under a post-FFN forensics run.
```

## 2026-05-01 00:29 CEST - DeltaNet QKV+Z MPS sidecar probe

Goal:

```text
Test whether the large DeltaNet QKV+Z projection should stay on handwritten
MSL SIMDgroup matmul or move to the same MPS matrix backend track as FFN.
```

Implemented:

```text
src/bin/pack_mps_delta_project_sidecar.rs
tools/mps_deltanet_project_sidecar_probe.swift
tools/run_mps_deltanet_project_sidecar_probe.sh
```

Sidecar build:

```text
target/release/pack_mps_delta_project_sidecar \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

delta_layers: 18
packed_bytes: 301989888
packed_gib: 0.281
layout: qkvz[1024,8192] fp16 row-major per DeltaNet layer
```

Probe:

```text
tools/run_mps_deltanet_project_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar 0 4096 3 1

median_s: 0.006910042
effective_tflops: 9.945
visible_gb_s: 13.354
```

Reference context:

```text
synthetic MPS qkvz p4096:
  median_s: 0.006537417
  effective_tflops: 10.512

existing standalone MSL delta project p4096:
  median_s: 0.255187917
  scope: RMSNorm + qkv/z/b/a projection plumbing, so not a direct one-op
         replacement measurement
```

Learning:

```text
The dense DeltaNet QKV+Z matrix phase should not remain a bespoke MSL matmul
optimization target. The real sidecar proves that MPS keeps the synthetic
matrix-backend speed when fed persistent Qwen weights. The next hard work is
the runtime bridge: MPS QKV+Z and MPS FFN must share command-buffer/timeline
orchestration with the existing MSL RMSNorm, SwiGLU, DeltaNet scan, attention,
and residual kernels. Until that bridge exists, these are strong backend
measurements but not full-prefill wins.
```

## 2026-05-01 00:42 CEST - Rust MPS FFN runtime bridge

Goal:

```text
Move the FFN sidecar from Swift-only probe to the Rust/Metal research runtime
so the next integration step can replace the full-prefill FFN path instead of
only estimating it externally.
```

Implemented:

```text
vendor/mps/mps_sidecar.mm
src/metal/mps_sidecar.rs
src/bin/bench_mps_ffn_sidecar_runtime.rs
vendor/metal/shaders/qwen35_08b/ffn_tiled.metal::qwen35_08b_mps_swiglu_gateup_fp16_i3584
build.rs ObjC++ static library link for MetalPerformanceShaders
```

Measurement:

```text
target/release/bench_mps_ffn_sidecar_runtime \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar 0 4096 3 1 1

include_norm:      1
median_s:          0.009669208
p95_s:             0.010470041
effective_tflops:  9.328

no-norm control:
  command suffix: 0
  median_s:      0.009181208
```

Comparison:

```text
Swift persistent-sidecar FFN probe:
  0.008587583 s

Rust runtime bridge overhead:
  norm-inclusive path is not directly comparable to the Swift no-norm probe;
  no-norm Rust control is ~1.069x slower than Swift

current MSL FFN Gate+Up + Down sum:
  0.057531000 s

Rust MPS bridge speedup over current MSL FFN:
  ~5.95x including RMSNorm
```

Projected p4096 full-prefill impact:

```text
tools/estimate_mps_ffn_prefill_impact.py --mps-ffn-s 0.009669208

projected_full_s:         2.215316992
projected_tok_s:          1848.95
projected_vs_llama_gap_x: 1.543
remaining_seconds_to_llama: 0.779484272
```

Learning:

```text
The MPS bridge is not the bottleneck; the sidecar speed survives
one-command-buffer Rust runtime composition. This is the first concrete
GPU-matrix-backend integration point for the prefill mega-pipeline. It still
does not beat llama.cpp alone, so the next full-prefill work must wire this
bridge into the integrated Delta+FFN stack and then attack DeltaNet QKV+Z/scan
and attention core with the same backend-grid discipline.
```

## 2026-05-01 01:18 CEST - Integrated Delta+FFN stack with MPS FFN sidecar

Goal:

```text
Verify that the MPS FFN sidecar still helps once it is inserted into the real
DeltaNet+FFN superblock pipeline, including DeltaOut residual input,
FFN RMSNorm, MPS Gate+Up, MSL SwiGLU, MPS Down, and final half residual add.
```

Implemented:

```text
vendor/metal/shaders/qwen35_08b/hidden_cast.metal:
  qwen35_08b_prefill_residual_add_fp16_to_fp16_k1024

src/metal/bench.rs:
  PrefillMpsFfnLayerWeights
  run_prefill_delta_ffn_stack_with_mps_ffn_sidecar(...)

src/bin/bench_metalpack_prefill_delta3_ffn_superblock.rs:
  optional trailing [mps-ffn-sidecar-dir] argument
```

Small integrated smoke:

```text
MSL baseline:
  tools/run_accepted_profile.sh \
    target/release/bench_metalpack_prefill_delta3_ffn_superblock \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 1 1 3

  median_s: 0.034042333

MPS FFN sidecar:
  tools/run_accepted_profile.sh \
    target/release/bench_metalpack_prefill_delta3_ffn_superblock \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 1 1 3 \
    /tmp/ctox_qwen35_08b_mps_ffn_sidecar

  median_s: 0.022567750

speedup:
  1.51x for p512 / 3 DeltaNet+FFN layers
```

Real integrated p4096 / 18 DeltaNet-layer stack:

```text
MSL accepted-profile reference:
  median_s: 1.701746125
  tok_s:    2407.0 approx for Delta18+FFN stack only

MPS FFN sidecar integrated:
  tools/run_accepted_profile.sh \
    target/release/bench_metalpack_prefill_delta3_ffn_superblock \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 2 1 18 \
    /tmp/ctox_qwen35_08b_mps_ffn_sidecar

  median_s: 1.180014250
  tok_s:    3471.1 approx for Delta18+FFN stack only

speedup:
  1.44x for the integrated Delta18+FFN stack

time_saved:
  0.521731875 s at p4096
```

Numerical drift versus MSL reference at p512 / 3 layers:

```text
target/release/compare_half_dump \
  /tmp/ctox_delta512_msl.bin \
  /tmp/ctox_delta512_mps.bin \
  512 1024

mismatch_count: 356792 / 524288
mean_abs_error: 0.000300663
rms_error:      0.000444082
max_abs_error:  0.015625000
checksum_delta: 0.231690586
```

Learning:

```text
The FFN sidecar is now a real integrated pipeline win, not just a standalone
MPS probe. It also proves that the command-buffer phase cut is viable:
MSL DeltaOut can end its compute encoder, MPS FFN can run, and MSL can resume
for residual/output. The cost is numerical drift from MPS/FP16 Down replacing
the old f32 down accumulation path, so promotion needs explicit error gates.

This still does not close the full prefill gap alone. It removes about 0.52 s
from the p4096 Delta18+FFN stack. The next remaining p4096 targets are:
DeltaNet QKV+Z projection on MPS/sidecar, DeltaNet scan+norm, and the six
attention-layer FFNs plus attention core.
```

## 2026-05-01 01:31 CEST - DeltaNet QKV+Z f32-output MPS feasibility

Goal:

```text
Check whether the DeltaNet QKV+Z MPS sidecar can write float32 output directly,
because the current DeltaNet conv/split/gating path consumes qkv_out and z_out
as f32 tensors.
```

Measurement:

```text
tools/run_mps_deltanet_project_sidecar_probe.sh \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar 0 4096 3 1 1

output_dtype:       float32
median_s:           0.008769667
effective_tflops:   7.836
visible_gb_s:       18.174
```

Comparison:

```text
float16-output MPS sidecar:
  median_s: 0.006910042

float32-output MPS sidecar:
  median_s: 0.008769667

existing integrated project phase for Delta18 stack:
  ~0.591669 s cumulative at p4096 accepted profile
```

Learning:

```text
MPS can produce the dtype needed by the current DeltaNet downstream path.
The f32-output penalty is real but acceptable for integration exploration.
The next candidate should route RMSNorm -> MPS QKV+Z f32 -> split qkv/z or
stride-aware downstream kernels. A naive materialized split is acceptable only
as a measurement bridge; the final layout should avoid copying the 8192-wide
projection result back into separate global tensors if stride-aware kernels
are faster.
```

## 2026-05-01 02:20 CEST - Integrated MPS QKV+Z sidecar into Delta18+FFN prefill

Goal:

```text
Move DeltaNet QKV+Z from a standalone MPS probe into the real prefill
DeltaNet+FFN superblock, while keeping b/a activation, conv/split, scan,
gated norm, DeltaOut, SwiGLU, and residual phases in MSL.
```

Implemented:

```text
vendor/mps/mps_sidecar.mm:
  CtoxMpsDeltaProjectPlan
  ctox_mps_delta_project_plan_new/free/encode

src/metal/mps_sidecar.rs:
  MpsDeltaProjectPlan

vendor/metal/shaders/qwen35_08b/prefill_deltanet_prepare.metal:
  qwen35_08b_prefill_deltanet_split_qkvz_project_f32

src/metal/bench.rs:
  PrefillMpsDeltaProjectLayerWeights
  optional MPS QKV+Z path inside run_prefill_delta_ffn_stack_with_mps_ffn_sidecar(...)

src/bin/bench_metalpack_prefill_delta3_ffn_superblock.rs:
  optional trailing [mps-delta-project-sidecar-dir]

src/bin/memory_forensics.rs:
  optional Delta18 sidecar arguments and sidecar-specific byte buckets
```

Integrated smoke:

```text
tools/run_accepted_profile.sh \
  target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 512 1 1 3 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

median_s: 0.017595208

previous MPS-FFN-only p512/3-layer run:
  median_s: 0.023523375

speedup:
  1.34x vs MPS-FFN-only
```

Integrated p4096 / 18 DeltaNet-layer stack:

```text
tools/run_accepted_profile.sh \
  target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 2 1 18 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

median_s: 0.814598250
tok_s_delta18_ffn_stack_only: 5028.3

previous MPS-FFN-only p4096/18-layer run:
  median_s: 1.180014250

old MSL accepted-profile p4096/18-layer run:
  median_s: 1.701746125

speedup:
  1.45x vs MPS-FFN-only
  2.09x vs old MSL accepted stack
```

Numerical drift versus MSL reference at p512 / 3 layers:

```text
target/release/compare_half_dump \
  /tmp/ctox_delta512_msl.bin \
  /tmp/ctox_delta512_mps_ffn_qkvz.bin \
  512 1024

mismatch_count: 361454 / 524288
mean_abs_error: 0.000312417
rms_error:      0.000459428
max_abs_error:  0.015625000
checksum_delta: 0.110043228
```

Full prefill forensics estimate with integrated Delta18 sidecars:

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

llama.cpp pp4096 reference:
  2852.70 tok/s

gap:
  llama.cpp remains 1.93x faster
```

Learning:

```text
The matrix-backend swap is now real and materially improves prefill. It also
proves that phase-cut orchestration between MSL and MPS can work inside a
single command buffer. The old QKV/Z and FFN weight-streaming flaw is no
longer the dominant Delta18 explanation in the sidecar path: modeled sidecar
weight streaming is down to one pass over ~739 MiB of unique weights.

The remaining Delta18+FFN gap is now structural: scan/gated-norm/DeltaOut,
materialized qkvz split, phase boundaries, and non-modeled stalls. The next
optimization target should therefore not be another QKVZ matmul variant. It
should be either:

1. stride-aware downstream kernels that consume qkvz[tokens,8192] directly and
   remove the materialized split, or
2. a fused/chunked DeltaNet scan+gated-norm path with a sidecar-aware byte
   model and error gate.
```

## 2026-05-01 02:48 CEST - qkvz-direct MPS sidecar bridge

Goal:

```text
Remove the materialized qkvz -> qkv_out + z_out split bridge from the
integrated MPS QKV+Z path. Conv/Split and GatedNorm should consume the
combined qkvz[tokens,8192] output directly.
```

Implemented:

```text
vendor/metal/shaders/qwen35_08b/prefill_deltanet_prepare.metal:
  qwen35_08b_prefill_deltanet_conv_split_qkvz_norm_tok_f32_to_fp16_h16d128
  qwen35_08b_prefill_deltanet_conv_state_update_qkvz_c6144_k4

vendor/metal/shaders/qwen35_08b/prefill_deltanet_gated_norm.metal:
  qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_tok_h16d128_f32_to_fp16

src/metal/bench.rs:
  CTOX_QWEN35_MPS_QKVZ_DIRECT=1
```

p4096 / 18 DeltaNet-layer stack:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1 \
tools/run_accepted_profile.sh \
  target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 2 1 18 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

median_s: 0.679421292

previous MPS QKVZ split bridge:
  median_s: 0.814598250

speedup:
  1.20x vs split bridge
  2.51x vs old MSL accepted stack
```

MPS qkvz-direct profile stops, p4096 / 18 layers:

```text
project:     0.117363291 s
conv_split:  0.143619791 s
scan_norm:   0.383966584 s
delta_out:   0.527728416 s
ffn_gate_up: 0.627994000 s
full:        0.684312708 s
```

Token sweep:

```text
p512 direct, 18 layers:
  median_s: 0.083995875

p16384 direct, 18 layers:
  median_s: 2.836903917
```

Drift check:

```text
target/release/compare_half_dump \
  /tmp/ctox_delta512_msl.bin \
  /tmp/ctox_delta512_mps_ffn_qkvz_direct.bin \
  512 1024

mismatch_count: 361454 / 524288
mean_abs_error: 0.000312417
rms_error:      0.000459428
max_abs_error:  0.015625000
checksum_delta: 0.110043228
```

Full prefill estimate with qkvz-direct:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1 \
target/release/memory_forensics \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 2 90 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

delta18+ffn:
  median_ms: 687.541

full_prefill_estimate_current_kernels:
  2.680 s
  1528.49 tok/s

llama.cpp pp4096:
  2852.70 tok/s

gap:
  llama.cpp remains 1.87x faster
```

Learning:

```text
The qkvz-direct bridge validates the layout hypothesis: the split bridge was
worth roughly 135 ms at p4096 / 18 DeltaNet layers. The next largest measured
Delta18 component is now scan+norm:

scan_norm - conv_split:
  0.240346793 s

delta_out - scan_norm:
  0.143761832 s

full - ffn_gate_up:
  0.056318708 s

The next real kernel target is therefore not QKVZ. It is DeltaNet scan+norm,
then DeltaOut, while attention.core remains the largest model-wide full-prefill
row outside Delta18.
```

## 2026-05-01 03:18 CEST - qkvz-direct plus lanes4 scan candidate

Goal:

```text
Re-test DeltaNet scan families in the new qkvz-direct sidecar context. The old
lanes4 scan had lost against rowcache_block32 before the MPS/QKVZ layout work,
but the new phase layout changed the surrounding bottlenecks enough that the
candidate needed to be re-measured.
```

ScanNorm sweep, p4096 / 18 DeltaNet layers:

```text
accepted rowcache_block32:
  median_s: 0.374539958

rowcache_block64:
  median_s: 0.374242083

rowcache_direct:
  median_s: 0.375005167

lanes4:
  median_s: 0.269049041

lanes4_ordered:
  median_s: 1.569576625

scan_gated_norm:
  median_s: 0.394096291
```

Full Delta18+FFN stack:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1 \
CTOX_QWEN35_DELTA_SCAN_LANES4=1 \
tools/run_accepted_profile.sh \
  target/release/bench_metalpack_prefill_delta3_ffn_superblock \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 4096 2 1 18 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

median_s: 0.539885917

qkvz-direct without lanes4:
  median_s: 0.679421292

speedup:
  1.26x vs qkvz-direct rowcache_block32
```

Token sweep:

```text
p512 direct+lanes4:
  median_s: 0.069712709

p16384 direct+lanes4:
  median_s: 2.430149041
```

Drift:

```text
target/release/compare_half_dump \
  /tmp/ctox_delta512_msl.bin \
  /tmp/ctox_delta512_mps_ffn_qkvz_direct_lanes4.bin \
  512 1024

mismatch_count: 362783 / 524288
mean_abs_error: 0.000315165
rms_error:      0.000461633
max_abs_error:  0.007812500
checksum_delta: 0.358400762
```

Full prefill estimate with MPS FFN for all FFN rows:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1 \
CTOX_QWEN35_DELTA_SCAN_LANES4=1 \
target/release/memory_forensics \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 2 90 \
  /tmp/ctox_qwen35_08b_mps_ffn_sidecar \
  /tmp/ctox_qwen35_08b_mps_delta_project_sidecar

delta18+ffn:
  median_ms: 560.844

attention.core:
  median_ms: 288.720

attention.ffn:
  median_ms: 9.080

full_prefill_estimate_current_kernels:
  2.348 s
  1744.73 tok/s

llama.cpp pp4096:
  2852.70 tok/s

gap:
  llama.cpp remains 1.64x faster
```

Learning:

```text
The lanes4 scan was previously rejected in the old layout, but it becomes a
strong candidate after qkvz-direct and MPS sidecars. This is an important
methodology point: rejected kernels are not permanent facts; a layout/backend
change can move the bottleneck enough that old candidates must be re-swept.

The full-prefill estimator also needed correction: once FFN sidecar integration
is proven for Delta layers, the six attention-layer FFNs should use the same
MPS FFN sidecar row in forensics. Otherwise the model-wide estimate is
artificially pessimistic.

The remaining largest model-wide row is now attention.core. Delta18+FFN is
still above its byte floor, but attention.core is the highest single full-model
time bucket after the sidecar and lanes4 wins.
```

## 2026-05-01 CEST - Attention O-Proj MPS Sidecar And SIMD32 Vec8 Scan

Hypotheses:

```text
1. attention.core must be split into cumulative stages before more kernel work.
2. Attention O projection is a plain [T x 2048] * [2048 x 1024] matmul and
   should use the same matrix backend lesson as FFN/Delta QKVZ sidecars.
3. The accepted qh4/qblk1 scan still pays two threadgroup barriers per key for
   256-thread reductions. A single-SIMDgroup kernel can remove those barriers
   by letting each lane own 8 head dimensions.
```

p4096 cumulative stage profile before O sidecar:

```text
norm:      0.000750417 s
project:   0.029946791 s
prepare:   0.030949667 s
attention: 0.275162583 s
full:      0.313158708 s

derived:
  q/k/v project:     29.2 ms
  prepare/rope:       1.0 ms
  scan:             244.2 ms
  o_proj:            38.0 ms
```

Added:

```text
pack_mps_attention_out_sidecar
bench_mps_attention_out_sidecar_runtime
optional bench_metalpack_prefill_attention_core MPS O sidecar arg
CTOX_QWEN35_ATTENTION_CORE_PROFILE_STOP
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8
```

Isolated MPS O projection:

```text
target/release/bench_mps_attention_out_sidecar_runtime \
  /tmp/ctox_qwen35_08b_mps_attention_out_sidecar 3 4096 7 3

median_s:          0.002202583
effective_tflops:  7.800
```

Integrated p4096 attention.core:

```text
qh4/qblk1 + MPS O:
  median_s: 0.275315333

qh4_simd32_vec8 + MPS O:
  median_s: 0.131357334

speedup:
  2.10x vs qh4/qblk1 + MPS O
```

Correctness against previous qh4/qblk1 attention dump at p512:

```text
count:           1048576
mismatch_count:  602
mean_abs_error:  0.000000029
rms_error:       0.000002845
max_abs_error:   0.000976562
checksum_delta: -0.007914960
```

Full p4096 model estimate with current sidecar candidate profile:

```text
CTOX_QWEN35_MPS_QKVZ_DIRECT=1
CTOX_QWEN35_DELTA_SCAN_LANES4=1
MPS FFN sidecar
MPS Delta QKV/Z sidecar
MPS Attention O sidecar
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1

delta18+ffn:      558.545 ms
attention.core:   132.234 ms
attention.ffn:      7.638 ms

full_prefill_estimate_current_kernels:
  1.398 s
  2930.37 tok/s

llama.cpp pp4096:
  2852.70 tok/s
```

Decision:

```text
Accept qh4_simd32_vec8 as the new attention env default. It is not a sparse or
approximate attention algorithm; it computes the same online-softmax result
within FP16 storage noise while removing threadgroup reduction barriers.

Keep the MPS Attention O sidecar as a backend-specific candidate and pass it
explicitly to benchmarks/forensics. It requires a sidecar pack path, so it is
not represented by env-only accepted_profile.env.
```

Learning:

```text
For a 256-wide head, "more threads" was the wrong local optimum. The old kernel
used 256 threads so every thread owned one dimension, but then paid cross-SIMD
threadgroup-memory reductions and barriers for every key. The faster kernel
uses one 32-lane SIMDgroup and gives each lane 8 dimensions. This increases
per-thread register work but removes per-key barriers and reaches roughly
153 GB/s on the benchmark's byte model.

This is the concrete SIMD lesson the project was missing: do not merely use
SIMD reductions; choose the work ownership that avoids expensive synchronization
while preserving coalesced KV loads.

Remaining caveat: this is validated at p4096 and p512 correctness. Long-context
p8192/p16384/p128k sweeps are still required before claiming the prefill gap is
closed for the user's requested 128k regime.
```

## 2026-05-01 CEST - Long Prefill Sweep, DeltaOut MPS, And Window Attention Candidate

Exact long-context sweep after qh4 SIMD32 vec8, MPS FFN, MPS QKV/Z, qkvz-direct,
lanes4 scan, and MPS Attention O:

```text
p8192:
  delta18+ffn:     1101.082 ms
  attention.core:   424.086 ms
  attention.ffn:     13.406 ms
  full estimate:      3.726 s
  tok/s:           2198.58

p16384:
  delta18+ffn:     2447.943 ms
  attention.core:  1568.726 ms
  attention.ffn:     30.676 ms
  full estimate:     12.044 s
  tok/s:           1360.30

llama.cpp pp16384:
  2065.71 tok/s
```

Interpretation:

```text
p4096 is now faster than llama.cpp, but exact p16384 is still slower. The gap is
not CPU overhead; it is exact quadratic attention. The qh4 SIMD32 vec8 kernel is
fast per byte, but the byte count grows as T^2.
```

Delta18+FFN p16384 phase profile with MPS sidecars:

```text
project:     0.450179166 s
conv_split:  0.554027958 s
scan_norm:   1.364092000 s
delta_out:   1.931923500 s
ffn_gate_up: 2.323833708 s
full:        2.532185917 s

phase deltas:
  project:     450 ms
  conv/split:  104 ms
  scan/norm:   810 ms
  delta_out:   568 ms
  ffn gate/up: 392 ms
  ffn down/residual/rest: 208 ms
```

Added MPS DeltaOut sidecar:

```text
pack_mps_delta_out_sidecar
bench_metalpack_prefill_delta3_ffn_superblock ... [mps-delta-out-sidecar-dir]
memory_forensics ... [mps-attention-out-sidecar-dir] [mps-delta-out-sidecar-dir]
```

Measured Delta18+FFN impact:

```text
p4096:
  MSL DeltaOut: 0.561171542 s
  MPS DeltaOut: 0.449904125 s

p16384:
  MSL DeltaOut: 2.500954542 s
  MPS DeltaOut: 2.111856459 s
```

DeltaOut MPS drift at p512 against the previous MSL DeltaOut path:

```text
count:           524288
mismatch_count:  467711
mean_abs_error:  0.001933543
rms_error:       0.002533772
max_abs_error:   0.039062500
checksum_delta:  7.538944244
```

Updated exact full estimates with MPS DeltaOut:

```text
p4096:
  full estimate: 1.316 s
  tok/s:         3112.20
  llama.cpp:     2852.70 tok/s

p16384:
  full estimate: 11.733 s
  tok/s:          1396.42
  llama.cpp:      2065.71 tok/s

p32768:
  full estimate: 41.642 s
  tok/s:           786.89
  llama.cpp:      1325.20 tok/s
```

Exact-path conclusion:

```text
MPS DeltaOut is a backend-specific sidecar candidate with non-trivial but small
hidden-state drift. It improves all measured token sizes, but it should stay
behind an explicit sidecar path until the model-level quality budget is tested.
The remaining exact long-context bottleneck is Attention T^2 traffic.
```

Sparse/window candidate:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WIN4096=1

This is a structured local-window attention candidate, not exact attention.
It keeps the most recent 4096 keys for each query.
```

Attention-core timing with MPS Attention O:

```text
          full_exact       win4096
p4096     0.121023500 s    0.130665875 s
p16384    1.578269541 s    0.771103375 s
p32768    6.035448458 s    1.602353875 s
```

Approximate projected full-prefill estimates, using measured exact Delta/FFN and
window Attention:

```text
p16384:
  approx_s = 2.100466 + 6 * (0.771103 + 0.029422)
           = 6.903616 s
  approx_tok_s = 2373 tok/s
  llama.cpp pp16384 = 2065.71 tok/s

p32768:
  approx_s = 4.457959 + 6 * (1.602354 + 0.066304)
           = 14.469907 s
  approx_tok_s = 2265 tok/s
  llama.cpp pp32768 = 1325.20 tok/s
```

Window p8192 drift against exact qh4 SIMD32 vec8:

```text
count:           16777216
mismatch_count:  8118148
mean_abs_error:  0.002596090
rms_error:       0.009273482
max_abs_error:   0.756835938
checksum_delta: -302.567766309
```

Decision:

```text
Do not promote WIN4096 into the exact accepted profile. Keep it as an opt-in
sparse-attention research candidate. It shows that a real sparse/KV-selection
strategy can beat llama.cpp at long prompt sizes, but the quality risk is large
and needs task/perplexity evaluation before it can be considered a valid model
mode.
```

Learning:

```text
For long prefill there are now two tracks:

1. exact track:
   continue toward FlashAttention-style query/key tiling or compressed exact KV
   movement. Current exact SIMD32 kernel is byte-efficient but still T^2.

2. approximate track:
   window/sparse/KV-selection methods can close the long-context speed gap, but
   must carry quality metrics, not just tok/s. Window 4096 is fast and simple,
   but p8192 drift is too high to treat as a safe default.
```

## 2026-05-01 CEST - Parameterized Window Attention Sweep Tool

Change:

```text
Added CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW=<tokens>
Added tools/run_attention_window_quality_sweep.sh
Extended compare_attention_raw_dump with rms_error and checksum_delta.
```

The previous `WIN4096` candidate was a single hard-coded experiment. The new
path makes the visible KV window a measured parameter and compares each
candidate against an exact qh4 SIMD32 vec8 raw attention dump in the same
serial run.

Smoke correctness:

```text
p1024 exact:       0.014566750 s
p1024 window 512: 0.012537625 s, mean_abs 0.002669876, rms 0.007219486
p1024 window1024: 0.014311500 s, bitexact against exact
```

Long attention-core sweep with MPS Attention O sidecar:

```text
p8192:
  exact:       0.437737750 s
  window4096: 0.347766042 s, mean_abs 0.002596090, rms 0.009273482, max_abs 0.756835938
  window8192: 0.443750625 s, bitexact

p16384:
  exact:        1.587089250 s
  window4096:  0.760995708 s, mean_abs 0.007775984, rms 0.023167125, max_abs 1.320556641
  window8192:  1.205333083 s, mean_abs 0.002944876, rms 0.010559149, max_abs 0.699707031
  window16384: 1.605048625 s, bitexact

p32768:
  exact:        6.462921750 s
  window4096:  1.687524959 s, mean_abs 0.013380524, rms 0.035488006, max_abs 1.833984375
  window8192:  2.964136750 s, mean_abs 0.007940244, rms 0.021617799, max_abs 1.457031250
  window16384: 4.853272625 s, mean_abs 0.002604114, rms 0.008850987, max_abs 0.750244141
  window32768: 6.307306750 s, bitexact
```

Projected full-prefill estimates with the existing measured MPS DeltaOut path:

```text
Formula:
  full_s = delta18_ffn_s + 6 * (attention_core_s + attention_ffn_s)

p16384, window4096:
  2.100466 + 6 * (0.760996 + 0.029422) = 6.842974 s
  tok/s = 2394.3 vs llama.cpp pp16384 2065.71

p16384, window8192:
  2.100466 + 6 * (1.205333 + 0.029422) = 9.509000 s
  tok/s = 1722.9 vs llama.cpp pp16384 2065.71

p32768, window4096:
  4.457959 + 6 * (1.687525 + 0.066304) = 14.980933 s
  tok/s = 2187.3 vs llama.cpp pp32768 1325.20

p32768, window8192:
  4.457959 + 6 * (2.964137 + 0.066304) = 22.640605 s
  tok/s = 1447.3 vs llama.cpp pp32768 1325.20

p32768, window16384:
  4.457959 + 6 * (4.853273 + 0.066304) = 33.975421 s
  tok/s = 964.5 vs llama.cpp pp32768 1325.20
```

Decision:

```text
Keep parameterized window attention as an explicit approximate research mode.
It can beat the llama.cpp prefill reference at p16384/p32768 only by changing
the attention semantics. The speedup is real, but the tensor drift is large
enough that it needs model-quality gates before it can be a product mode.
```

Learning:

```text
Window size must be treated like quantization: hardware performance alone is
not enough. Each setting needs speed, tensor drift, and eventually quality
metrics. The new sweep tool turns sparse-attention exploration from a single
ad-hoc flag into a measurable grid.
```

## 2026-05-01 CEST - Exact Attention Register-Pressure Check And Cache Model Fix

Hypothesis:

```text
The qh4/qblk2 SIMD32 exact kernel failed because it held too many per-query
gate values in registers. Move gate sigmoid reads to the final output store and
keep the online-softmax loop focused on q/acc/m/l.
```

Result:

```text
p1024 qh4/qblk1 late-gate:
  bitexact versus previous raw attention dump
  mismatch_count: 0
  mean_abs/rms/max_abs: 0

p8192 qh4/qblk1 late-gate:
  median_s: 0.484907958
  previous exact qh4 SIMD32 neighborhood: ~0.437-0.464 s
  decision: no promotion; reverted for accepted path

p8192 qh4/qblk2 late-gate:
  median_s: 1.946681791
  decision: still rejected
```

Decision:

```text
Late-gate is a negative result. It reduces visible register arrays, but the
compiler/hardware schedule gets slower. Keep gate resident in the accepted
qh4 SIMD32 vec8 kernel.
```

Cache-model update:

```text
Added attention.prefill_kv_stream to cache_analysis.
Fixed the byte model to use attention head_dim=256, not DeltaNet head_dim=128.
```

p32768 exact qh4 cache model at 174 GB/s sustained:

```text
attention.prefill_kv_stream:
  modeled DRAM miss bytes: 1024.03 GiB per attention layer
  modeled time floor:      6319.225 ms
  measured attention core: ~6307-6463 ms in recent exact runs
```

Learning:

```text
The exact p32768 Attention core is now explained by the K/V streaming floor.
This is not primarily an avoidable cache-miss problem in the current qh4 path:
qh4 already gives the expected 75% modeled hit/reuse versus naive per-Q-head
K/V reads. Further exact wins need either successful query-block K/V reuse
without register-pressure collapse, or lower-precision/static KV storage that
reduces the compulsory bytes.
```

## 2026-05-01 CEST - Exact Interleaved K/V Cache Layout Candidate

Hypothesis:

```text
Separate K and V buffers may produce less favorable cacheline/prefetch behavior
inside the qh4 SIMD32 attention scan. Packing K and V as adjacent half values
per token/head/dim could improve memory locality without changing semantics.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INTERLEAVED_KV=1

prepare kernel:
  qwen35_08b_prefill_attention_prepare_qk_rope_v_interleaved_gqa8_kv2_d256

attention kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_interleaved_kv_gqa8_kv2_d256_to_fp16
```

Correctness:

```text
p1024 raw attention dump vs accepted qh4 SIMD32 vec8:
  mismatch_count: 0
  mean_abs_error: 0
  rms_error: 0
  max_abs_error: 0
  checksum_delta: 0
```

Serial timing with MPS Attention O:

```text
p8192:
  accepted qh4 SIMD32 vec8:       0.450825334 s
  interleaved K/V exact candidate: 0.464868000 s

p16384:
  accepted qh4 SIMD32 vec8:       1.837716375 s
  interleaved K/V exact candidate: 1.874628750 s
```

Decision:

```text
Reject for promotion. Keep only as an opt-in layout probe and sweep variant.
The contiguous separate K and V streams are already better for this access
pattern than K/V interleaving.
```

Learning:

```text
Not every locality-looking layout helps. For this SIMD32 vec8 scan, each lane
loads 8 K values and 8 V values across the full head. Separate contiguous K/V
streams appear to coalesce/prefetch better than alternating K,V,K,V halfs.
```

## 2026-05-01 CEST - Int8 K/V Cache Candidate For Exact Attention Schedule

Hypothesis:

```text
The exact qh4 SIMD32 vec8 prefill Attention path is near the FP16 K/V streaming
floor. Store K/V cache as int8 plus one fp16 scale per token/KV-head and
dequantize lane-locally in the online-softmax loop. This avoids materializing a
dequant tensor and should reduce compulsory K/V bytes.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_KV=1

prepare kernel:
  qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_gqa8_kv2_d256

attention kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_kv_gqa8_kv2_d256_to_fp16

layout:
  k_cache_i8[token, kv_head, dim]
  v_cache_i8[token, kv_head, dim]
  kv_scale_fp16[token, kv_head]
```

Correctness/drift at p1024 versus accepted FP16 K/V qh4 SIMD32 vec8:

```text
mismatch_count:  2027653 / 2097152
mean_abs_error:  0.001691472
rms_error:       0.002557171
max_abs_error:   0.035156250
checksum_delta:  42.124013007
```

Serial timing with MPS Attention O:

```text
p8192:
  accepted FP16 K/V: 0.440077250 s
  int8 K/V:          0.468724917 s

p16384:
  accepted FP16 K/V: 1.596366667 s
  int8 K/V:          1.774528625 s
```

Decision:

```text
Reject for promotion. The byte reduction is real in the model, but scalar
char->float dequantization and scale multiplication inside the hot key loop
make the kernel slower on this Apple GPU schedule.
```

Learning:

```text
Quantization only helps if the hardware path consumes the quantized format
efficiently. For qh4 SIMD32 vec8, naive int8 K/V scalar loads are not enough.
Future KV quantization must test vectorized packed loads, pairwise int8/half
unpacking, or a different matrix/tensor-backed attention schedule.
```

## 2026-05-01 CEST - Int8 V-Only Cache Candidate

Hypothesis:

```text
The full int8 K/V candidate is slower because K dequantization sits directly in
the QK dot-product. Keep K in FP16 and quantize only V to int8+scale. This
reduces Value-stream bytes while preserving exact FP16 attention scores.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V=1

prepare kernel:
  qwen35_08b_prefill_attention_prepare_qk_rope_v_int8_v_gqa8_kv2_d256

attention kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_gqa8_kv2_d256_to_fp16

layout:
  k_cache_fp16[token, kv_head, dim]
  v_cache_i8[token, kv_head, dim]
  v_scale_fp16[token, kv_head]
```

Drift at p1024 versus accepted FP16 K/V qh4 SIMD32 vec8:

```text
mismatch_count:  1893601 / 2097152
mean_abs_error:  0.000575357
rms_error:       0.000878829
max_abs_error:   0.009765625
checksum_delta:  18.951141834
```

Serial timing with MPS Attention O:

```text
p8192:
  accepted FP16 K/V: 0.427998250 s
  int8 V-only:       0.461115375 s

p16384:
  accepted FP16 K/V: 1.599017250 s
  int8 V-only:       1.723293583 s
```

Decision:

```text
Reject for promotion. V-only quantization has much lower drift than int8 K/V,
but it is still slower than FP16 on the current SIMD32 scan schedule.
```

Learning:

```text
The qh4 SIMD32 vec8 attention kernel is sufficiently bandwidth- and
instruction-balanced that a 25% K/V byte reduction is not enough if it adds
scalar int8 conversion and per-key scale handling. The next quantized-Attention
attempt must use a format that maps to vectorized packed loads or a different
hardware backend, not scalar char loads.
```

Stage profiling:

```text
tool:
  tools/profile_attention_variant_stages.sh

p8192 int8 V-only:
  accepted prepare:        0.068291416 s
  accepted attention-only: 0.373918375 s
  int8 V prepare:          0.070542291 s
  int8 V attention-only:   0.389970334 s

delta:
  prepare:        +0.002250875 s
  attention-only: +0.016051959 s
```

Conclusion:

```text
The loss is mostly in the Attention hot loop, not in the one-time V
quantization prepare phase.
```

## 2026-05-01 CEST - Int8 V PACK4/Broadcast Candidate

Hypothesis:

```text
The int8 V-only candidate may be slow because each lane performs scalar byte
loads. Pack four int8 V values into one 32-bit load per four SIMD lanes, then
use simd_broadcast plus bit-unpack to distribute the values.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_INT8_V_PACK4=1

prepare:
  same int8 V-only prepare layout

attention:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_int8_v_pack4_gqa8_kv2_d256_to_fp16
```

Correctness:

```text
p1024 PACK4 vs scalar int8 V:
  mismatch_count: 0
  mean_abs_error: 0
  rms_error: 0
  checksum_delta: 0
```

Serial timing with MPS Attention O:

```text
p8192:
  FP16 K/V:        0.444675333 s
  scalar int8 V:   0.473095250 s
  PACK4 int8 V:    0.559044000 s

p16384:
  FP16 K/V:        1.615702958 s
  scalar int8 V:   1.750226125 s
  PACK4 int8 V:    2.104403417 s
```

Decision:

```text
Reject for promotion. PACK4 reduces the number of memory load instructions, but
simd_broadcast plus bit extraction is much more expensive than scalar byte
loads in this SIMD32 attention schedule.
```

Learning:

```text
For this Apple GPU/MSL path, lane-broadcast unpacking is not a free way to make
int8 V hardware-friendly. Future int8 attention attempts should avoid per-key
SIMD-lane broadcast/unpack and instead look for a native vector/matrix path or
a different ownership mapping.
```

## 2026-05-01 CEST - HALFACC Attention Compute Candidate

Hypothesis:

```text
The exact qh4 SIMD32 vec8 attention path is near the FP16 K/V byte floor, but
the previous int8 candidates showed that added scalar conversion can dominate.
Try a compute-precision candidate instead: keep the full K/V set and online
softmax m/l in FP32, but store q/k/v/gate and value accumulation in half.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFACC=1

kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfacc_gqa8_kv2_d256_to_fp16

semantics:
  full causal context
  no dropped tokens
  approximate due half q/k/gate/value accumulation
```

p1024 drift versus accepted qh4 SIMD32 vec8:

```text
mismatch_count:  1721843 / 2097152
mean_abs_error:  0.000382237
rms_error:       0.000784457
max_abs_error:   0.020507812
checksum_delta: -21.981298745
```

p8192 drift versus accepted qh4 SIMD32 vec8:

```text
mismatch_count:  16069022 / 16777216
mean_abs_error:  0.002941105
rms_error:       0.006426603
max_abs_error:   0.150390625
checksum_delta: -875.106115758
```

Serial timing with MPS Attention O:

```text
p8192:
  accepted: 0.440715500 s
  HALFACC:  0.288347334 s
  speedup:  1.53x

p16384:
  accepted: 1.588621708 s
  HALFACC:  1.013771083 s
  speedup:  1.57x

p32768:
  accepted: 6.697672416 s
  HALFACC:  4.333045291 s
  speedup:  1.55x
```

Stage profile at p8192:

```text
accepted prepare:        0.063283917 s
accepted attention-only: 0.371097500 s
HALFACC prepare:         0.062655583 s
HALFACC attention-only:  0.220770375 s
hotloop delta:          -0.150327125 s
```

Projected full-prefill estimates with existing MPS DeltaOut path:

```text
p16384:
  2.100466 + 6 * (1.013771083 + 0.029422) = 8.359624498 s
  tok/s = 1959.90 vs llama.cpp pp16384 2065.71

p32768:
  4.457959 + 6 * (4.333045291 + 0.066304) = 30.854054746 s
  tok/s = 1062.03 vs llama.cpp pp32768 1325.20
```

Decision:

```text
Keep HALFACC as the strongest approximate full-context attention candidate so
far. Do not promote to exact accepted profile: it changes numerical semantics
and still does not close the p16k/p32k full-prefill gap against llama.cpp.
```

Learning:

```text
For this Apple GPU, reducing accumulator/register precision is more effective
than scalar int8 K/V formats on the current qh4 SIMD32 schedule. The next
candidate should combine HALFACC with a safer reduced-memory strategy or reduce
non-attention Delta/FFN time, rather than continuing scalar int8 attention.
```

## 2026-05-01 CEST - HALFDOT Attention Compute Candidate

Hypothesis:

```text
HALFACC made value accumulation half-precision but still used a float
score-dot. Test whether doing the q*k score partial in half improves SIMDgroup
throughput enough to close the p16k prefill gap without changing the memory
layout or context coverage.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT=1

kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_halfdot_gqa8_kv2_d256_to_fp16

semantics:
  full causal context
  approximate due half q/k/gate/value accumulation and half score partials
  online softmax m/l remains FP32
```

p1024 drift versus accepted qh4 SIMD32 vec8:

```text
mismatch_count:  1791691 / 2097152
mean_abs_error:  0.000401121
rms_error:       0.000792377
max_abs_error:   0.021484375
checksum_delta: -22.499710381
```

p8192 drift versus accepted qh4 SIMD32 vec8:

```text
mismatch_count:  16148185 / 16777216
mean_abs_error:  0.002944298
rms_error:       0.006426372
max_abs_error:   0.153320312
checksum_delta: -874.254780114
```

Serial timing with MPS Attention O:

```text
p8192:
  HALFACC: 0.280871667 s
  HALFDOT: 0.261224500 s
  speedup over HALFACC: 1.08x

p16384:
  HALFACC: 1.013771083 s
  HALFDOT: 0.919213792 s
  speedup over HALFACC: 1.10x

p32768:
  HALFACC: 4.333045291 s
  HALFDOT: 4.130694917 s
  speedup over HALFACC: 1.05x
```

Stage profile:

```text
p8192:
  accepted prepare:        0.074614334 s
  accepted attention-only: 0.417726249 s
  HALFDOT prepare:         0.073213291 s
  HALFDOT attention-only:  0.227387375 s
  hotloop delta:          -0.190338874 s

p16384:
  accepted prepare:        0.141721708 s
  accepted attention-only: 1.714746417 s
  HALFDOT prepare:         0.145545917 s
  HALFDOT attention-only:  0.925195208 s
  hotloop delta:          -0.789551209 s
```

Projected full-prefill estimates with existing MPS DeltaOut path:

```text
p16384:
  2.100466 + 6 * (0.919213792 + 0.029422) = 7.792280752 s
  tok/s = 2102.59 vs llama.cpp pp16384 2065.71

p32768:
  4.457959 + 6 * (4.130694917 + 0.066304) = 29.639952502 s
  tok/s = 1105.53 vs llama.cpp pp32768 1325.20
```

Decision:

```text
Keep HALFDOT as the current strongest approximate full-context attention
candidate. It is the first full-context approximate p16k projection that beats
the llama.cpp pp16384 reference, but it is not exact and still fails pp32768.
Do not promote into the conservative accepted profile without model-quality
gates and a backend-specific approximate profile.
```

Learning:

```text
On this SIMD32 schedule, lowering the score partial to half gives an additional
5-10% over HALFACC, so precision policy is a real hardware schedule parameter,
not just an accuracy choice. The remaining p32768 gap likely needs a structural
attention change: Split-K/Flash-style tiling, sparse/window mode with accepted
quality loss, or a backend that exposes native matrix/tensor attention.
```

## 2026-05-01 CEST - WINDOW_HALFDOT Sparse/Precision Hybrid

Hypothesis:

```text
p32768 remains below llama.cpp even with full-context HALFDOT. Combine the two
successful approximate levers: local-window K/V traffic reduction and half
score/value accumulation. This explicitly trades attention quality for
bandwidth and compute throughput.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW_HALFDOT=<tokens>

kernel:
  qwen35_08b_prefill_attention_causal_qh4_qblk1_simd32_vec8_window_halfdot_gqa8_kv2_d256_to_fp16

semantics:
  approximate sparse attention
  keeps only the most recent <tokens> keys/values per query
  half q/k/gate/value accumulation and half q*k score partials
  online softmax m/l remains FP32
```

Semantic sanity check:

```text
p1024 with WINDOW_HALFDOT=4096 vs full-context HALFDOT:
  mismatch_count: 0
  mean_abs_error: 0
  rms_error: 0
  checksum_delta: 0
```

p8192 drift versus accepted exact qh4 SIMD32 vec8:

```text
WINDOW_HALFDOT=4096:
  mean_abs_error:  0.003992523
  rms_error:       0.010185974
  max_abs_error:   0.759521484
  checksum_delta: -1059.032199860

WINDOW_HALFDOT=8192:
  mean_abs_error:  0.002944298
  rms_error:       0.006426372
  max_abs_error:   0.153320312
  checksum_delta: -874.254780114
```

Serial attention-core timing with MPS Attention O:

```text
p8192:
  WINDOW_HALFDOT=4096: 0.229501583 s
  WINDOW_HALFDOT=8192: 0.303724916 s

p16384:
  WINDOW_HALFDOT=4096: 0.536036292 s
  WINDOW_HALFDOT=8192: 0.888198292 s

p32768:
  WINDOW_HALFDOT=4096:  1.156241041 s
  WINDOW_HALFDOT=8192:  2.032697458 s
  WINDOW_HALFDOT=16384: 3.386395417 s
```

Projected full-prefill estimates with existing MPS DeltaOut path:

```text
p16384:
  WINDOW_HALFDOT=4096: 2982.59 tok/s vs llama.cpp pp16384 2065.71
  WINDOW_HALFDOT=8192: 2154.04 tok/s vs llama.cpp pp16384 2065.71

p32768:
  WINDOW_HALFDOT=4096:  2778.54 tok/s vs llama.cpp pp32768 1325.20
  WINDOW_HALFDOT=8192:  1921.66 tok/s vs llama.cpp pp32768 1325.20
  WINDOW_HALFDOT=16384: 1301.65 tok/s vs llama.cpp pp32768 1325.20
```

Decision:

```text
Keep as the fastest approximate long-prefill candidate. Do not promote to the
accepted exact profile. This is a model-quality/product decision, not a kernel
correctness decision: win4096 and win8192 beat llama.cpp p32k in projection,
but they drop old context and add half-precision drift.
```

Learning:

```text
For long prefill, reducing the K/V visit count is stronger than another local
layout tweak. The measured effective GB/s remains stable around 245-250 GB/s,
so the speedup comes from doing less streamed work, not from hiding misses in
the existing exact algorithm.
```

## 2026-05-01 CEST - Hardware Backend Shootout: SME2 and ANE Status

Question:

```text
Are we actually using SME2 and the NPU/ANE on this M5 hardware?
```

Tooling added:

```text
tools/run_hardware_backend_shootout.sh <metalpack-dir> [tokens] [iterations] [output-dir]
tools/analyze_hardware_backend_shootout.py <shootout.md>

capture_hardware_feature_matrix.sh now records extended SME sysctls and clang
ARM feature macros.

cpu_quant_probe now reports:
  neon_dotprod_compile_feature
  i8mm_compile_feature
  bf16_compile_feature
  sme_compile_feature
  sme2_compile_feature
  sme2_usage_status
```

Local M5 facts:

```text
runtime sysctls:
  FEAT_SME=1
  FEAT_SME2=1
  FEAT_SME2p1=1
  FEAT_BF16=1
  FEAT_EBF16=1
  FEAT_I8MM=1
  SME_F16F32=1
  SME_I8I32=1

clang -mcpu=native macros:
  __ARM_FEATURE_SME=1
  __ARM_FEATURE_SME2=1
  __ARM_FEATURE_MATMUL_INT8=1
  __ARM_FEATURE_BF16=1
  __ARM_FEATURE_DOTPROD=1
```

p512 backend shootout:

```text
CPU quant probe:
  shape: tokens=512 rows=3584 k=1024
  first i8_median_s:        0.039959417
  first i8_effective_tops:  0.094
  first q4_unpack_median_s: 0.061448541
  first q4_effective_tops:  0.061
  latest i8_median_s:        0.133300083
  latest i8_effective_tops:  0.028
  latest q4_unpack_median_s: 0.141118000
  latest q4_effective_tops:  0.027
  latest-2 i8_median_s:        0.088641875
  latest-2 i8_effective_tops:  0.042
  latest-2 q4_unpack_median_s: 0.110470791
  latest-2 q4_effective_tops:  0.034
  sme2_usage_status:  not_used_by_this_probe

SME2 smoke probe:
  sme_streaming_call_status: ok
  sme_streaming_vector_words: 16
  sme_streaming_vector_bytes: 64
  sme_za_zero_status: ok
  disassembly: smstart / zero {za} / smstop present
  model_hotpath_status: smoke_only_not_model_path

SME2 I8 MOPA probe:
  status: ok
  streaming_vector_bytes: 64
  za_rows_s32: 16
  disassembly: smopa za0.s, p0/m, p0/m, z0.b, z1.b present
  hotpath_status: microkernel_probe_not_model_path

MPSMatrix fp16 GEMM:
  first best: 5.537 TFLOPS
  latest best: 5.279 TFLOPS
  latest-2 best: 4.903 TFLOPS

Core ML / ANE:
  status: ruled_out
  coremltools_available: false
  Core ML artifacts found: none
```

Decision:

```text
The current Qwen Metal/MPS path does not use CPU SME2 and does not use ANE.
SME2 is available and executable in minimal smoke and int8 MOPA probes,
including ZA usage and real `smopa` code generation. It still needs a real
Qwen-shaped matrix microkernel before it can be considered for packing,
quantized CPU fallback, or coarse prefill work. ANE requires a Core ML artifact
/ converter path and coarse graph boundaries; it is not part of the current
per-token Metal hot path.
```

Learning:

```text
Hardware availability is only column zero in the grid. A backend is considered
usable only after local measurements show the actual operation shape benefits.
For dense Qwen GEMM shapes on this M5, MPSMatrix is currently the measured fast
matrix backend; the CPU quant probe is a control path, not a hot-path candidate.
SME2-MOPA is now a viable research backend to develop, but no model path should
claim SME2 speed until a Qwen-shaped SME microkernel beats the GPU/MPS baseline.
```

## 2026-05-01 CEST - Exact qh4 Split-K Prefill Attention Probe

Hypothesis:

```text
The old PARTIAL_QBLK2 split-K design was structurally wrong for Qwen GQA: it
ran per Q-head and reread the same K/V cache for all four Q heads sharing one
KV head. A qh4 split-K design should preserve GQA reuse by assigning one
SIMD32 group to four Q heads and one KV head, then writing partial m/l/acc for
log-sum-exp combine.
```

Implementation:

```text
CTOX_QWEN35_ATTENTION_QH4_SPLITK64=1
CTOX_QWEN35_ATTENTION_QH4_SPLITK128=1
CTOX_QWEN35_ATTENTION_QH4_SPLITK256=1
CTOX_QWEN35_ATTENTION_QH4_SPLITK512=1

stage 1:
  qh4 split-K over key blocks
  one SIMD32 group owns 4 Q heads x 8 dims/lane
  writes partial_m, partial_l, partial_acc

stage 2:
  generic split-K combine with log-sum-exp merge
```

Correctness:

```text
p1024 qh4_splitk64 vs accepted qh4 SIMD32 vec8:
  mismatch_count: 1405 / 2097152
  mean_abs_error: 0.000000098
  rms_error:      0.000005869
  max_abs_error:  0.000976562

Interpretation:
  Not bitexact because split-K changes softmax accumulation order, but raw
  attention drift is tiny and similar to the earlier partial attention drift.
```

Timing:

```text
p1024:
  old PARTIAL_QBLK2 after early-return fix: 0.146809833 s
  qh4_splitk64:                          0.017433458 s
  qh4_splitk128:                         0.016720917 s
  qh4_splitk256:                         0.016573625 s

p4096:
  accepted qh4 SIMD32 vec8: 0.127131375 s
  qh4_splitk64:            0.197536708 s
  qh4_splitk128:           0.172208250 s
  qh4_splitk256:           0.151201042 s
  qh4_splitk512:           0.142699708 s

p8192:
  accepted qh4 SIMD32 vec8: 0.450240125 s
  qh4_splitk128:           0.577529792 s
  qh4_splitk256:           0.540268584 s
  qh4_splitk512:           0.525104833 s

p16384:
  accepted qh4 SIMD32 vec8 previous run: 1.588621708 s
  qh4_splitk256:                        1.984333333 s
  qh4_splitk512:                        1.939569000 s
```

Decision:

```text
Reject qh4 Split-K for the current exact prefill path. It fixes the GQA
overread problem in old PARTIAL_QBLK2 and is much faster than that old design,
but it still loses to accepted qh4 SIMD32 vec8 at p4096/p8192/p16384.
```

Learning:

```text
Split-K is not automatically a win for batch-prefill attention. It raises
parallelism, but writing full [query, head, key_block, head_dim] partial_acc
and then combining it creates too much scratch traffic. The next exact
attention design must avoid full partial_acc writes, e.g. Flash-style tiling
that keeps the output accumulator on chip, or a backend/tensor path that can
fuse the softmax/value accumulation more tightly.
```

## 2026-05-01 CEST - SME2 I8 Tile Stream Probe

Goal:

```text
Move SME2 from minimal instruction smoke testing toward Qwen-shape-near backend
evidence. The user correctly pushed that hardware features must be measured
against actual limits before being claimed as optimization wins.
```

Implementation:

```text
Added:
  tools/sme2_i8_tile_probe.c
  tools/run_sme2_i8_tile_probe.sh

Integrated into:
  tools/run_hardware_backend_shootout.sh
  tools/analyze_hardware_backend_shootout.py
  tools/kernel_dev_doctor.sh
  docs/kernel-dev/HARDWARE_BACKEND_GRID.md
  docs/kernel-dev/README.md
  README.md
  KERNEL_DEV_HANDBOOK.md

The probe:
  uses ACLE SME streaming mode
  uses svmopa_za32_s8_m for INT8 outer-product accumulation
  stores ZA rows with svst1_hor_za32
  reports MOPA/s and modeled stream GB/s
  labels itself as not layout-correct Qwen matmul and not model hot path
```

Standalone p512/Qwen-projection-shape result:

```text
tools/run_sme2_i8_tile_probe.sh 512 3584 1024 5 1

streaming_vector_bytes:       64
za_rows_s32:                  16
m_tiles:                      32
n_tiles:                      224
k_blocks:                     16
mopa_per_run:                 114688
best_s:                       0.000142500
mopa_per_s_best:              804828070.175
stream_gb_s_best:             154.527
hotpath_status:               tile_probe_not_model_path
interpretation:               qwen_shape_streaming_probe_not_layout_correct_matmul
disassembly evidence:         smopa + ZA st1w stores
```

Serial hardware shootout result:

```text
tools/run_hardware_backend_shootout.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 512 3 \
  /tmp/ctox_qwen35_hardware_backend_sme2_tile_20260501T061620Z

analysis:
  sme2_runtime_available:             yes
  sme2_compile_available:             yes
  sme2_smoke_executes:                yes
  sme2_i8_mopa_executes:              yes
  sme2_i8_tile_executes:              yes
  sme2_used_by_current_model_hotpath: no
  sme2_i8_tile_stream_gb_s_best:      42.966
  sme2_i8_mopa_per_s_best:            369795133.496
  mps_effective_tflops_best:          5.758
  mps_effective_tflops_all:           [5.758, 5.215, 5.423]
  coreml_ane_status:                  ruled_out
```

Decision:

```text
Do not promote SME2 into the model path yet.

This is meaningful progress because the tooling now verifies M5 SME2 INT8 MOPA
execution with panel streaming and ZA stores. It is not enough to beat
llama.cpp or MPSMatrix. The next SME2 milestone must be a layout-correct INT8
or Q4 matmul with packed weights, reference comparison, and operation-specific
comparison against MPS/Metal. More generic MOPA smoke tests are no longer useful.
```

Learning:

```text
Hardware availability is not performance. SME2 exists and works, but the
current dense Qwen projection evidence still favors GPU/MPSMatrix. To exploit
SME2, quantization format and packed layout must be designed around the SME
tile geometry first, then validated against model-level accuracy and latency.
```

## 2026-05-01 CEST - Static INT8 Metal Matmul Autotune

Goal:

```text
Turn the user's layout-tuning requirement into a serial autotune loop for the
existing static INT8 Metal matmul probe. The old fixed row_tile=8 result was not
enough to know whether the schedule was intrinsically bad or merely badly tuned.
```

Implementation:

```text
Changed:
  vendor/metal/shaders/qwen35_08b/prefill_matmul_int8.metal
    max_row_tile: 8 -> 16

  src/bin/bench_static_int8_matmul.rs
    added optional args:
      quant_group_size
      row_tile
      col_tile

Added:
  tools/run_static_int8_matmul_autotune.sh
  tools/analyze_static_int8_autotune.py
```

Measurement:

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
  row_tile=16
  quant_group_size=128
  col_tile=256
  median_s=0.029514792
  p95_s=0.036120125
  effective_visible_gb_s=0.411

worst:
  row_tile=4
  quant_group_size=256
  median_s=0.094780208

best_vs_worst_speedup:   3.211
best_vs_MPS_reference:   0.022x
```

Decision:

```text
Reject the current scalar-dequant static INT8 Metal matmul schedule for the
model hot path, even after row-tile tuning. Keep the autotune tooling.

The useful learning is not the absolute performance; the useful learning is
that row_tile and quant_group_size have a 3.2x swing even inside a bad schedule.
Every future quantized layout must be empirically swept this way before a fixed
layout is discussed.
```

## 2026-05-01 CEST - Static INT8 SIMD32 Matmul Variant

Goal:

```text
Test the hypothesis that the static INT8 Metal matmul is slow partly because it
uses a threadgroup-memory reduction instead of explicit SIMDgroup reductions.
```

Implementation:

```text
Added kernel:
  qwen35_08b_prefill_matmul_int8_row_tiled_simd32_k1024_f32

Design:
  one SIMDgroup owns one output row
  32 lanes sweep the 1024 K dimension
  simd_sum reduces the dot product
  one lane writes the row output
  benchmark arg selects scalar vs simd32
```

Measurement:

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
  kernel=scalar
  row_tile=16
  quant_group_size=64
  median_s=0.032614458
  p95_s=0.037204834

best simd32:
  kernel=simd32
  row_tile=4
  quant_group_size=64
  median_s=0.049373958
  p95_s=0.055636208

best_vs_MPS_reference: 0.020x
```

Decision:

```text
Reject this SIMD32 variant for the model path. It is slower than the best
scalar-dequant schedule and still far behind MPSMatrix.
```

Learning:

```text
SIMDgroup use is not a magic switch. This variant improved the reduction shape
but did not fix the real bottleneck: scalar int8-to-float consumption and poor
reuse of token/input values across many output rows. A viable quantized path
must use a backend-native dot/matrix primitive or a layout that lets each load
feed many rows/columns, not one row per SIMDgroup.
```

## 2026-05-01 CEST - Prefill Reference Report Tool

Goal:

```text
Stop manually comparing scattered prefill projections against llama.cpp and
separate exact-ish, approximate precision, and sparse-window rows explicitly.
```

Implementation:

```text
Added:
  tools/prefill_reference_report.py

Updated:
  docs/kernel-dev/README.md
  README.md
  KERNEL_DEV_HANDBOOK.md
  tools/kernel_dev_doctor.sh
```

Result:

```text
tools/prefill_reference_report.py

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

exact_gap:
  worst_tokens=32768
  vs_llama=0.594x
```

Decision:

```text
The current exact long-prefill target is attention traffic, not INT8 projection
matmul. Approximate precision/window modes can beat llama.cpp, but they require
quality gates and cannot be counted as accepted-profile wins.
```

## 2026-05-01 CEST - Exact Attention Traffic Report And Dense QK Matrix Probe

Goal:

```text
Determine whether the exact p16k/p32k Attention gap is a cache/layout bug in
the current scan, or whether the next exact path needs a different algorithmic
shape.
```

Implementation:

```text
Added:
  tools/exact_attention_traffic_report.py
  tools/run_attention_qk_mps_probe.sh
  tools/analyze_attention_qk_mps_probe.py

Updated:
  docs/kernel-dev/README.md
  README.md
  KERNEL_DEV_HANDBOOK.md
  tools/kernel_dev_doctor.sh
```

Exact traffic report:

```text
p32768 qh4/qblk1 exact:
  K/V stream:       1024.03 GiB
  byte floor @174:  6319.225 ms
  measured:         near byte floor

p32768 hypothetical qblk2/qblk4/qblk8:
  qblk2 K/V stream: 512.03 GiB = 0.500x qblk1
  qblk4 K/V stream: 256.03 GiB = 0.250x qblk1
  qblk8 K/V stream: 128.03 GiB = 0.125x qblk1

Prior measured candidates:
  qh4_qblk2_vec8_exact: rejected, register pressure
  qblk4_batch_exact:    rejected, slower than qh4
  qblk8_batch_exact:    rejected, slower than qh4
  qh4_splitk512_exact:  rejected, scratch traffic
```

Dense QK MPS probe:

```text
tools/run_mps_matrix_probe.sh 4096 4096 256 3 1
  median_s: 0.001797917
  effective_tflops: 4.778

tools/run_mps_matrix_probe.sh 8192 8192 256 3 1
  median_s: 0.005539041
  effective_tflops: 6.203

tools/run_mps_matrix_probe.sh 16384 16384 256 2 1
  median_s: 0.015537875
  effective_tflops: 8.845

analysis:
  p16384 qk_8_heads_s:  0.124303000
  p16384 qk_6_layers_s: 0.745818000
```

Decision:

```text
Do not spend more effort on tiny cache-miss cleanup in qh4/qblk1: it is already
near the modeled byte floor. The next exact-attention architecture should be a
tiled QK-softmax-V prototype using MPSMatrix or equivalent GPU matrix hardware
for QK tiles, with no full dense score materialization at long contexts.
```

Tiled prototype planner:

```text
tools/plan_tiled_attention.py --tokens 16384 \
  --q-tiles 64,128,256,512 \
  --k-tiles 256,512,1024,2048

recommended first grid:
  q_tile: 128..256
  k_tile: 512..1024

q_tile=128 k_tile=512:
  score tile:        0.125 MiB per Q head
  causal tile pairs: 2112
  K/V tile:          0.500 MiB

q_tile=256 k_tile=1024:
  score tile:        0.500 MiB per Q head
  causal tile pairs: 544
  K/V tile:          1.000 MiB
```

## 2026-05-01 CEST - Tiled QK MPS Prototype Grid

Goal:

```text
Before writing the full exact tiled QK-softmax-V attention kernel, verify the
tile schedule against actual Apple matrix backend encode/runtime overhead.
```

Implementation:

```text
Added:
  tools/tiled_attention_qk_mps_prototype.swift
  tools/run_tiled_attention_qk_mps_prototype.sh
  tools/run_tiled_attention_qk_mps_grid.sh
  tools/analyze_tiled_attention_qk_mps_grid.py

Updated:
  docs/kernel-dev/README.md
  README.md
  KERNEL_DEV_HANDBOOK.md
  docs/kernel-dev/HARDWARE_BACKEND_GRID.md
  tools/kernel_dev_doctor.sh
```

Contract:

```text
The prototype is synthetic. It repeatedly encodes causal QK tile GEMMs with
MPSMatrix in one command buffer. It has no real Q/K slicing, no softmax, no V
accumulation, and no accepted-profile semantics. It only tests whether tiled QK
matrix hardware usage is a plausible next architecture.
```

p4096 grid:

```text
tools/run_tiled_attention_qk_mps_grid.sh 4096 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_4096.txt

best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=40
  median_s=0.001742500
  effective_tflops=3.081
  mps_encodes_per_s=22955.524
```

p8192 grid:

```text
tools/run_tiled_attention_qk_mps_grid.sh 8192 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_8192.txt

best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=144
  median_s=0.006061125
  effective_tflops=3.189
  mps_encodes_per_s=23757.966
```

p16384 grid:

```text
tools/run_tiled_attention_qk_mps_grid.sh 16384 3 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_16384.txt

best:
  q_tile=256
  k_tile=1024
  causal_tile_pairs=544
  median_s=0.016984125
  effective_tflops=4.299
  mps_encodes_per_s=32029.910
```

p32768 grid:

```text
tools/run_tiled_attention_qk_mps_grid.sh 32768 2 1 \
  /tmp/ctox_qwen35_tiled_qk_mps_grid_32768.txt

best:
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
Use q_tile=256/k_tile=1024 as the first full tiled-attention prototype shape.
The exact qh4/qblk1 scan remains a byte-floor implementation, so the next
possible p16k/p32k exact-prefill speedup must come from changing the algorithmic
shape: matrix-backed QK tiles, block/online softmax, and V accumulation without
materializing full dense T x T scores.
```

## 2026-05-01 CEST - Full Tiled Attention MPS Prototype

Goal:

```text
Measure the complete tiled exact-attention stage sequence, not just QK:
QK tile GEMM, block softmax update, P*V tile GEMM, and online output combine.
```

Implementation:

```text
Added:
  tools/tiled_attention_full_mps_prototype.swift
  tools/run_tiled_attention_full_mps_prototype.sh

Updated:
  docs/kernel-dev/README.md
  README.md
  KERNEL_DEV_HANDBOOK.md
  docs/kernel-dev/HARDWARE_BACKEND_GRID.md
  tools/kernel_dev_doctor.sh
```

Important correction:

```text
The first softmax prototype used one thread per score row and was wrong for
Apple GPU SIMD utilization. Replacing it with a SIMD32 row softmax using
simd_max/simd_sum improved p1024 q_tile=256/k_tile=512 from roughly 4.19 ms to
1.40 ms for the one-head prototype.

The first online combine also missed the required PV scale
exp(tile_m - next_m). That is now fixed; the corrected prototype is slightly
slower but mathematically valid.
```

Qwen GQA correction:

```text
The first full prototype processed one Q head. The useful Qwen shape is qh4:
four Q heads share one KV head. The prototype now defaults to heads_per_group=4,
using q_rows_per_tile=q_tile*4 so K/V are reused across four Q heads.
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

MPSMatrix origins:

```text
The first qh4 version reused a single synthetic Q/K/V tile. The prototype now
allocates full synthetic Q/K/V matrices and uses MPSMatrix left/right origins
for each q_block/k_block. This removes the copy-kernel concern for tile slicing;
remaining gaps are quality parity and Rust benchmark integration.
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
Continue on this implementation track. It is still synthetic and not accepted,
but it is the first path whose p32k QK+softmax+PV projection is plausibly below
the old qh4/qblk1 attention byte floor. Next step: Rust benchmark integration
and full output-dump comparison against the accepted qh4 exact kernel.
```

## 2026-05-01 18:58 CEST - Decode Regression Lesson: Micro-Wins Are Not Mega-Kernel Wins

Rechecked the decode path after the earlier log showed decode above the
llama.cpp standalone tg512 reference.

Current serial measurements:

```text
accepted profile, tg128:
  command:
    tools/run_accepted_profile.sh \
      target/release/bench_metalpack_decode_layered_pattern \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored \
      107 3 0 128 128
  median_s: 2.907556833
  tok/s:    44.02

accepted profile + decode rowcache, tg128:
  command:
    CTOX_QWEN35_DECODE_DELTA_ROWCACHE=1 tools/run_accepted_profile.sh \
      target/release/bench_metalpack_decode_layered_pattern \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored \
      107 3 0 128 128
  median_s: 3.080838125
  tok/s:    41.55

accepted profile, tg512:
  command:
    tools/run_accepted_profile.sh \
      target/release/bench_metalpack_decode_layered_pattern \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored \
      107 1 0 512 512
  median_s: 11.802641583
  tok/s:    43.38

accepted profile + no Split-K, tg512:
  command:
    CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK=1 tools/run_accepted_profile.sh \
      target/release/bench_metalpack_decode_layered_pattern \
      /tmp/ctox_qwen35_08b_real_fp16.metalpack ignored ignored ignored \
      107 1 0 512 512
  median_s: 10.719503292
  tok/s:    47.76

llama.cpp standalone tg512 reference:
  44.77 tok/s
```

Regression finding:

```text
The old fast decode result was real enough for that build/flag state, but it
was not protected by a decode acceptance suite. Later work made Split-K128 the
default after short 4-token measurements. That was too aggressive: at tg512,
disabling Split-K is faster and again beats the llama.cpp tg512 reference.
At tg128, neither current accepted nor current no-Split-K beats llama.cpp tg128
52.98 tok/s yet.

Decode rowcache is currently a negative path in the full 24-layer tg128 run,
despite being a small win in the older single-layer/stability tests. It remains
experimental and should not be promoted.
```

Mega-kernel development lesson:

```text
1. A local kernel win is not a mega-kernel win.
   Split-K can improve a 4-token or single-attention slice and still lose when
   multiplied across 6 attention layers and 128/512 decode tokens.

2. Default flags need scenario gates, not boolean optimism.
   Split-K should be selected by measured context/decode-length thresholds, not
   by one default that applies to every token.

3. Accepted profiles must be mode-aware.
   Prefill flags, decode flags, approximate paths, and exact paths need separate
   acceptance gates. One env file can hide regressions across modes.

4. Promotion requires realistic output lengths.
   Minimum decode gate is tg128 and tg512 against llama.cpp. tg4 is useful only
   for correctness and dispatch-forensics, never for promotion.

5. Cache/bandwidth models must include orchestration and scratch.
   Split-K reduces per-block attention underutilization, but adds partial
   scratch writes/reads and extra dispatches. The mega-kernel decision is the
   total token loop, not the inner math alone.

6. Every accepted optimization needs a rollback trigger.
   If a later profile drops below the reference or below the previous accepted
   scenario, the tooling must point to the responsible flag family immediately.
```

Next action:

```text
Add a decode profile matrix and promotion guard:
  tg128/tg512
  Split-K on/off/block-size threshold
  decode rowcache on/off
  accepted profile vs llama.cpp reference

Do not promote decode changes from tg4 or isolated attention timing.
```

Implemented:

```text
tools/run_decode_regression_matrix.sh
```

The tool runs variants serially under a local lock:

```text
accepted
no_splitk
rowcache
no_splitk_rowcache
```

It also supports alternating rounds:

```text
tools/run_decode_regression_matrix.sh --iterations 3 --rounds 2 <metalpack>
```

First diagnostic run, `--sizes 128,512`, `--iterations 1`, `--rounds 1`:

```text
tg128:
  accepted             2.509064708 s   51.02 tok/s   0.963x llama.cpp tg128
  no_splitk            2.526612292 s   50.66 tok/s   0.956x
  rowcache             3.443654333 s   37.17 tok/s   0.702x
  no_splitk_rowcache   3.144545584 s   40.71 tok/s   0.768x

tg512:
  accepted            12.819735458 s   39.94 tok/s   0.892x llama.cpp tg512
  no_splitk           13.089339917 s   39.12 tok/s   0.874x
  rowcache            16.531475250 s   30.97 tok/s   0.692x
  no_splitk_rowcache  16.328967625 s   31.36 tok/s   0.700x
```

Important interpretation:

```text
This single-iteration matrix is diagnostic, not acceptance evidence. It shows
Rowcache is consistently bad in the current full decode path. It also shows
that prior single tg512 runs can vary materially, so promotion must use
multi-iteration, alternating-order rounds. The regression guard now exists; the
next optimization step should use it before changing any decode default.
```

## 2026-05-01 19:42 CEST - Decode Regression Follow-Up: Measurement State, CPU Cache, SIMD LM Head, Async Commands

Continued the decode regression investigation.

Code-path finding:

```text
An older log entry said private read-only weight buffers regressed tg512 and
shared should remain default for that probe. The current `new_readonly_buffer`
defaults to private Metal buffers unless `CTOX_QWEN35_SHARED_WEIGHTS=1` is set.
This storage-mode drift is now explicitly measurable with:

  tools/run_decode_regression_matrix.sh --storage-sweep ...

Do not infer storage-mode policy from a hot machine run; storage needs a cooled,
alternating matrix before promotion.
```

CPU orchestration change:

```text
Changed the Metal pipeline-state cache in `src/metal/ffi.rs` from a linear Vec
scan to a HashMap. This removes avoidable String comparisons and keeps the
cache behavior explicit. It is not a large measured decode win by itself.
```

Negative LM-head SIMD experiment:

```text
Added opt-in:
  CTOX_QWEN35_DECODE_LM_HEAD_SIMD32=1
  qwen35_08b_lm_head_argmax_rowtiles_simd32_f32_tiled_k1024

The kernel replaces 8x256 threadgroup-memory reductions with SIMD32 `simd_sum`
and only 8x8 threadgroup partials.

tg128 paired run:
  baseline: 2.913204125 s -> 43.94 tok/s
  SIMD32:   3.440843208 s -> 37.20 tok/s

Decision:
  Keep opt-in only as a negative/control candidate. On this shape, the simpler
  threadgroup reduction is faster than the theoretically cleaner SIMD32 version.
```

Async command-buffer experiment:

```text
Added opt-in:
  CTOX_QWEN35_DECODE_ASYNC_COMMANDS=1

For decode sequences with `steps > 32`, intermediate token command buffers are
queued on the same command queue without CPU wait; the final token waits before
CPU readback. This preserves the token-buffer dependency through queue order.

Measurements after rebuild:

tg128:
  baseline: 2.289521167 s -> 55.91 tok/s
  async:    2.305199584 s -> 55.53 tok/s

tg512:
  baseline: 9.199383333 s -> 55.66 tok/s
  async:    9.180149542 s -> 55.77 tok/s

llama.cpp references:
  tg128: 52.98 tok/s
  tg512: 44.77 tok/s
```

Interpretation:

```text
Decode can still beat llama.cpp on cooled, clean tg128/tg512 runs. The earlier
"regression" was partly a measurement-state problem: long serial matrices heat
or otherwise perturb the system enough that later rows collapse. That does not
mean the problem is solved; sustained-performance reporting must include round
order, optional storage/sync sweeps, and hardware state. But it does mean the
current accepted decode path is not fundamentally below llama.cpp.

Async command queuing is neutral/slightly positive at tg512 but not enough for
promotion. CPU per-token wait is not the dominant decode bottleneck right now.
```

Tooling update:

```text
tools/run_decode_regression_matrix.sh now supports:
  --storage-sweep
  --sync-sweep

Use these when investigating storage-mode or CPU-sync claims.
```

## 2026-05-01 20:24 CEST - Prefill Attention Backend Matrix: Exact Integration Target

Re-centered work on the remaining exact long-prefill gap.

Current reference report:

```text
exact_mps_deltaout:
  p4096:   3112.46 tok/s vs llama.cpp 2852.70 = 1.091x
  p16384:  1396.40 tok/s vs llama.cpp 2065.71 = 0.676x
  p32768:   786.90 tok/s vs llama.cpp 1325.20 = 0.594x

next_exact_target:
  reduce long-context attention traffic without semantic windowing
```

Measured real accepted attention-core versus the Rust MPS tiled exact-attention
prototype:

```text
tools/run_prefill_attention_backend_matrix.sh \
  --sizes 4096,16384,32768 \
  --accepted-iters 2 \
  --tiled-iters 2 \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack

tokens=4096:
  accepted_s              0.271157792
  tiled_mps_bridge_s      0.015845125
  accepted/tiled_bridge   17.11x

tokens=16384:
  accepted_s              2.171023792
  tiled_mps_bridge_s      0.205345750
  accepted/tiled_bridge   10.57x

tokens=32768:
  accepted_s              7.743548542
  tiled_mps_bridge_s      0.800409333
  accepted/tiled_bridge   9.67x
```

Contract:

```text
accepted:
  real current metalpack attention-core path

tiled_mps:
  synthetic Qwen-layout bridge packs accepted-layout Q/K/V caches, then runs
  the inner QK-softmax-PV architecture for both Qwen KV groups using Rust C-ABI
  MPSMatrix QK/PV plus MSL SIMD32 softmax/combine/store.

The MPS row is not accepted-profile performance. It does not yet include real
QKV projection, O projection, hidden-dump parity, or full model wiring.
```

Correctness fix before integration:

```text
bug:
  softmax/store used query_row = row % q_tile

actual MPS Q row layout:
  row = local_token * heads_per_group + head_in_group

fix:
  query_row = row / heads_per_group
```

Validation:

```text
target/release/bench_tiled_attention_mps 512 128 256 1 1 4 1 0
quality_mean_abs_error: 0.000075166
quality_max_abs_error:  0.000277638
```

Learning: A fast sidecar benchmark is worthless until its layout-to-causal-mask
mapping is independently checked. The bridge still shows a roughly 9.7x-17.1x
inner-attention opportunity after the fix, so integration remains justified.

Stage profile at p16384:

```text
tools/profile_attention_variant_stages.sh \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  16384 3 2 CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_HALFDOT=1

accepted:
  prepare_s              0.129162542
  attention_cumulative_s 1.688168042
  attention_only_s       1.559005500

halfdot approximate:
  prepare_s              0.122560667
  attention_cumulative_s 0.972122500
  attention_only_s       0.849561833
```

Interpretation:

```text
The p16k accepted attention path is dominated by the attention-only stage, not
by QKV prepare. Approximate halfdot confirms that changing the inner attention
math moves the needle, but exact acceptance requires the MPS tiled path or a
similarly strong exact kernel.

The next exact implementation target is:
  Q/K/V real buffers -> MPS tiled QK-softmax-PV -> real attn tensor -> real O projection

Quality gate:
  sparse output comparison first, then hidden-dump parity through the full
  attention core, then model-wide p16k/p32k projection.
```

Added:

```text
tools/run_prefill_attention_backend_matrix.sh
```

The tool serializes accepted-vs-MPS-tiled measurements and prints the semantic
contract so this comparison cannot be mistaken for an accepted speed claim.

## 2026-05-01 18:20 CEST - Real MPS Tiled Attention Core Integration

Integrated the corrected MPS tiled exact-attention path into the real
`bench_metalpack_prefill_attention_core` path behind:

```text
CTOX_QWEN35_ATTENTION_MPS_TILED=1
CTOX_QWEN35_ATTENTION_MPS_Q_TILE=256   # optional
CTOX_QWEN35_ATTENTION_MPS_K_TILE=1024  # optional, capped to tokens
```

Implementation details:

```text
real q/k/v projection -> prepare q_cache/k_cache/v_cache
  -> pack one KV group into MPS row/column-major scratch
  -> MPSMatrix QK
  -> MSL SIMD32 causal online softmax update
  -> MPSMatrix P*V
  -> MSL online combine
  -> gated store into real attn[token, q_head, dim]
  -> existing O projection path
```

Correctness gate, p512 attention raw dump vs existing exact MSL path:

```text
target/release/compare_attention_raw_dump \
  /tmp/ctox_attn_base_p512.bin \
  /tmp/ctox_attn_mps_p512.bin \
  512 2048

elements:        1048576
mean_abs_error:  0.000075514
rms_error:       0.000153930
max_abs_error:   0.002441406
checksum_delta:  0.171045542
```

The errors are expected FP16/MPS ordering drift, not bitwise parity.

Attention-core timing against the accepted QH4 SIMD32 path:

```text
p4096:
  accepted QH4:          0.140755542 s
  MPS tiled exact:       0.048725792 s
  speedup:              2.89x

p16384:
  accepted QH4:          1.664662708 s
  MPS tiled exact:       0.330725833 s
  speedup:              5.03x

p32768:
  accepted QH4:          6.564904291 s
  MPS tiled exact:       1.011043166 s
  speedup:              6.49x
```

Projected full-prefill impact if all six full-attention layers replace accepted
QH4 with MPS tiled exact attention, using the previous `exact_mps_deltaout`
full-prefill projection as baseline:

```text
p4096:
  old exact projection:  1.316000000 s = 3112.46 tok/s
  projected new:         0.763821500 s = 5362.51 tok/s
  llama.cpp reference:   2852.70 tok/s
  projected vs llama:    1.88x

p16384:
  old exact projection:  11.733000000 s = 1396.40 tok/s
  projected new:         3.729378750 s = 4393.23 tok/s
  llama.cpp reference:   2065.71 tok/s
  projected vs llama:    2.13x

p32768:
  old exact projection:  41.642000000 s = 786.90 tok/s
  projected new:         8.318833250 s = 3939.02 tok/s
  llama.cpp reference:   1325.20 tok/s
  projected vs llama:    2.97x
```

Learning: The large prefill gap was not primarily QKV prepare or O projection.
It was the exact long-context inner attention loop failing to use the platform
matrix backend. The MPS tiled path converts the attention core from one custom
SIMD scan per query into tiled matrix work plus thin MSL softmax/combine glue.
This is the first exact path whose projected model-wide long-prefill performance
is comfortably above llama.cpp without semantic windowing.

Next gate:

```text
1. Add a repeatable full-prefill projection tool that substitutes measured MPS
   attention-core timings instead of static Python constants.
2. Run p4096/p16384 raw-dump parity on several layers, not just layer 3 p512.
3. If stable, promote MPS tiled attention into candidate profile and update the
   accepted profile only after end-to-end prefill projection validation.
```

Follow-up tool added:

```text
tools/run_prefill_mps_tiled_projection.sh
```

Live projection run:

```text
tools/run_prefill_mps_tiled_projection.sh \
  --sizes 4096,16384,32768 \
  --iters 2 \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack

tokens=4096
  accepted_attention_s:     0.120393000
  mps_tiled_attention_s:    0.047869167
  attention_speedup:        2.52x
  projected_full_s:         0.880857002
  projected_tok_s:          4650.02
  projected_vs_llama:       1.630x

tokens=16384
  accepted_attention_s:     1.632261083
  mps_tiled_attention_s:    0.326072458
  attention_speedup:        5.01x
  projected_full_s:         3.895868250
  projected_tok_s:          4205.48
  projected_vs_llama:       2.036x

tokens=32768
  accepted_attention_s:     6.649290125
  mps_tiled_attention_s:    1.069706084
  attention_speedup:        6.22x
  projected_full_s:         8.164495754
  projected_tok_s:          4013.48
  projected_vs_llama:       3.029x
```

This supersedes the hand-calculated projection values above for reporting.

p4096 attention raw-dump parity against accepted QH4:

```text
target/release/compare_attention_raw_dump \
  /tmp/ctox_attn_accepted_p4096.bin \
  /tmp/ctox_attn_mps_p4096.bin \
  4096 2048

elements:        8388608
mean_abs_error:  0.000062596
rms_error:       0.000141654
max_abs_error:   0.002441406
checksum_delta: -0.042503059
```

This moves MPS tiled attention from synthetic-prioritization evidence to a real
candidate backend for the full-attention layers. The next blocker is not
attention-core math anymore; it is full-prefill wiring/profile promotion and
checking that the six attention layers behave the same way as layer 3.

## 2026-05-01 18:55 CEST - Full-Prefill Forensics With MPS Tiled Attention

Packed fresh MPS sidecars:

```text
/tmp/ctox_qwen35_mps_ffn_sidecar
/tmp/ctox_qwen35_mps_delta_project_sidecar
/tmp/ctox_qwen35_mps_delta_out_sidecar
/tmp/ctox_qwen35_mps_attention_out_sidecar
```

Updated `memory_forensics` so `CTOX_QWEN35_ATTENTION_MPS_TILED=1` uses the
integrated exact MPS tiled attention core instead of forcing the accepted QH4
SIMD32 path.

Command shape:

```text
CTOX_QWEN35_ATTENTION_MPS_TILED=1 \
  target/release/memory_forensics \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  <tokens> 2 150 \
  /tmp/ctox_qwen35_mps_ffn_sidecar \
  /tmp/ctox_qwen35_mps_delta_project_sidecar \
  /tmp/ctox_qwen35_mps_attention_out_sidecar \
  /tmp/ctox_qwen35_mps_delta_out_sidecar
```

Full-prefill forensics estimates:

```text
p4096:
  delta18+ffn:          692.383 ms
  attention.core:        50.486 ms
  attention.ffn:          9.610 ms
  full_prefill:        1.053 s
  tok/s:              3889.99
  llama.cpp tok/s:    2852.70
  vs llama.cpp:          1.36x

p16384:
  delta18+ffn:         2789.506 ms
  attention.core:       317.587 ms
  attention.ffn:         37.667 ms
  full_prefill:        4.921 s
  tok/s:              3329.39
  llama.cpp tok/s:    2065.71
  vs llama.cpp:          1.61x

p32768:
  delta18+ffn:         5553.995 ms
  attention.core:       968.732 ms
  attention.ffn:         81.913 ms
  full_prefill:       11.858 s
  tok/s:              2763.40
  llama.cpp tok/s:    1325.20
  vs llama.cpp:          2.09x
```

This supersedes the earlier hand projection as the current exact-prefill status.
The MPS tiled attention backend plus sidecars closes the previous long-prefill
gap against llama.cpp in the forensics estimator.

New bottleneck:

```text
Delta18+FFN with sidecars dominates all three sizes:
  p4096:  692 ms of 1053 ms
  p16k:  2790 ms of 4921 ms
  p32k:  5554 ms of 11858 ms
```

Forensics diagnosis: weight streaming is no longer the modeled gap for the
Delta stack when sidecars are enabled. The big remaining delta is unmodeled
stall/scan/gated-norm/out orchestration and custom MSL state work. Attention is
now fast enough to stop being the macro blocker for exact long prefill.

## 2026-05-01 10:18 CEST - MPS Tiled Attention Promoted To Accepted Profile

Promotion artifacts:

```text
experiment: docs/kernel-dev/experiments/20260501T081544Z-mps-tiled-attention.md
forensics:  docs/kernel-dev/forensics/20260501T081544Z-mps-tiled-attention.md
decision:   docs/kernel-dev/decisions/20260501T081544Z-mps-tiled-attention-accepted.md
proposal:   docs/kernel-dev/profile-updates/20260501T081741Z-mps-tiled-attention.md
```

Accepted profile change:

```diff
- export CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1
+ export CTOX_QWEN35_ATTENTION_MPS_TILED=1
```

Post-change checks:

```text
tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  active_flags: 10
  sha256: 9fbaabb2d5219904e92d5af877dc82aa8c9cabcc590a8f90ee2f1474c00ff8d4

tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env
  validation: PASS
  autotune_baseline_flags: 9

tools/run_accepted_profile.sh target/release/bench_metalpack_prefill_attention_core \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 3 4096 2 1
  median_s: 0.059546958
  effective_gb_s_attention_core_estimate: 1206.58
  checksum16: 0.000000

tools/kernel_dev_doctor.sh
  validation: PASS
```

This closes the previous exact long-prefill attention gap at the accepted-profile
level. QH4 SIMD32 remains available as a fallback/negative control, but the
default attention backend is now exact MPS tiled attention.

## 2026-05-01 10:30 CEST - Delta Scan SIMD32 Shared Q/K Candidate

Implemented a new approximate DeltaNet scan candidate:

```text
CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1
kernel:
  qwen35_08b_prefill_deltanet_scan_lanes4_sharedqk_f32_state_tok_h16d128
```

Rationale: plain `CTOX_QWEN35_DELTA_SCAN_LANES4=1` proved that assigning one
SIMDgroup to one Delta state row can reduce the scan work at p4096, but it
reloaded the same token-local Q/K vector for each row group and lost most of the
benefit at p16384. The shared-Q/K variant keeps the SIMDgroup row ownership but
loads Q/K once into threadgroup memory for the four rows in the threadgroup.

Tooling fixes made before measuring:

```text
tools/compare_delta_stack_candidate.sh
  now accepts:
    --mps-ffn-sidecar
    --mps-delta-project-sidecar
    --mps-delta-out-sidecar

target/release/profile_metalpack_prefill_delta_stack
  now accepts the same sidecar positions as the stack benchmark.

target/release/memory_forensics
  now reports:
    delta_scan_backend: exact rowcache_block32
    delta_scan_backend: approximate SIMD32 lanes4_sharedqk
  and supports:
    CTOX_QWEN35_FORENSICS_DELTA_SCAN_LANES4_SHAREDQK=1
```

Paired Delta18+FFN stack with MPS sidecars:

```text
p512:
  baseline_median_s:   0.071066604
  candidate_median_s:  0.061626000
  median_delta:       -13.2842%

p4096:
  baseline_median_s:   0.548291562
  candidate_median_s:  0.476927041
  median_delta:       -13.0158%

p16384:
  baseline_median_s:   2.198526084
  candidate_median_s:  1.917534208
  median_delta:       -12.7809%
```

p4096 hidden dump against exact rowcache_block32:

```text
mismatch_count:      3739506 / 4194304
mean_abs_error:      0.001943609
rms_error:           0.002542885
max_abs_error:       0.046875000
checksum_delta:      -22.414070845
baseline_checksum16: -0.927307
candidate_checksum16:-0.919556
```

Full-prefill forensics with exact MPS tiled attention:

```text
p4096:
  exact rowcache_block32:       0.853s, 4800.11 tok/s
  approx lanes4_sharedqk:       0.772s, 5306.76 tok/s

p16384:
  exact rowcache_block32:       4.000s, 4095.63 tok/s
  approx lanes4_sharedqk:       3.727s, 4396.51 tok/s

p32768:
  exact rowcache_block32:       9.684s, 3383.65 tok/s
  approx lanes4_sharedqk:       9.117s, 3594.08 tok/s
```

Decision:

```text
status: opt-in approximate
decision: docs/kernel-dev/decisions/20260501T083010Z-delta-scan-lanes4-sharedqk-optin.md
experiment: docs/kernel-dev/experiments/20260501T083010Z-delta-scan-lanes4-sharedqk.md
```

Learning: SIMD is necessary but not sufficient. The useful pattern is
SIMDgroup row ownership plus a data layout/cache plan that avoids reloading the
same token-local vectors. `LANES4_SHAREDQK` is faster than exact rowcache at
all measured sizes, but the reduction-order drift is too large for exact
accepted-profile semantics. Treat it like quantization: explicit opt-in,
model-level quality gates required.

Follow-up correctness guard from code inspection:

```text
prefill_delta_scan_gated_norm_enabled()
  now disables fused scan+gated-norm when these incompatible scan/layout modes
  are active:
    CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT
    CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64
    CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32
    CTOX_QWEN35_DELTA_SCAN_LANES4
    CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK
    CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED
    CTOX_QWEN35_MPS_QKVZ_DIRECT
```

Reason: the fused gated-norm kernels use the non-block row shape and assume
`z_out` ownership. Combining them with block scan shapes or qkvz-direct can
silently wire the wrong shape/buffer. The guard keeps autotune from measuring a
fast but invalid path.

## 2026-05-01 10:40 CEST - Rejected Delta GatedNorm SIMD32x4

Implemented an env-gated standalone gated RMSNorm candidate:

```text
CTOX_QWEN35_DELTA_GATED_NORM_SIMD32X4=1
kernels:
  qwen35_08b_prefill_deltanet_gated_rmsnorm_simd32x4_tok_h16d128_f32_to_fp16
  qwen35_08b_prefill_deltanet_gated_rmsnorm_qkvz_simd32x4_tok_h16d128_f32_to_fp16
```

Hypothesis: one SIMDgroup per `(token, head)` with four columns per lane should
remove the 128-thread tree-reduction barriers in separate gated RMSNorm, while
leaving the recurrent Delta scan order untouched.

Measured with MPS sidecars:

```text
p512:
  baseline_median_s:   0.081951354
  candidate_median_s:  0.081622729
  median_delta:       -0.4010%
  baseline_checksum16: -0.927307
  candidate_checksum16:-0.911438

p4096:
  baseline_median_s:   0.685376292
  candidate_median_s:  0.699806146
  median_delta:        2.1054%
  baseline_checksum16: -0.927307
  candidate_checksum16:-0.911438
```

Decision:

```text
status: rejected
decision: docs/kernel-dev/decisions/20260501T084028Z-delta-gated-norm-simd32x4-rejected.md
experiment: docs/kernel-dev/experiments/20260501T084028Z-delta-gated-norm-simd32x4.md
```

Learning: not every 128-thread reduction should be converted to SIMD32x4.
Here the standalone norm is not dominant enough, and the changed reduction
order still shifts the output. The candidate is removed from regular autotune
search; keep it only as a negative control.

Tooling follow-up:

```text
target/release/autotune_metalpack_prefill_delta_stack
  now accepts:
    [mps-ffn-sidecar-dir]
    [mps-delta-project-sidecar-dir]
    [mps-delta-out-sidecar-dir]
```

Smoke:

```text
CTOX_QWEN35_AUTOTUNE_SKIP_VALIDATE=1 \
target/release/autotune_metalpack_prefill_delta_stack \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack \
  512 1 1 18 0 0 \
  /tmp/ctox_qwen35_mps_ffn_sidecar \
  /tmp/ctox_qwen35_mps_delta_project_sidecar \
  /tmp/ctox_qwen35_mps_delta_out_sidecar

result:
  sidecar args printed
  accepted baseline measured on sidecar path
  passes=0 smoke completed
```

Learning: all tuning/compare/profile tools must accept the same sidecar
surface as the benchmark. Otherwise the tools optimize an obsolete pipeline and
produce misleading kernel decisions.

## 2026-05-01 10:48 CEST - Compare Tool Reset Mode And Exact Tile Candidate Rejected

Added fair comparison mode:

```text
tools/compare_delta_stack_candidate.sh --candidate-reset-tuning-env
```

Reason: `run_accepted_profile.sh` sources accepted flags after the caller's
environment, so mutually-exclusive candidates such as `QKVZ_MMA64` can be
hidden by accepted `QKVZ_MMA128`. Reset mode sources the accepted profile,
unsets the Delta stack tuning family, and then applies the candidate env.

The p512 sidecar autotune suggested an exact candidate:

```text
CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64=1
CTOX_QWEN35_DELTA_OUT_MMA32=1
CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL=1
CTOX_QWEN35_FFN_GATE_UP_MMA64=1
CTOX_QWEN35_DOWN_MMA32=1
CTOX_QWEN35_DOWN_MMA32_RESIDUAL=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1
CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
```

Fair paired result with reset mode:

```text
p512:
  baseline_median_s:   0.079356167
  candidate_median_s:  0.079647792
  median_delta:        0.3675%

p4096:
  baseline_median_s:   0.626822791
  candidate_median_s:  0.620857334
  median_delta:       -0.9517%

p16384:
  baseline_median_s:   2.507749625
  candidate_median_s:  2.517545333
  median_delta:        0.3906%
```

Decision: reject for now. The candidate is exact by checksum, but the gain is
small, inconsistent, and reverses at p512/p16384. Do not promote micro-tile
changes without robust paired wins across token sizes.

## 2026-05-01 10:54 CEST - Rejected Rowcache Block Auto

Implemented an exact token-aware scan selector:

```text
CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1
CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64_MIN_TOKENS=4096 default
```

Behavior:

```text
tokens < 4096:
  qwen35_08b_prefill_deltanet_scan_rowcache_block32_f32_state_tok_h16d128

tokens >= 4096:
  qwen35_08b_prefill_deltanet_scan_rowcache_block64_f32_state_tok_h16d128
```

Fair paired result with reset mode:

```text
p512:
  baseline_median_s:   0.075132958
  candidate_median_s:  0.072797500
  median_delta:       -3.1084%

p4096:
  baseline_median_s:   0.591575917
  candidate_median_s:  0.600406791
  median_delta:        1.4928%

p16384:
  baseline_median_s:   2.373350500
  candidate_median_s:  2.415476166
  median_delta:        1.7749%
```

Decision:

```text
status: rejected / keep opt-in only
decision: docs/kernel-dev/decisions/20260501T085442Z-delta-scan-rowcache-block-auto-rejected.md
experiment: docs/kernel-dev/experiments/20260501T085442Z-delta-scan-rowcache-block-auto.md
```

Learning: rowgroup size is not the structural scan fix. Block32 and block64 are
close enough that measurement order and token size can flip the winner. The
exact path needs a deeper scan-state/math redesign or a backend change, not a
threshold around rowgroup size.

Tool added:

```text
tools/run_delta_scan_family_sweep.sh
```

Purpose: serial, reset-based scan-family comparisons on the current MPS sidecar
pipeline. This prevents two recurring mistakes:

```text
1. measuring scan candidates without the sidecars used by the real pipeline
2. comparing mutually-exclusive scan flags while accepted-profile flags remain
   active
```

Smoke:

```text
tools/run_delta_scan_family_sweep.sh --tokens 512 --rounds 1 --iterations 1 --warmup 1

rowcache:
  median_delta: -0.5889%, checksum exact
rowcache_direct:
  median_delta: +9.6223%, checksum exact
rowcache_block64:
  median_delta: +0.2329%, checksum exact
rowcache_block32:
  median_delta: -0.0194%, checksum exact
rowcache_block_auto:
  median_delta: -0.3497%, checksum exact
lanes4_sharedqk_approx:
  median_delta: -5.7769%, checksum drift to -0.919556
```

Learning: the scan family now has a repeatable discovery tool. Exact rowcache
variants are too close for hand-picked one-off promotion; approximate lanes4
remains the fast control that needs model-level quality gates.

## 2026-05-01 11:08 CEST - Isolated Delta Scan Sweep And Corrected Byte Model

Implemented scan-only forensics:

```text
target/release/bench_metalpack_prefill_delta_scan
  new args: [warmup] [validate_tokens]
  new output: kernel, grid, threads, bytes_moved_estimate

tools/run_delta_scan_isolated_sweep.sh
  serial scan-only sweeps
  cases: plain, rowcache, rowcache_direct, rowcache_block64,
         rowcache_block32, rowcache_block_auto, lanes4_sharedqk_approx
  summary: median_s, tok_s, vs_block32, mean_gbps, error, kernel, dispatch
```

Fixed a measurement bug: the scan benchmark's previous GB/s estimate treated
rowcache kernels like the plain kernel and assumed repeated state streaming per
token. The corrected model uses:

```text
plain:
  repeated state stream per token

rowcache / lanes4:
  persistent state read/write once
  q/k/v + beta/decay + out stream per token
```

This makes the byte model more honest, but also changes interpretation:
`effective_GB/s` is no longer directly comparable across plain and rowcache
families. Rowcache should be judged by `median_s`, `tok_s`, `vs_block32`, and a
separate compulsory/avoided byte model.

Smoke:

```text
target/release/bench_metalpack_prefill_delta_scan \
  /tmp/ctox_qwen35_08b_real_fp16.metalpack 0 128 1 1 4

kernel: qwen35_08b_prefill_deltanet_scan_f32_state_tok_h16d128
grid: 16x1x1
threads: 128x1x1
bytes_moved_estimate: 405291008
max_abs_error_out_validate8: 0.000000010
max_abs_error_state_validate8: 0.000000011
```

Isolated scan sweep:

```text
tools/run_delta_scan_isolated_sweep.sh \
  --tokens 512,4096,16384 \
  --rounds 2 \
  --iterations 3 \
  --warmup 2 \
  --validate-tokens 8 \
  --output-dir /tmp/ctox_qwen35_scan_isolated_20260501T_continue
```

Key results:

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

Exact family result:

```text
rowcache_block32 remains the strongest exact scan variant in this sweep.
rowcache, direct, block64, and block_auto do not beat it robustly.
```

Approx family result:

```text
lanes4_sharedqk is structurally faster in isolated scan timing, but remains
opt-in approximate because the prior full-stack hidden dump drifted:
  mean_abs_error: 0.001943609
  rms_error:      0.002542885
  max_abs_error:  0.046875000
  checksum_delta: -22.414070845
```

Longer synthetic validation note:

```text
validate_tokens=512:
  lanes4_sharedqk max_abs_error_out:   0.000000024
  lanes4_sharedqk max_abs_error_state: 0.000000015
```

Learning: synthetic isolated q/k/v validation can miss model-pipeline drift.
For exact promotion, scan candidates still need full hidden/logit/greedy gates.
For approximate promotion, they need an explicit quantization-style error
acceptance policy.

Records:

```text
experiment: docs/kernel-dev/experiments/20260501T090838Z-delta-scan-isolated-sweep.md
forensics:  docs/kernel-dev/forensics/20260501T091106Z-isolated-delta-scan-byte-model.md
```

## 2026-05-01 21:10 CEST - Documentation Audit After Beating Reference Prefill

Reviewed the kernel docs after the exact prefill forensics crossed the
llama.cpp reference at p4096/p16384/p32768.

Finding:

```text
The knowledge base had the right raw material:
  reference report tool
  decode regression matrix
  accepted/approx decision records
  cache and scan forensics
  hardware/backend grid

But the top-level orientation was stale:
  KERNEL_DEV_HANDBOOK still framed llama.cpp as generally faster
  README did not show the current reference status near the top
  docs/kernel-dev/README did not state how to distinguish accepted,
  forensics, and approximate wins
```

Updated:

```text
KERNEL_DEV_HANDBOOK.md:
  added Current Reference Status
  changed "Why llama.cpp Is Still Faster" into "llama.cpp Transfer Lessons"
  replaced the old gap framing with current prefill/decode interpretation

README.md:
  added Current Reference Status with exact prefill, approximate prefill, and
  cooled decode numbers

docs/kernel-dev/README.md:
  added Current Outcome and the accepted/forensics/approx distinction
```

Current summary to keep visible:

```text
exact prefill:
  p4096:  4801.88 tok/s vs llama.cpp 2852.70 = 1.683x
  p16384: 4096.00 tok/s vs llama.cpp 2065.71 = 1.983x
  p32768: 3383.73 tok/s vs llama.cpp 1325.20 = 2.553x

decode:
  cooled tg128: 55.91 tok/s vs llama.cpp 52.98 = 1.055x
  cooled tg512: 55.66 tok/s vs llama.cpp 44.77 = 1.243x
```

Learning:

```text
Docs must not only archive experiments; they must surface the current strategic
state. Once a reference target is beaten, old gap-language becomes misleading
and should be rewritten into transferable lessons plus remaining acceptance
risks.
```
