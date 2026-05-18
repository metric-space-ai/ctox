#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_delta_out is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, PackLayout, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::{bf16 as half_bf16, f16 as half_f16};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_deltanet_out_block_with_weights, PrefillDeltaOutBlockBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_prefill_delta_out <metalpack-dir> [layer] [tokens] [iterations]"
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
    let out_proj = find_tensor(&pack, &prefix, "linear_attn.out_proj.weight")?;
    let norm = find_tensor(&pack, &prefix, "linear_attn.norm.weight")?;
    validate_out(out_proj)?;
    validate_norm(norm)?;

    let out_weights = read_u16_entry(&pack, out_proj)?;
    let norm_weight = read_float_entry(&pack, norm)?;
    let width = QWEN35_08B.deltanet_width();
    let mut delta_host = Vec::with_capacity(tokens * width);
    let mut z_host = Vec::with_capacity(tokens * width);
    for token in 0..tokens {
        for channel in 0..width {
            delta_host.push(((token * 23 + channel * 11) % 251) as f32 / 251.0 - 0.5);
            z_host.push(((token * 17 + channel * 13) % 239) as f32 / 64.0 - 1.75);
        }
    }

    let cfg = PrefillDeltaOutBlockBenchConfig {
        tokens,
        row_tile: out_proj.row_tile,
        col_tile: out_proj.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_deltanet_out_block_with_weights(
        cfg,
        &delta_host,
        &z_host,
        &norm_weight,
        &out_weights,
    )?;

    println!("qwen35-08b metalpack prefill DeltaNet gated-norm + out-proj benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
    println!("norm: {}", norm.tensor);
    println!("out_proj: {}", out_proj.tensor);
    println!(
        "shape: tokens={} rows={} cols={}",
        result.tokens, result.rows, result.cols
    );
    println!(
        "tile: tokens={} rows={} cols={} packed_bytes={}",
        result.token_tile, result.row_tile, result.col_tile, result.packed_weight_bytes
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_norm_out_estimate: {:.2}",
        result.effective_gb_s
    );
    println!("checksum16: {:.6}", result.checksum);
    println!("checksum_sparse: {:.6}", result.checksum_sparse);
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
fn validate_out(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "out_proj: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape != [QWEN35_08B.hidden_size, QWEN35_08B.deltanet_width()] {
        return Err(format!(
            "out_proj: expected [{}, {}], got {:?}",
            QWEN35_08B.hidden_size,
            QWEN35_08B.deltanet_width(),
            entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_norm(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.source_shape != [QWEN35_08B.deltanet_head_dim] {
        return Err(format!(
            "norm: expected [{}], got {:?}",
            QWEN35_08B.deltanet_head_dim, entry.source_shape
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
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
        other => Err(format!("{} unsupported dtype {other}", entry.tensor)),
    }
}
