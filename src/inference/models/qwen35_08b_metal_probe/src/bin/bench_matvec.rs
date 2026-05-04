#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_matvec is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{run_matvec_bench, MatvecBenchConfig};
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = MatvecBenchConfig::default();
    if let Some(rows) = args.get(1) {
        cfg.rows = rows
            .parse::<usize>()
            .map_err(|err| format!("invalid rows argument `{rows}`: {err}"))?;
    }
    if let Some(iterations) = args.get(2) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_matvec_bench(cfg)?;
    println!("qwen35-08b matvec_fp16_k1024 benchmark");
    println!(
        "shape: [{} x {}] @ [{}]",
        result.rows, result.cols, QWEN35_08B.hidden_size
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_weights_plus_io: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}
