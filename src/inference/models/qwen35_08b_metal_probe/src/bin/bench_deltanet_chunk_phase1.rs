#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_deltanet_chunk_phase1 is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use half::f16;

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::ffi::Device;
    use ctox_qwen35_08b_metal_probe::QWEN35_08B;
    use std::time::Instant;

    let args = std::env::args().collect::<Vec<_>>();
    let tokens = parse_arg(&args, 1, 4096usize, "tokens")?;
    let iterations = parse_arg(&args, 2, 10usize, "iterations")?;
    let warmup = parse_arg(&args, 3, 3usize, "warmup")?;

    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let width = QWEN35_08B.deltanet_width();
    let chunk = 8usize;
    let chunks = tokens.div_ceil(chunk);
    let pair_count = chunk * chunk;

    let mut k_host = Vec::with_capacity(tokens * width);
    for token in 0..tokens {
        for channel in 0..width {
            let x = (((token * 19 + channel * 5) % 241) as f32 - 120.0) / 512.0;
            k_host.push(f16::from_f32(x).to_bits());
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

    let out_pairs = chunks * heads * pair_count;
    let out_prefix = chunks * heads * chunk;
    let dev = Device::default_system()?;
    let k = dev.new_buffer(k_host.len() * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(beta_host.len() * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(decay_host.len() * std::mem::size_of::<f32>())?;
    let kdot = dev.new_buffer(out_pairs * std::mem::size_of::<f32>())?;
    let lower = dev.new_buffer(out_pairs * std::mem::size_of::<f32>())?;
    let prefix = dev.new_buffer(out_prefix * std::mem::size_of::<f32>())?;
    unsafe {
        k.write(0, &k_host);
        beta.write(0, &beta_host);
        decay.write(0, &decay_host);
    }

    for _ in 0..warmup {
        dispatch(
            &dev,
            &k,
            &beta,
            &decay,
            &kdot,
            &lower,
            &prefix,
            tokens as u32,
        )?;
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch(
            &dev,
            &k,
            &beta,
            &decay,
            &kdot,
            &lower,
            &prefix,
            tokens as u32,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut kdot_host = vec![0.0f32; out_pairs];
    let mut lower_host = vec![0.0f32; out_pairs];
    let mut prefix_host = vec![0.0f32; out_prefix];
    unsafe {
        kdot.read(0, &mut kdot_host);
        lower.read(0, &mut lower_host);
        prefix.read(0, &mut prefix_host);
    }
    let (kdot_ref, lower_ref, prefix_ref) = cpu_phase1(
        tokens,
        heads,
        head_dim,
        width,
        &k_host,
        &beta_host,
        &decay_host,
    );
    let max_kdot = max_abs_error(&kdot_host, &kdot_ref);
    let max_lower = max_abs_error(&lower_host, &lower_ref);
    let max_prefix = max_abs_error(&prefix_host, &prefix_ref);
    let bytes = tokens * width * std::mem::size_of::<u16>()
        + tokens * heads * std::mem::size_of::<f32>() * 2
        + out_pairs * std::mem::size_of::<f32>() * 2
        + out_prefix * std::mem::size_of::<f32>();

    println!("qwen35-08b DeltaNet chunk8 phase1 kdot benchmark");
    println!("tokens: {tokens}");
    println!("heads: {heads}");
    println!("head_dim: {head_dim}");
    println!("chunks: {chunks}");
    println!("iterations: {iterations}");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!(
        "effective_gb_s_phase1_bytes: {:.2}",
        bytes as f64 / median_s.max(1e-12) / 1e9
    );
    println!("max_abs_error_kdot: {max_kdot:.9}");
    println!("max_abs_error_lower: {max_lower:.9}");
    println!("max_abs_error_prefix: {max_prefix:.9}");
    println!("checksum32: {:.6}", checksum(&lower_host));
    if max_kdot > 1.0e-5 || max_lower > 1.0e-5 || max_prefix > 1.0e-6 {
        return Err("chunk phase1 validation failed".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn dispatch(
    dev: &ctox_qwen35_08b_metal_probe::metal::ffi::Device,
    k: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    beta: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    decay: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    kdot: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    lower: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    prefix: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    tokens: u32,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_prefill_deltanet_chunk8_phase1_kdot_h16d128")?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, k, 0);
    enc.set_buffer(1, beta, 0);
    enc.set_buffer(2, decay, 0);
    enc.set_buffer(3, kdot, 0);
    enc.set_buffer(4, lower, 0);
    enc.set_buffer(5, prefix, 0);
    enc.set_bytes(6, &tokens);
    let chunks = (tokens as usize).div_ceil(8);
    enc.dispatch_threadgroups((chunks, 16, 1), (64, 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
fn cpu_phase1(
    tokens: usize,
    heads: usize,
    head_dim: usize,
    width: usize,
    k: &[u16],
    beta: &[f32],
    decay: &[f32],
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let chunk = 8usize;
    let chunks = tokens.div_ceil(chunk);
    let pair_count = chunk * chunk;
    let mut kdot = vec![0.0f32; chunks * heads * pair_count];
    let mut lower = vec![0.0f32; chunks * heads * pair_count];
    let mut prefix = vec![0.0f32; chunks * heads * chunk];
    for chunk_id in 0..chunks {
        let base_token = chunk_id * chunk;
        for head in 0..heads {
            let out_base = (chunk_id * heads + head) * pair_count;
            let prefix_base = (chunk_id * heads + head) * chunk;
            for i in 0..chunk {
                let token_i = base_token + i;
                if token_i < tokens {
                    let mut p = 1.0f32;
                    for pp in 0..=i {
                        p *= decay[(base_token + pp) * heads + head];
                    }
                    prefix[prefix_base + i] = p;
                }
                for j in 0..chunk {
                    let token_j = base_token + j;
                    let pair = i * chunk + j;
                    if token_i >= tokens || token_j >= tokens {
                        continue;
                    }
                    let mut dot = 0.0f32;
                    for col in 0..head_dim {
                        let a = f16::from_bits(k[token_i * width + head * head_dim + col]).to_f32();
                        let b = f16::from_bits(k[token_j * width + head * head_dim + col]).to_f32();
                        dot += a * b;
                    }
                    kdot[out_base + pair] = dot;
                    if j < i {
                        lower[out_base + pair] = beta[token_i * heads + head] * dot;
                    }
                }
            }
        }
    }
    (kdot, lower, prefix)
}

#[cfg(target_os = "macos")]
fn parse_arg<T: std::str::FromStr>(
    args: &[String],
    idx: usize,
    default: T,
    label: &str,
) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    args.get(idx)
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|err| format!("invalid {label} argument `{value}`: {err}"))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}

#[cfg(target_os = "macos")]
fn percentile_sorted(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values[idx.min(values.len() - 1)]
}

#[cfg(target_os = "macos")]
fn max_abs_error(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

#[cfg(target_os = "macos")]
fn checksum(values: &[f32]) -> f32 {
    values
        .iter()
        .enumerate()
        .map(|(idx, value)| *value * ((idx % 23) as f32 + 1.0))
        .sum()
}
