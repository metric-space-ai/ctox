# ctox-qwen35-35b-a3b-dflash

Rust inference target crate for **Qwen3.5-35B-A3B (MoE)** paired with
the **z-lab DFlash** block-diffusion speculative-decoding draft. The target
contract is one curated model with two bare-metal backend implementations:
CUDA and Metal.

| Host                         | Path         | Target weights                       | Draft weights                                  | Reference port          |
|------------------------------|--------------|--------------------------------------|------------------------------------------------|-------------------------|
| macOS + Apple Silicon        | `src/metal/` | `mlx-community/Qwen3.5-35B-A3B-4bit` | `z-lab/Qwen3.5-35B-A3B-DFlash` (bf16)          | [bstnxbt/dflash-mlx]    |
| Linux + CUDA                 | `src/cuda/`  | `Qwen3.5-35B-A3B` target weights     | `z-lab/Qwen3.5-35B-A3B-DFlash` (bf16)          | owned glue kernels only |

Same platform-exclusivity rule as the sibling 27B crate: `cfg(target_os =
"macos")` selects Metal and `cfg(target_os = "linux")` selects CUDA. The
Linux/CUDA path is a required target, but it is currently an owned-kernel glue
surface plus an explicit fail-fast inference stub, not a full inference
implementation.

## Target vs current status

The target contract for this crate is bare-metal Rust plus owned CUDA or Metal
kernels, with no external inference runtime, tensor library, or model server.
CUDA and Metal are allowed only as platform APIs.

Current status is transitional and must not be read as production support:

* Metal builds on Apple Silicon with the DFlash custom pipelines, 35B-specific
  MoE glue kernels, and the vendored MLX Metal quantized kernels under
  `vendor/mlx`. The MLX `fp_quantized*.metal` sources now compile without a
  runtime MLX dependency, and the shared Metal dispatcher now uses MLX
  `gemv_bfloat16...` for single-token BF16 draft matmul plus repeated MLX QMV
  for tiny quantized matmul batches. This establishes a credible port source
  for quantized and dense matmul dispatch, but the Metal path is not yet a
  validated high-performance inference engine.
* Metal text target-only and draft-loaded minimal runs recorded below are
  historical bring-up runs. They do not prove useful warm throughput,
  long-context parity, high-performance grouped MoE scheduling, GPU rollback,
  or vision forward dispatch. Re-run them after every dispatcher/kernel change
  before updating support claims.
* CUDA is not implemented as an inference backend. `src/cuda/mod.rs` still
  fails explicitly if routed at runtime, but `vendor/cuda/` is physically
  present and the first owned BF16 RMSNorm, dense-matmul, MoE-router,
  elementwise, argmax, hidden-slot, positions, and mask glue kernels compile
  into a CUDA archive without linking ggml or another inference runtime.
* No local 35B-A3B high-performance reference execution has been validated in
  this checkout. The `dflash-mlx`, Ollama/MLX, and llama.cpp/GGML trees remain
  port candidates only where they provide direct kernels plus dispatcher logic
  that can be ported into this crate.
* CTOX root runtime integration is not proven by this crate existing. The root
  CTOX binary must call this crate directly before the model can be documented
  as supported by CTOX local inference.

[bstnxbt/dflash-mlx]: https://github.com/bstnxbt/dflash-mlx

## What's different from `qwen35_27b_dflash`

This crate is a **separate curated model**, not a variant — no code,
no kernels, no vendored trees are shared. The differences that matter:

* **MoE MLP**. Every text layer swaps the dense SwiGLU MLP for a top-K
  router over 256 experts. `DFLASH35B_NUM_EXPERTS = 256`,
  `EXPERTS_PER_TOK = 8`, `MOE_INTERMEDIATE = 512`. The router and
  expert blocks live in [`metal::moe`](src/metal/moe.rs).
* **Hybrid text mixer**. The real config has 40 text layers with
  `linear_attention` except every fourth `full_attention` layer.
  It is not the old 64-layer all-attention assumption.
