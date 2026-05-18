//! Model loader shell. Full graph execution is assembled from `encoder`,
//! `adapter`, `decoder`, `tokenizer` and `stream` modules.

use crate::kernels::KernelBackend;
use crate::safetensors::SafeTensors;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

pub struct Voxtral<B: KernelBackend> {
    pub model_dir: PathBuf,
    pub weights: SafeTensors,
    pub backend: B,
}

impl<B: KernelBackend> Voxtral<B> {
    pub fn load(model_dir: impl AsRef<Path>, backend: B) -> Result<Self> {
        let model_dir = model_dir.as_ref().to_path_buf();
        let weights_path = model_dir.join("consolidated.safetensors");
        if !weights_path.exists() {
            return Err(Error::InvalidFormat(
                "expected consolidated.safetensors in model_dir",
            ));
        }
        let weights = SafeTensors::open(weights_path)?;
        Ok(Self {
            model_dir,
            weights,
            backend,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }
}

pub mod names {
    pub const TOK_EMBEDDINGS: &str = "mm_streams_embeddings.embedding_module.tok_embeddings.weight";
    pub const ADAPTER_L0: &str =
        "mm_streams_embeddings.embedding_module.audio_language_projection.0.weight";
    pub const ADAPTER_L1: &str =
        "mm_streams_embeddings.embedding_module.audio_language_projection.2.weight";

    pub fn enc(prefix: &str) -> String {
        format!("mm_streams_embeddings.embedding_module.whisper_encoder.{prefix}")
    }

    pub fn dec(layer: usize, suffix: &str) -> String {
        format!("layers.{layer}.{suffix}")
    }
}
