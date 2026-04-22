//! IQ4_XS matrix-vector matmul (`mmvq`) — closes the last quant gap.
//!
//! Qwen3.5-27B ships the FFN `gate/up/down` projections as IQ4_XS
//! (136 bytes per 256-element block). The other quant variants
//! (Q4_K_M, Q5_K, Q6_K, Q8_0) already have mmvq paths; this file adds
//! the same shape for IQ4_XS so decode doesn't need a CPU-side
//! dequant to bf16 for every FFN forward.
//!
//! Two public entry points, mirroring the mmq_q4k pattern:
//!   * [`launch_mmvq_iq4_xs_q8_1_f32`] — hot path, caller has already
//!     pre-quantized `x` to q8_1 bytes (via
//!     [`super::quantize_q8_1::launch_quantize_q8_1_f32`]) and can
//!     reuse the buffer across fused FFN matmuls on the same
//!     activation (gate & up share `x`).
//!   * [`launch_mmvq_iq4_xs_f32`] — API-compatible entry that takes a
//!     raw `f32` `x` and does the q8_1 quantization internally into a
//!     scratch buffer before invoking the fast path. Same wrapping
//!     pattern as `mmq_q4k`.
//!
//! Matching f16-output variants are provided for both, for use when the
//! downstream op consumes half-precision activations.
//!
//! Correctness-first port: the kernel unpacks the q8_1 activation to
//! f32 inline and does a plain f32 inner product, same as the first-
//! pass Q5_K / Q6_K / Q8_0 ports. The DP4A / table-lookup hot path
//! (per ggml-cuda's `vec_dot_iq4_xs_q8_1`) is a follow-up once the
//! decode path is validated against the CPU golden.
//!
//! TODO: DP4A fast path using `get_int_from_table_16` + the 6-bit
//!       subblock scale to match the upstream mmvq throughput.
//! TODO: batched `mmq_iq4_xs` (mat-mat) path for prefill.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::f16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::quantize_q8_1::{launch_quantize_q8_1_f32, q8_1_packed_bytes, Q8_1_BLOCK_ELEMS};

// PTX blob emitted by build.rs for kernels/mmq_iq4_xs.cu.
use super::MMQ_IQ4_XS_PTX;

/// Per-process caches for the loaded kernel functions.
static MMVQ_IQ4_XS_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_IQ4_XS_F16_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_IQ4_XS_Q8_1_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_IQ4_XS_Q8_1_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per IQ4_XS block and logical elements per block (GGUF format):
/// `{ half d; uint16 scales_h; uint8 scales_l[4]; uint8 qs[128] }`.
const IQ4_XS_BLOCK_BYTES: usize = 136;
const IQ4_XS_BLOCK_ELEMS: usize = 256;