* **Attention shape**: 16 Q heads, 2 KV heads, head_dim 256. Full attention
  also packs an output gate in `q_proj`, so the Metal path splits `[q, gate]`
  and applies `attn * sigmoid(gate)` before `o_proj`.
* **Vision weights present**. The target bundle includes `vision_tower.*`
  tensors. Text loading is corrected for the nested `language_model.*`
  namespace; full vision forward is still pending.
* **DFlash draft shape**. The 35B-A3B draft is an 8-layer BF16 draft
  transformer with hidden size 2048. It consumes five captured target
  layer states (`[1, 10, 19, 28, 37]`) through an `fc` projection of
  shape `[2048, 10240]`.
* **No shared shader files**. Every `.metal` file under
  `vendor/metal/shaders/` is a physically separate copy from the
  27B crate, per the root development-guide rule. Textually identical today; may
  diverge if either crate tunes a shader for its own shape.

## Status

| Surface                                     | Status                       |
|---------------------------------------------|------------------------------|
| Crate layout + cfg gating                   | done                         |
| Metal build (metallib via `xcrun metal`)    | done                         |
| FFI, kernels, base qwen modules (RMSNorm, RoPE, Attention) | partial; compiles |
| MLX-4bit loader (`language_model.*`, hybrid layers, MoE tensors) | done |
| `MoeBlock::forward` + router top-K kernels  | routed through vendored MLX gather-QMV for selected experts |
| Runtime prefill + AR `one_cycle`            | real-weight target-only runs pass; DFlash path still below target |
| BF16 DFlash draft forward + target verify   | minimal verify/restore/commit pass with real weights |
| Vision tower forward                        | pending                      |
| Draft safetensors loader (bf16 path)        | done; validates 8 layers + `[2048, 10240]` fc |
| Draft load-only smoke (`cargo run --example load_draft_metal`) | **PASS** |
| Metal glue smoke (`cargo run --example smoke_metal`) | **PASS**; 41/41 35B glue/MLX pipelines resolve |
| CUDA Rust launch wrappers                   | done for current owned BF16/Q4_K glue kernels |
| CUDA glue smoke (`cargo run --example smoke_cuda`, remote) | **PASS** through Rust launch wrappers |
| Byte-for-byte parity vs `dflash-mlx` Python | pending |

## Verified Runs In This Checkout

These are **cold bring-up correctness runs**, not throughput benchmarks. The
first Metal forward includes lazy compute-pipeline construction, and these
numbers predate the current pipeline-warmup + Metal-blit rollback changes. The
current speculative path still uses naive BF16 draft matmul. Do not use these
numbers as the expected Metal performance envelope.

```text
macOS Metal target-only:
  cargo run --release --bin qwen35-35b-a3b-dflash-bench-metal -- ... \
    --target-only --dflash-max-ctx 2 --block-size 1
  prompt 1 token, generated 1 token in 68.345 s
  prefill 68.084 s = 0.01 tok/s, decode 3.83 tok/s

macOS Metal with BF16 DFlash draft loaded:
  cargo run --release --bin qwen35-35b-a3b-dflash-bench-metal -- ... \
    --dflash-max-ctx 2 --block-size 1
  prompt 1 token, generated 1 token in 82.450 s
  prefill 82.355 s = 0.01 tok/s, decode 10.50 tok/s

macOS Metal with BF16 DFlash draft verify/restore/commit:
  cargo run --release --bin qwen35-35b-a3b-dflash-bench-metal -- ... \
    --dflash-max-ctx 4 --block-size 2
  prompt 1 token, generated 2 tokens in 96.279 s
  prefill 81.525 s = 0.01 tok/s, decode 0.14 tok/s
  accepted=0/1 for this prompt

Linux CUDA owned-kernel smoke on A6000:
  cargo run --release --example smoke_cuda
  PASS: BF16 RMSNorm, dense matmul, add, mul, SiLU, argmax,
        hidden-slot copy, hidden repeat, positions4 fill, causal f16 mask,
        BF16 KV-cache store, BF16 SDPA decode, GGML Q4_K -> BF16 dequant,
        naive GGML Q4_K x BF16 matvec. The smoke calls the Rust
        launch wrappers, not raw FFI symbols.

macOS Metal 35B-DFlash draft load-only smoke:
  cargo run --release --example load_draft_metal -- \
    /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash/model.safetensors
  draft loaded: layers=8 fc=[2048, 10240]

macOS Metal glue pipeline smoke:
  cargo run --release --example smoke_metal
  ggml pre-instantiated: 5/5 resolved
  dflash pipelines (fc-specialized): 4/4 resolved
  glue pipelines: 41/41 resolved
```

