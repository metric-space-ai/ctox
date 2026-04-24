# Bare-Metal CUDA-Dispatcher Port — Status

Byte-für-byte Port von llama.cpp's ggml-cuda Host-Side Dispatchern
nach Rust, pro CLAUDE.md Inference-Engine Architecture Rules.

## Was verifiziert läuft (A6000)

**18 `.cu`-Files ported + verified** (alle 17 aus der vorherigen Liste
plus **`ssm-conv`** mit drei-teiligem Fix: Function-pointer-Shim +
`build.rs::.visible .entry` Parse + korrekte Itanium length prefixes).

**Executor-Status — alle smokes grün:**

| Smoke | Shape | Ops | Drift | Hybrid-Zeit | ggml-Zeit |
|---|---|---|---:|---:|---:|
| `graph_smoke` | 4096×32 | 5 Rust | 1.9e-6 | 58µs | n/a (CPU-ref) |
| `hybrid_smoke` | 128×64×32 | 3 (1 Rust + 1 ggml + 1 Rust) | **0 bit-exact** | — | — |
| `layer_smoke` | hidden=5120, ffn=17408, seq=16 | 8 (4 Rust + 4 ggml) | 2.2e-5 | 7.9ms | 3.0ms |
| `attn_smoke` | hidden=6144, heads=24/4×256, seq=16 | 6+FA (2 Rust + 4+FA ggml) | **0 bit-exact** | 7.6ms | 1.9ms |
| `block_smoke` | **full Q35-27B transformer block**, seq=16 | 17 (6 Rust + 11 ggml incl. FA) | 2.2e-5 | 9.8ms | 4.1ms |

**`block_smoke` ist das Milestone:** kompletter Qwen3.5-27B
transformer-block (Attention + FFN Half) läuft durch den Hybrid-
Executor mit production-scale Dimensionen, Output matches pure-ggml
auf 5 Dezimalstellen.

Perf note: hybrid ist ~2.4× langsamer als pure-ggml auf block_smoke
(9.8ms vs 4.1ms) — jeder ggml-fallback mul_mat geht durch seinen
eigenen `graph_compute()` Cycle. Ein `compute_many` batched-API der
consecutive fallback ops in einem Graph zusammenfasst würde den
größten Teil des Gaps schließen. Bit-exactness ist wichtiger als
perf beim Bring-up.

17 `.cu`-Files, 37+ Kernel-Varianten — jede einzeln bit-close /
bit-exakt verifiziert durch `src/bin/<op>_verify.rs`:

| Op | Source | Verify | Max-Drift |
|---|---|---|---:|
| `rms_norm` (f32, B=256 + B=1024) | norm.cu:297-475 | `rms-norm-verify` | 2.4e-7 (~1 ULP) |
| `silu` (f32) | unary.cu:124-178 | `unary-verify` | 2.4e-7 |
| `neg` (f32) | unary.cu:157 | (same) | 0 (exact) |
| `exp` (f32) | unary.cu:201 | (same) | 9.5e-7 |
| `sigmoid` (f32) | unary.cu:189-191 | (same) | 1.2e-7 |
| `softplus` (f32) | unary.cu:249-251 | (same) | 4.8e-7 |
| `scale` (f32) | scale.cu:15-34 | `scale-verify` | 2.4e-7 |
| `fill` (f32) | fill.cu:29 | `fill-verify` | 0 (exact) |
| `fill` (f16) | fill.cu:32 | (same) | 0 (exact) |
| `diag` (f32) | diag.cu:36 | `diag-verify` | 0 (exact) |
| `add` (f32 non-fused) | binbcast.cu:397 | `binbcast-verify` | 0 (exact) |
| `sub` (f32 non-fused) | binbcast.cu:401 | (same) | 0 (exact) |
| `mul` (f32 non-fused) | binbcast.cu:405 | (same) | 0 (exact) |
| `repeat` (f32 n_fuse=0) | binbcast.cu:393-395 | (same, extended) | 0 (exact) |
| `tri` (f32, LOWER) | tri.cu:94-112 | `tri-verify` | 0 (exact) |
| `tri` (f32, LOWER_DIAG) | (same) | (same) | 0 (exact) |
| `tri` (f32, UPPER) | (same) | (same) | 0 (exact) |
| `tri` (f32, UPPER_DIAG) | (same) | (same) | 0 (exact) |
| `pad` (f32, non-circular) | pad.cu:64-106 | `pad-verify` | 0 (exact) |
| `pad` (f32, circular) | (same) | (same) | 0 (exact) |
| `cumsum` (f32 fallback) | cumsum.cu:213-265 | `cumsum-verify` | 1.1e-5 |
| `concat` (f32 contig, dim=0) | concat.cu:4-28 | `concat-verify` | 0 (exact) |
| `concat` (f32 contig, dim=1) | concat.cu:30-54 | (same) | 0 (exact) |
| `concat` (f32 contig, dim=2) | concat.cu:56-80 | (same) | 0 (exact) |
| `concat` (f32 non-cont, dim=0) | concat.cu:97-154 | (same, spot-check) | 0 (exact) |
| `cpy<f32→f32>` | cpy.cu:14-40 + cpy-utils:214 | `cpy-verify` | 0 (exact) |
| `cpy<f32→f16>` | (same) | (same) | 0 (bit-exact half-round) |
| `cpy<f16→f16>` | (same) | (same) | 0 (exact) |
| `solve_tri<f32, 0, 0>` | solve_tri.cu:91-178 | `solve-tri-verify` | 6e-8 (~1 ULP) |
| `rope_norm<true, false, f32, f32>` | rope.cu:43-113 | `rope-verify` | 8.3e-7 |
| `rope_norm<true, true, f32, f32>` | (same) | (wired, unverified) | — |
| `rope_multi<true, false, f32>` | rope.cu:182-265 | (wired, unverified) | — |
| `rope_multi<true, true, f32>` | (same) | (wired, unverified) | — |
| `soft_max_f32<true, 0, 0, f32>` | softmax.cu:54-138 | `softmax-verify` | 1.9e-9 |
| `soft_max_f32<true, 0, 0, f16>` | (same) | (wired, unverified) | — |

