#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_deltanet_chunk_phase2 is only available on macOS + Metal.");
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
    let tokens = parse_arg(&args, 1, 128usize, "tokens")?;
    let iterations = parse_arg(&args, 2, 5usize, "iterations")?;
    let warmup = parse_arg(&args, 3, 2usize, "warmup")?;
    let chunk = parse_arg(&args, 4, 8usize, "chunk")?;
    let state_mode = StateMode::parse(args.get(5).map(String::as_str).unwrap_or("f32"))?;
    if ![4usize, 8, 16, 32].contains(&chunk) {
        return Err(format!("unsupported chunk {chunk}; use 4, 8, 16, or 32"));
    }

    let heads = QWEN35_08B.deltanet_v_heads;
    let head_dim = QWEN35_08B.deltanet_head_dim;
    let width = QWEN35_08B.deltanet_width();
    let chunks = tokens.div_ceil(chunk);

    let q_host = fill_half(tokens, width, 17, 3, 512.0);
    let k_host = fill_half(tokens, width, 19, 5, 640.0);
    let v_host = fill_half(tokens, width, 23, 7, 384.0);
    let initial_state_host = fill_state(heads, head_dim);
    let mut beta_host = Vec::with_capacity(tokens * heads);
    let mut decay_host = Vec::with_capacity(tokens * heads);
    for token in 0..tokens {
        for head in 0..heads {
            beta_host.push(0.10 + ((token * 7 + head * 11) % 37) as f32 / 256.0);
            decay_host.push(0.84 + ((token * 5 + head * 13) % 19) as f32 / 512.0);
        }
    }

    let out_elems = tokens * width;
    let state_elems = chunks * heads * head_dim * head_dim;
    let full_state_elems = heads * head_dim * head_dim;
    let local_state_bytes = state_elems * state_mode.bytes_per_element();
    let dev = Device::default_system()?;
    let q = dev.new_buffer(q_host.len() * std::mem::size_of::<u16>())?;
    let k = dev.new_buffer(k_host.len() * std::mem::size_of::<u16>())?;
    let v = dev.new_buffer(v_host.len() * std::mem::size_of::<u16>())?;
    let beta = dev.new_buffer(beta_host.len() * std::mem::size_of::<f32>())?;
    let decay = dev.new_buffer(decay_host.len() * std::mem::size_of::<f32>())?;
    let initial_state = dev.new_buffer(initial_state_host.len() * std::mem::size_of::<f32>())?;
    let local_out = dev.new_buffer(out_elems * std::mem::size_of::<f32>())?;
    let local_state = dev.new_buffer(local_state_bytes)?;
    let final_out = dev.new_buffer(out_elems * std::mem::size_of::<f32>())?;
    let final_state = dev.new_buffer(full_state_elems * std::mem::size_of::<f32>())?;
    unsafe {
        q.write(0, &q_host);
        k.write(0, &k_host);
        v.write(0, &v_host);
        beta.write(0, &beta_host);
        decay.write(0, &decay_host);
        initial_state.write(0, &initial_state_host);
    }

    for _ in 0..warmup {
        dispatch(
            &dev,
            &q,
            &k,
            &v,
            &beta,
            &decay,
            &local_out,
            &local_state,
            tokens as u32,
            chunk as u32,
            state_mode,
        )?;
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch(
            &dev,
            &q,
            &k,
            &v,
            &beta,
            &decay,
            &local_out,
            &local_state,
            tokens as u32,
            chunk as u32,
            state_mode,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut out_host = vec![0.0f32; out_elems];
    let state_host = read_local_state(&local_state, state_elems, state_mode)?;
    unsafe {
        local_out.read(0, &mut out_host);
    }
    let (out_ref, state_ref) = cpu_phase2_local_zero(
        tokens,
        heads,
        head_dim,
        width,
        &q_host,
        &k_host,
        &v_host,
        &beta_host,
        &decay_host,
        chunk,
    );
    let max_out = max_abs_error(&out_host, &out_ref);
    let max_state = max_abs_error(&state_host, &state_ref);
    let mean_out = mean_abs_error(&out_host, &out_ref);
    let mean_state = mean_abs_error(&state_host, &state_ref);
    let out_mismatch = max_abs_error_at(&out_host, &out_ref);
    let state_mismatch = max_abs_error_at(&state_host, &state_ref);

    let start_full = Instant::now();
    dispatch_phase3(
        &dev,
        &q,
        &k,
        &beta,
        &decay,
        &initial_state,
        &local_out,
        &local_state,
        &final_out,
        &final_state,
        tokens as u32,
        chunk as u32,
        state_mode,
    )?;
    let phase3_s = start_full.elapsed().as_secs_f64();

    let mut full_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch(
            &dev,
            &q,
            &k,
            &v,
            &beta,
            &decay,
            &local_out,
            &local_state,
            tokens as u32,
            chunk as u32,
            state_mode,
        )?;
        dispatch_phase3(
            &dev,
            &q,
            &k,
            &beta,
            &decay,
            &initial_state,
            &local_out,
            &local_state,
            &final_out,
            &final_state,
            tokens as u32,
            chunk as u32,
            state_mode,
        )?;
        full_samples.push(start.elapsed().as_secs_f64());
    }
    full_samples.sort_by(|a, b| a.total_cmp(b));
    let full_path_median_s = percentile_sorted(&full_samples, 0.50);
    let full_path_p95_s = percentile_sorted(&full_samples, 0.95);

    let mut final_out_host = vec![0.0f32; out_elems];
    let mut final_state_host = vec![0.0f32; full_state_elems];
    unsafe {
        final_out.read(0, &mut final_out_host);
        final_state.read(0, &mut final_state_host);
    }
    let (full_out_ref, full_state_ref) = cpu_full_serial(
        tokens,
        heads,
        head_dim,
        width,
        &q_host,
        &k_host,
        &v_host,
        &beta_host,
        &decay_host,
        &initial_state_host,
    );
    let max_full_out = max_abs_error(&final_out_host, &full_out_ref);
    let max_full_state = max_abs_error(&final_state_host, &full_state_ref);
    let mean_full_out = mean_abs_error(&final_out_host, &full_out_ref);
    let mean_full_state = mean_abs_error(&final_state_host, &full_state_ref);
    let full_out_mismatch = max_abs_error_at(&final_out_host, &full_out_ref);
    let full_state_mismatch = max_abs_error_at(&final_state_host, &full_state_ref);

    let read_bytes = (q_host.len() + k_host.len() + v_host.len()) * std::mem::size_of::<u16>()
        + (beta_host.len() + decay_host.len()) * std::mem::size_of::<f32>();
    let write_bytes = out_elems * std::mem::size_of::<f32>() + local_state_bytes;

    println!("qwen35-08b DeltaNet chunk8 phase2 local-zero benchmark");
    println!("tokens: {tokens}");
    println!("heads: {heads}");
    println!("head_dim: {head_dim}");
    println!("chunks: {chunks}");
    println!("chunk_tokens: {chunk}");
    println!("state_mode: {}", state_mode.label());
    println!("state_mb: {:.2}", local_state_bytes as f64 / 1e6);
    println!("iterations: {iterations}");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!("phase3_once_s: {phase3_s:.9}");
    println!("full_path_median_s: {full_path_median_s:.9}");
    println!("full_path_p95_s: {full_path_p95_s:.9}");
    println!(
        "effective_gb_s_visible_bytes: {:.2}",
        (read_bytes + write_bytes) as f64 / median_s.max(1e-12) / 1e9
    );
    println!("max_abs_error_out: {max_out:.9}");
    println!("max_abs_error_state: {max_state:.9}");
    println!("mean_abs_error_out: {mean_out:.9}");
    println!("mean_abs_error_state: {mean_state:.9}");
    println!(
        "max_abs_error_out_at: idx={} gpu={:.9} cpu={:.9}",
        out_mismatch.0, out_mismatch.1, out_mismatch.2
    );
    println!(
        "max_abs_error_state_at: idx={} gpu={:.9} cpu={:.9}",
        state_mismatch.0, state_mismatch.1, state_mismatch.2
    );
    println!("checksum_out: {:.6}", checksum(&out_host));
    println!("checksum_state: {:.6}", checksum(&state_host));
    println!("max_abs_error_full_out: {max_full_out:.9}");
    println!("max_abs_error_full_state: {max_full_state:.9}");
    println!("mean_abs_error_full_out: {mean_full_out:.9}");
    println!("mean_abs_error_full_state: {mean_full_state:.9}");
    println!(
        "max_abs_error_full_out_at: idx={} gpu={:.9} cpu={:.9}",
        full_out_mismatch.0, full_out_mismatch.1, full_out_mismatch.2
    );
    println!(
        "max_abs_error_full_state_at: idx={} gpu={:.9} cpu={:.9}",
        full_state_mismatch.0, full_state_mismatch.1, full_state_mismatch.2
    );
    println!("checksum_full_out: {:.6}", checksum(&final_out_host));
    println!("checksum_full_state: {:.6}", checksum(&final_state_host));

    if max_out > 2.0e-5 || max_state > 2.0e-5 || max_full_out > 2.0e-5 || max_full_state > 2.0e-5 {
        return Err("chunk phase2 local-zero validation failed".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn dispatch(
    dev: &ctox_qwen35_08b_metal_probe::metal::ffi::Device,
    q: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    k: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    v: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    beta: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    decay: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    local_out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    local_state: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    tokens: u32,
    chunk: u32,
    state_mode: StateMode,
) -> Result<(), String> {
    let pso = dev.pipeline(state_mode.phase2_kernel())?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, v, 0);
    enc.set_buffer(3, beta, 0);
    enc.set_buffer(4, decay, 0);
    enc.set_buffer(5, local_out, 0);
    enc.set_buffer(6, local_state, 0);
    enc.set_bytes(7, &tokens);
    enc.set_bytes(8, &chunk);
    let chunks = (tokens as usize).div_ceil(chunk.max(1) as usize);
    enc.dispatch_threadgroups(
        (chunks, 16, 128),
        (state_mode.threads_per_threadgroup(), 1, 1),
    );
    enc.end();
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn dispatch_phase3(
    dev: &ctox_qwen35_08b_metal_probe::metal::ffi::Device,
    q: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    k: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    beta: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    decay: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    initial_state: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    local_out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    local_state: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    final_out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    final_state: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    tokens: u32,
    chunk: u32,
    state_mode: StateMode,
) -> Result<(), String> {
    let pso = dev.pipeline(state_mode.phase3_kernel())?;
    let cmd = dev.command_buffer()?;
    let enc = cmd.compute()?;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, q, 0);
    enc.set_buffer(1, k, 0);
    enc.set_buffer(2, beta, 0);
    enc.set_buffer(3, decay, 0);
    enc.set_buffer(4, initial_state, 0);
    enc.set_buffer(5, local_out, 0);
    enc.set_buffer(6, local_state, 0);
    enc.set_buffer(7, final_out, 0);
    enc.set_buffer(8, final_state, 0);
    enc.set_bytes(9, &tokens);
    enc.set_bytes(10, &chunk);
    enc.dispatch_threadgroups((16, 128, 1), (state_mode.threads_per_threadgroup(), 1, 1));
    enc.end();
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum StateMode {
    F32,
    F16,
    F32x4,
    F16x4,
}

#[cfg(target_os = "macos")]
impl StateMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "f32" => Ok(Self::F32),
            "f16" | "hstate" => Ok(Self::F16),
            "f32x4" | "simd32x4" => Ok(Self::F32x4),
            "f16x4" | "simd32x4h" => Ok(Self::F16x4),
            _ => Err(format!(
                "unsupported state mode `{value}`; use f32, f16, f32x4, or f16x4"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F16 => "f16",
            Self::F32x4 => "f32x4",
            Self::F16x4 => "f16x4",
        }
    }

    fn bytes_per_element(self) -> usize {
        match self {
            Self::F32 => std::mem::size_of::<f32>(),
            Self::F16 => std::mem::size_of::<u16>(),
            Self::F32x4 => std::mem::size_of::<f32>(),
            Self::F16x4 => std::mem::size_of::<u16>(),
        }
    }

    fn phase2_kernel(self) -> &'static str {
        match self {
            Self::F32 => "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_h16d128",
            Self::F16 => "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_hstate_h16d128",
            Self::F32x4 => {
                "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_f32state_h16d128"
            }
            Self::F16x4 => {
                "qwen35_08b_prefill_deltanet_chunk8_phase2_local_zero_simd32x4_hstate_h16d128"
            }
        }
    }

    fn phase3_kernel(self) -> &'static str {
        match self {
            Self::F32 => "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_h16d128",
            Self::F16 => "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_hstate_h16d128",
            Self::F32x4 => {
                "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_f32state_h16d128"
            }
            Self::F16x4 => {
                "qwen35_08b_prefill_deltanet_chunk8_phase3_propagate_simd32x4_hstate_h16d128"
            }
        }
    }

    fn threads_per_threadgroup(self) -> usize {
        match self {
            Self::F32 | Self::F16 => 128,
            Self::F32x4 | Self::F16x4 => 32,
        }
    }
}

