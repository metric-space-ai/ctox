//! Reads noise_embed.f32.bin + target_hid.f32.bin produced by the
//! reference `dump_draft_io` tool, runs our Rust DFlashDraftModel
//! forward on IDENTICAL numeric inputs, writes our_output.f32.bin
//! next to the reference output, and prints side-by-side divergence
//! stats (max-abs diff, rel diff, first-position top-1 if logits).
//!
//! Purpose: nail down the exact op where our candle port diverges
//! from the ggml reference on the draft forward.

#![cfg(feature = "cuda")]

use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use clap::Parser;
use engine_core::{DFlashDraftConfig, DFlashDraftModel};
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    /// Local DFlash draft snapshot dir (must contain config.json +
    /// model.safetensors).
    #[arg(long)]
    draft_path: PathBuf,

    /// Directory written by dump_draft_io.
    #[arg(long, default_value = "/tmp/dflash_diff")]
    io_dir: PathBuf,

    /// CUDA device ordinal.
    #[arg(long, default_value_t = 0)]
    device: usize,
}

fn read_f32(path: &PathBuf) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {:?}", path))?;
    if bytes.len() % 4 != 0 {
        return Err(anyhow!("file {:?} len {} not /4", path, bytes.len()));
    }
    let n = bytes.len() / 4;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut b = [0u8; 4];
        b.copy_from_slice(&bytes[i * 4..i * 4 + 4]);
        out.push(f32::from_le_bytes(b));
    }
    Ok(out)
}
fn read_i32(path: &PathBuf) -> Result<Vec<i32>> {
    let bytes = std::fs::read(path).with_context(|| format!("read {:?}", path))?;
    if bytes.len() % 4 != 0 {
        return Err(anyhow!("file {:?} len {} not /4", path, bytes.len()));
    }
    let n = bytes.len() / 4;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut b = [0u8; 4];
        b.copy_from_slice(&bytes[i * 4..i * 4 + 4]);
        out.push(i32::from_le_bytes(b));
    }
    Ok(out)
}