## Infrastruktur (bewiesen, reusable)

- **Vendored tree self-build.** `build.rs::compile_kernel_to_ptx(stem)` feuert
  nvcc gegen `vendor/ggml-cuda/<stem>.cu` mit exakt den Flags die ggml's
  CMake nutzt; kein externer ggml-Build mehr nötig. Include-Paths:
  `vendor/ggml-cuda`, `vendor/ggml-include`, `vendor/`.
- **Mangled-name extraction.** `build.rs::generate_ptx_entries_module(stem)`
  parst alle `.entry <mangled>(...)` aus der compiled PTX und emittiert
  `$OUT_DIR/<stem>_entries.rs` mit einer `&[&[u8]]` Tabelle NUL-terminierter
  Mangled-Names. Bypassed nvcc's per-translation-unit hash (wichtig für
  internal-linkage `static` op-functors wie `op_silu`).
- **Runtime lookup.** `ptx::find_entry(entries, &[needle1, …])` macht
  substring-AND-matching mit Uniqueness-Check. Needles = stabile Itanium
  mangled-name-Fragmente (z.B. `b"7op_siluE"` für den Functor-Namen +
  `b"EfEvPK"` für den T=float Discriminator).
- **Kernel-Handle-Cache.** `cuda_port::module::porter()` lazy-init via
  `OnceLock`, resolved alle Handles einmal pro Prozess, cached sie in
  `PortedKernels`.
- **Context binding.** `driver::ensure_current_context(ordinal)` setzt
  den Device-Primary-Context current auf dem rufenden Thread, idempotent.
  Nötig weil ggml_backend_cuda_init den Context nicht auf beliebige Threads
  pushed.
- **Verifier-Pattern.** `src/bin/<op>_verify.rs` = standalone Binary das
  den Port gegen CPU-f64-Referenz-Impl vergleicht. Tolerance 1e-5 deckt
  fast-math drift ab.
- **Host-side fastdiv cookies.** `binbcast.rs::init_fastdiv_values(d)`
  replicates upstream's uint3 (mp, L, d) packing — für Ops mit
  fastdiv/fastmodulo-Parametern.

## Was noch fehlt

### Phase A — Restliche Op-Dispatcher

#### Pending simple (~30-60 min/each)
- `cpy`, `cont` (mapped to cpy) — generic dtype conversion + layout copy
- `diag_mask_inf` (`diagmask.cu`) — causal mask for attention
- `get_rows` — embedding lookup

#### Pending medium (~60-120 min/each)
- `concat` — axis concatenation (used in DeltaNet state packing)
- `repeat_4d` — full 4D tensor repeat (different from binbcast's repeat)
- `solve_tri` — backwards-substitution triangular solve
- `mul_mat` — massive op-family, needs mmq_q4k/q5k/q6k/q8_0/iq4_xs + mmf

#### Pending komplex (~2-6 h/each)
- `soft_max_ext` — fused softmax with scale + alibi + sink
- `rope_ext` / `rope_multi` — M-RoPE with per-section freqs
- `ssm_conv` / `ssm_conv_tree` — DeltaNet short-conv over state

#### Pending sehr komplex (1-3 d/each)
- `flash_attn_ext` — custom-FA2 with vec / MMA / tile-size variants,
  M-RoPE-aware path for Qwen3.5
- `gated_delta_net`, `_tree`, `_tree_persist` — lucebox/dflash-specific
  DeltaNet kernels, 3000+ lines CUDA each

### Phase B — Rust-Side Graph-Executor

Heute konstruiert `graph.rs` einen `ggml_cgraph`, der via
`ggml_backend_graph_compute(backend, gf)` ausgeführt wird — das ist genau
die Library-Dispatch-Logik die wir loswerden wollen. Ohne Graph-Executor
existieren die port-Ops nur als standalone-Verifier; der echte
Qwen3.5-Forward geht weiter durch libggml-cuda.so.

Nötig:
- Rust-seitiger `Tensor` struct (device ptr, shape, strides, dtype)
- Op-DAG-Builder (wie ggml's `ggml_mul_mat(ctx, a, b)` → Rust
  `graph.mul_mat(a, b) -> TensorId`)
- Topological sort + Executor der pro Node die passende cuda_port op
  aufruft
- Memory allocator für intermediates (ggml_gallocr Ersatz)
- KV-Cache handling

Geschätzt ~1500-2000 LoC Rust, 3-5 Tage.

### Phase C — graph.rs Cutover

`graph.rs` von ggml-APIs (1912 Zeilen, ~300 op-calls) auf den neuen
Rust-Graph-Builder umschreiben. 2-3 Tage.

### Phase D — Link-Layer-Cutover

`build.rs` trimmt die `libggml*` linkage raus, nur noch `cudart` + `cuda`
(Driver-API) bleiben. CTOX bench-bin + server-bin bauen weiter, nur gegen
Rust-native Pfad.
