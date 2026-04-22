//! Token embedding lookup — Rust wrappers around the vendored
//! `k_get_rows_float<src_t, dst_t>` template from
//! llama.cpp's ggml-cuda/getrows.cu.
//!
//! The vendor kernel is a general 3-axis row-gather with strided src
//! and indirection tensors. Our use case is the degenerate 1-axis path:
//! `out[t, :] = weight[token_ids[t], :]` where `weight` is
//! `[vocab_size, hidden_dim]`, `token_ids` is `[n_tokens]`, and `out`
//! is `[n_tokens, hidden_dim]`. Mapping to upstream's parameter
//! convention: `ne00=hidden_dim, ne10=n_tokens, ne11=ne12=1`, with the
//! unused axis strides set to 0.
//!
//! Three specializations are exposed by the shim
//! `kernels/sm_86/getrows.cu`:
//!
//!   * `k_get_rows_float<__nv_bfloat16, __nv_bfloat16>` — bf16→bf16 (bit-exact).
//!   * `k_get_rows_float<__half,        __nv_bfloat16>` — f16 →bf16 (per-element cast).
//!   * `k_get_rows_float<float,         __nv_bfloat16>` — f32 →bf16 (per-element cast).
//!
//! The vendor doesn't emit an OOB guard — out-of-range token ids
//! produce undefined reads. The caller is expected to have validated
//! ids upstream; the old self-authored kernel zero-filled OOB rows as
//! a safety net, but the tests use only in-range ids, and the hot
//! path has the upstream-validated tokenizer feeding this kernel.
//!
//! Public API (`launch_embedding_{bf16,f16,f32}`) is unchanged — the
//! callers in layers/*.rs don't see the re-wire.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::{bf16, f16};

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

// PTX blob comes from the parent module's auto-generated registry.
// The .cu file is named `getrows.cu` (a shim that `#include`s the
// vendored upstream ggml-cuda/getrows.cu and forces PTX emission of
// the three template specializations we call).
use super::GETROWS_PTX;

/// Matches upstream's `CUDA_GET_ROWS_BLOCK_SIZE` (see
/// `vendor/ggml-cuda/getrows.cuh`). Hard-coded rather than imported
/// because the constant lives in a `.cuh` we don't expose to Rust.
const CUDA_GET_ROWS_BLOCK_SIZE: u32 = 256;

// C++-mangled entry points emitted by nvcc for the three template
// specializations we force in the shim. Inspect:
//   nvcc --ptx ... kernels/sm_86/getrows.cu -o /tmp/getrows.ptx
//   grep '.visible .entry' /tmp/getrows.ptx
const ENTRY_BF16: &str =
    "_Z16k_get_rows_floatI13__nv_bfloat16S0_EvPKT_PKiPT0_lllmmmmmmmmm";
const ENTRY_F16: &str =
    "_Z16k_get_rows_floatI6__half13__nv_bfloat16EvPKT_PKiPT0_lllmmmmmmmmm";
const ENTRY_F32: &str =
    "_Z16k_get_rows_floatIf13__nv_bfloat16EvPKT_PKiPT0_lllmmmmmmmmm";

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
    let ptx_src = std::str::from_utf8(GETROWS_PTX)
        .map_err(|e| anyhow!("getrows.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module getrows.ptx: {:?}", e))?;
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

/// Launch geometry matches upstream `get_rows_cuda_float`:
///
///   block = (CUDA_GET_ROWS_BLOCK_SIZE, 1, 1)
///   grid  = (ne10, min(block_num_y, UINT16_MAX), min(ne11*ne12, UINT16_MAX))
///   block_num_y = ceil(ne00 / CUDA_GET_ROWS_BLOCK_SIZE)
///
/// In our 2D mapping: ne10 = n_tokens, ne00 = hidden_dim, ne11*ne12 = 1.
fn embedding_launch_config(n_tokens: usize, hidden_dim: usize) -> LaunchConfig {
    let block_num_y = (hidden_dim as u32)
        .div_ceil(CUDA_GET_ROWS_BLOCK_SIZE)
        .max(1);
    let block_num_y = block_num_y.min(u16::MAX as u32);
    LaunchConfig {
        grid_dim: (n_tokens as u32, block_num_y, 1),
        block_dim: (CUDA_GET_ROWS_BLOCK_SIZE, 1, 1),
        shared_mem_bytes: 0,
    }
}

/// Push the 15-scalar parameter tail that `k_get_rows_float` expects.
/// See vendor getrows.cu for the full list; the 2D special-casing sets
/// the unused higher-axis strides to 0.
///
/// Layout (after src0/src1/dst pointer args):
///   ne00, ne11, ne12,     — src0 row len, index grid axes (we use 1,1)
///   s1, s2, s3,           — out strides in ELEMENTS (s1 = hidden_dim)
///   nb01, nb02, nb03,     — weight strides in BYTES (nb01 = row bytes)
///   s10, s11, s12         — token_ids strides in ELEMENTS (s10 = 1)
///
/// We push s2, s3, nb02, nb03, s11, s12 as zero since ne11=ne12=1.
struct GetRowsArgs {
    ne00: i64,
    ne11: i64,
    ne12: i64,
    s1: usize,
    s2: usize,
    s3: usize,
    nb01: usize,
    nb02: usize,
    nb03: usize,
    s10: usize,
    s11: usize,
    s12: usize,
}

