#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_ffn_block is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, PackLayout, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::{bf16 as half_bf16, f16 as half_f16};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_delta_ffn_block_with_weights, PrefillDeltaFfnBlockBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_delta_ffn_block <metalpack-dir> [layer] [tokens] [iterations]".to_owned());
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
    if layer % 4 == 3 {
        return Err("this benchmark expects a DeltaNet layer, not an attention layer".to_string());
    }
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
    let delta_out = find_tensor(&pack, &prefix, "linear_attn.out_proj.weight")?;
    let ffn_norm = find_tensor(&pack, &prefix, "post_attention_layernorm.weight")?;
    let ffn_gate = find_tensor(&pack, &prefix, "mlp.gate_proj.weight")?;
    let ffn_up = find_tensor(&pack, &prefix, "mlp.up_proj.weight")?;
    let ffn_down = find_tensor(&pack, &prefix, "mlp.down_proj.weight")?;

    validate_vector(input_norm, QWEN35_08B.hidden_size)?;
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
    validate_projection(
        "delta_out",
        delta_out,
        &[QWEN35_08B.hidden_size, QWEN35_08B.deltanet_width()],
    )?;
    validate_conv_weight(conv_weight)?;
    if let Some(entry) = conv_bias {
        validate_conv_bias(entry)?;
    }
    validate_vector(a_log, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(dt_bias, QWEN35_08B.deltanet_v_heads)?;
    validate_vector(delta_norm, QWEN35_08B.deltanet_head_dim)?;
    validate_vector(ffn_norm, QWEN35_08B.hidden_size)?;
    validate_projection(
        "ffn_gate",
        ffn_gate,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "ffn_up",
        ffn_up,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_projection(
        "ffn_down",
        ffn_down,
        &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
    )?;

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
    let delta_out_weights = read_u16_entry(&pack, delta_out)?;
    let ffn_norm_weights = read_u16_entry(&pack, ffn_norm)?;
    let ffn_gate_weights = read_u16_entry(&pack, ffn_gate)?;
    let ffn_up_weights = read_u16_entry(&pack, ffn_up)?;
    let ffn_down_weights = read_u16_entry(&pack, ffn_down)?;

    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }

    let cfg = PrefillDeltaFfnBlockBenchConfig {
        tokens,
        row_tile: qkv.row_tile,
        hidden_col_tile: qkv.col_tile,
        delta_out_col_tile: delta_out.col_tile,
        intermediate_col_tile: ffn_down.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_delta_ffn_block_with_weights(
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
        &delta_out_weights,
        &ffn_norm_weights,
        &ffn_gate_weights,
        &ffn_up_weights,
        &ffn_down_weights,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet + FFN block benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("qkv: {}", qkv.tensor);
    println!("delta_out: {}", delta_out.tensor);
    println!("ffn_gate: {}", ffn_gate.tensor);
    println!("ffn_down: {}", ffn_down.tensor);
    println!(
        "shape: tokens={} hidden={} delta_width={} intermediate={} qkv_rows={}",
        result.tokens, result.hidden, result.delta_width, result.intermediate, result.qkv_rows
    );
    println!(
        "tile: project_tokens={} qkvz_tokens={} out_tokens={} ffn_tokens={} down_tokens={} rows={} packed_bytes={}",
        result.project_token_tile,
        result.qkvz_token_tile,
        result.out_token_tile,
        result.ffn_token_tile,
        result.down_token_tile,
        result.row_tile,
        result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_delta_ffn_block_estimate: {:.2}",
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
fn validate_vector(entry: &MetalPackEntry, len: usize) -> Result<(), String> {
    if entry.source_shape != [len] {
        return Err(format!(
            "{}: expected vector len {len}, got {:?}",
            entry.tensor, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_conv_weight(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_qkv_width(), 1, 4] {
        return Err(format!(
            "conv weight: expected [{}, 1, 4], got {:?}",
            QWEN35_08B.deltanet_qkv_width(),
            entry.source_shape
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
