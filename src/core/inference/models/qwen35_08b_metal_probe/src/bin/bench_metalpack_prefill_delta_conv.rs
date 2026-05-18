#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_conv is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_conv_with_weights, PrefillDeltaConvBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use half::f16;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_conv <metalpack-dir> [layer] [tokens] [iterations]"
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

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let prefix = format!("model.language_model.layers.{layer}.");
    let weight = find_tensor(&pack, &prefix, "linear_attn.conv1d.weight")?;
    let bias = find_optional_tensor(&pack, &prefix, "linear_attn.conv1d.bias");
    validate_weight(weight)?;
    if let Some(bias) = bias {
        validate_bias(bias)?;
    }

    let conv_weight = read_u16_entry(&pack, weight)?;
    let conv_bias = match bias {
        Some(entry) => read_u16_entry(&pack, entry)?,
        None => vec![f16::from_f32(0.0).to_bits(); QWEN35_08B.deltanet_qkv_width()],
    };
    let mut x_host = Vec::with_capacity(tokens * QWEN35_08B.deltanet_qkv_width());
    for token in 0..tokens {
        for channel in 0..QWEN35_08B.deltanet_qkv_width() {
            let v = (((token * 23 + channel * 11) % 251) as f32 - 125.0) / 251.0;
            x_host.push(v);
        }
    }
    let cfg = PrefillDeltaConvBenchConfig {
        tokens,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_deltanet_conv_with_weights(cfg, &x_host, &conv_weight, &conv_bias)?;

    println!("qwen35-08b metalpack prefill DeltaNet conv benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("weight: {}", weight.tensor);
    println!(
        "bias: {}",
        bias.map(|entry| entry.tensor.as_str()).unwrap_or("<zero>")
    );
    println!(
        "shape: tokens={} channels={} kernel_width={}",
        result.tokens, result.channels, result.kernel_width
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_sequence_conv_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_tensor<'a>(
    pack: &'a ctox_qwen35_08b_metal_probe::MetalPack,
    prefix: &str,
    name: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
        .ok_or_else(|| format!("missing tensor containing `{prefix}` and `{name}`"))
}

#[cfg(target_os = "macos")]
fn find_optional_tensor<'a>(
    pack: &'a ctox_qwen35_08b_metal_probe::MetalPack,
    prefix: &str,
    name: &str,
) -> Option<&'a MetalPackEntry> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
}

#[cfg(target_os = "macos")]
fn validate_weight(entry: &MetalPackEntry) -> Result<(), String> {
    let shape_a = [QWEN35_08B.deltanet_qkv_width(), 4];
    let shape_b = [QWEN35_08B.deltanet_qkv_width(), 1, 4];
    if entry.source_shape != shape_a && entry.source_shape != shape_b {
        return Err(format!(
            "conv weight: expected {:?} or {:?}, got {:?}",
            shape_a, shape_b, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_bias(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_qkv_width()] {
        return Err(format!(
            "conv bias: expected [{}], got {:?}",
            QWEN35_08B.deltanet_qkv_width(),
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(
    pack: &ctox_qwen35_08b_metal_probe::MetalPack,
    entry: &MetalPackEntry,
) -> Result<Vec<u16>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err(format!(
            "{} byte length is not divisible by two",
            entry.tensor
        ));
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect())
}