#[cfg(target_os = "macos")]
fn read_local_state(
    buffer: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    elems: usize,
    state_mode: StateMode,
) -> Result<Vec<f32>, String> {
    match state_mode {
        StateMode::F32 | StateMode::F32x4 => {
            let mut out = vec![0.0f32; elems];
            unsafe {
                buffer.read(0, &mut out);
            }
            Ok(out)
        }
        StateMode::F16 | StateMode::F16x4 => {
            let mut raw = vec![0u16; elems];
            unsafe {
                buffer.read(0, &mut raw);
            }
            Ok(raw
                .into_iter()
                .map(|x| f16::from_bits(x).to_f32())
                .collect())
        }
    }
}

#[cfg(target_os = "macos")]
fn fill_half(tokens: usize, width: usize, a: usize, b: usize, denom: f32) -> Vec<u16> {
    let mut out = Vec::with_capacity(tokens * width);
    for token in 0..tokens {
        for channel in 0..width {
            let x = (((token * a + channel * b) % 251) as f32 - 125.0) / denom;
            out.push(f16::from_f32(x).to_bits());
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn fill_state(heads: usize, head_dim: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(heads * head_dim * head_dim);
    for head in 0..heads {
        for row in 0..head_dim {
            for col in 0..head_dim {
                out.push((((head * 17 + row * 5 + col * 3) % 127) as f32 - 63.0) / 4096.0);
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn cpu_phase2_local_zero(
    tokens: usize,
    heads: usize,
    head_dim: usize,
    width: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    decay: &[f32],
    chunk: usize,
) -> (Vec<f32>, Vec<f32>) {
    let chunks = tokens.div_ceil(chunk);
    let mut out = vec![0.0f32; tokens * width];
    let mut state_chunks = vec![0.0f32; chunks * heads * head_dim * head_dim];

    for chunk_id in 0..chunks {
        let base_token = chunk_id * chunk;
        for head in 0..heads {
            for row in 0..head_dim {
                let mut state = vec![0.0f32; head_dim];
                for local_t in 0..chunk {
                    let token = base_token + local_t;
                    if token >= tokens {
                        break;
                    }
                    let token_base = token * width + head * head_dim;
                    let beta_t = beta[token * heads + head];
                    let decay_t = decay[token * heads + head];
                    let v_row = f16::from_bits(v[token_base + row]).to_f32();
                    let mut kv_mem = 0.0f32;
                    for col in 0..head_dim {
                        let k_col = f16::from_bits(k[token_base + col]).to_f32();
                        kv_mem += state[col] * decay_t * k_col;
                    }
                    let delta = (v_row - kv_mem) * beta_t;
                    let mut acc = 0.0f32;
                    for col in 0..head_dim {
                        let k_col = f16::from_bits(k[token_base + col]).to_f32();
                        let q_col = f16::from_bits(q[token_base + col]).to_f32();
                        state[col] = state[col] * decay_t + k_col * delta;
                        acc += state[col] * q_col;
                    }
                    out[token_base + row] = acc;
                }
                let state_base = ((chunk_id * heads + head) * head_dim + row) * head_dim;
                state_chunks[state_base..state_base + head_dim].copy_from_slice(&state);
            }
        }
    }

    (out, state_chunks)
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn cpu_full_serial(
    tokens: usize,
    heads: usize,
    head_dim: usize,
    width: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    decay: &[f32],
    initial_state: &[f32],
) -> (Vec<f32>, Vec<f32>) {
    let mut out = vec![0.0f32; tokens * width];
    let mut state = initial_state.to_vec();
    for token in 0..tokens {
        for head in 0..heads {
            let token_base = token * width + head * head_dim;
            let beta_t = beta[token * heads + head];
            let decay_t = decay[token * heads + head];
            for row in 0..head_dim {
                let state_base = (head * head_dim + row) * head_dim;
                let v_row = f16::from_bits(v[token_base + row]).to_f32();
                let mut kv_mem = 0.0f32;
                for col in 0..head_dim {
                    let k_col = f16::from_bits(k[token_base + col]).to_f32();
                    kv_mem += state[state_base + col] * decay_t * k_col;
                }
                let delta = (v_row - kv_mem) * beta_t;
                let mut acc = 0.0f32;
                for col in 0..head_dim {
                    let k_col = f16::from_bits(k[token_base + col]).to_f32();
                    let q_col = f16::from_bits(q[token_base + col]).to_f32();
                    state[state_base + col] = state[state_base + col] * decay_t + k_col * delta;
                    acc += state[state_base + col] * q_col;
                }
                out[token_base + row] = acc;
            }
        }
    }
    (out, state)
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
fn mean_abs_error(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len() as f32
}

#[cfg(target_os = "macos")]
fn max_abs_error_at(a: &[f32], b: &[f32]) -> (usize, f32, f32) {
    let mut best = (0usize, 0.0f32, 0.0f32);
    let mut best_err = -1.0f32;
    for (idx, (x, y)) in a.iter().zip(b).enumerate() {
        let err = (x - y).abs();
        if err > best_err {
            best = (idx, *x, *y);
            best_err = err;
        }
    }
    best
}

#[cfg(target_os = "macos")]
fn checksum(values: &[f32]) -> f32 {
    values
        .iter()
        .enumerate()
        .map(|(idx, value)| *value * ((idx % 29) as f32 + 1.0))
        .sum()
}
