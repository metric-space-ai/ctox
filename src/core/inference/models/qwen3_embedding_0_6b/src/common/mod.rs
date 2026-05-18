use std::error::Error;
use std::fmt;

pub const QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL: &str = "Qwen/Qwen3-Embedding-0.6B";
pub const QWEN3_EMBEDDING_0_6B_OLLAMA_ALIAS: &str = "qwen3-embedding:0.6b";
pub const QWEN3_EMBEDDING_0_6B_DEFAULT_DIM: usize = 1024;
pub const QWEN3_EMBEDDING_0_6B_DEFAULT_MAX_TOKENS: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingBackend {
    Cpu,
    Metal,
    Cuda,
}

impl fmt::Display for EmbeddingBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cpu => formatter.write_str("cpu"),
            Self::Metal => formatter.write_str("metal"),
            Self::Cuda => formatter.write_str("cuda"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolingMode {
    LastToken,
    Mean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen3EmbeddingConfig {
    pub model: String,
    pub embedding_dim: usize,
    pub max_tokens: usize,
    pub pooling: PoolingMode,
    pub normalize: bool,
}

impl Default for Qwen3EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL.to_string(),
            embedding_dim: QWEN3_EMBEDDING_0_6B_DEFAULT_DIM,
            max_tokens: QWEN3_EMBEDDING_0_6B_DEFAULT_MAX_TOKENS,
            pooling: PoolingMode::LastToken,
            normalize: true,
        }
    }
}

pub struct EmbedBatchRequest<'a> {
    pub inputs: &'a [String],
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedBatchResponse {
    pub model: String,
    pub embeddings: Vec<Vec<f32>>,
}

pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingError {
    EmptyInput,
    InvalidArtifacts {
        detail: String,
    },
    InvalidShape {
        detail: String,
    },
    BackendNotWired {
        backend: EmbeddingBackend,
        detail: String,
    },
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("embedding input is empty"),
            Self::InvalidArtifacts { detail } => {
                write!(formatter, "invalid model artifacts: {detail}")
            }
            Self::InvalidShape { detail } => {
                write!(formatter, "invalid embedding tensor shape: {detail}")
            }
            Self::BackendNotWired { backend, detail } => {
                write!(formatter, "{backend} embedding backend not wired: {detail}")
            }
        }
    }
}

impl Error for EmbeddingError {}

pub fn canonical_embedding_model_name(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "qwen/qwen3-embedding-0.6b"
        | "qwen3-embedding-0.6b"
        | "qwen3 embedding 0.6b"
        | "qwen3-embedding:0.6b" => Some(QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_ollama_style_alias_without_adding_ollama_support() {
        assert_eq!(
            canonical_embedding_model_name(QWEN3_EMBEDDING_0_6B_OLLAMA_ALIAS),
            Some(QWEN3_EMBEDDING_0_6B_CANONICAL_MODEL)
        );
    }
}
