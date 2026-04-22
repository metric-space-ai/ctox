//! Q4_K_M matrix-vector matmul (`mmvq`) — decode hot-path workhorse.
//!
//! Follows the conventions set by `rmsnorm`: one Rust wrapper module
//! per `.cu` file, `OnceLock` caches per loaded `CudaFunction`, shape
//! validation up front, no stream synchronization.
//!
//! The byte-packed Q4_K_M buffer is carried as a `CudaTensor<i8>` by
//! convention — the call site tracks the real `DType::Q4K` out of
//! band. The total byte count must equal `(k / 256) * n * 144`.
//!
//! Entry points:
//!   * `launch_mmvq_q4k_f32` — `y: f32[n] = A_q4k[n,k] · x[k]`
//!   * `launch_mmvq_q4k_f16` — same but writes to an f16 output buffer.
//!
//! TODO: batched `mmq_q4k` mat-mat variant (prefill path) — not ported
//!       yet, decode-first.
//! TODO: the reference quantizes `x` to q8_1 and uses DP4A. We skipped
//!       that optimization; port when correctness is locked in.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::f16;

use crate::device::DeviceContext;
use crate::tensor::CudaTensor;

// PTX blob emitted by build.rs for kernels/mmq_q4k.cu.
use super::MMQ_Q4K_PTX;

