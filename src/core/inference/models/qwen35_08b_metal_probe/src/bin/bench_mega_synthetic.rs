#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_mega_synthetic is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_synthetic_mega_bench, SyntheticMegaBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = SyntheticMegaBenchConfig::default();
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
    if let Some(layers) = args.get(3) {
        cfg.layers = layers
            .parse::<usize>()
            .map_err(|err| format!("invalid layers argument `{layers}`: {err}"))?;
    }
    if let Some(token) = args.get(4) {
        cfg.input_token = token
            .parse::<u32>()
            .map_err(|err| format!("invalid token argument `{token}`: {err}"))?;
    }

    let result = run_synthetic_mega_bench(cfg)?;
    println!("qwen35-08b synthetic single-dispatch mega benchmark");
    println!(
        "pipeline: token -> embedding -> {} synthetic RMS/matvec layers -> lm_head_argmax -> next_token",
        result.layers
    );
    println!("shape: vocab={} hidden={}", result.vocab_rows, result.cols);
    println!("input_token: {}", result.input_token);
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!("estimated_gb_s_weight_stream: {:.2}", result.estimated_gb_s);
    println!("next_token: {}", result.next_token);
    println!("score: {:.6}", result.score);
    Ok(())
}
