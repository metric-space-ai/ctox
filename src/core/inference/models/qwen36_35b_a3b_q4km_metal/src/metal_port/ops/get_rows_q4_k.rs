// Origin: CTOX
// License: AGPL-3.0-only

//! Rust dispatcher for the vendored `kernel_get_rows_q4_K` MSL kernel
//! — token-embedding lookup for Qwen3.6-35B-A3B (vocab=248320, hidden=2048).
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:9163-9192 (kernel)
//! ref: vendor/ggml-metal/ggml-metal-impl.h:920-932 (kargs_get_rows)
//! ref: vendor/ggml-metal/ggml-metal.metal:10049 (host_name)
//!
//! Dispatch: one threadgroup per (token, output-element-block) tuple.
//! Threadgroup size = 32, processes ne00t = ne00 / NWG=1 elements.
//!
//! For Qwen3.6 decode: src1 = `[N=1]` int32 token ID; output = `[N, 2048]`
//! f32 dequantized embedding row.

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

use crate::metal_port::ops::q4_k::{BlockQ4K, BLOCK_Q4_K_BYTES, QK_K};
use crate::metal_port::runtime::MetalRuntime;

#[repr(C)]
#[derive(Clone, Copy)]
struct KargsGetRows {
    ne00t: i32,
    ne00: i32,
    nb01: u64,
    nb02: u64,
    nb03: u64,
    ne10: i32,
    _pad: u32,
    nb10: u64,
    nb11: u64,
    nb12: u64,
    nb1: u64,
    nb2: u64,
    nb3: u64,
}

const _: () = assert!(size_of::<KargsGetRows>() == 88);

pub struct GetRowsQ4KKernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

impl GetRowsQ4KKernel {
    pub fn new(rt: &MetalRuntime) -> Result<Self> {
        let name = NSString::from_str("kernel_get_rows_q4_K");
        let func = rt
            .library
            .newFunctionWithName(&name)
            .ok_or_else(|| anyhow!("metallib has no function `kernel_get_rows_q4_K`"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction: {err:?}"))?;
        Ok(Self { pso })
    }
}

/// Look up `n_tokens` rows from a Q4_K embedding table of
/// shape `[vocab, hidden]` and dequantize each row into f32.
pub fn dispatch_get_rows_q4_k(
    rt: &MetalRuntime,
    kernel: &GetRowsQ4KKernel,
    embedding_q4k: &[BlockQ4K],
    token_ids: &[i32],
    vocab: usize,
    hidden: usize,
) -> Result<Vec<f32>> {
    if hidden % QK_K != 0 {
        return Err(anyhow!("hidden must be divisible by 256, got {hidden}"));
    }
    let blocks_per_row = hidden / QK_K;
    if embedding_q4k.len() != vocab * blocks_per_row {
        return Err(anyhow!("embedding size mismatch"));
    }

    let device = &rt.device;
    let opts = MTLResourceOptions::MTLResourceStorageModeShared;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;

    let emb_nn = NonNull::new(embedding_q4k.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("embedding ptr null"))?;
    let buf_emb = unsafe {
        device.newBufferWithBytes_length_options(emb_nn, vocab * row_bytes, opts)
    }
    .ok_or_else(|| anyhow!("buf_emb nil"))?;

    let ids_nn = NonNull::new(token_ids.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("token_ids ptr null"))?;
    let buf_ids = unsafe {
        device.newBufferWithBytes_length_options(
            ids_nn,
            token_ids.len() * size_of::<i32>(),
            opts,
        )
    }
    .ok_or_else(|| anyhow!("buf_ids nil"))?;

    let n_tokens = token_ids.len();
    let buf_dst = device
        .newBufferWithLength_options(n_tokens * hidden * size_of::<f32>(), opts)
        .ok_or_else(|| anyhow!("buf_dst nil"))?;

    let kargs = KargsGetRows {
        ne00t: hidden as i32,
        ne00: hidden as i32,
        nb01: row_bytes as u64,
        nb02: (vocab * row_bytes) as u64,
        nb03: (vocab * row_bytes) as u64,
        ne10: n_tokens as i32,
        _pad: 0,
        nb10: size_of::<i32>() as u64,
        nb11: (n_tokens * size_of::<i32>()) as u64,
        nb12: (n_tokens * size_of::<i32>()) as u64,
        nb1: (hidden * size_of::<f32>()) as u64,
        nb2: (n_tokens * hidden * size_of::<f32>()) as u64,
        nb3: (n_tokens * hidden * size_of::<f32>()) as u64,
    };

    let cmd = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer nil"))?;
    let enc = cmd
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("encoder nil"))?;
    enc.setComputePipelineState(&kernel.pso);

    let kargs_nn = NonNull::new(&kargs as *const KargsGetRows as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsGetRows>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_emb), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_ids), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_dst), 0, 3);
    }

    // Grid: tgpig.x = NWG=1 × ne10 tokens, tgpig.y/.z = batch (=1).
    // Threadgroup width = 32 (one simdgroup per row).
    let grid = MTLSize {
        width: n_tokens,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: 32,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    enc.endEncoding();
    cmd.commit();
    unsafe { cmd.waitUntilCompleted() };

    let mut out = vec![0.0f32; n_tokens * hidden];
    unsafe {
        let src = buf_dst.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), n_tokens * hidden);
    }
    Ok(out)
}

impl GetRowsQ4KKernel {
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// Record a get_rows dispatch into an existing encoder.
pub fn record_get_rows_q4_k(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &GetRowsQ4KKernel,
    embedding_buf: &ProtocolObject<dyn MTLBuffer>,
    ids_buf: &ProtocolObject<dyn MTLBuffer>,
    dst_buf: &ProtocolObject<dyn MTLBuffer>,
    vocab: usize,
    hidden: usize,
    n_tokens: usize,
) -> Result<()> {
    if hidden % QK_K != 0 {
        return Err(anyhow!("hidden must be divisible by 256"));
    }
    let blocks_per_row = hidden / QK_K;
    let row_bytes = blocks_per_row * BLOCK_Q4_K_BYTES;
    let kargs = KargsGetRows {
        ne00t: hidden as i32,
        ne00: hidden as i32,
        nb01: row_bytes as u64,
        nb02: (vocab * row_bytes) as u64,
        nb03: (vocab * row_bytes) as u64,
        ne10: n_tokens as i32,
        _pad: 0,
        nb10: size_of::<i32>() as u64,
        nb11: (n_tokens * size_of::<i32>()) as u64,
        nb12: (n_tokens * size_of::<i32>()) as u64,
        nb1: (hidden * size_of::<f32>()) as u64,
        nb2: (n_tokens * hidden * size_of::<f32>()) as u64,
        nb3: (n_tokens * hidden * size_of::<f32>()) as u64,
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsGetRows as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsGetRows>(), 0);
        enc.setBuffer_offset_atIndex(Some(embedding_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(ids_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(dst_buf), 0, 3);
    }
    let grid = MTLSize {
        width: n_tokens,
        height: 1,
        depth: 1,
    };
    let tg = MTLSize {
        width: 32,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(grid, tg);
    Ok(())
}
