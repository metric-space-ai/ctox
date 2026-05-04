#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_down is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_down_matmul_with_weights, PrefillDownMatmulBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_down <metalpack-dir> [layer] [tokens] [iterations]"
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
    let down = find_tensor(&pack, &prefix, "mlp.down_proj.weight")?;
    validate_down(down)?;

    let down_weights = read_u16_entry(&pack, down)?;
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.ffn_intermediate);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.ffn_intermediate {
            let v = (((token * 29 + col * 19) % 263) as f32 - 131.0) / 263.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }
    let cfg = PrefillDownMatmulBenchConfig {
        tokens,
        row_tile: down.row_tile,
        col_tile: down.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_down_matmul_with_weights(cfg, &x_host, &down_weights)?;

    println!("qwen35-08b metalpack prefill FFN down benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("down: {}", down.tensor);
    println!(
        "shape: tokens={} rows={} cols={}",
        result.tokens, result.rows, result.cols
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
fn validate_down(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "down: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != [QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate] {
        return Err(format!(
            "down: expected [{}, {}], got {:?}",
            QWEN35_08B.hidden_size, QWEN35_08B.ffn_intermediate, entry.source_shape
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
