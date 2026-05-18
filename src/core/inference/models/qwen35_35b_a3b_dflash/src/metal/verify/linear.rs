//! Small-projection linear verify path.
//!
//! The Python reference swaps `nn.QuantizedLinear` modules in the
//! model tree with a `VerifyQuantizedLinear` that checks at call
//! time whether the batch dim `M` equals 16 (the verify spec-decode
//! block). If so, it routes through `verify_matmul` (the
//! simdgroup-MMA kernels); otherwise it falls back to the stock
//! `mx.quantized_matmul` path.
//!
//! In Rust we collapse the nn-module-tree rewrite into a thin wrapper
//! `VerifyQuantizedLinear` that holds the same buffer handles as
//! [`super::super::qwen::Linear4Bit`] plus the eligibility filter
//! logic (bits/group-size/dim gates, and the `DFLASH_VERIFY_INCLUDE`
//! env-driven suffix tag allow-list).
//!
//! The actual dispatch lives in [`super::qmm`].
//!
//! ref: `dflash_mlx/verify_linear.py`

use std::env;

use crate::metal::ffi::{Buffer, ComputeEncoder, Device};
use crate::metal::qwen::Linear4Bit;
use crate::metal::verify::qmm;

/// Default value of `DFLASH_VERIFY_MAX_N` — skip verify on wider
/// output projections than this because the M=16 specialization
/// doesn't recoup its overhead at very large N.
const VERIFY_MAX_N_DEFAULT: i32 = 100_000;

/// Suffix tag for a projection path. Mirrors `_PROJ_TAGS` in the Python.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjTag {
    MlpGate,
    MlpUp,
    MlpDown,
    AttnQ,
    AttnK,
    AttnV,
    AttnO,
    GdnQkv,
    GdnZ,
    GdnO,
    Other,
}

impl ProjTag {
    pub fn from_path(path: &str) -> Self {
        use ProjTag::*;
        if path.ends_with("mlp.gate_proj") {
            MlpGate
        } else if path.ends_with("mlp.up_proj") {
            MlpUp
        } else if path.ends_with("mlp.down_proj") {
            MlpDown
        } else if path.ends_with("self_attn.q_proj") {
            AttnQ
        } else if path.ends_with("self_attn.k_proj") {
            AttnK
        } else if path.ends_with("self_attn.v_proj") {
            AttnV
        } else if path.ends_with("self_attn.o_proj") {
            AttnO
        } else if path.ends_with("linear_attn.in_proj_qkv") {
            GdnQkv
        } else if path.ends_with("linear_attn.in_proj_z") {
            GdnZ
        } else if path.ends_with("linear_attn.out_proj") {
            GdnO
        } else {
            Other
        }
    }

    pub fn as_str(&self) -> &'static str {
        use ProjTag::*;
        match self {
            MlpGate => "mlp_gate",
            MlpUp => "mlp_up",
            MlpDown => "mlp_down",
            AttnQ => "attn_q",
            AttnK => "attn_k",
            AttnV => "attn_v",
            AttnO => "attn_o",
            GdnQkv => "gdn_qkv",
            GdnZ => "gdn_z",
            GdnO => "gdn_o",
            Other => "other",
        }
    }
}

fn env_i32(name: &str, default: i32) -> i32 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Decide whether a linear projection should be routed through the
/// verify-specialized dispatch given its shape + path.
pub fn is_verify_eligible(
    bits: i32,
    group_size: i32,
    in_features: i32,
    out_features: i32,
    path: &str,
) -> bool {
    if bits != 4 {
        return false;
    }
    if group_size != 32 && group_size != 64 && group_size != 128 {
        return false;
    }
    if out_features % 32 != 0 || in_features % 32 != 0 {
        return false;
    }
    if out_features >= env_i32("DFLASH_VERIFY_MAX_N", VERIFY_MAX_N_DEFAULT) {
        return false;
    }
    let include = env::var("DFLASH_VERIFY_INCLUDE")
        .unwrap_or_else(|_| "all".into())
        .trim()
        .to_lowercase();
    if include.is_empty() || include == "all" {
        return true;
    }
    let tag = ProjTag::from_path(path).as_str();
    let groups: Vec<&str> = include.split(',').map(|s| s.trim()).collect();
    let mut allowed: std::collections::BTreeSet<&str> = groups.iter().copied().collect();
    if allowed.contains("mlp") {
        allowed.extend(["mlp_gate", "mlp_up", "mlp_down"]);
    }
    if allowed.contains("attn") {
        allowed.extend(["attn_q", "attn_k", "attn_v", "attn_o"]);
    }
    if allowed.contains("gdn") {
        allowed.extend(["gdn_qkv", "gdn_z", "gdn_o"]);
    }
    allowed.contains(tag)
}

/// Wrapper around a [`Linear4Bit`] that gets the verify-specialized
/// dispatch when the batch dim M == 16, and falls back to the
/// generic quantized-matmul otherwise.
pub struct VerifyQuantizedLinear {
    pub base: Linear4Bit,
    /// Optional per-out-channel bias in bf16 (some Qwen variants have
    /// attention `q_proj` biases; the 35B-A3B MLX-4bit export does not).
    pub bias: Option<Buffer>,
    pub group_size: i32,
    pub bits: i32,
}

impl VerifyQuantizedLinear {
    pub fn from_linear(base: Linear4Bit, bias: Option<Buffer>, group_size: i32) -> Self {
        Self {
            base,
            bias,
            group_size,
            bits: 4,
        }
    }

    /// Forward pass with spec-decode dispatch.
    ///
    /// `m` is the flattened batch dim (product of all leading axes of `x`).
    /// `x_buf`/`y_buf` are the usual bf16 activation buffers.
    pub fn forward(
        &self,
        enc: &ComputeEncoder,
        dev: &Device,
        x: &Buffer,
        y: &Buffer,
        m: i32,
    ) -> bool {
        if m == 16 {
            qmm::dispatch_verify(
                enc,
                dev,
                &self.base,
                x,
                y,
                m,
                self.group_size,
                self.bits,
                None,
            )
        } else {
            self.base.forward(enc, dev, x, y, m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_tag_basic() {
        assert_eq!(
            ProjTag::from_path("model.layers.3.mlp.down_proj"),
            ProjTag::MlpDown
        );
        assert_eq!(
            ProjTag::from_path("model.layers.3.self_attn.q_proj"),
            ProjTag::AttnQ
        );
        assert_eq!(ProjTag::from_path("model.embed_tokens"), ProjTag::Other);
    }

    #[test]
    fn eligibility_bits_and_group() {
        assert!(!is_verify_eligible(8, 64, 1024, 1024, "mlp.gate_proj"));
        assert!(!is_verify_eligible(4, 48, 1024, 1024, "mlp.gate_proj"));
        assert!(is_verify_eligible(4, 64, 1024, 1024, "mlp.gate_proj"));
        assert!(!is_verify_eligible(4, 64, 1024, 1023, "mlp.gate_proj"));
    }
}
