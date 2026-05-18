//! Metadata-first `.metalpack` writer for Qwen3.5-0.8B.
//!
//! The writer is deliberately simple and deterministic:
//!
//! - tensors are written in `PackPlan` order
//! - FP16 matrices are row/column padded into fixed tiles
//! - vectors/state tensors are copied byte-for-byte
//! - `manifest.json` records every source tensor and packed offset

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use half::{bf16, f16};
use serde_json::{json, Value};

use crate::{
    artifacts::{ArtifactError, ArtifactReport, TensorHeader},
    inspect_model_artifacts, PackEntry, PackLayout, QuantScheme, TensorClass, QWEN35_08B,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetalPackReport {
    pub output_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub weights_path: PathBuf,
    pub entries: usize,
    pub packed_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetalPack {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub weights_path: PathBuf,
    pub packed_bytes: u64,
    pub entries: Vec<MetalPackEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetalPackEntry {
    pub tensor: String,
    pub class: TensorClass,
    pub layer: Option<usize>,
    pub dtype: String,
    pub source_shape: Vec<usize>,
    pub layout: PackLayout,
    pub row_tile: usize,
    pub col_tile: usize,
    pub quant_scheme: QuantScheme,
    pub quant_group_size: usize,
    pub packed_shape: Vec<usize>,
    pub packed_offset: u64,
    pub packed_bytes: u64,
}

impl MetalPack {
    pub fn find_first_class(&self, class: TensorClass) -> Option<&MetalPackEntry> {
        self.entries.iter().find(|entry| entry.class == class)
    }

    pub fn read_entry_bytes(&self, entry: &MetalPackEntry) -> Result<Vec<u8>, ArtifactError> {
        let mut file = File::open(&self.weights_path).map_err(|source| ArtifactError::Io {
            path: self.weights_path.clone(),
            source,
        })?;
        file.seek(SeekFrom::Start(entry.packed_offset))
            .map_err(|source| ArtifactError::Io {
                path: self.weights_path.clone(),
                source,
            })?;
        let mut bytes = vec![0u8; entry.packed_bytes as usize];
        file.read_exact(&mut bytes)
            .map_err(|source| ArtifactError::Io {
                path: self.weights_path.clone(),
                source,
            })?;
        Ok(bytes)
    }
}

pub fn open_metalpack(root: impl AsRef<Path>) -> Result<MetalPack, ArtifactError> {
    let root = root.as_ref().to_path_buf();
    let manifest_path = root.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|source| ArtifactError::Io {
        path: manifest_path.clone(),
        source,
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|source| ArtifactError::Json {
        path: manifest_path.clone(),
        source,
    })?;
    let format = value
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_manifest(&manifest_path, "missing format"))?;
    if format != "ctox.qwen35_08b.metalpack" {
        return Err(invalid_manifest(&manifest_path, "unexpected format"));
    }
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_manifest(&manifest_path, "missing version"))?;
    if version != 1 {
        return Err(invalid_manifest(&manifest_path, "unsupported version"));
    }
    let weights_file = value
        .get("weights_file")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_manifest(&manifest_path, "missing weights_file"))?;
    let weights_path = root.join(weights_file);
    let packed_bytes = value
        .get("packed_bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_manifest(&manifest_path, "missing packed_bytes"))?;
    let entries_value = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_manifest(&manifest_path, "missing entries"))?;
    let mut entries = Vec::with_capacity(entries_value.len());
    for entry in entries_value {
        entries.push(parse_manifest_entry(&manifest_path, entry)?);
    }

    Ok(MetalPack {
        root,
        manifest_path,
        weights_path,
        packed_bytes,
        entries,
    })
}

