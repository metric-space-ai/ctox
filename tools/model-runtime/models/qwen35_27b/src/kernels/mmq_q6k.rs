//! Q6_K matrix-vector matmul (`mmvq`) — decode hot-path workhorse.
//!
//! Follows the conventions set by `mmq_q4k`: one Rust wrapper module
//! per `.cu` file, `OnceLock` caches per loaded `CudaFunction`, shape
//! validation up front, no stream synchronization.
//!
//! The byte-packed Q6_K buffer is carried as a `CudaTensor<i8>` by
//! convention — the call site tracks the real `DType::Q6K` out of
//! band. The total byte count must equal `(k / 256) * n * 210`.
//!
//! Entry points:
//!   * `launch_mmvq_q6k_f32` — `y: f32[n] = A_q6k[n,k] · x[k]`
//!   * `launch_mmvq_q6k_f16` — same but writes to an f16 output buffer.
//!
//! TODO: batched `mmq_q6k` mat-mat variant (prefill path) — not ported
//!       yet, decode-first.
//! TODO: the reference quantizes `x` to q8_1 and uses DP4A. We skipped
//!       that optimization; port when correctness is locked in.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, CudaView, CudaViewMut, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::f16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob emitted by build.rs for kernels/mmq_q6k.cu.
use super::MMQ_Q6K_PTX;

