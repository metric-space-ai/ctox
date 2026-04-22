//! MRoPE — Qwen3.5 4-axis Multi-axis Rotary Position Embedding on bf16
//! Q/K tensors. In-place rotation.
//!
//! Backed by the vendored upstream llama.cpp kernel
//! `rope_multi<forward=true, has_ff=false, T=__nv_bfloat16>` (see
//! `kernels/sm_86/rope.cu` for the verbatim-upstream copy with the
//! PTX-emission sink). Our call-sites pass `freq_scale=1.0`,
//! `ext_factor=0.0` (no YaRN), `attn_factor=1.0`, `freq_factors=nullptr`,
//! `is_imrope=false` — those collapse upstream's YaRN/IMRoPE branches away
//! and leave a plain 4-axis MRoPE identical to the previous self-authored
//! implementation.
//!
//! Public signature `launch_rope_mrope_bf16(device, qk, positions,
//! theta_base, rope_dim)` is preserved — call-sites in `layers/*.rs`
//! continue to work unchanged.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use cudarc::driver::{CudaFunction, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;
use half::bf16;

use ctox_cuda_primitives::device::DeviceContext;
use ctox_cuda_primitives::tensor::CudaTensor;

use super::ROPE_PTX;

// Mangled name of the `rope_multi<true, false, __nv_bfloat16>`
// specialization emitted by the vendored rope.cu. Verified in the compiled
// `rope.ptx`. Itanium C++ ABI mangling of:
//   rope_multi<(bool)true, (bool)false, __nv_bfloat16>(
//       const __nv_bfloat16*, __nv_bfloat16*,
//       int, int, int, int, int, int, int, int, int, int,
//       const int32_t*, float, float, float,
//       rope_corr_dims, float,
//       const float*, mrope_sections, bool)
const ROPE_MULTI_BF16_SYM: &str =
    "_Z10rope_multiILb1ELb0E13__nv_bfloat16EvPKT1_PS1_iiiiiiiiiiPKifff14rope_corr_dimsfPKf14mrope_sectionsb";

static ROPE_MROPE_BF16_FN: OnceLock<CudaFunction> = OnceLock::new();

fn rope_mrope_bf16_fn(device: &Arc<DeviceContext>) -> Result<CudaFunction> {
    if let Some(f) = ROPE_MROPE_BF16_FN.get() {
        return Ok(f.clone());
    }
    let ptx_src = std::str::from_utf8(ROPE_PTX)
        .map_err(|e| anyhow!("rope.ptx not UTF-8: {}", e))?
        .to_string();
    let module = device
        .raw()
        .load_module(Ptx::from_src(ptx_src))
        .map_err(|e| anyhow!("load_module rope.ptx: {:?}", e))?;
    let f = module
        .load_function(ROPE_MULTI_BF16_SYM)
        .map_err(|e| anyhow!("load_function {}: {:?}", ROPE_MULTI_BF16_SYM, e))?;
    let _ = ROPE_MROPE_BF16_FN.set(f.clone());
    Ok(f)
}

/// Rust mirror of upstream `rope_corr_dims` (2 floats). Used only when
/// `ext_factor != 0.0` for YaRN; we always pass `ext_factor=0.0` so these
/// values are don't-cares, but the struct must be present in the kernel
/// ABI. Upstream layout: `struct rope_corr_dims { float v[2]; };`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RopeCorrDims {
    v: [f32; 2],
}
unsafe impl cudarc::driver::DeviceRepr for RopeCorrDims {}

/// Rust mirror of upstream `mrope_sections` (4 ints). Defines how the
/// head_dim pairs are partitioned across the 4 position axes. For
/// Qwen3.5's plain MRoPE we set each section to `rope_dim / 8` (equal
/// quarter-splits of `rope_dim/2` pairs), so pair index `p` maps to
/// axis `p / (rope_dim/8)`. This reproduces the self-authored kernel's
/// axis-sectioning exactly.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct MRopeSections {
    v: [i32; 4],
}
unsafe impl cudarc::driver::DeviceRepr for MRopeSections {}

