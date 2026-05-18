//! Rust port of the concat-op dispatcher in
//! `vendor/ggml-cuda/concat.cu`.
//!
//! ref: vendor/ggml-cuda/concat.cu
//!
//! Concatenates two f32 tensors along a given axis. Upstream has
//! three fast-path kernels for contiguous dim=0/1/2, a
//! D2D-memcpy path for dim=3 (concat-along-batch on contiguous
//! input), and a templated non-contiguous kernel
//! `concat_f32_non_cont<dim>` for arbitrary stride layouts.
//!
//! Scope of the current port:
//!   • f32 contiguous paths for all four dims
//!       - dim ∈ {0,1,2}: launch `concat_f32_dim<0|1|2>` once per i3
//!       - dim == 3: pair of `cudaMemcpyAsync` D2D (no kernel)
//!   • f32 non-contiguous path — the `concat_f32_non_cont<dim>`
//!     template for all four dims
//!
//! Qwen3.5's forward uses dim=0 (channel concat in SSM state) and
//! dim=1 (head concat in attention output). dim 2/3 are not on the
//! hot path but the ports are trivial once dim=0 works.
//!
//! # Mangled-name handling
//!
//! `concat_f32_dim{0,1,2}` are `static __global__` functions (no
//! template args), so they get per-TU-hashed mangled names. The
//! non-cont kernel is a template on `int dim`, so four
//! instantiations live in the PTX.

use std::ffi::c_void;
use std::os::raw::c_int;

use crate::cuda_port::driver::{cuLaunchKernel, CUdeviceptr, CUfunction, CUresult, CUstream};

/// ref: vendor/ggml-cuda/concat.cuh:3
const CUDA_CONCAT_BLOCK_SIZE: c_int = 256;

#[derive(Default)]
pub struct ConcatKernels {
    pub dim0: CUfunction,
    pub dim1: CUfunction,
    pub dim2: CUfunction,
    /// Non-contiguous `concat_f32_non_cont<dim>` template
    /// instantiations.
    pub non_cont: [CUfunction; 4],
}

/// Mangled-name lookup for the contiguous fast-path kernels.
pub fn mangled_concat_dim(dim: i32) -> Result<&'static [u8], String> {
    let needle: &[u8] = match dim {
        0 => b"concat_f32_dim0",
        1 => b"concat_f32_dim1",
        2 => b"concat_f32_dim2",
        _ => return Err(format!("contiguous concat_f32_dim{dim} not defined")),
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::concat_entries::ENTRIES,
        &[needle],
    )
}

/// Mangled-name lookup for the non-contiguous template kernel.
/// `concat_f32_non_cont<dim>` with `dim ∈ {0,1,2,3}`.
pub fn mangled_concat_non_cont(dim: i32) -> Result<&'static [u8], String> {
    let add: &[u8] = match dim {
        0 => b"ILi0E",
        1 => b"ILi1E",
        2 => b"ILi2E",
        3 => b"ILi3E",
        _ => return Err(format!("non_cont dim={dim} out of range")),
    };
    crate::cuda_port::ptx::find_entry(
        crate::cuda_port::ptx::concat_entries::ENTRIES,
        &[b"concat_f32_non_cont", add],
    )
}

