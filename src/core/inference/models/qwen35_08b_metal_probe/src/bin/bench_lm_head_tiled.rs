#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_lm_head_tiled is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_lm_head_argmax_tiled_bench, LmHeadArgmaxTiledBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = LmHeadArgmaxTiledBenchConfig::default();
    if let Some(rows) = args.get(1) {
        cfg.vocab_rows = if rows == "full" {
            QWEN35_08B.vocab_size
        } else {
            rows.parse::<usize>()
                .map_err(|err| format!("invalid vocab rows argument `{rows}`: {err}"))?
        };
    }
    if let Some(iterations) = args.get(2) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_lm_head_argmax_tiled_bench(cfg)?;
    println!("qwen35-08b lm_head_argmax_fp16_tiled_k1024 benchmark");
    println!(
        "shape: [{} x {}] @ [{}]",
        result.vocab_rows, result.cols, QWEN35_08B.hidden_size
    );
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
