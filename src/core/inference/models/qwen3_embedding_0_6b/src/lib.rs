//! Bare-metal CTOX native port surface for Qwen3-Embedding-0.6B.
//!
//! This crate is intentionally self-contained. It must not depend on Ollama,
//! llama.cpp as a process, ctox-engine, Python, or any new third-party runtime.
//! The public API is the contract the CTOX scraper/doc/ticket stacks will call
//! once the backend forward passes are wired.

pub mod artifacts;
pub mod common;
pub mod cpu;
pub mod tokenizer;
pub mod weights;

#[cfg(any(target_os = "macos", feature = "metal"))]
pub mod metal;

#[cfg(any(target_os = "linux", feature = "cuda"))]
pub mod cuda;

pub use common::{
    EmbedBatchRequest, EmbedBatchResponse, EmbeddingBackend, EmbeddingError, EmbeddingResult,
    PoolingMode, Qwen3EmbeddingConfig, QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL,
    QWEN3_EMBEDDING_0_6B_DEFAULT_DIM,
};

/// Minimal model handle. Weight loading and transformer forward are implemented
/// per backend; CPU pooling/normalization helpers are already shared by tests.
#[derive(Debug, Clone)]
pub struct Qwen3EmbeddingModel {
    config: Qwen3EmbeddingConfig,
    backend: EmbeddingBackend,
    weights: Option<weights::WeightIndex>,
}

impl Qwen3EmbeddingModel {
    pub fn new(config: Qwen3EmbeddingConfig, backend: EmbeddingBackend) -> Self {
        Self {
            config,
            backend,
            weights: None,
        }
    }

    pub fn from_artifacts(
        artifacts: &artifacts::ModelArtifacts,
        backend: EmbeddingBackend,
    ) -> EmbeddingResult<Self> {
        let inspection = artifacts::inspect_artifacts(artifacts)
            .map_err(|detail| EmbeddingError::InvalidArtifacts { detail })?;
        if !inspection.required_tensors_present {
            return Err(EmbeddingError::InvalidArtifacts {
                detail: format!(
                    "missing required tensors: {}",
                    inspection.missing_required_tensors.join(", ")
                ),
            });
        }
        let pooling = inspection
            .pooling
            .as_ref()
            .map(|pooling| {
                if pooling.pooling_mode_lasttoken {
                    PoolingMode::LastToken
                } else {
                    PoolingMode::Mean
                }
            })
            .unwrap_or(PoolingMode::LastToken);
        let weights = weights::WeightIndex::from_artifacts(artifacts)?;
        Ok(Self {
            config: Qwen3EmbeddingConfig {
                model: QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL.to_string(),
                embedding_dim: inspection.model.hidden_size,
                max_tokens: inspection.model.max_position_embeddings,
                pooling,
                normalize: inspection
                    .pooling
                    .as_ref()
                    .map(|pooling| pooling.normalize_module_present)
                    .unwrap_or(true),
            },
            backend,
            weights: Some(weights),
        })
    }

    pub fn config(&self) -> &Qwen3EmbeddingConfig {
        &self.config
    }

    pub fn backend(&self) -> EmbeddingBackend {
        self.backend
    }

    pub fn weights_loaded(&self) -> bool {
        self.weights.is_some()
    }

    pub fn token_embedding_rows(&self, token_ids: &[usize]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let weights = self
            .weights
            .as_ref()
            .ok_or_else(|| EmbeddingError::InvalidArtifacts {
                detail: "model weights are not loaded".to_string(),
            })?;
        weights.read_bf16_rows_as_f32("embed_tokens.weight", token_ids)
    }

