# Qwen3.5 DFlash Inference Port Status

This document records only locally inspected or measured status. It is not a
support matrix.

## Required engines

CTOX still requires four separate bare-metal engines:

| Model | CUDA | Metal |
| --- | --- | --- |
| Qwen3.5-27B + DFlash | required | required |
| Qwen3.5-35B-A3B + DFlash | required | required |

Each engine must keep its own Rust glue and vendored kernels. No model-server,
runtime tensor library, or linked MLX/llama.cpp/ggml runtime is allowed in the
final inference path.

## Port-candidate rule

A reference is a port candidate only when both are true:

1. It carries direct performant kernels for the relevant hot path.
2. Its host-side dispatcher, shape logic, cache logic, and verification logic
   can be ported into Rust with explicit ownership of the copied code.

Wrappers around a runtime are not enough. A candidate that does not run fast on
this Mac can still be useful as source material if it contains the kernels and
the dispatcher logic; it is not valid as a performance reference unless it runs
successfully with the target model and weights.

## Current candidate assessment

| Source | Candidate use | Current evidence |
| --- | --- | --- |
| `lucebox/dflash` CUDA | Qwen3.5-27B DFlash+DDTree graph, CUDA rollback, GGUF target path | Verified on the A6000 host with `scripts/bench_he.py --n-gen 256 --ddtree-budget 22`: 115.49 tok/s HumanEval-style mean, 8.26 average commit/step, 51.6% acceptance, 82.46-159.90 tok/s prompt range. This is the current CUDA 27B performance baseline. |
| `dflash-mlx` / `ddtree-mlx` | DFlash algorithm, tape/verify logic, MLX custom kernels | Local 27B Python reference is too slow to be the Metal performance target: about 4.0 tok/s baseline decode and about 0.03 tok/s DFlash verify on this Mac. Local 35B-A3B target-only MLX runs, but the DFlash reference is also not performant here: the 1-token DFlash run took 131.7 s wall time and rounded to 0.0 tok/s. |
| Ollama MLX | Qwen3.5 dense/MoE model logic, MLX quantized matmul dispatch, GatedDeltaNet kernels | Small Qwen3.5 MLX model runs after the local stream-fix build. `qwen3.5:27b-nvfp4` aborts with a Metal GPU timeout on this 32 GB Mac, so it is source material, not a validated local reference. |
| MLX kernel sources | Metal `fp_quantized`, `quantized`, `rms_norm`, `rope`, `softmax`, SDPA kernels | Vendored into both model crates under `vendor/mlx`; compile is now verified by `cargo check --bins` on Apple Silicon. |
| llama.cpp / GGML | Direct CUDA/Metal kernel corpus for quantized matmul, attention, top-K/MoE, GatedDeltaNet | Useful kernel/dispatcher source. It does not provide DFlash for these models by itself. |

## Verified external performance baselines

| Platform | Model/path | Command/source | Result |
| --- | --- | --- | --- |
| CUDA A6000 | Qwen3.5-27B Q4_K_M + DFlash + DDTree | `DFLASH_TARGET=models/Qwen3.5-27B-Q4_K_M.gguf DFLASH_DRAFT=models/draft/model.safetensors python3 scripts/bench_he.py --n-gen 256 --ddtree-budget 22` in `/home/metricspace/dflash-ref/dflash` | 115.49 tok/s mean, 8.26 AL, 51.6% acceptance; per-prompt range 82.46-159.90 tok/s. |
| CUDA A6000 | Qwen3.5-27B Q4_K_M autoregressive | `build/test_generate` on `/tmp/he_prompt_00.bin` through `/tmp/he_prompt_02.bin`, 256 generated tokens | 31.33, 31.26, 31.20 tok/s. |
| CUDA A6000 | CTOX Qwen3.5-27B compatibility bench via `GGML_LIB_DIR` + owned CUDA f32 argmax for DDTree verify | `tests/parity/run_cuda_reference_compare.sh` from `/tmp/ctox-qwen35-27b-dflash-check`, 10 prompts, 256 generated tokens each, `--fast-rollback --ddtree --ddtree-budget=22` | All 10 outputs byte-identical to C++ reference. Reference mean 113.75 tok/s, CTOX mean 116.87 tok/s, delta +2.74%. Per-prompt CTOX tok/s: 83.28, 129.64, 117.37, 100.67, 95.94, 104.33, 124.18, 116.91, 162.29, 134.04. |
| Metal, this Mac | Qwen3.5-27B 4-bit target-only | `mlx_lm.benchmark --prompt-tokens 128 --generation-tokens 16 --num-trials 2` | 67.239 prompt tok/s, 4.631 decode tok/s. |
| Metal, this Mac | Qwen3.5-35B-A3B 4-bit target-only | same MLX-LM benchmark shape | 106.756 prompt tok/s, 27.012 decode tok/s. |
| Metal, this Mac | Qwen3.5-35B-A3B 4-bit target-only, complete SD-card model store | `HF_HOME=/Volumes/Models/huggingface python -m mlx_lm.generate --model /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit --prompt hello --max-tokens 64 --temp 0` | Prompt 11 tokens at 0.798 tok/s, generation 64 tokens at 45.786 tok/s, peak memory 19.593 GB. |
| Metal, this Mac | Qwen3.5-35B-A3B + DFlash Python reference, complete SD-card model store | `HF_HOME=/Volumes/Models/huggingface DFLASH_VERIFY_LINEAR=0 dflash --model /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit --draft /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash --prompt hello --max-tokens 1 --no-chat-template` | Completed in 131.7 s wall time and printed `1 tokens | 0.0 tok/s | 0.0% acceptance`. 4-token runs with and without `DFLASH_VERIFY_QMM` were killed after 180 s with only one token emitted. |
| Metal, this Mac | CTOX Qwen3.5-35B-A3B target-only, complete SD-card model store | `target/release/qwen35-35b-a3b-dflash-bench-metal ... --target-only --dflash-max-ctx 2 --block-size 1 --profile --no-pipeline-warmup`, prompt `hello`, 16 generated tokens | Prompt 1 token, generated 16 tokens in 77.319 s; prefill 75.643 s = 0.01 tok/s; decode 9.55 tok/s. Per-token target commits were about 0.100-0.110 s each. |

