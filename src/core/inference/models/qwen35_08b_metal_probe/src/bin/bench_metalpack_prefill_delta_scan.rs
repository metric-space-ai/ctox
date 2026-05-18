#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_scan is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_scan_with_inputs, PrefillDeltaScanBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::{open_metalpack, QWEN35_08B};
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_scan <metalpack-dir> [layer] [tokens] [iterations] [warmup] [validate_tokens]"
                .to_owned(),
        );
    }

    let root = PathBuf::from(&args[1]);
    let layer = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid layer argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(0);
    let tokens = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid tokens argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(512);
    let iterations = args
        .get(4)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(5);
    let warmup = args
        .get(5)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid warmup argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);
    let validate_tokens = args
        .get(6)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid validate_tokens argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(8);

    // The scan kernel itself has no learned weights. Opening the metalpack keeps
    // this benchmark anchored to the same real-model invocation path as the
    // surrounding DeltaNet projection/conv/prepare measurements.
    let _pack = open_metalpack(&root).map_err(|err| err.to_string())?;

    let width = QWEN35_08B.deltanet_width();
    let heads = QWEN35_08B.deltanet_v_heads;
    let dim = QWEN35_08B.deltanet_head_dim;
    let mut q_host = Vec::with_capacity(tokens * width);
    let mut k_host = Vec::with_capacity(tokens * width);
    let mut v_host = Vec::with_capacity(tokens * width);
    for token in 0..tokens {
        for channel in 0..width {
            let q = (((token * 17 + channel * 7) % 251) as f32 - 125.0) / 2048.0;
            let k = (((token * 19 + channel * 5) % 241) as f32 - 120.0) / 512.0;
            let v = (((token * 23 + channel * 3) % 239) as f32 - 119.0) / 239.0;
            q_host.push(f16::from_f32(q).to_bits());
            k_host.push(f16::from_f32(k).to_bits());
            v_host.push(f16::from_f32(v).to_bits());
        }
    }

    let mut beta_host = Vec::with_capacity(tokens * heads);
    let mut decay_host = Vec::with_capacity(tokens * heads);
    for token in 0..tokens {
        for head in 0..heads {
            beta_host.push(0.15 + ((token * 7 + head * 11) % 31) as f32 / 256.0);
            decay_host.push(0.88 + ((token * 5 + head * 13) % 17) as f32 / 512.0);
        }
    }

    let state_elems = heads * dim * dim;
    let state_host: Vec<f32> = (0..state_elems)
        .map(|i| ((i.wrapping_mul(11) % 127) as f32 - 63.0) / 8192.0)
        .collect();

    let cfg = PrefillDeltaScanBenchConfig {
        tokens,
        warmup,
        iterations,
        validate_tokens,
    };
    let result = run_prefill_deltanet_scan_with_inputs(
        cfg,
        &q_host,
        &k_host,
        &v_host,
        &beta_host,
        &decay_host,
        &state_host,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet scan benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!(
        "shape: tokens={} heads={} head_dim={} state_bytes={}",
        result.tokens, result.heads, result.head_dim, result.state_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("warmup: {}", warmup);
    println!("validate_tokens: {}", validate_tokens);
    println!("kernel: {}", result.kernel_name);
    println!("grid: {}x{}x{}", result.grid.0, result.grid.1, result.grid.2);
    println!(
        "threads: {}x{}x{}",
        result.threads.0, result.threads.1, result.threads.2
    );
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!("bytes_moved_estimate: {}", result.bytes_moved_estimate);
    println!(
        "effective_gb_s_state_scan_estimate: {:.2}",
        result.effective_gb_s
    );
    println!(
        "max_abs_error_out_validate8: {:.9}",
        result.max_abs_error_out
    );
    println!(
        "max_abs_error_state_validate8: {:.9}",
        result.max_abs_error_state
    );
    println!("checksum32: {:.6}", result.checksum);
    Ok(())
}
