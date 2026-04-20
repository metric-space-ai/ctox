//! DFlash block-diffusion draft for Qwen3.5-family targets.
//!
//! Port of `z-lab/Qwen3.5-27B-DFlash`
//! (<https://huggingface.co/z-lab/Qwen3.5-27B-DFlash>) into the CTOX
//! candle hard-fork, matching the reference C++/ggml implementation in
//! `dflash/src/qwen3_dflash_graph.cpp` tensor-for-tensor.
//!
//! # What this is
//!
//! A five-layer dense Qwen3-flavoured transformer that, given
//!   - `input_ids`: `[B, block_size]` = `[last_target_tok, MASK × 15]`,
//!   - `cond`:      `[B, ctx_len, 5 × hidden]` = captured target hidden
//!     states at layer indices `target_layer_ids` (for 27B: [1, 16, 31, 46, 61]),
//!
//! returns hidden states `[B, block_size, hidden]`. The caller projects
//! these through the **target's** `lm_head` — the draft does not carry
//! its own token embedding or lm head. That keeps the draft weights to
//! 3.46 GB BF16 and guarantees the output distribution is in the exact
//! vocabulary of the target for rejection sampling.
//!
//! # Why it is structurally stronger than a chain AR draft
//!
//! Every masked position in the block attends to the same captured
//! target context (via cross-attention mixed with self-attention over
//! the noise positions). So a Qwen3.5-0.8B autoregressive draft
//! predicts token i from its own noisy prediction at i-1; the DFlash
//! draft predicts token i directly from real target features. Mean
//! Acceptance Length on the reference is ~8 (HumanEval) versus ~3 for
//! chain EAGLE, which is the whole reason the pipeline hits ~95 tok/s
//! on an A6000 instead of ~40.
//!
//! # Scope of this commit
//!
//! This module is **standalone**: it loads the safetensors, builds the
//! graph, and runs a forward pass. It does not plug into the engine
//! pipeline (`SpeculativePipeline` / `DFlashPipeline`) yet and does not
//! quantise — the draft stays BF16 at 3.46 GB on VRAM. Commits 2-5
//! build on top:
//!   - Commit 2 adds the target-feature capture hook on Qwen3.5.
//!   - Commit 3 wires a new `DFlashPipeline` with chain verify.
//!   - Commit 4 adds DDTree tree-structured verify.
//!   - Commit 5 ports the three tree CUDA kernels from the ggml fork.

pub mod capture;
pub mod config;
pub mod model;

pub use capture::FeatureCapture;
pub use config::DFlashDraftConfig;
pub use model::DFlashDraftModel;