The CUDA 27B port target is therefore above 100 tok/s for DFlash+DDTree steady
decode. The older 36 tok/s single run is not a valid DFlash+DDTree baseline.

## CTOX Metal changes verified in this checkout

- Both model crates vendor and compile MLX `fp_quantized.metal` and
  `fp_quantized_nax.metal`.
- Both Metal dispatchers now route single-token BF16 dense matmul through MLX
  `gemv_bfloat16...` kernels, using dispatcher logic ported from
  `mlx/backend/metal/matmul.cpp::gemv_axbpy`.
- Both Metal dispatchers now route small quantized matmul batches (`m <= 4`)
  through repeated MLX QMV instead of MLX QMM. This avoids the slow tiny-QMM
  path used by block-size-2 target verification.
- `cargo run --example smoke_metal` resolves the relevant GEMV pipelines:
  27B resolves 13/13 glue pipelines and 35B resolves 19/19 glue pipelines.
- A real-weight 27B Metal DFlash run with `block-size=2` reaches the draft path
  after the GEMV and small-QMV changes. Measured on this Mac:

```text
pipeline warmup: 46/46
cold first target forward: 48.458 s, excluded from timed run
prefill: 1 token in 0.244 s = 4.11 tok/s
cycle block_len=2:
  draft_s=0.583
  verify_s=0.370
  restore_s=0.002
  recommit_s=0.204
  accepted=0/1
total: 2 generated tokens in 1.628 s = 1.44 tok/s decode
```

This verifies execution of the current 27B Metal draft/verify path. It also
confirms the implementation is still far too slow for the target.
- The 35B-A3B model store was completed on the SD card at:

```text
/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit      19G
/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash   905M
```

- A real-weight CTOX 35B-A3B Metal target-only run from that SD-card store now
  completes when pipeline warmup is disabled. Diagnostic layer caps show the
  basis path runs (`CTOX_METAL_MAX_LAYERS=0`: 62.71 tok/s decode), 1 layer runs
  (54.76 tok/s), 4 layers run (34.20 tok/s), and 10 layers run (28.31 tok/s).
  The full 40-layer target-only run generated 16 tokens at 9.55 decode tok/s
  after a 75.643 s 1-token prefill. This is a performance blocker, not a
  correctness pass.

## Known CTOX blockers

- Cold first target execution on Metal is still unusably slow and must stay
  excluded from steady-state throughput. After explicit warmup, measured 27B
  one-token prefill is about 0.244 s on this Mac.
- Multi-token BF16 draft matmul and target prefill still need full Steel GEMM
  or equivalent direct-kernel dispatch; only single-token BF16 GEMV has been
  ported so far.
- Qwen3.5-35B-A3B CUDA still fails fast in `src/cuda/mod.rs`.
- `lucebox/dflash` is not a 35B-A3B CUDA reference: source inspection on the
  A6000 host shows project/header/loader constants hardcoded to `dflash27b`,
  Qwen3.5-27B target dimensions, and the 27B draft shape. A separate 35B MoE
  CUDA reference or a port from GGUF/llama.cpp MoE target logic plus DFlash
  draft logic is still required.
- Vision weight surfaces exist, but vision forward is not proven.
- No byte-for-byte parity harness has passed against the current source tree.
- CTOX root runtime does not yet call these per-model crates as the production
  local inference path.

## Verified build status

On Apple Silicon, both model crates currently pass:

```bash
cargo check --bins
```

That verifies Rust compile and Metal shader compilation, including the vendored
MLX FP quantized Metal sources. It does not verify inference correctness or
throughput.
