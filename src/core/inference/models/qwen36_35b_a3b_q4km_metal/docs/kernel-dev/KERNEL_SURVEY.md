# Online Kernel Survey — Q4_K_M Apple Silicon

Mandate from the skill ("Choose Candidate Families", method-playbook §6.4):
*"Use platform matrix/tensor APIs for dense GEMM-like phases."*
And explicitly: hand-author kernels for "recurrence, reductions, online softmax,
sampling, and layout-specific glue" — **not** for dense GEMM where mature
implementations already exist. This document surveys what already exists
before any new kernel work begins.

## 1. Already vendored, never measured

These ship in our `vendor/ggml-metal/` (llama.cpp commit `3e941b81`)
and we already compile them into the metallib via `build.rs`. We have
not benchmarked them yet — these are **free wins** if any beats
`kernel_mul_mv_q4_K_f32` on our shapes.

| Kernel host_name | Layout | Use case | Status |
|---|---|---|---|
| `kernel_mul_mv_q4_K_f32` | scalar simd reduce | matvec, decode (N=1) | dispatcher in `metal_port::ops::mul_mv_q4_k`; correctness ✓; min latency 67–80 % roofline |
| `kernel_mul_mv_ext_q4_K_f32_r1_2` | row-batched matvec, 2 src1 rows / call | prefill micro-batch | not yet wired |
| `kernel_mul_mv_ext_q4_K_f32_r1_3` | 3 src1 rows / call | prefill micro-batch | not yet wired |
| `kernel_mul_mv_ext_q4_K_f32_r1_4` | 4 src1 rows / call | prefill micro-batch | not yet wired |
| `kernel_mul_mv_ext_q4_K_f32_r1_5` | 5 src1 rows / call | prefill micro-batch | not yet wired |
| `kernel_mul_mm_q4_K_f32` | **`simdgroup_half8x8` matmat** | prefill (N≫1), large attn shapes | not yet wired — **highest expected win** |
| `kernel_mul_mm_q4_K_f16` | matmat with f16 dst | prefill into f16 KV | not yet wired |
| `kernel_mul_mm_id_q4_K_f32` | indexed matmat | MoE expert dispatch | not yet wired — **MoE win path** |
| `kernel_mul_mv_id_q4_K_f32` | indexed matvec | MoE decode | not yet wired |
| `kernel_get_rows_q4_K` | gather | embedding lookup | not yet wired |
| `kernel_flash_attn_ext_f16_dk256_dv256` | **head_dim=256 SDPA** for Qwen3.6 | prefill attention | not yet wired |
| `kernel_flash_attn_ext_vec_f16_dk256_dv256` | head_dim=256 decode | decode attention | not yet wired |
| `kernel_flash_attn_ext_q8_0_dk256_dv256` | Q8_0-quantized K/V | KV-cache compression | not yet wired |
| `kernel_flash_attn_ext_q4_0_dk256_dv256` | Q4_0-quantized K/V | aggressive KV compression | not yet wired |

So **of the ~12 kernels we'd need for the full Qwen3.6 forward path,
we already have all of them vendored**. Stage 3 is mostly a wiring +
benchmarking exercise, not a kernel-writing exercise.

The only ones we likely need to author:

- a Rust-side **MoE top-8 router** (selects which experts run; this is
  Rust dispatch logic, not an MSL kernel — `mul_mv_id_q4_K_f32`
  already does the indexed compute)
- the **linear-attention "dflash" block** (Qwen3.5/6's GatedDeltaNet
  variant) — explicitly out of scope for the "ohne dflash" stage 1-3
  cut

## 2. Available alternative sources (need vendoring)

### 2.1 ik_llama.cpp (active fork by Iwan Kawrakow, Q4_K's original author)

- Latest commit `9a26522af234`, dated 2026-05-07: literally
  *"qwen35moe : support MTP tail layer"* — actively maintaining for our
  model family.
