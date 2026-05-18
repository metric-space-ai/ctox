# Optimization Plan — qwen36_35b_a3b_q4km_metal

This is the canonical, evidence-backed plan for stage 3+ of the
local-llm-inference-optimization skill applied to Qwen3.6-35B-A3B
Q4_K_M on Apple Silicon M5. It anchors every kernel decision in
measured baselines + measured hardware ceilings.

## 1. Measured Inputs

### 1.1 Hardware ceilings (M5) — measured

From llama.cpp init log + `qwen36-35b-a3b-q4km-metal-roofline` run on
2026-05-08:

```text
chip                  Apple M5
gpu cores             10 (Apple10 family, Metal 4)
unified memory        32 GiB; recommended GPU working set 26.8 GiB
simdgroup reduction   true
simdgroup matrix      true   ← MetalPerformancePrimitives matrix API available
has tensor            true
has bfloat            true

sustained DRAM read   60.6 GB/s   (1 GiB shared blit)
sustained read+write  121.2 GB/s
peak at 256 MiB       51 GB/s read
fall-off at 4 GiB     50 GB/s read (vs 60 at 1 GiB; mild thermal/page TLB cost)
small-buffer limit    20 GB/s @ 16 MiB shared (latency-bound, not relevant)
```

Use **60.6 GB/s** as the read-bandwidth ceiling for any kernel that
streams ≥ 1 GiB per dispatch. The advertised "150 GB/s peak" is a
read+write traffic number; for a Q4_K_M matvec (read-dominated), the
right ceiling is the read-only one above.

Matrix/tensor-API throughput is measured implicitly through the
`mul_mv_q4_K_f32` bench in §3.1.

### 1.2 GGML default-config baseline (BLAS + Metal)

[docs/kernel-dev/baselines/2026-05-08T0725Z/llama_bench.json](baselines/2026-05-08T0725Z/llama_bench.json)
captured 2026-05-08 against
`bartowski/Qwen_Qwen3.6-35B-A3B-Q4_K_M.gguf`
(sha256 `6f5c72e2cde7fb0a1584cc009cdb4513f26733740369d3e2df0e7d7247112d05`):

```text
phase     n_prompt n_gen   t/s ± stddev
prefill        512       0  758.08 ± 19.71
prefill       4096       0  710.69 ± 16.62
prefill      16384       0  544.69 ± 17.60
decode           0     128   33.62 ± 0.94
decode           0     512   33.76 ± 0.71
```

This is the **default config** llama.cpp uses on Apple Silicon — BLAS
(via Accelerate/AMX/SME co-processor) handles dense prefill matmul,
Metal handles decode and everything else. It is the number a user gets
out of the box and the realistic comparison for "is the user better
off using the new engine".

### 1.3 GGML pure-Metal baseline (BLAS disabled)

In progress as of writing
([docs/kernel-dev/baselines/2026-05-08T0725Z-pure-metal/](baselines/2026-05-08T0725Z-pure-metal/)).

This is the **apples-to-apples** baseline against this crate. Our
crate is GPU-only (no AMX/SME path planned), so the right comparator
is llama.cpp running pure-Metal too. Results land here once the bench
finishes.

## 2. Bottleneck Math

### 2.1 Decode = bandwidth-bound

Per-token bytes streamed (Q4_K_M weights for the active 3B params):

```text
Q4_K_M block size           = 144 B per 256 weights
                            = 0.5625 B / weight
active params per token     ≈ 3.0 B  (35B total × 8/256 routed top-8 ≈ 1.1B
                                      + 256 experts × 1/256 always-shared ≈ 0.5B
                                      + non-MoE attn/embed ≈ 1.4B)
bytes per token             ≈ 3.0 × 0.5625 = 1.69 GB
observed decode             = 33.7 tok/s
effective bandwidth         = 1.69 × 33.7 = 57 GB/s
M5 advertised peak          ≈ 150 GB/s
utilization                 ≈ 38 %
theoretical headroom        ≈ 2.5 ×
```

If we hit 75 % of M5 unified bandwidth (a realistic target on
SIMD-group-matrix-mul + fused dequant), decode lands at
≈ 0.75 × 150 / 1.69 = **66 tok/s** — roughly 2× the BLAS+Metal default.

### 2.2 Prefill = matmul + attention quadratic

```text
n_prompt   t/s   normalized
   512    758   1.00
  4096    711   0.94   (-6%, cache pressure)
 16384    545   0.72   (-28%, attention quadratic dominates)
```

Up to ~4k the bottleneck is dense matmul throughput on the MoE
expert banks (each token visits 8 of 256 experts). At 16k the
softmax-attention `O(N²)` term kicks in.