impl GetRowsArgs {
    fn for_2d<T>(n_tokens: usize, hidden_dim: usize) -> Self {
        let elem_bytes = std::mem::size_of::<T>();
        Self {
            ne00: hidden_dim as i64,
            ne11: 1,
            ne12: 1,
            s1: hidden_dim,           // bf16 out row stride = hidden_dim elements
            s2: 0,                    // ne11 = 1 → axis unused
            s3: 0,                    // ne12 = 1 → axis unused
            nb01: hidden_dim * elem_bytes, // weight row stride in bytes
            nb02: 0,
            nb03: 0,
            s10: 1,                   // token_ids contiguous
            s11: 0,
            s12: 0,
        }
        .discard_unused(n_tokens)
    }

    fn discard_unused(self, _n_tokens: usize) -> Self {
        self
    }
}

/// `out[t, :] = weight[token_ids[t], :]` — bf16 weight, bf16 out.
///
/// Does not synchronize the stream. Caller syncs at phase boundary.
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
    let f = load_embedding_fn(device, &EMBEDDING_BF16_FN, ENTRY_BF16)?;
    let stream = device.raw().default_stream();

    let args = GetRowsArgs::for_2d::<bf16>(n_tokens, hidden_dim);
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&args.ne00)
        .arg(&args.ne11)
        .arg(&args.ne12)
        .arg(&args.s1)
        .arg(&args.s2)
        .arg(&args.s3)
        .arg(&args.nb01)
        .arg(&args.nb02)
        .arg(&args.nb03)
        .arg(&args.s10)
        .arg(&args.s11)
        .arg(&args.s12);

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

/// f16 weight → bf16 out. Cast happens per fetch on the device via
/// upstream `ggml_cuda_cast`.
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
    let f = load_embedding_fn(device, &EMBEDDING_F16_FN, ENTRY_F16)?;
    let stream = device.raw().default_stream();

    let args = GetRowsArgs::for_2d::<f16>(n_tokens, hidden_dim);
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&args.ne00)
        .arg(&args.ne11)
        .arg(&args.ne12)
        .arg(&args.s1)
        .arg(&args.s2)
        .arg(&args.s3)
        .arg(&args.nb01)
        .arg(&args.nb02)
        .arg(&args.nb03)
        .arg(&args.s10)
        .arg(&args.s11)
        .arg(&args.s12);

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
    let f = load_embedding_fn(device, &EMBEDDING_F32_FN, ENTRY_F32)?;
    let stream = device.raw().default_stream();

    let args = GetRowsArgs::for_2d::<f32>(n_tokens, hidden_dim);
    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(weight.buf())
        .arg(token_ids.buf())
        .arg(out.buf_mut())
        .arg(&args.ne00)
        .arg(&args.ne11)
        .arg(&args.ne12)
        .arg(&args.s1)
        .arg(&args.s2)
        .arg(&args.s3)
        .arg(&args.nb01)
        .arg(&args.nb02)
        .arg(&args.nb03)
        .arg(&args.s10)
        .arg(&args.s11)
        .arg(&args.s12);

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
    /// dimensions), 32 random in-range token ids. bf16→bf16 is a
    /// bit-exact strided gather so tolerance is 0.
    ///
    /// NOTE: the vendor kernel does NOT zero-fill OOB ids (unlike the
    /// old self-authored kernel); the test uses only in-range ids, and
    /// the production caller upstream-validates tokens.
    #[test]
    #[ignore]
    fn embedding_vs_cpu_golden() {
        let vocab_size: usize = 151936;
        let hidden_dim: usize = 5120;
        let n_tokens: usize = 32;

        let mut seed: u32 = 0xDEADBEEF;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };

        let numel = vocab_size * hidden_dim;
        let mut w_host: Vec<bf16> = Vec::with_capacity(numel);
        for _ in 0..numel {
            w_host.push(bf16::from_f32(rand_f()));
        }

        // All in-range ids — vendor kernel has no OOB guard.
        let mut token_ids: Vec<i32> = Vec::with_capacity(n_tokens);
        let mut id_seed: u32 = 0xB16B00B5;
        for _ in 0..n_tokens {
            id_seed = id_seed.wrapping_mul(1103515245).wrapping_add(12345);
            let id = (id_seed as usize) % vocab_size;
            token_ids.push(id as i32);
        }

        // CPU golden: bf16→bf16 gather.
        let zero = bf16::from_f32(0.0);
        let mut out_cpu: Vec<bf16> = vec![zero; n_tokens * hidden_dim];
        for (t, &id) in token_ids.iter().enumerate() {
            let src = &w_host[(id as usize) * hidden_dim..(id as usize + 1) * hidden_dim];
            let dst = &mut out_cpu[t * hidden_dim..(t + 1) * hidden_dim];
            dst.copy_from_slice(src);
        }

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
