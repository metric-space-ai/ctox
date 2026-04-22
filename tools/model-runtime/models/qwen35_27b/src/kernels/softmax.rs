//! softmax — numerically stable row softmax over f32 tensors.
//!
//! Backed by the vendored upstream llama.cpp kernel
//! `soft_max_f32<use_shared=false, ncols_template=0, block_size_template=0,
//! T=float>` (see `kernels/sm_86/softmax.cu` for the shim that pulls the
//! vendored TU into the PTX registry).
//!
//! Our call-sites use softmax without ALiBi, without mask, without sinks,
//! and without scale. Those are all "off-path" branches in the upstream
//! kernel: mask=nullptr short-circuits the mask dereference; sinks=nullptr
//! skips the sinks-exp term; scale=1.0 and max_bias=0.0 collapse the
//! scale/slope multiplies. What remains is a plain row-softmax identical
//! to the previous self-authored version, but with the vendored warp
//! reduction and loop unrolls.
//!
//! Public signature unchanged from the self-authored version:
//! `launch_softmax_f32(device, x, y)` — `[n_rows, n_cols]` f32 in/out,
//! no synchronization.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::SOFTMAX_PTX;

// Mangled name of the `soft_max_f32<false, 0, 0, float>` specialization
// emitted by the vendored softmax TU. Verified in the compiled
// `softmax.ptx`. Exact string is the Itanium C++ ABI mangling of:
//   soft_max_f32<(bool)false, 0, 0, float>(
//       const float*, const float*, const float*, float*, soft_max_params)
const SOFT_MAX_F32_SYM: &str = "_Z12soft_max_f32ILb0ELi0ELi0EfEvPKfPKT2_S1_Pf15soft_max_params";

static SOFTMAX_F32_FN: OnceLock<CudaFunction> = OnceLock::new();

fn softmax_f32_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = SOFTMAX_F32_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(SOFTMAX_PTX)
        .map_err(|e| anyhow!("softmax.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module softmax.ptx: {:?}", e))?;
    let f = module
        .load_function(SOFT_MAX_F32_SYM)
        .map_err(|e| anyhow!("load_function {}: {:?}", SOFT_MAX_F32_SYM, e))?;
    let _ = SOFTMAX_F32_FN.set(f.clone());
    Ok(f)
}

/// Rust mirror of the upstream `soft_max_params` struct.
///
/// Field order, types, and natural alignment must exactly match the CUDA
/// side — the kernel receives this struct by value and indexes fields by
/// offset. The C++ layout inserts 4 bytes of tail padding after
/// `n_head_log2` (u32) before the next i64; `#[repr(C)]` on x86-64 / sm_8x
/// produces the same layout. Total size = 8*11 + 4 + 4 + 4*4 = 108 aligned
/// to 120 bytes (one trailing 4-byte pad after `m1` to align to 8 is NOT
/// required since there's no subsequent i64 — CUDA ABI treats the end as-is
/// but we don't care about sizeof here, only field offsets).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SoftMaxParams {
    nheads: i64,
    n_head_log2: u32,
    // 4-byte tail-align pad inserted by #[repr(C)] so `ncols` is 8-aligned.
    _pad0: u32,
    ncols: i64,
    nrows_x: i64,
    nrows_y: i64,
    ne00: i64,
    ne01: i64,
    ne02: i64,
    ne03: i64,
    nb11: i64,
    nb12: i64,
    nb13: i64,
    ne12: i64,
    ne13: i64,
    scale: f32,
    max_bias: f32,
    m0: f32,
    m1: f32,
}

// Safety: plain-old-data, all fields trivially Pod. Needed so cudarc's
// `PushKernelArg` can treat it as a kernel argument.
unsafe impl cudarc::driver::DeviceRepr for SoftMaxParams {}

