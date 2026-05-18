#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_stream is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{run_stream_bench, StreamBenchConfig};

    let mut cfg = StreamBenchConfig::default();
    let args: Vec<String> = std::env::args().collect();
    if let Some(mib) = args.get(1) {
        let mib = mib
            .parse::<usize>()
            .map_err(|err| format!("invalid MiB argument `{mib}`: {err}"))?;
        cfg.bytes = mib * 1024 * 1024;
    }
    if let Some(iterations) = args.get(2) {
        cfg.iterations = iterations
            .parse::<usize>()
            .map_err(|err| format!("invalid iteration argument `{iterations}`: {err}"))?;
    }

    let result = run_stream_bench(cfg)?;
    println!("qwen35-08b stream_rw benchmark");
    println!("bytes: {}", result.bytes);
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_read_plus_write: {:.2}",
        result.effective_gb_s
    );
    println!("checksum: {}", result.checksum);
    Ok(())
}
