#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_deltanet is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_deltanet_step_bench, DeltaNetStepBenchConfig,
    };

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = DeltaNetStepBenchConfig::default();
    if let Some(iterations) = args.get(1) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_deltanet_step_bench(cfg)?;
    println!("qwen35-08b deltanet_step_f32_state benchmark");
    println!("heads: {}", result.heads);
    println!("head_dim: {}", result.head_dim);
    println!("state_bytes: {}", result.state_bytes);
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_state_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("max_abs_error_out: {:.9}", result.max_abs_error_out);
    println!("max_abs_error_state: {:.9}", result.max_abs_error_state);
    println!("checksum32: {:.6}", result.checksum);
    Ok(())
}
