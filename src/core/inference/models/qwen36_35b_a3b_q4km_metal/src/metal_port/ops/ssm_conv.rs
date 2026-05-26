// Origin: CTOX
// License: AGPL-3.0-only

//! Rust dispatcher for the vendored short causal conv1d kernel
//! `kernel_ssm_conv_f32_f32` — runs on q/k/v before the
//! gated_delta_net to apply the per-row 4-tap conv1d Mamba-2 uses.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:2076-2106 (impl)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:801-818 (kargs_ssm_conv)
//!
//! For Qwen3.6, conv kernel size = 4 (per `linear_conv_kernel_dim=4`
//! in the frozen ABI). One dispatch per row of the post-projection
//! q/k/v stream.
//!
//! Inputs:
//!   src0 = `[ne0=n_t, ne01=n_rows, ne02=batch]` f32 — projected Q/K/V
//!         (each row is a sliding window of the recurrent state +
//!          new token)
//!   src1 = `[ne10=conv_size=4, ne11=n_rows]` f32 — per-row conv weights
//! Output:
//!   dst  = `[ne0=1, ne1=n_rows, ne2=batch]` f32 — the conv'd output
//!          per row per token. For decode, ne0 is per-token (= 1).

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
    MTLComputePipelineState, MTLDevice, MTLLibrary, MTLResourceOptions, MTLSize,
};

use crate::metal_port::runtime::MetalRuntime;

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsSsmConv {
    ne00: i64,
    ne01: i64,
    ne02: i64,
    nb00: u64,
    nb01: u64,
    nb02: u64,
    ne10: i64,
    ne11: i64,
    nb10: u64,
    nb11: u64,
    ne0: i64,
    ne1: i64,
    ne2: i64,
    nb0: u64,
    nb1: u64,
    nb2: u64,
}

const _: () = assert!(size_of::<KargsSsmConv>() == 128);

pub struct SsmConvKernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    /// `true` = vec4 path (`kernel_ssm_conv_f32_f32_4`). Requires
    /// `conv_size % 4 == 0` (Qwen3.6 has 4, so vec4 fits).
    pub vec4: bool,
}