fn write_f32(path: &PathBuf, data: &[f32]) -> Result<()> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, &bytes).with_context(|| format!("write {:?}", path))?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let device = Device::new_cuda(args.device)?;

    // Read dims.i32.bin = [hidden, q_len, fc_in, ctx_len].
    let dims_path = args.io_dir.join("dims.i32.bin");
    let dims = read_i32(&dims_path)?;
    if dims.len() != 4 {
        return Err(anyhow!("dims.i32.bin must have 4 i32s, got {}", dims.len()));
    }
    let (hidden, q_len, fc_in, ctx_len) =
        (dims[0] as usize, dims[1] as usize, dims[2] as usize, dims[3] as usize);
    eprintln!(
        "dims: hidden={} q_len={} fc_in={} ctx_len={}",
        hidden, q_len, fc_in, ctx_len
    );

    // Read inputs.
    let noise_data = read_f32(&args.io_dir.join("noise_embed.f32.bin"))?;
    let target_data = read_f32(&args.io_dir.join("target_hid.f32.bin"))?;
    if noise_data.len() != hidden * q_len {
        return Err(anyhow!(
            "noise_embed size {} != hidden*q_len={}",
            noise_data.len(),
            hidden * q_len
        ));
    }
    if target_data.len() != fc_in * ctx_len {
        return Err(anyhow!(
            "target_hid size {} != fc_in*ctx_len={}",
            target_data.len(),
            fc_in * ctx_len
        ));
    }

    // Build tensors. Reference uses F32 inputs (activations flow F32
    // through the ggml graph since CUDA rms_norm requires F32). We
    // feed BF16 into our candle draft since its layer norm etc are
    // typed at load dtype. Cast at input boundary.
    //
    // NB: the reference ggml layout is [hidden, q_len, 1] for
    // noise_embed — that's math-shape (q_len, hidden). Since both
    // formats store row-major in the same memory layout (q_len outer,
    // hidden inner) the raw f32 bytes are interchangeable; we just
    // reshape to candle's (1, q_len, hidden) math convention.
    let noise_t = Tensor::from_vec(noise_data.clone(), (1, q_len, hidden), &device)?
        .to_dtype(DType::BF16)?;
    let target_t =
        Tensor::from_vec(target_data.clone(), (1, ctx_len, fc_in), &device)?
            .to_dtype(DType::BF16)?;

    eprintln!(
        "noise_t shape={:?}, target_t shape={:?}",
        noise_t.dims(),
        target_t.dims()
    );

    // Load draft weights.
    eprintln!("loading draft…");
    let draft_cfg_path = args.draft_path.join("config.json");
    let draft_cfg_text = std::fs::read_to_string(&draft_cfg_path)?;
    let draft_cfg: DFlashDraftConfig = serde_json::from_str(&draft_cfg_text)?;
    let draft_shards: Vec<PathBuf> = std::fs::read_dir(&args.draft_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("safetensors"))
        .collect();
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&draft_shards, DType::BF16, &device)?
    };
    let draft = DFlashDraftModel::load(vb, draft_cfg.clone())?;
    eprintln!("draft loaded");

    // Run forward_hidden.
    eprintln!("running draft forward…");
    let out = draft.forward_hidden(&noise_t, &target_t)?;
    eprintln!("out shape={:?} dtype={:?}", out.dims(), out.dtype());

    // Pull to host as f32.
    let out_f32: Vec<f32> = out
        .to_dtype(DType::F32)?
        .flatten_all()?
        .to_vec1()?;
    let out_bin = args.io_dir.join("our_output.f32.bin");
    write_f32(&out_bin, &out_f32)?;
    eprintln!("wrote {:?} ({} floats)", out_bin, out_f32.len());

    // Stats on our output.
    let (mean, std, mn, mx) = stats(&out_f32);
    eprintln!(
        "OUR output: mean={:.4e} std={:.4e} min={:.4e} max={:.4e}  first4={:.4} {:.4} {:.4} {:.4}",
        mean, std, mn, mx, out_f32[0], out_f32[1], out_f32[2], out_f32[3]
    );

    // Read reference output + compare element-wise.
    let ref_out = read_f32(&args.io_dir.join("ref_output.f32.bin"))?;
    if ref_out.len() != out_f32.len() {
        return Err(anyhow!(
            "ref size {} != our size {}",
            ref_out.len(),
            out_f32.len()
        ));
    }
    let (rm, rs, rmn, rmx) = stats(&ref_out);
    eprintln!(
        "REF output: mean={:.4e} std={:.4e} min={:.4e} max={:.4e}  first4={:.4} {:.4} {:.4} {:.4}",
        rm, rs, rmn, rmx, ref_out[0], ref_out[1], ref_out[2], ref_out[3]
    );

    let mut max_abs_diff = 0.0_f32;
    let mut max_rel_diff = 0.0_f32;
    let mut sum_sq_diff = 0.0_f64;
    let mut n_big = 0usize;
    for (a, b) in out_f32.iter().zip(ref_out.iter()) {
        let d = (a - b).abs();
        if d > max_abs_diff {
            max_abs_diff = d;
        }
        sum_sq_diff += (d as f64) * (d as f64);
        let scale = a.abs().max(b.abs()).max(1e-6);
        let rel = d / scale;
        if rel > max_rel_diff {
            max_rel_diff = rel;
        }
        if d > 0.1 {
            n_big += 1;
        }
    }
    let rms = (sum_sq_diff / out_f32.len() as f64).sqrt();
    eprintln!(
        "\nDIFF our vs ref: max_abs={:.6e} max_rel={:.6e} rms={:.6e} n_diff_over_0.1={} / {}",
        max_abs_diff,
        max_rel_diff,
        rms,
        n_big,
        out_f32.len()
    );
    if max_abs_diff < 1e-2 {
        eprintln!("→ CLOSE: within numerical noise of bf16 roundoff");
    } else if max_abs_diff < 1.0 {
        eprintln!("→ DRIFT: small numerical drift, likely RoPE / norm eps / dtype boundary");
    } else {
        eprintln!("→ DIVERGENT: large numerical disagreement — likely layer-level bug");
    }

    Ok(())
}

fn stats(v: &[f32]) -> (f64, f64, f32, f32) {
    let n = v.len() as f64;
    let sum = v.iter().map(|x| *x as f64).sum::<f64>();
    let mean = sum / n;
    let var = v.iter().map(|x| (*x as f64 - mean).powi(2)).sum::<f64>() / n;
    let std = var.sqrt();
    let mn = v.iter().cloned().fold(f32::INFINITY, f32::min);
    let mx = v.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    (mean, std, mn, mx)
}
