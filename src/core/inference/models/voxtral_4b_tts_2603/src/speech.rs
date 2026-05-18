use crate::model::names;
use crate::safetensors::SafeTensors;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

pub const VOXTRAL_4B_TTS_2603_CANONICAL_MODEL: &str = "engineai/Voxtral-4B-TTS-2603";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoxtralTtsBackend {
    Cpu,
    Metal,
    Cuda,
    Wgsl,
}

impl VoxtralTtsBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "cpu-rust-reference",
            Self::Metal => "metal-vendored-kernels",
            Self::Cuda => "cuda-vendored-kernels",
            Self::Wgsl => "wgsl-vendored-kernels",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxtralTtsConfig {
    pub model: String,
    pub max_text_tokens: usize,
    pub response_format: String,
}

impl Default for VoxtralTtsConfig {
    fn default() -> Self {
        Self {
            model: VOXTRAL_4B_TTS_2603_CANONICAL_MODEL.to_string(),
            max_text_tokens: 8192,
            response_format: "wav".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxtralTtsArtifactInspection {
    pub root: PathBuf,
    pub weights_path: PathBuf,
    pub tensor_count: usize,
    pub required_tensors_present: bool,
    pub missing_required_tensors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VoxtralTtsModel {
    config: VoxtralTtsConfig,
    backend: VoxtralTtsBackend,
    inspection: Option<VoxtralTtsArtifactInspection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechRequest<'a> {
    pub input: &'a str,
    pub voice: Option<&'a str>,
    pub response_format: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechResponse {
    pub model: String,
    pub audio: Vec<u8>,
    pub response_format: String,
}

impl VoxtralTtsModel {
    pub fn new(config: VoxtralTtsConfig, backend: VoxtralTtsBackend) -> Self {
        Self {
            config,
            backend,
            inspection: None,
        }
    }

    pub fn from_model_dir(model_dir: impl AsRef<Path>, backend: VoxtralTtsBackend) -> Result<Self> {
        let inspection = inspect_model_dir(model_dir)?;
        if !inspection.required_tensors_present {
            return Err(Error::Parse(format!(
                "missing required tensors: {}",
                inspection.missing_required_tensors.join(", ")
            )));
        }
        Ok(Self {
            config: VoxtralTtsConfig::default(),
            backend,
            inspection: Some(inspection),
        })
    }

    pub fn config(&self) -> &VoxtralTtsConfig {
        &self.config
    }

    pub fn backend(&self) -> VoxtralTtsBackend {
        self.backend
    }

    pub fn artifacts_loaded(&self) -> bool {
        self.inspection.is_some()
    }

    pub fn inspection(&self) -> Option<&VoxtralTtsArtifactInspection> {
        self.inspection.as_ref()
    }

    pub fn synthesize(&self, request: &SpeechRequest<'_>) -> Result<SpeechResponse> {
        if request.input.trim().is_empty() {
            return Err(Error::InvalidFormat("speech input is empty"));
        }
        if request.response_format != "wav" {
            return Err(Error::Unsupported(
                "native Voxtral TTS currently accepts wav output only",
            ));
        }
        let _ = request.voice;
        Err(Error::Unsupported(
            "native Voxtral TTS text-to-audio graph is not wired yet",
        ))
    }
}

pub fn inspect_model_dir(model_dir: impl AsRef<Path>) -> Result<VoxtralTtsArtifactInspection> {
    let root = model_dir.as_ref().to_path_buf();
    let weights_path = root.join("consolidated.safetensors");
    if !weights_path.is_file() {
        return Err(Error::InvalidFormat(
            "expected consolidated.safetensors in model_dir",
        ));
    }
    let weights = SafeTensors::open(&weights_path)?;
    let missing_required_tensors = required_tensors()
        .into_iter()
        .filter(|name| weights.find(name).is_none())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(VoxtralTtsArtifactInspection {
        root,
        weights_path,
        tensor_count: weights.tensors().len(),
        required_tensors_present: missing_required_tensors.is_empty(),
        missing_required_tensors,
    })
}

pub fn required_tensors() -> Vec<&'static str> {
    vec![names::TOK_EMBEDDINGS, names::ADAPTER_L0, names::ADAPTER_L1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_targets_voxtral_tts() {
        let config = VoxtralTtsConfig::default();
        assert_eq!(config.model, VOXTRAL_4B_TTS_2603_CANONICAL_MODEL);
        assert_eq!(config.max_text_tokens, 8192);
        assert_eq!(config.response_format, "wav");
    }

    #[test]
    fn synthesize_fails_until_graph_is_wired() {
        let model = VoxtralTtsModel::new(VoxtralTtsConfig::default(), VoxtralTtsBackend::Cpu);
        let err = model
            .synthesize(&SpeechRequest {
                input: "Hallo CTOX.",
                voice: None,
                response_format: "wav",
            })
            .expect_err("native TTS must not return fake audio");
        assert!(err.to_string().contains("not wired"));
    }
}
