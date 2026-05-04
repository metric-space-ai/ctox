#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_prefill_rms_matmul is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_rms_matmul_bench, PrefillRmsMatmulBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = PrefillRmsMatmulBenchConfig::default();
    if let Some(tokens) = args.get(1) {
        cfg.tokens = tokens
            .parse::<usize>()
            .map_err(|err| format!("invalid tokens argument `{tokens}`: {err}"))?;
    }
    if let Some(rows) = args.get(2) {
        cfg.rows = rows
            .parse::<usize>()
            .map_err(|err| format!("invalid rows argument `{rows}`: {err}"))?;
    }
    if let Some(iterations) = args.get(3) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_prefill_rms_matmul_bench(cfg)?;
    println!("qwen35-08b prefill fused_rms_matmul_fp16_tiled_k1024 benchmark");
    println!(
        "shape: tokens={} [{} x {}] @ hidden={}",
        result.tokens, result.rows, result.cols, QWEN35_08B.hidden_size
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