/// Per-process caches for the loaded kernel functions. See `rmsnorm.rs`
/// for the multi-GPU caveat.
static MMVQ_Q4K_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_Q4K_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per Q4_K_M block and logical elements per block (both fixed
/// by the GGUF format).
const Q4K_BLOCK_BYTES: usize = 144;
const Q4K_BLOCK_ELEMS: usize = 256;

fn load_mmq_q4k_fn(
    device: &Arc<DeviceContext>,
    cache: &'static OnceLock<CudaFunction>,
    sym: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(MMQ_Q4K_PTX)
        .map_err(|e| anyhow!("mmq_q4k.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module mmq_q4k.ptx: {:?}", e))?;
    let f = module
        .load_function(sym)
        .map_err(|e| anyhow!("load_function {}: {:?}", sym, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate common shapes/sizes before we touch the kernel. `k` must be
/// a whole multiple of 256 (the Q4_K_M block width).
fn validate_mmvq_q4k_shapes<T, U>(
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<T>,
    y: &CudaTensor<U>,
) -> Result<()>
where
    T: crate::tensor::TensorElem,
    U: crate::tensor::TensorElem,
{
    if k == 0 || n == 0 {
        return Err(anyhow!("mmvq_q4k: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q4K_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q4k: k must be a multiple of {} (got k={})",
            Q4K_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q4K_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q4K_BLOCK_BYTES;
    if a_q4k.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q4k: a_q4k byte count {} != (k/256)*n*144 = {} (k={}, n={})",
            a_q4k.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    // Input vector: [k] or [1, k] — accept either.
    let x_numel = x.numel();
    if x_numel != k {
        return Err(anyhow!(
            "mmvq_q4k: x.numel()={} != k={}",
            x_numel,
            k
        ));
    }
    // Output vector: [n] or [1, n] — accept either.
    let y_numel = y.numel();
    if y_numel != n {
        return Err(anyhow!(
            "mmvq_q4k: y.numel()={} != n={}",
            y_numel,
            n
        ));
    }
    Ok(())
}

/// `y[n] ← A_q4k[n, k] · x[k]`, all in f32 on the host contract.
pub fn launch_mmvq_q4k_f32(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, x, y)?;

    // grid.x = ceil(n/2): two output columns per block. Kernel guards
    // the out-of-range column when n is odd.
    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q4k_fn(device, &MMVQ_Q4K_F32_FN, "mmvq_q4k_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q4k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q4k_f32_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Same as the f32 variant but writes an f16 output row (used when the
/// downstream op consumes half-precision activations).
pub fn launch_mmvq_q4k_f16(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, x, y)?;

    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q4k_fn(device, &MMVQ_Q4K_F16_FN, "mmvq_q4k_f16_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q4k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q4k_f16_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Encode 256 f32 elements into a single 144-byte Q4_K_M block.
    /// Mirrors llama.cpp's `quantize_row_q4_K_ref` but simplified for a
    /// single block in the test harness. The block-level dequantization
    /// `dall * sc_j * q - dmin * m_j` must invert exactly (modulo 4-bit
    /// round-trip loss).
    fn encode_q4k_block(vals: &[f32; 256], out: &mut [u8; 144]) {
        // Per 32-element sub-block: choose (scale, min) that map q in
        // 0..=15 back onto vals with min-MSE. Use the simple linear
        // calibration: dmin_sub = min(vals), dmax_sub = max(vals),
        // scale = (dmax - dmin) / 15. Then q = round((v - dmin)/scale).
        // Then for the super-block: dall = max(scale_sub), dmin_all =
        // max(min_sub). Each per-sub-block scale/min is re-quantized
        // to 6 bits and packed into `scales[12]`.
        let mut scale_sub = [0.0f32; 8];
        let mut min_sub = [0.0f32; 8];
        let mut q_sub = [[0u8; 32]; 8];

        for s in 0..8 {
            let chunk = &vals[s * 32..(s + 1) * 32];
            let (mn, mx) = chunk.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| {
                (a.min(v), b.max(v))
            });
            // scale_sub stores "dmin" in ggml-land (the min val); the
            // ggml dequant is y = dall * sc * q - dmin_all * m, and the
            // calibration solves (dall * sc, dmin_all * m) as (per-sub
            // slope, per-sub intercept). For a single-block test we can
            // choose dall = max(slope_sub), dmin_all = max(intercept_sub).
            let slope = (mx - mn).max(1e-8) / 15.0;
            scale_sub[s] = slope;
            min_sub[s] = -mn;           // intercept = -mn so y=slope*q+mn → y=slope*q - (-mn)
            for (i, &v) in chunk.iter().enumerate() {
                let qf = ((v - mn) / slope).round().clamp(0.0, 15.0);
                q_sub[s][i] = qf as u8;
            }
        }

        // Super-block dall = max(scale_sub), dmin_all = max(min_sub).
        // 6-bit per-sub (sc, m): sc_q = round(slope/dall * 63),
        // m_q = round(intercept/dmin_all * 63). To keep the test
        // small-and-correct, keep all sub-block slopes/intercepts
        // identical (we'll feed vals that are uniform enough) — but
        // guard against division-by-zero.
        let dall = scale_sub.iter().cloned().fold(0.0f32, f32::max);
        let dmin_all = min_sub.iter().cloned().fold(0.0f32, f32::max);
        let dall_safe = if dall > 0.0 { dall } else { 1.0 };
        let dmin_safe = if dmin_all > 0.0 { dmin_all } else { 1.0 };

        let mut sc6 = [0u8; 8];
        let mut m6 = [0u8; 8];
        for s in 0..8 {
            sc6[s] = ((scale_sub[s] / dall_safe) * 63.0).round().clamp(0.0, 63.0) as u8;
            m6[s] = ((min_sub[s] / dmin_safe) * 63.0).round().clamp(0.0, 63.0) as u8;
        }

        // Write scales[12] per ggml's get_scale_min_k4:
        //   j<4: scales[j] low 6b = sc; scales[j+4] low 6b = m
        //   j>=4: scales[j+4] low nibble = sc_low; top 2 of sc go to
        //         scales[j-4] high 2b. Same for m: scales[j+4] high
        //         nibble = m_low; top 2 of m go to scales[j] high 2b.
        let mut scales = [0u8; 12];
        for j in 0..8 {
            let sc = sc6[j];
            let m = m6[j];
            if j < 4 {
                scales[j]     |= sc & 0x3F;
                scales[j + 4] |= m & 0x3F;
            } else {
                // low 4 of sc in scales[j+4] low nibble.
                scales[j + 4] |= sc & 0x0F;
                // top 2 of sc in scales[j-4] high 2 bits.
                scales[j - 4] |= ((sc >> 4) & 0x3) << 6;
                // low 4 of m in scales[j+4] high nibble.
                scales[j + 4] |= (m & 0x0F) << 4;
                // top 2 of m in scales[j] high 2 bits.
                scales[j] |= ((m >> 4) & 0x3) << 6;
            }
        }

        // Write the d/dmin halves (bytes 0..4).
        let d_h = f16::from_f32(dall_safe).to_bits();
        let dmin_h = f16::from_f32(dmin_safe).to_bits();
        out[0] = (d_h & 0xFF) as u8;
        out[1] = (d_h >> 8) as u8;
        out[2] = (dmin_h & 0xFF) as u8;
        out[3] = (dmin_h >> 8) as u8;
        // scales[12] at bytes 4..16.
        out[4..16].copy_from_slice(&scales);
        // qs[128] at bytes 16..144. The packing in ggml's dequant is:
        //   for il in 0..4, ir in 0..8, l in 0..4:
        //     qs[32*il + 4*ir + l] low nibble  = q_sub[2*il + 0][4*ir + l]
        //     qs[32*il + 4*ir + l] high nibble = q_sub[2*il + 1][4*ir + l]
        let mut qs = [0u8; 128];
        for il in 0..4 {
            for ir in 0..8 {
                for l in 0..4 {
                    let idx = 32 * il + 4 * ir + l;
                    let lo = q_sub[2 * il][4 * ir + l] & 0x0F;
                    let hi = q_sub[2 * il + 1][4 * ir + l] & 0x0F;
                    qs[idx] = lo | (hi << 4);
                }
            }
        }
        out[16..144].copy_from_slice(&qs);
    }

    /// Reference CPU dequant mirroring the kernel math: walk the 8 sub-
    /// blocks, decode (sc, m) with get_scale_min_k4, expand nibbles.
    fn dequant_q4k_block(bytes: &[u8; 144], out: &mut [f32; 256]) {
        let d = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let dmin = f16::from_bits(u16::from_le_bytes([bytes[2], bytes[3]])).to_f32();
        let scales = &bytes[4..16];
        let qs = &bytes[16..144];

        for il in 0..4usize {
            // Decode scale/min pairs for sub_a = 2*il, sub_b = 2*il+1.
            let get = |j: usize| -> (u8, u8) {
                if j < 4 {
                    (scales[j] & 63, scales[j + 4] & 63)
                } else {
                    let sc = (scales[j + 4] & 0x0F) | ((scales[j - 4] >> 6) << 4);
                    let m = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
                    (sc, m)
                }
            };
            let (sc_a, m_a) = get(2 * il);
            let (sc_b, m_b) = get(2 * il + 1);
            let d1 = d * sc_a as f32;
            let m1 = dmin * m_a as f32;
            let d2 = d * sc_b as f32;
            let m2 = dmin * m_b as f32;
            for ir in 0..8usize {
                for l in 0..4usize {
                    let qb = qs[32 * il + 4 * ir + l];
                    let lo = (qb & 0x0F) as f32;
                    let hi = (qb >> 4) as f32;
                    out[64 * il + 4 * ir + l] = d1 * lo - m1;
                    out[64 * il + 4 * ir + l + 32] = d2 * hi - m2;
                }
            }
        }
    }

    /// End-to-end integration test against a CPU golden. Run with:
    ///   cargo test -p ctox-engine-cuda --features cuda --release -- \
    ///       --ignored --nocapture mmvq_q4k
    #[test]
    #[ignore]
    fn mmvq_q4k_vs_cpu_golden() {
        // 16 blocks × 256 elems/block = 4096 columns (k). n = 64
        // output columns gives 32 blocks × 2 cols/block in grid.
        let k = 4096usize;
        let n = 64usize;
        let blocks_per_col = k / 256;
        assert_eq!(blocks_per_col, 16);

        // Deterministic pseudo-random. Use a narrower range so each
        // 32-elem sub-block's local (scale, min) don't blow up the
        // 6-bit super-block re-quantization.
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        // Build the packed Q4_K_M matrix: n columns × blocks_per_col
        // blocks/col × 144 bytes/block, laid out column-major over
        // columns (i.e. all blocks of col 0, then all blocks of col 1).
        // Track the CPU-dequantized matrix in parallel so we can compute
        // the golden via a plain f32 matmul.
        let total_bytes = n * blocks_per_col * 144;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                // Generate 256 random values, but compress the dynamic
                // range a bit so the 4-bit quantizer's 15-step rounding
                // doesn't dominate the error budget: one slope+offset
                // per sub-block is already an approximation.
                let mut vals = [0.0f32; 256];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5 + 0.25;
                }
                let mut block = [0u8; 144];
                encode_q4k_block(&vals, &mut block);
                // Actual Q4_K_M reconstruction — the matmul golden uses
                // this, not the original vals, because Q4_K_M is lossy
                // and the kernel's output matches the dequantized
                // matrix exactly (up to f32 round-off), not the input.
                let mut deq = [0.0f32; 256];
                dequant_q4k_block(&block, &mut deq);
                a_bytes[(col * blocks_per_col + b) * 144
                    ..(col * blocks_per_col + b + 1) * 144]
                    .copy_from_slice(&block);
                a_deq[col * k + b * 256..col * k + (b + 1) * 256]
                    .copy_from_slice(&deq);
            }
        }

        // Random f32 activation.
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

        // Run on device. Transmute the u8 bytes to i8 for the carrier
        // type; the kernel reads them as uint8_t internally.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let a_i8: Vec<i8> = a_bytes.iter().map(|&b| b as i8).collect();
        let a_gpu = CudaTensor::<i8>::from_host(
            dev.clone(),
            vec![a_i8.len()],
            &a_i8,
        )
        .expect("upload a_q4k");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");
        let mut y_gpu = CudaTensor::<f32>::zeros(dev.clone(), vec![n])
            .expect("alloc y");

        launch_mmvq_q4k_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu).expect("launch");
        dev.synchronize().expect("sync");

        let y_host = y_gpu.to_host().expect("download y");

        // Diff. The GPU kernel reconstructs the same dequantized matrix
        // we used for the CPU golden, so the residual is just f32
        // reduction-order drift (k=4096 → ~few × machine_eps).
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_host.iter()) {
            let d = (a - b).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale = a.abs().max(b.abs()).max(1e-6);
            let rel = d / scale;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        eprintln!(
            "mmvq_q4k diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 1e-2,
            "GPU mmvq_q4k diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
