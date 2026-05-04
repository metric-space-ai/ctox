//! Local Hugging Face artifact inspection for Qwen3.5-0.8B.
//!
//! This module intentionally stops at metadata. It validates config and
//! safetensors headers before any production weight packing is attempted.

use std::{
    collections::BTreeSet,
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

use crate::model_shape::{LayerKind, ModelShape, QWEN35_08B};
use crate::pack_plan::{PackPlan, PackPlanWarning};

#[derive(Debug)]
pub enum ArtifactError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    MissingConfig(PathBuf),
    MissingSafetensors(PathBuf),
    InvalidConfig(String),
    InvalidSafetensorHeader {
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::Json { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::MissingConfig(root) => write!(f, "missing config.json in {}", root.display()),
            Self::MissingSafetensors(root) => {
                write!(f, "no .safetensors files found in {}", root.display())
            }
            Self::InvalidConfig(reason) => write!(f, "invalid artifact config: {reason}"),
            Self::InvalidSafetensorHeader { path, reason } => {
                write!(
                    f,
                    "{}: invalid safetensors header: {}",
                    path.display(),
                    reason
                )
            }
        }
    }
}

impl std::error::Error for ArtifactError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReport {
    pub root: PathBuf,
    pub config: ConfigReport,
    pub safetensors: SafetensorsReport,
    pub pack_plan: PackPlan,
}

impl ArtifactReport {
    pub fn is_shape_compatible(&self) -> bool {
        self.config.mismatches.is_empty()
    }