fn load_mmq_iq4_xs_fn(
    device: &Arc<DeviceContext>,
    cache: &'static OnceLock<CudaFunction>,
    sym: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(MMQ_IQ4_XS_PTX)
        .map_err(|e| anyhow!("mmq_iq4_xs.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module mmq_iq4_xs.ptx: {:?}", e))?;
    let f = module
        .load_function(sym)
        .map_err(|e| anyhow!("load_function {}: {:?}", sym, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate the IQ4_XS byte count and output dimensions. `k` must be a
/// whole multiple of 256 (the IQ4_XS block width).
fn validate_mmvq_iq4_xs_shapes<U>(
    a_iq4: &CudaTensor<i8>,
    k: usize,
    n: usize,
    y: &CudaTensor<U>,
) -> Result<()>
where
    U: ctox_cuda_primitives::tensor::TensorElem,
{
    if k == 0 || n == 0 {
        return Err(anyhow!(
            "mmvq_iq4_xs: k and n must be nonzero (k={}, n={})",
            k,
            n
        ));
    }
    if !k.is_multiple_of(IQ4_XS_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_iq4_xs: k must be a multiple of {} (got k={})",
            IQ4_XS_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / IQ4_XS_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * IQ4_XS_BLOCK_BYTES;
    if a_iq4.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_iq4_xs: a_iq4_xs byte count {} != (k/256)*n*136 = {} (k={}, n={})",
            a_iq4.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    if y.numel() != n {
        return Err(anyhow!("mmvq_iq4_xs: y.numel()={} != n={}", y.numel(), n));
    }
    Ok(())
}

fn validate_q8_1_x(x_q8_1: &CudaTensor<i8>, k: usize) -> Result<()> {
    let expected = q8_1_packed_bytes(k);
    if x_q8_1.numel() < expected {
        return Err(anyhow!(
            "mmvq_iq4_xs: x_q8_1 bytes {} < required {} for k={}",
            x_q8_1.numel(),
            expected,
            k
        ));
    }
    Ok(())
}

fn mmvq_launch_cfg(n: usize) -> LaunchConfig {
    // NCOLS_Y=2 output columns per block. Kernel guards the
    // out-of-range column when n is odd.
    let grid_x = n.div_ceil(2) as u32;
    LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    }
}

// ---- Hot path: pre-quantized q8_1 x --------------------------------------

/// `y[n] ← A_iq4_xs[n, k] · x[k]`, activation delivered as pre-packed q8_1.
///
/// `x_q8_1` must be at least `q8_1_packed_bytes(k)` bytes and must have
/// been produced by [`launch_quantize_q8_1_f32`] over the same `k`
/// elements. The fused Qwen3.5 FFN uses this to share one quantized
/// activation buffer across the gate and up projections.
pub fn launch_mmvq_iq4_xs_q8_1_f32(
    device: &Arc<DeviceContext>,
    a: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_q8_1: &CudaTensor<i8>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_iq4_xs_shapes(a, k, n, y)?;
    validate_q8_1_x(x_q8_1, k)?;

    let f = load_mmq_iq4_xs_fn(
        device,
        &MMVQ_IQ4_XS_Q8_1_F32_FN,
        "mmvq_iq4_xs_q8_1_f32_out",
    )?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x_q8_1.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(mmvq_launch_cfg(n)) }.map_err(|e| {
        anyhow!(
            "mmvq_iq4_xs_q8_1_f32 launch (k={} n={}): {:?}",
            k,
            n,
            e
        )
    })?;
    Ok(())
}

/// Same as the f32 variant but writes to an f16 output row.
pub fn launch_mmvq_iq4_xs_q8_1_f16(
    device: &Arc<DeviceContext>,
    a: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_q8_1: &CudaTensor<i8>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_iq4_xs_shapes(a, k, n, y)?;
    validate_q8_1_x(x_q8_1, k)?;

    let f = load_mmq_iq4_xs_fn(
        device,
        &MMVQ_IQ4_XS_Q8_1_F16_FN,
        "mmvq_iq4_xs_q8_1_f16_out",
    )?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x_q8_1.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(mmvq_launch_cfg(n)) }.map_err(|e| {
        anyhow!(
            "mmvq_iq4_xs_q8_1_f16 launch (k={} n={}): {:?}",
            k,
            n,
            e
        )
    })?;
    Ok(())
}

// ---- API-compatible entry: quantize x internally -------------------------

/// `y[n] ← A_iq4_xs[n, k] · x[k]`, all in f32 on the host contract.
///
/// Internally quantizes `x` to q8_1 in a scratch buffer, then invokes
/// the hot path. Callers that do repeated matmuls on the same `x`
/// (fused FFN gate + up) should pre-quantize once via
/// [`launch_quantize_q8_1_f32`] and call [`launch_mmvq_iq4_xs_q8_1_f32`]
/// directly to avoid the redundant quantize pass.
pub fn launch_mmvq_iq4_xs_f32(
    device: &Arc<DeviceContext>,
    a: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_iq4_xs_shapes(a, k, n, y)?;
    if x.numel() != k {
        return Err(anyhow!("mmvq_iq4_xs: x.numel()={} != k={}", x.numel(), k));
    }
    if !k.is_multiple_of(Q8_1_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_iq4_xs: k must be a multiple of {} (got k={})",
            Q8_1_BLOCK_ELEMS,
            k
        ));
    }

    let mut scratch = CudaTensor::<i8>::zeros(device.clone(), vec![q8_1_packed_bytes(k)])
        .map_err(|e| anyhow!("alloc q8_1 scratch: {:?}", e))?;
    launch_quantize_q8_1_f32(device, x, &mut scratch, k)?;
    launch_mmvq_iq4_xs_q8_1_f32(device, a, k, n, &scratch, y)
}

