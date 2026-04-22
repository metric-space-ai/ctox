//! Q4_K_M matrix-vector matmul (`mmvq`) — decode hot-path workhorse.
//!
//! Two launch tiers:
//!   1. `launch_mmvq_q4k_q8_1_{f32,f16}` — the fast path. Caller has
//!      already pre-quantized `x` to q8_1 blocks (via
//!      `launch_quantize_q8_1_f32`) and can reuse the buffer across
//!      several matmuls on the same activation. The kernel consumes
//!      those bytes directly and runs the DP4A inner-product loop.
//!   2. `launch_mmvq_q4k_{f32,f16}` — API-compatible entry that takes a
//!      raw f32 `x` and does the q8_1 quantization internally into a
//!      scratch buffer before invoking the fast path. This is what
//!      keeps existing call sites working without touching every LLM
//!      block that uses Q4_K for Q/K/V projections.
//!
//! Entry-point details (ported from ggml-cuda's mul_mat_vec_q<Q4_K>):
//!   * nwarps = 4, rows_per_cuda_block = 1, warp_size = 32.
//!   * grid_dim = (n, 1, 1), block_dim = (32, 4, 1).
//!   * Each CTA owns one output row; threads within the CTA iterate
//!     `blocks_per_iter = 8` q4_K blocks per round with DP4A inner
//!     products against q8_1 activations.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, CudaView, CudaViewMut, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::f16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::quantize_q8_1::{launch_quantize_q8_1_f32, q8_1_packed_bytes, Q8_1_BLOCK_ELEMS};

// PTX blob emitted by build.rs for kernels/mmq_q4k.cu.
use super::MMQ_Q4K_PTX;

/// Per-process caches for the loaded kernel functions.
static MMVQ_Q4K_Q8_1_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_Q4K_Q8_1_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per Q4_K_M block and logical elements per block (GGUF format).
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
/// a whole multiple of 256 (the Q4_K_M block width) and a whole multiple
/// of 32 (the Q8_1 block width — which is implied since 256 is a multiple
/// of 32).
fn validate_mmvq_q4k_shapes<U>(
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    y: &CudaTensor<U>,
) -> Result<()>
where
    U: ctox_cuda_primitives::tensor::TensorElem,
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
    if y.numel() != n {
        return Err(anyhow!("mmvq_q4k: y.numel()={} != n={}", y.numel(), n));
    }
    Ok(())
}

fn validate_q8_1_x(x_q8_1: &CudaTensor<i8>, k: usize) -> Result<()> {
    let expected = q8_1_packed_bytes(k);
    if x_q8_1.numel() < expected {
        return Err(anyhow!(
            "mmvq_q4k: x_q8_1 bytes {} < required {} for k={}",
            x_q8_1.numel(),
            expected,
            k
        ));
    }
    Ok(())
}

fn mmvq_launch_cfg(n: usize) -> LaunchConfig {
    // One CTA per output row (rows_per_cuda_block=1), NWARPS=4.
    LaunchConfig {
        grid_dim: (n as u32, 1, 1),
        block_dim: (32, 4, 1),
        shared_mem_bytes: 0,
    }
}

// ---- Direct (pre-quantized x) entry points --------------------------------

