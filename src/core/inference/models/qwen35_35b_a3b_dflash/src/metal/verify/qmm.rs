//! Verify-specialized int4 quantized matmul dispatch logic.
//!
//! Port of `dflash_mlx/verify_qmm.py`. The Metal kernels themselves are
//! in `vendor/metal/shaders/dflash/verify_qmm_mma2big*.metal` and are
//! driven through `metal::kernels::verify_qmm_*`.
//!
//! Two variants (same names as the reference):
//!
//!   * `mma2big`       — no K-split, single-pass accumulation.
//!   * `mma2big_pipe`  — K-split with double-buffered staging of the
//!                       dequantized B-tile. Writes per-k-part partials
//!                       which we then reduce-sum on the Rust side.
//!
//! Variant selection:
//!
//!   * `DFLASH_VERIFY_VARIANT=auto` (default) → reference heuristic
//!     [`auto_variant`] mirroring the reference (K >= 8192 OR
//!     N <= 8192 → pipe/KP=8; else mma2big).
//!   * `DFLASH_VERIFY_VARIANT={mma2big,mma2big_pipe}` forces a
//!     variant.
//!   * `DFLASH_VERIFY_QMM_KPARTS=<int>` overrides the K-split (default 4).
//! Verify-QMM is not optional in the 35B DFlash hot path. If the exact
//! high-performance kernel cannot be used, dispatch fails instead of silently
//! routing through a generic quantized matmul.
//!
//! ref: `dflash_mlx/verify_qmm.py`

use crate::common::errors::set_last_error;
use crate::metal::ffi::{Buffer, ComputeEncoder, Device};
use crate::metal::kernels;
use crate::metal::qwen::Linear4Bit;
use crate::metal::work::{VERIFY_QMM_MAX_KPARTS, VERIFY_QMM_MAX_N};
use std::env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Mma2big,
    Mma2bigPipe,
}

