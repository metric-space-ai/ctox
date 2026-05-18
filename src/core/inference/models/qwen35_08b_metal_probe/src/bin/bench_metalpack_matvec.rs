#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_matvec is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_matvec_tiled_with_weights, MatvecTiledBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::{open_metalpack, PackLayout, QWEN35_08B};
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_matvec <metalpack-dir> [tensor-name-substring] [iterations]"
                .to_owned(),
        );
    }

    let root = PathBuf::from(&args[1]);
    let tensor_filter = args.get(2).and_then(|arg| arg.to_str());
    let iterations = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(10);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let entry = pack
        .entries
        .iter()
        .find(|entry| {
            entry.dtype == "F16"
                && entry.layout == PackLayout::Fp16RowTiled
                && entry.source_shape.len() == 2
                && entry.source_shape[1] == QWEN35_08B.hidden_size
                && tensor_filter
                    .map(|filter| entry.tensor.contains(filter))
                    .unwrap_or(true)
        })
        .ok_or_else(|| "no matching F16 row-tiled [rows, 1024] tensor in metalpack".to_owned())?;

    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err("packed tensor byte length is not divisible by two".to_owned());
    }
    let weights = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let x_host = (0..QWEN35_08B.hidden_size)
        .map(|i| f16::from_f32(((i % 97) as f32 - 48.0) / 97.0).to_bits())
        .collect::<Vec<_>>();

    let cfg = MatvecTiledBenchConfig {
        rows: entry.source_shape[0],
        row_tile: entry.row_tile,
        col_tile: entry.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_matvec_tiled_with_weights(cfg, &x_host, &weights)?;

    println!("qwen35-08b metalpack matvec benchmark");
    println!("metalpack: {}", root.display());
    println!("tensor: {}", entry.tensor);
    println!("class: {}", entry.class.as_str());
    println!("shape: [{} x {}]", result.rows, result.cols);
    println!(
        "tile: rows={} cols={} packed_bytes={}",
        result.row_tile, result.col_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_packed_weights_plus_io: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}
