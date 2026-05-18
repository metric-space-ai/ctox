#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_attention_project is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_attention_project_with_weights, PrefillAttentionProjectBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_attention_project <metalpack-dir> [layer] [tokens] [iterations]".to_owned());
    }

    let root = PathBuf::from(&args[1]);
    let layer = parse_arg(&args, 2, "layer")?.unwrap_or(3);
    let tokens = parse_arg(&args, 3, "tokens")?.unwrap_or(512);
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(5);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let prefix = format!("model.language_model.layers.{layer}.");
    let norm = find_tensor(&pack, &prefix, "input_layernorm.weight")?;
    let q = find_tensor(&pack, &prefix, "self_attn.q_proj.weight")?;
    let k = find_tensor(&pack, &prefix, "self_attn.k_proj.weight")?;
    let v = find_tensor(&pack, &prefix, "self_attn.v_proj.weight")?;
    validate_norm(norm)?;
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

    let norm_weights = read_u16_entry(&pack, norm)?;
    let q_weights = read_u16_entry(&pack, q)?;
    let k_weights = read_u16_entry(&pack, k)?;
    let v_weights = read_u16_entry(&pack, v)?;
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let value = ((token * 23 + col * 11) % 257) as f32 / 257.0 - 0.5;
            x_host.push(f16::from_f32(value).to_bits());
        }
    }

    let cfg = PrefillAttentionProjectBenchConfig {
        tokens,
        row_tile: q.row_tile,
        col_tile: q.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_attention_project_with_weights(
        cfg,
        &x_host,
        &norm_weights,
        &q_weights,
        &k_weights,
        &v_weights,
    )?;

    println!("qwen35-08b metalpack prefill attention q/k/v projection benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("norm: {}", norm.tensor);
    println!("q: {}", q.tensor);
    println!("k: {}", k.tensor);
    println!("v: {}", v.tensor);
    println!(
        "shape: tokens={} hidden={} q_rows={} kv_rows={}",
        result.tokens, result.hidden, result.q_rows, result.kv_rows
    );
    println!(
        "tile: tokens={} rows={} packed_bytes={}",
        result.token_tile, result.row_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_attention_project_estimate: {:.2}",
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
fn validate_norm(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.hidden_size] {
        return Err(format!(
            "norm: expected [{}], got {:?}",
            QWEN35_08B.hidden_size, entry.source_shape
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
