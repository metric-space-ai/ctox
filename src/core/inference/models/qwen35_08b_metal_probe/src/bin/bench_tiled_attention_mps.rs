#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_tiled_attention_mps is only available on macOS + Metal/MPS.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::metal::{
    ffi::{Buffer, CommandBuffer, Device},
    mps_sidecar::{device_supports_mps, MpsTiledAttentionPlan},
};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use std::{env, time::Instant};

    let args = env::args().collect::<Vec<_>>();
    let tokens = parse_arg(&args, 1, "tokens")?.unwrap_or(4096);
    let q_tile = parse_arg(&args, 2, "q_tile")?.unwrap_or(256);
    let k_tile = parse_arg(&args, 3, "k_tile")?.unwrap_or(1024);
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(3);
    let warmup = parse_arg(&args, 5, "warmup")?.unwrap_or(1);
    let heads_per_group = parse_arg(&args, 6, "heads_per_group")?.unwrap_or(4);
    let quality_check = parse_arg(&args, 7, "quality_check")?.unwrap_or(0) != 0;
    let qwen_bridge = parse_arg(&args, 8, "qwen_bridge")?.unwrap_or(0) != 0;
    let head_dim = 256usize;

    if tokens == 0 || q_tile == 0 || k_tile == 0 || iterations == 0 || heads_per_group == 0 {
        return Err("tokens, q_tile, k_tile, iterations, and heads_per_group must be > 0".into());
    }

    let dev = Device::default_system()?;
    if !device_supports_mps(&dev) {
        return Err("MPS does not support this Metal device".into());
    }

    let element_bytes = std::mem::size_of::<u16>();
    let q_row_bytes = aligned_row_bytes(head_dim, element_bytes);
    let k_matrix_columns = tokens;
    let k_row_bytes = aligned_row_bytes(k_matrix_columns, element_bytes);
    let v_row_bytes = aligned_row_bytes(head_dim, element_bytes);
    let score_row_bytes = aligned_row_bytes(k_tile, element_bytes);
    let out_row_bytes = aligned_row_bytes(head_dim, element_bytes);
    let q_rows = q_tile * heads_per_group;
    let q_matrix_rows = tokens * heads_per_group;
    let q_bytes = q_matrix_rows * q_row_bytes;
    let k_bytes = head_dim * k_row_bytes;
    let v_bytes = tokens * v_row_bytes;
    let qwen_q_cache_bytes = tokens * 8 * head_dim * element_bytes;
    let qwen_kv_cache_bytes = tokens * 2 * head_dim * element_bytes;
    let score_bytes = q_rows * score_row_bytes;
    let out_bytes = q_rows * out_row_bytes;
    let global_out_bytes = q_matrix_rows * out_row_bytes;
    let row_state_bytes = q_rows * std::mem::size_of::<f32>();

    let q = if qwen_bridge {
        dev.new_buffer(q_bytes)?
    } else {
        seeded_half_buffer(&dev, q_bytes, 0x1234_5678)?
    };
    let k = if qwen_bridge {
        dev.new_buffer(k_bytes)?
    } else {
        seeded_half_buffer(&dev, k_bytes, 0x9abc_def0)?
    };
    let v = if qwen_bridge {
        dev.new_buffer(v_bytes)?
    } else {
        seeded_half_buffer(&dev, v_bytes, 0x0bad_cafe)?
    };
    let qwen_q_cache = if qwen_bridge {
        Some(seeded_half_buffer(&dev, qwen_q_cache_bytes, 0x1234_5678)?)
    } else {
        None
    };
    let qwen_k_cache = if qwen_bridge {
        Some(seeded_half_buffer(&dev, qwen_kv_cache_bytes, 0x9abc_def0)?)
    } else {
        None
    };
    let qwen_v_cache = if qwen_bridge {
        Some(seeded_half_buffer(&dev, qwen_kv_cache_bytes, 0x0bad_cafe)?)
    } else {
        None
    };
    let score = dev.new_buffer(score_bytes)?;
    let prob = dev.new_buffer(score_bytes)?;
    let pv = dev.new_buffer(out_bytes)?;
    let out = dev.new_buffer(out_bytes)?;
    let global_out = dev.new_buffer(global_out_bytes)?;
    let m_state = dev.new_buffer(row_state_bytes)?;
    let l_state = dev.new_buffer(row_state_bytes)?;
    let old_scale = dev.new_buffer(row_state_bytes)?;
    let inv_l = dev.new_buffer(row_state_bytes)?;
    let pv_scale = dev.new_buffer(row_state_bytes)?;

    let plan = MpsTiledAttentionPlan::new(
        &dev,
        tokens,
        q_tile,
        k_tile,
        head_dim,
        heads_per_group,
        q_row_bytes,
        k_row_bytes,
        v_row_bytes,
        score_row_bytes,
        out_row_bytes,
    )?;

    let q_blocks = tokens.div_ceil(q_tile);
    let k_blocks = tokens.div_ceil(k_tile);
    let mut causal_tile_pairs = 0usize;
    for qb in 0..q_blocks {
        let q_last = ((qb + 1) * q_tile).min(tokens) - 1;
        causal_tile_pairs += k_blocks.min(q_last / k_tile + 1);
    }

    for _ in 0..warmup {
        run_once_maybe_bridge(
            &dev,
            &plan,
            tokens,
            q_tile,
            k_tile,
            head_dim,
            heads_per_group,
            q_rows,
            q_blocks,
            k_blocks,
            q_row_bytes / element_bytes,
            k_row_bytes / element_bytes,
            v_row_bytes / element_bytes,
            score_row_bytes / element_bytes,
            out_row_bytes / element_bytes,
            qwen_bridge,
            qwen_q_cache.as_ref(),
            qwen_k_cache.as_ref(),
            qwen_v_cache.as_ref(),
            &q,
            &k,
            &v,
            &score,
            &prob,
            &pv,
            &out,
            &global_out,
            &m_state,
            &l_state,
            &old_scale,
            &inv_l,
            &pv_scale,
        )?;
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        run_once_maybe_bridge(
            &dev,
            &plan,
            tokens,
            q_tile,
            k_tile,
            head_dim,
            heads_per_group,
            q_rows,
            q_blocks,
            k_blocks,
            q_row_bytes / element_bytes,
            k_row_bytes / element_bytes,
            v_row_bytes / element_bytes,
            score_row_bytes / element_bytes,
            out_row_bytes / element_bytes,
            qwen_bridge,
            qwen_q_cache.as_ref(),
            qwen_k_cache.as_ref(),
            qwen_v_cache.as_ref(),
            &q,
            &k,
            &v,
            &score,
            &prob,
            &pv,
            &out,
            &global_out,
            &m_state,
            &l_state,
            &old_scale,
            &inv_l,
            &pv_scale,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));

    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);
    let measured_kv_groups = if qwen_bridge { 2usize } else { 1usize };
    let qk_flops = 2.0 * q_rows as f64 * k_tile as f64 * head_dim as f64 * causal_tile_pairs as f64;
    let total_flops = qk_flops * 2.0 * measured_kv_groups as f64;
    let q_tile_bytes = q_rows * q_row_bytes;
    let k_tile_bytes = head_dim * aligned_row_bytes(k_tile, element_bytes);
    let v_tile_bytes = k_tile * v_row_bytes;
    let qk_bytes_per_pair = q_tile_bytes + k_tile_bytes + score_bytes;
    let pv_bytes_per_pair = score_bytes + v_tile_bytes + out_bytes;
    let bridge_pack_bytes = if qwen_bridge {
        measured_kv_groups * (q_bytes + k_bytes + v_bytes)
    } else {
        0
    };
    let modeled_traffic_bytes =
        measured_kv_groups * causal_tile_pairs * (qk_bytes_per_pair + pv_bytes_per_pair)
            + bridge_pack_bytes;

    println!("qwen35-08b Rust MPS tiled attention prototype");
    println!("device: Metal default device");
    println!("tokens: {tokens}");
    println!("q_tile: {q_tile}");
    println!("k_tile: {k_tile}");
    println!("heads_per_group: {heads_per_group}");
    println!("q_rows_per_tile: {q_rows}");
    println!("head_dim: {head_dim}");
    println!("q_blocks: {q_blocks}");
    println!("k_blocks: {k_blocks}");
    println!("causal_tile_pairs: {causal_tile_pairs}");
    println!("iterations: {iterations}");
    println!("warmup: {warmup}");
    println!("quality_check: {quality_check}");
    println!("qwen_bridge: {qwen_bridge}");
    println!("measured_kv_groups: {measured_kv_groups}");
    println!("backend: Rust C-ABI MPSMatrix QK/PV + MSL SIMD32 softmax/combine/store");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!(
        "effective_tflops_qk_plus_pv: {:.3}",
        total_flops / median_s.max(1e-12) / 1.0e12
    );
    println!(
        "effective_gb_s_modeled_tile_traffic: {:.3}",
        modeled_traffic_bytes as f64 / median_s.max(1e-12) / 1.0e9
    );
    println!(
        "tile_pairs_per_s: {:.3}",
        causal_tile_pairs as f64 / median_s.max(1e-12)
    );
    if quality_check && !qwen_bridge {
        run_quality_check(
            tokens,
            q_tile,
            head_dim,
            heads_per_group,
            q_row_bytes / element_bytes,
            k_row_bytes / element_bytes,
            v_row_bytes / element_bytes,
            out_row_bytes / element_bytes,
            &q,
            &k,
            &v,
            &global_out,
        )?;
    } else if quality_check {
        println!("quality_check_skipped: qwen_bridge layout uses packed scratch buffers");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_once_maybe_bridge(
    dev: &Device,
    plan: &MpsTiledAttentionPlan,
    tokens: usize,
    q_tile: usize,
    k_tile: usize,
    head_dim: usize,
    heads_per_group: usize,
    q_rows: usize,
    q_blocks: usize,
    k_blocks: usize,
    q_row_stride: usize,
    k_row_stride: usize,
    v_row_stride: usize,
    score_row_stride: usize,
    out_row_stride: usize,
    qwen_bridge: bool,
    qwen_q_cache: Option<&Buffer>,
    qwen_k_cache: Option<&Buffer>,
    qwen_v_cache: Option<&Buffer>,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    score: &Buffer,
    prob: &Buffer,
    pv: &Buffer,
    out: &Buffer,
    global_out: &Buffer,
    m_state: &Buffer,
    l_state: &Buffer,
    old_scale: &Buffer,
    inv_l: &Buffer,
    pv_scale: &Buffer,
) -> Result<(), String> {
    let cmd = dev.command_buffer()?;
    let kv_groups = if qwen_bridge { 2 } else { 1 };
    for kv_group in 0..kv_groups {
        if qwen_bridge {
            encode_pack_qwen_group(
                dev,
                &cmd,
                tokens,
                kv_group,
                q_row_stride,
                k_row_stride,
                v_row_stride,
                qwen_q_cache.ok_or_else(|| "missing qwen_q_cache".to_owned())?,
                qwen_k_cache.ok_or_else(|| "missing qwen_k_cache".to_owned())?,
                qwen_v_cache.ok_or_else(|| "missing qwen_v_cache".to_owned())?,
                q,
                k,
                v,
            )?;
        }
        for qb in 0..q_blocks {
            encode_init(dev, &cmd, q_rows, head_dim, m_state, l_state, out)?;
            let q_last = ((qb + 1) * q_tile).min(tokens) - 1;
            let allowed_k_blocks = k_blocks.min(q_last / k_tile + 1);
            for kb in 0..allowed_k_blocks {
                plan.encode_qk(&cmd, q, k, score, qb, kb)?;
                encode_softmax(
                    dev,
                    &cmd,
                    q_rows,
                    k_tile,
                    score_row_stride,
                    qb,
                    kb,
                    q_tile,
                    score,
                    prob,
                    m_state,
                    l_state,
                    old_scale,
                    inv_l,
                    pv_scale,
                )?;
                plan.encode_pv(&cmd, prob, v, pv, kb)?;
                encode_combine(
                    dev,
                    &cmd,
                    q_rows,
                    head_dim,
                    out_row_stride,
                    out,
                    pv,
                    old_scale,
                    inv_l,
                    pv_scale,
                )?;
            }
            encode_store(
                dev,
                &cmd,
                tokens,
                q_tile,
                q_rows,
                head_dim,
                out_row_stride,
                qb,
                out,
                global_out,
            )?;
        }
    }
    let _ = heads_per_group;
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn encode_pack_qwen_group(
    dev: &Device,
    cmd: &CommandBuffer,
    tokens: usize,
    kv_group: usize,
    q_row_stride: usize,
    k_row_stride: usize,
    v_row_stride: usize,
    qwen_q_cache: &Buffer,
    qwen_k_cache: &Buffer,
    qwen_v_cache: &Buffer,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_pack_qwen_qkv_group")?;
    let enc = cmd.compute()?;
    let tokens_u32 = tokens as u32;
    let kv_group_u32 = kv_group as u32;
    let q_row_stride_u32 = q_row_stride as u32;
    let k_row_stride_u32 = k_row_stride as u32;
    let v_row_stride_u32 = v_row_stride as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, qwen_q_cache, 0);
    enc.set_buffer(1, qwen_k_cache, 0);
    enc.set_buffer(2, qwen_v_cache, 0);
    enc.set_buffer(3, q, 0);
    enc.set_buffer(4, k, 0);
    enc.set_buffer(5, v, 0);
    enc.set_bytes(6, &tokens_u32);
    enc.set_bytes(7, &kv_group_u32);
    enc.set_bytes(8, &q_row_stride_u32);
    enc.set_bytes(9, &k_row_stride_u32);
    enc.set_bytes(10, &v_row_stride_u32);
    enc.dispatch_threads(tokens * 4 * 256, 256);
    enc.end();
    Ok(())
}

#[cfg(target_os = "macos")]
fn encode_init(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    head_dim: usize,
    m_state: &Buffer,
    l_state: &Buffer,
    out: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_init_rows")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let head_dim_u32 = head_dim as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, m_state, 0);
    enc.set_buffer(1, l_state, 0);
    enc.set_buffer(2, out, 0);
    enc.set_bytes(3, &q_rows_u32);
    enc.set_bytes(4, &head_dim_u32);
    enc.dispatch_threads(q_rows.max(q_rows * head_dim), 256);
    enc.end();
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn encode_softmax(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    k_tile: usize,
    score_row_stride: usize,
    q_block: usize,
    k_block: usize,
    q_tile: usize,
    score: &Buffer,
    prob: &Buffer,
    m_state: &Buffer,
    l_state: &Buffer,
    old_scale: &Buffer,
    inv_l: &Buffer,
    pv_scale: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_softmax_update_simd32")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let k_tile_u32 = k_tile as u32;
    let score_row_stride_u32 = score_row_stride as u32;
    let q_block_u32 = q_block as u32;
    let k_block_u32 = k_block as u32;
    let q_tile_u32 = q_tile as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, score, 0);
    enc.set_buffer(1, prob, 0);
    enc.set_buffer(2, m_state, 0);
    enc.set_buffer(3, l_state, 0);
    enc.set_buffer(4, old_scale, 0);
    enc.set_buffer(5, inv_l, 0);
    enc.set_buffer(6, pv_scale, 0);
    enc.set_bytes(7, &q_rows_u32);
    enc.set_bytes(8, &k_tile_u32);
    enc.set_bytes(9, &score_row_stride_u32);
    enc.set_bytes(10, &q_block_u32);
    enc.set_bytes(11, &k_block_u32);
    enc.set_bytes(12, &q_tile_u32);
    enc.dispatch_threads(q_rows * 32, 256);
    enc.end();
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn encode_combine(
    dev: &Device,
    cmd: &CommandBuffer,
    q_rows: usize,
    head_dim: usize,
    out_row_stride: usize,
    out: &Buffer,
    pv: &Buffer,
    old_scale: &Buffer,
    inv_l: &Buffer,
    pv_scale: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_combine")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let head_dim_u32 = head_dim as u32;
    let out_row_stride_u32 = out_row_stride as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, out, 0);
    enc.set_buffer(1, pv, 0);
    enc.set_buffer(2, old_scale, 0);
    enc.set_buffer(3, inv_l, 0);
    enc.set_buffer(4, pv_scale, 0);
    enc.set_bytes(5, &q_rows_u32);
    enc.set_bytes(6, &head_dim_u32);
    enc.set_bytes(7, &out_row_stride_u32);
    enc.dispatch_threads(q_rows * head_dim, 256);
    enc.end();
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn encode_store(
    dev: &Device,
    cmd: &CommandBuffer,
    tokens: usize,
    q_tile: usize,
    q_rows: usize,
    head_dim: usize,
    out_row_stride: usize,
    q_block: usize,
    out: &Buffer,
    global_out: &Buffer,
) -> Result<(), String> {
    let pso = dev.pipeline("qwen35_08b_tiled_attention_store_global")?;
    let enc = cmd.compute()?;
    let q_rows_u32 = q_rows as u32;
    let head_dim_u32 = head_dim as u32;
    let out_row_stride_u32 = out_row_stride as u32;
    let q_block_u32 = q_block as u32;
    let q_tile_u32 = q_tile as u32;
    let tokens_u32 = tokens as u32;
    enc.set_pipeline(&pso);
    enc.set_buffer(0, out, 0);
    enc.set_buffer(1, global_out, 0);
    enc.set_bytes(2, &q_rows_u32);
    enc.set_bytes(3, &head_dim_u32);
    enc.set_bytes(4, &out_row_stride_u32);
    enc.set_bytes(5, &out_row_stride_u32);
    enc.set_bytes(6, &q_block_u32);
    enc.set_bytes(7, &q_tile_u32);
    enc.set_bytes(8, &tokens_u32);
    enc.dispatch_threads(q_rows * head_dim, 256);
    enc.end();
    Ok(())
}

#[cfg(target_os = "macos")]
fn seeded_half_buffer(dev: &Device, byte_len: usize, seed: u32) -> Result<Buffer, String> {
    let mut data = vec![0u16; byte_len / 2];
    let mut x = seed;
    for item in &mut data {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mantissa = ((x >> 13) & 0x03ff) as u16;
        *item = 0x3400 | mantissa;
    }
    let buf = dev.new_buffer(byte_len)?;
    unsafe {
        buf.write(0, &data);
    }
    Ok(buf)
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_quality_check(
    tokens: usize,
    q_tile: usize,
    head_dim: usize,
    heads_per_group: usize,
    q_stride: usize,
    k_stride: usize,
    v_stride: usize,
    out_stride: usize,
    q: &Buffer,
    k: &Buffer,
    v: &Buffer,
    global_out: &Buffer,
) -> Result<(), String> {
    let q_rows = q_tile * heads_per_group;
    let q_matrix_rows = tokens * heads_per_group;
    let mut q_host = vec![0u16; q_matrix_rows * q_stride];
    let mut k_host = vec![0u16; head_dim * k_stride];
    let mut v_host = vec![0u16; tokens * v_stride];
    let mut out_host = vec![0u16; q_matrix_rows * out_stride];
    unsafe {
        q.read(0, &mut q_host);
        k.read(0, &mut k_host);
        v.read(0, &mut v_host);
        global_out.read(0, &mut out_host);
    }

    let row_samples = sparse_indices(q_matrix_rows);
    let dim_samples = sparse_indices(head_dim);
    let scale = 1.0f32 / (head_dim as f32).sqrt();
    let mut scores = vec![0.0f32; tokens];
    let mut checked = 0usize;
    let mut max_abs = 0.0f32;
    let mut sum_abs = 0.0f64;
    let mut worst_row = 0usize;
    let mut worst_dim = 0usize;
    let mut worst_ref = 0.0f32;
    let mut worst_gpu = 0.0f32;

    for global_row in row_samples {
        let q_block = global_row / q_rows;
        let local_row = global_row - q_block * q_rows;
        let query_row = local_row / heads_per_group;
        let q_abs = q_block * q_tile + query_row;
        if q_abs >= tokens {
            continue;
        }

        let mut max_score = f32::NEG_INFINITY;
        for key in 0..=q_abs {
            let mut dot = 0.0f32;
            for dim in 0..head_dim {
                dot += half_to_f32(q_host[global_row * q_stride + dim])
                    * half_to_f32(k_host[dim * k_stride + key]);
            }
            let score = dot * scale;
            scores[key] = score;
            max_score = max_score.max(score);
        }

        let mut denom = 0.0f32;
        for score in scores.iter_mut().take(q_abs + 1) {
            let p = (*score - max_score).exp();
            *score = p;
            denom += p;
        }

        for dim in &dim_samples {
            let mut acc = 0.0f32;
            for key in 0..=q_abs {
                acc += scores[key] * half_to_f32(v_host[key * v_stride + *dim]);
            }
            let reference = acc / denom;
            let gpu = half_to_f32(out_host[global_row * out_stride + *dim]);
            let abs = (reference - gpu).abs();
            checked += 1;
            sum_abs += abs as f64;
            if abs > max_abs {
                max_abs = abs;
                worst_row = global_row;
                worst_dim = *dim;
                worst_ref = reference;
                worst_gpu = gpu;
            }
        }
    }

    let mean_abs = if checked == 0 {
        0.0
    } else {
        sum_abs / checked as f64
    };
    println!("quality_checked_points: {checked}");
    println!("quality_mean_abs_error: {mean_abs:.9}");
    println!("quality_max_abs_error: {max_abs:.9}");
    println!("quality_worst_row: {worst_row}");
    println!("quality_worst_dim: {worst_dim}");
    println!("quality_worst_ref: {worst_ref:.9}");
    println!("quality_worst_gpu: {worst_gpu:.9}");
    Ok(())
}

#[cfg(target_os = "macos")]
fn half_to_f32(bits: u16) -> f32 {
    half::f16::from_bits(bits).to_f32()
}

#[cfg(target_os = "macos")]
fn sparse_indices(len: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let mut values = vec![
        0,
        len / 11,
        len / 7,
        len / 3,
        len / 2,
        len * 2 / 3,
        len * 6 / 7,
        len - 1,
    ];
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(target_os = "macos")]
fn aligned_row_bytes(columns: usize, element_bytes: usize) -> usize {
    (columns * element_bytes).div_ceil(128) * 128
}

#[cfg(target_os = "macos")]
fn percentile_sorted(samples: &[f64], q: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let idx = ((samples.len() - 1) as f64 * q).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

#[cfg(target_os = "macos")]
fn parse_arg(args: &[String], idx: usize, name: &str) -> Result<Option<usize>, String> {
    args.get(idx)
        .map(|raw| {
            raw.parse::<usize>()
                .map_err(|err| format!("invalid {name} argument `{raw}`: {err}"))
        })
        .transpose()
}