/// Per-process caches for the loaded kernel functions. See `rmsnorm.rs`
/// for the multi-GPU caveat.
static MMVQ_Q6K_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_Q6K_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per Q6_K block and logical elements per block (both fixed
/// by the GGUF format).
const Q6K_BLOCK_BYTES: usize = 210;
const Q6K_BLOCK_ELEMS: usize = 256;

fn load_mmq_q6k_fn(
    device: &Arc<DeviceContext>,
    cache: &'static OnceLock<CudaFunction>,
    sym: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(MMQ_Q6K_PTX)
        .map_err(|e| anyhow!("mmq_q6k.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module mmq_q6k.ptx: {:?}", e))?;
    let f = module
        .load_function(sym)
        .map_err(|e| anyhow!("load_function {}: {:?}", sym, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate common shapes/sizes before we touch the kernel. `k` must be
/// a whole multiple of 256 (the Q6_K block width).
fn validate_mmvq_q6k_shapes<T, U>(
    a_q6k: &CudaTensor<i8>,
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
        return Err(anyhow!("mmvq_q6k: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q6K_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q6k: k must be a multiple of {} (got k={})",
            Q6K_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q6K_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q6K_BLOCK_BYTES;
    if a_q6k.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q6k: a_q6k byte count {} != (k/256)*n*210 = {} (k={}, n={})",
            a_q6k.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    let x_numel = x.numel();
    if x_numel != k {
        return Err(anyhow!(
            "mmvq_q6k: x.numel()={} != k={}",
            x_numel,
            k
        ));
    }
    let y_numel = y.numel();
    if y_numel != n {
        return Err(anyhow!(
            "mmvq_q6k: y.numel()={} != n={}",
            y_numel,
            n
        ));
    }
    Ok(())
}

/// `y[n] ← A_q6k[n, k] · x[k]`, all in f32 on the host contract.
pub fn launch_mmvq_q6k_f32(
    device: &Arc<DeviceContext>,
    a_q6k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q6k_shapes(a_q6k, k, n, x, y)?;

    // grid.x = ceil(n/2): two output columns per block. Kernel guards
    // the out-of-range column when n is odd.
    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q6k_fn(device, &MMVQ_Q6K_F32_FN, "mmvq_q6k_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q6k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q6k_f32_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Slice/view variant of [`launch_mmvq_q6k_f32`] — same kernel, but
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
pub fn launch_mmvq_q6k_f32_view(
    device: &Arc<DeviceContext>,
    a_q6k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_row: &CudaView<'_, f32>,
    y_row: &mut CudaViewMut<'_, f32>,
) -> Result<()> {
    // Inlined weight shape check — the CudaTensor-only validator wants
    // owned x/y tensors for their numel() assertions, which we don't
    // have here. Everything else lines up with validate_mmvq_q6k_shapes().
    if k == 0 || n == 0 {
        return Err(anyhow!("mmvq_q6k: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q6K_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q6k: k must be a multiple of {} (got k={})",
            Q6K_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q6K_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q6K_BLOCK_BYTES;
    if a_q6k.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q6k: a_q6k byte count {} != (k/256)*n*210 = {} (k={}, n={})",
            a_q6k.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    if x_row.len() < k {
        return Err(anyhow!(
            "mmvq_q6k: x_row view len {} < k={}",
            x_row.len(),
            k
        ));
    }
    if y_row.len() < n {
        return Err(anyhow!(
            "mmvq_q6k: y_row view len {} < n={}",
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

    let f = load_mmq_q6k_fn(device, &MMVQ_Q6K_F32_FN, "mmvq_q6k_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q6k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x_row)
        .arg(y_row);

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q6k_f32_view launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Same as the f32 variant but writes an f16 output row (used when the
/// downstream op consumes half-precision activations).
pub fn launch_mmvq_q6k_f16(
    device: &Arc<DeviceContext>,
    a_q6k: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q6k_shapes(a_q6k, k, n, x, y)?;

    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q6k_fn(device, &MMVQ_Q6K_F16_FN, "mmvq_q6k_f16_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q6k.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q6k_f16_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Encode 256 f32 elements into a single 210-byte Q6_K block.
    ///
    /// Mirrors llama.cpp's `quantize_row_q6_K_ref` but simplified for
    /// the test harness. Strategy:
    ///   * Pick one super-scale `d = max(|v|) / (127 * 32)` so that
    ///     per-16-elem `scale_sub * q_signed` stays in the representable
    ///     range (`scales` are i8 in Q6_K, and `q_signed` is in -32..31).
    ///   * For each of the 16 sub-blocks of 16 elems: pick an i8
    ///     `scale_sub` that minimizes |round(v / (d * scale_sub))|
    ///     over the 16 values. Simple approach: pick the absmax in
    ///     the sub-block, then `scale_sub = round(absmax / (d * 31))`.
    ///   * Quantize each element as `q = clamp(round(v / (d * scale_sub)), -32, 31)`
    ///     and pack into ql[128] (low 4 bits) + qh[64] (high 2 bits),
    ///     then shift by +32 to put in 0..63.
    ///
    /// Keep things simple: the test values are narrow enough that
    /// every sub-block shares a similar range, so we can pick a
    /// single per-block scaling and keep quant error bounded.
    fn encode_q6k_block(vals: &[f32; 256], out: &mut [u8; 210]) {
        // Super-block scale: choose d so the maximum |scaled_q| stays
        // well under 127 (the i8 scale limit) × 31 (the quant limit).
        let mut absmax = 0.0f32;
        for &v in vals.iter() {
            if v.abs() > absmax {
                absmax = v.abs();
            }
        }
        // Target: d * 31 * 127 >= absmax → d >= absmax / (31 * 127).
        // Leave headroom so per-sub-block scales don't hit the ceiling.
        let d = (absmax / (31.0 * 100.0)).max(1e-8);

        // Per-16-elem sub-scale (i8) and the quantized 6-bit values.
        let mut sc = [0i8; 16];
        let mut q_signed = [0i32; 256];

        for s in 0..16 {
            let chunk = &vals[s * 16..(s + 1) * 16];
            let mut sub_absmax = 0.0f32;
            for &v in chunk.iter() {
                if v.abs() > sub_absmax {
                    sub_absmax = v.abs();
                }
            }
            // Per-sub scale such that scale_sub * 31 * d ≈ sub_absmax.
            // Clamp to i8 range and avoid zero.
            let s_f = (sub_absmax / (d * 31.0)).round().clamp(1.0, 127.0);
            sc[s] = s_f as i8;
            let denom = d * (s_f as f32);
            for (i, &v) in chunk.iter().enumerate() {
                let q = (v / denom).round().clamp(-32.0, 31.0) as i32;
                q_signed[s * 16 + i] = q;
            }
        }

        // Pack into the Q6_K block layout. ql[128] (low 4 bits per
        // elem) and qh[64] (high 2 bits per elem). Element index
        // layout matches dequant_q6_k_to_bf16 in src/gguf.rs.
        let mut ql = [0u8; 128];
        let mut qh = [0u8; 64];

        // Walk the two 128-element halves.
        for n_half in 0..2usize {
            let n_off = n_half * 128;
            let ql_n_off = n_off / 2;
            let qh_n_off = n_off / 4;

            // For each l in 0..32: fill quads at (l, l+32, l+64, l+96).
            for l in 0..32usize {
                let shift_l = q_signed[n_off + l] + 32;          // goes into ql_n[l]      low nibble + qh high bits 0..2
                let shift_l32 = q_signed[n_off + l + 32] + 32;   // ql_n[l+32] low nibble + qh bits 2..4
                let shift_l64 = q_signed[n_off + l + 64] + 32;   // ql_n[l]    high nibble + qh bits 4..6
                let shift_l96 = q_signed[n_off + l + 96] + 32;   // ql_n[l+32] high nibble + qh bits 6..8

                let ql_low_l = (shift_l as u32) & 0x0F;
                let ql_low_l32 = (shift_l32 as u32) & 0x0F;
                let ql_high_l = ((shift_l as u32) >> 4) & 0x03;
                let ql_high_l32 = ((shift_l32 as u32) >> 4) & 0x03;
                let ql_low_l64 = (shift_l64 as u32) & 0x0F;
                let ql_low_l96 = (shift_l96 as u32) & 0x0F;
                let ql_high_l64 = ((shift_l64 as u32) >> 4) & 0x03;
                let ql_high_l96 = ((shift_l96 as u32) >> 4) & 0x03;

                // Dequant reads:
                //   ql_n[l]      low  = q1 low4
                //   ql_n[l]      high = q3 low4   (note: dequant uses "ql_n[l] >> 4" for q3)
                //   ql_n[l+32]   low  = q2 low4
                //   ql_n[l+32]   high = q4 low4
                //
                //   qh_n[l]      bits[0:2] = q1 high2
                //   qh_n[l]      bits[2:4] = q2 high2
                //   qh_n[l]      bits[4:6] = q3 high2
                //   qh_n[l]      bits[6:8] = q4 high2
                ql[ql_n_off + l] = (ql_low_l | (ql_low_l64 << 4)) as u8;
                ql[ql_n_off + l + 32] = (ql_low_l32 | (ql_low_l96 << 4)) as u8;
                qh[qh_n_off + l] =
                    (ql_high_l | (ql_high_l32 << 2) | (ql_high_l64 << 4) | (ql_high_l96 << 6))
                        as u8;
            }
        }

        // Write the block out: ql[128] qh[64] scales[16 i8] d(half)[2].
        out[0..128].copy_from_slice(&ql);
        out[128..192].copy_from_slice(&qh);
        for (i, &s) in sc.iter().enumerate() {
            out[192 + i] = s as u8;
        }
        let d_h = f16::from_f32(d).to_bits();
        out[208] = (d_h & 0xFF) as u8;
        out[209] = (d_h >> 8) as u8;
    }

    /// Reference CPU dequant mirroring the kernel math. Matches
    /// dequant_q6_k_to_bf16 in src/gguf.rs bit-for-bit (minus the
    /// bf16 rounding — we keep f32).
    fn dequant_q6k_block(bytes: &[u8; 210], out: &mut [f32; 256]) {
        let ql = &bytes[0..128];
        let qh = &bytes[128..192];
        let sc = &bytes[192..208];
        let d = f16::from_bits(u16::from_le_bytes([bytes[208], bytes[209]])).to_f32();

        for n_half in 0..2usize {
            let n_off = n_half * 128;
            let ql_n = &ql[(n_off / 2)..(n_off / 2 + 64)];
            let qh_n = &qh[(n_off / 4)..(n_off / 4 + 32)];
            let sc_n = &sc[(n_off / 16)..(n_off / 16 + 8)];

            for l in 0..32usize {
                let is = l / 16;
                let q1 = ((ql_n[l] & 0x0F) | ((qh_n[l] & 3) << 4)) as i32 - 32;
                let q2 = ((ql_n[l + 32] & 0x0F) | (((qh_n[l] >> 2) & 3) << 4)) as i32 - 32;
                let q3 = ((ql_n[l] >> 4) | (((qh_n[l] >> 4) & 3) << 4)) as i32 - 32;
                let q4 = ((ql_n[l + 32] >> 4) | (((qh_n[l] >> 6) & 3) << 4)) as i32 - 32;
                let s0 = sc_n[is] as i8 as f32;
                let s2 = sc_n[is + 2] as i8 as f32;
                let s4 = sc_n[is + 4] as i8 as f32;
                let s6 = sc_n[is + 6] as i8 as f32;
                out[n_off + l] = d * s0 * (q1 as f32);
                out[n_off + l + 32] = d * s2 * (q2 as f32);
                out[n_off + l + 64] = d * s4 * (q3 as f32);
                out[n_off + l + 96] = d * s6 * (q4 as f32);
            }
        }
    }

    /// End-to-end integration test against a CPU golden. Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture mmvq_q6k
    #[test]
    #[ignore]
    fn mmvq_q6k_vs_cpu_golden() {
        let k = 4096usize;
        let n = 256usize;
        let blocks_per_col = k / 256;
        assert_eq!(blocks_per_col, 16);

        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let total_bytes = n * blocks_per_col * 210;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                // Narrow range so the 16-bucket linear quant stays
                // well-behaved. Q6 has ~2^6 = 64 levels per sub-block,
                // so the quant error is ~1% of the sub-block range.
                let mut vals = [0.0f32; 256];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5;
                }
                let mut block = [0u8; 210];
                encode_q6k_block(&vals, &mut block);
                let mut deq = [0.0f32; 256];
                dequant_q6k_block(&block, &mut deq);
                a_bytes[(col * blocks_per_col + b) * 210
                    ..(col * blocks_per_col + b + 1) * 210]
                    .copy_from_slice(&block);
                a_deq[col * k + b * 256..col * k + (b + 1) * 256]
                    .copy_from_slice(&deq);
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

        // Run on device. Transmute the u8 bytes to i8 for the carrier
        // type; the kernel reads them as uint8_t internally.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let a_i8: Vec<i8> = a_bytes.iter().map(|&b| b as i8).collect();
        let a_gpu = CudaTensor::<i8>::from_host(
            dev.clone(),
            vec![a_i8.len()],
            &a_i8,
        )
        .expect("upload a_q6k");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");
        let mut y_gpu = CudaTensor::<f32>::zeros(dev.clone(), vec![n])
            .expect("alloc y");

        launch_mmvq_q6k_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu).expect("launch");
        dev.synchronize().expect("sync");

        let y_host = y_gpu.to_host().expect("download y");

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
            "mmvq_q6k diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 1e-2,
            "GPU mmvq_q6k diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
