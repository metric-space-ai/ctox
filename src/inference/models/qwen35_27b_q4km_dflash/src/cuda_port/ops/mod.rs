//! Ported ggml-cuda op dispatchers.
//!
//! One file per upstream `.cu` we port. Each file mirrors the layout
//! of the corresponding `vendor/ggml-cuda/<op>.cu` with `// ref:`
//! anchors pointing at the upstream line ranges (same porting
//! discipline as `graph.rs` for the Qwen3.5 forward pass).

pub mod binbcast;
pub mod concat;
pub mod cpy;
pub mod cumsum;
pub mod diag;
pub mod fill;
pub mod norm;
pub mod pad;
pub mod scale;
pub mod tri;
pub mod unary;
