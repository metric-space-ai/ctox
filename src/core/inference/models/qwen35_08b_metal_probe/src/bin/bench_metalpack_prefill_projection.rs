#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_projection is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_rms_matmul_with_weights, PrefillRmsMatmulBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_projection <metalpack-dir> [tensor-filter] [tokens] [iterations]"
                .to_owned(),
        );
    }

    let root = PathBuf::from(&args[1]);
    let tensor_filter = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .unwrap_or("gate_proj");
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
    let entry = pack
        .entries
        .iter()
        .find(|entry| {
            entry.tensor.contains(tensor_filter)
                && entry.layout == PackLayout::Fp16RowTiled
                && entry.source_shape.len() == 2
                && entry.source_shape[1] == QWEN35_08B.hidden_size
        })
        .ok_or_else(|| {
            format!(
                "no fp16 row-tiled [rows, {}] tensor matching `{tensor_filter}`",
                QWEN35_08B.hidden_size
            )
        })?;
    validate_entry(entry)?;

    let weights = read_u16_entry(&pack, entry)?;
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.hidden_size);
    for token in 0..tokens {
        for col in 0..QWEN35_08B.hidden_size {
            let v = (((token * 31 + col * 17) % 257) as f32 - 128.0) / 257.0;
            x_host.push(f16::from_f32(v).to_bits());
        }
    }
    let norm = (0..QWEN35_08B.hidden_size)
        .map(|i| f16::from_f32(1.0 + ((i % 17) as f32 - 8.0) / 256.0).to_bits())
        .collect::<Vec<_>>();
    let cfg = PrefillRmsMatmulBenchConfig {
        tokens,
        rows: entry.source_shape[0],
        row_tile: entry.row_tile,
        col_tile: entry.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_rms_matmul_with_weights(cfg, &x_host, &norm, &weights)?;

    println!("qwen35-08b metalpack prefill projection benchmark");
    println!("metalpack: {}", root.display());
    println!("tensor: {}", entry.tensor);
    println!("class: {}", entry.class.as_str());
    println!(
        "shape: tokens={} [{} x {}]",
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
fn validate_entry(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.row_tile != 8 {
        return Err(format!(
            "{}: expected row_tile=8, got {}",
            entry.tensor, entry.row_tile
        ));
    }
    if !QWEN35_08B.hidden_size.is_multiple_of(entry.col_tile) {
        return Err(format!(
            "{}: hidden size {} is not a multiple of col_tile {}",
            entry.tensor, QWEN35_08B.hidden_size, entry.col_tile
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
