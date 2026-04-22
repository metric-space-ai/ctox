//! Token embedding lookup — Rust-side wrappers.
//!
//! Given a batch of token ids and the `[vocab_size, hidden_dim]`
//! embedding weight matrix, emit the corresponding rows into an
//! `[n_tokens, hidden_dim]` bf16 output. Three variants cover the
//! weight dtypes we see in GGUF-loaded embed tables: bf16, f16, f32.
//! The bf16→bf16 path is a bit-exact copy; the f16/f32 paths cast via
//! f32 per fetch.
//!
//! OOB ids (`id < 0 || id >= vocab_size`) zero-fill that row rather
//! than erroring — keeps the kernel branchless across threads and
//! avoids a host sync on the hot path. The caller is expected to
//! validate tokenization upstream; this is a safety net.
//!
//! Wrapper conventions mirror `rmsnorm` — see that module for the
//! canonical template.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::{bf16, f16};

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::EMBEDDING_PTX;

static EMBEDDING_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();
static EMBEDDING_F16_FN: OnceLock<CudaFunction> = OnceLock::new();
static EMBEDDING_F32_FN: OnceLock<CudaFunction> = OnceLock::new();

fn load_embedding_fn(
    device: &Arc<DeviceContext>,
    cell: &'static OnceLock<CudaFunction>,
    entry: &'static str,
) -> Result<CudaFunction> {
    if let Some(f) = cell.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(EMBEDDING_PTX)
        .map_err(|e| anyhow!("embedding.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module embedding.ptx: {:?}", e))?;
    let f = module
        .load_function(entry)
        .map_err(|e| anyhow!("load_function {}: {:?}", entry, e))?;
    let _ = cell.set(f.clone());
    Ok(f)
}

/// Validate shapes common to every embedding variant. Returns
/// `(n_tokens, vocab_size, hidden_dim)` on success.
fn validate_shapes(
    weight_shape: &[usize],
    token_ids_shape: &[usize],
    out_shape: &[usize],
) -> Result<(usize, usize, usize)> {
    if weight_shape.len() != 2 {
        return Err(anyhow!(
            "embedding: weight must be 2D [vocab_size, hidden_dim], got {:?}",
            weight_shape
        ));
    }
    if token_ids_shape.len() != 1 {
        return Err(anyhow!(
            "embedding: token_ids must be 1D [n_tokens], got {:?}",
            token_ids_shape
        ));
    }
    if out_shape.len() != 2 {
        return Err(anyhow!(
            "embedding: out must be 2D [n_tokens, hidden_dim], got {:?}",
            out_shape
        ));
    }
    let vocab_size = weight_shape[0];
    let hidden_dim = weight_shape[1];
    let n_tokens = token_ids_shape[0];
    if out_shape[0] != n_tokens || out_shape[1] != hidden_dim {
        return Err(anyhow!(
            "embedding: out shape {:?} != [{}, {}] (n_tokens × hidden_dim)",
            out_shape,
            n_tokens,
            hidden_dim
        ));
    }
    Ok((n_tokens, vocab_size, hidden_dim))
}

/// Launch config matches the rmsnorm template: one block per output
/// token, block_dim = min(hidden_dim, 1024) rounded up to a warp.
fn embedding_launch_config(n_tokens: usize, hidden_dim: usize) -> LaunchConfig {
    let mut block_dim = hidden_dim.min(1024);
    block_dim = block_dim.div_ceil(32) * 32;
    LaunchConfig {
        grid_dim: (n_tokens as u32, 1, 1),
        block_dim: (block_dim as u32, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// `out[t, :] = weight[token_ids[t], :]` — bf16 weight, bf16 out.
///
/// OOB ids zero-fill the corresponding output row. Does not
/// synchronize the stream.
pub fn launch_embedding_bf16(
    device: &Arc<DeviceContext>,
    weight: &CudaTensor<bf16>,
    token_ids: &CudaTensor<i32>,
    out: &mut CudaTensor<bf16>,
) -> Result<()> {
    let (n_tokens, vocab_size, hidden_dim) =
        validate_shapes(weight.shape(), token_ids.shape(), out.shape())?;
    if n_tokens == 0 || hidden_dim == 0 {
        return Ok(());
    }

    let cfg = embedding_launch_config(n_tokens, hidden_dim);
    let f = load_embedding_fn(device, &EMBEDDING_BF16_FN, "embedding_bf16")?;
    let stream = device.raw().default_stream();
    let vocab_size_i32 = vocab_size as i32;
    let hidden_dim_i32 = hidden_dim as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&vocab_size_i32)
        .arg(&hidden_dim_i32);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "embedding_bf16 launch (n_tokens={} vocab={} hidden={}): {:?}",
            n_tokens,
            vocab_size,
            hidden_dim,
            e
        )
    })?;
    Ok(())
}

/// f16 weight → bf16 out. Cast happens per fetch on the device.
pub fn launch_embedding_f16(
    device: &Arc<DeviceContext>,
    weight: &CudaTensor<f16>,
    token_ids: &CudaTensor<i32>,
    out: &mut CudaTensor<bf16>,
) -> Result<()> {
    let (n_tokens, vocab_size, hidden_dim) =
        validate_shapes(weight.shape(), token_ids.shape(), out.shape())?;
    if n_tokens == 0 || hidden_dim == 0 {
        return Ok(());
    }

    let cfg = embedding_launch_config(n_tokens, hidden_dim);
    let f = load_embedding_fn(device, &EMBEDDING_F16_FN, "embedding_f16")?;
    let stream = device.raw().default_stream();
    let vocab_size_i32 = vocab_size as i32;
    let hidden_dim_i32 = hidden_dim as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&vocab_size_i32)
        .arg(&hidden_dim_i32);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "embedding_f16 launch (n_tokens={} vocab={} hidden={}): {:?}",
            n_tokens,
            vocab_size,
            hidden_dim,
            e
        )
    })?;
    Ok(())
}

