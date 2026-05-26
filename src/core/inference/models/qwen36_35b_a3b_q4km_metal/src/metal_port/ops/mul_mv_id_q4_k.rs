// Origin: CTOX
// License: AGPL-3.0-only

//! Rust dispatcher for the vendored `kernel_mul_mv_id_q4_K_f32` —
//! the indexed Q4_K matvec that powers the MoE expert dispatch on
//! decode (top-k of 256 experts per token). Used together with the
//! Rust [`moe_router`](super::moe_router) which produces the index
//! buffer this kernel reads.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:10256-10320 (kernel_mul_mv_id)
//! ref: vendor/ggml-metal/ggml-metal.metal:10349 (host_name kernel_mul_mv_id_q4_K_f32)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:528-547 (kargs_mul_mv_id)
//!
//! Hot-path picture for Qwen3.6-35B-A3B decode (n_tokens=1, top-8 of 256):
//!   `src0s` = `[256, m, k]` Q4_K       all expert weights stacked
//!   `src1`  = `[1, 1, k]` f32           (broadcast across the 8 slots)
//!   `ids`   = `[1, 8]` int32            top-8 chosen expert IDs
//!   `dst`   = `[1, 8, m]` f32           per-slot expert output
//!
//! Caller weighted-sums the 8 outputs by the router weights.

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

/// Mirror of `ggml_metal_kargs_mul_mv_id` — Rust `#[repr(C)]` adds the
/// same implicit 4-byte pad between an i32 and the following u64 that
/// the C compiler does, so we don't list explicit `_pad*` fields.
/// Verified offsets:
///   nei0=0, nei1=4, nbi1=8, ne00=16, ne01=20, ne02=24,
///   nb00=32 (implicit 4B pad), nb01=40, nb02=48,
///   ne10=56, ne11=60, ne12=64, ne13=68,
///   nb10=72, nb11=80, nb12=88,
///   ne0=96, ne1=100, nb1=104, nr0=112,
///   trailing 4B pad to 120 (struct align 8).
#[repr(C)]
#[derive(Clone, Copy)]
struct KargsMulMvId {
    nei0: i32,
    nei1: i32,
    nbi1: u64,
    ne00: i32,
    ne01: i32,
    ne02: i32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    ne10: i32,
    ne11: i32,
    ne12: i32,
    ne13: i32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    ne0: i32,
    ne1: i32,
    nb1: u64,
    nr0: i32,
}

const _: () = assert!(size_of::<KargsMulMvId>() == 120);

const FC_MUL_MV_NSG_INDEX: u64 = 600;

/// Compiled pipeline. NSG and N_R0_Q4_K=2 (template-baked) match
/// what the impl uses internally — see `kernel_mul_mv_id` template
/// at line 10322 (it specialises `kernel_mul_mv_q4_K_f32_impl` with
/// `N_R0_Q4_K=2`).
pub struct MulMvIdQ4KF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub nsg: u32,
}

impl MulMvIdQ4KF32Kernel {
    pub fn new(rt: &MetalRuntime, nsg: u32) -> Result<Self> {
        if !(1..=16).contains(&nsg) {
            return Err(anyhow!("nsg must be in [1, 16], got {nsg}"));
        }
        let constants = MTLFunctionConstantValues::new();
        let nsg_short: i16 = nsg as i16;
        let nsg_nn = NonNull::new(&nsg_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("nsg ptr null"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                nsg_nn,
                MTLDataType::Short,
                FC_MUL_MV_NSG_INDEX as usize,
            );
        }
        let name = NSString::from_str("kernel_mul_mv_id_q4_K_f32");
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| anyhow!("newFunctionWithName_constantValues kernel_mul_mv_id_q4_K_f32: {err:?}"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self { pso, nsg })
    }
}

const N_R0_Q4_K: u32 = 2;

