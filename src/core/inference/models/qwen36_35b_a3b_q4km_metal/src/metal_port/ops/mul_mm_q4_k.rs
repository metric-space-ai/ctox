// Origin: CTOX
// License: Apache-2.0

//! Rust dispatcher for the vendored `kernel_mul_mm_q4_K_f32` MSL kernel
//! — the prefill batched matmul for Qwen3.6-35B-A3B Q4_K_M.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:9305-9431 (MSL kernel,
//!      `mpp::tensor_ops::matmul2d` path under GGML_METAL_HAS_TENSOR)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:432-451 (`ggml_metal_kargs_mul_mm`)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:94 (`#define FC_MUL_MM 700`)
//!
//! The vendored kernel uses the Metal-4 MetalPerformancePrimitives
//! matrix tensor API on devices with `has_tensor=true` (M3+/M4/M5
//! Apple9/10 family). On our M5 this is the **fast Apple-tensor-unit
//! path** — exactly the hardware accelerator our roofline data showed
//! is needed to push prefill throughput past `kernel_mul_mv_q4_K_f32`.
//!
//! Shapes computed:
//!   srcA = `[M, K]` Q4_K block-quantized   (weights)
//!   srcB = `[N, K]` f32 (col-major in GGML convention; row stride = K*4)
//!   dst  = `[N, M]` f32 (col-major; row stride = M*4)
//!
//! Threadgroup tile (from MSL constants):
//!   NRA = 64  (rows of A per tile)
//!   NRB = 128 (rows of B / cols of dst per tile)
//!   NUM_THREADS = 128
//!   shmem = NRA * N_MM_NK_TOTAL * sizeof(half) = 64 * 32 * 2 = 4096 bytes
//!
//! Grid = `(ceil(N / 128), ceil(M / 64), batch)`.

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

/// Mirror of `ggml_metal_kargs_mul_mm` in
/// `vendor/ggml-metal/ggml-metal-impl.h:432-451`.
///
/// Layout (with explicit padding for the C-struct alignment rules):
///   offset 0..4   ne00   (i32)
///   offset 4..8   ne02   (i32)
///   offset 8..16  nb01   (u64)
///   offset 16..24 nb02   (u64)
///   offset 24..32 nb03   (u64)
///   offset 32..36 ne12   (i32)
///   offset 36..40 padding
///   offset 40..48 nb10   (u64)
///   offset 48..56 nb11   (u64)
///   offset 56..64 nb12   (u64)
///   offset 64..72 nb13   (u64)
///   offset 72..76 ne0    (i32)
///   offset 76..80 ne1    (i32)
///   offset 80..82 r2     (i16)
///   offset 82..84 r3     (i16)
///   offset 84..88 trailing padding (struct alignment is 8 because of u64s,
///                                   so size rounds up to 88)
///   total: 88 bytes
#[repr(C)]
#[derive(Clone, Copy)]
struct KargsMulMm {
    ne00: i32,
    ne02: i32,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne12: i32,
    _pad: u32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb13: u64,
    ne0: i32,
    ne1: i32,
    r2: i16,
    r3: i16,
}

const _: () = assert!(size_of::<KargsMulMm>() == 88);

const FC_MUL_MM_BC_INP_INDEX: u64 = 700;
const FC_MUL_MM_BC_OUT_INDEX: u64 = 701;

/// Compiled pipeline. The bounds-check function constants are baked
/// in at pipeline creation. We currently always set both to `false`
/// because every Qwen3.6 shape we hit is a multiple of the kernel's
/// tile constants (M ∈ {2048, 4096} % 64 = 0, K ∈ {512, 2048, 4096} %
/// 32 = 0). Stage 5 may add bc-on candidates for shapes that emerge
/// from autotuning.
pub struct MulMmQ4KF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

impl MulMmQ4KF32Kernel {
    pub fn new(rt: &MetalRuntime) -> Result<Self> {
        let constants = MTLFunctionConstantValues::new();
        let bc_inp: bool = false;
        let bc_out: bool = false;
        let bc_inp_nn = NonNull::new(&bc_inp as *const bool as *mut c_void)
            .ok_or_else(|| anyhow!("bc_inp ptr null"))?;
        let bc_out_nn = NonNull::new(&bc_out as *const bool as *mut c_void)
            .ok_or_else(|| anyhow!("bc_out ptr null"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                bc_inp_nn,
                MTLDataType::Bool,
                FC_MUL_MM_BC_INP_INDEX as usize,
            );
            constants.setConstantValue_type_atIndex(
                bc_out_nn,
                MTLDataType::Bool,
                FC_MUL_MM_BC_OUT_INDEX as usize,
            );
        }

        let name = NSString::from_str("kernel_mul_mm_q4_K_f32");
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| {
                anyhow!("newFunctionWithName_constantValues kernel_mul_mm_q4_K_f32: {err:?}")
            })?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self { pso })
    }

    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// Record a `kernel_mul_mm_q4_K_f32` dispatch into an existing