    pub fn embed_batch(
        &self,
        _request: &EmbedBatchRequest<'_>,
    ) -> EmbeddingResult<EmbedBatchResponse> {
        Err(EmbeddingError::BackendNotWired {
            backend: self.backend,
            detail: "transformer forward pass is not wired yet".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_targets_qwen3_embedding() {
        let config = Qwen3EmbeddingConfig::default();
        assert_eq!(config.model, QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL);
        assert_eq!(config.embedding_dim, QWEN3_EMBEDDING_0_6B_DEFAULT_DIM);
        assert_eq!(config.pooling, PoolingMode::LastToken);
        assert!(config.normalize);
    }

    #[test]
    fn model_surface_fails_until_backend_forward_is_wired() {
        let model =
            Qwen3EmbeddingModel::new(Qwen3EmbeddingConfig::default(), EmbeddingBackend::Cpu);
        let request = EmbedBatchRequest {
            inputs: &["hello".to_string()],
        };
        let err = model
            .embed_batch(&request)
            .expect_err("forward should not be faked");
        assert!(err.to_string().contains("not wired"));
    }

    #[test]
    fn model_loads_shape_contract_from_artifacts_without_faking_forward() {
        let root = std::env::temp_dir().join(format!(
            "ctox-qwen3-model-load-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let model_dir = root.join("runtime/models/Qwen3-Embedding-0.6B");
        std::fs::create_dir_all(model_dir.join("1_Pooling")).unwrap();
        std::fs::write(
            model_dir.join("config.json"),
            r#"{
              "model_type": "qwen3",
              "hidden_size": 1024,
              "intermediate_size": 3072,
              "max_position_embeddings": 32768,
              "num_hidden_layers": 28,
              "num_attention_heads": 16,
              "num_key_value_heads": 8,
              "head_dim": 128,
              "vocab_size": 151669
            }"#,
        )
        .unwrap();
        std::fs::write(model_dir.join("tokenizer.json"), "{}").unwrap();
        std::fs::write(model_dir.join("model.safetensors"), minimal_safetensors()).unwrap();
        std::fs::write(
            model_dir.join("1_Pooling/config.json"),
            r#"{"word_embedding_dimension":1024,"pooling_mode_lasttoken":true}"#,
        )
        .unwrap();
        let artifacts = artifacts::artifacts_at(&model_dir).unwrap();
        let model = Qwen3EmbeddingModel::from_artifacts(&artifacts, EmbeddingBackend::Cpu).unwrap();
        assert_eq!(model.config().embedding_dim, 1024);
        assert_eq!(model.config().max_tokens, 32768);
        assert!(model.weights_loaded());
        let rows = model.token_embedding_rows(&[0]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 1024);
        let request = EmbedBatchRequest {
            inputs: &["hello".to_string()],
        };
        assert!(model.embed_batch(&request).is_err());
    }

    fn minimal_safetensors() -> Vec<u8> {
        let required = [
            ("embed_tokens.weight", "[151669,1024]", "[0,2048]"),
            ("layers.0.input_layernorm.weight", "[1024]", "[2048,2048]"),
            (
                "layers.0.self_attn.q_proj.weight",
                "[2048,1024]",
                "[2048,2048]",
            ),
            (
                "layers.0.self_attn.k_proj.weight",
                "[1024,1024]",
                "[2048,2048]",
            ),
            (
                "layers.0.self_attn.v_proj.weight",
                "[1024,1024]",
                "[2048,2048]",
            ),
            (
                "layers.0.self_attn.o_proj.weight",
                "[1024,2048]",
                "[2048,2048]",
            ),
            (
                "layers.0.mlp.gate_proj.weight",
                "[3072,1024]",
                "[2048,2048]",
            ),
            ("layers.0.mlp.up_proj.weight", "[3072,1024]", "[2048,2048]"),
            (
                "layers.0.mlp.down_proj.weight",
                "[1024,3072]",
                "[2048,2048]",
            ),
            ("layers.27.input_layernorm.weight", "[1024]", "[2048,2048]"),
            (
                "layers.27.self_attn.q_proj.weight",
                "[2048,1024]",
                "[2048,2048]",
            ),
            (
                "layers.27.mlp.down_proj.weight",
                "[1024,3072]",
                "[2048,2048]",
            ),
            ("norm.weight", "[1024]", "[2048,2048]"),
        ];
        let mut header = String::from("{");
        for (index, (name, shape, offsets)) in required.iter().enumerate() {
            if index > 0 {
                header.push(',');
            }
            header.push_str(&format!(
                "\"{name}\":{{\"dtype\":\"BF16\",\"shape\":{shape},\"data_offsets\":{offsets}}}"
            ));
        }
        header.push('}');
        let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(header.as_bytes());
        bytes.extend(std::iter::repeat(0_u8).take(2048));
        bytes
    }
}
