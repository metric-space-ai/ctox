# ctox-qwen36-35b-a3b-q4km-metal

Self-contained Rust + MSL kernel inference engine for
**Qwen/Qwen3.6-35B-A3B in Q4_K_M** on **Apple Silicon (M-series)**.
Stage-1 skeleton.

This crate is the bare-metal target the local-llm-inference-optimization
skill points at when CTOX runs on a Mac. It is intentionally separate
from `qwen36_35b_a3b_ggml/`, which is a transitional Unix-socket shim
that shells out to `llama-server` over loopback HTTP and stays in
place as a working fallback **and as the optimization baseline** until
this crate is accepted.

## The hard "no-library" rule

The whole point of this crate is to remove every runtime dependency on
ggml / llama.cpp. The produced binary links against:

- `libSystem` (libc, pthread) — OS boundary, fine
- `Metal.framework`, `MetalKit.framework`, `Foundation.framework` —
  OS boundary, fine
- nothing else inference-related

Specifically NOT linked: `libggml.dylib`, `libggml-metal.dylib`,
`libllama.dylib`, MLX, MPSGraph wrapped-as-library, ONNX Runtime, any
Python anything. `llama.cpp` and `ggml` enter this crate **only as
vendored source files** (in `vendor/ggml-metal/*.metal` and
`vendor/ggml-include/*.h`), copied 1:1 from a pinned upstream commit
and called via a Rust dispatcher in `src/metal_port/`. The
"vendor/ggml-metal" directory name describes which subtree of upstream
llama.cpp the kernel sources come from — it does not imply a link
against libggml.

The `qwen36_35b_a3b_ggml/` shim violates that rule by design (it runs
`llama-server` as a child process, which dlopens libggml-metal). It
exists for two reasons only:

1. To give the harness a working Qwen3.6-35B-A3B path while this crate
   is being built.
2. To serve as the optimization baseline — it runs the same Q4_K_M
   GGUF on the same M5 with the same Responses-IPC contract, so
   `t_ours / t_shim` per phase is the canonical "is the optimization
   working yet" metric. See [docs/kernel-dev/BENCHMARK_PROTOCOL.md](docs/kernel-dev/BENCHMARK_PROTOCOL.md).

## What "ohne dflash" means here

`Qwen3.6-35B-A3B` is a hybrid MoE: 40 decoder layers in a repeating
`[linear, linear, linear, full]` pattern (`full_attention_interval = 4`),
giving 30 linear-attention layers + 10 full-softmax-attention layers.
It also carries a 1-layer multi-token-prediction (MTP) head and a
27-layer ViT vision tower. See `vendor/upstream-config/`.

"Ohne dflash" scopes stage 1 to the **full-attention layers + MoE FFN
core** only — the linear-attention (DeltaNet-style, conv1d-kernel-4,
fp32 SSM state) layers are explicitly deferred. End-to-end inference
therefore does not work in stage 1; reference numbers come from
upstream `llama-bench` against the same Q4_K_M GGUF, and CTOX-side
verification is per-op until the linear-attention block ports too.

## Layout

```
src/core/inference/models/qwen36_35b_a3b_q4km_metal/
├── Cargo.toml                      standalone crate (no parent workspace)
├── build.rs                        rerun pins on vendor/* (no MSL build yet)
├── README.md                       this file
├── RESEARCH_LOG.md                 chronological tuning log (start)
├── vendor/
│   ├── upstream-config/            frozen HF config snapshot
│   ├── llama-cpp.version           commit pin (TBD; stage 2)
│   └── ggml-metal.version          subtree pin (TBD; stage 2)
├── docs/kernel-dev/
│   ├── MODEL_SHAPE.md              frozen kernel ABI (canonical)
│   ├── BENCHMARK_PROTOCOL.md       prefill/decode protocol
│   ├── EXPERIMENT_TEMPLATE.md      one-experiment record template
│   ├── DECISION_RECORD_TEMPLATE.md accept/reject/opt-in template
│   ├── FORENSICS_RECORD_TEMPLATE.md cache + roofline forensics template
│   └── ACCEPTED_PROFILE.env        accepted runtime config (empty stage 1)
└── src/
    ├── lib.rs                      re-exports
    ├── model.rs                    Qwen36MoeTextConfig + frozen const
    ├── loader.rs                   Q4_K_M GGUF loader skeleton
    ├── driver.rs                   Engine orchestration skeleton
    ├── server.rs                   Unix-socket Responses-IPC server
    ├── wire.rs                     vendored Responses-IPC types
    ├── metal_port/                 per-op MSL kernel ports (empty)
    └── bin/
        ├── server.rs               CLI for the IPC server
        ├── bench.rs                bench harness (stage-1: exits 2)
        └── probe_m5_metal.rs       hardware-fact probe (no Metal SDK yet)
```

