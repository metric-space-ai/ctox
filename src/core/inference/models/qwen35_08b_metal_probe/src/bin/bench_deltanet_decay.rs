#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_deltanet_decay is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_deltanet_decay_activation_bench, DeltaNetDecayActivationBenchConfig,
    };

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = DeltaNetDecayActivationBenchConfig::default();
    if let Some(iterations) = args.get(1) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_deltanet_decay_activation_bench(cfg)?;
    println!("qwen35-08b deltanet beta+decay activation benchmark");
    println!("heads: {}", result.heads);
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!("max_abs_error_beta: {:.9}", result.max_abs_error_beta);
    println!("max_abs_error_decay: {:.9}", result.max_abs_error_decay);
    println!("checksum: {:.6}", result.checksum);
    Ok(())
}
