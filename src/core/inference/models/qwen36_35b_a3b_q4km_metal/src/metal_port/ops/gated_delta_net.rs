// Origin: CTOX
// License: Apache-2.0

//! Rust dispatcher for the vendored `kernel_gated_delta_net_f32_{1,2,4}`
//! — the linear-attention "dflash" block of Qwen3.6's hybrid layers.
//! 30 of 40 layers are linear-attention; this kernel runs once per
//! linear-attention layer per token (in decode) or per chunk in
//! prefill, internalising the recurrent scan over the time dim.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:2532-2647
//! ref: vendor/ggml-metal/ggml-metal-impl.h:854-890 (kargs_gated_delta_net)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:103   (FC_GATED_DELTA_NET = 1600)
//!
//! Inputs:
//!   q     [batch, num_q_heads,  n_tokens, S_v]  f32
//!   k     [batch, num_k_heads,  n_tokens, S_v]  f32 (S_v = head_dim)
//!   v     [batch, num_v_heads,  n_tokens, S_v]  f32
//!   g     [batch, num_v_heads,  n_tokens, G]    f32   (G = 1 for non-KDA, > 1 for KDA)
//!   b     [batch, num_v_heads,  n_tokens]       f32   (per-(b,h,t) scalar beta)
//!   s     [batch, num_v_heads,  S_v, S_v]       f32   (initial recurrent state)
//!
//! Output (concatenated):
//!   dst   [batch, num_v_heads,  n_tokens, S_v]  f32   (attention output)
//!   followed by:
//!         [batch, num_v_heads,  S_v, S_v]       f32   (final recurrent state)
//!
//! This module ships the dispatcher only — a CPU correctness reference
//! is provided as well so `gated_delta_net_verify` can byte-compare
//! the recurrent scan.

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

use crate::metal_port::runtime::MetalRuntime;

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsGatedDeltaNet {
    ne00: i32,
    ne01: i32,
    ne02: i32,
    ne03: i32,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne10: i32,
    ne11: i32,
    ne12: i32,
    ne13: i32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb13: u64,
    ne20: i32,
    ne21: i32,
    ne22: i32,
    ne23: i32,
    nb20: u64,
    nb21: u64,
    nb22: u64,
    nb23: u64,
    ns02: i32,
    ns12: i32,
    ns22: i32,
    ne0: i32,
    ne1: i32,
    ne2: i32,
    ne3: i32,
    nb0: u64,
    nb1: u64,
    nb2: u64,
    nb3: u64,
}

// Implicit 4-byte pad before nb0 to align to u64; total = 208.
const _: () = assert!(size_of::<KargsGatedDeltaNet>() == 208);

const FC_GATED_DELTA_NET_NE20_INDEX: u64 = 1600;
const FC_GATED_DELTA_NET_NE30_INDEX: u64 = 1601;

/// Pre-instantiated NSG variants the kernel ships:
/// `kernel_gated_delta_net_f32_1/_2/_4`.
#[derive(Clone, Copy, Debug)]
pub enum GatedDeltaNetNsg {
    N1,
    N2,
    N4,
}

impl GatedDeltaNetNsg {
    fn host_name(self) -> &'static str {
        match self {
            Self::N1 => "kernel_gated_delta_net_f32_1",
            Self::N2 => "kernel_gated_delta_net_f32_2",
            Self::N4 => "kernel_gated_delta_net_f32_4",
        }
    }
    fn nsg(self) -> u32 {
        match self {
            Self::N1 => 1,
            Self::N2 => 2,
            Self::N4 => 4,
        }
    }
}

pub struct GatedDeltaNetKernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub nsg: u32,
    pub s_v: u32,
    pub g: u32,
}

