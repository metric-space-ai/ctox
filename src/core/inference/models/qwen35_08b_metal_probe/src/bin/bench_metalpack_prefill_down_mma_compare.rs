#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_down_mma_compare is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_down_mma_compare_with_weights, PrefillFfnBlockBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_down_mma_compare <metalpack-dir> [layer] [tokens] [iterations]".to_owned());
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
    let norm = find_tensor(&pack, &prefix, "post_attention_layernorm.weight")?;
    let gate = find_tensor(&pack, &prefix, "mlp.gate_proj.weight")?;
    let up = find_tensor(&pack, &prefix, "mlp.up_proj.weight")?;
    let down = find_tensor(&pack, &prefix, "mlp.down_proj.weight")?;
    validate_norm(norm)?;
    validate_hidden_to_intermediate("gate", gate)?;
    validate_hidden_to_intermediate("up", up)?;
    validate_intermediate_to_hidden(down)?;

    let norm_weights = read_u16_entry(&pack, norm)?;
    let gate_weights = read_u16_entry(&pack, gate)?;
    let up_weights = read_u16_entry(&pack, up)?;
    let down_weights = read_u16_entry(&pack, down)?;
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }

    let cfg = PrefillFfnBlockBenchConfig {
        tokens,
        row_tile: gate.row_tile,
        hidden_col_tile: gate.col_tile,
        intermediate_col_tile: down.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_down_mma_compare_with_weights(
        cfg,
        &x_host,
        &norm_weights,
        &gate_weights,
        &up_weights,
        &down_weights,
    )?;

    println!("qwen35-08b metalpack prefill FFN down MMA compare");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!(
        "shape: tokens={} hidden={} intermediate={}",
        result.tokens, result.hidden, result.intermediate
    );
    println!("iterations: {}", iterations);
    println!("baseline_median_s: {:.9}", result.baseline_median_s);
    println!("baseline_p95_s: {:.9}", result.baseline_p95_s);
    println!("mma_median_s: {:.9}", result.mma_median_s);
    println!("mma_p95_s: {:.9}", result.mma_p95_s);
    println!("baseline_checksum16: {:.6}", result.baseline_checksum);
    println!("mma_checksum16: {:.6}", result.mma_checksum);
    println!("max_abs_error: {:.9}", result.max_abs_error);
    println!("mean_abs_error: {:.9}", result.mean_abs_error);
    println!("max_abs_index: {}", result.max_abs_index);
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
fn validate_hidden_to_intermediate(label: &str, entry: &MetalPackEntry) -> Result<(), String> {
    validate_layout(label, entry)?;
    if entry.source_shape != [QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size] {
        return Err(format!(
            "{label}: expected [{}, {}], got {:?}",
            QWEN35_08B.ffn_intermediate, QWEN35_08B.hidden_size, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_intermediate_to_hidden(entry: &MetalPackEntry) -> Result<(), String> {
    validate_layout("down", entry)?;
    if entry.source_shape != [QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate] {
        return Err(format!(
            "down: expected [{}, {}], got {:?}",
            QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_layout(label: &str, entry: &MetalPackEntry) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
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