/// Indexed mat-vec for one MoE FFN matrix (e.g. gate, up, or down).
///
/// `expert_weights_q4k`: stacked `[n_experts, m, k]` Q4_K, row-major
/// in (n_experts, m). Each (expert, row) is a contiguous run of
/// `k/256` super-blocks.
///
/// `input_f32`: `[n_tokens, k]` f32 — the layer-input that the router
/// already applied to each token.
///
/// `expert_ids`: `[n_tokens, n_expert_used]` int32 — index into
/// `expert_weights_q4k`'s expert dim, produced by
/// [`super::moe_router::router_softmax_top_k`].
///
/// Returns `[n_tokens, n_expert_used, m]` f32 — the per-slot output
/// of every (token, slot) pair. Caller is responsible for the
/// weighted-sum across slots.
pub fn dispatch_mul_mv_id_q4_k_f32(
    rt: &MetalRuntime,
    kernel: &MulMvIdQ4KF32Kernel,
    expert_weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    expert_ids: &[i32],
    n_experts: usize,
    m: usize,
    k: usize,
    n_tokens: usize,
    n_expert_used: usize,
) -> Result<Vec<f32>> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256, got {k}"));
    }
    let blocks_per_row = k / QK_K;
    if expert_weights_q4k.len() != n_experts * m * blocks_per_row {
        return Err(anyhow!(
            "weights len {} != n_experts({n_experts}) × m({m}) × blocks_per_row({blocks_per_row})",
            expert_weights_q4k.len()
        ));
    }
    if input_f32.len() != n_tokens * k {
        return Err(anyhow!(
            "input len {} != n_tokens({n_tokens}) × k({k})",
            input_f32.len()
        ));
    }
    if expert_ids.len() != n_tokens * n_expert_used {
        return Err(anyhow!(
            "ids len {} != n_tokens({n_tokens}) × n_expert_used({n_expert_used})",
            expert_ids.len()
        ));
    }
    if m % (kernel.nsg as usize * N_R0_Q4_K as usize) != 0 {
        return Err(anyhow!(
            "m={m} must be divisible by nsg({}) × N_R0_Q4_K({N_R0_Q4_K})",
            kernel.nsg
        ));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;

    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let per_expert_bytes = m * row_bytes;

    let weights_nn = NonNull::new(expert_weights_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("weights ptr null"))?;
    let buf_w = unsafe {
        device.newBufferWithBytes_length_options(weights_nn, n_experts * per_expert_bytes, opts)
    }
    .ok_or_else(|| anyhow!("buf_w nil"))?;

    let input_nn = NonNull::new(input_f32.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("input ptr null"))?;
    let buf_in = unsafe {
        device.newBufferWithBytes_length_options(input_nn, n_tokens * k * size_of::<f32>(), opts)
    }
    .ok_or_else(|| anyhow!("buf_in nil"))?;

    let ids_nn = NonNull::new(expert_ids.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("ids ptr null"))?;
    let buf_ids = unsafe {
        device.newBufferWithBytes_length_options(
            ids_nn,
            expert_ids.len() * size_of::<i32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("buf_ids nil"))?;

    let out_elems = n_tokens * n_expert_used * m;
    let buf_out = device
        .newBufferWithLength_options(out_elems * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("buf_out nil"))?;

    let kargs = KargsMulMvId {
        nei0: n_expert_used as i32,
        nei1: n_tokens as i32,
        nbi1: (n_expert_used * size_of::<i32>()) as u64,
        ne00: k as i32,
        ne01: m as i32,
        ne02: n_experts as i32,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: per_expert_bytes as u64,
        ne10: k as i32,
        // ne11 = 1: src1 is [n_tokens, k] f32 (one row per token, no
        // per-slot replication). The kernel computes i11 = idx % ne11
        // and we want i11 = 0 always, since src1 has no slot dim.
        // Cross-token broadcast happens via tgpig.z = iid1*nei0 + idx.
        ne11: 1,
        ne12: n_tokens as i32,
        ne13: 1,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (n_tokens * k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: n_expert_used as i32,
        nb1: (m * size_of::<f32>()) as u64,
        nr0: N_R0_Q4_K as i32,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsMulMvId as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMvId>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_w), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_in), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_out), 0, 3);
        enc.setBuffer_offset_atIndex(Some(&buf_ids), 0, 4);
    }

    // Grid math from the kernel:
    //   tgpig.x = M-tile row index — `(m / (nsg × N_R0_Q4_K))` tiles
    //   tgpig.y = 1 (because args.ne11 is forced to 1 inside the wrapper)
    //   tgpig.z = iid1 × nei0 + idx, covering n_tokens × n_expert_used
    let rows_per_tg = kernel.nsg as usize * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1,
        depth: n_tokens * n_expert_used,
    };
    let tg = MTLSize {
        width: kernel.nsg as usize * 32,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; out_elems];
    unsafe {
        let src = buf_out.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), out_elems);
    }
    Ok(out)
}

