use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use ctox_qwen35_08b_metal_probe::{LayerKind, PackLayout, TensorClass, QWEN35_08B};
use half::f16;
use serde_json::{json, Value};

fn main() -> Result<(), String> {
    let args = env::args_os().collect::<Vec<_>>();
    if args.len() < 2 {
        return Err(
            "usage: make_synthetic_metalpack <output-dir> [vocab-rows=8192] [q-gate=1]".to_owned(),
        );
    }
    let output = PathBuf::from(&args[1]);
    let vocab_rows = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .map(|arg| {
            arg.parse::<usize>()
                .map_err(|err| format!("invalid vocab rows argument `{arg}`: {err}"))
        })
        .transpose()?
        .unwrap_or(8192);
    let include_q_gate = args
        .get(3)
        .and_then(|arg| arg.to_str())
        .map(|arg| arg != "0")
        .unwrap_or(true);

    fs::create_dir_all(&output).map_err(|err| format!("create {}: {err}", output.display()))?;
    let weights_path = output.join("weights.bin");
    let manifest_path = output.join("manifest.json");
    let mut weights = BufWriter::new(File::create(&weights_path).map_err(|err| err.to_string())?);

    let row_tile = 8usize;
    let col_tile = 256usize;
    let hidden = QWEN35_08B.hidden_size;
    let delta_width = QWEN35_08B.deltanet_width();
    let attn_q_rows = if include_q_gate {
        QWEN35_08B.attention_q_with_head_gate_width()
    } else {
        QWEN35_08B.attention_q_width()
    };

    let mut templates = BTreeMap::new();
    let mut offset = 0u64;
    for (key, rows, cols, seed) in [
        ("embedding", vocab_rows, hidden, 1u32),
        ("delta_qkv", QWEN35_08B.deltanet_qkv_width(), hidden, 11),
        ("delta_z", delta_width, hidden, 12),
        ("delta_b", QWEN35_08B.deltanet_v_heads, hidden, 13),
        ("delta_a", QWEN35_08B.deltanet_v_heads, hidden, 14),
        ("delta_out", hidden, delta_width, 15),
        ("attn_q", attn_q_rows, hidden, 21),
        ("attn_k", QWEN35_08B.attention_kv_width(), hidden, 22),
        ("attn_v", QWEN35_08B.attention_kv_width(), hidden, 23),
        ("attn_o", hidden, QWEN35_08B.attention_q_width(), 24),
        ("ffn_gate", QWEN35_08B.ffn_intermediate, hidden, 31),
        ("ffn_up", QWEN35_08B.ffn_intermediate, hidden, 32),
        ("ffn_down", hidden, QWEN35_08B.ffn_intermediate, 33),
    ] {
        let bytes = write_tiled_matrix(&mut weights, rows, cols, row_tile, col_tile, seed)
            .map_err(|err| err.to_string())?;
        templates.insert(key, (rows, cols, offset, bytes));
        offset += bytes;
    }
    let mut raw_templates = BTreeMap::new();
    for (key, shape, dtype, seed) in [
        (
            "delta_a_log",
            vec![QWEN35_08B.deltanet_v_heads],
            "F32",
            41u32,
        ),
        (
            "delta_dt_bias",
            vec![QWEN35_08B.deltanet_v_heads],
            "F32",
            42,
        ),
        (
            "delta_conv1d_weight",
            vec![QWEN35_08B.deltanet_qkv_width(), 4],
            "F16",
            43,
        ),
        (
            "delta_conv1d_bias",
            vec![QWEN35_08B.deltanet_qkv_width()],
            "F16",
            44,
        ),
        (
            "delta_norm_weight",
            vec![QWEN35_08B.deltanet_head_dim],
            "F32",
            45,
        ),
        ("layer_norm", vec![hidden], "F16", 46),
    ] {
        let bytes = if key == "delta_norm_weight" {
            write_f32_constant(&mut weights, &shape, 1.0)
        } else {
            write_raw_state(&mut weights, &shape, dtype, seed)
        }
        .map_err(|err| format!("write {key}: {err}"))?;
        raw_templates.insert(key, (shape, dtype, offset, bytes));
        offset += bytes;
    }
    weights.flush().map_err(|err| err.to_string())?;

    let mut entries = Vec::new();
    let (rows, cols, entry_offset, bytes) = templates["embedding"];
    entries.push(entry(
        "model.embed_tokens.weight",
        TensorClass::TokenEmbedding,
        None,
        rows,
        cols,
        row_tile,
        col_tile,
        entry_offset,
        bytes,
    ));
    let (shape, dtype, entry_offset, bytes) = &raw_templates["layer_norm"];
    entries.push(vector_entry(
        "model.norm.weight",
        TensorClass::FinalNorm,
        None,
        dtype,
        shape,
        *entry_offset,
        *bytes,
    ));

    for layer in 0..QWEN35_08B.n_layers {
        let (shape, dtype, entry_offset, bytes) = &raw_templates["layer_norm"];
        for suffix in ["input_layernorm.weight", "post_attention_layernorm.weight"] {
            entries.push(vector_entry(
                &format!("model.layers.{layer}.{suffix}"),
                TensorClass::LayerNorm,
                Some(layer),
                dtype,
                shape,
                *entry_offset,
                *bytes,
            ));
        }
        match QWEN35_08B.layer_kind(layer) {
            LayerKind::GatedDeltaNet => {
                push_template(
                    &mut entries,
                    &templates,
                    layer,
                    "in_proj_qkv",
                    TensorClass::DeltaQkv,
                    "delta_qkv",
                );
                push_template(
                    &mut entries,
                    &templates,
                    layer,
                    "in_proj_z",
                    TensorClass::DeltaZ,
                    "delta_z",
                );
                push_template(
                    &mut entries,
                    &templates,
                    layer,
                    "in_proj_b",
                    TensorClass::DeltaB,
                    "delta_b",
                );
                push_template(
                    &mut entries,
                    &templates,
                    layer,
                    "in_proj_a",
                    TensorClass::DeltaA,
                    "delta_a",
                );
                push_template(
                    &mut entries,
                    &templates,
                    layer,
                    "out_proj",
                    TensorClass::DeltaOut,
                    "delta_out",
                );
                for (suffix, key) in [
                    ("A_log", "delta_a_log"),
                    ("dt_bias", "delta_dt_bias"),
                    ("conv1d.weight", "delta_conv1d_weight"),
                    ("conv1d.bias", "delta_conv1d_bias"),
                    ("norm.weight", "delta_norm_weight"),
                ] {
                    let (shape, dtype, entry_offset, bytes) = &raw_templates[key];
                    entries.push(raw_entry(
                        &format!("model.layers.{layer}.mixer.{suffix}"),
                        TensorClass::DeltaStateParam,
                        Some(layer),
                        dtype,
                        shape,
                        *entry_offset,
                        *bytes,
                    ));
                }
            }
            LayerKind::FullAttention => {
                push_attention_template(
                    &mut entries,
                    &templates,
                    layer,
                    "q_proj",
                    TensorClass::AttentionQ,
                    "attn_q",
                );
                push_attention_template(
                    &mut entries,
                    &templates,
                    layer,
                    "k_proj",
                    TensorClass::AttentionK,
                    "attn_k",
                );
                push_attention_template(
                    &mut entries,
                    &templates,
                    layer,
                    "v_proj",
                    TensorClass::AttentionV,
                    "attn_v",
                );
                push_attention_template(
                    &mut entries,
                    &templates,
                    layer,
                    "o_proj",
                    TensorClass::AttentionO,
                    "attn_o",
                );
            }
        }

        for (suffix, class, key) in [
            ("gate_proj", TensorClass::MlpGate, "ffn_gate"),
            ("up_proj", TensorClass::MlpUp, "ffn_up"),
            ("down_proj", TensorClass::MlpDown, "ffn_down"),
        ] {
            let (rows, cols, entry_offset, bytes) = templates[key];
            entries.push(entry(
                &format!("model.layers.{layer}.mlp.{suffix}.weight"),
                class,
                Some(layer),
                rows,
                cols,
                row_tile,
                col_tile,
                entry_offset,
                bytes,
            ));
        }
    }

    let manifest = json!({
        "format": "ctox.qwen35_08b.metalpack",
        "version": 1,
        "model": QWEN35_08B.model,
        "source_root": "synthetic",
        "shape": {
            "hidden_size": QWEN35_08B.hidden_size,
            "vocab_size": vocab_rows,
            "layers": QWEN35_08B.n_layers,
            "ffn_intermediate": QWEN35_08B.ffn_intermediate,
            "layer_pattern": (0..QWEN35_08B.n_layers)
                .map(|layer| match QWEN35_08B.layer_kind(layer) {
                    LayerKind::GatedDeltaNet => "D",
                    LayerKind::FullAttention => "A",
                })
                .collect::<Vec<_>>(),
        },
        "weights_file": "weights.bin",
        "packed_bytes": offset,
        "entries": entries,
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;

    println!("synthetic qwen35-08b metalpack");
    println!("output: {}", output.display());
    println!("entries: {}", manifest["entries"].as_array().unwrap().len());
    println!("packed_bytes: {}", offset);
    println!("vocab_rows: {}", vocab_rows);
    println!("attention_q_rows: {}", attn_q_rows);
    Ok(())
}

fn push_template(
    entries: &mut Vec<Value>,
    templates: &BTreeMap<&'static str, (usize, usize, u64, u64)>,
    layer: usize,
    suffix: &str,
    class: TensorClass,
    key: &'static str,
) {
    let (rows, cols, offset, bytes) = templates[key];
    entries.push(entry(
        &format!("model.layers.{layer}.mixer.{suffix}.weight"),
        class,
        Some(layer),
        rows,
        cols,
        8,
        256,
        offset,
        bytes,
    ));
}

fn push_attention_template(
    entries: &mut Vec<Value>,
    templates: &BTreeMap<&'static str, (usize, usize, u64, u64)>,
    layer: usize,
    suffix: &str,
    class: TensorClass,
    key: &'static str,
) {
    let (rows, cols, offset, bytes) = templates[key];
    entries.push(entry(
        &format!("model.layers.{layer}.self_attn.{suffix}.weight"),
        class,
        Some(layer),
        rows,
        cols,
        8,
        256,
        offset,
        bytes,
    ));
}

#[allow(clippy::too_many_arguments)]
fn entry(
    tensor: &str,
    class: TensorClass,
    layer: Option<usize>,
    rows: usize,
    cols: usize,
    row_tile: usize,
    col_tile: usize,
    packed_offset: u64,
    packed_bytes: u64,
) -> Value {
    json!({
        "tensor": tensor,
        "class": class.as_str(),
        "layer": layer,
        "dtype": "F16",
        "source_shape": [rows, cols],
        "layout": PackLayout::Fp16RowTiled.as_str(),
        "row_tile": row_tile,
        "col_tile": col_tile,
        "packed_shape": [round_up(rows, row_tile), round_up(cols, col_tile)],
        "packed_offset": packed_offset,
        "packed_bytes": packed_bytes,
    })
}

fn raw_entry(
    tensor: &str,
    class: TensorClass,
    layer: Option<usize>,
    dtype: &str,
    source_shape: &[usize],
    packed_offset: u64,
    packed_bytes: u64,
) -> Value {
    json!({
        "tensor": tensor,
        "class": class.as_str(),
        "layer": layer,
        "dtype": dtype,
        "source_shape": source_shape,
        "layout": PackLayout::RawState.as_str(),
        "row_tile": 0,
        "col_tile": 0,
        "packed_shape": source_shape,
        "packed_offset": packed_offset,
        "packed_bytes": packed_bytes,
    })
}

fn vector_entry(
    tensor: &str,
    class: TensorClass,
    layer: Option<usize>,
    dtype: &str,
    source_shape: &[usize],
    packed_offset: u64,
    packed_bytes: u64,
) -> Value {
    json!({
        "tensor": tensor,
        "class": class.as_str(),
        "layer": layer,
        "dtype": dtype,
        "source_shape": source_shape,
        "layout": PackLayout::Fp16Vector.as_str(),
        "row_tile": 0,
        "col_tile": 0,
        "packed_shape": source_shape,
        "packed_offset": packed_offset,
        "packed_bytes": packed_bytes,
    })
}

fn write_tiled_matrix(
    output: &mut dyn Write,
    rows: usize,
    cols: usize,
    row_tile: usize,
    col_tile: usize,
    seed: u32,
) -> std::io::Result<u64> {
    let padded_rows = round_up(rows, row_tile);
    let padded_cols = round_up(cols, col_tile);
    let mut written = 0u64;
    for row_base in (0..padded_rows).step_by(row_tile) {
        for col_base in (0..padded_cols).step_by(col_tile) {
            for row_lane in 0..row_tile {
                for col_lane in 0..col_tile {
                    let row = row_base + row_lane;
                    let col = col_base + col_lane;
                    let bits = if row < rows && col < cols {
                        synthetic_half(row, col, seed)
                    } else {
                        0
                    };
                    output.write_all(&bits.to_le_bytes())?;
                    written += 2;
                }
            }
        }
    }
    Ok(written)
}

fn synthetic_half(row: usize, col: usize, seed: u32) -> u16 {
    let hash = (row as u32)
        .wrapping_mul(1_103_515_245)
        .wrapping_add((col as u32).wrapping_mul(12_345))
        .wrapping_add(seed.wrapping_mul(97));
    let value = ((hash % 251) as f32 - 125.0) / 4096.0;
    f16::from_f32(value).to_bits()
}

fn write_raw_state(
    output: &mut dyn Write,
    shape: &[usize],
    dtype: &str,
    seed: u32,
) -> std::io::Result<u64> {
    let elements = shape.iter().product::<usize>();
    let mut written = 0u64;
    for index in 0..elements {
        let row = index / 256;
        let col = index % 256;
        if dtype == "F32" {
            let hash = (index as u32)
                .wrapping_mul(1_664_525)
                .wrapping_add(seed.wrapping_mul(1_013_904_223));
            let value = ((hash % 257) as f32 - 128.0) / 2048.0;
            output.write_all(&value.to_le_bytes())?;
            written += 4;
        } else {
            let bits = synthetic_half(row, col, seed);
            output.write_all(&bits.to_le_bytes())?;
            written += 2;
        }
    }
    Ok(written)
}

fn write_f32_constant(output: &mut dyn Write, shape: &[usize], value: f32) -> std::io::Result<u64> {
    let elements = shape.iter().product::<usize>();
    let mut written = 0u64;
    for _ in 0..elements {
        output.write_all(&value.to_le_bytes())?;
        written += 4;
    }
    Ok(written)
}

fn round_up(value: usize, multiple: usize) -> usize {
    value.div_ceil(multiple) * multiple
}
