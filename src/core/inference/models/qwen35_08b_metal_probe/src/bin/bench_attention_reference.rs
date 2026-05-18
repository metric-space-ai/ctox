#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_attention_reference is only available on macOS + Metal.");
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
        .unwrap_or(4);
    let max_context = args
        .get(2)
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid max_context argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(steps);
    if steps == 0 || max_context == 0 || steps > max_context {
        return Err("expected 0 < steps <= max_context for the reference check".to_owned());
    }

    let q_heads = QWEN35_08B.attention_q_heads;
    let kv_heads = QWEN35_08B.attention_kv_heads;
    let head_dim = QWEN35_08B.attention_head_dim;
    let q_width = QWEN35_08B.attention_q_width();
    let kv_width = QWEN35_08B.attention_kv_width();
    let q_rows = QWEN35_08B.attention_q_with_head_gate_width();

    let dev = Device::default_system()?;
    let q_buf = dev.new_buffer(q_rows * std::mem::size_of::<f32>())?;
    let k_buf = dev.new_buffer(kv_width * std::mem::size_of::<f32>())?;
    let v_buf = dev.new_buffer(kv_width * std::mem::size_of::<f32>())?;
    let k_cache_buf = dev.new_buffer(max_context * kv_width * std::mem::size_of::<u16>())?;
    let v_cache_buf = dev.new_buffer(max_context * kv_width * std::mem::size_of::<u16>())?;
    let out_buf = dev.new_buffer(q_width * std::mem::size_of::<u16>())?;
    let pso = dev.pipeline("qwen35_08b_attention_single_token_gqa8_kv2_d256_rope_cache_to_fp16")?;

    let mut cpu_k_cache = vec![0.0f32; max_context * kv_width];
    let mut cpu_v_cache = vec![0.0f32; max_context * kv_width];
    let mut max_abs_err = 0.0f32;

    for position in 0..steps {
        let q = synthetic_q(position, q_rows, q_width, q_heads);
        let k = synthetic_projection(position, kv_width, 19, 151);
        let v = synthetic_projection(position, kv_width, 23, 173);

        unsafe {
            q_buf.write(0, &q);
            k_buf.write(0, &k);
            v_buf.write(0, &v);
        }

        let pos_u32 = u32::try_from(position).map_err(|_| "position exceeds u32")?;
        let max_context_u32 = u32::try_from(max_context).map_err(|_| "max_context exceeds u32")?;
        let q_rows_u32 = u32::try_from(q_rows).map_err(|_| "q_rows exceeds u32")?;
        let cmd = dev.command_buffer()?;
        let enc = cmd.compute()?;
        enc.set_pipeline(&pso);
        enc.set_buffer(0, &q_buf, 0);
        enc.set_buffer(1, &k_buf, 0);
        enc.set_buffer(2, &v_buf, 0);
        enc.set_buffer(3, &k_cache_buf, 0);
        enc.set_buffer(4, &v_cache_buf, 0);
        enc.set_buffer(5, &out_buf, 0);
        enc.set_bytes(6, &q_rows_u32);
        enc.set_bytes(7, &pos_u32);
        enc.set_bytes(8, &max_context_u32);
        enc.dispatch_threadgroups((q_heads, 1, 1), (256, 1, 1));
        enc.end();
        cmd.commit_and_wait()?;

        update_cpu_cache(
            position,
            max_context,
            kv_heads,
            head_dim,
            &k,
            &v,
            &mut cpu_k_cache,
            &mut cpu_v_cache,
        );
        let expected = cpu_attention(
            position,
            max_context,
            q_heads,
            kv_heads,
            head_dim,
            q_width,
            &q,
            &cpu_k_cache,
            &cpu_v_cache,
        );

        let mut got_bits = vec![0u16; q_width];
        unsafe {
            out_buf.read(0, &mut got_bits);
        }
        for (got, want) in got_bits.iter().zip(expected.iter()) {
            let got = f16::from_bits(*got).to_f32();
            max_abs_err = max_abs_err.max((got - *want).abs());
        }
    }

    let tolerance = 0.0025f32;
    println!("qwen35-08b attention RoPE/KV-cache reference check");
    println!("steps: {steps}");
    println!("max_context: {max_context}");
    println!("heads: q={q_heads} kv={kv_heads} dim={head_dim}");
    println!("max_abs_err: {max_abs_err:.8}");
    println!("tolerance: {tolerance:.8}");
    if max_abs_err > tolerance {
        return Err(format!(
            "attention reference check failed: max_abs_err {max_abs_err:.8} > {tolerance:.8}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn synthetic_projection(position: usize, len: usize, mul: usize, modulo: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let raw = ((i + position * 31).wrapping_mul(mul) % modulo) as f32;
            (raw - (modulo as f32 * 0.5)) / (modulo as f32 * 2.0)
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn synthetic_q(position: usize, len: usize, q_width: usize, q_heads: usize) -> Vec<f32> {
    let mut q = synthetic_projection(position, len, 17, 197);
    for head in 0..q_heads {
        let base = if len >= q_width * 2 {
            head * QWEN35_08B.attention_head_dim * 2
        } else {
            head * QWEN35_08B.attention_head_dim
        };
        for d in 0..QWEN35_08B.attention_head_dim {
            let gate_idx = if len >= q_width * 2 {
                base + QWEN35_08B.attention_head_dim + d
            } else {
                q_width + head
            };
            q[gate_idx] = ((head as f32 - 3.5) * 0.125)
                + (d as f32 - 127.5) / 4096.0
                + position as f32 * 0.03125;
        }
    }
    q
}

#[cfg(target_os = "macos")]
fn update_cpu_cache(
    position: usize,
    max_context: usize,
    kv_heads: usize,
    head_dim: usize,
    k: &[f32],
    v: &[f32],
    k_cache: &mut [f32],
    v_cache: &mut [f32],
) {
    let kv_width = kv_heads * head_dim;
    let cache_pos = position.min(max_context - 1);
    for kv_head in 0..kv_heads {
        let base = kv_head * head_dim;
        let cache_base = cache_pos * kv_width + base;
        for d in 0..head_dim {
            let k_value = rope_component(k, base, d, position);
            k_cache[cache_base + d] = half::f16::from_f32(k_value).to_f32();
            v_cache[cache_base + d] = half::f16::from_f32(v[base + d]).to_f32();
        }
    }
}

#[cfg(target_os = "macos")]
fn cpu_attention(
    position: usize,
    max_context: usize,
    q_heads: usize,
    kv_heads: usize,
    head_dim: usize,
    q_width: usize,
    q: &[f32],
    k_cache: &[f32],
    v_cache: &[f32],
) -> Vec<f32> {
    let kv_width = kv_heads * head_dim;
    let mut out = vec![0.0f32; q_width];
    let n_ctx = (position + 1).min(max_context);
    for q_head in 0..q_heads {
        let kv_head = (q_head / (q_heads / kv_heads)).min(kv_heads - 1);
        let q_base = if q.len() >= q_width * 2 {
            q_head * head_dim * 2
        } else {
            q_head * head_dim
        };
        let q_out_base = q_head * head_dim;
        let kv_base = kv_head * head_dim;
        let mut scores = vec![0.0f32; n_ctx];
        for t in 0..n_ctx {
            let t_base = t * kv_width + kv_base;
            let mut dot = 0.0f32;
            for d in 0..head_dim {
                dot += rope_component(q, q_base, d, position) * k_cache[t_base + d];
            }
            scores[t] = dot * 0.0625;
        }
        let max_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let denom = scores.iter().map(|s| (*s - max_score).exp()).sum::<f32>();
        for d in 0..head_dim {
            let gate = if q.len() >= q_width * 2 {
                sigmoid(q[q_base + head_dim + d])
            } else {
                sigmoid(q[q_width + q_head])
            };
            let mut acc = 0.0f32;
            for t in 0..n_ctx {
                let weight = (scores[t] - max_score).exp() / denom;
                acc += weight * v_cache[t * kv_width + kv_base + d];
            }
            out[q_out_base + d] = half::f16::from_f32(acc * gate).to_f32();
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn rope_component(x: &[f32], base: usize, d: usize, position: usize) -> f32 {
    const THETA: f32 = 10_000_000.0;
    const ROPE_DIM: usize = 64;
    if d >= ROPE_DIM {
        return x[base + d];
    }
    let pair = d % (ROPE_DIM / 2);
    let angle = position as f32 * THETA.powf(-(2.0 * pair as f32) / ROPE_DIM as f32);
    let c = angle.cos();
    let s = angle.sin();
    let a = x[base + (d % (ROPE_DIM / 2))];
    let b = x[base + (d % (ROPE_DIM / 2)) + (ROPE_DIM / 2)];
    if d < ROPE_DIM / 2 {
        a * c - b * s
    } else {
        b * c + a * s
    }
}

#[cfg(target_os = "macos")]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