impl GatedDeltaNetKernel {
    /// `s_v` = `linear_value_head_dim` (= 128 for Qwen3.6).
    /// `g`   = number of gate groups (= 1 non-KDA, > 1 KDA). For
    ///         Qwen3.6 it's 1 per the Hugging Face implementation.
    pub fn new(rt: &MetalRuntime, nsg: GatedDeltaNetNsg, s_v: u32, g: u32) -> Result<Self> {
        if !(s_v as usize >= nsg.nsg() as usize && (s_v as usize) % (nsg.nsg() as usize) == 0) {
            return Err(anyhow!(
                "s_v={s_v} must be ≥ nsg={} and divisible by it",
                nsg.nsg()
            ));
        }
        let constants = MTLFunctionConstantValues::new();
        let s_v_short: i16 = s_v as i16;
        let g_short: i16 = g as i16;
        let s_v_nn = NonNull::new(&s_v_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("s_v ptr null"))?;
        let g_nn = NonNull::new(&g_short as *const i16 as *mut c_void)
            .ok_or_else(|| anyhow!("g ptr null"))?;
        unsafe {
            constants.setConstantValue_type_atIndex(
                s_v_nn,
                MTLDataType::Short,
                FC_GATED_DELTA_NET_NE20_INDEX as usize,
            );
            constants.setConstantValue_type_atIndex(
                g_nn,
                MTLDataType::Short,
                FC_GATED_DELTA_NET_NE30_INDEX as usize,
            );
        }

        let name = NSString::from_str(nsg.host_name());
        let func = rt
            .library
            .newFunctionWithName_constantValues_error(&name, &constants)
            .map_err(|err| anyhow!("newFunctionWithName_constantValues {}: {err:?}", nsg.host_name()))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self {
            pso,
            nsg: nsg.nsg(),
            s_v,
            g,
        })
    }
}

/// Single-step decode (n_tokens=1, batch=1) parameters.
pub struct GdnDecodeShape {
    pub num_q_heads: u32,
    pub num_k_heads: u32,
    pub num_v_heads: u32,
    pub n_tokens: u32,
    pub batch: u32,
}