pub fn write_metalpack_from_model_dir(
    model_dir: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> Result<MetalPackReport, ArtifactError> {
    let report = inspect_model_artifacts(model_dir)?;
    write_metalpack_from_report(&report, output_dir)
}

fn parse_manifest_entry(path: &Path, value: &Value) -> Result<MetalPackEntry, ArtifactError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_manifest(path, "entry is not an object"))?;
    let tensor = get_str(object, path, "tensor")?.to_owned();
    let class = TensorClass::from_str(get_str(object, path, "class")?)
        .ok_or_else(|| invalid_manifest(path, "unknown tensor class"))?;
    let layout = PackLayout::from_str(get_str(object, path, "layout")?)
        .ok_or_else(|| invalid_manifest(path, "unknown pack layout"))?;
    Ok(MetalPackEntry {
        tensor,
        class,
        layer: object.get("layer").and_then(|value| {
            if value.is_null() {
                None
            } else {
                value.as_u64().and_then(|value| usize::try_from(value).ok())
            }
        }),
        dtype: get_str(object, path, "dtype")?.to_owned(),
        source_shape: get_usize_array(object, path, "source_shape")?,
        layout,
        row_tile: get_usize(object, path, "row_tile")?,
        col_tile: get_usize(object, path, "col_tile")?,
        quant_scheme: object
            .get("quant_scheme")
            .and_then(Value::as_str)
            .and_then(QuantScheme::from_str)
            .unwrap_or(QuantScheme::None),
        quant_group_size: object
            .get("quant_group_size")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0),
        packed_shape: get_usize_array(object, path, "packed_shape")?,
        packed_offset: get_u64(object, path, "packed_offset")?,
        packed_bytes: get_u64(object, path, "packed_bytes")?,
    })
}

fn get_str<'a>(
    object: &'a serde_json::Map<String, Value>,
    path: &Path,
    key: &str,
) -> Result<&'a str, ArtifactError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_manifest(path, &format!("missing string field {key}")))
}

fn get_u64(
    object: &serde_json::Map<String, Value>,
    path: &Path,
    key: &str,
) -> Result<u64, ArtifactError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_manifest(path, &format!("missing u64 field {key}")))
}

fn get_usize(
    object: &serde_json::Map<String, Value>,
    path: &Path,
    key: &str,
) -> Result<usize, ArtifactError> {
    let value = get_u64(object, path, key)?;
    usize::try_from(value).map_err(|_| invalid_manifest(path, &format!("{key} exceeds usize")))
}

fn get_usize_array(
    object: &serde_json::Map<String, Value>,
    path: &Path,
    key: &str,
) -> Result<Vec<usize>, ArtifactError> {
    object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_manifest(path, &format!("missing array field {key}")))?
        .iter()
        .map(|item| {
            item.as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid_manifest(path, &format!("invalid item in {key}")))
        })
        .collect()
}

fn invalid_manifest(path: &Path, reason: &str) -> ArtifactError {
    ArtifactError::InvalidSafetensorHeader {
        path: path.to_path_buf(),
        reason: format!("invalid metalpack manifest: {reason}"),
    }
}

pub fn write_metalpack_from_report(
    report: &ArtifactReport,
    output_dir: impl AsRef<Path>,
) -> Result<MetalPackReport, ArtifactError> {
    let output_dir = output_dir.as_ref().to_path_buf();
    fs::create_dir_all(&output_dir).map_err(|source| ArtifactError::Io {
        path: output_dir.clone(),
        source,
    })?;

    let weights_path = output_dir.join("weights.bin");
    let manifest_path = output_dir.join("manifest.json");
    let mut weights = File::create(&weights_path).map_err(|source| ArtifactError::Io {
        path: weights_path.clone(),
        source,
    })?;

    let mut manifest_entries = Vec::with_capacity(report.pack_plan.entries.len());
    let mut offset = 0u64;
    for entry in &report.pack_plan.entries {
        let tensor = report
            .safetensors
            .tensors
            .iter()
            .find(|tensor| tensor.name == entry.tensor)
            .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                path: report.root.clone(),
                reason: format!("pack entry missing tensor header: {}", entry.tensor),
            })?;
        let packed = write_entry(&mut weights, entry, tensor)?;
        manifest_entries.push(entry_manifest(entry, tensor, offset, packed));
        offset += packed;
    }
    weights.flush().map_err(|source| ArtifactError::Io {
        path: weights_path.clone(),
        source,
    })?;

    let manifest = json!({
        "format": "ctox.qwen35_08b.metalpack",
        "version": 1,
        "model": QWEN35_08B.model,
        "source_root": report.root,
        "shape": {
            "hidden_size": QWEN35_08B.hidden_size,
            "vocab_size": QWEN35_08B.vocab_size,
            "layers": QWEN35_08B.n_layers,
            "ffn_intermediate": QWEN35_08B.ffn_intermediate,
            "layer_pattern": (0..QWEN35_08B.n_layers)
                .map(|layer| match QWEN35_08B.layer_kind(layer) {
                    crate::LayerKind::GatedDeltaNet => "D",
                    crate::LayerKind::FullAttention => "A",
                })
                .collect::<Vec<_>>(),
        },
        "weights_file": "weights.bin",
        "packed_bytes": offset,
        "entries": manifest_entries,
    });
    write_pretty_json(&manifest_path, &manifest)?;

    Ok(MetalPackReport {
        output_dir,
        manifest_path,
        weights_path,
        entries: report.pack_plan.entries.len(),
        packed_bytes: offset,
    })
}