For prefill (batched matmuls), the M5 SIMD-group-matrix-mul (= Apple's
on-chip tensor units) outperforms vanilla SIMD reduction by 4-8× for
f16 GEMM at typical tile sizes. Apple Accelerate's BLAS is currently
faster because its AMX/SME path peaks higher per cycle, but with a
custom tensor-API kernel that fuses Q4_K dequant **in-kernel** we
should be able to close the gap or beat it (BLAS has to consume f16
weights, paying dequant traffic that we skip).

## 3. Kernel Priority Order

Numbered in expected-impact order. Each gets the full
[EXPERIMENT_TEMPLATE](EXPERIMENT_TEMPLATE.md) →
[DECISION_RECORD_TEMPLATE](DECISION_RECORD_TEMPLATE.md) treatment.

### 3.1 `mul_mat_q4_k_m` (the dominant win)

- Touches: every layer's attn QKV proj, attn O proj, FFN gate/up/down
  per expert, shared-expert FFN, LM head
- Dependence: blocks decode + prefill simultaneously
- Phased approach (CLAUDE.md rule 4 + 5):
  1. **Baseline drop-in** — call vendored `kernel_mul_mv_q4_K_f32` and
     `kernel_mul_mm_q4_K_f32` from a Rust dispatcher with byte-exact
     verifier against an in-crate Q4_K dequant + naive matmul CPU
     reference. (Done — see decision record
     [decisions/2026-05-08-mul_mv_q4_k_f32.md](decisions/2026-05-08-mul_mv_q4_k_f32.md).)
  2. **Custom kernel candidates** — author new MSL kernels in
     `src/metal_port/kernels/mul_mat_q4_k_m/` that we expect to beat
     the vendored kernel on this M5. Examples worth trying:
       - `mul_mv_q4_K_f32_simdmatmul` — uses the
         `simdgroup_matrix_mul` apple10 path instead of upstream's
         scalar-loop simd-reduction;
       - `mul_mv_q4_K_f32_tensor4` — uses the Metal 4
         `MetalPerformancePrimitives` matrix tensor API
         (`has_tensor=true` in device init);
       - `mul_mv_q4_K_f32_dequant_fused_rms_norm` — fuses with the
         pre-norm scale that follows in the residual block, removing
         one threadgroup roundtrip per layer per token;
       - `mul_mm_q4_K_f32_split_K` — splits K across multiple
         threadgroups for prefill batches where the standard tile
         under-uses the M5's 10 GPU cores.
     Each candidate is gated behind a flag, ships with a verifier
     against the vendored kernel (within 4 ULP for f32 / 8 ULP for f16
     intermediates), and an isolated bench against the same vendored
     kernel on this M5.
- Promotion: the candidate replaces the vendored default in
  `accepted_profile.env` only when isolated bench beats vendored ≥ 5 %
  median + p95 stable, integrated forward path beats by ≥ 3 %,
  correctness gate green at the integrated layer-block level.
- Correctness gate: byte-compare against the vendored kernel for
  exactly the shapes the integrated path uses (Q-proj 4096×2048,
  KV-proj 512×2048, O-proj 2048×4096, FFN gate/up 512×2048, FFN
  down 2048×512). Tolerance starts strict (≤ 4 ULP f32) and only
  loosens with documented justification.

### 3.2 `moe_router` + top-8 expert dispatch fusion

- 256 experts, top-8 per token. Naive dispatch = 8 small matmuls per
  token; even worse, the router itself touches all 256 expert weights
  for the routing scores
- Approach: fuse router-softmax + top-8 selection + per-expert
  scatter into a single Rust dispatcher that issues exactly **8**
  expert matmuls (not 256) and collects results back via a single
  scatter-add into the residual
- For prefill: bin tokens by selected expert, then issue one
  `mul_mat_q4_k_m` per *expert* over its assigned token bin (instead
  of one per token-expert pair) — this gets back to apple10's
  preferred tile sizes
- Correctness: each token's residual delta must be exactly equal to
  the sum of its 8 routed expert contributions weighted by softmax
  scores

### 3.3 `gqa_softmax_attn` with online softmax + `attn_output_gate` fused

- The 28 % drop pp512 → pp16384 is the attention-quadratic regime
- Approach: tile-streaming SDPA with online softmax (in-tile running
  max + running denominator). The `attn_output_gate` (sigmoid * O
  proj input) is folded into the epilogue so we don't pay an extra
  dispatch for it
- M5 advantage: SIMD-group matrix mul lets one threadgroup compute a
  block of QKᵀ via the matrix unit, then keep the block in
  threadgroup memory for the V multiplication — exactly the access
  pattern flash-attention 2 needs
- Correctness: byte-compare logits against a CPU reference for
  ctx ∈ {1024, 4096, 16384}; max abs ≤ 1e-3 on the top-50 logits

