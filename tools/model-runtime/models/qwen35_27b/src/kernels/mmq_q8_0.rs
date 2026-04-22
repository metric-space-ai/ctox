//! Q8_0 matrix-vector matmul (`mmvq`) — decode hot-path workhorse.
//!
//! Follows the conventions set by `mmq_q4k`: one Rust wrapper module
//! per `.cu` file, `OnceLock` caches per loaded `CudaFunction`, shape
//! validation up front, no stream synchronization.
//!
//! The byte-packed Q8_0 buffer is carried as a `CudaTensor<i8>` by
//! convention — the call site tracks the real `DType::Q8_0` out of
//! band. The total byte count must equal `(k / 32) * n * 34`.
//!
//! Note the smaller block width (32 elems) relative to the K-quants
//! (256 elems per block). Each Q8_0 block is just a fp16 super-scale
//! plus 32 signed int8 quants.
//!
//! Entry points:
//!   * `launch_mmvq_q8_0_f32` — `y: f32[n] = A_q8_0[n,k] · x[k]`
//!   * `launch_mmvq_q8_0_f16` — same but writes to an f16 output buffer.
//!
//! TODO: batched `mmq_q8_0` mat-mat variant (prefill path) — not ported
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

// PTX blob emitted by build.rs for kernels/mmq_q8_0.cu.
use super::MMQ_Q8_0_PTX;

/// Per-process caches for the loaded kernel functions. See `rmsnorm.rs`
/// for the multi-GPU caveat.
static MMVQ_Q8_0_F32_FN: OnceLock<CudaFunction> = OnceLock::new();
static MMVQ_Q8_0_F16_FN: OnceLock<CudaFunction> = OnceLock::new();

/// Bytes per Q8_0 block and logical elements per block (both fixed
/// by the GGUF format). Unlike the K-quants, Q8_0 is a 32-element
/// block format: 2-byte fp16 super-scale + 32 signed int8 quants.
const Q8_0_BLOCK_BYTES: usize = 34;
const Q8_0_BLOCK_ELEMS: usize = 32;

fn load_mmq_q8_0_fn(
    device: &Arc<DeviceContext>,
    cache: &'static OnceLock<CudaFunction>,
    sym: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cache.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(MMQ_Q8_0_PTX)
        .map_err(|e| anyhow!("mmq_q8_0.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module mmq_q8_0.ptx: {:?}", e))?;
    let f = module
        .load_function(sym)
        .map_err(|e| anyhow!("load_function {}: {:?}", sym, e))?;
    let _ = cache.set(f.clone());
    Ok(f)
}