fn write_entry(
    output: &mut File,
    entry: &PackEntry,
    tensor: &TensorHeader,
) -> Result<u64, ArtifactError> {
    match entry.layout {
        PackLayout::Fp16RowTiled
            if (tensor.dtype == "F16" || tensor.dtype == "BF16") && tensor.shape.len() == 2 =>
        {
            write_16bit_row_tiled_as_fp16(output, entry, tensor)
        }
        PackLayout::Int8RowTiled if tensor.shape.len() == 2 => {
            write_quantized_row_tiled(output, entry, tensor, 8)
        }
        PackLayout::Int4GroupwiseRowTiled if tensor.shape.len() == 2 => {
            write_quantized_row_tiled(output, entry, tensor, 4)
        }
        _ => copy_tensor_bytes(output, tensor),
    }
}

fn write_16bit_row_tiled_as_fp16(
    output: &mut File,
    entry: &PackEntry,
    tensor: &TensorHeader,
) -> Result<u64, ArtifactError> {
    let rows = tensor.shape[0];
    let cols = tensor.shape[1];
    let row_tile = entry.row_tile.max(1);
    let col_tile = entry.col_tile.max(1);
    let padded_rows = round_up(rows, row_tile);
    let padded_cols = round_up(cols, col_tile);
    let row_bytes = cols * 2;
    let mut input = File::open(&tensor.shard).map_err(|source| ArtifactError::Io {
        path: tensor.shard.clone(),
        source,
    })?;
    let mut row_block = vec![0u8; row_tile * row_bytes];
    let zero = [0u8; 2];
    let mut written = 0u64;

    for row_base in (0..padded_rows).step_by(row_tile) {
        row_block.fill(0);
        let rows_present = rows.saturating_sub(row_base).min(row_tile);
        if rows_present > 0 {
            let absolute =
                tensor.data_start + tensor.data_offsets[0] + (row_base * row_bytes) as u64;
            input
                .seek(SeekFrom::Start(absolute))
                .map_err(|source| ArtifactError::Io {
                    path: tensor.shard.clone(),
                    source,
                })?;
            input
                .read_exact(&mut row_block[..rows_present * row_bytes])
                .map_err(|source| ArtifactError::Io {
                    path: tensor.shard.clone(),
                    source,
                })?;
        }

        for col_base in (0..padded_cols).step_by(col_tile) {
            for row in 0..row_tile {
                for col in 0..col_tile {
                    let src_col = col_base + col;
                    if row_base + row < rows && src_col < cols {
                        let start = row * row_bytes + src_col * 2;
                        if tensor.dtype == "F16" {
                            output
                                .write_all(&row_block[start..start + 2])
                                .map_err(|source| ArtifactError::Io {
                                    path: tensor.shard.clone(),
                                    source,
                                })?;
                        } else {
                            let bits = u16::from_le_bytes([row_block[start], row_block[start + 1]]);
                            let half_bits = f16::from_f32(bf16::from_bits(bits).to_f32()).to_bits();
                            output
                                .write_all(&half_bits.to_le_bytes())
                                .map_err(|source| ArtifactError::Io {
                                    path: tensor.shard.clone(),
                                    source,
                                })?;
                        }
                    } else {
                        output
                            .write_all(&zero)
                            .map_err(|source| ArtifactError::Io {
                                path: tensor.shard.clone(),
                                source,
                            })?;
                    }
                    written += 2;
                }
            }
        }
    }

    Ok(written)
}