These historical runs prove only that the real text weights loaded and a tiny
text path executed in the checkout where they were captured. They do not prove
useful warm steady-state throughput, full DFlash acceleration, vision forward,
high-performance GPU rollback, or byte-for-byte parity.

April 25, 2026 re-check on this Mac:

```text
Model store:
  /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit      19G
  /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash   905M

Python MLX target-only reference:
  HF_HOME=/Volumes/Models/huggingface python -m mlx_lm.generate \
    --model /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit \
    --prompt hello --max-tokens 64 --temp 0
  prompt 11 tokens at 0.798 tok/s
  generation 64 tokens at 45.786 tok/s
  peak memory 19.593 GB

Python dflash-mlx reference:
  HF_HOME=/Volumes/Models/huggingface DFLASH_VERIFY_LINEAR=0 dflash \
    --model /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit \
    --draft /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash \
    --prompt hello --max-tokens 1 --no-chat-template
  completed in 131.7 s wall time
  printed: 1 tokens | 0.0 tok/s | 0.0% acceptance

Rust CTOX target-only:
  cargo run --release --bin qwen35-35b-a3b-dflash-bench-metal -- ... \
    --target-only --dflash-max-ctx 2 --block-size 1 --profile --no-pipeline-warmup
  before MoE/Argmax/SDPA fixes:
    prompt 1 token, generated 16 tokens in 77.319 s
    prefill 75.643 s = 0.01 tok/s
    decode 9.55 tok/s
    per-token target commits around 0.100-0.110 s
  after MoE gather-QMV + parallel argmax + vectorized decode-SDPA:
    prompt 1 token, generated 64 tokens in 69.646 s
    prefill 67.685 s = 0.01 tok/s
    decode 32.64 tok/s

Rust CTOX target-only layer-cap diagnostics:
  before fixes:
    CTOX_METAL_MAX_LAYERS=0  decode 62.71 tok/s
    CTOX_METAL_MAX_LAYERS=1  decode 54.76 tok/s
    CTOX_METAL_MAX_LAYERS=4  decode 34.20 tok/s
    CTOX_METAL_MAX_LAYERS=10 decode 28.31 tok/s
  after fixes:
    CTOX_METAL_MAX_LAYERS=0  decode 244.54 tok/s
    CTOX_METAL_MAX_LAYERS=1  decode 62.46 tok/s
    CTOX_METAL_MAX_LAYERS=10 decode 53.95 tok/s
```

The Python target-only path is a valid local execution check, but the Python
DFlash path is not a valid high-performance baseline on this Mac. The Rust
Metal target path has improved materially (`9.55` → `32.64` decode tok/s on
the SD-card weights) but still has not closed the target-only gap to MLX
(`45.786` decode tok/s on the local Python target-only reference).

## Building / Running

Same CLI shape as the 27B bench, just re-targeted:

```bash
# macOS + Apple Silicon
cargo build --release --bin qwen35-35b-a3b-dflash-bench-metal

qwen35-35b-a3b-dflash-bench-metal <target_dir>  <draft.safetensors> \
                                  <prompt.bin> <n_gen> <out.bin>
```

`cargo run --example smoke_metal` validates the metallib and the required
DFlash/glue pipeline names without needing any weights on disk.

## Parity harness

Not yet copied into this crate. Once local 35B target/draft weights are
present and the text path survives a real target-only generation run, the
same three-script harness from
[`qwen35_27b_dflash/tests/parity/`](../qwen35_27b_dflash/tests/parity/)
gets copied under `tests/parity/` here and pointed at the A3B model refs.
