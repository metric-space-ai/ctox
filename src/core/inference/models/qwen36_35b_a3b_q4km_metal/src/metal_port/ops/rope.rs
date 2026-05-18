// Origin: CTOX
// License: Apache-2.0

//! M-RoPE dispatcher (`kernel_rope_multi_f32`) — applies multi-axis
//! rotary embeddings to Q or K projections in the full-attention block.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:4437-4518 (kernel_rope_multi)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:331+ (ggml_metal_kargs_rope)
//! ref: vendor/ggml-metal/ggml-metal.metal:4291  (FC_rope_is_imrope = FC_ROPE+0 = 800)
//!
//! Qwen3.6 settings (per QWEN36_35B_A3B_TEXT_CONFIG):
//!   n_dims         = head_dim * partial_rotary_factor = 256 * 0.25 = 64
//!   sect_0, sect_1, sect_2 = 11, 11, 10  (mrope_section)
//!   sect_3         = 0  (no temporal axis used in qwen3.6 text-only path)
//!   freq_base      = 10_000_000.0
//!   imrope         = true (mrope_interleaved)

#![cfg(feature = "metal")]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

use anyhow::{anyhow, Result};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLCommandEncoder, MTLComputeCommandEncoder, MTLComputePipelineState, MTLDataType,
    MTLDevice, MTLFunctionConstantValues, MTLLibrary, MTLSize,
};

use crate::metal_port::runtime::MetalRuntime;

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsRope {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    ne03: i32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne0: i32,
    ne1: i32,
    ne2: i32,
    ne3: i32,
    nb0: u64,
    nb1: u64,
    nb2: u64,
    nb3: u64,
    n_past: i32,
    n_dims: i32,
    n_ctx_orig: i32,
    freq_base: f32,
    freq_scale: f32,
    ext_factor: f32,
    attn_factor: f32,
    beta_fast: f32,
    beta_slow: f32,
    sect_0: i32,
    sect_1: i32,
    sect_2: i32,
    sect_3: i32,
    src2: bool,
}
// 4×i32 + 4×u64 + 4×i32 + 4×u64 + 3×i32 + 6×f32 + 4×i32 + bool
//  = 16 + 32 + 16 + 32 + 12 + 24 + 16 + 1 = 149
// Aligned to 8 → 152
const _: () = assert!(size_of::<KargsRope>() == 152);

const FC_ROPE_IS_IMROPE_INDEX: u64 = 800;

pub struct RopeMultiF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

impl RopeMultiF32Kernel {
    /// `imrope` true for Qwen3.6 (mrope_interleaved=true).
    pub fn new(rt: &MetalRuntime, imrope: bool) -> Result<Self> {
        let constants = MTLFunctionConstantValues::new();
        let imrope_b: bool = imrope;
        let nn = NonNull::new(&imrope_b as *const bool as *mut c_void)
            .ok_or_else(|| anyhow!("imrope nn"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                nn,
                MTLDataType::Bool,
                FC_ROPE_IS_IMROPE_INDEX as usize,
            );
        }
        let name = NSString::from_str("kernel_rope_multi_f32");
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| anyhow!("kernel_rope_multi_f32: {err:?}"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("pso: {err:?}"))?;
        Ok(Self { pso })
    }

    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

pub struct RopeShape {
    /// head_dim — total per-head dim (= 256 for Qwen3.6).
    pub head_dim: u32,
    pub n_heads: u32,
    pub n_tokens: u32,
    pub batch: u32,
    /// = head_dim * partial_rotary_factor (= 64 for Qwen3.6).
    pub n_dims_rotated: u32,
    /// mrope_section. For Qwen3.6: [11, 11, 10, 0].
    pub sect: [u32; 4],
    pub freq_base: f32,
}

/// Record an M-RoPE dispatch. `pos_buf` holds the per-token, per-axis
/// positions ([n_tokens, 4] int32 row-major). For decode this is just
/// the current sequence position replicated 4× (text axis only).
pub fn record_rope_multi_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &RopeMultiF32Kernel,
    src_buf: &ProtocolObject<dyn MTLBuffer>,
    pos_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    shape: &RopeShape,
) -> Result<()> {
    let head_dim = shape.head_dim as usize;
    let n_heads = shape.n_heads as usize;
    let n_tokens = shape.n_tokens as usize;
    let batch = shape.batch as usize;
    let f32_b = size_of::<f32>() as u64;

    // src0 is treated as [head_dim, n_heads, n_tokens, batch] f32 by the
    // kernel — see line 4502: src + i3*nb03 + i2*nb02 + i1*nb01 + ic*nb00.
    // Match my dispatchers: q/k buffers are [batch, n_heads, n_tokens, head_dim] row-major,
    // so:
    //   nb00 = sizeof(f32)
    //   nb01 = head_dim * 4              (per token)
    //   nb02 = n_heads * head_dim * 4   (per head row… wait, double check)
    //
    // The kernel tgpig.x = i1 (head idx), tgpig.y = i2 (token), tgpig.z = i3 (batch).
    // i01 maps to head, i02 to token, i03 to batch.
    // So nb01 = head_dim * 4 (per head), nb02 = n_heads * head_dim * 4 (per token),
    // nb03 = n_tokens * n_heads * head_dim * 4 (per batch).
    let kargs = KargsRope {
        ne00: head_dim as i32,
        ne01: n_heads as i32,
        ne02: n_tokens as i32,
        ne03: batch as i32,
        nb00: f32_b,
        nb01: (head_dim as u64) * f32_b,
        nb02: (n_heads as u64) * (head_dim as u64) * f32_b,
        nb03: (n_tokens as u64) * (n_heads as u64) * (head_dim as u64) * f32_b,
        ne0: head_dim as i32,
        ne1: n_heads as i32,
        ne2: n_tokens as i32,
        ne3: batch as i32,
        nb0: f32_b,
        nb1: (head_dim as u64) * f32_b,
        nb2: (n_heads as u64) * (head_dim as u64) * f32_b,
        nb3: (n_tokens as u64) * (n_heads as u64) * (head_dim as u64) * f32_b,
        n_past: 0,
        n_dims: shape.n_dims_rotated as i32,
        n_ctx_orig: 262_144, // Qwen3.6 max_position_embeddings
        freq_base: shape.freq_base,
        freq_scale: 1.0,
        ext_factor: 0.0,
        attn_factor: 1.0,
        beta_fast: 32.0,
        beta_slow: 1.0,
        sect_0: shape.sect[0] as i32,
        sect_1: shape.sect[1] as i32,
        sect_2: shape.sect[2] as i32,
        sect_3: shape.sect[3] as i32,
        src2: false, // no freq_factors
    };

    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsRope as *mut c_void)
        .ok_or_else(|| anyhow!("kargs nn"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsRope>(), 0);
        enc.setBuffer_offset_atIndex(Some(src_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(pos_buf), 0, 2);
        // src2 (freq_factors) is unused; the kernel guards via args.src2,
        // but the slot still needs SOMETHING bound. Re-bind src_buf as a
        // safe placeholder.
        enc.setBuffer_offset_atIndex(Some(src_buf), 0, 3);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 4);
    }
    // Grid: tgpig.x=i1=head, tgpig.y=i2=token, tgpig.z=i3=batch.
    // Threadgroup: tptg.x threads cover the head-dim loop (`for i0=2*tiitg; ...`).
    let tg_w = (head_dim / 2).max(1).min(512);
    let grid = MTLSize {
        width: n_heads,
        height: n_tokens,
        depth: batch,
    };
    let tg = MTLSize {
        width: tg_w,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}
