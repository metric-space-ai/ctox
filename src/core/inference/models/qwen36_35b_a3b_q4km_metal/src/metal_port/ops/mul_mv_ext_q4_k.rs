// Origin: CTOX
// License: Apache-2.0

//! Rust dispatcher for the vendored row-batched matvec family
//! `kernel_mul_mv_ext_q4_K_f32_r1_{2,3,4,5}`.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:3763-3868 (impl)
//! ref: vendor/ggml-metal/ggml-metal.metal:3956-3959 (host_names)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:475-494 (kargs_mul_mv_ext)
//! ref: vendor/ggml-metal/ggml-metal.metal:3354-3355 (FC_mul_mv_nsg/nxpsg)
//!
//! These fill the **N=8..31 gap** between mul_mv (best at N=1-7) and
//! mul_mm (best at N≥32). Each dispatch processes `r1ptg` rows of
//! src1 simultaneously, sharing the weight stream — so per-token
//! bandwidth scales with N (up to r1ptg) without paying the matmat
//! tile-misfill cost at small N.
//!
//! Grid math (from kernel impl):
//!   width  = ceil(M / (nypsg * NSG))    where nypsg = 32 / nxpsg
//!   height = ceil(N / r1ptg)
//!   threadgroup = 32 * NSG

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

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsMulMvExt {
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
    r2: i16,
    r3: i16,
}

// Same accounting as KargsMulMv minus nr0 + trailing alignment pad to 8.
const _: () = assert!(size_of::<KargsMulMvExt>() == 112);

const FC_MUL_MV_NSG_INDEX: u64 = 600;
const FC_MUL_MV_NXPSG_INDEX: u64 = 601;

/// Compiled pipeline parameterised on row-batch size + simd-group + nxpsg.
pub struct MulMvExtQ4KF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub r1ptg: u32, // 2, 3, 4, or 5 — selects which host_name we built
    pub nsg: u32,
    pub nxpsg: u32,
}

impl MulMvExtQ4KF32Kernel {
    /// `r1ptg` ∈ {2, 3, 4, 5} (selects the host_name).
    /// `nsg` ∈ {1, 2, 4, 8}, `nxpsg` ∈ {2, 4, 8, 16, 32} with
    /// 32 % nxpsg == 0.
    pub fn new(rt: &MetalRuntime, r1ptg: u32, nsg: u32, nxpsg: u32) -> Result<Self> {
        if !(2..=5).contains(&r1ptg) {
            return Err(anyhow!("r1ptg must be in [2, 5], got {r1ptg}"));
        }
        if !(1..=16).contains(&nsg) {
            return Err(anyhow!("nsg must be in [1, 16], got {nsg}"));
        }
        if !matches!(nxpsg, 2 | 4 | 8 | 16 | 32) {
            return Err(anyhow!("nxpsg must be in {{2,4,8,16,32}}, got {nxpsg}"));
        }
        let constants = MTLFunctionConstantValues::new();
        let nsg_short: i16 = nsg as i16;
        let nxpsg_short: i16 = nxpsg as i16;
        let nsg_nn = NonNull::new(&nsg_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("nsg ptr null"))?;
        let nxpsg_nn = NonNull::new(&nxpsg_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("nxpsg ptr null"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                nsg_nn,
                MTLDataType::Short,
                FC_MUL_MV_NSG_INDEX as usize,
            );
            constants.setConstantValue_type_atIndex(
                nxpsg_nn,
                MTLDataType::Short,
                FC_MUL_MV_NXPSG_INDEX as usize,
            );
        }

        let host_name = format!("kernel_mul_mv_ext_q4_K_f32_r1_{r1ptg}");
        let name = NSString::from_str(&host_name);
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| anyhow!("newFunctionWithName_constantValues {host_name}: {err:?}"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self {
            pso,
            r1ptg,
            nsg,
            nxpsg,
        })
    }
}

/// `weights[m, k]` Q4_K × `input[n, k]` f32 → `out[n, m]` f32 (row-major).
/// `n` should be small (2..5 ideally to match r1ptg, but the kernel
/// handles arbitrary n by issuing ceil(n / r1ptg) tiles in the y-dim).
pub fn dispatch_mul_mv_ext_q4_k_f32(
    rt: &MetalRuntime,
    kernel: &MulMvExtQ4KF32Kernel,
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
        return Err(anyhow!("weights len mismatch"));
    }
    if input_f32.len() != n * k {
        return Err(anyhow!("input len mismatch"));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let total_weight_bytes = m * row_bytes;

    let weights_nn = NonNull::new(weights_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("weights ptr null"))?;
    let buf_w = unsafe {
        device.newBufferWithBytes_length_options(weights_nn, total_weight_bytes, opts)
    }
    .ok_or_else(|| anyhow!("buf_w nil"))?;

    let input_nn = NonNull::new(input_f32.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("input ptr null"))?;
    let buf_in = unsafe {
        device.newBufferWithBytes_length_options(
            input_nn,
            input_f32.len() * size_of::<f32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("buf_in nil"))?;

    let buf_out = device
        .newBufferWithLength_options(n * m * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("buf_out nil"))?;

    let kargs = KargsMulMvExt {
        ne00: k as i32,
        ne01: m as i32,
        ne02: 1,
        _pad0: 0,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne10: k as i32,
        ne11: n as i32,
        ne12: 1,
        _pad1: 0,
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

    let kargs_nn = NonNull::new(&kargs as *const KargsMulMvExt as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMvExt>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_w), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_in), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_out), 0, 3);
    }

    let nypsg = 32 / kernel.nxpsg as usize;
    let rows_per_tg = nypsg * kernel.nsg as usize;
    let grid = MTLSize {
        width: (m + rows_per_tg - 1) / rows_per_tg,
        height: (n + kernel.r1ptg as usize - 1) / kernel.r1ptg as usize,
        depth: 1,
    };
    let tg = MTLSize {
        width: 32 * kernel.nsg as usize,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; n * m];
    unsafe {
        let src = buf_out.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), n * m);
    }
    Ok(out)
}

/// Record a row-batched matvec dispatch into an existing encoder
/// (Stage-4 chained command buffer pattern).
#[allow(clippy::too_many_arguments)]
pub fn record_mul_mv_ext_q4_k_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &MulMvExtQ4KF32Kernel,
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
    let kargs = KargsMulMvExt {
        ne00: k as i32,
        ne01: m as i32,
        ne02: 1,
        _pad0: 0,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: total_weight_bytes as u64,
        nb03: total_weight_bytes as u64,
        ne10: k as i32,
        ne11: n as i32,
        ne12: 1,
        _pad1: 0,
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
    let kargs_nn = NonNull::new(&kargs as *const KargsMulMvExt as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMvExt>(), 0);
        enc.setBuffer_offset_atIndex(Some(weights_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(input_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
    }
    let nypsg = 32 / kernel.nxpsg as usize;
    let rows_per_tg = nypsg * kernel.nsg as usize;
    let grid = MTLSize {
        width: (m + rows_per_tg - 1) / rows_per_tg,
        height: (n + kernel.r1ptg as usize - 1) / kernel.r1ptg as usize,
        depth: 1,
    };
    let tg = MTLSize {
        width: 32 * kernel.nsg as usize,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

impl MulMvExtQ4KF32Kernel {
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}
