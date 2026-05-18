# ctox-qwen35-27b-q4km-dflash

Rust inference crate for the **Qwen3.5-27B Q4_K_M** target model paired
with the **z-lab DFlash** speculative-decoding block-diffusion draft,
treated as one curated model.

Byte-exact port of the [lucebox/dflash](https://github.com/lucebox/dflash)
C++ reference. Every Rust module corresponds 1:1 to a `.cpp`/`.h`/`.cu`
file in the reference; ops are built on top of the ggml C API via the
in-crate FFI bindings in `src/ffi.rs`.

## Layout

```
src/core/inference/models/qwen35_27b_q4km_dflash/
├── Cargo.toml                  standalone crate (no parent workspace)
├── README.md                   this file
├── build.rs                    ggml link + f16_convert.cu nvcc step
├── vendor/
│   ├── ggml-cuda/              61 ggml-cuda .cu files + f16_convert.cu + .cuh headers
│   │                            (source-of-truth: llama.cpp b16de65)
│   ├── ggml-include/           ggml C API headers (ggml.h, gguf.h, etc.)
│   ├── llama-cpp.version       upstream commit pin
│   └── dflash.version          lucebox commit pin
└── src/
    ├── lib.rs                  re-exports + constants + last_error
    ├── ffi.rs                  raw ggml / ggml-backend / ggml-cuda / gguf bindings
    ├── model.rs                TargetWeights / DraftWeights / TargetCache
    │                            (ref: internal.h)
    ├── loader.rs               gguf target + safetensors draft loaders
    │                            (ref: gguf_target_loader.cpp + safetensors_draft.cpp)
    ├── graph.rs                all ggml graph builders + delta-net chunking
    │                            (ref: qwen35_target_graph.cpp +
    │                                  qwen3_dflash_graph.cpp +
    │                                  delta_net_chunked.cpp)
    ├── ddtree.rs               DDTree tree-verify helpers
    ├── driver.rs               3-mode spec-decode driver
    │                            (ref: test_dflash.cpp)
    └── bin/bench.rs            `qwen35-27b-q4km-dflash-bench` CLI
```

## Current Status

This crate is **not yet the final CTOX bare-metal engine**. The
end-to-end 27B CUDA path is byte-exact and fast, but it still links the
bench/server binaries against the lucebox-built `libggml-base.so` and
`libggml-cuda.so` for the full forward graph.

What is verified today:

- The end-to-end DFlash bench output is byte-identical to the C++
  reference for the measured A6000 run.
- The best measured CTOX mode is `--fast-rollback --ddtree
  --ddtree-budget 22`.
- The vendored bare-metal CUDA dispatcher port currently has standalone
  verifiers for 15 op groups: `binbcast`, `concat`, `cpy`, `cumsum`,
  `diag`, `fill`, `pad`, `rms_norm`, `rope`, `scale`, `softmax`,
  `solve_tri`, `ssm_conv`, `tri`, and `unary`.

What is still missing before this satisfies the CTOX architecture rule:

- remove `libggml-base.so` / `libggml-cuda.so` from the link layer;
- port the remaining hot op families into Rust launchers over vendored
  kernels, especially quantized `mul_mat`/MMQ/MMVQ, Flash Attention,
  GatedDeltaNet tree/persist, embedding/get-rows, diag-mask, and graph
  allocation/execution;
- cut `graph.rs` and `driver.rs` over from `ggml_cgraph` /
  `ggml_backend_graph_compute` to the Rust-side CUDA graph executor.

## Self-containment

Every model-specific Rust file lives inside this directory — **no code is
shared with other models**. The vendored `ggml-cuda/` tree is pinned by
`vendor/llama-cpp.version` and is used in two ways:

- current end-to-end path: via the lucebox-built `libggml-cuda.so`;
- migration path: selected `.cu` files are compiled by `build.rs` into
  PTX and launched by `src/cuda_port`.

The current end-to-end path is therefore self-contained at the source
tree level, but not yet self-contained at the binary link/runtime level.

Per-compute-capability optimization happens at the nvcc layer:
`CTOX_CUDA_SM` (default `86`) is passed as `-arch=sm_XX` when compiling
`f16_convert.cu`, and the linked `libggml-cuda.so` is itself built with
the same SM target.

## Building

```bash
# dev box with lucebox reference build tree available:
GGML_LIB_DIR=/home/metricspace/dflash-ref/dflash/build/deps/llama.cpp/ggml/src \
    cargo build --release --features=cuda

# just the Rust surface, no CUDA toolchain required:
cargo check
```

## Running the bench

```bash
GGML_LIB_DIR=<path>                                                          \
    LD_LIBRARY_PATH=$GGML_LIB_DIR:$GGML_LIB_DIR/ggml-cuda:$LD_LIBRARY_PATH   \
    cargo run --release --features=cuda --bin qwen35-27b-q4km-dflash-bench   \
        -- <target.gguf> <draft.safetensors> <prompt.bin> <n_gen> <out.bin>  \
        --fast-rollback                                                      \
        --ddtree --ddtree-budget 22
```

Verifies bit-exact against the reference's `test_dflash` output via `cmp`
on the `out.bin` file.

## Verified A6000 Run

Machine: NVIDIA RTX A6000, 49 GiB VRAM, driver 580.105.08.
Prompt: `/home/metricspace/dflash-ref/dflash/tmp_prompt128.bin`
Target: `/home/metricspace/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf`
Draft: `/home/metricspace/dflash-ref/dflash/models/draft/model.safetensors`
Generation length: 128 new tokens.

| Implementation | Mode | Prefill | Decode |
|---|---|---:|---:|
| C++ reference | default replay | 128 tokens in 0.42 s | 98.93 tok/s |
| C++ reference | `--fast-rollback` | 128 tokens in 0.42 s | 141.38 tok/s |
| C++ reference | `--fast-rollback --ddtree --ddtree-budget=22` | 128 tokens in 0.42 s | 165.81 tok/s |
| CTOX Rust hybrid | `--fast-rollback --ddtree --ddtree-budget=22` | 128 tokens in 0.421 s | 156.69 tok/s |

The CTOX Rust hybrid output was checked with `cmp` against the C++
reference output and matched byte-for-byte for this run.

## Porting discipline

Every function carries a `// ref: <file>:<line-range>` doc annotation so
reviewers can diff against the C++ source line-by-line. Variable names
match the reference (`ne[0..3]` / `nb[0..3]` etc.). Comments from the
reference are translated verbatim when they describe algorithm;
paraphrased only when they reference C/C++ constructs that don't exist
in Rust.