/// Apply MRoPE in place to a Q- or K-tensor.
///
/// Shapes:
///   * `qk`:        `[n_tokens, n_heads, head_dim]` bf16 (in-place)
///   * `positions`: `[4, n_tokens]`                 i32
///
/// `rope_dim` is the number of leading dims per head to rotate (must be
/// even and ≤ head_dim). `theta_base` is the RoPE base (10000.0 for
/// Qwen3.5). 4-axis: section s of the rotated range uses
/// `positions[s, token]`.
///
/// Does not synchronize the stream.
pub fn launch_rope_mrope_bf16(
    device: &Arc<DeviceContext>,
    qk: &mut CudaTensor<bf16>,
    positions: &CudaTensor<i32>,
    theta_base: f32,
    rope_dim: i32,
) -> Result<()> {
    if qk.shape().len() != 3 {
        return Err(anyhow!(
            "rope_mrope: qk must be 3D [n_tokens, n_heads, head_dim], got {:?}",
            qk.shape()
        ));
    }
    let n_tokens = qk.shape()[0];
    let n_heads = qk.shape()[1];
    let head_dim = qk.shape()[2];

    if positions.shape().len() != 2 || positions.shape()[0] != 4 || positions.shape()[1] != n_tokens
    {
        return Err(anyhow!(
            "rope_mrope: positions must be [4, {}], got {:?}",
            n_tokens,
            positions.shape()
        ));
    }
    if rope_dim <= 0 {
        return Err(anyhow!(
            "rope_mrope: rope_dim must be positive, got {}",
            rope_dim
        ));
    }
    if rope_dim as usize > head_dim {
        return Err(anyhow!(
            "rope_mrope: rope_dim {} > head_dim {}",
            rope_dim,
            head_dim
        ));
    }
    if rope_dim % 2 != 0 {
        return Err(anyhow!("rope_mrope: rope_dim {} not even", rope_dim));
    }
    if head_dim % 2 != 0 {
        return Err(anyhow!("rope_mrope: head_dim {} not even", head_dim));
    }
    if n_tokens == 0 || n_heads == 0 || head_dim == 0 {
        return Ok(());
    }

    // Upstream ggml index convention: ne00 = fastest/innermost dim.
    // Our row-major [n_tokens, n_heads, head_dim] maps to:
    //   ne00 = head_dim, ne01 = n_heads, ne02 = n_tokens, ne03 = 1.
    let ne00 = head_dim as i32;
    let ne01 = n_heads as i32;
    let ne02 = n_tokens as i32;
    // nr = "number of rows" = ne01*ne02*ne03 = count of head-vectors.
    let nr = (n_heads * n_tokens) as i32;

    // Strides in element units (upstream divides bytes by ggml_type_size).
    let s01 = ne00; // step between heads within a token
    let s02 = ne00 * ne01; // step between tokens
    let s03 = ne00 * ne01 * ne02; // step between batch (unused, ne03=1)
    let s1 = s01;
    let s2 = s02;
    let s3 = s03;

    // Block / grid layout mirrors upstream rope_multi_cuda:
    //   block_dims = (1, CUDA_ROPE_BLOCK_SIZE, 1)  — 256 threads along y.
    //   n_blocks_x = ceil(ne00 / (2*256))          — one pair per thread.
    //   block_nums = (nr, n_blocks_x, 1).
    const ROPE_BLOCK: u32 = 256;
    let n_blocks_y = ((ne00 as u32) + 2 * ROPE_BLOCK - 1) / (2 * ROPE_BLOCK);
    let cfg = LaunchConfig {
        grid_dim: (nr as u32, n_blocks_y, 1),
        block_dim: (1, ROPE_BLOCK, 1),
        shared_mem_bytes: 0,
    };

    // theta_scale = theta_base^(-2/n_dims). Upstream precomputes this in
    // rope_multi_cuda before launching the kernel.
    let n_dims = rope_dim; // we rotate the full leading rope_dim of ne00
    let theta_scale = theta_base.powf(-2.0 / n_dims as f32);

    // mrope_sections: split rope_dim/2 pairs evenly across 4 axes so
    // pair index p maps to axis `p / (rope_dim/8)`. Matches the
    // self-authored kernel's sectioning AND the CPU reference in tests.
    let section_size = rope_dim / 8;
    let sections = MRopeSections {
        v: [section_size, section_size, section_size, section_size],
    };

    // No YaRN. corr_dims only consulted when ext_factor != 0.
    let corr_dims = RopeCorrDims { v: [0.0, 0.0] };
    let freq_scale = 1.0f32;
    let ext_factor = 0.0f32;
    let attn_factor = 1.0f32;
    let is_imrope = false;

    // has_ff=false specialization means `freq_factors` pointer is never
    // dereferenced — pass null.
    let null_ptr: u64 = 0;

    let f = rope_mrope_bf16_fn(device)?;
    let stream = device.raw().default_stream();

    // In-place: x and dst alias. Rust-side borrow checker rejects taking
    // both `qk.buf()` and `qk.buf_mut()` simultaneously, so we grab the
    // mut borrow once and pass the raw device pointer for the `x` arg.
    // The kernel's read-before-write pattern (each thread loads x[ix],
    // x[ix+n_dims/2] before writing dst[idst]) is safe under aliasing.
    let qk_dst = qk.buf_mut();
    let qk_src_ptr: u64 = {
        // The CudaSlice `DevicePtr` is stable across calls; we keep
        // `qk_dst` live below so the underlying allocation isn't
        // dropped. Using the raw pointer as an immutable kernel arg
        // here is equivalent to passing `qk.buf()` but side-steps the
        // borrow checker since we've already taken the mutable borrow.
        use cudarc::driver::DevicePtr;
        let stream_ref = device.raw().default_stream();
        let (p, _guard) = qk_dst.device_ptr(&stream_ref);
        p
    };

    let mut launcher = stream.launch_builder(&f);
    launcher
        .arg(&qk_src_ptr) // const T* x (alias of dst — in-place)
        .arg(qk_dst) // T* dst
        .arg(&ne00)
        .arg(&ne01)
        .arg(&ne02)
        .arg(&s01)
        .arg(&s02)
        .arg(&s03)
        .arg(&s1)
        .arg(&s2)
        .arg(&s3)
        .arg(&n_dims)
        .arg(positions.buf())
        .arg(&freq_scale)
        .arg(&ext_factor)
        .arg(&attn_factor)
        .arg(&corr_dims)
        .arg(&theta_scale)
        .arg(&null_ptr) // const float* freq_factors = nullptr
        .arg(&sections)
        .arg(&is_imrope);

    unsafe { launcher.launch(cfg) }.map_err(|e| {
        anyhow!(
            "rope_multi<bf16> launch (n_tokens={} n_heads={} head_dim={} rope_dim={}): {:?}",
            n_tokens,
            n_heads,
            head_dim,
            rope_dim,
            e
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference matching the .cu math exactly: NeoX-style pairing
    /// with 4-way axis sectioning.
    fn rope_mrope_cpu(
        qk: &mut [f32],
        positions: &[i32], // [4 * n_tokens] flat, axis-major
        n_tokens: usize,
        n_heads: usize,
        head_dim: usize,
        rope_dim: usize,
        theta_base: f32,
    ) {
        assert!(rope_dim <= head_dim && rope_dim % 2 == 0);
        let section_size = rope_dim / 8;
        for token in 0..n_tokens {
            for head in 0..n_heads {
                let base = (token * n_heads + head) * head_dim;
                for p in 0..(rope_dim / 2) {
                    let mut axis = 0usize;
                    if section_size > 0 {
                        axis = p / section_size;
                        if axis > 3 {
                            axis = 3;
                        }
                    }
                    let pos = positions[axis * n_tokens + token];
                    let exponent = -2.0f32 * (p as f32) / (rope_dim as f32);
                    let freq = theta_base.powf(exponent);
                    let theta = pos as f32 * freq;
                    let (sin_t, cos_t) = theta.sin_cos();
                    let i0 = base + p;
                    let i1 = base + p + rope_dim / 2;
                    let x0 = qk[i0];
                    let x1 = qk[i1];
                    qk[i0] = x0 * cos_t - x1 * sin_t;
                    qk[i1] = x0 * sin_t + x1 * cos_t;
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn rope_mrope_vs_cpu_golden() {
        let n_tokens = 16usize;
        let n_heads = 8usize;
        let head_dim = 128usize;
        let rope_dim = 128i32;
        let theta_base = 10000.0f32;

        // Deterministic pseudo-random bf16 input in [-1, 1].
        let mut seed: u32 = 0xC0FFEE;
        let mut rand_f = || -> f32 {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            ((seed >> 16) as f32 / 32768.0) - 1.0
        };
        let numel = n_tokens * n_heads * head_dim;
        let x_f32: Vec<f32> = (0..numel).map(|_| rand_f()).collect();
        // Quantize to bf16 both for device input and CPU reference so
        // we only see kernel-induced error, not float-width drift.
        let x_bf16_round: Vec<f32> = x_f32
            .iter()
            .map(|&v| bf16::from_f32(v).to_f32())
            .collect();
        let x_host_bf16: Vec<bf16> = x_f32.iter().map(|&v| bf16::from_f32(v)).collect();

        // Positions: axis 0/1/2 = monotonically growing text position,
        // axis 3 = 0. Mirrors the "plain text" case described in the
        // task spec.
        let mut positions: Vec<i32> = vec![0; 4 * n_tokens];
        for t in 0..n_tokens {
            let pos = t as i32 + 1;
            positions[0 * n_tokens + t] = pos;
            positions[1 * n_tokens + t] = pos;
            positions[2 * n_tokens + t] = pos;
            positions[3 * n_tokens + t] = 0;
        }

        // CPU golden.
        let mut y_cpu = x_bf16_round.clone();
        rope_mrope_cpu(
            &mut y_cpu,
            &positions,
            n_tokens,
            n_heads,
            head_dim,
            rope_dim as usize,
            theta_base,
        );
        // Round back to bf16 domain for a fair diff.
        let y_cpu_bf16: Vec<f32> = y_cpu
            .iter()
            .map(|&v| bf16::from_f32(v).to_f32())
            .collect();

        // Device run.
        let dev = Arc::new(DeviceContext::new(0).expect("cuda init"));
        let mut qk = CudaTensor::<bf16>::from_host(
            dev.clone(),
            vec![n_tokens, n_heads, head_dim],
            &x_host_bf16,
        )
        .expect("upload qk");
        let pos = CudaTensor::<i32>::from_host(dev.clone(), vec![4, n_tokens], &positions)
            .expect("upload positions");

        launch_rope_mrope_bf16(&dev, &mut qk, &pos, theta_base, rope_dim).expect("launch");
        dev.synchronize().expect("sync");

        let y_gpu_bf16: Vec<bf16> = qk.to_host().expect("download qk");
        let y_gpu_f32: Vec<f32> = y_gpu_bf16.iter().map(|v| v.to_f32()).collect();

        let mut max_abs = 0.0f32;
        let mut max_rel = 0.0f32;
        for (a, b) in y_cpu_bf16.iter().zip(y_gpu_f32.iter()) {
            let d = (a - b).abs();
            if d > max_abs {
                max_abs = d;
            }
            let scale = a.abs().max(b.abs()).max(1e-3);
            let rel = d / scale;
            if rel > max_rel {
                max_rel = rel;
            }
        }
        eprintln!(
            "rope_mrope diff: max_abs={:.6e} max_rel={:.6e}",
            max_abs, max_rel
        );
        // CUDA sincosf + powf round to within a few f32 ULPs of Rust's
        // libm; on a handful of pairs those few ULPs shift the final
        // f32 across a bf16 tie-breaking boundary, producing a 1-ULP
        // bf16 disagreement. 1 bf16 ULP at magnitude ~0.125 is 2^-10
        // (≈ 9.77e-4), which corresponds to ~5–8e-3 relative at that
        // magnitude. The bound below covers that worst case while
        // still catching real implementation bugs.
        assert!(
            max_abs < 2.0e-3,
            "GPU rope_mrope diverges from CPU golden beyond 1 bf16 ULP: max_abs={}",
            max_abs
        );
        assert!(
            max_rel < 6e-3,
            "GPU rope_mrope diverges from CPU golden: max_rel={}",
            max_rel
        );
    }
}
