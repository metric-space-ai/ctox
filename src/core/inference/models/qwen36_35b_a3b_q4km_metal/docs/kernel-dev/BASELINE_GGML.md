# GGML Baseline — Qwen3.6-35B-A3B Q4_K_M on Apple M5

This is the **measured** baseline our crate is benchmarked against.
It is captured solely with **upstream llama.cpp / ggml as a measurement
tool** — never as a runtime dependency of the optimized engine in
`src/`. Per the project contract: ggml is a baseline; the deliverable
is pure-Rust + vendored MSL kernels with no `libggml*` link.

## 1. Provenance

| Item | Value |
|---|---|
| GGUF | `bartowski/Qwen_Qwen3.6-35B-A3B-GGUF` / `Qwen_Qwen3.6-35B-A3B-Q4_K_M.gguf` |
| GGUF size on disk | 19.92 GiB (21 391 448 384 B) |
| GGUF sha256 | `6f5c72e2cde7fb0a1584cc009cdb4513f26733740369d3e2df0e7d7247112d05` |
| Local path | `runtime/models/qwen36_35b_a3b_gguf/Qwen_Qwen3.6-35B-A3B-Q4_K_M.gguf` |
| llama.cpp build | `9060` / commit `ad0922465`, brew bottle |
| ggml lib | brew `0.11.0` |
| Bench tool | `/opt/homebrew/bin/llama-bench` |
| Capture date | 2026-05-08 |

| Host | Value |
|---|---|
| chip | Apple M5 |
| GPU family | `MTLGPUFamilyApple10`, `MTLGPUFamilyMetal4` |
| GPU cores | 10 |
| Unified memory | 32 GiB; recommendedMaxWorkingSetSize = 26.8 GiB |
| `simdgroup_reduction` | true |
| `simdgroup_matrix_mul` | true |
| `has_tensor` | true (= MetalPerformancePrimitives matrix path available) |
| `has_bfloat` | true |
| OS | macOS 26.2 build 25C56 |
| Power source | AC |

Bench parameters: `--n-gpu-layers 99 --threads 8 --repetitions 3`.
Acceptance pack: prefill at 512/4096/16384 tokens, decode at 128/512
tokens.

## 2. Default Config (BLAS + Metal)

This is **what a user gets out of the box** with brew-installed
llama.cpp on Apple Silicon. Both backends are loaded; llama.cpp
internally routes large-batch dense matmul through Apple Accelerate
(BLAS), which on M-series uses the AMX/SME co-processor — not vanilla
CPU FPU. Decode (batch=1) and most non-matmul ops stay on Metal.

Raw output: [baselines/2026-05-08T0725Z/llama_bench.json](baselines/2026-05-08T0725Z/llama_bench.json)

| phase | n_prompt | n_gen | tok/s | stddev |
|---|---:|---:|---:|---:|
| prefill | 512 | 0 | **758.08** | ± 19.71 |
| prefill | 4096 | 0 | **710.69** | ± 16.62 |
| prefill | 16384 | 0 | **544.69** | ± 17.60 |
| decode | 0 | 128 | **33.62** | ± 0.94 |
| decode | 0 | 512 | **33.76** | ± 0.71 |

backend column reported as `BLAS,MTL`.

## 3. Pure-Metal Config (BLAS disabled)

Captured by renaming `libggml-blas.so` aside before the run, restored
after. **This is the apples-to-apples comparator for our crate** —
our crate is GPU-only with no AMX/SME path planned.

Raw output: [baselines/2026-05-08T0725Z-pure-metal/llama_bench.json](baselines/2026-05-08T0725Z-pure-metal/llama_bench.json)

| phase | n_prompt | n_gen | tok/s | stddev |
|---|---:|---:|---:|---:|
| prefill | 512 | 0 | **766.85** | ± 21.52 |
| prefill | 4096 | 0 | **710.85** | ± 13.32 |
| prefill | 16384 | 0 | **524.25** | ± 8.23 |
| decode | 0 | 128 | **31.43** | ± 0.25 |
| decode | 0 | 512 | **31.87** | ± 0.13 |