fn env_i32(name: &str, default: i32) -> i32 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_str(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

pub fn is_enabled() -> bool {
    true
}

/// ref: `verify_qmm.py::_auto_variant`
pub fn auto_variant(k: i32, n: i32) -> (Variant, i32) {
    if k >= 8192 || n <= 8192 {
        (Variant::Mma2bigPipe, 8)
    } else {
        (Variant::Mma2big, 1)
    }
}

/// Pick the concrete variant + K_PARTS value for the current call.
/// Env overrides win over the auto heuristic.
pub fn resolve_variant(k: i32, n: i32) -> (Variant, i32) {
    let requested = env_str("DFLASH_VERIFY_VARIANT", "auto");
    let (auto_v, auto_kp) = auto_variant(k, n);
    let variant = match requested.as_str() {
        "mma2big" => Variant::Mma2big,
        "mma2big_pipe" => Variant::Mma2bigPipe,
        _ => auto_v,
    };
    let k_parts = match variant {
        Variant::Mma2bigPipe => env_i32("DFLASH_VERIFY_QMM_KPARTS", auto_kp.max(1)),
        Variant::Mma2big => 1,
    };
    (variant, k_parts)
}

/// ref: `verify_qmm.py::_should_use_verify`
pub fn should_use_verify(m: i32, bits: i32, group_size: i32) -> bool {
    if !is_enabled() {
        return false;
    }
    if bits != 4 {
        return false;
    }
    if group_size != 32 && group_size != 64 && group_size != 128 {
        return false;
    }
    m == 16
}

/// Full verify-matmul dispatch. This is a required high-performance path:
/// if the shape gates reject or a selected variant is not fully wired, return
/// false with a precise error. No generic matmul fallback is allowed here.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_verify(
    enc: &ComputeEncoder,
    dev: &Device,
    base: &Linear4Bit,
    x: &Buffer,
    y: &Buffer,
    m: i32,
    group_size: i32,
    bits: i32,
    partials: Option<&Buffer>,
) -> bool {
    if !should_use_verify(m, bits, group_size) {
        set_last_error(format!(
            "verify_qmm: unsupported shape M={m} bits={bits} group_size={group_size}; \
             no generic quantized-matmul fallback is allowed"
        ));
        return false;
    }
    if group_size != 64 {
        set_last_error(format!(
            "verify_qmm: group_size={group_size} requires its own vendored kernel wiring; \
             only gs64 is wired in this crate"
        ));
        return false;
    }
    let k = base.in_features;
    let n = base.out_features;
    let (variant, k_parts) = resolve_variant(k, n);

    match variant {
        Variant::Mma2big => {
            if n % 32 != 0 || k % 32 != 0 {
                set_last_error(format!(
                    "verify_qmm_mma2big: unsupported shape M={m} K={k} N={n}; \
                     no generic quantized-matmul fallback is allowed"
                ));
                return false;
            }
            kernels::verify_qmm_mma2big_gs64_bf16(
                enc,
                dev,
                x,
                &base.w_q,
                &base.scales,
                &base.biases,
                y,
                m,
                k,
                n,
            )
        }
        Variant::Mma2bigPipe => {
            if n % 32 != 0 || k % (32 * k_parts) != 0 {
                set_last_error(format!(
                    "verify_qmm_mma2big_pipe: unsupported shape M={m} K={k} N={n} \
                     K_PARTS={k_parts}; no generic quantized-matmul fallback is allowed"
                ));
                return false;
            }
            if k_parts as usize > VERIFY_QMM_MAX_KPARTS || n as usize > VERIFY_QMM_MAX_N {
                set_last_error(format!(
                    "verify_qmm_mma2big_pipe: scratch too small for K_PARTS={k_parts} N={n}; \
                     max K_PARTS={VERIFY_QMM_MAX_KPARTS} max N={VERIFY_QMM_MAX_N}; \
                     no generic quantized-matmul fallback is allowed"
                ));
                return false;
            }
            let Some(partials) = partials else {
                set_last_error(
                    "verify_qmm_mma2big_pipe: missing partials scratch; \
                     no generic quantized-matmul fallback is allowed",
                );
                return false;
            };
            if !kernels::verify_qmm_mma2big_pipe_gs64_bf16(
                enc,
                dev,
                x,
                &base.w_q,
                &base.scales,
                &base.biases,
                partials,
                m,
                k,
                n,
                k_parts,
            ) {
                return false;
            }
            kernels::verify_qmm_reduce_partials_bf16(enc, dev, partials, y, k_parts, n)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn dispatch_verify_with_scratch(
    enc: &ComputeEncoder,
    dev: &Device,
    base: &Linear4Bit,
    x: &Buffer,
    y: &Buffer,
    m: i32,
    group_size: i32,
    bits: i32,
    partials: &Buffer,
) -> bool {
    dispatch_verify(enc, dev, base, x, y, m, group_size, bits, Some(partials))
}

/// Force the non-pipe M=16 verify kernel.
///
/// This is the currently fully wired high-performance verify path in CTOX.
/// The reference auto-heuristic often prefers `mma2big_pipe`, but that path
/// needs a `partials -> reduce -> bf16` scratch pipeline. Until that is wired,
/// call sites that choose verify must use this explicit kernel rather than
/// silently falling through to generic quantized matmul.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_verify_mma2big(
    enc: &ComputeEncoder,
    dev: &Device,
    base: &Linear4Bit,
    x: &Buffer,
    y: &Buffer,
    m: i32,
    group_size: i32,
    bits: i32,
) -> bool {
    if !should_use_verify(m, bits, group_size) {
        set_last_error(format!(
            "verify_qmm_mma2big: unsupported shape M={m} bits={bits} group_size={group_size}; \
             no generic quantized-matmul fallback is allowed"
        ));
        return false;
    }
    if group_size != 64 {
        set_last_error(format!(
            "verify_qmm_mma2big: group_size={group_size} requires its own vendored kernel wiring; \
             only gs64 is wired in this crate"
        ));
        return false;
    }
    let k = base.in_features;
    let n = base.out_features;
    if n % 32 != 0 || k % 32 != 0 {
        set_last_error(format!(
            "verify_qmm_mma2big: unsupported shape M={m} K={k} N={n}; \
             no generic quantized-matmul fallback is allowed"
        ));
        return false;
    }
    kernels::verify_qmm_mma2big_gs64_bf16(
        enc,
        dev,
        x,
        &base.w_q,
        &base.scales,
        &base.biases,
        y,
        m,
        k,
        n,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_variant_big_k() {
        assert_eq!(auto_variant(16_384, 4096), (Variant::Mma2bigPipe, 8));
    }

    #[test]
    fn auto_variant_small_both() {
        assert_eq!(auto_variant(1024, 4096), (Variant::Mma2bigPipe, 8));
    }

    #[test]
    fn auto_variant_mid_k_big_n() {
        assert_eq!(auto_variant(4096, 16_384), (Variant::Mma2big, 1));
    }

    #[test]
    fn should_use_requires_flag() {
        std::env::remove_var("DFLASH_VERIFY_QMM");
        assert!(should_use_verify(16, 4, 64));
    }
}