/// Same as the f32 variant but writes an f16 output row.
pub fn launch_mmvq_iq4_xs_f16(
    device: &Arc<DeviceContext>,
    a: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_iq4_xs_shapes(a, k, n, y)?;
    if x.numel() != k {
        return Err(anyhow!("mmvq_iq4_xs: x.numel()={} != k={}", x.numel(), k));
    }
    if !k.is_multiple_of(Q8_1_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_iq4_xs: k must be a multiple of {} (got k={})",
            Q8_1_BLOCK_ELEMS,
            k
        ));
    }

    let mut scratch = CudaTensor::<i8>::zeros(device.clone(), vec![q8_1_packed_bytes(k)])
        .map_err(|e| anyhow!("alloc q8_1 scratch: {:?}", e))?;
    launch_quantize_q8_1_f32(device, x, &mut scratch, k)?;
    launch_mmvq_iq4_xs_q8_1_f16(device, a, k, n, &scratch, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// IQ4_NL codebook — must match `kvalues_iq4nl` in ggml-common.h and
    /// the device-side `kvalues_iq4nl_dev` in kernels/sm_86/mmq_iq4_xs.cu.
    const IQ4_NL_KVALUES: [i8; 16] = [
        -127, -104, -83, -65, -49, -35, -22, -10,
           1,   13,  25,  38,  53,  69,  89, 113,
    ];

    /// Encode 256 f32 elements into a single 136-byte IQ4_XS block.
    ///
    /// Good-enough reference encoder for a round-trip test: we pick the
    /// per-sub-block 6-bit scale so `round(x / dl) + 32` lies in [0, 63],
    /// then index each quant into `kvalues_iq4nl` by nearest-neighbor.
    /// The block is then dequantized via the same math the kernel uses,
    /// so the CPU golden and GPU output agree modulo f32 rounding.
    fn encode_iq4_xs_block(vals: &[f32; 256], out: &mut [u8; 136]) {
        // Choose a super-scale `d` that lets every sub-block fit in
        // ls ∈ [-31, 31] (6-bit signed after `- 32`). We pick `d` from
        // the absmax across the whole block, then per-sub-block choose
        // `ls` to match that sub-block's own range.
        let mut absmax = 0.0f32;
        for &v in vals.iter() {
            if v.abs() > absmax {
                absmax = v.abs();
            }
        }
        // Codebook absmax is 127; d must be big enough that
        // ls ∈ [-31, 31] and codebook * dl covers absmax. Pick so that
        // at ls = 31 -> dl = 31 * d, codebook up to 127 -> max |val| = 31*127*d.
        // Target: max |val| ≤ absmax -> d ≥ absmax / (31 * 127).
        let d = (absmax / (31.0 * 127.0)).max(1e-8);
        let d_h = f16::from_f32(d).to_bits();
        let d_q = f16::from_bits(d_h).to_f32().max(1e-8);

        let mut scales_h: u16 = 0;
        let mut scales_l = [0u8; 4];
        let mut qs = [0u8; 128];

        for ib in 0..8 {
            let chunk = &vals[ib * 32..(ib + 1) * 32];
            let mut sub_absmax = 0.0f32;
            for &v in chunk.iter() {
                if v.abs() > sub_absmax {
                    sub_absmax = v.abs();
                }
            }
            // Choose ls so that dl = d_q * (ls - 32) covers this sub.
            // Codebook absmax = 127, so target dl = sub_absmax / 127.
            let target_dl = (sub_absmax / 127.0).max(1e-8);
            let ls_signed = (target_dl / d_q).round().clamp(-31.0, 31.0) as i32;
            let ls = (ls_signed + 32).clamp(0, 63) as u32;
            let ls_low = ls & 0x0F;
            let ls_high = (ls >> 4) & 0x03;

            // Pack scales_l[ib/2] (low nibble for even ib, high for odd).
            let pair = ib / 2;
            if ib % 2 == 0 {
                scales_l[pair] |= ls_low as u8;
            } else {
                scales_l[pair] |= (ls_low as u8) << 4;
            }
            // Pack scales_h: 2 bits per sub-block, starting at bit 2*ib.
            scales_h |= (ls_high as u16) << (2 * ib);

            let dl = d_q * (ls_signed as f32);
            // Nearest-neighbor codebook index per element.
            let mut codes = [0u8; 32];
            for i in 0..32 {
                let target = if dl.abs() < 1e-12 { 0.0 } else { chunk[i] / dl };
                let mut best = 0usize;
                let mut best_err = f32::INFINITY;
                for (cb_idx, &cb) in IQ4_NL_KVALUES.iter().enumerate() {
                    let err = (target - cb as f32).abs();
                    if err < best_err {
                        best_err = err;
                        best = cb_idx;
                    }
                }
                codes[i] = best as u8;
            }
            // Pack: qs[ib*16 + j] = codes[j] (low nibble) | codes[j+16] (high nibble).
            for j in 0..16 {
                qs[ib * 16 + j] = (codes[j] & 0x0F) | ((codes[j + 16] & 0x0F) << 4);
            }
        }

        // Write block header.
        out[0] = (d_h & 0xFF) as u8;
        out[1] = (d_h >> 8) as u8;
        out[2] = (scales_h & 0xFF) as u8;
        out[3] = (scales_h >> 8) as u8;
        out[4..8].copy_from_slice(&scales_l);
        out[8..136].copy_from_slice(&qs);
    }

    /// Reference CPU dequant — mirrors `dequant_iq4_xs_to_bf16` in
    /// `src/gguf_loader.rs` but keeps f32 so the unit test can measure
    /// bit-for-bit agreement with the kernel.
    fn dequant_iq4_xs_block(bytes: &[u8; 136], out: &mut [f32; 256]) {
        let d = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let scales_h = u16::from_le_bytes([bytes[2], bytes[3]]);
        let scales_l = &bytes[4..8];
        let qs = &bytes[8..136];

        for ib in 0..8 {
            let ls_low = (scales_l[ib / 2] >> (4 * (ib % 2))) & 0x0F;
            let ls_high = ((scales_h >> (2 * ib)) & 0x03) as u8;
            let ls = (ls_low | (ls_high << 4)) as i32;
            let dl = d * ((ls - 32) as f32);
            let qs_ib = &qs[ib * 16..(ib + 1) * 16];
            for j in 0..16 {
                let lo = (qs_ib[j] & 0x0F) as usize;
                let hi = ((qs_ib[j] >> 4) & 0x0F) as usize;
                out[ib * 32 + j] = dl * (IQ4_NL_KVALUES[lo] as f32);
                out[ib * 32 + j + 16] = dl * (IQ4_NL_KVALUES[hi] as f32);
            }
        }
    }

    fn build_test_matrix(
        n: usize,
        k: usize,
    ) -> (Vec<u8>, Vec<f32>, Vec<f32>, Vec<f32>) {
        let blocks_per_col = k / IQ4_XS_BLOCK_ELEMS;
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let total_bytes = n * blocks_per_col * IQ4_XS_BLOCK_BYTES;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                let mut vals = [0.0f32; 256];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5;
                }
                let mut block = [0u8; 136];
                encode_iq4_xs_block(&vals, &mut block);
                let mut deq = [0.0f32; 256];
                dequant_iq4_xs_block(&block, &mut deq);
                let base = (col * blocks_per_col + b) * IQ4_XS_BLOCK_BYTES;
                a_bytes[base..base + IQ4_XS_BLOCK_BYTES].copy_from_slice(&block);
                a_deq[col * k + b * 256..col * k + (b + 1) * 256].copy_from_slice(&deq);
            }
        }

        let x_host: Vec<f32> = (0..k).map(|_| rand_f()).collect();

        // CPU golden: y[col] = sum_i a_deq[col, i] * x[i].
        let mut y_cpu = vec![0.0f32; n];
        for col in 0..n {
            let mut acc = 0.0f32;
            for i in 0..k {
                acc += a_deq[col * k + i] * x_host[i];
            }
            y_cpu[col] = acc;
        }

        (a_bytes, a_deq, x_host, y_cpu)
    }

    /// Dequantize a host-side packed q8_1 buffer back to f32.
    ///
    /// Used to build a "post-q8_1" CPU golden: both kernel entry points
    /// consume `x` after q8_1 quantization (the internal-quant path
    /// quantizes on-device, the pre-quant path takes an already-packed
    /// buffer), so the fair CPU reference is `a_deq · dequant_q8_1(x)`,
    /// not `a_deq · x_original`. This isolates the kernel's own error
    /// from the unavoidable q8_1 rounding on the activation (which is
    /// validated separately by the quantize_q8_1 test).
    fn dequant_q8_1_host(bytes: &[u8], k: usize) -> Vec<f32> {
        const Q8_1_ELEMS: usize = 32;
        const Q8_1_BYTES: usize = 36;
        assert!(bytes.len() >= (k / Q8_1_ELEMS) * Q8_1_BYTES);
        let mut out = vec![0.0f32; k];
        let blocks = k / Q8_1_ELEMS;
        for b in 0..blocks {
            let base = b * Q8_1_BYTES;
            let d = f16::from_bits(u16::from_le_bytes([bytes[base], bytes[base + 1]])).to_f32();
            for i in 0..Q8_1_ELEMS {
                let q = bytes[base + 4 + i] as i8;
                out[b * Q8_1_ELEMS + i] = (q as f32) * d;
            }
        }
        out
    }

    /// End-to-end integration test against a CPU golden. Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture mmvq_iq4_xs_vs_cpu_golden
    ///
    /// Golden construction: the kernel consumes `x` after q8_1
    /// quantization (either applied internally or supplied pre-quantized
    /// by the caller). To measure the kernel's own precision and not
    /// the q8_1 activation rounding (a separate, validated op), we
    /// first quantize `x` on-device, dequantize it back to f32 on the
    /// host, and compute `y_cpu = a_deq · dequant_q8_1(x)`. Both GPU
    /// paths then match this golden to within f32 epsilon.
    #[test]
    #[ignore]
    fn mmvq_iq4_xs_vs_cpu_golden() {
        let k = 4096usize;
        let n = 256usize;
        let (a_bytes, a_deq, x_host, _y_cpu_exact) = build_test_matrix(n, k);

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let a_i8: Vec<i8> = a_bytes.iter().map(|&b| b as i8).collect();
        let a_gpu = CudaTensor::<i8>::from_host(dev.clone(), vec![a_i8.len()], &a_i8)
            .expect("upload a_iq4_xs");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");

        // Quantize once and build a matching CPU golden from the
        // dequantized q8_1 buffer. Both GPU paths see this same
        // quantized activation at the kernel boundary.
        let mut x_q8_1 = CudaTensor::<i8>::zeros(dev.clone(), vec![q8_1_packed_bytes(k)])
            .expect("alloc q8_1 scratch");
        launch_quantize_q8_1_f32(&dev, &x_gpu, &mut x_q8_1, k).expect("launch quantize_q8_1");
        dev.synchronize().expect("sync quantize");
        let x_q8_1_host: Vec<i8> = x_q8_1.to_host().expect("download q8_1");
        let x_q8_1_bytes: Vec<u8> = x_q8_1_host.iter().map(|&v| v as u8).collect();
        let x_deq = dequant_q8_1_host(&x_q8_1_bytes, k);

        let mut y_cpu = vec![0.0f32; n];
        for col in 0..n {
            let mut acc = 0.0f32;
            for i in 0..k {
                acc += a_deq[col * k + i] * x_deq[i];
            }
            y_cpu[col] = acc;
        }

        // Pass 1: API-compat path (internal quantize).
        {
            let mut y_gpu =
                CudaTensor::<f32>::zeros(dev.clone(), vec![n]).expect("alloc y");
            launch_mmvq_iq4_xs_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu)
                .expect("launch internal-quant path");
            dev.synchronize().expect("sync");

            let y_host = y_gpu.to_host().expect("download y");
            let (max_abs, max_rel) = diff(&y_cpu, &y_host);
            eprintln!(
                "[internal-quant] mmvq_iq4_xs diff: max_abs={:.6e} max_rel={:.6e}",
                max_abs, max_rel
            );
            assert!(
                max_rel < 1e-2,
                "internal-quant path diverges: max_rel={}",
                max_rel
            );
        }

        // Pass 2: pre-quantized q8_1 path.
        {
            let mut y_gpu =
                CudaTensor::<f32>::zeros(dev.clone(), vec![n]).expect("alloc y");
            launch_mmvq_iq4_xs_q8_1_f32(&dev, &a_gpu, k, n, &x_q8_1, &mut y_gpu)
                .expect("launch pre-quant path");
            dev.synchronize().expect("sync");

            let y_host = y_gpu.to_host().expect("download y");
            let (max_abs, max_rel) = diff(&y_cpu, &y_host);
            eprintln!(
                "[pre-quant] mmvq_iq4_xs diff: max_abs={:.6e} max_rel={:.6e}",
                max_abs, max_rel
            );
            assert!(
                max_rel < 1e-2,
                "pre-quant path diverges: max_rel={}",
                max_rel
            );
        }
    }

    /// Vector-wise absolute + relative error. `max_rel` uses a floor of
    /// `1e-3 * overall_absmax(a ∪ b)` in the denominator so that near-
    /// zero entries (which are common in random-weight matmuls and which
    /// get amplified by the q8_1 rounding in the internal-quantize path)
    /// don't inflate `max_rel` into meaninglessness. This matches how
    /// `numpy.testing.assert_allclose` and ggml's quant test suite
    /// measure error for vector outputs.
    fn diff(a: &[f32], b: &[f32]) -> (f32, f32) {
        let overall = a
            .iter()
            .chain(b.iter())
            .fold(0.0f32, |m, &v| m.max(v.abs()));
        let rel_floor = (overall * 1e-3).max(1e-6);
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (x, y) in a.iter().zip(b.iter()) {
            let d = (x - y).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale = x.abs().max(y.abs()).max(rel_floor);
            let rel = d / scale;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        (max_abs, max_rel)
    }
}
