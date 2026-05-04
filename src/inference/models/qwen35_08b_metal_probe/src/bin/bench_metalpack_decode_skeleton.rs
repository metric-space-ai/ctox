#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_metalpack_decode_skeleton is only available on macOS + Metal.");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
use ctox_qwen35_08b_metal_probe::{MetalPackEntry, PackLayout, QWEN35_08B};

#[cfg(target_os = "macos")]
fn main() -> Result<(), String> {
    use ctox_qwen35_08b_metal_probe::metal::bench::{
        run_decode_skeleton_tiled_with_weights, DecodeSkeletonBenchConfig,
    };
    use ctox_qwen35_08b_metal_probe::{open_metalpack, TensorClass};
    use std::{env, path::PathBuf};

    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: bench_metalpack_decode_skeleton <metalpack-dir> [input-token] [iterations]"
                .to_owned(),
        );
    }

    let root = PathBuf::from(&args[1]);
    let input_token = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<u32>()
                .map_err(|err| format!("invalid input token argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(107);
    let iterations = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid iterations argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(3);

    let pack = open_metalpack(&root).map_err(|err| err.to_string())?;
    let embedding = pack
        .find_first_class(TensorClass::TokenEmbedding)
        .ok_or_else(|| "metalpack has no token_embedding entry".to_owned())?;
    let lm_head = pack
        .find_first_class(TensorClass::LmHead)
        .unwrap_or(embedding);
    validate_entry("embedding", embedding)?;
    validate_entry("lm_head", lm_head)?;
    if embedding.source_shape != lm_head.source_shape {
        return Err(format!(
            "embedding and lm_head shape mismatch: {:?} vs {:?}",
            embedding.source_shape, lm_head.source_shape
        ));
    }
    if embedding.row_tile != lm_head.row_tile || embedding.col_tile != lm_head.col_tile {
        return Err("embedding and lm_head tile mismatch".to_owned());
    }

    let embedding_weights = read_u16_entry(&pack, embedding)?;
    let lm_weights = if embedding.tensor == lm_head.tensor {
        embedding_weights.clone()
    } else {
        read_u16_entry(&pack, lm_head)?
    };
    let cfg = DecodeSkeletonBenchConfig {
        vocab_rows: embedding.source_shape[0],
        input_token,
        warmup: 1,
        iterations,
        ..DecodeSkeletonBenchConfig::default()
    };
    let result = run_decode_skeleton_tiled_with_weights(
        cfg,
        &embedding_weights,
        &lm_weights,
        embedding.row_tile,
        embedding.col_tile,
    )?;

    println!("qwen35-08b metalpack decode skeleton benchmark");
    println!("metalpack: {}", root.display());
    println!("embedding: {}", embedding.tensor);
    println!("lm_head: {}", lm_head.tensor);
    println!("input_token: {}", result.input_token);
    println!("shape: [{} x {}]", result.vocab_rows, result.cols);
    println!(
        "tile: rows={} cols={}",
        embedding.row_tile, embedding.col_tile
    );
    println!("iterations: {}", result.iterations);
    println!("median_s: {:.9}", result.median_s);
    println!("p95_s: {:.9}", result.p95_s);
    println!(
        "effective_gb_s_packed_lm_head_plus_pairs: {:.2}",
        result.effective_gb_s
    );
    println!("next_token: {}", result.next_token);
    println!("score: {:.6}", result.score);
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_entry(label: &str, entry: &MetalPackEntry) -> Result<(), String> {
    if entry.dtype != "F16" {
        return Err(format!("{label}: expected F16 tensor, got {}", entry.dtype));
    }
    if entry.layout != PackLayout::Fp16RowTiled {
        return Err(format!(
            "{label}: expected fp16_row_tiled layout, got {:?}",
            entry.layout
        ));
    }
    if entry.source_shape.len() != 2 || entry.source_shape[1] != QWEN35_08B.hidden_size {
        return Err(format!(
            "{label}: expected [rows, {}] source shape, got {:?}",
            QWEN35_08B.hidden_size, entry.source_shape
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