    pub fn blocking_warnings(&self) -> Vec<&PackPlanWarning> {
        self.pack_plan
            .warnings
            .iter()
            .filter(|warning| warning.blocking)
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigReport {
    pub path: PathBuf,
    pub model_type: Option<String>,
    pub architectures: Vec<String>,
    pub extracted: ExtractedQwenConfig,
    pub mismatches: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExtractedQwenConfig {
    pub hidden_size: Option<usize>,
    pub vocab_size: Option<usize>,
    pub n_layers: Option<usize>,
    pub ffn_intermediate: Option<usize>,
    pub attention_q_heads: Option<usize>,
    pub attention_kv_heads: Option<usize>,
    pub attention_head_dim: Option<usize>,
    pub native_context: Option<usize>,
    pub layer_kinds: Option<Vec<LayerKind>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafetensorsReport {
    pub index_path: Option<PathBuf>,
    pub shards: Vec<SafetensorShard>,
    pub tensors: Vec<TensorHeader>,
    pub total_tensor_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafetensorShard {
    pub path: PathBuf,
    pub tensor_count: usize,
    pub tensor_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TensorHeader {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub data_offsets: [u64; 2],
    pub data_start: u64,
    pub shard: PathBuf,
}

pub fn inspect_model_artifacts(root: impl AsRef<Path>) -> Result<ArtifactReport, ArtifactError> {
    inspect_model_artifacts_for_shape(root, &QWEN35_08B)
}

pub fn inspect_model_artifacts_for_shape(
    root: impl AsRef<Path>,
    shape: &ModelShape,
) -> Result<ArtifactReport, ArtifactError> {
    let root = root.as_ref().to_path_buf();
    let config = inspect_config(&root, shape)?;
    let safetensors = inspect_safetensors(&root)?;
    let pack_plan = PackPlan::from_tensors(shape, &safetensors.tensors);

    Ok(ArtifactReport {
        root,
        config,
        safetensors,
        pack_plan,
    })
}

fn inspect_config(root: &Path, shape: &ModelShape) -> Result<ConfigReport, ArtifactError> {
    let path = root.join("config.json");
    if !path.exists() {
        return Err(ArtifactError::MissingConfig(root.to_path_buf()));
    }

    let value = read_json(&path)?;
    let text_value = value.get("text_config").unwrap_or(&value);
    let extracted = extract_qwen_config(text_value);
    let model_type = text_value
        .get("model_type")
        .and_then(Value::as_str)
        .or_else(|| value.get("model_type").and_then(Value::as_str))
        .map(str::to_owned);
    let architectures = value
        .get("architectures")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mismatches = validate_config(&extracted, shape);

    Ok(ConfigReport {
        path,
        model_type,
        architectures,
        extracted,
        mismatches,
    })
}

fn read_json(path: &Path) -> Result<Value, ArtifactError> {
    let bytes = fs::read(path).map_err(|source| ArtifactError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ArtifactError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn extract_qwen_config(value: &Value) -> ExtractedQwenConfig {
    let hidden_size = get_usize(value, &["hidden_size", "d_model"]);
    let attention_q_heads = get_usize(value, &["num_attention_heads", "n_head"]);
    let attention_head_dim = get_usize(value, &["head_dim", "attention_head_dim"])
        .or_else(|| hidden_size.and_then(|hidden| attention_q_heads.map(|heads| hidden / heads)));

    ExtractedQwenConfig {
        hidden_size,
        vocab_size: get_usize(value, &["vocab_size"]),
        n_layers: get_usize(value, &["num_hidden_layers", "n_layer", "num_layers"]),
        ffn_intermediate: get_usize(value, &["intermediate_size", "ffn_intermediate_size"]),
        attention_q_heads,
        attention_kv_heads: get_usize(value, &["num_key_value_heads", "num_kv_heads"]),
        attention_head_dim,
        native_context: get_usize(
            value,
            &[
                "max_position_embeddings",
                "seq_length",
                "max_sequence_length",
                "model_max_length",
            ],
        ),
        layer_kinds: extract_layer_kinds(value),
    }
}

fn get_usize(value: &Value, keys: &[&str]) -> Option<usize> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn extract_layer_kinds(value: &Value) -> Option<Vec<LayerKind>> {
    let array = value
        .get("layer_types")
        .or_else(|| value.get("layers_block_type"))
        .and_then(Value::as_array)?;

    let mut kinds = Vec::with_capacity(array.len());
    for item in array {
        let raw = item.as_str()?.to_ascii_lowercase();
        let kind = if raw.contains("attention") && !raw.contains("linear") {
            LayerKind::FullAttention
        } else if raw.contains("delta") || raw.contains("linear") {
            LayerKind::GatedDeltaNet
        } else {
            return None;
        };
        kinds.push(kind);
    }
    Some(kinds)
}

fn validate_config(extracted: &ExtractedQwenConfig, shape: &ModelShape) -> Vec<String> {
    let mut mismatches = Vec::new();
    compare_usize(
        &mut mismatches,
        "hidden_size",
        extracted.hidden_size,
        shape.hidden_size,
    );
    compare_usize(
        &mut mismatches,
        "vocab_size",
        extracted.vocab_size,
        shape.vocab_size,
    );
    compare_usize(
        &mut mismatches,
        "n_layers",
        extracted.n_layers,
        shape.n_layers,
    );
    compare_usize(
        &mut mismatches,
        "ffn_intermediate",
        extracted.ffn_intermediate,
        shape.ffn_intermediate,
    );
    compare_usize(
        &mut mismatches,
        "attention_q_heads",
        extracted.attention_q_heads,
        shape.attention_q_heads,
    );
    compare_usize(
        &mut mismatches,
        "attention_kv_heads",
        extracted.attention_kv_heads,
        shape.attention_kv_heads,
    );
    compare_usize(
        &mut mismatches,
        "attention_head_dim",
        extracted.attention_head_dim,
        shape.attention_head_dim,
    );

    if let Some(layer_kinds) = &extracted.layer_kinds {
        if layer_kinds.len() != shape.n_layers {
            mismatches.push(format!(
                "layer_kinds: expected {} entries, got {}",
                shape.n_layers,
                layer_kinds.len()
            ));
        } else {
            for (layer, actual) in layer_kinds.iter().enumerate() {
                let expected = shape.layer_kind(layer);
                if *actual != expected {
                    mismatches.push(format!(
                        "layer {layer}: expected {:?}, got {:?}",
                        expected, actual
                    ));
                }
            }
        }
    }

    mismatches
}

fn compare_usize(mismatches: &mut Vec<String>, name: &str, actual: Option<usize>, expected: usize) {
    if let Some(actual) = actual {
        if actual != expected {
            mismatches.push(format!("{name}: expected {expected}, got {actual}"));
        }
    }
}

fn inspect_safetensors(root: &Path) -> Result<SafetensorsReport, ArtifactError> {
    let (index_path, shard_paths) = discover_safetensor_shards(root)?;
    let mut shards = Vec::with_capacity(shard_paths.len());
    let mut tensors = Vec::new();
    let mut total_tensor_bytes = 0u64;

    for path in shard_paths {
        let shard_tensors = read_safetensor_header(&path)?;
        let tensor_bytes = shard_tensors
            .iter()
            .map(|tensor| tensor.data_offsets[1].saturating_sub(tensor.data_offsets[0]))
            .sum();
        total_tensor_bytes += tensor_bytes;
        shards.push(SafetensorShard {
            path: path.clone(),
            tensor_count: shard_tensors.len(),
            tensor_bytes,
        });
        tensors.extend(shard_tensors);
    }

    tensors.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(SafetensorsReport {
        index_path,
        shards,
        tensors,
        total_tensor_bytes,
    })
}

fn discover_safetensor_shards(
    root: &Path,
) -> Result<(Option<PathBuf>, Vec<PathBuf>), ArtifactError> {
    let index_path = root.join("model.safetensors.index.json");
    if index_path.exists() {
        let value = read_json(&index_path)?;
        let weight_map = value
            .get("weight_map")
            .and_then(Value::as_object)
            .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                path: index_path.clone(),
                reason: "missing weight_map".to_owned(),
            })?;
        let mut files = BTreeSet::new();
        for file in weight_map.values().filter_map(Value::as_str) {
            files.insert(root.join(file));
        }
        if files.is_empty() {
            return Err(ArtifactError::MissingSafetensors(root.to_path_buf()));
        }
        return Ok((Some(index_path), files.into_iter().collect()));
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(root).map_err(|source| ArtifactError::Io {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ArtifactError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("safetensors") {
            files.push(path);
        }
    }
    files.sort();

    if files.is_empty() {
        return Err(ArtifactError::MissingSafetensors(root.to_path_buf()));
    }

    Ok((None, files))
}

fn read_safetensor_header(path: &Path) -> Result<Vec<TensorHeader>, ArtifactError> {
    let mut file = File::open(path).map_err(|source| ArtifactError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes)
        .map_err(|source| ArtifactError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let header_len = u64::from_le_bytes(len_bytes);
    if header_len > 128 * 1024 * 1024 {
        return Err(ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("header length {header_len} exceeds guard"),
        });
    }
    let mut header = vec![0u8; header_len as usize];
    file.seek(SeekFrom::Start(8))
        .map_err(|source| ArtifactError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.read_exact(&mut header)
        .map_err(|source| ArtifactError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let value: Value = serde_json::from_slice(&header).map_err(|source| ArtifactError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let data_start = 8 + header_len;
    let object = value
        .as_object()
        .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: "top-level JSON is not an object".to_owned(),
        })?;

    let mut tensors = Vec::new();
    for (name, info) in object {
        if name == "__metadata__" {
            continue;
        }
        let info = info
            .as_object()
            .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                path: path.to_path_buf(),
                reason: format!("{name}: tensor entry is not an object"),
            })?;
        let dtype = info
            .get("dtype")
            .and_then(Value::as_str)
            .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                path: path.to_path_buf(),
                reason: format!("{name}: missing dtype"),
            })?
            .to_owned();
        let shape = info
            .get("shape")
            .and_then(Value::as_array)
            .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                path: path.to_path_buf(),
                reason: format!("{name}: missing shape"),
            })?
            .iter()
            .map(|item| {
                item.as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
                        path: path.to_path_buf(),
                        reason: format!("{name}: invalid shape item"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let data_offsets = parse_offsets(path, name, info)?;
        tensors.push(TensorHeader {
            name: name.clone(),
            dtype,
            shape,
            data_offsets,
            data_start,
            shard: path.to_path_buf(),
        });
    }

    Ok(tensors)
}

fn parse_offsets(
    path: &Path,
    name: &str,
    info: &Map<String, Value>,
) -> Result<[u64; 2], ArtifactError> {
    let offsets = info
        .get("data_offsets")
        .and_then(Value::as_array)
        .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("{name}: missing data_offsets"),
        })?;
    if offsets.len() != 2 {
        return Err(ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("{name}: data_offsets length must be 2"),
        });
    }
    let start = offsets[0]
        .as_u64()
        .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("{name}: invalid start offset"),
        })?;
    let end = offsets[1]
        .as_u64()
        .ok_or_else(|| ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("{name}: invalid end offset"),
        })?;
    if end < start {
        return Err(ArtifactError::InvalidSafetensorHeader {
            path: path.to_path_buf(),
            reason: format!("{name}: end offset precedes start offset"),
        });
    }
    Ok([start, end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn inspects_minimal_local_artifacts() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("config.json"),
            r#"{
              "model_type": "qwen3_5",
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
        write_safetensors_header(
            &root.join("model.safetensors"),
            r#"{
              "model.embed_tokens.weight": {"dtype": "F16", "shape": [248320, 1024], "data_offsets": [0, 508559360]},
              "model.layers.0.mlp.gate_proj.weight": {"dtype": "F16", "shape": [3584, 1024], "data_offsets": [508559360, 515899392]},
              "lm_head.weight": {"dtype": "F16", "shape": [248320, 1024], "data_offsets": [515899392, 1024458752]}
            }"#,
        );

        let report = inspect_model_artifacts(&root).unwrap();
        assert!(
            report.is_shape_compatible(),
            "{:?}",
            report.config.mismatches
        );
        assert_eq!(report.safetensors.shards.len(), 1);
        assert_eq!(report.safetensors.tensors.len(), 3);
        assert!(report
            .pack_plan
            .entries
            .iter()
            .any(|entry| entry.tensor == "model.embed_tokens.weight"));

        let _ = fs::remove_dir_all(root);
    }

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ctox-qwen35-artifacts-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_safetensors_header(path: &Path, header: &str) {
        let mut file = File::create(path).unwrap();
        file.write_all(&(header.len() as u64).to_le_bytes())
            .unwrap();
        file.write_all(header.as_bytes()).unwrap();
    }
}
