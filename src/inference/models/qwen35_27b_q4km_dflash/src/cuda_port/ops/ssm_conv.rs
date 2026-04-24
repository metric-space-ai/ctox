//! Rust port of the ssm-conv-op dispatcher in
//! `vendor/ggml-cuda/ssm-conv.cu`.
//!
//! ref: vendor/ggml-cuda/ssm-conv.cu
//!
//! Short 1-D convolution over the SSM state buffer. Used by
//! Qwen3.5's DeltaNet blocks. Upstream has three kernel families:
//!
//!   • `ssm_conv_f32<apply_silu, split_d_inner, d_conv>`
//!     — register-resident short-token path (n_t ≤ 32).
//!   • `ssm_conv_long_token_f32<apply_silu, split_d_inner, d_conv,
//!     split_n_t>` — shmem-tiled long-token path.
//!   • `ssm_conv_tree_f32<apply_silu, split_d_inner, d_conv>`
//!     — tree-mode (walks parent_ids[]). Used for beam / draft
//!     decoding in DFlash.
//!
//! Scope of the current port:
//!   • non-tree `ssm_conv_f32` + `ssm_conv_long_token_f32`
//!   • `d_conv ∈ {3, 4, 5, 9}` (the four sizes upstream permits)
//!   • `apply_silu ∈ {false, true}`
//!   • `split_d_inner = 128` (upstream constant)
//!   • `split_n_t = 32` (upstream constant)
//!
//! Total 16 kernel handles (2 paths × 4 d_conv × 2 silu). Tree mode
//! is wired into PTX but not resolved here — each tree variant is
//! a ~10-line addition when the graph-executor cutover needs them.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: ssm-conv.cu:200 (threads = 128 → split_d_inner template arg)
const SPLIT_D_INNER: i64 = 128;
/// ref: ssm-conv.cu:235 (split_n_t for the long-token path)
const SPLIT_N_T: i64 = 32;

/// Resolved kernel handles. Layout: `[path][silu][nc_index]` where
///   path    ∈ {0=short, 1=long_token}
///   silu    ∈ {0=no_silu, 1=apply_silu}
///   nc_idx  ∈ {0=d_conv_3, 1=d_conv_4, 2=d_conv_5, 3=d_conv_9}
#[derive(Default)]
pub struct SsmConvKernels {
    pub short_kernels: [[CUfunction; 4]; 2],
    pub long_kernels: [[CUfunction; 4]; 2],
}

fn nc_index(nc: i64) -> Result<usize, String> {
    match nc {
        3 => Ok(0),
        4 => Ok(1),
        5 => Ok(2),
        9 => Ok(3),
        other => Err(format!(
            "ssm_conv: unsupported d_conv={other} (upstream only builds 3/4/5/9)"
        )),
    }
}

/// Mangled-name needles for the four d_conv values (Itanium
/// encoding of `size_t` is `m`, so template arg `<128, d_conv>`
/// becomes `ILm128ELm<d_conv>E`).
fn nc_needle(nc: i64) -> &'static [u8] {
    match nc {
        3 => b"Lm128ELm3E",
        4 => b"Lm128ELm4E",
        5 => b"Lm128ELm5E",
        9 => b"Lm128ELm9E",
        _ => b"__invalid__",
    }
}

/// Resolve `ssm_conv_f32<apply_silu, 128, d_conv>`.
pub fn mangled_ssm_conv_short(apply_silu: bool, nc: i64) -> Result<&'static [u8], String> {
    let silu: &[u8] = if apply_silu { b"Lb1E" } else { b"Lb0E" };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::ssm_conv_entries::ENTRIES,
        // `11ssm_conv_f32` — the short-path kernel has length 11.
        // `ssm_conv_long_token_f32` (length 23) is excluded by this.
        &[b"11ssm_conv_f32", b"IL", silu, nc_needle(nc)],
    )
}

/// Resolve `ssm_conv_long_token_f32<apply_silu, 128, d_conv, 32>`.
pub fn mangled_ssm_conv_long(apply_silu: bool, nc: i64) -> Result<&'static [u8], String> {
    let silu: &[u8] = if apply_silu { b"Lb1E" } else { b"Lb0E" };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::ssm_conv_entries::ENTRIES,
        &[
            b"22ssm_conv_long_token_f32",
            silu,
            nc_needle(nc),
            b"Ll32E",
        ],
    )
}

