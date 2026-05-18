#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_block is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, PackLayout, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::{bf16 as half_bf16, f16 as half_f16};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_block_with_weights, PrefillDeltaBlockBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_block <metalpack-dir> [layer] [tokens] [iterations]"
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
    let input_norm = find_tensor(&pack, &prefix, "input_layernorm.weight")?;
    let qkv = find_tensor(&pack, &prefix, "linear_attn.in_proj_qkv.weight")?;
    let z = find_tensor(&pack, &prefix, "linear_attn.in_proj_z.weight")?;
    let b = find_tensor(&pack, &prefix, "linear_attn.in_proj_b.weight")?;
    let a = find_tensor(&pack, &prefix, "linear_attn.in_proj_a.weight")?;
    let conv_weight = find_tensor(&pack, &prefix, "linear_attn.conv1d.weight")?;
    let conv_bias = find_optional_tensor(&pack, &prefix, "linear_attn.conv1d.bias");
    let a_log = find_tensor(&pack, &prefix, "linear_attn.A_log")?;
    let dt_bias = find_tensor(&pack, &prefix, "linear_attn.dt_bias")?;
    let delta_norm = find_tensor(&pack, &prefix, "linear_attn.norm.weight")?;
    let out_proj = find_tensor(&pack, &prefix, "linear_attn.out_proj.weight")?;

    validate_input_norm(input_norm)?;
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
    validate_conv_weight(conv_weight)?;
    if let Some(entry) = conv_bias {
        validate_conv_bias(entry)?;
    }
    validate_vector(a_log, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(dt_bias, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(delta_norm, QWEN35_08B.deltanet_head_dim)?;
    validate_projection(
        "out_proj",
        out_proj,
        &[QWEN35_08B.hidden_size, QWEN35_08B.deltanet_width()],
    )?;
    for entry in [z, b, a] {
        if entry.row_tile != qkv.row_tile || entry.col_tile != qkv.col_tile {
            return Err(format!("projection tile mismatch for {}", entry.tensor));
        }
    }

    let input_norm_weights = read_u16_entry(&pack, input_norm)?;
    let qkv_weights = read_u16_entry(&pack, qkv)?;
    let z_weights = read_u16_entry(&pack, z)?;
    let b_weights = read_u16_entry(&pack, b)?;
    let a_weights = read_u16_entry(&pack, a)?;
    let conv_weights = read_u16_entry(&pack, conv_weight)?;
    let conv_bias_weights = match conv_bias {
        Some(entry) => read_u16_entry(&pack, entry)?,
        None => vec![f16::from_f32(0.0).to_bits(); QWEN35_08B.deltanet_qkv_width()],
    };
    let a_log_host = read_float_entry(&pack, a_log)?;
    let dt_bias_host = read_float_entry(&pack, dt_bias)?;
    let delta_norm_host = read_float_entry(&pack, delta_norm)?;
    let out_weights = read_u16_entry(&pack, out_proj)?;

    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }

    let cfg = PrefillDeltaBlockBenchConfig {
        tokens,
        row_tile: qkv.row_tile,
        hidden_col_tile: qkv.col_tile,
        out_col_tile: out_proj.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_deltanet_block_with_weights(
        cfg,
        &x_host,
        &input_norm_weights,
        &qkv_weights,
        &z_weights,
        &b_weights,
        &a_weights,
        &conv_weights,
        &conv_bias_weights,
        &a_log_host,
        &dt_bias_host,
        &delta_norm_host,
        &out_weights,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet full block benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("qkv: {}", qkv.tensor);
    println!("out_proj: {}", out_proj.tensor);
    println!(
        "shape: tokens={} hidden={} delta_width={} qkv_rows={}",
        result.tokens, result.hidden, result.delta_width, result.qkv_rows
    );
    println!(
        "tile: project_tokens={} out_tokens={} rows={} packed_bytes={}",
        result.token_tile, result.out_token_tile, result.row_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_delta_block_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_tensor<'a>(
    pack: &'a MetalPack,
    prefix: &str,
    name: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
        .ok_or_else(|| format!("missing tensor containing `{prefix}` and `{name}`"))
}

#[cfg(target_os = "macos")]
fn find_optional_tensor<'a>(
    pack: &'a MetalPack,
    prefix: &str,
    name: &str,
) -> Option<&'a MetalPackEntry> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
}

#[cfg(target_os = "macos")]
fn validate_input_norm(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.hidden_size] {
        return Err(format!(
            "input norm: expected [{}], got {:?}",
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
fn validate_conv_weight(entry: &MetalPackEntry) -> Result<(), String> {
    let shape_a = [QWEN35_08B.deltanet_qkv_width(), 4];
    let shape_b = [QWEN35_08B.deltanet_qkv_width(), 1, 4];
    if entry.source_shape != shape_a && entry.source_shape != shape_b {
        return Err(format!(
            "conv weight: expected {:?} or {:?}, got {:?}",
            shape_a, shape_b, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_conv_bias(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_qkv_width()] {
        return Err(format!(
            "conv bias: expected [{}], got {:?}",
            QWEN35_08B.deltanet_qkv_width(),
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_vector(entry: &MetalPackEntry, len: usize) -> Result<(), String> {
    if entry.source_shape != [len] {
        return Err(format!(
            "{}: expected [{len}], got {:?}",
            entry.tensor, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
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

#[cfg(target_os = "macos")]
fn read_float_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<f32>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    match entry.dtype.as_str() {
        "F32" => {
            if bytes.len() % 4 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by four",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect())
        }
        "F16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| half_f16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32())
                .collect())
        }
        "BF16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| {
                    half_bf16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32()
                })
                .collect())
        }
        other => Err(format!("{} unsupported dtype {other}", entry.tensor)),
    }
}