fn write_quantized_row_tiled(
    output: &mut File,
    entry: &PackEntry,
    tensor: &TensorHeader,
    bits: u8,
) -> Result<u64, ArtifactError> {
    if !matches!(tensor.dtype.as_str(), "F16" | "BF16" | "F32") {
        return Err(ArtifactError::InvalidConfig(format!(
            "static quantized layout `{}` for `{}` requires F16/BF16/F32 source, got {}",
            entry.layout.as_str(),
            entry.tensor,
            tensor.dtype
        )));
    }
    if bits != 4 && bits != 8 {
        return Err(ArtifactError::InvalidConfig(format!(
            "unsupported quant bit width {bits} for `{}`",
            entry.tensor
        )));
    }

    let rows = tensor.shape[0];
    let cols = tensor.shape[1];
    let row_tile = entry.row_tile.max(1);
    let col_tile = entry.col_tile.max(1);
    let group_size = validate_quant_group(entry, bits, col_tile)?;
    let groups_per_col_tile = col_tile / group_size;
    let padded_rows = round_up(rows, row_tile);
    let padded_cols = round_up(cols, col_tile);
    let dtype_bytes = dtype_byte_width(&tensor.dtype).ok_or_else(|| {
        ArtifactError::InvalidConfig(format!(
            "unsupported dtype {} for `{}`",
            tensor.dtype, entry.tensor
        ))
    })?;
    let row_bytes = cols * dtype_bytes;
    let mut input = File::open(&tensor.shard).map_err(|source| ArtifactError::Io {
        path: tensor.shard.clone(),
        source,
    })?;
    let mut row_block = vec![0u8; row_tile * row_bytes];
    let mut group = vec![0.0f32; group_size];
    let mut written = 0u64;

    for row_base in (0..padded_rows).step_by(row_tile) {
        row_block.fill(0);
        let rows_present = rows.saturating_sub(row_base).min(row_tile);
        if rows_present > 0 {
            let absolute =
                tensor.data_start + tensor.data_offsets[0] + (row_base * row_bytes) as u64;
            input
                .seek(SeekFrom::Start(absolute))
                .map_err(|source| ArtifactError::Io {
                    path: tensor.shard.clone(),
                    source,
                })?;
            input
                .read_exact(&mut row_block[..rows_present * row_bytes])
                .map_err(|source| ArtifactError::Io {
                    path: tensor.shard.clone(),
                    source,
                })?;
        }

        for col_base in (0..padded_cols).step_by(col_tile) {
            for row in 0..row_tile {
                for group_id in 0..groups_per_col_tile {
                    let group_col_base = col_base + group_id * group_size;
                    for (col, value) in group.iter_mut().enumerate() {
                        let src_col = group_col_base + col;
                        *value = if row_base + row < rows && src_col < cols {
                            let start = row * row_bytes + src_col * dtype_bytes;
                            read_source_scalar(
                                &row_block[start..start + dtype_bytes],
                                &tensor.dtype,
                            )
                        } else {
                            0.0
                        };
                    }

                    let max_abs = group.iter().map(|value| value.abs()).fold(0.0f32, f32::max);
                    let qmax = if bits == 8 { 127.0 } else { 7.0 };
                    let scale = if max_abs > 0.0 { max_abs / qmax } else { 1.0 };
                    let scale_bits = f16::from_f32(scale).to_bits();
                    output
                        .write_all(&scale_bits.to_le_bytes())
                        .map_err(|source| ArtifactError::Io {
                            path: tensor.shard.clone(),
                            source,
                        })?;
                    written += 2;

                    if bits == 8 {
                        for value in &group {
                            let q = quantize_symmetric(*value, scale, 127) as i8;
                            output.write_all(&q.to_ne_bytes()).map_err(|source| {
                                ArtifactError::Io {
                                    path: tensor.shard.clone(),
                                    source,
                                }
                            })?;
                            written += 1;
                        }
                    } else {
                        for pair in group.chunks_exact(2) {
                            let lo = (quantize_symmetric(pair[0], scale, 7) as i8) & 0x0f;
                            let hi = (quantize_symmetric(pair[1], scale, 7) as i8) & 0x0f;
                            let packed = (lo as u8) | ((hi as u8) << 4);
                            output
                                .write_all(&[packed])
                                .map_err(|source| ArtifactError::Io {
                                    path: tensor.shard.clone(),
                                    source,
                                })?;
                            written += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(written)
}

fn validate_quant_group(
    entry: &PackEntry,
    bits: u8,
    col_tile: usize,
) -> Result<usize, ArtifactError> {
    let group_size = entry.quant_group_size;
    if group_size == 0 {
        return Err(ArtifactError::InvalidConfig(format!(
            "quantized layout `{}` for `{}` requires quant_group_size > 0",
            entry.layout.as_str(),
            entry.tensor
        )));
    }
    if group_size > col_tile {
        return Err(ArtifactError::InvalidConfig(format!(
            "quant_group_size {group_size} for `{}` exceeds col_tile {col_tile}",
            entry.tensor
        )));
    }
    if col_tile % group_size != 0 {
        return Err(ArtifactError::InvalidConfig(format!(
            "quant_group_size {group_size} for `{}` must divide col_tile {col_tile}",
            entry.tensor
        )));
    }
    if bits == 4 && group_size % 2 != 0 {
        return Err(ArtifactError::InvalidConfig(format!(
            "int4 quantized layout for `{}` requires even quant_group_size, got {group_size}",
            entry.tensor
        )));
    }
    Ok(group_size)
}

fn dtype_byte_width(dtype: &str) -> Option<usize> {
    match dtype {
        "F16" | "BF16" => Some(2),
        "F32" => Some(4),
        _ => None,
    }
}

fn read_source_scalar(bytes: &[u8], dtype: &str) -> f32 {
    match dtype {
        "F16" => f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32(),
        "BF16" => bf16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32(),
        "F32" => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        _ => 0.0,
    }
}

fn quantize_symmetric(value: f32, scale: f32, qmax: i32) -> i32 {
    if scale <= 0.0 {
        return 0;
    }
    (value / scale).round().clamp(-(qmax as f32), qmax as f32) as i32
}

fn copy_tensor_bytes(output: &mut File, tensor: &TensorHeader) -> Result<u64, ArtifactError> {
    let mut input = File::open(&tensor.shard).map_err(|source| ArtifactError::Io {
        path: tensor.shard.clone(),
        source,
    })?;
    let len = tensor.data_offsets[1].saturating_sub(tensor.data_offsets[0]);
    input
        .seek(SeekFrom::Start(tensor.data_start + tensor.data_offsets[0]))
        .map_err(|source| ArtifactError::Io {
            path: tensor.shard.clone(),
            source,
        })?;
    let mut limited = input.take(len);
    std::io::copy(&mut limited, output).map_err(|source| ArtifactError::Io {
        path: tensor.shard.clone(),
        source,
    })
}

fn entry_manifest(
    entry: &PackEntry,
    tensor: &TensorHeader,
    offset: u64,
    packed_bytes: u64,
) -> Value {
    let packed_shape = if matches!(
        entry.layout,
        PackLayout::Fp16RowTiled | PackLayout::Int8RowTiled | PackLayout::Int4GroupwiseRowTiled
    ) && tensor.shape.len() == 2
    {
        vec![
            round_up(tensor.shape[0], entry.row_tile.max(1)),
            round_up(tensor.shape[1], entry.col_tile.max(1)),
        ]
    } else {
        tensor.shape.clone()
    };

    json!({
        "tensor": entry.tensor,
        "class": entry.class.as_str(),
        "layer": entry.layer,
        "dtype": entry.dtype,
        "source_shape": entry.shape,
        "source_shard": tensor.shard,
        "source_offsets": tensor.data_offsets,
        "layout": entry.layout.as_str(),
        "row_tile": entry.row_tile,
        "col_tile": entry.col_tile,
        "quant_scheme": entry.quant_scheme.as_str(),
        "quant_group_size": entry.quant_group_size,
        "quant_scale_dtype": if entry.quant_scheme == QuantScheme::None { "none" } else { "f16" },
        "quant_value_bits": match entry.quant_scheme {
            QuantScheme::None => 0,
            QuantScheme::Int8Symmetric => 8,
            QuantScheme::Int4GroupwiseSymmetric => 4,
        },
        "packed_shape": packed_shape,
        "packed_offset": offset,
        "packed_bytes": packed_bytes,
    })
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<(), ArtifactError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| ArtifactError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, bytes).map_err(|source| ArtifactError::Io {
        path: path.to_path_buf(),
        source,
    })
}

const fn round_up(value: usize, multiple: usize) -> usize {
    if multiple == 0 {
        value
    } else {
        value.div_ceil(multiple) * multiple
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::inspect_model_artifacts;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_metadata_and_packed_weights_for_small_fixture() {
        let root = temp_root("model");
        let out = temp_root("pack");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            r#"{
              "hidden_size": 1024,
              "vocab_size": 248320,
              "num_hidden_layers": 24,
              "intermediate_size": 3584,
              "num_attention_heads": 8,
              "num_key_value_heads": 2,
              "head_dim": 256
            }"#,
        )
        .unwrap();
        let mut data = Vec::new();
        for value in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            data.extend_from_slice(&bf16::from_f32(value).to_bits().to_le_bytes());
        }
        data.extend_from_slice(&7.0f32.to_le_bytes());
        data.extend_from_slice(&8.0f32.to_le_bytes());
        write_safetensors(
            &root.join("model.safetensors"),
            r#"{
              "model.embed_tokens.weight": {"dtype": "BF16", "shape": [2, 3], "data_offsets": [0, 12]},
              "model.layers.0.A_log": {"dtype": "F32", "shape": [2], "data_offsets": [12, 20]}
            }"#,
            &data,
        );

        let report = inspect_model_artifacts(&root).unwrap();
        let written = write_metalpack_from_report(&report, &out).unwrap();
        assert_eq!(written.entries, 2);
        assert!(written.packed_bytes > 20);
        assert!(written.manifest_path.exists());
        assert!(written.weights_path.exists());

        let pack = open_metalpack(&out).unwrap();
        let embedding = pack.find_first_class(TensorClass::TokenEmbedding).unwrap();
        assert_eq!(embedding.layout, PackLayout::Fp16RowTiled);
        assert_eq!(embedding.packed_shape, vec![8, 256]);
        let packed_embedding = pack.read_entry_bytes(embedding).unwrap();
        assert_eq!(packed_embedding.len(), 4096);
        for (idx, expected) in [1.0f32, 2.0, 3.0].into_iter().enumerate() {
            let start = idx * 2;
            let bits = u16::from_le_bytes([packed_embedding[start], packed_embedding[start + 1]]);
            assert_eq!(bits, f16::from_f32(expected).to_bits());
        }

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn writes_static_int8_quantized_row_tiled_payload() {
        let root = temp_root("quant-src");
        let out = temp_root("quant-out");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&out).unwrap();
        let src_path = root.join("weights.bin");
        let mut src = Vec::new();
        for value in [1.0f32, -1.0, 0.5, -0.5, 0.0, 0.25, -0.25, 0.125] {
            src.extend_from_slice(&f16::from_f32(value).to_bits().to_le_bytes());
        }
        fs::write(&src_path, src).unwrap();

        let tensor = TensorHeader {
            name: "test.weight".to_owned(),
            dtype: "F16".to_owned(),
            shape: vec![2, 4],
            data_offsets: [0, 16],
            data_start: 0,
            shard: src_path,
        };
        let entry = PackEntry {
            tensor: tensor.name.clone(),
            class: TensorClass::MlpGate,
            layer: Some(0),
            dtype: tensor.dtype.clone(),
            shape: tensor.shape.clone(),
            bytes: 16,
            layout: PackLayout::Int8RowTiled,
            row_tile: 1,
            col_tile: 4,
            quant_scheme: QuantScheme::Int8Symmetric,
            quant_group_size: 4,
        };

        let dst_path = out.join("quant.bin");
        let mut dst = File::create(&dst_path).unwrap();
        let written = write_quantized_row_tiled(&mut dst, &entry, &tensor, 8).unwrap();
        drop(dst);
        assert_eq!(written, 12);

        let bytes = fs::read(&dst_path).unwrap();
        assert_eq!(bytes.len(), 12);
        let scale0 = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        assert!((scale0 - (1.0 / 127.0)).abs() < 1e-5);
        assert_eq!(i8::from_ne_bytes([bytes[2]]), 127);
        assert_eq!(i8::from_ne_bytes([bytes[3]]), -127);
        assert_eq!(i8::from_ne_bytes([bytes[4]]), 64);
        assert_eq!(i8::from_ne_bytes([bytes[5]]), -64);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn writes_static_int8_quantized_payload_with_multiple_groups_per_tile() {
        let root = temp_root("quant-src-groups");
        let out = temp_root("quant-out-groups");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&out).unwrap();
        let src_path = root.join("weights.bin");
        let mut src = Vec::new();
        for value in [1.0f32, -1.0, 0.5, -0.5] {
            src.extend_from_slice(&f16::from_f32(value).to_bits().to_le_bytes());
        }
        fs::write(&src_path, src).unwrap();

        let tensor = TensorHeader {
            name: "test.weight".to_owned(),
            dtype: "F16".to_owned(),
            shape: vec![1, 4],
            data_offsets: [0, 8],
            data_start: 0,
            shard: src_path,
        };
        let entry = PackEntry {
            tensor: tensor.name.clone(),
            class: TensorClass::MlpGate,
            layer: Some(0),
            dtype: tensor.dtype.clone(),
            shape: tensor.shape.clone(),
            bytes: 8,
            layout: PackLayout::Int8RowTiled,
            row_tile: 1,
            col_tile: 4,
            quant_scheme: QuantScheme::Int8Symmetric,
            quant_group_size: 2,
        };

        let dst_path = out.join("quant.bin");
        let mut dst = File::create(&dst_path).unwrap();
        let written = write_quantized_row_tiled(&mut dst, &entry, &tensor, 8).unwrap();
        drop(dst);
        assert_eq!(written, 8);

        let bytes = fs::read(&dst_path).unwrap();
        assert_eq!(bytes.len(), 8);
        let scale0 = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let scale1 = f16::from_bits(u16::from_le_bytes([bytes[4], bytes[5]])).to_f32();
        assert!((scale0 - (1.0 / 127.0)).abs() < 1e-5);
        assert!((scale1 - (0.5 / 127.0)).abs() < 1e-5);
        assert_eq!(i8::from_ne_bytes([bytes[2]]), 127);
        assert_eq!(i8::from_ne_bytes([bytes[3]]), -127);
        assert_eq!(i8::from_ne_bytes([bytes[6]]), 127);
        assert_eq!(i8::from_ne_bytes([bytes[7]]), -127);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn writes_static_int4_groupwise_quantized_payload() {
        let root = temp_root("quant-src-int4");
        let out = temp_root("quant-out-int4");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&out).unwrap();
        let src_path = root.join("weights.bin");
        let mut src = Vec::new();
        for value in [1.0f32, -1.0, 0.5, -0.5] {
            src.extend_from_slice(&f16::from_f32(value).to_bits().to_le_bytes());
        }
        fs::write(&src_path, src).unwrap();

        let tensor = TensorHeader {
            name: "test.weight".to_owned(),
            dtype: "F16".to_owned(),
            shape: vec![1, 4],
            data_offsets: [0, 8],
            data_start: 0,
            shard: src_path,
        };
        let entry = PackEntry {
            tensor: tensor.name.clone(),
            class: TensorClass::MlpGate,
            layer: Some(0),
            dtype: tensor.dtype.clone(),
            shape: tensor.shape.clone(),
            bytes: 8,
            layout: PackLayout::Int4GroupwiseRowTiled,
            row_tile: 1,
            col_tile: 4,
            quant_scheme: QuantScheme::Int4GroupwiseSymmetric,
            quant_group_size: 2,
        };

        let dst_path = out.join("quant.bin");
        let mut dst = File::create(&dst_path).unwrap();
        let written = write_quantized_row_tiled(&mut dst, &entry, &tensor, 4).unwrap();
        drop(dst);
        assert_eq!(written, 6);

        let bytes = fs::read(&dst_path).unwrap();
        assert_eq!(bytes.len(), 6);
        let scale0 = f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32();
        let scale1 = f16::from_bits(u16::from_le_bytes([bytes[3], bytes[4]])).to_f32();
        assert!((scale0 - (1.0 / 7.0)).abs() < 1e-4);
        assert!((scale1 - (0.5 / 7.0)).abs() < 1e-4);
        assert_eq!(bytes[2], 0x97);
        assert_eq!(bytes[5], 0x97);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(out);
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ctox-qwen35-metalpack-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_safetensors(path: &Path, header: &str, data: &[u8]) {
        let mut file = File::create(path).unwrap();
        file.write_all(&(header.len() as u64).to_le_bytes())
            .unwrap();
        file.write_all(header.as_bytes()).unwrap();
        file.write_all(data).unwrap();
    }
}
