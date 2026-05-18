// Origin: CTOX
// License: Apache-2.0

//! Per-operator Rust dispatchers over the vendored MSL kernels, plus
//! pure-Rust quant block layouts and CPU correctness references.

pub mod q4_k;

#[cfg(feature = "metal")]
pub mod rms_norm;

#[cfg(feature = "metal")]
pub mod mul_mv_q4_k;

#[cfg(feature = "metal")]
pub mod mul_mv_q4_k_bench;

#[cfg(feature = "metal")]
pub mod mul_mm_q4_k;

#[cfg(feature = "metal")]
pub mod mul_mv_ext_q4_k;

#[cfg(feature = "metal")]
pub mod get_rows_q4_k;

pub mod moe_router;

#[cfg(feature = "metal")]
pub mod mul_mv_id_q4_k;

#[cfg(feature = "metal")]
pub mod gated_delta_net;

#[cfg(feature = "metal")]
pub mod ssm_conv;

#[cfg(feature = "metal")]
pub mod elementwise;

#[cfg(feature = "metal")]
pub mod rope;
