#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_lm_head is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_lm_head_argmax_bench, LmHeadArgmaxBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = LmHeadArgmaxBenchConfig::default();
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

    let result = run_lm_head_argmax_bench(cfg)?;
    println!("qwen35-08b lm_head_argmax_fp16_k1024 benchmark");
    println!(
        "shape: [{} x {}] @ [{}]",
        result.vocab_rows, result.cols, QWEN35_08B.hidden_size
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_weights_plus_pairs: {:.2}",
        result.effective_gb_s
    );
    println!("next_token: {}", result.next_token);
    println!("score: {:.6}", result.score);
    Ok(())
}