/// compute encoder (Stage-4 chained command buffer pattern).
#[allow(clippy::too_many_arguments)]
pub fn record_mul_mm_q4_k_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &MulMmQ4KF32Kernel,
    weights_buf: &ProtocolObject<dyn MTLBuffer>,
    input_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    m: usize,
    k: usize,
    n: usize,
) -> Result<()> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256"));
    }
    let blocks_per_row = k / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;
    let kargs = KargsMulMm {
        ne00: k as i32,
        ne02: 1,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne12: 1,
        _pad: 0,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (n * k * size_of::<f32>()) as u64,
        nb13: (n * k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: n as i32,
        r2: 1,
        r3: 1,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsMulMm as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMm>(), 0);
        enc.setBuffer_offset_atIndex(Some(weights_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(input_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
        enc.setThreadgroupMemoryLength_atIndex(4096, 0);
    }
    const NRA: usize = 64;
    const NRB: usize = 128;
    const NUM_THREADS: usize = 128;
    let grid = MTLSize {
        width: (n + NRB - 1) / NRB,
        height: (m + NRA - 1) / NRA,
        depth: 1,
    };
    let tg = MTLSize {
        width: NUM_THREADS,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

/// Prefill matmul: weights `[m, k]` Q4_K × inputs `[n, k]` f32 →
/// output `[n, m]` f32 (column-major in GGML's convention).
pub fn dispatch_mul_mm_q4_k_f32(
    rt: &MetalRuntime,
    kernel: &MulMmQ4KF32Kernel,
    weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    m: usize,
    k: usize,
    n: usize,
) -> Result<Vec<f32>> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256, got {k}"));
    }
    let blocks_per_row = k / QK_K;
    if weights_q4k.len() != m * blocks_per_row {
        return Err(anyhow!(
            "weights len {} != m({m}) * blocks_per_row({blocks_per_row})",
            weights_q4k.len()
        ));
    }
    if input_f32.len() != n * k {
        return Err(anyhow!("input len {} != n({n}) * k({k})", input_f32.len()));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;

    let weights_nn = NonNull::new(weights_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("weights ptr null"))?;
    let buf_a = unsafe {
        device.newBufferWithBytes_length_options(weights_nn, total_weight_bytes, opts)
    }
    .ok_or_else(|| anyhow!("buf_a nil"))?;

    let input_nn = NonNull::new(input_f32.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("input ptr null"))?;
    let buf_b = unsafe {
        device.newBufferWithBytes_length_options(
            input_nn,
            input_f32.len() * size_of::<f32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("buf_b nil"))?;

    let out_bytes = n * m * size_of::<f32>();
    let buf_dst = device
        .newBufferWithLength_options(out_bytes, opts)
        .ok_or_else(|| anyhow!("buf_dst nil"))?;

    let kargs = KargsMulMm {
        ne00: k as i32,
        ne02: 1,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne12: 1,
        _pad: 0,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (n * k * size_of::<f32>()) as u64,
        nb13: (n * k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: n as i32,
        r2: 1,
        r3: 1,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsMulMm as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMm>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_a), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_b), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_dst), 0, 3);
        // Threadgroup memory: NRA × N_MM_NK_TOTAL × sizeof(half) = 64*32*2 = 4096 B.
        enc.setThreadgroupMemoryLength_atIndex(4096, 0);
    }

    // From MSL constants: NRA=64, NRB=128, NUM_THREADS=128.
    const NRA: usize = 64;
    const NRB: usize = 128;
    const NUM_THREADS: usize = 128;

    let grid = MTLSize {
        width: (n + NRB - 1) / NRB,
        height: (m + NRA - 1) / NRA,
        depth: 1,
    };
    let tg = MTLSize {
        width: NUM_THREADS,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    // dst is [n, m] column-major: row r ∈ [0, n), col c ∈ [0, m), index = r*m + c.
    let mut out = vec![0.0f32; n * m];
    unsafe {
        let src = buf_dst.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), n * m);
    }
    Ok(out)
}

/// CPU reference for the matmat path. Layout matches the MSL kernel:
/// `dst[r * m + c] = Σ_k weights[c, k] * input[r, k]` (= weights @ input.T).
pub fn cpu_reference_mul_mm_q4_k_f32(
    weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    m: usize,
    k: usize,
    n: usize,
) -> Vec<f32> {
    let dequant = crate::metal_port::ops::q4_k::dequantize_q4_k_to_f32(weights_q4k);
    debug_assert_eq!(dequant.len(), m * k);
    let mut out = vec![0.0f32; n * m];
    for r in 0..n {
        for c in 0..m {
            let mut acc = 0.0f64;
            for kk in 0..k {
                acc += dequant[c * k + kk] as f64 * input_f32[r * k + kk] as f64;
            }
            out[r * m + c] = acc as f32;
        }
    }
    out
}
