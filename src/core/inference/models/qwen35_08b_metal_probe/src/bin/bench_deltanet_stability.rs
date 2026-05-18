#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_deltanet_stability is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{metal::ffi::Device, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use half::f16;
    use std::env;

    let args = env::args().collect::<Vec<_>>();
    let steps = args
        .get(1)
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid steps argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    if steps == 0 {
        return Err("steps must be > 0".to_owned());
    }

    let heads = QWEN35_08B.deltanet_v_heads;
    let dim = QWEN35_08B.deltanet_head_dim;
    let vec_elems = heads * dim;
    let state_elems = heads * dim * dim;

    let dev = Device::default_system()?;
    let q_buf = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let k_buf = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let v_buf = dev.new_buffer(vec_elems * std::mem::size_of::<u16>())?;
    let beta_buf = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let gate_buf = dev.new_buffer(heads * std::mem::size_of::<f32>())?;
    let state_buf = dev.new_buffer(state_elems * std::mem::size_of::<f32>())?;
    let out_buf = dev.new_buffer(vec_elems * std::mem::size_of::<f32>())?;
    let step_kernel = if std::env::var_os("CTOX_QWEN35_DECODE_DELTA_ROWCACHE").is_some() {
        "qwen35_08b_deltanet_step_rowcache_f32_state"
    } else {
        "qwen35_08b_deltanet_step_f32_state"
    };
    let pso = dev.pipeline(step_kernel)?;

    let mut cpu_state = vec![0.0f32; state_elems];
    let mut cpu_out = vec![0.0f32; vec_elems];
    let zero_state = vec![0.0f32; state_elems];
    unsafe {
        state_buf.write(0, &zero_state);
    }
    let mut samples = Vec::with_capacity(steps);

    for step in 0..steps {
        let (q, k, v) = normalized_trace(step, heads, dim);
        let beta = beta_trace(step, heads);
        let gate = decay_trace(step, heads);

        unsafe {
            q_buf.write(0, &q);
            k_buf.write(0, &k);
            v_buf.write(0, &v);
            beta_buf.write(0, &beta);
            gate_buf.write(0, &gate);
        }

        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        enc.set_pipeline(&pso);
        enc.set_buffer(0, &q_buf, 0);
        enc.set_buffer(1, &k_buf, 0);
        enc.set_buffer(2, &v_buf, 0);
        enc.set_buffer(3, &beta_buf, 0);
        enc.set_buffer(4, &gate_buf, 0);
        enc.set_buffer(5, &state_buf, 0);
        enc.set_buffer(6, &out_buf, 0);
        enc.dispatch_threadgroups((heads, 1, 1), (128, 1, 1));
        enc.end();
        let start = std::time::Instant::now();
        cmd.commit_and_wait()?;
        samples.push(start.elapsed().as_secs_f64());

        cpu_step(
            heads,
            dim,
            &q,
            &k,
            &v,
            &beta,
            &gate,
            &mut cpu_state,
            &mut cpu_out,
        );
    }

    let mut gpu_state = vec![0.0f32; state_elems];
    let mut gpu_out = vec![0.0f32; vec_elems];
    unsafe {
        state_buf.read(0, &mut gpu_state);
        out_buf.read(0, &mut gpu_out);
    }

    let max_abs_error_out = max_abs_error(&gpu_out, &cpu_out);
    let max_abs_error_state = max_abs_error(&gpu_state, &cpu_state);
    let tolerance = 0.0005f32;
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    println!("qwen35-08b DeltaNet normalized multistep stability check");
    println!("kernel: {step_kernel}");
    println!("steps: {steps}");
    println!("heads: {heads}");
    println!("head_dim: {dim}");
    println!("median_step_s: {median_s:.9}");
    println!("p95_step_s: {p95_s:.9}");
    println!("max_abs_error_out: {max_abs_error_out:.9}");
    println!("max_abs_error_state: {max_abs_error_state:.9}");
    println!("tolerance: {tolerance:.9}");
    if max_abs_error_out > tolerance || max_abs_error_state > tolerance {
        return Err(format!(
            "DeltaNet stability check failed: out={max_abs_error_out:.9}, state={max_abs_error_state:.9}, tolerance={tolerance:.9}"
        ));
    }

    // Keep f16 in scope for target-specific dependency validation in this bin.
    let _ = f16::from_bits(0).to_f32();
    Ok(())
}

