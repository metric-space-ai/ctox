#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_prefill_attention_out is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPack, MetalPackEntry, PackLayout, QWEN35_08B};
#[cfg(target_os = "macos")]
use half::f16;

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_prefill_out2048_matmul_with_weights, PrefillOut2048MatmulBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::open_metalpack;
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("usage: bench_metalpack_prefill_attention_out <metalpack-dir> [layer] [tokens] [iterations]".to_owned());
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
        .unwrap_or(3);
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
    let out_proj = find_tensor(&pack, &prefix, "self_attn.o_proj.weight")
        .or_else(|_| find_tensor(&pack, &prefix, "attention.o_proj.weight"))
        .or_else(|_| find_tensor(&pack, &prefix, "o_proj.weight"))?;
    validate_out(out_proj)?;

    let out_weights = read_u16_entry(&pack, out_proj)?;
    let cols = QWEN35_08B.attention_q_width();
    let mut x_host = Vec::with_capacity(tokens * cols);
    for token in 0..tokens {
        for channel in 0..cols {
            let value = ((token * 19 + channel * 7) % 257) as f32 / 257.0 - 0.5;
            x_host.push(f16::from_f32(value).to_bits());
        }
    }

    let cfg = PrefillOut2048MatmulBenchConfig {
        tokens,
        row_tile: out_proj.row_tile,
        col_tile: out_proj.col_tile,
        warmup: 3,
        iterations,
    };
    let result = run_prefill_out2048_matmul_with_weights(cfg, &x_host, &out_weights)?;

    println!("qwen35-08b metalpack prefill attention out-proj benchmark");
    println!("metalpack: {}", root.display());
    println!("layer: {}", layer);
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
        "effective_gb_s_attention_out_estimate: {:.2}",
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
fn validate_out(entry: &MetalPackEntry) -> Result<(), String> {
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{} must be Fp16RowTiled, got {:?}",
            entry.tensor, entry.layout
        ));
    }
    if entry.source_shape != [QWEN35_08B.hidden_size, QWEN35_08B.attention_q_width()] {
        return Err(format!(
            "{} has shape {:?}, expected [{}, {}]",
            entry.tensor,
            entry.source_shape,
            QWEN35_08B.hidden_size,
            QWEN35_08B.attention_q_width()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_u16_entry(pack: &MetalPack, entry: &MetalPackEntry) -> Result<Vec<u16>, String> {
    let data = pack
        .read_entry_bytes(entry)
        .map_err(|err| format!("failed to read {}: {err}", entry.tensor))?;
    if data.len() % 2 != 0 {
        return Err(format!("{} byte length is not even", entry.tensor));
    }
    let mut out = Vec::with_capacity(data.len() / 2);
    for chunk in data.chunks_exact(2) {
        out.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Ok(out)
}
