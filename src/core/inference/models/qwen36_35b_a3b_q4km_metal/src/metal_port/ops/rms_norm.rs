// Origin: CTOX
// License: Apache-2.0

//! Rust dispatcher for the vendored `kernel_rms_norm_f32` MSL kernel.
//!
//! ref: vendor/ggml-metal/ggml-metal.metal:2989-3055
//! ref: vendor/ggml-metal/ggml-metal-impl.h:551-564 (ggml_metal_kargs_norm)
//!
//! The MSL template `kernel_rms_norm_fuse_impl<T, F>` exposes 6
//! pre-instantiated entry points:
//!
//! ```text
//! F = 1   pure RMS norm:               y = x * (1 / sqrt(mean(x²) + eps))
//! F = 2   fused norm * weight gain:    y = x*scale * f0
//! F = 3   fused norm * gain + bias:    y = x*scale * f0 + f1
//! T = float | float4   (vector lane width)
//! ```
//!
//! Stage-2 first port is `kernel_rms_norm_f32` (T=float, F=1) — pure
//! RMS without fused gain. Qwen3.6 uses a learned weight gain in
//! every RMSNorm; the fused F=2 form will be added in a follow-up
//! commit, behind a candidate flag, so we can compare F=1+separate-mul
//! vs the fused form on this M5.

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

/// Mirror of `ggml_metal_kargs_norm` in
/// `vendor/ggml-metal/ggml-metal-impl.h:551-564`. Field order, types
/// and padding must match exactly — the MSL kernel reads it as a
/// `constant ggml_metal_kargs_norm &`.
#[repr(C)]
#[derive(Clone, Copy)]
struct KargsNorm {
    ne00: i32,
    ne00_t: i32,
    nb1: u64,
    nb2: u64,
    nb3: u64,
    eps: f32,
    nef1: [i32; 3],
    nef2: [i32; 3],
    nef3: [i32; 3],
    nbf1: [u64; 3],
    nbf2: [u64; 3],
    nbf3: [u64; 3],
}

/// CPU reference. Exact f32, used by the verifier.
pub fn rms_norm_f32_cpu(x: &[f32], eps: f32) -> Vec<f32> {
    let n = x.len();
    let mean: f64 = x.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / (n as f64);
    let scale = 1.0_f64 / (mean + eps as f64).sqrt();
    x.iter().map(|&v| ((v as f64) * scale) as f32).collect()
}

/// Compiled pipeline + cached pipeline-state-object. Created once per
/// runtime, reused across every dispatch.
pub struct RmsNormF32Kernel {
    pso: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
}

impl RmsNormF32Kernel {
    pub fn new(rt: &MetalRuntime) -> Result<Self> {
        let name = NSString::from_str("kernel_rms_norm_f32");
        let func = rt
            .library
            .newFunctionWithName(&name)
            .ok_or_else(|| anyhow!("metallib has no function `kernel_rms_norm_f32`"))?;
        let pso = rt
            .device
            .newComputePipelineStateWithFunction_error(&func)
            .map_err(|err| anyhow!("newComputePipelineStateWithFunction failed: {err:?}"))?;
        Ok(Self { pso })
    }

    /// Borrow pso for chained command-buffer encoding.
    pub fn pso_handle(&self) -> Retained<ProtocolObject<dyn MTLComputePipelineState>> {
        self.pso.clone()
    }
}

