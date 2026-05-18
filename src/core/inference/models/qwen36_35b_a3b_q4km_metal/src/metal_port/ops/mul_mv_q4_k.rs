// Origin: CTOX
// License: Apache-2.0

//! Rust dispatcher for the vendored `kernel_mul_mv_q4_K_f32` MSL kernel
//! — the dominant decode kernel for Qwen3.6-35B-A3B Q4_K_M.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:7715-7833 (kernel + impl)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:451-473 (`ggml_metal_kargs_mul_mv`)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:51 (`#define N_R0_Q4_K 2`)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:93 (`#define FC_MUL_MV 600`)
//! ref: vendor/ggml-metal/ggml-metal.metal:3354 (`FC_mul_mv_nsg [[function_constant(FC_MUL_MV+0)]]`)
//!
//! Computes `dst = src0 · src1ᵀ` where:
//!   src0 (weights) = `[ne01 = M, ne00 = K]` Q4_K block-quantized
//!   src1 (input)   = `[ne11 = N, ne10 = K]` f32
//!   dst            = `[ne1 = N, ne0 = M]`   f32
//!
//! Matches the kernel's grid:
//!   threadgroup size = NSG × 32 (one simdgroup per group of 32 threads)
//!   grid = (M / (NSG × N_R0_Q4_K), N, batch)
//!   each simdgroup produces N_R0_Q4_K = 2 output rows of `dst`

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLComputeCommandEncoder,
    MTLComputePipelineState, MTLDataType, MTLDevice, MTLFunctionConstantValues, MTLLibrary,
    MTLResourceOptions, MTLSize,
};

use crate::metal_port::ops::q4_k::{BlockQ4K, BLOCK_Q4_K_BYTES, QK_K};
use crate::metal_port::runtime::MetalRuntime;

/// Mirror of `ggml_metal_kargs_mul_mv` in
/// `vendor/ggml-metal/ggml-metal-impl.h:453-473`.
///
/// The C struct has implicit 4-byte padding after each `int32_t`-triple
/// to align the following `uint64_t` field. We replicate that with
/// explicit `_pad*` fields and assert the total size at compile time.
#[repr(C)]
#[derive(Clone, Copy)]
struct KargsMulMv {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    _pad0: u32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne10: i32,
    ne11: i32,
    ne12: i32,
    _pad1: u32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb13: u64,
    ne0: i32,
    ne1: i32,
    nr0: i32,
    r2: i16,
    r3: i16,
}

const _: () = assert!(size_of::<KargsMulMv>() == 112);

/// `N_R0_Q4_K` — each simdgroup produces this many rows of dst.
const N_R0_Q4_K: u32 = 2;
/// `FC_MUL_MV + 0` — function-constant index of `FC_mul_mv_nsg`.
const FC_MUL_MV_NSG_INDEX: u64 = 600;

/// Compiled pipeline. The `nsg` (simdgroups per threadgroup) is baked
/// in at pipeline creation time via the function-constant binding;
/// changing `nsg` later requires creating a new kernel handle.
pub struct MulMvQ4KF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    /// Mirror of the function constant we set — the dispatcher uses
    /// it to size threadgroups.
    pub nsg: u32,
}

impl MulMvQ4KF32Kernel {
    /// Borrow the compiled pipeline state. `pub` so the Stage-4
    /// layer-block driver (in a separate bin target) can encode
    /// kernel calls into a chained command buffer with persistent
    /// buffers.
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

impl MulMvQ4KF32Kernel {
    /// `nsg` ∈ {1, 2, 4, 8}. Upstream's autotuned default for Q4_K
    /// matvec on Apple Silicon is typically 4; we accept it as a
    /// caller parameter so a future autotuner can sweep.
    pub fn new(rt: &MetalRuntime, nsg: u32) -> Result<Self> {
        if !(1..=16).contains(&nsg) {
            return Err(anyhow!("nsg must be in [1, 16], got {nsg}"));
        }
        let constants = unsafe { MTLFunctionConstantValues::new() };
        let nsg_short: i16 = nsg as i16;
        let nsg_nn = NonNull::new(&nsg_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("nsg pointer is null"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                nsg_nn,
                MTLDataType::Short,
                FC_MUL_MV_NSG_INDEX as usize,
            );
        }

        let name = NSString::from_str("kernel_mul_mv_q4_K_f32");
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| {
                anyhow!("newFunctionWithName_constantValues failed for kernel_mul_mv_q4_K_f32: {err:?}")
            })?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction failed: {err:?}"))?;
        Ok(Self { pso, nsg })
    }
}