### 3.4 `rope_partial_mrope` + `rms_norm_mul_f32` fusion

- M-RoPE on 64 of 256 head-dim lanes (`partial_rotary_factor=0.25`,
  `mrope_section=[11,11,10]`). Currently a separate dispatch in
  llama.cpp; fusing it onto the QKV projection epilogue saves one
  threadgroup roundtrip per layer per token
- The `kernel_rms_norm_mul_f32` (F=2) variant already exists in our
  vendored MSL; expose it as a second `RmsNormF32Kernel` flavour and
  route the per-layer attn-norm + ffn-norm through it (pre-multiplies
  by the learned weight gain in one dispatch)

### 3.5 `embed_get_rows` + `lm_head` + on-GPU sampling

- Currently negligible vs decode bandwidth, but every microsecond
  CPU↔GPU sync at the end of each token costs at the t/s = 33.7
  rate. Putting argmax / top-k sampling on-GPU avoids the readback
  of the full 248 320-element logit row

## 4. Promotion Gates

Per the skill's [measurement-gates.md](../../../../skills/system/model_optimization/local-llm-inference-optimization/references/measurement-gates.md):

A candidate kernel is **accepted** only when **all** of:

1. its per-op verifier passes within 1 ULP for f32 / 2 ULP for f16
2. its **isolated** microbench beats the equivalent op in the pure-Metal
   baseline by ≥ 5 % median, ≥ 0 % p95
3. swapping it into the integrated forward path (once stage 4 lands)
   advances the corresponding cell of the **acceptance pack** by ≥ 3 %
   median with no p95 regression
4. correctness on the integrated path: exact greedy-token parity with
   pure-Metal llama.cpp at temperature=0 over a 32-token prompt
5. a [DECISION_RECORD](DECISION_RECORD_TEMPLATE.md) entry exists with
   evidence + rollback condition

Numbers below 5 % isolated win get tagged `keep opt-in` not `accept`,
per skill rule.

## 5. Anti-targets

Per the skill's [qwen35-lessons.md](../../../../skills/system/model_optimization/local-llm-inference-optimization/references/qwen35-lessons.md), do not pursue:

- 4-token decode microbenchmarks (mislead promotion)
- "no cache misses" claims without hardware counters
- block-size knob sweeps that sit within ±2 % of each other (look
  for structural change instead)
- hand-rolled SIMD32 reductions outside the matrix unit (lost on
  Qwen3.5)
- scalar Q4_K dequant outside the matmul (unpack overhead loses)
- per-layer ANE/Core ML ping-pong (coarse graph track, not a kernel
  partner)

## 5b. Reporting discipline (per Skill `measurement-gates.md`)

Every step end MUST close with a **Standing Status Card** that
states the current Rust-native engine pp/tg tok/s vs the shim
baseline. This is non-negotiable per the skill's measurement-gates:

> Record: command, env flags, git state, model artifact hash,
> hardware/OS/runtime version, warmup/iterations/round order,
> median_s and p95_s, **tok/s**, correctness metrics

The card has three rows:

```text
Baseline  (qwen36_35b_a3b_ggml shim, -ub 2048 -fa auto, this M5):
  pp4096   = 672 ± 7   tok/s
  pp16384  = 558 ± 36  tok/s
  tg128    = 36.1 ± 0.2 tok/s

Current rust-native engine (this commit):
  pp4096   = <measured> | N/A — integration not yet wired
  pp16384  = <measured> | N/A
  tg128    = <measured> | N/A

Gap to stretch target:
  pp4096   = <delta> %     (target  ~770 t/s, +15 %)
  pp16384  = <delta> %     (target  ~670 t/s, +20 %)
  tg128    = <delta> %     (target  ~46  t/s, +27 %)
```

Per-kernel wins (TFLOPS, GB/s, max_abs) are evidence that supports
the projection on integrated tok/s — they are not a substitute for
measuring it. When integration isn't running yet, write `N/A — reason`
in the integrated rows but always re-state the baseline target.

The wakeup loop exits ONLY when the integrated card shows
≥ +5 % per phase over baseline OR when truly blocked by something
that needs user input.

## 6. Order of operations for stage 3

1. Run `qwen36-35b-a3b-q4km-metal-roofline` (wired in stage 2, not
   yet executed — needs the bench-bench-bench thermal isolation rule).
   This sets the **measured** stream BW and matrix throughput numbers
   that turn the speculative percentages above into real targets.
2. Refine §2 of this document with the measured numbers.
3. Open the §3.1 experiment record. Implement `mul_mat_q4_k_m`
   behind a flag, run isolated bench, run correctness gate.
4. Wait for pure-Metal baseline to land in §1.3 (currently in
   progress) — that's the comparator §3.1 needs.
5. Promote or reject §3.1; move to §3.2.
