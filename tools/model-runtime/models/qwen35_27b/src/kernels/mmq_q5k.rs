//! Q5_K matrix-vector matmul (`mmvq`) — decode hot-path workhorse.
//!
//! Mirrors [`mmq_q4k`](super::mmq_q4k) exactly: same `OnceLock` cache
//! layout, same launch shape, same shape validation. The only deltas
//! are the 176-byte block size (Q4_K is 144) and a 1-bit high-bit
//! `qh[32]` array inside the block that extends each 4-bit low nibble
//! up to a 5-bit unsigned quant.
//!
//! The 27B Qwen model ships 96 tensors in Q5_K (attn_qkv, attn_output,
//! ssm_out) — more than any other quantization — so this kernel plus
//! the packed upload path lets those tensors live on GPU as raw Q5_K
//! bytes instead of bf16, halving their VRAM footprint.
//!
//! The byte-packed Q5_K buffer is carried as a `CudaTensor<i8>` by
//! convention (same as Q4_K) — the call site tracks the real
//! `DType::Q5K` out of band. The total byte count must equal
//! `(k / 256) * n * 176`.
//!
//! Entry points:
//!   * `launch_mmvq_q5k_f32` — `y: f32[n] = A_q5k[n,k] · x[k]`
//!   * `launch_mmvq_q5k_f16` — same but writes to an f16 output buffer.
//!
//! TODO: batched `mmq_q5k` mat-mat variant (prefill path) — not ported
//!       yet, decode-first. Q5_K prefill cost on the 27B model is
//!       dominated by the same 96 tensors, so this is the natural
//!       follow-up once Agent T's packed-upload path lands.
//! TODO: the reference quantizes `x` to q8_1 and uses DP4A. We skipped
//!       that optimization here for parity with mmq_q4k; port once the
//!       two mat-vec kernels are known-correct end-to-end.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, CudaView, CudaViewMut, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::f16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob emitted by build.rs for kernels/mmq_q5k.cu.
use super::MMQ_Q5K_PTX;