## Stages of work (per the optimization skill)

- **Stage 1 — Skeleton + Freeze + Probe** (current). Crate scaffolding,
  frozen kernel ABI in `src/model.rs` and `docs/kernel-dev/MODEL_SHAPE.md`,
  Responses-IPC server stub returning `engine_not_ready`, hardware-fact
  probe binary.
- **Stage 2 — Baseline capture + GGUF loader + first MSL kernel.**
  Capture prefill/decode numbers from the existing
  `qwen36_35b_a3b_ggml/` shim against the Q4_K_M GGUF on this M5 (same
  Responses-IPC contract, so the comparison is apples-to-apples).
  Pin `vendor/llama-cpp.version`, copy the relevant `.metal` and `.h`
  files into `vendor/ggml-metal/` (source only; no link against the
  upstream's compiled `libggml-metal.dylib`), write the Q4_K_M GGUF
  loader, and port `rms_norm` (simplest non-trivial kernel) end-to-end
  with a per-op verifier.
- **Stage 3 — Full-attention forward path.** RoPE-partial M-RoPE,
  GQA softmax SDPA, attn_output_gate, full Q4_K_M `mul_mat`, MoE
  router top-8, expert + shared SwiGLU. Block-smoke verifier compares
  one full-attention layer against the upstream reference.
- **Stage 4 — Linear-attention layers (the "dflash" port).** This is
  the one stage 1 explicitly defers. Without it, end-to-end inference
  does not run.
- **Stage 5 — Roofline-driven autotuning + accepted profile.** Tile
  sweeps, layout sweeps, MPSGraph sidecars where they win; promotion
  gates per `docs/kernel-dev/`.

## Building

```bash
# Just the Rust surface, no Metal SDK required:
cargo check
cargo test

# Stage 2+: compile the vendored MSL kernels and link against
# AppKit/Metal frameworks (this will require Xcode CLT). Gated under
# the `metal` feature, off by default.
cargo build --release --features metal
```

## Running the host probe

```bash
cargo run --release --bin qwen36-35b-a3b-q4km-metal-probe | tee probe.json
```

Output is the canonical input to the "Capture hardware facts" step of
the optimization skill (method-playbook §4). On the M5 dev box used to
bootstrap this crate it reports `chip="Apple M5"`, `gpu_cores=10`,
`unified_memory_bytes ≈ 32 GiB`, `metal_support="Metal 4"`,
`macos_product_version="26.2"`.

## Self-containment contract

Every model-specific Rust file lives inside this directory; **no code
is shared with other model crates**, including the existing
`qwen36_35b_a3b_ggml/` shim. The vendored MSL kernel sources, when
they land in stage 2, will be copied 1:1 from upstream llama.cpp at the
commit pinned in `vendor/llama-cpp.version`; the dispatcher port goes
into `src/metal_port/ops/` with `// ref: vendor/ggml-metal/<file>:<line>`
anchors per the optimization skill rule (and per the pattern already
established in `qwen35_27b_q4km_dflash/src/cuda_port/`).

The kernel sources are vendored as **text**, not linked as a library.
`build.rs` will compile them with `xcrun -sdk macosx metal -c` +
`metallib` into a `default.metallib` shipped next to the binary; no
inference framework appears in the link command.