/// Decode-time matvec: weights `[m, k]` Q4_K × input `[k]` f32 → `[m]` f32.
/// `weights_q4k` carries `(m × k / 256)` super-blocks of 144 bytes each,
/// laid out row-major (one row of M = `k/256` super-blocks).
pub fn dispatch_mul_mv_q4_k_f32_decode(
    rt: &MetalRuntime,
    kernel: &MulMvQ4KF32Kernel,
    weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    m: usize,
    k: usize,
) -> Result<Vec<f32>> {
    if k % QK_K != 0 {
        return Err(anyhow!(
            "Q4_K matvec requires k divisible by 256, got k={k}"
        ));
    }
    let blocks_per_row = k / QK_K;
    if weights_q4k.len() != m * blocks_per_row {
        return Err(anyhow!(
            "weights len {} != m({}) × blocks_per_row({})",
            weights_q4k.len(),
            m,
            blocks_per_row
        ));
    }
    if input_f32.len() != k {
        return Err(anyhow!("input len {} != k {}", input_f32.len(), k));
    }
    if m % (kernel.nsg as usize * N_R0_Q4_K as usize) != 0 {
        return Err(anyhow!(
            "m={} must be divisible by nsg({}) × N_R0_Q4_K({}) = {}",
            m,
            kernel.nsg,
            N_R0_Q4_K,
            kernel.nsg * N_R0_Q4_K
        ));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;

    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;
    let weights_nn = NonNull::new(weights_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("weights pointer is null"))?;
    let buf_w = unsafe {
        device.newBufferWithBytes_length_options(weights_nn, total_weight_bytes, opts)
    }
    .ok_or_else(|| anyhow!("newBufferWithBytes weights returned nil"))?;

    let input_nn = NonNull::new(input_f32.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("input pointer is null"))?;
    let buf_in = unsafe {
        device.newBufferWithBytes_length_options(
            input_nn,
            input_f32.len() * size_of::<f32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("newBufferWithBytes input returned nil"))?;

    let buf_out = device
        .newBufferWithLength_options(m * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("newBufferWithLength out returned nil"))?;

    let kargs = KargsMulMv {
        ne00: k as i32,
        ne01: m as i32,
        ne02: 1,
        _pad0: 0,
        nb00: 0, // unused: src0 is block-quantized; row stride dominates
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne10: k as i32,
        ne11: 1,
        ne12: 1,
        _pad1: 0,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (k * size_of::<f32>()) as u64,
        nb13: (k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: 1,
        nr0: 1,
        r2: 1,
        r3: 1,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer() returned nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("computeCommandEncoder() returned nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsMulMv as *mut c_void)
        .ok_or_else(|| anyhow!("kargs pointer is null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMv>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_w), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_in), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_out), 0, 3);
    }

    // Grid math from the kernel: r0 = tgpig.x, im = tgpig.z; each
    // simdgroup processes nr0 = N_R0_Q4_K rows; each threadgroup has
    // `nsg` simdgroups → `nsg × N_R0_Q4_K` rows per threadgroup.
    let rows_per_tg = kernel.nsg as usize * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1, // ne11 = 1 for matvec
        depth: 1,  // ne12 × ne13 = 1
    };
    let tg = MTLSize {
        width: kernel.nsg as usize * 32, // SIMD width = 32 on Apple GPUs
        height: 1,
        depth: 1,
    };

    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; m];
    unsafe {
        let src = buf_out.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), m);
    }
    Ok(out)
}

/// Record a `kernel_mul_mv_q4_K_f32` dispatch into an existing
/// compute encoder. Caller owns the buffer pool.
///
/// Layout matches `dispatch_mul_mv_q4_k_f32_decode`: weights are
/// `[m, k]` Q4_K row-major super-blocks, input is `[k]` f32, output
/// is `[m]` f32 written into `dst_buf`.
#[allow(clippy::too_many_arguments)]
pub fn record_mul_mv_q4_k_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &MulMvQ4KF32Kernel,
    weights_buf: &ProtocolObject<dyn MTLBuffer>,
    input_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    m: usize,
    k: usize,
) -> Result<()> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256, got {k}"));
    }
    if m % (kernel.nsg as usize * N_R0_Q4_K as usize) != 0 {
        return Err(anyhow!(
            "m={m} must be divisible by nsg({}) × N_R0_Q4_K({N_R0_Q4_K})",
            kernel.nsg
        ));
    }
    let blocks_per_row = k / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;
    let kargs = KargsMulMv {
        ne00: k as i32,
        ne01: m as i32,
        ne02: 1,
        _pad0: 0,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne10: k as i32,
        ne11: 1,
        ne12: 1,
        _pad1: 0,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (k * size_of::<f32>()) as u64,
        nb13: (k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: 1,
        nr0: 1,
        r2: 1,
        r3: 1,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsMulMv as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMv>(), 0);
        enc.setBuffer_offset_atIndex(Some(weights_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(input_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
    }
    let rows_per_tg = kernel.nsg as usize * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: kernel.nsg as usize * 32,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

/// CPU reference: dequantize all super-blocks to f32, then do the
/// matvec by hand. This is the byte-compare ground truth the
/// per-op verifier uses.
pub fn cpu_reference_mul_mv_q4_k_f32(
    weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    m: usize,
    k: usize,
) -> Vec<f32> {
    let dequant = crate::metal_port::ops::q4_k::dequantize_q4_k_to_f32(weights_q4k);
    debug_assert_eq!(dequant.len(), m * k);
    let mut out = vec![0.0f32; m];
    for row in 0..m {
        let mut acc = 0.0f64;
        for col in 0..k {
            acc += dequant[row * k + col] as f64 * input_f32[col] as f64;
        }
        out[row] = acc as f32;
    }
    out
}