/// Record an rms_norm dispatch into an existing compute encoder
/// without committing. Used by the Stage-4 layer-block driver to
/// chain multiple kernel calls in one MTLCommandBuffer.
///
/// `x_buf` must contain the input `[rows × cols]` f32; output goes to
/// `y_buf`. Both must be ≥ rows*cols*sizeof(f32) bytes.
#[allow(clippy::too_many_arguments)]
pub fn record_rms_norm_f32(
    enc: &ProtocolObject<dyn MTLComputeCommandEncoder>,
    kernel: &RmsNormF32Kernel,
    device: &ProtocolObject<dyn MTLDevice>,
    x_buf: &ProtocolObject<dyn MTLBuffer>,
    y_buf: &ProtocolObject<dyn MTLBuffer>,
    rows: usize,
    cols: usize,
    eps: f32,
) -> Result<()> {
    use std::ptr::NonNull;
    let kargs = KargsNorm {
        ne00: cols as i32,
        ne00_t: cols as i32,
        nb1: (cols * size_of::<f32>()) as u64,
        nb2: (rows * cols * size_of::<f32>()) as u64,
        nb3: (rows * cols * size_of::<f32>()) as u64,
        eps,
        nef1: [rows as i32, 1, 1],
        nef2: [1, 1, 1],
        nef3: [1, 1, 1],
        nbf1: [(cols * size_of::<f32>()) as u64, 0, 0],
        nbf2: [(rows * cols * size_of::<f32>()) as u64, 0, 0],
        nbf3: [(rows * cols * size_of::<f32>()) as u64, 0, 0],
    };
    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsNorm as *mut c_void)
        .ok_or_else(|| anyhow!("kargs ptr null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsNorm>(), 0);
        enc.setBuffer_offset_atIndex(Some(x_buf), 0, 1);
        enc.setBuffer_offset_atIndex(Some(x_buf), 0, 2);
        enc.setBuffer_offset_atIndex(Some(x_buf), 0, 3);
        enc.setBuffer_offset_atIndex(Some(y_buf), 0, 4);
        enc.setThreadgroupMemoryLength_atIndex(32 * size_of::<f32>(), 0);
    }
    let max_threads = device.maxThreadsPerThreadgroup().width;
    let tg_width = max_threads.min(((cols + 31) / 32 * 32).max(32));
    let threads_per_threadgroup = MTLSize {
        width: tg_width,
        height: 1,
        depth: 1,
    };
    let threadgroups_per_grid = MTLSize {
        width: rows,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(threadgroups_per_grid, threads_per_threadgroup);
    Ok(())
}

/// Run `y = x / sqrt(mean(x²) + eps)` over a row-major `[rows, cols]`
/// f32 tensor. Each row is one independent normalization. Matches the
/// way Qwen3.6 RMSNorm is invoked in the forward graph (one row per
/// token).
pub fn dispatch_rms_norm_f32(
    rt: &MetalRuntime,
    kernel: &RmsNormF32Kernel,
    x: &[f32],
    rows: usize,
    cols: usize,
    eps: f32,
) -> Result<Vec<f32>> {
    if x.len() != rows * cols {
        return Err(anyhow!(
            "rms_norm input length {} does not match rows={} × cols={}",
            x.len(),
            rows,
            cols
        ));
    }

    let bytes_x = (x.len() * size_of::<f32>()) as u64;
    let device = &rt.device;

    let opts = MTLResourceOptions::MTLResourceStorageModeShared;
    let x_nn = NonNull::new(x.as_ptr() as *mut c_void)
        .ok_or_else(|| anyhow!("x slice pointer is null"))?;
    let buf_x = unsafe {
        device.newBufferWithBytes_length_options(x_nn, bytes_x as usize, opts)
    }
    .ok_or_else(|| anyhow!("device.newBufferWithBytes (x) returned nil"))?;
    let buf_y = device
        .newBufferWithLength_options(bytes_x as usize, opts)
        .ok_or_else(|| anyhow!("device.newBufferWithLength (y) returned nil"))?;

    // Layout we present to the MSL kernel for the [rows, cols] f32
    // input is "ne00 cols × ne01 rows × 1 × 1". src0 has row stride
    // `cols * 4` in dim 1; dims 2/3 are degenerate so any non-zero
    // stride works. For F=1 the kernel still computes pointers for
    // src1_0/src1_1 but never dereferences them — it does index a
    // `(i03 % nef3[1])` etc., though, so every nef* must be ≥ 1 or we
    // hit a modulo-by-zero. We bind buf_x as src1_0/src1_1 too so the
    // pointer arithmetic lands somewhere valid even on a hypothetical
    // future change that does dereference for F==1.
    let row_stride = (cols * size_of::<f32>()) as u64;
    let plane_stride = (rows * cols * size_of::<f32>()) as u64;
    let kargs = KargsNorm {
        ne00: cols as i32,
        // For T=float, ne00_t == ne00 (no float4 packing).
        ne00_t: cols as i32,
        nb1: row_stride,
        nb2: plane_stride,
        nb3: plane_stride,
        eps,
        // dim sizes per source — index [0]=src0, [1]=src1_0, [2]=src1_1.
        // Any value ≥ 1 is safe; 1 is the no-broadcast choice.
        nef1: [rows as i32, 1, 1],
        nef2: [1, 1, 1],
        nef3: [1, 1, 1],
        // dim byte-strides per source. For src0 the row stride in
        // dim 1 has to be cols*4 so the per-row pointer math lands
        // at the right place; deeper-dim strides are degenerate.
        // For src1_0/src1_1 the strides are 0 since F=1 never reads.
        nbf1: [row_stride, 0, 0],
        nbf2: [plane_stride, 0, 0],
        nbf3: [plane_stride, 0, 0],
    };

    let cmd_buf = rt
        .queue
        .commandBuffer()
        .ok_or_else(|| anyhow!("commandBuffer() returned nil"))?;
    let enc = cmd_buf
        .computeCommandEncoder()
        .ok_or_else(|| anyhow!("computeCommandEncoder() returned nil"))?;

    enc.setComputePipelineState(&kernel.pso);
    let kargs_nn = NonNull::new(&kargs as *const KargsNorm as *mut c_void)
        .ok_or_else(|| anyhow!("kargs stack pointer is null"))?;
    unsafe {
        enc.setBytes_length_atIndex(kargs_nn, size_of::<KargsNorm>(), 0);
        enc.setBuffer_offset_atIndex(Some(&buf_x), 0, 1);
        enc.setBuffer_offset_atIndex(Some(&buf_x), 0, 2);
        enc.setBuffer_offset_atIndex(Some(&buf_x), 0, 3);
        enc.setBuffer_offset_atIndex(Some(&buf_y), 0, 4);
        // shmem_f32 [[threadgroup(0)]] — sized to one float per simdgroup.
        // 32 simdgroups max per threadgroup is safe; allocating 32 floats
        // keeps the arithmetic in line with the kernel's `shmem_f32[sgitg]`
        // and `shmem_f32[tiisg]` indexing.
        enc.setThreadgroupMemoryLength_atIndex(32 * size_of::<f32>(), 0);
    }

    // Pick the threadgroup size: the kernel uses a parallel sum over
    // threads in the threadgroup with simd-level reduction. A
    // threadgroup width = SIMD width × num_simdgroups works as long
    // as `cols` is divisible by SIMD width and threadgroup ≤ device
    // max. We pick min(1024, 32 * ceil(cols/32)) clamped to
    // device.maxThreadsPerThreadgroup. For cols=2048 (Qwen3.6 hidden)
    // this gives 1024 threads = 32 simdgroups — close to optimal for
    // the M5 according to the qwen35-lessons handbook.
    let max_threads = device.maxThreadsPerThreadgroup().width;
    let tg_width = max_threads.min(((cols + 31) / 32 * 32).max(32));
    let threads_per_threadgroup = MTLSize {
        width: tg_width,
        height: 1,
        depth: 1,
    };
    let threadgroups_per_grid = MTLSize {
        width: rows,
        height: 1,
        depth: 1,
    };
    enc.dispatchThreadgroups_threadsPerThreadgroup(
        threadgroups_per_grid,
        threads_per_threadgroup,
    );
    enc.endEncoding();
    cmd_buf.commit();
    unsafe { cmd_buf.waitUntilCompleted() };

    // Copy back from shared-storage buffer.
    let mut out = vec![0.0f32; x.len()];
    unsafe {
        let src = buf_y.contents().as_ptr().cast::<f32>().cast_const();
        std::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), x.len());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_rms_norm_matches_reference_definition() {
        // Hand computation: x = [1, 2, 2, 4], n = 4
        // mean(x²) = (1 + 4 + 4 + 16) / 4 = 6.25
        // scale = 1 / sqrt(6.25 + 0) = 0.4
        // y = [0.4, 0.8, 0.8, 1.6]
        let y = rms_norm_f32_cpu(&[1.0, 2.0, 2.0, 4.0], 0.0);
        let want = [0.4, 0.8, 0.8, 1.6];
        for (got, w) in y.iter().zip(want.iter()) {
            assert!((got - w).abs() < 1e-6, "got {got} want {w}");
        }
    }

    #[test]
    fn cpu_rms_norm_with_eps() {
        // Tiny x so eps dominates.
        let y = rms_norm_f32_cpu(&[1e-6, 1e-6, 1e-6, 1e-6], 1.0);
        // mean(x²) ≈ 1e-12, eps = 1, scale ≈ 1
        for v in &y {
            assert!((v - 1e-6).abs() < 1e-6);
        }
    }
}
