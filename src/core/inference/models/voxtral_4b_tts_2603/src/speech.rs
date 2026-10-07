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
            Self::Cpu => "cpu-vendored-voxtral-graph",
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
    session: Option<std::sync::Arc<crate::native_graph::NativeSession>>,
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
            session: None,
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
        if !crate::native_graph::available(backend == VoxtralTtsBackend::Cuda)
            || !matches!(backend, VoxtralTtsBackend::Cpu | VoxtralTtsBackend::Cuda)
            || (backend == VoxtralTtsBackend::Cpu && cfg!(voxtral_cuda))
        {
            return Err(Error::Unsupported(
                "requested native Voxtral graph backend was not compiled",
            ));
        }
        let session = crate::native_graph::load(&inspection.root)?;
        Ok(Self {
            config: VoxtralTtsConfig::default(),
            backend,
            inspection: Some(inspection),
            session: Some(session),
        })
    }

    pub fn config(&self) -> &VoxtralTtsConfig {
        &self.config
    }

    pub fn backend(&self) -> VoxtralTtsBackend {
        self.backend
    }

    pub fn artifacts_loaded(&self) -> bool {
        self.session.is_some()
    }

    pub fn inspection(&self) -> Option<&VoxtralTtsArtifactInspection> {
        self.inspection.as_ref()
    }

    pub fn graph_wired(&self) -> bool {
        matches!(
            self.backend,
            VoxtralTtsBackend::Cpu | VoxtralTtsBackend::Cuda
        ) && crate::native_graph::available(self.backend == VoxtralTtsBackend::Cuda)
            && (self.backend != VoxtralTtsBackend::Cpu || !cfg!(voxtral_cuda))
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
        if request.input.len() > 4096 {
            return Err(Error::InvalidFormat(
                "speech input exceeds 4096-byte turn limit",
            ));
        }
        if !self.graph_wired() {
            return Err(Error::Unsupported(
                "requested native Voxtral graph backend was not compiled",
            ));
        }
        let inspection = self.inspection.as_ref().ok_or(Error::Unsupported(
            "native Voxtral artifacts are not loaded",
        ))?;
        let voice = request.voice.unwrap_or("neutral_female");
        if !PRESET_VOICES.contains(&voice) {
            return Err(Error::Unsupported("unknown native Voxtral preset voice"));
        }
        if !inspection
            .root
            .join("voice_embedding")
            .join(format!("{voice}.pt"))
            .is_file()
        {
            return Err(Error::InvalidFormat(
                "requested preset voice artifact is missing",
            ));
        }
        let audio = self
            .session
            .as_ref()
            .ok_or(Error::Unsupported(
                "native Voxtral artifacts are not loaded",
            ))?
            .synthesize(request.input, voice)?;
        Ok(SpeechResponse {
            model: self.config.model.clone(),
            audio,
            response_format: "wav".into(),
        })
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
    if !root.join("tekken.json").is_file() {
        return Err(Error::InvalidFormat("expected tekken.json in model_dir"));
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
    vec![
        "mm_audio_embeddings.tok_embeddings.weight",
        "layers.0.attention.wq.weight",
        "acoustic_transformer.input_projection.weight",
        "acoustic_transformer.semantic_codebook_output.weight",
    ]
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
        assert!(err.to_string().contains("not loaded") || err.to_string().contains("not compiled"));
    }
}

const PRESET_VOICES: &[&str] = &[
    "casual_female",
    "casual_male",
    "cheerful_female",
    "neutral_female",
    "neutral_male",
    "fr_female",
    "fr_male",
    "de_female",
    "de_male",
    "es_female",
    "es_male",
    "it_female",
    "it_male",
    "pt_female",
    "pt_male",
    "nl_female",
    "nl_male",
    "ar_male",
    "hi_female",
    "hi_male",
];