/// `y[r, :] = softmax(x[r, :])` for every row.
///
/// Shapes:
///   * `x`, `y`: `[n_rows, n_cols]` f32 row-major
///
/// Does not synchronize the stream.
pub fn launch_softmax_f32(
    device: &Arc<DeviceContext>,
    x: &CudaTensor<f32>,
    y: &mut CudaTensor<f32>,
) -> Result<()> {
    if x.shape().len() != 2 {
        return Err(anyhow!(
            "softmax: x must be 2D [n_rows, n_cols], got {:?}",
            x.shape()
        ));
    }
    if y.shape() != x.shape() {
        return Err(anyhow!(
            "softmax: y.shape {:?} != x.shape {:?}",
            y.shape(),
            x.shape()
        ));
    }
    let n_rows = x.shape()[0];
    let n_cols = x.shape()[1];
    if n_rows == 0 || n_cols == 0 {
        return Ok(());
    }

    // Block-dim selection mirrors upstream `soft_max_f32_cuda`: start at
    // WARP_SIZE=32 and double until we cover ncols or hit
    // CUDA_SOFT_MAX_BLOCK_SIZE=1024.
    let mut nth: u32 = 32;
    while (nth as usize) < n_cols && nth < 1024 {
        nth *= 2;
    }
    let cfg = LaunchConfig {
        grid_dim: (n_rows as u32, 1, 1),
        block_dim: (nth, 1, 1),
        // With use_shared=false, vals is aliased onto dst; the only shmem
        // the kernel needs is WARP_SIZE floats for the inter-warp reduction
        // buffer `buf_iw`. Upstream calls this `nbytes_shared_low`.
        shared_mem_bytes: 32 * std::mem::size_of::<f32>() as u32,
    };

    // Build params. Mask/sinks paths are off — nb11/nb12/nb13 would divide
    // `sizeof(T)` with T=float in the address computation for `mask`, but
    // since mask=null the result gets multiplied by `(mask != nullptr)`
    // (=0) in the upstream kernel, so values are don't-cares. We set them
    // to 1 (float units) just to keep the shape coherent.
    let params = SoftMaxParams {
        nheads: 1,
        n_head_log2: 1, // log2(1) == 0 — only used in get_alibi_slope with max_bias=0, which early-returns.
        _pad0: 0,
        ncols: n_cols as i64,
        nrows_x: n_rows as i64,
        nrows_y: n_rows as i64,
        ne00: n_cols as i64,
        ne01: n_rows as i64,
        ne02: 1,
        ne03: 1,
        nb11: 1,
        nb12: 1,
        nb13: 1,
        ne12: 1,
        ne13: 1,
        scale: 1.0,
        max_bias: 0.0,
        m0: 1.0,
        m1: 1.0,
    };

    let f = softmax_f32_fn(device)?;
    let stream = device.raw().default_stream();

    // Null device pointers for `mask` and `sinks`. Upstream guards both
    // with pointer-null checks before any dereference.
    let null_ptr: u64 = 0;

    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(x.buf())
        .arg(&null_ptr) // const T* mask = nullptr
        .arg(&null_ptr) // const float* sinks = nullptr
        .arg(y.buf_mut())
        .arg(&params);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "soft_max_f32 launch (n_rows={} n_cols={} nth={}): {:?}",
            n_rows,
            n_cols,
            nth,
            e
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn softmax_cpu(x: &[f32], y: &mut [f32], n_cols: usize) {
        let n_rows = x.len() / n_cols;
        for r in 0..n_rows {
            let row_x = &x[r * n_cols..(r + 1) * n_cols];
            let row_y = &mut y[r * n_cols..(r + 1) * n_cols];
            let mut m = f32::NEG_INFINITY;
            for &v in row_x {
                if v > m {
                    m = v;
                }
            }
            let mut sum = 0.0f32;
            for (i, &v) in row_x.iter().enumerate() {
                let e = (v - m).exp();
                row_y[i] = e;
                sum += e;
            }
            let inv = 1.0 / sum;
            for v in row_y.iter_mut() {
                *v *= inv;
            }
        }
    }

    /// Runs on A6000 CI only. `cargo test --features cuda -- --ignored softmax_vs_cpu_golden`.
    #[test]
    #[ignore]
    fn softmax_vs_cpu_golden() {
        // Qwen3.5 vocab = 151936. 32 rows exercises both multi-warp
        // fan-in and heavy column stride per thread.
        let n_rows = 32usize;
        let n_cols = 151936usize;

        let mut seed: u32 = 0xDEADBEEF;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            // Pre-softmax logits are typically in [-10, 10] range. Push
            // beyond that to stress max-subtract stability.
            ((seed >> 16) as f32 / 32768.0 - 1.0) * 15.0
        };
        let x_host: Vec<f32> = (0..n_rows * n_cols).map(|_| rand_f()).collect();

        let mut y_cpu = vec![0.0f32; n_rows * n_cols];
        softmax_cpu(&x_host, &mut y_cpu, n_cols);

        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let x = CudaTensor::<f32>::from_host(dev.clone(), vec![n_rows, n_cols], &x_host)
            .expect("upload x");
        let mut y = CudaTensor::<f32>::zeros(dev.clone(), vec![n_rows, n_cols])
            .expect("alloc y");

        launch_softmax_f32(&dev, &x, &mut y).expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu = y.to_host().expect("download y");

        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu.iter().zip(y_gpu.iter()) {
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
            "softmax diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        assert!(
            max_rel < 1e-4,
            "GPU softmax diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
