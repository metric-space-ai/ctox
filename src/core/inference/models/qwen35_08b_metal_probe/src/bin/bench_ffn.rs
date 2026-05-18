#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_ffn is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{run_ffn_swiglu_bench, FfnSwiGluBenchConfig};

    let args: Vec<String> = std::env::args().collect();
    let mut cfg = FfnSwiGluBenchConfig::default();
    if let Some(iterations) = args.get(1) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iterations argument `{iterations}`: {err}"))?;
    }

    let result = run_ffn_swiglu_bench(cfg)?;
    println!("qwen35-08b fused_ffn_swiglu_fp16 benchmark");
    println!("hidden: {}", result.hidden);
    println!("intermediate: {}", result.intermediate);
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!("effective_gb_s_weight_stream: {:.2}", result.effective_gb_s);
    println!("checksum32: {:.6}", result.checksum);
    Ok(())
}
