//! ctox-qwen35-27b-q4km-dflash — byte-exact Rust port of lucebox/dflash
//! for the Qwen3.5-27B Q4_K_M target paired with the z-lab block-diffusion
//! DFlash draft.
//!
//! # Model pairing (one model)
//!
//! CTOX treats the Qwen3.5-27B Q4_K_M target and its DFlash drafter as a
//! single curated model. Every bit of hardware-specific code the pair
//! needs — CUDA kernels for each SM target, Metal shaders, ggml C API
//! FFI, loaders, graph builders and the speculative-decode driver —
//! lives inside this crate. There is no `ctox-ggml-sys`-style shared
//! FFI layer: other models vendor their own kernels and their own
//! ggml/ggml-cuda trees, with no code sharing between model crates.
//!
//! # File layout
//!
//! ```text
//! src/inference/models/qwen35_27b_q4km_dflash/
//! ├── Cargo.toml
//! ├── README.md
//! ├── build.rs                  ← nvcc per-SM → kernels/sm_XX/*
//! ├── kernels/{sm_80,sm_86,sm_89,sm_90,metal}/
//! ├── vendor/                   ← ggml-cuda + ggml-include + f16_convert.cu
//! └── src/
//!     ├── lib.rs                ← this file
//!     ├── ffi.rs                ← all ggml / ggml-cuda / gguf / CUDA FFI
//!     ├── loader.rs             ← gguf target + safetensors draft loaders
//!     ├── model.rs              ← TargetWeights / DraftWeights / TargetCache
//!     ├── graph.rs              ← all ggml graph builders + delta-net chunk
//!     ├── ddtree.rs             ← DDTree tree-verify helpers
//!     ├── driver.rs             ← 3-mode spec-decode driver
//!     └── bin/bench.rs          ← `qwen35-27b-q4km-dflash-bench` CLI
//! ```
//!
//! # Porting discipline
//!
//! Each function carries a `// ref: <file>:<line-range>` doc annotation
//! so reviewers can diff against the C++ reference line-by-line. Variable
//! names match the reference (`ne[0..3]` / `nb[0..3]` etc.). Comments
//! from the reference are translated verbatim when they describe
//! algorithm; paraphrased only when they reference C/C++ constructs that
//! don't exist in Rust.

pub mod ddtree;
pub mod driver;
pub mod ffi;
pub mod graph;
pub mod loader;
pub mod model;

// Server-side modules — wire + tokenizer + adapter + socket listener.
// Enabled on the server binary's required-features path (cuda-only);
// listed unconditionally here because the Rust sources compile without
// CUDA too and we want `cargo check` to cover them.
pub mod adapter;
pub mod server;
pub mod tokenizer;
pub mod wire;

// ─── Target model constants ─────────────────────────────────────────
//
// ref: `dflash/include/dflash27b.h`

pub const DFLASH27B_TARGET_HIDDEN: i32 = 5120;
pub const DFLASH27B_TARGET_LAYERS: i32 = 64;

/// NOTE: the `DFLASH27B_TARGET_N_*` / `_HEAD_DIM` constants are
/// DRAFT dimensions (z-lab draft: 32 Q heads, 8 KV heads, 128 head_dim).
/// The TARGET Qwen3.5-27B hybrid uses 24 Q heads, 4 KV heads, 256 head_dim,
/// which live on `TargetWeights` (`n_embd_head_k/v`, `n_head`, `n_head_kv`).
/// Naming is historical — do NOT change without updating the draft loader
/// + draft graph, which consume these as draft-side constants.
pub const DFLASH27B_TARGET_N_HEADS: i32 = 32;
pub const DFLASH27B_TARGET_N_KV_HEADS: i32 = 8;
pub const DFLASH27B_TARGET_HEAD_DIM: i32 = 128;
pub const DFLASH27B_TARGET_INTERMEDIATE: i32 = 17408;
pub const DFLASH27B_TARGET_VOCAB: i32 = 248320;
pub const DFLASH27B_ROPE_THETA: f32 = 10_000_000.0;
pub const DFLASH27B_RMS_EPS: f32 = 1e-6;

// ─── Draft model constants ──────────────────────────────────────────

pub const DFLASH27B_DRAFT_LAYERS: i32 = 5;
pub const DFLASH27B_DRAFT_BLOCK_SIZE: i32 = 16;

/// fc projects 5*hidden -> hidden
pub const DFLASH27B_DRAFT_N_TARGET_LAYERS: i32 = 5;
pub const DFLASH27B_DRAFT_MASK_TOKEN_ID: i32 = 248_070;

/// target_layer_ids = {1, 16, 31, 46, 61} — 0-indexed target layers whose
/// OUTPUT we capture (HF hidden_states[lid + 1]).
pub const DFLASH27B_DRAFT_TARGET_LAYER_IDS: [i32; 5] = [1, 16, 31, 46, 61];

// ─── Thread-safe last-error slot ────────────────────────────────────
//
// ref: `dflash/src/errors.cpp:1-27`

use std::sync::{Mutex, OnceLock};

fn last_error_slot() -> &'static Mutex<String> {
    static SLOT: OnceLock<Mutex<String>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(String::new()))
}

/// Record a new last-error message. Loader and graph error paths call
/// this in place of the C++ `dflash27b::set_last_error(...)`.
pub fn set_last_error(msg: impl Into<String>) {
    let mut g = last_error_slot().lock().unwrap();
    *g = msg.into();
}

/// Read the most recent error. Mirrors `dflash27b_last_error()` on the
/// C side — callers get an owned `String` (idiomatic Rust) rather than
/// a `const char *`; the semantics (next `set_last_error` overwrites)
/// are preserved.
pub fn last_error() -> String {
    last_error_slot().lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_byte_match() {
        assert_eq!(DFLASH27B_TARGET_HIDDEN, 5120);
        assert_eq!(DFLASH27B_TARGET_LAYERS, 64);
        assert_eq!(DFLASH27B_TARGET_N_HEADS, 32);
        assert_eq!(DFLASH27B_TARGET_N_KV_HEADS, 8);
        assert_eq!(DFLASH27B_TARGET_HEAD_DIM, 128);
        assert_eq!(DFLASH27B_TARGET_INTERMEDIATE, 17_408);
        assert_eq!(DFLASH27B_TARGET_VOCAB, 248_320);
        assert!((DFLASH27B_ROPE_THETA - 10_000_000.0).abs() < 1e-3);
        assert!((DFLASH27B_RMS_EPS - 1e-6).abs() < 1e-12);
        assert_eq!(DFLASH27B_DRAFT_LAYERS, 5);
        assert_eq!(DFLASH27B_DRAFT_BLOCK_SIZE, 16);
        assert_eq!(DFLASH27B_DRAFT_N_TARGET_LAYERS, 5);
        assert_eq!(DFLASH27B_DRAFT_MASK_TOKEN_ID, 248_070);
        assert_eq!(DFLASH27B_DRAFT_TARGET_LAYER_IDS, [1, 16, 31, 46, 61]);
    }

    #[test]
    fn last_error_set_and_read() {
        set_last_error("boom");
        assert_eq!(last_error(), "boom");
        set_last_error("different");
        assert_eq!(last_error(), "different");
    }
}