/// f32 weight → bf16 out. Cast happens per fetch on the device.
pub fn launch_embedding_f32(
    device: &Arc<DeviceContext>,
    weight: &CudaTensor<f32>,
    token_ids: &CudaTensor<i32>,
    out: &mut CudaTensor<bf16>,
) -> Result<()> {
    let (n_tokens, vocab_size, hidden_dim) =
        validate_shapes(weight.shape(), token_ids.shape(), out.shape())?;
    if n_tokens == 0 || hidden_dim == 0 {
        return Ok(());
    }

    let cfg = embedding_launch_config(n_tokens, hidden_dim);
    let f = load_embedding_fn(device, &EMBEDDING_F32_FN, "embedding_f32")?;
    let stream = device.raw().default_stream();
    let vocab_size_i32 = vocab_size as i32;
    let hidden_dim_i32 = hidden_dim as i32;
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&vocab_size_i32)
        .arg(&hidden_dim_i32);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "embedding_f32 launch (n_tokens={} vocab={} hidden={}): {:?}",
            n_tokens,
            vocab_size,
            hidden_dim,
            e
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full-scale sanity test: 151936 × 5120 bf16 embed table (Qwen3.5
    /// dimensions), 32 random token ids, one OOB id to exercise the
    /// zero-fill branch. bf16→bf16 path is a bit-exact gather so
    /// tolerance is 0.
    #[test]
    #[ignore]
    fn embedding_vs_cpu_golden() {
        let vocab_size: usize = 151936;
        let hidden_dim: usize = 5120;
        let n_tokens: usize = 32;

        // Deterministic pseudo-random weight table — LCG seeded so the
        // test is host-independent. We build the bf16 table directly
        // via round-trip from f32.
        let mut seed: u32 = 0xDEADBEEF;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let numel = vocab_size * hidden_dim;
        // 151936 × 5120 × 2 bytes ≈ 1.5 GiB — fine on an A6000 (48 GB)
        // but we don't want to populate the host-side vec with a Vec
        // iterator closure trip; write directly into a boxed slice.
        let mut w_host: Vec<bf16> = Vec::with_capacity(numel);
        for _ in 0..numel {
            w_host.push(bf16::from_f32(rand_f()));
        }

        // Token ids: 31 in-range + one intentionally OOB (-1) to
        // exercise the zero-fill guard.
        let mut token_ids: Vec<i32> = Vec::with_capacity(n_tokens);
        let mut id_seed: u32 = 0xB16B00B5;
        for k in 0..n_tokens {
            if k == 7 {
                token_ids.push(-1); // OOB — expect zero row.
            } else {
                id_seed = id_seed.wrapping_mul(1103515245).wrapping_add(12345);
                let id = (id_seed as usize) % vocab_size;
                token_ids.push(id as i32);
            }
        }

        // CPU golden. bf16→bf16 gather; OOB → zero row.
        let zero = bf16::from_f32(0.0);
        let mut out_cpu: Vec<bf16> = vec![zero; n_tokens * hidden_dim];
        for (t, &id) in token_ids.iter().enumerate() {
            if id < 0 || (id as usize) >= vocab_size {
                // already zero
                continue;
            }
            let src = &w_host[(id as usize) * hidden_dim..(id as usize + 1) * hidden_dim];
            let dst = &mut out_cpu[t * hidden_dim..(t + 1) * hidden_dim];
            dst.copy_from_slice(src);
        }

        // Device run.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let w = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![vocab_size, hidden_dim],
            &w_host,
        )
        .expect("upload weight");
        let ids = CudaTensor::<i32>::from_host(dev.clone(), vec![n_tokens], &token_ids)
            .expect("upload token_ids");
        let mut out = CudaTensor::<bf16>::zeros(dev.clone(), vec![n_tokens, hidden_dim])
            .expect("alloc out");

        launch_embedding_bf16(&dev, &w, &ids, &mut out).expect("launch");
        dev.synchronize().expect("sync");

        let out_gpu: Vec<bf16> = out.to_host().expect("download out");

        // bf16→bf16 copy must be bit-exact. Compare via f32 view but
        // expect max_abs == 0.
        let mut max_abs = 0.0f32;
        let mut mismatches = 0usize;
        for (a, b) in out_cpu.iter().zip(out_gpu.iter()) {
            let af = a.to_f32();
            let bf = b.to_f32();
            let d = (af - bf).abs();
            if d > max_abs {
                max_abs = d;
            }
            if a.to_bits() != b.to_bits() {
                mismatches += 1;
            }
        }
        eprintln!(
            "embedding diff: max_abs={:.6e} mismatches={}/{}",
            max_abs,
            mismatches,
            out_cpu.len()
        );
        assert_eq!(
            max_abs, 0.0,
            "bf16→bf16 embedding gather is not bit-exact: max_abs={}",
            max_abs
        );
        assert_eq!(mismatches, 0, "bf16→bf16 embedding gather has bit-level mismatches");
    }
}
