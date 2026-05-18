# Byte-for-byte parity harness

The Rust port of [bstnxbt/dflash-mlx](https://github.com/bstnxbt/dflash-mlx)
must produce output **byte-identical** to the Python reference for any
deterministic-decode setting. This directory carries the scripts that
prove (or fail) that property.

## Gate 0 — pre-parity Rust-only smoke

Before any Python-reference comparison is meaningful, the Rust port's
Metal pipeline has to at least load its metallib and dispatch a kernel
without error. Run:

```
cargo run --release --example smoke_metal
```

Expected output:

```
[smoke] metallib resolves + 32-element copy_raw_bf16 ok
```

If this fails, nothing else in this directory will work. Fix the
metallib build + pipeline cache first, then come back.

## Gate 1 — byte-for-byte parity

Once smoke passes, the three scripts below run a full decode cycle on
both sides and `cmp(1)` the outputs.

1. `run_python_reference.sh <prompt.txt> <n_gen> <ref.bin>`
   Installs the vendored reference commit (pinned in
   `../../vendor/metal/dflash-mlx.version`) into a scratch venv and
   runs it against the prompt. Writes prompt + generated tokens as
   `i32` little-endian — same format as the CUDA bench.

2. `run_rust_port.sh <prompt_ids.bin> <n_gen> <port.bin>`
   Builds `qwen35-35b-a3b-dflash-bench-metal` (release) and runs it on
   the same prompt IDs.

3. `compare.sh <ref.bin> <port.bin>`
   Plain `cmp(1)` between the two. Exits 0 iff byte-identical.

## What has to match

| Surface                                | Must match? | Notes                                                                  |
|----------------------------------------|:-----------:|------------------------------------------------------------------------|
| Token sequence (prompt + generated)    |     Yes     | The greedy-accept rule picks the target model's argmax every step; any divergence means a real bug. |
| Tape-replay recurrent state (per-step) |     Yes     | `tape_replay` is byte-lossless by construction; drift = kernel bug.   |
| Raw logits (fp values)                 |     No      | Stock MLX vs our Metal port disagree in the low bits for bf16 matmul reductions; that's allowed by the reference README. Argmax stability is what matters. |

Only the lossless **greedy** path is exercised. Sampling (temperature,
top_p, top_k) introduces RNG dependence and is out of scope.

## Host prerequisites

Step 1 (the Python reference run) needs:

* macOS + Apple Silicon (the Python reference is MLX-only).
* Python 3.12 for the reference venv. This checkout verified Python 3.12.13
  installed by `uv`; the system `python3` is 3.14 and should not be used for
  the reference gate. Install via Homebrew or `uv`:
  ```
  brew install python@3.12
  # or:
  uv python install 3.12
  ```
* A complete local model store. On this Mac the verified store path is:
  ```
  /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit
  /Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash
  ```

Step 2 (the Rust port run) needs:

* A local copy of both weight sets. The Rust port does **not**
  download from HuggingFace itself (the CTOX installer is responsible
  for that). Set:
  ```
  export CTOX_QWEN35_TARGET_DIR=/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-4bit
  export CTOX_QWEN35_DRAFT_PATH=/Volumes/Models/huggingface/local/Qwen3.5-35B-A3B-DFlash/model.safetensors
  ```

## Current status of the gate

| Gate                                       | Status         |
|--------------------------------------------|----------------|
| 0. metallib loads + kernel dispatches      | **PASS** (`examples/smoke_metal`) |
| 1a. `one_cycle` degenerate AR path         | starts with real SD-card weights, but the 1-token target-only run was killed after >5 min in `MTLCommandBuffer::waitUntilCompleted` |
| 1b. `one_cycle` full spec-decode (draft + verify + tape-replay) | pending |
| 1c. Draft model loader (bf16 safetensors)  | done |
| 1d. Head repeat kernel for GDN (num_k_heads != num_v_heads) | pending |
| 2. Python reference runs on this host      | target-only MLX runs; DFlash reference is not performant on this Mac |

The April 25, 2026 local reference check used the SD-card model store above.
`mlx_lm.generate` target-only produced 64 tokens from prompt `hello` at
45.786 decode tok/s with 19.593 GB peak memory. `dflash-mlx` with the matching
35B-A3B draft completed a 1-token run in 131.7 s wall time and printed
`1 tokens | 0.0 tok/s | 0.0% acceptance`; 4-token runs with and without
`DFLASH_VERIFY_QMM` were killed after 180 s with only one token emitted.
That reference remains useful for source-level kernel and dispatcher porting,
but it is not a valid high-performance local baseline on this Mac.