#[cfg(target_os = "macos")]
fn normalized_trace(step: usize, heads: usize, dim: usize) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let mut q = vec![0u16; heads * dim];
    let mut k = vec![0u16; heads * dim];
    let mut v = vec![0u16; heads * dim];
    for head in 0..heads {
        let base = head * dim;
        let mut q_raw = vec![0.0f32; dim];
        let mut k_raw = vec![0.0f32; dim];
        for i in 0..dim {
            q_raw[i] = centered(step, head, i, 17, 193);
            k_raw[i] = centered(step, head, i, 29, 211);
            v[base + i] = half::f16::from_f32(centered(step, head, i, 31, 181) * 0.125).to_bits();
        }
        let q_inv = 1.0 / (q_raw.iter().map(|x| x * x).sum::<f32>() + 1.0e-6).sqrt();
        let k_inv = 1.0 / (k_raw.iter().map(|x| x * x).sum::<f32>() + 1.0e-6).sqrt();
        let q_scale = 1.0 / (dim as f32).sqrt();
        for i in 0..dim {
            q[base + i] = half::f16::from_f32(q_raw[i] * q_inv * q_scale).to_bits();
            k[base + i] = half::f16::from_f32(k_raw[i] * k_inv).to_bits();
        }
    }
    (q, k, v)
}

#[cfg(target_os = "macos")]
fn centered(step: usize, head: usize, i: usize, mul: usize, modulo: usize) -> f32 {
    let raw = ((step * 37 + head * 13 + i).wrapping_mul(mul) % modulo) as f32;
    (raw - modulo as f32 * 0.5) / modulo as f32
}

#[cfg(target_os = "macos")]
fn beta_trace(step: usize, heads: usize) -> Vec<f32> {
    (0..heads)
        .map(|head| 0.12 + ((step + head) % 5) as f32 * 0.015)
        .collect()
}

#[cfg(target_os = "macos")]
fn decay_trace(step: usize, heads: usize) -> Vec<f32> {
    (0..heads)
        .map(|head| 0.82 + ((step * 3 + head) % 7) as f32 * 0.015)
        .collect()
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn cpu_step(
    heads: usize,
    dim: usize,
    q: &[u16],
    k: &[u16],
    v: &[u16],
    beta: &[f32],
    gate: &[f32],
    state: &mut [f32],
    out: &mut [f32],
) {
    let prev_state = state.to_vec();
    for head in 0..heads {
        let vec_base = head * dim;
        let state_base = head * dim * dim;
        for i in 0..dim {
            let mut kv_mem = 0.0f32;
            for j in 0..dim {
                kv_mem += prev_state[state_base + i * dim + j]
                    * gate[head]
                    * half::f16::from_bits(k[vec_base + j]).to_f32();
            }
            let delta = (half::f16::from_bits(v[vec_base + i]).to_f32() - kv_mem) * beta[head];
            for j in 0..dim {
                state[state_base + i * dim + j] = prev_state[state_base + i * dim + j] * gate[head]
                    + half::f16::from_bits(k[vec_base + j]).to_f32() * delta;
            }
        }
        for i in 0..dim {
            let mut acc = 0.0f32;
            for j in 0..dim {
                acc += state[state_base + i * dim + j]
                    * half::f16::from_bits(q[vec_base + j]).to_f32();
            }
            out[vec_base + i] = acc;
        }
    }
}

#[cfg(target_os = "macos")]
fn max_abs_error(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x - *y).abs())
        .fold(0.0f32, f32::max)
}

#[cfg(target_os = "macos")]
fn percentile_sorted(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[idx.min(values.len() - 1)]
}