/// ref: vendor/ggml-cuda/concat.cu:82-94 — single kernel launch for
/// contiguous path per (i3, dim ∈ {0,1,2}).
fn launch_cont(
    func: CUfunction,
    x: CUdeviceptr,
    y: CUdeviceptr,
    dst: CUdeviceptr,
    ne0: i32,
    ne1: i32,
    ne2: i32,
    aux: i32, // ne00 for dim0, ne01 for dim1, ne02 for dim2
    stream: CUstream,
) -> CUresult {
    let num_blocks = ((ne0 + CUDA_CONCAT_BLOCK_SIZE - 1) / CUDA_CONCAT_BLOCK_SIZE) as u32;
    let grid_x = num_blocks;
    let grid_y = ne1 as u32;
    let grid_z = ne2 as u32;

    let x_val = x.0;
    let y_val = y.0;
    let d_val = dst.0;
    let args: [*const c_void; 5] = [
        &x_val as *const u64 as *const c_void,
        &y_val as *const u64 as *const c_void,
        &d_val as *const u64 as *const c_void,
        &ne0 as *const i32 as *const c_void,
        &aux as *const i32 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            grid_x,
            grid_y,
            grid_z,
            CUDA_CONCAT_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}

/// ref: vendor/ggml-cuda/concat.cu:157-190
///
/// Entry for the contiguous-src fast path, dim ∈ {0,1,2}. Upstream
/// loops over `i3` on the host and launches one kernel per batch
/// slice. `nb3_elems` is each tensor's stride along the outermost
/// axis **in elements** (upstream does `->nb[3] / 4`).
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_concat_f32_contiguous(
    kernels: &ConcatKernels,
    dim: i32,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    src0_ne: [i32; 4],
    dst_ne: [i32; 4],
    src0_nb3_elems: i64,
    src1_nb3_elems: i64,
    dst_nb3_elems: i64,
    stream: CUstream,
) -> CUresult {
    assert!(dim >= 0 && dim <= 2, "use the dim=3 path for dim==3");

    let func = match dim {
        0 => kernels.dim0,
        1 => kernels.dim1,
        2 => kernels.dim2,
        _ => unreachable!(),
    };
    let aux = match dim {
        0 => src0_ne[0],
        1 => src0_ne[1],
        _ => src0_ne[2],
    };

    for i3 in 0..dst_ne[3] as i64 {
        let x = CUdeviceptr(src0.0 + (i3 * src0_nb3_elems * 4) as u64);
        let y = CUdeviceptr(src1.0 + (i3 * src1_nb3_elems * 4) as u64);
        let d = CUdeviceptr(dst.0 + (i3 * dst_nb3_elems * 4) as u64);
        let rc = launch_cont(func, x, y, d, dst_ne[0], dst_ne[1], dst_ne[2], aux, stream);
        if rc != 0 {
            return rc;
        }
    }
    0
}

/// ref: vendor/ggml-cuda/concat.cu:192-220
///
/// Non-contiguous entry (any dim, any layout). `nb_*` are **byte**
/// strides — the kernel casts src/dst to `char*` and adds raw byte
/// offsets.
#[allow(clippy::too_many_arguments)]
pub fn ggml_cuda_op_concat_f32_non_contiguous(
    kernels: &ConcatKernels,
    dim: i32,
    src0: CUdeviceptr,
    src1: CUdeviceptr,
    dst: CUdeviceptr,
    src0_ne: [i64; 4],
    src0_nb: [u64; 4],
    src1_ne: [i64; 4],
    src1_nb: [u64; 4],
    dst_ne: [i64; 4],
    dst_nb: [u64; 4],
    stream: CUstream,
) -> CUresult {
    assert!((0..=3).contains(&dim), "dim out of range: {dim}");
    let func = kernels.non_cont[dim as usize];

    let grid_x = dst_ne[1] as u32;
    let grid_y = dst_ne[2] as u32;
    let grid_z = dst_ne[3] as u32;

    // Kernel signature (concat.cu:98-126): 27 args total —
    //   src0*, src1*, dst*,
    //   ne00..ne03 (4× int64),
    //   nb00..nb03 (4× uint64),
    //   ne10..ne13 (4× int64, unused),
    //   nb10..nb13 (4× uint64),
    //   ne0..ne3   (4× int64),
    //   nb0..nb3   (4× uint64)
    let s0 = src0.0;
    let s1 = src1.0;
    let d_val = dst.0;

    let args: [*const c_void; 27] = [
        &s0 as *const u64 as *const c_void,
        &s1 as *const u64 as *const c_void,
        &d_val as *const u64 as *const c_void,
        &src0_ne[0] as *const i64 as *const c_void,
        &src0_ne[1] as *const i64 as *const c_void,
        &src0_ne[2] as *const i64 as *const c_void,
        &src0_ne[3] as *const i64 as *const c_void,
        &src0_nb[0] as *const u64 as *const c_void,
        &src0_nb[1] as *const u64 as *const c_void,
        &src0_nb[2] as *const u64 as *const c_void,
        &src0_nb[3] as *const u64 as *const c_void,
        &src1_ne[0] as *const i64 as *const c_void,
        &src1_ne[1] as *const i64 as *const c_void,
        &src1_ne[2] as *const i64 as *const c_void,
        &src1_ne[3] as *const i64 as *const c_void,
        &src1_nb[0] as *const u64 as *const c_void,
        &src1_nb[1] as *const u64 as *const c_void,
        &src1_nb[2] as *const u64 as *const c_void,
        &src1_nb[3] as *const u64 as *const c_void,
        &dst_ne[0] as *const i64 as *const c_void,
        &dst_ne[1] as *const i64 as *const c_void,
        &dst_ne[2] as *const i64 as *const c_void,
        &dst_ne[3] as *const i64 as *const c_void,
        &dst_nb[0] as *const u64 as *const c_void,
        &dst_nb[1] as *const u64 as *const c_void,
        &dst_nb[2] as *const u64 as *const c_void,
        &dst_nb[3] as *const u64 as *const c_void,
    ];

    unsafe {
        cuLaunchKernel(
            func,
            grid_x,
            grid_y,
            grid_z,
            CUDA_CONCAT_BLOCK_SIZE as u32,
            1,
            1,
            0,
            stream,
            args.as_ptr(),
            std::ptr::null(),
        )
    }
}
