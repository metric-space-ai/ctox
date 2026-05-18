#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_mps_attention_out_sidecar_runtime is only available on macOS + Metal/MPS.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{
    metal::{ffi::Device, mps_sidecar::MpsDeltaProjectPlan},
    QWEN35_08B,
};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use serde_json::Value;
    use std::{env, fs, path::PathBuf, time::Instant};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_mps_attention_out_sidecar_runtime <mps-attention-out-sidecar-dir> [layer] [tokens] [iterations] [warmup]"
                .to_owned(),
        );
    }
    let sidecar = PathBuf::from(&args[1]);
    let layer = parse_arg(&args, 2, "layer")?.unwrap_or(3);
    let tokens = parse_arg(&args, 3, "tokens")?.unwrap_or(4096);
    let iterations = parse_arg(&args, 4, "iterations")?.unwrap_or(5);
    let warmup = parse_arg(&args, 5, "warmup")?.unwrap_or(2);
    let hidden = QWEN35_08B.hidden_size;
    let attention_width = QWEN35_08B.attention_q_width();
    if tokens == 0 || iterations == 0 {
        return Err("tokens and iterations must be > 0".to_owned());
    }

    let manifest_path = sidecar.join("manifest.json");
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|err| format!("{}: {err}", manifest_path.display()))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|err| err.to_string())?;
    if manifest.get("format").and_then(Value::as_str)
        != Some("ctox.qwen35_08b.mps_attention_out_sidecar")
    {
        return Err(format!(
            "invalid MPS attention out sidecar: {}",
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
    let out_entry = entry
        .get("out")
        .ok_or_else(|| "missing out entry".to_owned())?;
    let out_offset = json_usize(out_entry, "offset")?;
    let out_bytes = json_usize(out_entry, "bytes")?;
    let weights_path = sidecar.join(weights_file);
    let weights =
        fs::read(&weights_path).map_err(|err| format!("{}: {err}", weights_path.display()))?;
    if out_offset + out_bytes > weights.len() {
        return Err("sidecar weight offsets exceed weights.bin length".to_owned());
    }

    let element_bytes = std::mem::size_of::<u16>();
    let x_row_bytes = attention_width * element_bytes;
    let weight_row_bytes = hidden * element_bytes;
    let out_row_bytes = hidden * std::mem::size_of::<f32>();
    if out_bytes != attention_width * weight_row_bytes {
        return Err(format!(
            "out bytes mismatch: expected {}, got {out_bytes}",
            attention_width * weight_row_bytes
        ));
    }

    let dev = Device::default_system()?;
    let plan = MpsDeltaProjectPlan::new(
        &dev,
        tokens,
        attention_width,
        hidden,
        x_row_bytes,
        weight_row_bytes,
        out_row_bytes,
    )?;
    let x = dev.new_buffer(tokens * x_row_bytes)?;
    let weight = dev.new_buffer(out_bytes)?;
    let out = dev.new_buffer(tokens * out_row_bytes)?;

    let mut x_host = vec![0u16; tokens * attention_width];
    fill_half_words(&mut x_host, 0x4567_1234);
    unsafe {
        x.write(0, &x_host);
        weight.write(0, &weights[out_offset..out_offset + out_bytes]);
    }

    for _ in 0..warmup {
        dispatch_once(&dev, &plan, &x, &weight, &out)?;
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        dispatch_once(&dev, &plan, &x, &weight, &out)?;
        samples.push(start.elapsed().as_secs_f64());
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let median_s = percentile_sorted(&samples, 0.50);
    let p95_s = percentile_sorted(&samples, 0.95);

    let mut first = vec![0.0f32; hidden.min(16)];
    unsafe {
        out.read(0, &mut first);
    }
    let checksum = first.iter().sum::<f32>();
    let flops = 2.0 * tokens as f64 * attention_width as f64 * hidden as f64;
    let weight_bytes = attention_width * hidden * element_bytes;
    let stream_bytes = tokens * x_row_bytes + weight_bytes + tokens * out_row_bytes;

    println!("qwen35-08b Rust MPS attention out sidecar runtime benchmark");
    println!("sidecar: {}", sidecar.display());
    println!("layer: {layer}");
    println!("shape: tokens={tokens} attention_width={attention_width} hidden={hidden}");
    println!("backend: Rust C-ABI MPSMatrix attention O-proj sidecar");
    println!("iterations: {iterations}");
    println!("warmup: {warmup}");
    println!("median_s: {median_s:.9}");
    println!("p95_s: {p95_s:.9}");
    println!(
        "effective_tflops: {:.3}",
        flops / median_s.max(1e-12) / 1.0e12
    );
    println!(
        "effective_gb_s_stream_estimate: {:.2}",
        stream_bytes as f64 / median_s.max(1e-12) / 1.0e9
    );
    println!("checksum16: {checksum:.6}");
    Ok(())
}

#[cfg(target_os = "macos")]
fn dispatch_once(
    dev: &Device,
    plan: &MpsDeltaProjectPlan,
    x: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    weight: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
    out: &ctox_qwen35_08b_metal_probe::metal::ffi::Buffer,
) -> Result<(), String> {
    let cmd = dev.command_buffer()?;
    plan.encode(&cmd, x, weight, out)?;
    cmd.commit_and_wait()
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
fn json_usize(value: &serde_json::Value, key: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("missing/invalid {key}"))
}

#[cfg(target_os = "macos")]
fn fill_half_words(values: &mut [u16], mut state: u32) {
    for value in values {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let signed = ((state >> 16) & 0xff) as i32 - 128;
        *value = half::f16::from_f32(signed as f32 / 128.0).to_bits();
    }
}

#[cfg(target_os = "macos")]
fn percentile_sorted(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * p).round() as usize;
    values[idx.min(values.len() - 1)]
}
