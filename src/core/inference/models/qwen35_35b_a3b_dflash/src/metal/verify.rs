//! Verify-path specializations.
//!
//!   * `linear` — small-projection linear verify path. Ref:
//!     `dflash_mlx/verify_linear.py`.
//!   * `qmm`    — verify-specialized int4 quantized matmul (M=16
//!     simdgroup-MMA kernel, two shape-adaptive variants `mma2big` and
//!     `mma2big_pipe`). Ref: `dflash_mlx/verify_qmm.py`.
//!
//! # Status
//!
//! Skeleton. Populated in the "Port verify_linear + verify_qmm" todo
//! step.

pub mod linear;
pub mod qmm;
