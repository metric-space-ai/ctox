#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_lm_head is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_lm_head_argmax_tiled_with_weights, LmHeadArgmaxTiledBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::{open_metalpack, PackLayout, TensorClass, QWEN35_08B};
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_lm_head <metalpack-dir> [iterations]".to_owned());
    }
    let root = PathBuf::from(&args[1]);
    let iterations = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let entry = pack
        .find_first_class(TensorClass::LmHead)
        .or_else(|| pack.find_first_class(TensorClass::TokenEmbedding))
        .ok_or_else(|| "metalpack has neither lm_head nor token_embedding entry".to_owned())?;
    if entry.dtype != "F16" {
        return Err(format!("expected F16 tensor, got {}", entry.dtype));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape.len() != 2 || entry.source_shape[1] != QWEN35_08B.hidden_size {
        return Err(format!(
            "expected [rows, {}] source shape, got {:?}",
            QWEN35_08B.hidden_size, entry.source_shape
        ));
    }
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
        .map(|i| f16::from_f32(((i % 113) as f32 - 56.0) / 113.0).to_bits())
        .collect::<Vec<_>>();
    let cfg = LmHeadArgmaxTiledBenchConfig {
        vocab_rows: entry.source_shape[0],
        row_tile: entry.row_tile,
        col_tile: entry.col_tile,
        warmup: 1,
        iterations,
    };
    let result = run_lm_head_argmax_tiled_with_weights(cfg, &x_host, &weights)?;

    println!("qwen35-08b metalpack lm_head benchmark");
    println!("metalpack: {}", root.display());
    println!("tensor: {}", entry.tensor);
    println!("shape: [{} x {}]", result.vocab_rows, result.cols);
    println!(
        "tile: rows={} cols={} packed_bytes={}",
        result.row_tile, result.col_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_packed_weights_plus_pairs: {:.2}",
        result.effective_gb_s
    );
    println!("next_token: {}", result.next_token);
    println!("score: {:.6}", result.score);
    Ok(())
}
