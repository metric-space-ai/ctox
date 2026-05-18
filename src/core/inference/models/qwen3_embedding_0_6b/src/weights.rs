use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::artifacts::{read_safetensors_header, ModelArtifacts, TensorInfo};
use crate::common::{EmbeddingError, EmbeddingResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorLocation {
    pub path: PathBuf,
    pub data_start: u64,
    pub info: TensorInfo,
}

#[derive(Debug, Clone)]
pub struct WeightIndex {
    tensors: BTreeMap<String, TensorLocation>,
}

impl WeightIndex {
    pub fn from_artifacts(artifacts: &ModelArtifacts) -> EmbeddingResult<Self> {
        let mut tensors = BTreeMap::new();
        for path in &artifacts.safetensors {
            let header = read_safetensors_header(path)
                .map_err(|detail| EmbeddingError::InvalidArtifacts { detail })?;
            for info in header.tensors {
                if tensors.contains_key(&info.name) {
                    return Err(EmbeddingError::InvalidArtifacts {
                        detail: format!("duplicate tensor `{}`", info.name),
                    });
                }
                tensors.insert(
                    info.name.clone(),
                    TensorLocation {
                        path: path.clone(),
                        data_start: header.data_start,
                        info,
                    },
                );
            }
        }
        Ok(Self { tensors })
    }

    pub fn tensor_count(&self) -> usize {
        self.tensors.len()
    }

    pub fn tensor(&self, name: &str) -> Option<&TensorLocation> {
        self.tensors.get(name)
    }

    pub fn read_bf16_tensor_as_f32(&self, name: &str) -> EmbeddingResult<Vec<f32>> {
        let location = self.required_tensor(name)?;
        ensure_bf16(location)?;
        let expected_values = checked_element_count(&location.info.shape)?;
        let bytes = read_tensor_payload(location)?;
        if bytes.len() != expected_values * 2 {
            return Err(EmbeddingError::InvalidArtifacts {
                detail: format!(
                    "tensor `{name}` has {} bytes, expected {}",
                    bytes.len(),
                    expected_values * 2
                ),
            });
        }
        Ok(bytes
            .chunks_exact(2)
            .map(|bytes| bf16_to_f32(u16::from_le_bytes([bytes[0], bytes[1]])))
            .collect())
    }

    pub fn read_bf16_rows_as_f32(
        &self,
        name: &str,
        row_indices: &[usize],
    ) -> EmbeddingResult<Vec<Vec<f32>>> {
        let location = self.required_tensor(name)?;
        ensure_bf16(location)?;
        if location.info.shape.len() != 2 {
            return Err(EmbeddingError::InvalidShape {
                detail: format!("tensor `{name}` is not rank-2"),
            });
        }
        let rows = location.info.shape[0];
        let cols = location.info.shape[1];
        let row_bytes = cols
            .checked_mul(2)
            .ok_or_else(|| EmbeddingError::InvalidShape {
                detail: format!("tensor `{name}` row byte length overflows usize"),
            })?;
        let mut file = std::fs::File::open(&location.path).map_err(|err| {
            EmbeddingError::InvalidArtifacts {
                detail: format!("failed to open {}: {err}", location.path.display()),
            }
        })?;
        let mut out = Vec::with_capacity(row_indices.len());
        for &row in row_indices {
            if row >= rows {
                return Err(EmbeddingError::InvalidShape {
                    detail: format!("tensor `{name}` row {row} out of range 0..{rows}"),
                });
            }
            let row_offset = (row as u64).checked_mul(row_bytes as u64).ok_or_else(|| {
                EmbeddingError::InvalidShape {
                    detail: format!("tensor `{name}` row offset overflows u64"),
                }
            })?;
            let start = location
                .data_start
                .checked_add(location.info.data_offsets.0)
                .and_then(|value| value.checked_add(row_offset))
                .ok_or_else(|| EmbeddingError::InvalidShape {
                    detail: format!("tensor `{name}` absolute row offset overflows u64"),
                })?;
            file.seek(SeekFrom::Start(start))
                .map_err(|err| EmbeddingError::InvalidArtifacts {
                    detail: format!("failed to seek tensor `{name}` row {row}: {err}"),
                })?;
            let mut bytes = vec![0_u8; row_bytes];
            file.read_exact(&mut bytes)
                .map_err(|err| EmbeddingError::InvalidArtifacts {
                    detail: format!("failed to read tensor `{name}` row {row}: {err}"),
                })?;
            out.push(
                bytes
                    .chunks_exact(2)
                    .map(|bytes| bf16_to_f32(u16::from_le_bytes([bytes[0], bytes[1]])))
                    .collect(),
            );
        }
        Ok(out)
    }

    fn required_tensor(&self, name: &str) -> EmbeddingResult<&TensorLocation> {
        self.tensor(name)
            .ok_or_else(|| EmbeddingError::InvalidArtifacts {
                detail: format!("missing tensor `{name}`"),
            })
    }
}

pub fn bf16_to_f32(raw: u16) -> f32 {
    f32::from_bits((raw as u32) << 16)
}

fn ensure_bf16(location: &TensorLocation) -> EmbeddingResult<()> {
    if location.info.dtype == "BF16" {
        Ok(())
    } else {
        Err(EmbeddingError::InvalidArtifacts {
            detail: format!(
                "tensor `{}` dtype is {}, expected BF16",
                location.info.name, location.info.dtype
            ),
        })
    }
}

fn checked_element_count(shape: &[usize]) -> EmbeddingResult<usize> {
    shape.iter().try_fold(1_usize, |acc, dim| {
        acc.checked_mul(*dim)
            .ok_or_else(|| EmbeddingError::InvalidShape {
                detail: "tensor element count overflows usize".to_string(),
            })
    })
}

fn read_tensor_payload(location: &TensorLocation) -> EmbeddingResult<Vec<u8>> {
    if location.info.data_offsets.1 < location.info.data_offsets.0 {
        return Err(EmbeddingError::InvalidArtifacts {
            detail: format!("tensor `{}` has invalid offsets", location.info.name),
        });
    }
    let len = location.info.data_offsets.1 - location.info.data_offsets.0;
    if len > usize::MAX as u64 {
        return Err(EmbeddingError::InvalidShape {
            detail: format!("tensor `{}` is too large for this host", location.info.name),
        });
    }
    let mut file =
        std::fs::File::open(&location.path).map_err(|err| EmbeddingError::InvalidArtifacts {
            detail: format!("failed to open {}: {err}", location.path.display()),
        })?;
    let start = location
        .data_start
        .checked_add(location.info.data_offsets.0)
        .ok_or_else(|| EmbeddingError::InvalidShape {
            detail: format!("tensor `{}` data offset overflows u64", location.info.name),
        })?;
    file.seek(SeekFrom::Start(start))
        .map_err(|err| EmbeddingError::InvalidArtifacts {
            detail: format!("failed to seek tensor `{}`: {err}", location.info.name),
        })?;
    let mut bytes = vec![0_u8; len as usize];
    file.read_exact(&mut bytes)
        .map_err(|err| EmbeddingError::InvalidArtifacts {
            detail: format!("failed to read tensor `{}`: {err}", location.info.name),
        })?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn converts_bf16_to_f32() {
        assert_eq!(bf16_to_f32(0x3f80), 1.0);
        assert_eq!(bf16_to_f32(0x4000), 2.0);
    }

    #[test]
    fn reads_bf16_tensor_and_rows() {
        let root = temp_root("weights");
        let model = root.join("runtime/models/Qwen3-Embedding-0.6B");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("config.json"), "{}").unwrap();
        std::fs::write(model.join("tokenizer.json"), "{}").unwrap();
        std::fs::write(model.join("model.safetensors"), test_safetensors()).unwrap();
        let artifacts = crate::artifacts::artifacts_at(&model).unwrap();
        let weights = WeightIndex::from_artifacts(&artifacts).unwrap();

        assert_eq!(weights.tensor_count(), 2);
        assert_eq!(
            weights.read_bf16_tensor_as_f32("norm.weight").unwrap(),
            vec![1.0, 2.0, 3.0]
        );
        assert_eq!(
            weights
                .read_bf16_rows_as_f32("embed_tokens.weight", &[1, 0])
                .unwrap(),
            vec![vec![4.0, 5.0, 6.0], vec![1.0, 2.0, 3.0]]
        );
    }

    fn test_safetensors() -> Vec<u8> {
        let header = r#"{"embed_tokens.weight":{"dtype":"BF16","shape":[2,3],"data_offsets":[0,12]},"norm.weight":{"dtype":"BF16","shape":[3],"data_offsets":[12,18]}}"#;
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(header.as_bytes());
        for value in [
            0x3f80_u16, 0x4000, 0x4040, 0x4080, 0x40a0, 0x40c0, 0x3f80, 0x4000, 0x4040,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ctox-qwen3-{label}-{unique}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