/// Validate common shapes/sizes before we touch the kernel. `k` must
/// be a whole multiple of 32 (the Q8_0 block width).
fn validate_mmvq_q8_0_shapes<T, U>(
    a_q80: &CudaTensor<i8>,
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
        return Err(anyhow!("mmvq_q8_0: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q8_0_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q8_0: k must be a multiple of {} (got k={})",
            Q8_0_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q8_0_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q8_0_BLOCK_BYTES;
    if a_q80.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q8_0: a_q8_0 byte count {} != (k/32)*n*34 = {} (k={}, n={})",
            a_q80.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    let x_numel = x.numel();
    if x_numel != k {
        return Err(anyhow!(
            "mmvq_q8_0: x.numel()={} != k={}",
            x_numel,
            k
        ));
    }
    let y_numel = y.numel();
    if y_numel != n {
        return Err(anyhow!(
            "mmvq_q8_0: y.numel()={} != n={}",
            y_numel,
            n
        ));
    }
    Ok(())
}

/// `y[n] ← A_q8_0[n, k] · x[k]`, all in f32 on the host contract.
pub fn launch_mmvq_q8_0_f32(
    device: &Arc<DeviceContext>,
    a_q80: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    validate_mmvq_q8_0_shapes(a_q80, k, n, x, y)?;

    // grid.x = ceil(n/2): two output columns per block. Kernel guards
    // the out-of-range column when n is odd.
    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q8_0_fn(device, &MMVQ_Q8_0_F32_FN, "mmvq_q8_0_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q80.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q8_0_f32_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Slice/view variant of [`launch_mmvq_q8_0_f32`] — same kernel, but
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
pub fn launch_mmvq_q8_0_f32_view(
    device: &Arc<DeviceContext>,
    a_q80: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x_row: &CudaView<'_, f32>,
    y_row: &mut CudaViewMut<'_, f32>,
) -> Result<()> {
    // Inlined weight shape check — the CudaTensor-only validator wants
    // owned x/y tensors for their numel() assertions, which we don't
    // have here. Everything else lines up with validate_mmvq_q8_0_shapes().
    if k == 0 || n == 0 {
        return Err(anyhow!("mmvq_q8_0: k and n must be nonzero (k={}, n={})", k, n));
    }
    if !k.is_multiple_of(Q8_0_BLOCK_ELEMS) {
        return Err(anyhow!(
            "mmvq_q8_0: k must be a multiple of {} (got k={})",
            Q8_0_BLOCK_ELEMS,
            k
        ));
    }
    let blocks_per_col = k / Q8_0_BLOCK_ELEMS;
    let expected_bytes = blocks_per_col * n * Q8_0_BLOCK_BYTES;
    if a_q80.numel() != expected_bytes {
        return Err(anyhow!(
            "mmvq_q8_0: a_q8_0 byte count {} != (k/32)*n*34 = {} (k={}, n={})",
            a_q80.numel(),
            expected_bytes,
            k,
            n
        ));
    }
    if x_row.len() < k {
        return Err(anyhow!(
            "mmvq_q8_0: x_row view len {} < k={}",
            x_row.len(),
            k
        ));
    }
    if y_row.len() < n {
        return Err(anyhow!(
            "mmvq_q8_0: y_row view len {} < n={}",
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

    let f = load_mmq_q8_0_fn(device, &MMVQ_Q8_0_F32_FN, "mmvq_q8_0_f32_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q80.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x_row)
        .arg(y_row);

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q8_0_f32_view launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

/// Same as the f32 variant but writes an f16 output row (used when the
/// downstream op consumes half-precision activations).
pub fn launch_mmvq_q8_0_f16(
    device: &Arc<DeviceContext>,
    a_q80: &CudaTensor<i8>,
    k: usize,
    n: usize,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f16>,
) -> Result<()> {
    validate_mmvq_q8_0_shapes(a_q80, k, n, x, y)?;

    let grid_x = n.div_ceil(2) as u32;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, 1, 1),
        block_dim: (32, 2, 1),
        shared_mem_bytes: 0,
    };

    let f = load_mmq_q8_0_fn(device, &MMVQ_Q8_0_F16_FN, "mmvq_q8_0_f16_out")?;
    let stream = device.raw().default_stream();
    let k_i32 = k as i32;
    let n_i32 = n as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(a_q80.buf())
        .arg(&k_i32)
        .arg(&n_i32)
        .arg(x.buf())
        .arg(y.buf_mut());

    unsafe { launcher.launch(cfg) }
        .map_err(|e| anyhow!("mmvq_q8_0_f16_out launch (k={} n={}): {:?}", k, n, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use half::f16;

    /// Encode 32 f32 elements into a single 34-byte Q8_0 block.
    ///
    /// Mirrors `quantize_row_q8_0_ref`: super-scale `d = absmax / 127`,
    /// quants `q = clamp(round(v / d), -128, 127)`. Dequant inverts to
    /// `d * q` per element.
    fn encode_q8_0_block(vals: &[f32; 32], out: &mut [u8; 34]) {
        let mut absmax = 0.0f32;
        for &v in vals.iter() {
            if v.abs() > absmax {
                absmax = v.abs();
            }
        }
        let d = (absmax / 127.0).max(1e-8);
        let d_h = f16::from_f32(d).to_bits();
        out[0] = (d_h & 0xFF) as u8;
        out[1] = (d_h >> 8) as u8;
        // Use the f16-quantized d for the quant step so dequant
        // reconstructs these exact values (the kernel dequantizes
        // with the f16 scale, not the f32 original).
        let d_q = f16::from_bits(d_h).to_f32().max(1e-8);
        for (i, &v) in vals.iter().enumerate() {
            let q = (v / d_q).round().clamp(-128.0, 127.0) as i32;
            out[2 + i] = (q as i8) as u8;
        }
    }

    /// Reference CPU dequant matching `dequant_q8_0_to_bf16` in
    /// src/gguf.rs (modulo the bf16 rounding).
    fn dequant_q8_0_block(bytes: &[u8; 34], out: &mut [f32; 32]) {
        let d = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        for i in 0..32 {
            let q = bytes[2 + i] as i8 as f32;
            out[i] = d * q;
        }
    }

    /// End-to-end integration test against a CPU golden. Run with:
    ///   cargo test -p ctox-qwen35-27b --features cuda --release -- \
    ///       --ignored --nocapture mmvq_q8_0
    #[test]
    #[ignore]
    fn mmvq_q8_0_vs_cpu_golden() {
        let k = 4096usize;
        let n = 256usize;
        let blocks_per_col = k / 32;
        assert_eq!(blocks_per_col, 128);

        let mut seed: u32 = 0x9E3779B9;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let total_bytes = n * blocks_per_col * 34;
        let mut a_bytes = vec![0u8; total_bytes];
        let mut a_deq = vec![0.0f32; n * k];

        for col in 0..n {
            for b in 0..blocks_per_col {
                let mut vals = [0.0f32; 32];
                for v in vals.iter_mut() {
                    *v = rand_f() * 0.5;
                }
                let mut block = [0u8; 34];
                encode_q8_0_block(&vals, &mut block);
                let mut deq = [0.0f32; 32];
                dequant_q8_0_block(&block, &mut deq);
                a_bytes[(col * blocks_per_col + b) * 34
                    ..(col * blocks_per_col + b + 1) * 34]
                    .copy_from_slice(&block);
                a_deq[col * k + b * 32..col * k + (b + 1) * 32]
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
        .expect("upload a_q8_0");
        let x_gpu = CudaTensor::<f32>::from_host(dev.clone(), vec![k], &x_host)
            .expect("upload x");
        let mut y_gpu = CudaTensor::<f32>::zeros(dev.clone(), vec![n])
            .expect("alloc y");

        launch_mmvq_q8_0_f32(&dev, &a_gpu, k, n, &x_gpu, &mut y_gpu).expect("launch");
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
            "mmvq_q8_0 diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 5e-3,
            "GPU mmvq_q8_0 diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