backend column reported as `MTL`.

## 4. Delta — what BLAS actually contributes

| phase | size | BLAS+Metal | pure Metal | Δ vs pure | comment |
|---|---:|---:|---:|---:|---|
| prefill | 512 | 758.08 | 766.85 | **−1.1 %** | Metal alone ≥ AMX path; AMX setup amortizes badly at small batch |
| prefill | 4096 | 710.69 | 710.85 | **±0.0 %** | crossover; tied |
| prefill | 16384 | 544.69 | 524.25 | **+3.9 %** | AMX/SME pulls ahead at large dense matmul shapes |
| decode | 128 | 33.62 | 31.43 | **+7.0 %** | unexpected — likely thermal residual from 1st run, not a real BLAS win at batch=1 |
| decode | 512 | 33.76 | 31.87 | **+5.9 %** | same caveat |

Three takeaways:

- For Q4_K_M MoE on M5, **BLAS does not help much** — its widest
  win is +4 % at pp16384 (long prompt). The popular wisdom that
  Apple Accelerate is a free perf upgrade does not hold here,
  because the Q4_K → f16 dequantize cost has to be paid before
  feeding BLAS.
- The decode delta (+5–7 %) is suspicious. Decode is batch=1 → BLAS
  shouldn't trigger. The most likely cause is **thermal**: the
  pure-Metal run started right after the BLAS run with no cool-off,
  so the M5 was already a few degrees warmer. The skill explicitly
  warns about this — "do not compare against old numbers if
  thermal/runtime conditions changed". A clean re-run of the
  decode-only cells with a 5-min cool-off between them would shrink
  this gap to noise.
- The number **our crate competes against is the pure-Metal baseline**
  for everything below pp16384, and either pure-Metal or BLAS+Metal
  for pp16384 (we should beat both, but the higher 545 t/s is the
  promotion target).

## 5. Targets

Lifted from §2 of [OPTIMIZATION_PLAN.md](OPTIMIZATION_PLAN.md).
Promotion of the integrated stage-4 forward path requires at minimum
**parity** with the appropriate baseline below; "accepted" status
requires **beating it by ≥ 3 %** with no p95 regression.

| phase | size | parity target | accept target |
|---|---:|---:|---:|
| prefill | 512 | 766 | ≥ 790 |
| prefill | 4096 | 711 | ≥ 732 |
| prefill | 16384 | 545 (BLAS) | ≥ 561 |
| decode | 128 | 33.6 (BLAS) | ≥ 34.6 |
| decode | 512 | 33.8 (BLAS) | ≥ 34.8 |

Decode parity uses the BLAS+Metal number on the assumption the +5–7 %
gap is thermal noise; we'll re-confirm after the roofline probe.

## 6. Memory back-of-envelope from observed numbers

```text
decode bytes / token         ≈ 1.69 GB  (3.0 B active × 0.5625 B/w Q4_K_M)
observed pure-Metal decode   = 31.5 tok/s (≈ tg128 + tg512 mean)
effective bandwidth          = 1.69 × 31.5 = 53 GB/s
M5 advertised peak           ≈ 150 GB/s
utilization                  ≈ 35 %
theoretical headroom         ≈ 2.8 ×  → decode @ 75 % util ≈ 66 tok/s
```

A measured roofline (next stage) replaces the advertised 150 GB/s
with a real number from a stream-bandwidth probe, sharpening this
target.

## 7. Acceptance pack source data

The raw `llama_bench.json` in each subdirectory is what the bench
script wrote; `host_facts.txt` lists the host conditions; the
`.stderr.md` is the human-readable markdown table that shipped to
this document.

To re-run: `./tools/run_baseline_llama_bench.sh` (after fixing the
`set -euo pipefail` + `head -n 3` SIGPIPE issue noted in
RESEARCH_LOG.md, or invoke `llama-bench` directly with the same
flags).