/// Run the gated delta-net once. All input tensors are pre-projected
/// f32 (q/k/v come from the standard Q4_K matmuls upstream of this
/// dispatch, conv1d-passed). State `s_init` is `[batch, num_v_heads,
/// S_v, S_v]` f32.
pub fn dispatch_gated_delta_net(
    rt: &MetalRuntime,
    kernel: &GatedDeltaNetKernel,
    q_f32: &[f32],
    k_f32: &[f32],
    v_f32: &[f32],
    g_f32: &[f32],
    b_f32: &[f32],
    s_init_f32: &[f32],
    shape: &GdnDecodeShape,
) -> Result<Vec<f32>> {
    let s_v = kernel.s_v as usize;
    let g_groups = kernel.g as usize;
    let bsz = shape.batch as usize;
    let nq = shape.num_q_heads as usize;
    let nk = shape.num_k_heads as usize;
    let nv = shape.num_v_heads as usize;
    let nt = shape.n_tokens as usize;

    // Per the kernel, q is indexed by [batch, q_head, t, s_v] and is
    // assumed to share its S_v with the value dimension. The kernel
    // does `q_ptr = q + i23*nb03 + i01*nb01`, then walks `+= ns02`
    // per timestep.
    if q_f32.len() != bsz * nq * nt * s_v {
        return Err(anyhow!("q_f32 len mismatch: {} != {}*{}*{}*{}", q_f32.len(), bsz, nq, nt, s_v));
    }
    if k_f32.len() != bsz * nk * nt * s_v {
        return Err(anyhow!("k_f32 len mismatch"));
    }
    if v_f32.len() != bsz * nv * nt * s_v {
        return Err(anyhow!("v_f32 len mismatch"));
    }
    if g_f32.len() != bsz * nv * nt * g_groups {
        return Err(anyhow!("g_f32 len mismatch"));
    }
    if b_f32.len() != bsz * nv * nt {
        return Err(anyhow!("b_f32 len mismatch"));
    }
    if s_init_f32.len() != bsz * nv * s_v * s_v {
        return Err(anyhow!("s_init_f32 len mismatch"));
    }
    if s_v % kernel.nsg as usize != 0 {
        return Err(anyhow!("s_v={s_v} must be divisible by nsg={}", kernel.nsg));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;

    let make_buf = |data: &[f32]| -> Result<Retained<ProtocolObject<dyn MTLBuffer>>> {
        let nn = NonNull::new(data.as_ptr() as *mut c_void)
            .ok_or_else(|| anyhow!("ptr null"))?;
        unsafe {
            device.newBufferWithBytes_length_options(nn, data.len() * size_of::<f32>(), opts)
        }
        .ok_or_else(|| anyhow!("buffer nil"))
    };

    let buf_q = make_buf(q_f32)?;
    let buf_k = make_buf(k_f32)?;
    let buf_v = make_buf(v_f32)?;
    let buf_g = make_buf(g_f32)?;
    let buf_b = make_buf(b_f32)?;
    let buf_s = make_buf(s_init_f32)?;

    // dst layout: [batch * num_v_heads * n_tokens * S_v] (attention) +
    //             [batch * num_v_heads * S_v * S_v]      (final state)
    let dst_attn_elems = bsz * nv * nt * s_v;
    let dst_state_elems = bsz * nv * s_v * s_v;
    let dst_total = dst_attn_elems + dst_state_elems;
    let buf_dst = device
        .newBufferWithLength_options(dst_total * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("dst buf nil"))?;

    // kargs strides — one f32 = 4 bytes everywhere; layouts are
    // contiguous in C order [batch, head, t, dim] for q/k/v.
    let f32_b = size_of::<f32>() as u64;
    let q_stride_dim = f32_b;                           // nb00
    let q_stride_t = (s_v as u64) * f32_b;              // ns02 (advances per t)
    let q_stride_h = (nt as u64) * (s_v as u64) * f32_b; // nb01
    let q_stride_b = (nq as u64) * q_stride_h;          // nb03

    let k_stride_t = (s_v as u64) * f32_b;
    let k_stride_h = (nt as u64) * (s_v as u64) * f32_b;
    let k_stride_b = (nk as u64) * k_stride_h;

    let v_stride_t = (s_v as u64) * f32_b;
    let v_stride_h = (nt as u64) * (s_v as u64) * f32_b;
    let v_stride_b = (nv as u64) * v_stride_h;

    let kargs = KargsGatedDeltaNet {
        ne00: s_v as i32,
        ne01: nq as i32,
        ne02: nt as i32,
        ne03: bsz as i32,
        nb00: q_stride_dim,
        nb01: q_stride_h,
        nb02: q_stride_t,
        nb03: q_stride_b,
        ne10: s_v as i32,
        ne11: nk as i32,
        ne12: nt as i32,
        ne13: bsz as i32,
        nb10: f32_b,
        nb11: k_stride_h,
        nb12: k_stride_t,
        nb13: k_stride_b,
        ne20: s_v as i32,
        ne21: nv as i32,
        ne22: nt as i32,
        ne23: bsz as i32,
        nb20: f32_b,
        nb21: v_stride_h,
        nb22: v_stride_t,
        nb23: v_stride_b,
        // Per-timestep advance (ns02/ns12/ns22): the kernel adds these
        // to *_ptr each iteration of the t loop. They count *float
        // elements*, not bytes (the kernel does `q_ptr += args.ns02`
        // on a `float *`).
        ns02: s_v as i32,
        ns12: s_v as i32,
        ns22: s_v as i32,
        ne0: s_v as i32,
        ne1: nv as i32,
        ne2: nt as i32,
        ne3: bsz as i32,
        nb0: f32_b,
        nb1: (s_v as u64) * f32_b,
        nb2: (nv as u64) * (s_v as u64) * f32_b,
        nb3: (nt as u64) * (nv as u64) * (s_v as u64) * f32_b,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsGatedDeltaNet as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsGatedDeltaNet>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_q), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_k), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_v), 0, 3);
        enc.setBuffer_offset_atIndex(Some(&buf_g), 0, 4);
        enc.setBuffer_offset_atIndex(Some(&buf_b), 0, 5);
        enc.setBuffer_offset_atIndex(Some(&buf_s), 0, 6);
        enc.setBuffer_offset_atIndex(Some(&buf_dst), 0, 7);
    }

    // Grid (per kernel impl):
    //   tgpig.x = i20 / NSG  →  S_v / NSG  threadgroups in x
    //   tgpig.y = head index → ne21 (num_v_heads)
    //   tgpig.z = batch      → ne23
    // Threadgroup width = S_v / NSG threads × NSG (in y) = S_v threads.
    let grid = MTLSize {
        width: s_v / kernel.nsg as usize,
        height: nv,
        depth: bsz,
    };
    let tg = MTLSize {
        width: s_v / kernel.nsg as usize,
        height: kernel.nsg as usize,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; dst_total];
    unsafe {
        let src = buf_dst.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), dst_total);
    }
    Ok(out)
}

