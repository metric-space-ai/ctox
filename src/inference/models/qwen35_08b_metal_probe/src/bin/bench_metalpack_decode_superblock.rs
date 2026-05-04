#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_decode_superblock is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, TensorClass, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_decode_superblock_tiled_with_weights, DecodeSkeletonBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_decode_superblock <metalpack-dir> [delta-prefix] [attention-prefix] [ffn-prefix] [input-token] [iterations] [superblocks] [decode-position] [max-context]".to_owned());
    }

    let root = PathBuf::from(&args[1]);
    let delta_prefix = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.0");
    let attention_prefix = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.3.self_attn");
    let ffn_prefix = args
        .get(4)
        .and_then(|arg| arg.to_str())
        .unwrap_or("model.layers.0.mlp");
    let input_token = args
        .get(5)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<u32>()
                .map_err(|err| format!("invalid input token argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(107);
    let iterations = args
        .get(6)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);
    let superblocks = args
        .get(7)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid superblocks argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(1);
    let decode_position = args
        .get(8)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<u32>()
                .map_err(|err| format!("invalid decode position argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);
    let max_context = args
        .get(9)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid max context argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(1);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let embedding = pack
        .find_first_class(TensorClass::TokenEmbedding)
        .ok_or_else(|| "metalpack has no token_embedding entry".to_owned())?;
    let lm_head = pack
        .find_first_class(TensorClass::LmHead)
        .unwrap_or(embedding);

    let delta_qkv = find_tensor(&pack, delta_prefix, "in_proj_qkv")?;
    let delta_z = find_tensor(&pack, delta_prefix, "in_proj_z")?;
    let delta_b = find_tensor(&pack, delta_prefix, "in_proj_b")?;
    let delta_a = find_tensor(&pack, delta_prefix, "in_proj_a")?;
    let delta_out = find_tensor(&pack, delta_prefix, "out_proj")?;
    let attn_q = find_tensor(&pack, attention_prefix, "q_proj")?;
    let attn_k = find_tensor(&pack, attention_prefix, "k_proj")?;
    let attn_v = find_tensor(&pack, attention_prefix, "v_proj")?;
    let attn_o = find_tensor(&pack, attention_prefix, "o_proj")?;
    let ffn_gate = find_tensor(&pack, ffn_prefix, "gate_proj")?;
    let ffn_up = find_tensor(&pack, ffn_prefix, "up_proj")?;
    let ffn_down = find_tensor(&pack, ffn_prefix, "down_proj")?;

    let delta_width = QWEN35_08B.deltanet_v_heads * QWEN35_08B.deltanet_head_dim;
    validate_entry(
        "embedding",
        embedding,
        &[embedding.source_shape[0], QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "lm_head",
        lm_head,
        &[embedding.source_shape[0], QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "delta_qkv",
        delta_qkv,
        &[delta_width * 3, QWEN35_08B.hidden_size],
    )?;
    validate_entry("delta_z", delta_z, &[delta_width, QWEN35_08B.hidden_size])?;
    validate_entry(
        "delta_b",
        delta_b,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "delta_a",
        delta_a,
        &[QWEN35_08B.deltanet_v_heads, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "delta_out",
        delta_out,
        &[QWEN35_08B.hidden_size, delta_width],
    )?;
    validate_entry_one_of(
        "attn_q",
        attn_q,
        &[
            vec![QWEN35_08B.attention_q_width(), QWEN35_08B.hidden_size],
            vec![
                QWEN35_08B.attention_q_with_head_gate_width(),
                QWEN35_08B.hidden_size,
            ],
        ],
    )?;
    validate_entry(
        "attn_k",
        attn_k,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "attn_v",
        attn_v,
        &[QWEN35_08B.attention_kv_width(), QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "attn_o",
        attn_o,
        &[QWEN35_08B.hidden_size, QWEN35_08B.attention_q_width()],
    )?;
    validate_entry(
        "ffn_gate",
        ffn_gate,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "ffn_up",
        ffn_up,
        &[QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size],
    )?;
    validate_entry(
        "ffn_down",
        ffn_down,
        &[QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate],
    )?;

    for entry in [
        lm_head, delta_qkv, delta_z, delta_b, delta_a, delta_out, attn_q, attn_k, attn_v, attn_o,
        ffn_gate, ffn_up, ffn_down,
    ] {
        if entry.row_tile != embedding.row_tile || entry.col_tile != embedding.col_tile {
            return Err(format!("tile mismatch for {}", entry.tensor));
        }
    }

    let embedding_weights = read_u16_entry(&pack, embedding)?;
    let lm_weights = if embedding.tensor == lm_head.tensor {
        embedding_weights.clone()
    } else {
        read_u16_entry(&pack, lm_head)?
    };
    let norm = (0..QWEN35_08B.hidden_size)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect::<Vec<_>>();
    let cfg = DecodeSkeletonBenchConfig {
        vocab_rows: embedding.source_shape[0],
        input_token,
        decode_position,
        max_context,
        warmup: 1,
        iterations,
        ..DecodeSkeletonBenchConfig::default()
    };
    let result = run_decode_superblock_tiled_with_weights(
        cfg,
        superblocks,
        &embedding_weights,
        &norm,
        &read_u16_entry(&pack, delta_qkv)?,
        &read_u16_entry(&pack, delta_z)?,
        &read_u16_entry(&pack, delta_b)?,
        &read_u16_entry(&pack, delta_a)?,
        &read_u16_entry(&pack, delta_out)?,
        &read_u16_entry(&pack, attn_q)?,
        &read_u16_entry(&pack, attn_k)?,
        &read_u16_entry(&pack, attn_v)?,
        &read_u16_entry(&pack, attn_o)?,
        &read_u16_entry(&pack, ffn_gate)?,
        &read_u16_entry(&pack, ffn_up)?,
        &read_u16_entry(&pack, ffn_down)?,
        &lm_weights,
        embedding.row_tile,
        embedding.col_tile,
    )?;

    println!("qwen35-08b metalpack decode D/D/D/A superblock benchmark");
    println!("metalpack: {}", root.display());
    println!("embedding: {}", embedding.tensor);
    println!("delta_prefix: {}", delta_prefix);
    println!("attention_prefix: {}", attention_prefix);
    println!("ffn_prefix: {}", ffn_prefix);
    println!("lm_head: {}", lm_head.tensor);
    println!("input_token: {}", result.input_token);
    println!("decode_position: {}", decode_position);
    println!("max_context: {}", max_context);
    println!("shape: [{} x {}]", result.vocab_rows, result.cols);
    println!("superblocks: {}", superblocks);
    println!(
        "pattern: {}",
        "D+FFN D+FFN D+FFN A+FFN ".repeat(superblocks).trim()
    );
    println!(
        "tile: rows={} cols={}",
        embedding.row_tile, embedding.col_tile
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_superblock_lm_head_pairs: {:.2}",
        result.effective_gb_s
    );
    println!("next_token: {}", result.next_token);
    println!("score: {:.6}", result.score);
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
fn validate_entry(label: &str, entry: &MetalPackEntry, shape: &[usize]) -> Result<(), String> {
    if entry.dtype != "F16" {
        return Err(format!("{label}: expected F16 tensor, got {}", entry.dtype));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != shape {
        return Err(format!(
            "{label}: expected shape {:?}, got {:?}",
            shape, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_entry_one_of(
    label: &str,
    entry: &MetalPackEntry,
    shapes: &[Vec<usize>],
) -> Result<(), String> {
    if entry.dtype != "F16" {
        return Err(format!("{label}: expected F16 tensor, got {}", entry.dtype));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if !shapes.iter().any(|shape| shape == &entry.source_shape) {
        return Err(format!(
            "{label}: expected one of {:?}, got {:?}",
            shapes, entry.source_shape
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