/// Per-process caches for the loaded kernel functions. See `rmsnorm.rs`
/// for the multi-GPU caveat (same as Q4_K).
static MMVQ_Q5K_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_Q5K_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per Q5_K block and logical elements per block (both fixed
/// by the GGUF format).
const Q5K_BLOCK_BYTES: usize = 176;
const Q5K_BLOCK_ELEMS: usize = 256;

fn load_mmq_q5k_fn(
    device: &Arc<DeviceContext>,
    cache: &'static OnceLock<CudaFunction>,
    sym: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(MMQ_Q5K_PTX)
        .map_err(|e| anyhow!("mmq_q5k.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module mmq_q5k.ptx: {:?}", e))?;
    let f = module
        .load_function(sym)
        .map_err(|e| anyhow!("load_function {}: {:?}", sym, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate common shapes/sizes before we touch the kernel. `k` must be
/// a whole multiple of 256 (the Q5_K block width, same as Q4_K).
fn validate_mmvq_q5k_shapes<T, U>(
    a_q5k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<T>,
    y: &CudaTensor<U>,
) -> Result<()>
where
    T: ctox_cuda_primitives::tensor::TensorElem,
    U: ctox_cuda_primitives::tensor::TensorElem,
{
    if k == 0 || n == 0 {
        return Err(anyhow!("mmvq_q5k: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q5K_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q5k: k must be a multiple of {} (got k={})",
            Q5K_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q5K_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q5K_BLOCK_BYTES;
    if a_q5k.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q5k: a_q5k byte count {} != (k/256)*n*176 = {} (k={}, n={})",
            a_q5k.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    // Input vector: [k] or [1, k] — accept either.
    let x_numel = x.numel();
    if x_numel != k {
        return Err(anyhow!(
            "mmvq_q5k: x.numel()={} != k={}",
            x_numel,
            k
        ));
    }
    // Output vector: [n] or [1, n] — accept either.
    let y_numel = y.numel();
    if y_numel != n {
        return Err(anyhow!(
            "mmvq_q5k: y.numel()={} != n={}",
            y_numel,
            n
        ));
    }
    Ok(())
}

/// `y[n] ← A_q5k[n, k] · x[k]`, all in f32 on the host contract.
pub fn launch_mmvq_q5k_f32(
    device: &Arc<DeviceContext>,
    a_q5k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q5k_shapes(a_q5k, k, n, x, y)?;

    // grid.x = ceil(n/2): two output columns per block. Kernel guards
    // the out-of-range column when n is odd.
    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q5k_fn(device, &MMVQ_Q5K_F32_FN, "mmvq_q5k_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q5k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q5k_f32_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Slice/view variant of [`launch_mmvq_q5k_f32`] — same kernel, but
/// the f32 activation row and the f32 output row are passed as cudarc
/// device views instead of owned `CudaTensor`s.
///
/// Motivation: the host-side per-row loop in
/// `PackedWeight::matmul_f32` used to memcpy each input row into a
/// scratch `CudaTensor<f32>`, launch the mmvq, then memcpy the result
/// row back out. With this entry point the caller slices directly
/// into the contiguous `[m, k]` activation / `[m, n]` output tensors
/// and hands this function a view for each row — zero row-level
/// memcpys, just `m` kernel launches. See `matmul_q_rows` in
/// `layers/packed_weight.rs` for the batched orchestration.
///
/// The `x_row` view must cover `k` f32s; the `y_row` view must cover
/// `n` f32s. Weight-side validation matches the owned-tensor variant.
pub fn launch_mmvq_q5k_f32_view(
    device: &Arc<DeviceContext>,
    a_q5k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_row: &CudaView<'_, f32>,
    y_row: &mut CudaViewMut<'_, f32>,
) -> Result<()> {
    // Inlined weight shape check — the CudaTensor-only validator wants
    // owned x/y tensors for their numel() assertions, which we don't
    // have here. Everything else lines up with validate_mmvq_q5k_shapes().
    if k == 0 || n == 0 {
        return Err(anyhow!("mmvq_q5k: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q5K_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q5k: k must be a multiple of {} (got k={})",
            Q5K_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q5K_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q5K_BLOCK_BYTES;
    if a_q5k.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q5k: a_q5k byte count {} != (k/256)*n*176 = {} (k={}, n={})",
            a_q5k.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    if x_row.len() < k {
        return Err(anyhow!(
            "mmvq_q5k: x_row view len {} < k={}",
            x_row.len(),
            k
        ));
    }
    if y_row.len() < n {
        return Err(anyhow!(
            "mmvq_q5k: y_row view len {} < n={}",
            y_row.len(),
            n
        ));
    }

    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q5k_fn(device, &MMVQ_Q5K_F32_FN, "mmvq_q5k_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q5k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x_row)
        .arg(y_row);

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q5k_f32_view launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Same as the f32 variant but writes an f16 output row (used when the
/// downstream op consumes half-precision activations).
pub fn launch_mmvq_q5k_f16(
    device: &Arc<DeviceContext>,
    a_q5k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q5k_shapes(a_q5k, k, n, x, y)?;

    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q5k_fn(device, &MMVQ_Q5K_F16_FN, "mmvq_q5k_f16_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q5k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q5k_f16_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Encode 256 f32 elements into a single 176-byte Q5_K block.
    ///
    /// Strategy mirrors `encode_q4k_block` in mmq_q4k.rs: per 32-elem
    /// sub-block pick (slope, intercept) via min/max, quantize each
    /// value to a 5-bit unsigned code `q ∈ [0, 31]`, then pack the
    /// low 4 bits into `qs` and the high bit into `qh`. Per-sub-block
    /// (slope, intercept) are re-quantized to 6 bits against the
    /// super-block (dall, dmin_all) and written into `scales[12]` with
    /// the shared Q4_K / Q5_K packing (`get_scale_min_k4`).
    ///
    /// The test exercises the high-bit path by using a 32-step
    /// quantizer — values are genuinely 5-bit, not degenerate 4-bit.
    fn encode_q5k_block(vals: &[f32; 256], out: &mut [u8; 176]) {
        let mut scale_sub = [0.0f32; 8];
        let mut min_sub = [0.0f32; 8];
        // 32 unsigned 5-bit quants per sub-block (values in 0..=31).
        let mut q_sub = [[0u8; 32]; 8];

        for s in 0..8 {
            let chunk = &vals[s * 32..(s + 1) * 32];
            let (mn, mx) = chunk.iter().fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(a, b), &v| (a.min(v), b.max(v)),
            );
            // 5-bit quantizer: 31 steps between min and max.
            let slope = (mx - mn).max(1e-8) / 31.0;
            scale_sub[s] = slope;
            min_sub[s] = -mn;
            for (i, &v) in chunk.iter().enumerate() {
                let qf = ((v - mn) / slope).round().clamp(0.0, 31.0);
                q_sub[s][i] = qf as u8;
            }
        }

        // Super-block dall = max(slope_sub), dmin_all = max(intercept_sub).
        let dall = scale_sub.iter().cloned().fold(0.0f32, f32::max);
        let dmin_all = min_sub.iter().cloned().fold(0.0f32, f32::max);
        let dall_safe = if dall > 0.0 { dall } else { 1.0 };
        let dmin_safe = if dmin_all > 0.0 { dmin_all } else { 1.0 };

        // Per-sub 6-bit (sc, m). Same packing as Q4_K.
        let mut sc6 = [0u8; 8];
        let mut m6 = [0u8; 8];
        for s in 0..8 {
            sc6[s] = ((scale_sub[s] / dall_safe) * 63.0).round().clamp(0.0, 63.0) as u8;
            m6[s] = ((min_sub[s] / dmin_safe) * 63.0).round().clamp(0.0, 63.0) as u8;
        }

        // Write scales[12] per `get_scale_min_k4` (bit-identical to Q4_K).
        let mut scales = [0u8; 12];
        for j in 0..8 {
            let sc = sc6[j];
            let m = m6[j];
            if j < 4 {
                scales[j]     |= sc & 0x3F;
                scales[j + 4] |= m & 0x3F;
            } else {
                scales[j + 4] |= sc & 0x0F;
                scales[j - 4] |= ((sc >> 4) & 0x3) << 6;
                scales[j + 4] |= (m & 0x0F) << 4;
                scales[j] |= ((m >> 4) & 0x3) << 6;
            }
        }

        // Bytes 0..4: d / dmin halves.
        let d_h = f16::from_f32(dall_safe).to_bits();
        let dmin_h = f16::from_f32(dmin_safe).to_bits();
        out[0] = (d_h & 0xFF) as u8;
        out[1] = (d_h >> 8) as u8;
        out[2] = (dmin_h & 0xFF) as u8;
        out[3] = (dmin_h >> 8) as u8;
        // Bytes 4..16: scales[12].
        out[4..16].copy_from_slice(&scales);

        // Bytes 16..48: qh[32]. Each element's high bit (bit 4 of its
        // 5-bit code) is stored at position `within_block` (0..32), at
        // bit index `sub_id` (0..8) within that byte. Two sub-blocks
        // share the same `within_block` byte: sub-block `2*il` uses bit
        // `2*il`, sub-block `2*il+1` uses bit `2*il+1`.
        let mut qh = [0u8; 32];
        for il in 0..4usize {
            for w in 0..32usize {
                let qa = q_sub[2 * il][w];
                let qb = q_sub[2 * il + 1][w];
                let bit_a = ((qa >> 4) & 0x1) << (2 * il);
                let bit_b = ((qb >> 4) & 0x1) << (2 * il + 1);
                qh[w] |= bit_a | bit_b;
            }
        }
        out[16..48].copy_from_slice(&qh);

        // Bytes 48..176: qs[128]. Identical layout to Q4_K — low nibble
        // = sub-block 2*il, high nibble = sub-block 2*il+1, per the
        // `32*il + 4*ir + l` tiling. Only the low 4 bits of each 5-bit
        // code are stored here; the 5th bit lives in `qh`.
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
        out[48..176].copy_from_slice(&qs);
    }

    /// Reference CPU dequant mirroring the kernel math: walks the 8
    /// sub-blocks with `get_scale_min_k4`, reassembles 5-bit codes
    /// from qs (low nibble) + qh (high bit), and applies the affine
    /// `d * sc * q - dmin * m` per element.
    ///
    /// This is structurally identical to `gguf::dequant_q5_k_to_bf16`
    /// but emits f32 (the kernel golden is f32 end-to-end) instead of
    /// bf16. Both walk the 64-element (two-sub-block) groups with the
    /// u1/u2 bit masks. We keep the kernel's `(il, ir, l)` tiling so
    /// any bug in the shared encoder surfaces identically in CPU and
    /// GPU reconstructions.
    fn dequant_q5k_block(bytes: &[u8; 176], out: &mut [f32; 256]) {
        let d = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let dmin = f16::from_bits(u16::from_le_bytes([bytes[2], bytes[3]])).to_f32();
        let scales = &bytes[4..16];
        let qh = &bytes[16..48];
        let qs = &bytes[48..176];

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
            let u1 = 1u8 << (2 * il);
            let u2 = 2u8 << (2 * il);
            for ir in 0..8usize {
                for l in 0..4usize {
                    let byte_idx = 32 * il + 4 * ir + l;
                    // qh is a flat 32-byte array shared across all 8
                    // sub-blocks. Both sub_a and sub_b of this (il,ir,l)
                    // pair index the same byte `qh[within]`, just with
                    // different single-bit masks (u1 and u2).
                    let within = 4 * ir + l;
                    let qb = qs[byte_idx];
                    let hb = qh[within];
                    let qa = (qb & 0x0F) as i32 + if (hb & u1) != 0 { 16 } else { 0 };
                    let qc = (qb >> 4) as i32 + if (hb & u2) != 0 { 16 } else { 0 };
                    out[64 * il + 4 * ir + l] = d1 * qa as f32 - m1;
                    out[64 * il + 4 * ir + l + 32] = d2 * qc as f32 - m2;
                }
            }
        }
    }

    /// End-to-end integration test against a CPU golden. Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture mmvq_q5k
    #[test]
    #[ignore]
    fn mmvq_q5k_vs_cpu_golden() {
        // 16 blocks × 256 elems/block = 4096 columns (k). n = 256
        // output columns: 128 blocks × 2 cols/block in grid. Slightly
        // larger than the Q4_K test so the 5-bit high-bit branch gets
        // exercised across many distinct sub-block configurations.
        let k = 4096usize;
        let n = 256usize;
        let blocks_per_col = k / 256;
        assert_eq!(blocks_per_col, 16);

        // Deterministic pseudo-random, same LCG as the Q4_K test.
        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        // Build the packed Q5_K matrix: n columns × blocks_per_col
        // blocks/col × 176 bytes/block, laid out column-major over
        // columns. Track the CPU-dequantized matrix in parallel so we
        // can compute the golden via a plain f32 matmul against the
        // SAME reconstruction the GPU kernel will produce.
        let total_bytes = n * blocks_per_col * 176;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                // Generate 256 random values. Q5 gives us 32 steps per
                // sub-block (vs 16 for Q4), so we can afford a wider
                // dynamic range before the 5-bit quantizer saturates.
                let mut vals = [0.0f32; 256];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5 + 0.25;
                }
                let mut block = [0u8; 176];
                encode_q5k_block(&vals, &mut block);
                // Q5_K-reconstructed matrix — matmul golden uses this,
                // not the original `vals`, because Q5_K is lossy and
                // the kernel's output matches the dequantized matrix
                // exactly (up to f32 reduction-order drift), not the
                // pre-quantization input.
                let mut deq = [0.0f32; 256];
                dequant_q5k_block(&block, &mut deq);
                a_bytes[(col * blocks_per_col + b) * 176
                    ..(col * blocks_per_col + b + 1) * 176]
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
        .expect("upload a_q5k");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");
        let mut y_gpu = CudaTensor::<f32>::zeros(dev.clone(), vec![n])
            .expect("alloc y");

        launch_mmvq_q5k_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu).expect("launch f32");
        dev.synchronize().expect("sync");

        let y_host = y_gpu.to_host().expect("download y");

        // f32 diff. The GPU kernel reconstructs the same dequantized
        // matrix we used for the CPU golden, so the residual is just
        // f32 reduction-order drift (k=4096 → ~few × machine_eps).
        let mut max_abs_f32 = 0.0f32;
        let mut max_rel_f32 = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_host.iter()) {
            let d = (a - b).abs();
            if d > max_abs_f32 {
                max_abs_f32 = d;
            }
            let scale = a.abs().max(b.abs()).max(1e-6);
            let rel = d / scale;
            if rel > max_rel_f32 {
                max_rel_f32 = rel;
            }
        }
        eprintln!(
            "mmvq_q5k_f32 diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs_f32, max_rel_f32
        );
        assert!(
            max_rel_f32 < 1e-2,
            "GPU mmvq_q5k_f32 diverges from CPU golden: max_rel={}",
            max_rel_f32
        );

        // Also exercise the f16 output path against the same golden.
        let mut y_gpu_f16 = CudaTensor::<f16>::zeros(dev.clone(), vec![n])
            .expect("alloc y_f16");
        launch_mmvq_q5k_f16(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu_f16)
            .expect("launch f16");
        dev.synchronize().expect("sync f16");
        let y_host_f16 = y_gpu_f16.to_host().expect("download y_f16");
        let mut max_abs_f16 = 0.0f32;
        let mut max_rel_f16 = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_host_f16.iter()) {
            let b_f32 = b.to_f32();
            let d = (a - b_f32).abs();
            if d > max_abs_f16 {
                max_abs_f16 = d;
            }
            let scale = a.abs().max(b_f32.abs()).max(1e-6);
            let rel = d / scale;
            if rel > max_rel_f16 {
                max_rel_f16 = rel;
            }
        }
        eprintln!(
            "mmvq_q5k_f16 diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs_f16, max_rel_f16
        );
        // f16 output adds ~2^-10 rounding; keep the same 1e-2 envelope
        // since the golden magnitudes are ~O(1) and both formats match
        // on the first ~3 decimal digits.
        assert!(
            max_rel_f16 < 1e-2,
            "GPU mmvq_q5k_f16 diverges from CPU golden: max_rel={}",
            max_rel_f16
        );
    }
}