/// Fast path — caller supplies a pre-quantized q8_1 activation buffer.
///
/// `x_q8_1` must be at least `q8_1_packed_bytes(k)` bytes and must have
/// been produced by `launch_quantize_q8_1_f32` over the same `k`
/// elements.
pub fn launch_mmvq_q4k_q8_1_f32(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_q8_1: &CudaTensor<i8>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, y)?;
    validate_q8_1_x(x_q8_1, k)?;

    let f = load_mmq_q4k_fn(device, &MMVQ_Q4K_Q8_1_F32_FN, "mmvq_q4k_q8_1_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q4k.buf())
        .arg(x_q8_1.buf())
        .arg(y.buf_mut())
        .arg(&k_i32)
        .arg(&n_i32);

    unsafe { launcher.launch(mmvq_launch_cfg(n)) }
        .map_err(|e| anyhow!("mmvq_q4k_q8_1_f32 launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Slice/view variant of [`launch_mmvq_q4k_q8_1_f32`] — same kernel,
/// but the activation q8_1 bytes and the f32 output row are passed as
/// cudarc device views instead of owned `CudaTensor`s.
///
/// Motivation: the host-side per-row loop in
/// `PackedWeight::matmul_f32` used to memcpy each input row into a
/// scratch `CudaTensor<f32>`, launch the mmvq, then memcpy the result
/// row back out. With this entry point the caller pre-quantizes the
/// whole `[m, k]` activation into a contiguous q8_1 scratch once,
/// then hands this function a view for each row — zero row-level
/// memcpys, just `m` kernel launches. See `matmul_q_rows` in
/// `layers/packed_weight.rs` for the batched orchestration.
///
/// The `x_q8_1_row` view must cover at least `q8_1_packed_bytes(k)`
/// bytes; the `y_row` view must cover `n` f32s. Validation on byte
/// counts matches the owned-tensor variant.
pub fn launch_mmvq_q4k_q8_1_f32_view(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_q8_1_row: &CudaView<'_, i8>,
    y_row: &mut CudaViewMut<'_, f32>,
) -> Result<()> {
    // Inlined weight shape check — the CudaTensor-only validator wants
    // an owned y tensor for its numel() assertion, which we don't have
    // here. Everything else lines up with validate_mmvq_q4k_shapes().
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

    let needed_bytes = q8_1_packed_bytes(k);
    if x_q8_1_row.len() < needed_bytes {
        return Err(anyhow!(
            "mmvq_q4k: x_q8_1 view len {} < required {} for k={}",
            x_q8_1_row.len(),
            needed_bytes,
            k
        ));
    }
    if y_row.len() < n {
        return Err(anyhow!(
            "mmvq_q4k: y_row view len {} < n={}",
            y_row.len(),
            n
        ));
    }

    let f = load_mmq_q4k_fn(device, &MMVQ_Q4K_Q8_1_F32_FN, "mmvq_q4k_q8_1_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q4k.buf())
        .arg(x_q8_1_row)
        .arg(y_row)
        .arg(&k_i32)
        .arg(&n_i32);

    unsafe { launcher.launch(mmvq_launch_cfg(n)) }
        .map_err(|e| anyhow!("mmvq_q4k_q8_1_f32_view launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Same as the f32 variant but writes to an f16 output row.
pub fn launch_mmvq_q4k_q8_1_f16(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_q8_1: &CudaTensor<i8>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, y)?;
    validate_q8_1_x(x_q8_1, k)?;

    let f = load_mmq_q4k_fn(device, &MMVQ_Q4K_Q8_1_F16_FN, "mmvq_q4k_q8_1_f16_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q4k.buf())
        .arg(x_q8_1.buf())
        .arg(y.buf_mut())
        .arg(&k_i32)
        .arg(&n_i32);

    unsafe { launcher.launch(mmvq_launch_cfg(n)) }
        .map_err(|e| anyhow!("mmvq_q4k_q8_1_f16 launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

// ---- API-compatible entry points (quantize internally) --------------------

/// `y[n] ← A_q4k[n, k] · x[k]`, all in f32 on the host contract.
///
/// Internally quantizes `x` to q8_1 in a scratch buffer, then invokes the
/// DP4A fast path. Callers that do repeated matmuls on the same `x` (e.g.
/// fused Q/K/V projections) should pre-quantize once via
/// `launch_quantize_q8_1_f32` and call `launch_mmvq_q4k_q8_1_f32` directly.
pub fn launch_mmvq_q4k_f32(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, y)?;
    if x.numel() != k {
        return Err(anyhow!("mmvq_q4k: x.numel()={} != k={}", x.numel(), k));
    }
    if !k.is_multiple_of(Q8_1_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q4k: k must be a multiple of {} (got k={})",
            Q8_1_BLOCK_ELEMS,
            k
        ));
    }

    let mut scratch =
        CudaTensor::<i8>::zeros(device.clone(), vec![q8_1_packed_bytes(k)])
            .map_err(|e| anyhow!("alloc q8_1 scratch: {:?}", e))?;
    launch_quantize_q8_1_f32(device, x, &mut scratch, k)?;
    launch_mmvq_q4k_q8_1_f32(device, a_q4k, k, n, &scratch, y)
}

/// Same as the f32 variant but writes an f16 output row.
pub fn launch_mmvq_q4k_f16(
    device: &Arc<DeviceContext>,
    a_q4k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q4k_shapes(a_q4k, k, n, y)?;
    if x.numel() != k {
        return Err(anyhow!("mmvq_q4k: x.numel()={} != k={}", x.numel(), k));
    }
    if !k.is_multiple_of(Q8_1_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q4k: k must be a multiple of {} (got k={})",
            Q8_1_BLOCK_ELEMS,
            k
        ));
    }

    let mut scratch =
        CudaTensor::<i8>::zeros(device.clone(), vec![q8_1_packed_bytes(k)])
            .map_err(|e| anyhow!("alloc q8_1 scratch: {:?}", e))?;
    launch_quantize_q8_1_f32(device, x, &mut scratch, k)?;
    launch_mmvq_q4k_q8_1_f16(device, a_q4k, k, n, &scratch, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Encode 256 f32 elements into a single 144-byte Q4_K_M block.
    /// Mirrors llama.cpp's `quantize_row_q4_K_ref` closely enough for a
    /// single-block round-trip to be lossless modulo 4-bit and 6-bit
    /// re-quantization.
    fn encode_q4k_block(vals: &[f32; 256], out: &mut [u8; 144]) {
        let mut scale_sub = [0.0f32; 8];
        let mut min_sub = [0.0f32; 8];
        let mut q_sub = [[0u8; 32]; 8];

        for s in 0..8 {
            let chunk = &vals[s * 32..(s + 1) * 32];
            let (mn, mx) = chunk.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(a, b), &v| (a.min(v), b.max(v)),
            );
            let slope = (mx - mn).max(1e-8) / 15.0;
            scale_sub[s] = slope;
            min_sub[s] = -mn;
            for (i, &v) in chunk.iter().enumerate() {
                let qf = ((v - mn) / slope).round().clamp(0.0, 15.0);
                q_sub[s][i] = qf as u8;
            }
        }

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

        let mut scales = [0u8; 12];
        for j in 0..8 {
            let sc = sc6[j];
            let m = m6[j];
            if j < 4 {
                scales[j] |= sc & 0x3F;
                scales[j + 4] |= m & 0x3F;
            } else {
                scales[j + 4] |= sc & 0x0F;
                scales[j - 4] |= ((sc >> 4) & 0x3) << 6;
                scales[j + 4] |= (m & 0x0F) << 4;
                scales[j] |= ((m >> 4) & 0x3) << 6;
            }
        }

        let d_h = f16::from_f32(dall_safe).to_bits();
        let dmin_h = f16::from_f32(dmin_safe).to_bits();
        out[0] = (d_h & 0xFF) as u8;
        out[1] = (d_h >> 8) as u8;
        out[2] = (dmin_h & 0xFF) as u8;
        out[3] = (dmin_h >> 8) as u8;
        out[4..16].copy_from_slice(&scales);
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

    /// Reference CPU dequant mirroring the kernel math.
    fn dequant_q4k_block(bytes: &[u8; 144], out: &mut [f32; 256]) {
        let d = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let dmin = f16::from_bits(u16::from_le_bytes([bytes[2], bytes[3]])).to_f32();
        let scales = &bytes[4..16];
        let qs = &bytes[16..144];

        for il in 0..4usize {
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

    /// Build the standard Q4_K_M test matrix + CPU golden used by both the
    /// correctness test and the perf bench.
    fn build_test_matrix(
        n: usize,
        k: usize,
    ) -> (Vec<u8>, Vec<f32>, Vec<f32>, Vec<f32>) {
        let blocks_per_col = k / Q4K_BLOCK_ELEMS;
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let total_bytes = n * blocks_per_col * Q4K_BLOCK_BYTES;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                let mut vals = [0.0f32; 256];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5 + 0.25;
                }
                let mut block = [0u8; 144];
                encode_q4k_block(&vals, &mut block);
                let mut deq = [0.0f32; 256];
                dequant_q4k_block(&block, &mut deq);
                let base = (col * blocks_per_col + b) * Q4K_BLOCK_BYTES;
                a_bytes[base..base + Q4K_BLOCK_BYTES].copy_from_slice(&block);
                a_deq[col * k + b * 256..col * k + (b + 1) * 256].copy_from_slice(&deq);
            }
        }

        let x_host: Vec<f32> = (0..k).map(|_| rand_f()).collect();

        // CPU golden — use the dequantized A and exact f32 matmul.
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

    /// End-to-end integration test against a CPU golden.
    /// Exercises both the API-compat path (quantize x internally) and the
    /// pre-quantized-x fast path.
    #[test]
    #[ignore]
    fn mmvq_q4k_vs_cpu_golden() {
        let k = 4096usize;
        let n = 64usize;
        let (a_bytes, _a_deq, x_host, y_cpu) = build_test_matrix(n, k);

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let a_i8: Vec<i8> = a_bytes.iter().map(|&b| b as i8).collect();
        let a_gpu = CudaTensor::<i8>::from_host(dev.clone(), vec![a_i8.len()], &a_i8)
            .expect("upload a_q4k");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");

        // Pass 1: API-compat path (internal quantize).
        {
            let mut y_gpu =
                CudaTensor::<f32>::zeros(dev.clone(), vec![n]).expect("alloc y");
            launch_mmvq_q4k_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu)
                .expect("launch internal-quant path");
            dev.synchronize().expect("sync");

            let y_host = y_gpu.to_host().expect("download y");
            let (max_abs, max_rel) = diff(&y_cpu, &y_host);
            eprintln!(
                "[internal-quant] mmvq_q4k diff: max_abs={:.6e} max_rel={:.6e}",
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
            let mut x_q8_1 =
                CudaTensor::<i8>::zeros(dev.clone(), vec![q8_1_packed_bytes(k)])
                    .expect("alloc q8_1 scratch");
            launch_quantize_q8_1_f32(&dev, &x_gpu, &mut x_q8_1, k)
                .expect("launch quantize_q8_1");

            let mut y_gpu =
                CudaTensor::<f32>::zeros(dev.clone(), vec![n]).expect("alloc y");
            launch_mmvq_q4k_q8_1_f32(&dev, &a_gpu, k, n, &x_q8_1, &mut y_gpu)
                .expect("launch pre-quant path");
            dev.synchronize().expect("sync");

            let y_host = y_gpu.to_host().expect("download y");
            let (max_abs, max_rel) = diff(&y_cpu, &y_host);
            eprintln!(
                "[pre-quant] mmvq_q4k diff: max_abs={:.6e} max_rel={:.6e}",
                max_abs, max_rel
            );
            assert!(
                max_rel < 1e-2,
                "pre-quant path diverges: max_rel={}",
                max_rel
            );
        }
    }

    fn diff(a: &[f32], b: &[f32]) -> (f32, f32) {
        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (x, y) in a.iter().zip(b.iter()) {
            let d = (x - y).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale = x.abs().max(y.abs()).max(1e-6);
            let rel = d / scale;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        (max_abs, max_rel)
    }

    /// Perf bench — one 27B-sized Q projection per launch, 1000 iterations.
    /// Target on sm_86: > 500 GB/s on the weight-matrix read.
    ///
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture mmvq_q4k_perf_bench
    #[test]
    #[ignore]
    fn mmvq_q4k_perf_bench() {
        use cudarc::driver::sys::CUevent_flags;

        // Shape: one 27B FullAttention Q projection (k=4096, n=5120).
        let k = 4096usize;
        let n = 5120usize;
        let iters = 1000u32;

        let (a_bytes, _a_deq, x_host, _y_cpu) = build_test_matrix(n, k);
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let a_i8: Vec<i8> = a_bytes.iter().map(|&b| b as i8).collect();
        let a_gpu = CudaTensor::<i8>::from_host(dev.clone(), vec![a_i8.len()], &a_i8)
            .expect("upload a");
        let x_gpu =
            CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host).expect("upload x");
        let mut x_q8_1 =
            CudaTensor::<i8>::zeros(dev.clone(), vec![q8_1_packed_bytes(k)])
                .expect("alloc q8_1 scratch");
        launch_quantize_q8_1_f32(&dev, &x_gpu, &mut x_q8_1, k).expect("quantize");
        let mut y_gpu =
            CudaTensor::<f32>::zeros(dev.clone(), vec![n]).expect("alloc y");

        // Warm-up — first launch also loads the PTX module.
        for _ in 0..3 {
            launch_mmvq_q4k_q8_1_f32(&dev, &a_gpu, k, n, &x_q8_1, &mut y_gpu)
                .expect("warmup launch");
        }
        dev.synchronize().expect("warmup sync");

        // CUDA event timing with cudarc's safe API. CU_EVENT_DEFAULT keeps
        // timing enabled; DISABLE_TIMING would make elapsed_ms fail.
        let ctx = dev.raw();
        let stream = ctx.default_stream();
        let start = ctx
            .new_event(Some(CUevent_flags::CU_EVENT_DEFAULT))
            .expect("create start event");
        let end = ctx
            .new_event(Some(CUevent_flags::CU_EVENT_DEFAULT))
            .expect("create end event");

        start.record(&stream).expect("record start");
        for _ in 0..iters {
            launch_mmvq_q4k_q8_1_f32(&dev, &a_gpu, k, n, &x_q8_1, &mut y_gpu)
                .expect("bench launch");
        }
        end.record(&stream).expect("record end");
        end.synchronize().expect("end sync");

        let ms = start.elapsed_ms(&end).expect("elapsed_ms");
        let per_iter_s = (ms as f64 / 1000.0) / iters as f64;
        let a_bytes_read = (k as f64 / 256.0) * n as f64 * 144.0;
        let gbps = a_bytes_read / per_iter_s / 1.0e9;

        eprintln!(
            "mmvq_q4k_perf_bench: k={} n={} iters={} avg={:.3} us/iter, A-read = {:.1} GB/s",
            k,
            n,
            iters,
            per_iter_s * 1.0e6,
            gbps
        );

        // Hard floor: <300 GB/s means we broke the hot path or a launch
        // setting regressed. Upper target is > 500 GB/s on sm_86 — below
        // that we warn but don't fail, since HW/clocks/thermals can nudge
        // the number on a single run.
        assert!(
            gbps > 300.0,
            "mmvq_q4k perf floor breached: {:.1} GB/s < 300",
            gbps
        );
        if gbps < 500.0 {
            eprintln!("WARN: below 500 GB/s target on sm_86 ({:.1} GB/s)", gbps);
        }
    }
}