/// Byte-stride bundle — upstream passes `int` byte strides straight
/// from the `ggml_tensor::nb` array. `nb_*` are always in bytes; the
/// kernel divides by `sizeof(float)` internally.
#[derive(Copy, Clone, Debug)]
pub struct SsmConvLayout {
    pub src0_nb0: c_int, // unused by kernel, passed for ABI compat
    pub src0_nb1: c_int,
    pub src0_nb2: c_int,
    pub src1_nb1: c_int,
    pub dst_nb0: c_int,
    pub dst_nb1: c_int,
    pub dst_nb2: c_int,
}

/// ref: vendor/ggml-cuda/ssm-conv.cu:220-250 (non-tree path)
///
/// Picks between the short-token kernel (n_t ≤ 32, register-
/// resident) and the long-token kernel (shmem-tiled) exactly like
/// upstream.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_ssm_conv_f32(
    kernels: &SsmConvKernels,
    apply_silu: bool,
    src0: CUdeviceptr, // conv_x
    src1: CUdeviceptr, // conv1d.weight
    dst: CUdeviceptr,
    layout: &SsmConvLayout,
    nc: i64,  // d_conv
    nr: i64,  // d_inner — upstream asserts nr % 128 == 0
    n_t: i64, // tokens per sequence
    n_s: i64, // number of sequences
    stream: CUstream,
) -> CUresult {
    let nc_idx = match nc_index(nc) {
        Ok(i) => i,
        Err(_) => return 1, // CUDA_ERROR_INVALID_VALUE
    };
    let silu_idx = apply_silu as usize;

    let threads = SPLIT_D_INNER as u32;
    if nr % SPLIT_D_INNER != 0 {
        // Upstream has GGML_ASSERT on this — surface it as an
        // error return rather than a panic.
        return 1;
    }
    let by = ((nr + SPLIT_D_INNER - 1) / SPLIT_D_INNER) as u32;

    // Kernel signature (ssm-conv.cu:9-13 for short, :54-58 for long):
    //   (const float * src0, const float * src1,
    //    int src0_nb0, int src0_nb1, int src0_nb2, int src1_nb1,
    //    float * dst, int dst_nb0, int dst_nb1, int dst_nb2,
    //    int64_t n_t)
    // = 2 ptrs + 4 strides + ptr + 3 strides + 1 int64 = 11 args.
    let src0_val = src0.0;
    let src1_val = src1.0;
    let dst_val = dst.0;
    let args: [*const c_void; 11] = [
        &src0_val as *const u64 as *const c_void,
        &src1_val as *const u64 as *const c_void,
        &layout.src0_nb0 as *const c_int as *const c_void,
        &layout.src0_nb1 as *const c_int as *const c_void,
        &layout.src0_nb2 as *const c_int as *const c_void,
        &layout.src1_nb1 as *const c_int as *const c_void,
        &dst_val as *const u64 as *const c_void,
        &layout.dst_nb0 as *const c_int as *const c_void,
        &layout.dst_nb1 as *const c_int as *const c_void,
        &layout.dst_nb2 as *const c_int as *const c_void,
        &n_t as *const i64 as *const c_void,
    ];

    if n_t <= SPLIT_N_T {
        // Short-token path — one block per (seq, d_inner-chunk).
        let func = kernels.short_kernels[silu_idx][nc_idx];
        unsafe {
            cuLaunchKernel(
                func,
                n_s as u32,
                by,
                1,
                threads,
                1,
                1,
                0, // no shmem
                stream,
                args.as_ptr(),
                std::ptr::null(),
            )
        }
    } else {
        // Long-token path — extra block dim over chunks of SPLIT_N_T.
        let func = kernels.long_kernels[silu_idx][nc_idx];
        let bz = ((n_t + SPLIT_N_T - 1) / SPLIT_N_T) as u32;
        let shmem = (threads as usize
            * (nc as usize - 1 + SPLIT_N_T as usize)
            * std::mem::size_of::<f32>()) as u32;
        unsafe {
            cuLaunchKernel(
                func,
                n_s as u32,
                by,
                bz,
                threads,
                1,
                1,
                shmem,
                stream,
                args.as_ptr(),
                std::ptr::null(),
            )
        }
    }
}