impl GatedDeltaNetKernel {
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// Record a gated_delta_net dispatch into an existing encoder
/// (Stage-4 chained pattern). One call per linear-attention layer
/// per token.
#[allow(clippy::too_many_arguments)]
pub fn record_gated_delta_net(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &GatedDeltaNetKernel,
    q_buf: &ProtocolObject<dyn MTLBuffer>,
    k_buf: &ProtocolObject<dyn MTLBuffer>,
    v_buf: &ProtocolObject<dyn MTLBuffer>,
    g_buf: &ProtocolObject<dyn MTLBuffer>,
    b_buf: &ProtocolObject<dyn MTLBuffer>,
    s_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    shape: &GdnDecodeShape,
) -> Result<()> {
    let s_v = kernel.s_v as usize;
    let bsz = shape.batch as usize;
    let nq = shape.num_q_heads as usize;
    let nk = shape.num_k_heads as usize;
    let nv = shape.num_v_heads as usize;
    let nt = shape.n_tokens as usize;

    if s_v % kernel.nsg as usize != 0 {
        return Err(anyhow!("s_v={s_v} must be divisible by nsg"));
    }
    let f32_b = size_of::<f32>() as u64;
    let q_stride_t = (s_v as u64) * f32_b;
    let q_stride_h = (nt as u64) * (s_v as u64) * f32_b;
    let q_stride_b = (nq as u64) * q_stride_h;
    let k_stride_t = (s_v as u64) * f32_b;
    let k_stride_h = (nt as u64) * (s_v as u64) * f32_b;
    let k_stride_b = (nk as u64) * k_stride_h;
    let v_stride_t = (s_v as u64) * f32_b;
    let v_stride_h = (nt as u64) * (s_v as u64) * f32_b;
    let v_stride_b = (nv as u64) * v_stride_h;

    let kargs = KargsGatedDeltaNet {
        ne00: s_v as i32,
        ne01: nq as i32,
        ne02: nt as i32,
        ne03: bsz as i32,
        nb00: f32_b,
        nb01: q_stride_h,
        nb02: q_stride_t,
        nb03: q_stride_b,
        ne10: s_v as i32,
        ne11: nk as i32,
        ne12: nt as i32,
        ne13: bsz as i32,
        nb10: f32_b,
        nb11: k_stride_h,
        nb12: k_stride_t,
        nb13: k_stride_b,
        ne20: s_v as i32,
        ne21: nv as i32,
        ne22: nt as i32,
        ne23: bsz as i32,
        nb20: f32_b,
        nb21: v_stride_h,
        nb22: v_stride_t,
        nb23: v_stride_b,
        ns02: s_v as i32,
        ns12: s_v as i32,
        ns22: s_v as i32,
        ne0: s_v as i32,
        ne1: nv as i32,
        ne2: nt as i32,
        ne3: bsz as i32,
        nb0: f32_b,
        nb1: (s_v as u64) * f32_b,
        nb2: (nv as u64) * (s_v as u64) * f32_b,
        nb3: (nt as u64) * (nv as u64) * (s_v as u64) * f32_b,
    };

    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsGatedDeltaNet as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsGatedDeltaNet>(), 0);
        enc.setBuffer_offset_atIndex(Some(q_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(k_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(v_buf), 0, 3);
        enc.setBuffer_offset_atIndex(Some(g_buf), 0, 4);
        enc.setBuffer_offset_atIndex(Some(b_buf), 0, 5);
        enc.setBuffer_offset_atIndex(Some(s_buf), 0, 6);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 7);
    }
    let grid = MTLSize {
        width: s_v / kernel.nsg as usize,
        height: nv,
        depth: bsz,
    };
    let tg = MTLSize {
        width: s_v / kernel.nsg as usize,
        height: kernel.nsg as usize,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

/// CPU reference for non-KDA (G=1) gated delta-net. Mirrors the
/// kernel's recurrent scan exactly so the verifier can byte-compare.
/// Returns concatenated `[attn_out, final_state]` matching the GPU's
/// dst layout.
pub fn cpu_reference_gated_delta_net_g1(
    q: &[f32],
    k: &[f32],
    v: &[f32],
    g: &[f32],
    b: &[f32],
    s_init: &[f32],
    s_v: usize,
    num_q_heads: usize,
    num_k_heads: usize,
    num_v_heads: usize,
    n_tokens: usize,
    batch: usize,
) -> Vec<f32> {
    let scale = 1.0_f32 / (s_v as f32).sqrt();
    let attn_size = batch * num_v_heads * n_tokens * s_v;
    let state_size = batch * num_v_heads * s_v * s_v;
    let mut out = vec![0.0f32; attn_size + state_size];
    let mut state = s_init.to_vec();

    for ib in 0..batch {
        for ih in 0..num_v_heads {
            // q, k heads use modular indexing per the kernel.
            let i01 = ih % num_q_heads;
            let i11 = ih % num_k_heads;
            // state offset for this (batch, head): [s_v, s_v]
            let state_off = (ib * num_v_heads + ih) * s_v * s_v;
            // q/k/v base offsets — q,k,v use [batch, head, t, s_v] layout
            let q_base = (ib * num_q_heads + i01) * n_tokens * s_v;
            let k_base = (ib * num_k_heads + i11) * n_tokens * s_v;
            let v_base = (ib * num_v_heads + ih) * n_tokens * s_v;
            // b and g use [batch, t, head, (G)] layout because the
            // kernel does `b_ptr += ne21` and `g_ptr += ne21 * G` per
            // t-step (= head_count, head-fastest within a token row).
            // The attention OUTPUT also uses [batch, t, head, s_v]
            // because the kernel writes
            // `dst[(i23*ne22*ne21 + t*ne21 + i21)*S_v + i20]`.

            for t in 0..n_tokens {
                // [batch, t, head] for b — head is the fastest axis.
                let g_idx = ib * n_tokens * num_v_heads + t * num_v_heads + ih;
                let b_idx = ib * n_tokens * num_v_heads + t * num_v_heads + ih;
                let g_exp = g[g_idx].exp();
                // For each row of state, multiply by g_exp.
                for is in 0..s_v * s_v {
                    state[state_off + is] *= g_exp;
                }
                // s_k[i20] = Σ_is state[i20, is] * k[is]
                let mut s_k = vec![0.0f32; s_v];
                for i20 in 0..s_v {
                    let mut sk = 0.0f32;
                    for is in 0..s_v {
                        sk += state[state_off + i20 * s_v + is] * k[k_base + t * s_v + is];
                    }
                    s_k[i20] = sk;
                }
                // d[i20] = (v[i20] - s_k[i20]) * b
                let beta = b[b_idx];
                let mut d = vec![0.0f32; s_v];
                for i20 in 0..s_v {
                    d[i20] = (v[v_base + t * s_v + i20] - s_k[i20]) * beta;
                }
                // state[i20, is] += k[is] * d[i20]
                for i20 in 0..s_v {
                    for is in 0..s_v {
                        state[state_off + i20 * s_v + is] += k[k_base + t * s_v + is] * d[i20];
                    }
                }
                // y[i20] = Σ_is state[i20, is] * q[is]
                for i20 in 0..s_v {
                    let mut y = 0.0f32;
                    for is in 0..s_v {
                        y += state[state_off + i20 * s_v + is] * q[q_base + t * s_v + is];
                    }
                    // dst layout: [batch, t, head, s_v] (kernel writes
                    // (i23*ne22*ne21 + t*ne21 + ih)*S_v + i20).
                    let dst_idx = ((ib * n_tokens + t) * num_v_heads + ih) * s_v + i20;
                    out[dst_idx] = y * scale;
                }
            }
        }
    }
    // Append final state.
    out[attn_size..].copy_from_slice(&state);
    out
}
