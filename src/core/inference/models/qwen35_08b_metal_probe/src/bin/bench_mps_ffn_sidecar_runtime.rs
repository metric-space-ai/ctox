#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_mps_ffn_sidecar_runtime is only available on macOS + Metal/MPS.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{
    metal::{ffi::Device, mps_sidecar::MpsFfnPlan},
    QWEN35_08B,
};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use serde_json::Value;
    use std::{env, fs, path::PathBuf, time::Instant};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_mps_ffn_sidecar_runtime <mps-ffn-sidecar-dir> [layer] [tokens] [iterations] [warmup] [include-norm 0|1]"
                .to_owned(),
        );
    }
    let sidecar = PathBuf::from(&args[1]);
    let layer = parse_arg(&args, 2, "layer")?.unwrap_or(0);
    let tokens = parse_arg(&args, 3, "tokens")?.unwrap_or(4096);
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(3);
    let warmup = parse_arg(&args, 5, "warmup")?.unwrap_or(1);
    let include_norm = parse_arg(&args, 6, "include-norm")?.unwrap_or(1) != 0;
    let hidden = QWEN35_08B.hidden_size;
    let intermediate = QWEN35_08B.ffn_intermediate;
    if tokens == 0 || iterations == 0 {
        return Err("tokens and iterations must be > 0".to_owned());
    }

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str) != Some("ctox.qwen35_08b.mps_ffn_sidecar") {
        return Err(format!(
            "invalid MPS FFN sidecar: {}",
            manifest_path.display()
        ));
    }
    let weights_file = manifest
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing weights_file".to_owned())?;
    let entries = manifest
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "missing entries".to_owned())?;
    let entry = entries
        .iter()
        .find(|entry| entry.get("layer").and_then(Value::as_u64) == Some(layer as u64))
        .ok_or_else(|| format!("missing layer {layer} in sidecar"))?;
    let gate_up = entry
        .get("gate_up")
        .ok_or_else(|| "missing gate_up entry".to_owned())?;
    let down = entry
        .get("down")
        .ok_or_else(|| "missing down entry".to_owned())?;
    let gate_up_offset = json_usize(gate_up, "offset")?;
    let gate_up_bytes = json_usize(gate_up, "bytes")?;
    let down_offset = json_usize(down, "offset")?;
    let down_bytes = json_usize(down, "bytes")?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;
    if gate_up_offset + gate_up_bytes > weights.len() || down_offset + down_bytes > weights.len() {
        return Err("sidecar weight offsets exceed weights.bin length".to_owned());
    }

    let element_bytes = std::mem::size_of::<u16>();
    let x_row_bytes = aligned_row_bytes(hidden, element_bytes);
    let gate_up_weight_row_bytes = aligned_row_bytes(intermediate * 2, element_bytes);
    let gate_up_row_bytes = aligned_row_bytes(intermediate * 2, element_bytes);
    let act_row_bytes = aligned_row_bytes(intermediate, element_bytes);
    let down_weight_row_bytes = aligned_row_bytes(hidden, element_bytes);
    let out_row_bytes = aligned_row_bytes(hidden, element_bytes);

    if gate_up_bytes != hidden * gate_up_weight_row_bytes {
        return Err(format!(
            "gate_up bytes mismatch: expected {}, got {gate_up_bytes}",
            hidden * gate_up_weight_row_bytes
        ));
    }
    if down_bytes != intermediate * down_weight_row_bytes {
        return Err(format!(
            "down bytes mismatch: expected {}, got {down_bytes}",
            intermediate * down_weight_row_bytes
        ));
    }

    let dev = Device::default_system()?;
    let plan = MpsFfnPlan::new(
        &dev,
        tokens,
        hidden,
        intermediate,
        x_row_bytes,
        gate_up_weight_row_bytes,
        gate_up_row_bytes,
        act_row_bytes,
        down_weight_row_bytes,
        out_row_bytes,
    )?;
    let norm_pso = if include_norm {
        Some(dev.pipeline("qwen35_08b_prefill_rmsnorm_fp16_k1024")?)
    } else {
        None
    };
    let swiglu_pso = dev.pipeline("qwen35_08b_mps_swiglu_gateup_fp16_i3584")?;

    let x = dev.new_buffer(tokens * x_row_bytes)?;
    let norm = dev.new_buffer(hidden * element_bytes)?;
    let normed = dev.new_buffer(tokens * x_row_bytes)?;
    let gate_up_weight = dev.new_buffer(gate_up_bytes)?;
    let gate_up_out = dev.new_buffer(tokens * gate_up_row_bytes)?;
    let act = dev.new_buffer(tokens * act_row_bytes)?;
    let down_weight = dev.new_buffer(down_bytes)?;
    let out = dev.new_buffer(tokens * out_row_bytes)?;

    let mut x_host = vec![0u16; tokens * x_row_bytes / element_bytes];
    fill_half_words(&mut x_host, 0x1234_5678);
    let mut norm_host = vec![0u16; hidden];
    fill_half_words(&mut norm_host, 0x5eed_1234);
    unsafe {
        x.write(0, &x_host);
        norm.write(0, &norm_host);
        gate_up_weight.write(0, &weights[gate_up_offset..gate_up_offset + gate_up_bytes]);
        down_weight.write(0, &weights[down_offset..down_offset + down_bytes]);
    }

    for _ in 0..warmup {
        dispatch_runtime_once(
            &dev,
            &plan,
            norm_pso.as_deref(),
            &swiglu_pso,
            &x,
            &norm,
            &normed,
            &gate_up_weight,
            &gate_up_out,
            &act,
            &down_weight,
            &out,
            tokens,
            intermediate,
            gate_up_row_bytes / element_bytes,
            act_row_bytes / element_bytes,
            include_norm,
        )?;
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch_runtime_once(
            &dev,
            &plan,
            norm_pso.as_deref(),
            &swiglu_pso,
            &x,
            &norm,
            &normed,
            &gate_up_weight,
            &gate_up_out,
            &act,
            &down_weight,
            &out,
            tokens,
            intermediate,
            gate_up_row_bytes / element_bytes,
            act_row_bytes / element_bytes,
            include_norm,
        )?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0u16; hidden.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().fold(0u64, |acc, v| acc + u64::from(*v));
    let flops = 2.0 * tokens as f64 * hidden as f64 * (intermediate * 2) as f64
        + 2.0 * tokens as f64 * intermediate as f64 * hidden as f64;

    println!("qwen35-08b Rust MPS FFN sidecar runtime benchmark");
    println!("sidecar: {}", sidecar.display());
    println!("layer: {layer}");
    println!("shape: tokens={tokens} hidden={hidden} intermediate={intermediate}");
    println!(
        "backend: Rust C-ABI {}MPSMatrix + MSL SwiGLU + persistent sidecar",
        if include_norm { "MSL RMSNorm + " } else { "" }
    );
    println!("include_norm: {}", if include_norm { 1 } else { 0 });
    println!("iterations: {iterations}");
    println!("warmup: {warmup}");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!(
        "effective_tflops: {:.3}",
        flops / median_s.max(1e-12) / 1.0e12
    );
    println!("checksum16: {checksum}");
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn dispatch_runtime_once(
    dev: &Device,
    plan: &MpsFfnPlan,
    norm_pso: Option<&objc2::runtime::ProtocolObject<dyn objc2_metal::MTLComputePipelineState>>,
    swiglu_pso: &objc2::runtime::ProtocolObject<dyn objc2_metal::MTLComputePipelineState>,
    x: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    norm: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    normed: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    gate_up_weight: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    gate_up_out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    act: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    down_weight: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    tokens: usize,
    intermediate: usize,
    gate_up_stride: usize,
    act_stride: usize,
    include_norm: bool,
) -> Result<(), String> {
    let cmd = dev.command_buffer()?;
    let gate_up_input = if include_norm {
        let enc = cmd.compute()?;
        enc.set_pipeline(norm_pso.expect("norm pso"));
        enc.set_buffer(0, x, 0);
        enc.set_buffer(1, norm, 0);
        enc.set_buffer(2, normed, 0);
        enc.set_bytes(3, &as_u32(tokens, "tokens")?);
        enc.dispatch_threadgroups((tokens, 1, 1), (256, 1, 1));
        enc.end();
        normed
    } else {
        x
    };
    plan.encode_gate_up(&cmd, gate_up_input, gate_up_weight, gate_up_out)?;
    let enc = cmd.compute()?;
    enc.set_pipeline(swiglu_pso);
    enc.set_buffer(0, gate_up_out, 0);
    enc.set_buffer(1, act, 0);
    enc.set_bytes(2, &as_u32(intermediate, "intermediate")?);
    enc.set_bytes(3, &as_u32(gate_up_stride, "gate_up_stride")?);
    enc.set_bytes(4, &as_u32(act_stride, "act_stride")?);
    enc.dispatch_threadgroups((intermediate.div_ceil(256), tokens, 1), (256, 1, 1));
    enc.end();
    plan.encode_down(&cmd, act, down_weight, out)?;
    cmd.commit_and_wait()
}

#[cfg(target_os = "macos")]
fn json_usize(value: &serde_json::Value, key: &str) -> Result<usize, String> {
    let raw = value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("missing {key}"))?;
    usize::try_from(raw).map_err(|_| format!("{key} exceeds usize"))
}

#[cfg(target_os = "macos")]
fn aligned_row_bytes(columns: usize, element_bytes: usize) -> usize {
    let raw = columns * element_bytes;
    raw.div_ceil(128) * 128
}

#[cfg(target_os = "macos")]
fn fill_half_words(values: &mut [u16], seed: u32) {
    let mut x = seed;
    for value in values {
        x = 1_664_525u32.wrapping_mul(x).wrapping_add(1_013_904_223);
        let mantissa = ((x >> 15) & 0x01ff) as u16;
        *value = 0x3800 | mantissa;
    }
}

#[cfg(target_os = "macos")]
fn percentile_sorted(samples: &[f64], percentile: f64) -> f64 {
    let idx = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

#[cfg(target_os = "macos")]
fn parse_arg(args: &[std::ffi::OsString], idx: usize, name: &str) -> Result<Option<usize>, String> {
    args.get(idx)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid {name} argument `{arg}`: {err}"))
        })
        .transpose()
}

#[cfg(target_os = "macos")]
fn as_u32(value: usize, label: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("{label} exceeds u32"))
}
