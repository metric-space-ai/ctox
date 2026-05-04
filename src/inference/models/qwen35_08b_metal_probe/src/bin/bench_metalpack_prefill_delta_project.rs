#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_project is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_project_with_weights, PrefillDeltaProjectBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_project <metalpack-dir> [layer] [tokens] [iterations]"
                .to_owned(),
        );
    }

    let root = PathBuf::from(&args[1]);
    let layer = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid layer argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);
    let tokens = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid tokens argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(512);
    let iterations = args
        .get(4)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(5);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let prefix = format!("model.language_model.layers.{layer}.");
    let norm = find_tensor(&pack, &prefix, "input_layernorm.weight")?;
    let qkv = find_tensor(&pack, &prefix, "linear_attn.in_proj_qkv.weight")?;
    let z = find_tensor(&pack, &prefix, "linear_attn.in_proj_z.weight")?;
    let b = find_tensor(&pack, &prefix, "linear_attn.in_proj_b.weight")?;
    let a = find_tensor(&pack, &prefix, "linear_attn.in_proj_a.weight")?;
    validate_norm(norm)?;
    validate_projection(
        "qkv",
        qkv,
        &[QWEN35_08B.deltanet_qkv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "z",
        z,
        &[QWEN35_08B.deltanet_width(), QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "b",
        b,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "a",
        a,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    for entry in [z, b, a] {
        if entry.row_tile != qkv.row_tile || entry.col_tile != qkv.col_tile {
            return Err(format!("tile mismatch for {}", entry.tensor));
        }
    }

    let norm_weights = read_u16_entry(&pack, norm)?;
    let qkv_weights = read_u16_entry(&pack, qkv)?;
    let z_weights = read_u16_entry(&pack, z)?;
    let b_weights = read_u16_entry(&pack, b)?;
    let a_weights = read_u16_entry(&pack, a)?;
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }
    let cfg = PrefillDeltaProjectBenchConfig {
        tokens,
        row_tile: qkv.row_tile,
        col_tile: qkv.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_deltanet_project_with_weights(
        cfg,
        &x_host,
        &norm_weights,
        &qkv_weights,
        &z_weights,
        &b_weights,
        &a_weights,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet projection benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("norm: {}", norm.tensor);
    println!("qkv: {}", qkv.tensor);
    println!("z: {}", z.tensor);
    println!("b: {}", b.tensor);
    println!("a: {}", a.tensor);
    println!(
        "shape: tokens={} hidden={} qkv_rows={} z_rows={} gate_rows={}",
        result.tokens, result.hidden, result.qkv_rows, result.z_rows, result.gate_rows
    );
    println!(
        "tile: tokens={} rows={} cols={} packed_bytes={}",
        result.token_tile, result.row_tile, result.col_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_token_tile_weight_reuse_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
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
