#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_prepare is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::{bf16 as half_bf16, f16 as half_f16};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_prepare_with_weights, PrefillDeltaPrepareBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_prepare <metalpack-dir> [layer] [tokens] [iterations]"
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
    let a_log = find_tensor(&pack, &prefix, "linear_attn.A_log")?;
    let dt_bias = find_tensor(&pack, &prefix, "linear_attn.dt_bias")?;
    validate_vector("A_log", a_log, QWEN35_08B.deltanet_v_heads)?;
    validate_vector("dt_bias", dt_bias, QWEN35_08B.deltanet_v_heads)?;

    let a_log_host = read_float_entry(&pack, a_log)?;
    let dt_bias_host = read_float_entry(&pack, dt_bias)?;
    let mut qkv_host = Vec::with_capacity(tokens * QWEN35_08B.deltanet_qkv_width());
    for token in 0..tokens {
        for channel in 0..QWEN35_08B.deltanet_qkv_width() {
            let v = (((token * 23 + channel * 11) % 251) as f32 - 125.0) / 251.0;
            qkv_host.push(v);
        }
    }
    let mut beta_raw = Vec::with_capacity(tokens * QWEN35_08B.deltanet_v_heads);
    let mut alpha_raw = Vec::with_capacity(tokens * QWEN35_08B.deltanet_v_heads);
    for token in 0..tokens {
        for head in 0..QWEN35_08B.deltanet_v_heads {
            beta_raw.push(((token * 7 + head * 13) % 97) as f32 / 97.0 - 0.5);
            alpha_raw.push(((token * 5 + head * 17) % 89) as f32 / 89.0 - 0.5);
        }
    }
    let cfg = PrefillDeltaPrepareBenchConfig {
        tokens,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_deltanet_prepare_with_weights(
        cfg,
        &qkv_host,
        &beta_raw,
        &alpha_raw,
        &a_log_host,
        &dt_bias_host,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet prepare benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("a_log: {}", a_log.tensor);
    println!("dt_bias: {}", dt_bias.tensor);
    println!(
        "shape: tokens={} heads={} head_dim={} qkv_width={}",
        result.tokens, result.heads, result.head_dim, result.qkv_width
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_prepare_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_tensor<'a>(
    pack: &'a MetalPack,
    prefix: &str,
    name: &str,
) -> Result<&'a MetalPackEntry, String> {
    pack.entries
        .iter()
        .find(|entry| entry.tensor.starts_with(prefix) && entry.tensor.contains(name))
        .ok_or_else(|| format!("missing tensor containing `{prefix}` and `{name}`"))
}

#[cfg(target_os = "macos")]
fn validate_vector(label: &str, entry: &MetalPackEntry, len: usize) -> Result<(), String> {
    if entry.source_shape != [len] {
        return Err(format!(
            "{label}: expected shape [{len}], got {:?}",
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_float_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<f32>, String> {
    let bytes = pack
        .read_entry_bytes(entry)
        .map_err(|err| err.to_string())?;
    match entry.dtype.as_str() {
        "F32" => {
            if bytes.len() % 4 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by four",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect())
        }
        "F16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| half_f16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32())
                .collect())
        }
        "BF16" => {
            if bytes.len() % 2 != 0 {
                return Err(format!(
                    "{} byte length is not divisible by two",
                    entry.tensor
                ));
            }
            Ok(bytes
                .chunks_exact(2)
                .map(|chunk| {
                    half_bf16::from_bits(u16::from_le_bytes([chunk[0], chunk[1]])).to_f32()
                })
                .collect())
        }
        other => Err(format!(
            "{} unsupported float state dtype {other}",
            entry.tensor
        )),
    }
}