/// Record an indexed-matvec dispatch into an existing encoder
/// (Stage-4 chained pattern). MoE-FFN per-layer dispatch chain
/// uses three of these (gate/up/down) per layer.
#[allow(clippy::too_many_arguments)]
pub fn record_mul_mv_id_q4_k_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &MulMvIdQ4KF32Kernel,
    weights_buf: &ProtocolObject<dyn MTLBuffer>,
    input_buf: &ProtocolObject<dyn MTLBuffer>,
    ids_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    n_experts: usize,
    m: usize,
    k: usize,
    n_tokens: usize,
    n_expert_used: usize,
) -> Result<()> {
    if k % QK_K != 0 {
        return Err(anyhow!("k must be divisible by 256"));
    }
    let blocks_per_row = k / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let per_expert_bytes = m * row_bytes;
    let kargs = KargsMulMvId {
        nei0: n_expert_used as i32,
        nei1: n_tokens as i32,
        nbi1: (n_expert_used * size_of::<i32>()) as u64,
        ne00: k as i32,
        ne01: m as i32,
        ne02: n_experts as i32,
        nb00: 0,
        nb01: row_bytes as u64,
        nb02: per_expert_bytes as u64,
        ne10: k as i32,
        ne11: 1,
        ne12: n_tokens as i32,
        ne13: 1,
        nb10: size_of::<f32>() as u64,
        nb11: (k * size_of::<f32>()) as u64,
        nb12: (n_tokens * k * size_of::<f32>()) as u64,
        ne0: m as i32,
        ne1: n_expert_used as i32,
        nb1: (m * size_of::<f32>()) as u64,
        nr0: N_R0_Q4_K as i32,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsMulMvId as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsMulMvId>(), 0);
        enc.setBuffer_offset_atIndex(Some(weights_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(input_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
        enc.setBuffer_offset_atIndex(Some(ids_buf), 0, 4);
    }
    let rows_per_tg = kernel.nsg as usize * N_R0_Q4_K as usize;
    let grid = MTLSize {
        width: m / rows_per_tg,
        height: 1,
        depth: n_tokens * n_expert_used,
    };
    let tg = MTLSize {
        width: kernel.nsg as usize * 32,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

impl MulMvIdQ4KF32Kernel {
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// CPU reference: dequantize the chosen experts and run a per-slot
/// matvec. Used by the verifier to byte-compare against the GPU
/// indexed kernel.
pub fn cpu_reference_mul_mv_id_q4_k_f32(
    expert_weights_q4k: &[BlockQ4K],
    input_f32: &[f32],
    expert_ids: &[i32],
    _n_experts: usize,
    m: usize,
    k: usize,
    n_tokens: usize,
    n_expert_used: usize,
) -> Vec<f32> {
    let blocks_per_row = k / QK_K;
    let per_expert_blocks = m * blocks_per_row;
    let mut out = vec![0.0f32; n_tokens * n_expert_used * m];
    for t in 0..n_tokens {
        for slot in 0..n_expert_used {
            let e = expert_ids[t * n_expert_used + slot] as usize;
            let exp_blocks =
                &expert_weights_q4k[e * per_expert_blocks..(e + 1) * per_expert_blocks];
            let dequant = crate::metal_port::ops::q4_k::dequantize_q4_k_to_f32(exp_blocks);
            let inp = &input_f32[t * k..(t + 1) * k];
            for row in 0..m {
                let mut acc = 0.0f64;
                for col in 0..k {
                    acc += dequant[row * k + col] as f64 * inp[col] as f64;
                }
                out[(t * n_expert_used + slot) * m + row] = acc as f32;
            }
        }
    }
    out
}