- Has the entire `iq4_*` quant family as additional kernels:
  - `iq4_k`, `iq4_kt`, `iq4_ks`, `iq4_kss` (Iwan's improved 4-bit quants)
  - `iq4_nl`, `iq4_xs` (cross-fork standards)
- Also has flash-attention with explicit head_dim variants
  (`kernel_flash_attn_ext_f16_h{64,80,96,112,128,256}` — same head_dim
  enumeration as upstream).
- **ABI difference**: ik passes mul_mv kargs as 19 separate `constant`
  arguments instead of a struct. Vendoring requires translation in our
  Rust dispatcher (or a thin MSL adapter wrapping ik's kernel in the
  upstream-style struct ABI).

### 2.2 MLX (Apple's official ML framework)

- `quantized.metal` (9 KB) + `quantized.h` (80 KB) — group-wise 4-bit
  matmul, heavily Apple-Silicon-tuned.
- `quantized_nax.metal` + `quantized_nax.h` (NAX = Apple Neural Accelerator)
  — uses M-series tensor units explicitly (the `has_tensor=true`
  feature flag we observed on M5).
- **Not drop-in**: MLX uses a different quant block layout than
  Q4_K_M. Adopting MLX's matmul requires re-quantizing the model from
  Q4_K_M to MLX's group-wise format, which changes the artifact.
  Useful as a *technique source* for tensor-API integration, not as a
  direct vendor.

### 2.3 IQ4-format public GGUFs for Qwen3.6-35B-A3B (HF survey)

All public on Hugging Face as of 2026-05-08:

```text
RDson/Qwen3.6-35B-A3B-IQ4_KS-GGUF
abovespec/Qwen3.6-35B-A3B-IQ4_K_R4-GGUF        ← _R4 = row-interleaved
Thireus/Qwen3.6-35B-A3B-THIREUS-IQ4_K-SPECIAL_SPLIT
Thireus/Qwen3.6-35B-A3B-THIREUS-IQ4_KS-SPECIAL_SPLIT
Thireus/Qwen3.6-35B-A3B-THIREUS-IQ4_KSS-SPECIAL_SPLIT
Thireus/Qwen3.6-35B-A3B-THIREUS-IQ4_KT-SPECIAL_SPLIT
Thireus/Qwen3.6-35B-A3B-THIREUS-IQ4_K_R4-SPECIAL_SPLIT
Krasnopjorovs/Qwen3.6-35B-A3B-IQ4_XS-Imatrix
localweights/Qwen3.6-35B-A3B-MTP-IQ4_XS-GGUF
```

Adopting any of these requires (a) re-downloading the corresponding
GGUF (~20 GiB), (b) vendoring ik_llama's metal sources, (c) running
ik's quality benches at imatrix to confirm parity with Q4_K_M.

## 3. Honest priority order

Cost on the *implementation* axis (lower = cheaper to do this turn);
expected benefit on the *speedup* axis (higher = bigger Δ tok/s).

| Stage | Action | Implementation cost | Expected benefit | Reason |
|---|---|---|---|---|
| 3.2 | Wire `kernel_mul_mm_q4_K_f32` for prefill | LOW (already vendored, just need a dispatcher) | HIGH on prefill | uses `simdgroup_half8x8` matrix path — exactly the apple10 tensor unit |
| 3.3 | Wire `_ext_q4_K_f32_r1_{2..5}` row-batched matvec | LOW (vendored) | MEDIUM on decode-burst | autotune the row-batch size for Qwen3.6 shapes |
| 3.4 | Wire `kernel_flash_attn_ext_*_dk256_dv256` for SDPA | LOW (vendored) | HIGH on long prefill | breaks the pp16384 N² regime |
| 3.5 | Wire `kernel_mul_mv_id_q4_K_f32` + Rust router | MEDIUM (router is new logic) | HIGH on MoE decode | 8 expert dispatches instead of 256 |
| 3.6 | Wire `kernel_get_rows_q4_K` for embedding | LOW (vendored) | LOW (one dispatch per token) | finishes the forward path |
| 3.7 | Vendor ik_llama.cpp + try IQ4_KS / IQ4_K_R4 | HIGH (ABI translate, ~20 GiB GGUF) | MEDIUM-HIGH (Iwan's quants often beat Q4_K_M) | second source — only after stage 3.2-3.6 done |
| 3.8 | Hand-author `dflash` linear-attention block | HIGH (no vendored equivalent for Qwen3.6's GatedDeltaNet) | NEEDED for end-to-end | required to close stage 4 |

## 4. What this means for the work order

**Was wrong**: writing custom MSL kernels for Q4_K matmul from scratch.
Upstream and ik_llama already ship multiple variants we have not
benchmarked. The skill's order — vendored → bench → adopt the winner →
custom only for what's missing — was being skipped.

**Right**: every stage in §3 above either calls an already-vendored
kernel (rows 3.2-3.6), pulls a second source (3.7), or writes a custom
kernel only where no public one exists (3.8 — the linear-attention
block, which is genuinely Qwen-specific layout).

The optimization plan in [OPTIMIZATION_PLAN.md](OPTIMIZATION_PLAN.md)
is updated to reflect this order; this document is the source-of-truth
for what's available.
