//! `quantize_q8_1` — host-side launcher for f32 → q8_1 activation quantization.
//!
//! Follows the same conventions as the other kernel wrappers: one PTX
//! blob, `OnceLock`-cached `CudaFunction`, shape validation up front, no
//! stream synchronization.
//!
//! The output buffer is carried as a `CudaTensor<i8>` — the kernel
//! writes half/half/int8[32] chunks, but the Rust side only cares that
//! the byte count matches `ceil(K / 32) * 36`. Callers that hand the
//! buffer to a subsequent `mmvq_q?k` kernel pass it by pointer alone
//! and let the kernel reinterpret.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

use crate::device::DeviceContext;
use crate::tensor::CudaTensor;

// PTX blob emitted by build.rs for kernels/quantize_q8_1.cu.
use super::QUANTIZE_Q8_1_PTX;

/// Per-process cache. See `rmsnorm.rs` multi-GPU caveat.
static QUANTIZE_Q8_1_F32_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Size of one packed q8_1 block on device (half d + half s + int8[32]).
pub const Q8_1_BLOCK_BYTES: usize = 36;
/// Elements per q8_1 block (QK8_1 in ggml).
pub const Q8_1_BLOCK_ELEMS: usize = 32;
/// Threads per CTA (matches the reference's CUDA_QUANTIZE_BLOCK_SIZE).
const QUANTIZE_BLOCK_SIZE: u32 = 256;

/// Bytes required to hold `k` f32 elements after q8_1 packing.
///
/// Panics in debug if `k` isn't a multiple of `Q8_1_BLOCK_ELEMS`; the
/// kernel itself tolerates a partial tail (lanes past `ne00` quantize
/// zero), but `CudaTensor<i8>::zeros(...)` needs the byte count the
/// kernel will actually write.
#[inline]
pub fn q8_1_packed_bytes(k: usize) -> usize {
    k.div_ceil(Q8_1_BLOCK_ELEMS) * Q8_1_BLOCK_BYTES
}

fn quantize_q8_1_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = QUANTIZE_Q8_1_F32_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(QUANTIZE_Q8_1_PTX)
        .map_err(|e| anyhow!("quantize_q8_1.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module quantize_q8_1.ptx: {:?}", e))?;
    let f = module
        .load_function("quantize_q8_1_f32")
        .map_err(|e| anyhow!("load_function quantize_q8_1_f32: {:?}", e))?;
    let _ = QUANTIZE_Q8_1_F32_FN.set(f.clone());
    Ok(f)
}

/// Quantize `x[K]` (f32) into `y_q8_1` (packed q8_1 bytes).
///
/// `y_q8_1` must be at least `q8_1_packed_bytes(k)` bytes in length.
/// `K` must be a multiple of 32 (QK8_1) — callers that need padding
/// quantize into a padded buffer and skip the tail at the consumer.
pub fn launch_quantize_q8_1_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y_q8_1: &mut CudaTensor<i8>,
    k: usize,
) -> Result<()> {
    if k == 0 {
        return Ok(());
    }
    if !k.is_multiple_of(Q8_1_BLOCK_ELEMS) {
        return Err(anyhow!(
            "quantize_q8_1: k must be a multiple of {} (got k={})",
            Q8_1_BLOCK_ELEMS,
            k
        ));
    }
    if x.numel() < k {
        return Err(anyhow!(
            "quantize_q8_1: x.numel()={} < k={}",
            x.numel(),
            k
        ));
    }
    let expected_bytes = q8_1_packed_bytes(k);
    if y_q8_1.numel() < expected_bytes {
        return Err(anyhow!(
            "quantize_q8_1: y_q8_1.numel()={} < required {} bytes",
            y_q8_1.numel(),
            expected_bytes
        ));
    }

    // One thread per element. Threads past ne00 quantize as zero.
    let grid_x = ((k as u32) + QUANTIZE_BLOCK_SIZE - 1) / QUANTIZE_BLOCK_SIZE;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (QUANTIZE_BLOCK_SIZE, 1, 1),
        shared_mem_bytes: 0,
    };

    let f = quantize_q8_1_fn(device)?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let ne00_i32 = x.numel().min(k) as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(y_q8_1.buf_mut())
        .arg(&k_i32)
        .arg(&ne00_i32);

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("quantize_q8_1_f32 launch (k={}): {:?}", k, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Dequantize a host-side packed q8_1 buffer back to f32 for diffing.
    fn dequant_q8_1_host(bytes: &[u8], k: usize) -> Vec<f32> {
        assert!(bytes.len() >= q8_1_packed_bytes(k));
        let mut out = vec![0.0f32; k];
        let blocks = k / Q8_1_BLOCK_ELEMS;
        for b in 0..blocks {
            let base = b * Q8_1_BLOCK_BYTES;
            let d = f16::from_bits(u16::from_le_bytes([bytes[base], bytes[base + 1]])).to_f32();
            // s is stored but we don't need it for dequant.
            for i in 0..Q8_1_BLOCK_ELEMS {
                let q = bytes[base + 4 + i] as i8;
                out[b * Q8_1_BLOCK_ELEMS + i] = (q as f32) * d;
            }
        }
        out
    }

    #[test]
    #[ignore]
    fn quantize_q8_1_roundtrip_matches_cpu() {
        let k = 4096usize;

        // Deterministic LCG — host-independent.
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };
        let x_host: Vec<f32> = (0..k).map(|_| rand_f()).collect();

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");
        let mut y_gpu = CudaTensor::<i8>::zeros(dev.clone(), vec![q8_1_packed_bytes(k)])
            .expect("alloc q8_1");

        launch_quantize_q8_1_f32(&dev, &x_gpu, &mut y_gpu, k).expect("launch");
        dev.synchronize().expect("sync");

        let y_host_i8 = y_gpu.to_host().expect("download q8_1");
        let y_host: Vec<u8> = y_host_i8.iter().map(|&b| b as u8).collect();
        let deq = dequant_q8_1_host(&y_host, k);

        // Per 32-elem block the quantization step is d = amax/127, so
        // per-element absolute error is bounded by d/2 ≤ amax/254. For
        // uniform-[-1, 1] input the per-block amax is close to 1, giving
        // a theoretical max abs of ~0.004. Check against max abs, not
        // max rel — values near zero have huge rel errors that are still
        // numerically correct.
        let mut max_abs = 0.0f32;
        let mut block_amax = vec![0.0f32; k / Q8_1_BLOCK_ELEMS];
        for b in 0..block_amax.len() {
            let mut m = 0.0f32;
            for i in 0..Q8_1_BLOCK_ELEMS {
                m = m.max(x_host[b * Q8_1_BLOCK_ELEMS + i].abs());
            }
            block_amax[b] = m;
        }
        for (i, (&src, &got)) in x_host.iter().zip(deq.iter()).enumerate() {
            let d = (src - got).abs();
            if d > max_abs {
                max_abs = d;
            }
            let allow = block_amax[i / Q8_1_BLOCK_ELEMS] / 127.0;
            assert!(
                d <= allow * 1.5 + 1e-6,
                "q8_1 element {}: |src-dq|={} > bound {} (src={}, dq={})",
                i,
                d,
                allow,
                src,
                got
            );
        }
        eprintln!("quantize_q8_1 diff: max_abs={:.6e}", max_abs);
    }
}