impl SsmConvKernel {
    pub fn new(rt: &MetalRuntime, vec4: bool) -> Result<Self> {
        let host_name = if vec4 {
            "kernel_ssm_conv_f32_f32_4"
        } else {
            "kernel_ssm_conv_f32_f32"
        };
        let name = NSString::from_str(host_name);
        let func = rt
            .library
            .newFunctionWithName(&name)
            .ok_or_else(|| anyhow!("metallib has no function `{host_name}`"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self { pso, vec4 })
    }
}

/// One conv1d step: each of `n_rows` rows convolves its `conv_size`
/// recent tokens with its weight vector, producing one output element.
///
/// `recent_tokens_f32` is shape `[batch, n_rows, conv_size]` — for
/// each (batch, row), the last `conv_size` token values laid out
/// contiguously. (For decode the caller maintains this rolling buffer
/// per layer.)
///
/// `conv_weights_f32` is shape `[n_rows, conv_size]` — per-row conv
/// kernel.
///
/// Returns `[batch, n_rows]` f32 — one output element per row per call.
pub fn dispatch_ssm_conv_f32(
    rt: &MetalRuntime,
    kernel: &SsmConvKernel,
    recent_tokens_f32: &[f32],
    conv_weights_f32: &[f32],
    n_rows: usize,
    conv_size: usize,
    batch: usize,
) -> Result<Vec<f32>> {
    if recent_tokens_f32.len() != batch * n_rows * conv_size {
        return Err(anyhow!(
            "recent len {} != batch({batch}) × n_rows({n_rows}) × conv_size({conv_size})",
            recent_tokens_f32.len()
        ));
    }
    if conv_weights_f32.len() != n_rows * conv_size {
        return Err(anyhow!(
            "weights len {} != n_rows({n_rows}) × conv_size({conv_size})",
            conv_weights_f32.len()
        ));
    }
    if kernel.vec4 && conv_size % 4 != 0 {
        return Err(anyhow!(
            "vec4 kernel requires conv_size divisible by 4, got {conv_size}"
        ));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;

    let make_buf = |data: &[f32]| -> Result<Retained<ProtocolObject<dyn MTLBuffer>>> {
        let nn = NonNull::new(data.as_ptr() as *mut c_void)
            .ok_or_else(|| anyhow!("ptr null"))?;
        unsafe {
            device.newBufferWithBytes_length_options(nn, data.len() * size_of::<f32>(), opts)
        }
        .ok_or_else(|| anyhow!("buf nil"))
    };

    let buf_src = make_buf(recent_tokens_f32)?;
    let buf_w = make_buf(conv_weights_f32)?;
    let buf_dst = device
        .newBufferWithLength_options(batch * n_rows * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("dst buf nil"))?;

    let f32_b = size_of::<f32>() as u64;
    let kargs = KargsSsmConv {
        // src0 = [conv_size, n_rows, batch] (each row's last conv_size tokens)
        ne00: conv_size as i64,
        ne01: n_rows as i64,
        ne02: batch as i64,
        nb00: f32_b,
        nb01: (conv_size as u64) * f32_b,
        nb02: (n_rows as u64) * (conv_size as u64) * f32_b,
        // src1 = conv weights [conv_size, n_rows]
        ne10: conv_size as i64,
        ne11: n_rows as i64,
        nb10: f32_b,
        nb11: (conv_size as u64) * f32_b,
        // dst = [1, n_rows, batch]
        ne0: 1,
        ne1: n_rows as i64,
        ne2: batch as i64,
        nb0: f32_b,
        nb1: f32_b,
        nb2: (n_rows as u64) * f32_b,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsSsmConv as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsSsmConv>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_src), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_w), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_dst), 0, 3);
    }

    // Grid: tgpig.x = ir (row), .y = i2 (batch dim 1, unused for our shape), .z = i3 (batch).
    // Per kernel: one thread per (row, batch) does the conv.
    let grid = MTLSize {
        width: n_rows,
        height: 1, // ne1=1 in dst (one output per token; we collapsed time dim)
        depth: batch,
    };
    let tg = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; batch * n_rows];
    unsafe {
        let src = buf_dst.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), batch * n_rows);
    }
    Ok(out)
}

impl SsmConvKernel {
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// Record an ssm_conv dispatch into an existing encoder.
#[allow(clippy::too_many_arguments)]
pub fn record_ssm_conv_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &SsmConvKernel,
    src_buf: &ProtocolObject<dyn MTLBuffer>,
    weights_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    n_rows: usize,
    conv_size: usize,
    batch: usize,
) -> Result<()> {
    if kernel.vec4 && conv_size % 4 != 0 {
        return Err(anyhow!("vec4 needs conv_size%4==0"));
    }
    let f32_b = size_of::<f32>() as u64;
    let kargs = KargsSsmConv {
        ne00: conv_size as i64,
        ne01: n_rows as i64,
        ne02: batch as i64,
        nb00: f32_b,
        nb01: (conv_size as u64) * f32_b,
        nb02: (n_rows as u64) * (conv_size as u64) * f32_b,
        ne10: conv_size as i64,
        ne11: n_rows as i64,
        nb10: f32_b,
        nb11: (conv_size as u64) * f32_b,
        ne0: 1,
        ne1: n_rows as i64,
        ne2: batch as i64,
        nb0: f32_b,
        nb1: f32_b,
        nb2: (n_rows as u64) * f32_b,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsSsmConv as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsSsmConv>(), 0);
        enc.setBuffer_offset_atIndex(Some(src_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(weights_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
    }
    let grid = MTLSize {
        width: n_rows,
        height: 1,
        depth: batch,
    };
    let tg = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}

/// CPU reference: `out[b, r] = Σ_i recent[b, r, i] * weights[r, i]`.
pub fn cpu_reference_ssm_conv_f32(
    recent: &[f32],
    weights: &[f32],
    n_rows: usize,
    conv_size: usize,
    batch: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; batch * n_rows];
    for ib in 0..batch {
        for ir in 0..n_rows {
            let mut sum = 0.0f32;
            for i in 0..conv_size {
                sum += recent[ib * n_rows * conv_size + ir * conv_size + i]
                    * weights[ir * conv_size + i];
            }
            out[ib * n_rows + ir] = sum;
        }
    }
    out
}
