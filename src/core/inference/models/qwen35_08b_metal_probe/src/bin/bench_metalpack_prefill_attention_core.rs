#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_attention_core is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_attention_core_with_weights, PrefillAttentionCoreBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_attention_core <metalpack-dir> [layer] [tokens] [iterations] [project-mma 0|1] [mps-attention-out-sidecar-dir]".to_owned());
    }

    let root = PathBuf::from(&args[1]);
    let layer = parse_arg(&args, 2, "layer")?.unwrap_or(3);
    let tokens = parse_arg(&args, 3, "tokens")?.unwrap_or(512);
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(5);
    let use_project_mma = parse_arg(&args, 5, "project-mma")?.unwrap_or(1) != 0;
    let mps_attention_out_sidecar = args.get(6).map(PathBuf::from);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let prefix = format!("model.language_model.layers.{layer}.");
    let norm = find_tensor(&pack, &prefix, "input_layernorm.weight")?;
    let q_norm = find_tensor(&pack, &prefix, "self_attn.q_norm.weight")?;
    let k_norm = find_tensor(&pack, &prefix, "self_attn.k_norm.weight")?;
    let q = find_tensor(&pack, &prefix, "self_attn.q_proj.weight")?;
    let k = find_tensor(&pack, &prefix, "self_attn.k_proj.weight")?;
    let v = find_tensor(&pack, &prefix, "self_attn.v_proj.weight")?;
    let o = find_tensor(&pack, &prefix, "self_attn.o_proj.weight")?;
    validate_vector("input_norm", norm, QWEN35_08B.hidden_size)?;
    validate_vector("q_norm", q_norm, QWEN35_08B.attention_head_dim)?;
    validate_vector("k_norm", k_norm, QWEN35_08B.attention_head_dim)?;
    validate_projection(
        "q",
        q,
        &[
            QWEN35_08B.attention_q_with_head_gate_width(),
            QWEN35_08B.hidden_size,
        ],
    )?;
    validate_projection(
        "k",
        k,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "v",
        v,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "o",
        o,
        &[QWEN35_08B.hidden_size, QWEN35_08B.attention_q_width()],
    )?;

    let norm_weights = read_u16_entry(&pack, norm)?;
    let q_norm_weights = read_u16_entry(&pack, q_norm)?;
    let k_norm_weights = read_u16_entry(&pack, k_norm)?;
    let q_weights = read_u16_entry(&pack, q)?;
    let k_weights = read_u16_entry(&pack, k)?;
    let v_weights = read_u16_entry(&pack, v)?;
    let o_weights = read_u16_entry(&pack, o)?;
    let mps_attention_out_weight = if let Some(sidecar) = mps_attention_out_sidecar.as_ref() {
        Some(load_mps_attention_out_sidecar(sidecar, layer)?)
    } else {
        None
    };
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let value = ((token * 23 + col * 11) % 257) as f32 / 257.0 - 0.5;
            x_host.push(f16::from_f32(value).to_bits());
        }
    }

    let cfg = PrefillAttentionCoreBenchConfig {
        tokens,
        row_tile: q.row_tile,
        hidden_col_tile: q.col_tile,
        attention_col_tile: o.col_tile,
        use_project_mma,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_attention_core_with_weights(
        cfg,
        &x_host,
        &norm_weights,
        &q_norm_weights,
        &k_norm_weights,
        &q_weights,
        &k_weights,
        &v_weights,
        &o_weights,
        mps_attention_out_weight.as_deref(),
    )?;

    println!("qwen35-08b metalpack prefill attention core benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("project_mma: {}", use_project_mma);
    if let Some(sidecar) = mps_attention_out_sidecar {
        println!("mps_attention_out_sidecar: {}", sidecar.display());
    }
    println!(
        "shape: tokens={} hidden={} q_rows={} kv_rows={} attention_width={}",
        result.tokens, result.hidden, result.q_rows, result.kv_rows, result.attention_width
    );
    println!(
        "tile: project_tokens={} out_tokens={} rows={} packed_bytes={}",
        result.project_token_tile,
        result.out_token_tile,
        result.row_tile,
        result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_attention_core_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}

#[cfg(target_os = "macos")]
fn parse_arg(args: &[std::ffi::OsString], idx: usize, name: &str) -> Result<Option<usize>, String> {
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid {name} argument `{arg}`: {err}"))
        })
        .transpose()
}

#[cfg(target_os = "macos")]
fn load_mps_attention_out_sidecar(
    sidecar: &std::path::Path,
    layer: usize,
) -> Result<Vec<u8>, String> {
    use serde_json::Value;
    use std::fs;

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str)
        != Some("ctox.qwen35_08b.mps_attention_out_sidecar")
    {
        return Err(format!(
            "invalid MPS attention out sidecar: {}",
            manifest_path.display()
        ));
    }
    let weights_file = manifest
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing weights_file".to_owned())?;
    let entries = manifest
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing entries".to_owned())?;
    let entry = entries
        .iter()
        .find(|entry| entry.get("layer").and_then(Value::as_u64) == Some(layer as u64))
        .ok_or_else(|| format!("missing layer {layer} in MPS attention out sidecar"))?;
    let out = entry
        .get("out")
        .ok_or_else(|| "missing out entry".to_owned())?;
    let offset = json_usize(out, "offset")?;
    let bytes = json_usize(out, "bytes")?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;
    if offset + bytes > weights.len() {
        return Err("sidecar weight offsets exceed weights.bin length".to_owned());
    }
    Ok(weights[offset..offset + bytes].to_vec())
}

#[cfg(target_os = "macos")]
fn json_usize(value: &serde_json::Value, key: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("missing/invalid {key}"))
}

#[cfg(target_os = "macos")]
fn find_tensor<'a>(
    pack: &'a ctox_qwen35_08b_metal_probe::MetalPack,
    prefix: &str,
    name: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
        .ok_or_else(|| format!("missing tensor containing `{prefix}` and `{name}`"))
}

#[cfg(target_os = "macos")]
fn validate_vector(label: &str, entry: &MetalPackEntry, len: usize) -> Result<(), String> {
    if entry.source_shape != [len] {
        return Err(format!(
            "{label}: expected [{len}], got {:?}",
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_projection(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(
    pack: &ctox_qwen35_08b_metal_probe::MetalPack,
    entry: &MetalPackEntry,
) -> Result<Vec<u16>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "{} byte length is not divisible by two",
            entry.tensor
        ));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}
