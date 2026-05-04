use crate::audio;
use crate::consts::{
    VOX_DEC_DIM, VOX_DEC_HEAD_DIM, VOX_DEC_HEADS, VOX_DEC_HIDDEN, VOX_DEC_KV_HEADS,
    VOX_DEC_LAYERS, VOX_ENC_DIM, VOX_ENC_HEAD_DIM, VOX_ENC_HEADS, VOX_ENC_HIDDEN,
    VOX_ENC_KV_HEADS, VOX_ENC_LAYERS, VOX_NUM_MEL_BINS, VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
};
use crate::gguf;
use crate::kernels::VoxtralSttBackend;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxtralSttConfig {
    pub model: String,
    pub max_audio_tokens: usize,
    pub max_decode_tokens: usize,
}

impl Default for VoxtralSttConfig {
    fn default() -> Self {
        Self {
            model: VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL.to_string(),
            max_audio_tokens: 8192,
            max_decode_tokens: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoxtralSttArtifactInspection {
    pub root: PathBuf,
    pub gguf_path: PathBuf,
    pub tensor_count: usize,
    pub architecture: Option<String>,
    pub required_tensors_present: bool,
    pub missing_required_tensors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VoxtralSttModel {
    config: VoxtralSttConfig,
    backend: VoxtralSttBackend,
    inspection: Option<VoxtralSttArtifactInspection>,
    preprocess: audio::MelSpectrogramPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionRequest<'a> {
    pub audio_bytes: &'a [u8],
    pub response_format: &'a str,
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionResponse {
    pub model: String,
    pub text: String,
}

impl VoxtralSttModel {
    pub fn new(config: VoxtralSttConfig, backend: VoxtralSttBackend) -> Self {
        Self {
            config,
            backend,
            inspection: None,
            preprocess: audio::MelSpectrogramPlan::default(),
        }
    }

    pub fn from_gguf(path: impl AsRef<Path>, backend: VoxtralSttBackend) -> Result<Self> {
        let inspection = inspect_gguf(path)?;
        if !inspection.required_tensors_present {
            return Err(Error::Parse(format!(
                "missing required tensors: {}",
                inspection.missing_required_tensors.join(", ")
            )));
        }
        Ok(Self {
            config: VoxtralSttConfig::default(),
            backend,
            inspection: Some(inspection),
            preprocess: audio::MelSpectrogramPlan::default(),
        })
    }

    pub fn config(&self) -> &VoxtralSttConfig {
        &self.config
    }

    pub fn backend(&self) -> VoxtralSttBackend {
        self.backend
    }

    pub fn artifacts_loaded(&self) -> bool {
        self.inspection.is_some()
    }

    pub fn inspection(&self) -> Option<&VoxtralSttArtifactInspection> {
        self.inspection.as_ref()
    }

    pub fn transcribe(&self, request: &TranscriptionRequest<'_>) -> Result<TranscriptionResponse> {
        if request.audio_bytes.is_empty() {
            return Err(Error::InvalidFormat("transcription audio is empty"));
        }
        if request.response_format != "json" && request.response_format != "text" {
            return Err(Error::Unsupported(
                "native Voxtral STT currently accepts json or text response formats only",
            ));
        }
        let wav = audio::parse_wav(request.audio_bytes)?;
        let padded = audio::pad_audio_streaming(&wav.samples, 32, 17);
        let _mel = self.preprocess.compute(&padded);
        let _ = request.max_tokens;
        Err(Error::Unsupported(
            "native Voxtral STT encoder/adapter/decoder graph is not wired yet",
        ))
    }
}

pub fn inspect_gguf(path: impl AsRef<Path>) -> Result<VoxtralSttArtifactInspection> {
    let gguf_path = path.as_ref().to_path_buf();
    if !gguf_path.is_file() {
        return Err(Error::InvalidFormat("expected Voxtral GGUF model file"));
    }
    let inspected = gguf::inspect(&gguf_path)?;
    let missing_required_tensors = required_tensors()
        .into_iter()
        .filter(|name| !inspected.tensors.iter().any(|tensor| tensor.name == *name))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let root = gguf_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(VoxtralSttArtifactInspection {
        root,
        gguf_path,
        tensor_count: inspected.tensors.len(),
        architecture: inspected.architecture,
        required_tensors_present: missing_required_tensors.is_empty(),
        missing_required_tensors,
    })
}

pub fn required_tensors() -> Vec<&'static str> {
    let mut out = vec![
        "enc.conv0.weight",
        "enc.conv0.bias",
        "enc.conv1.weight",
        "enc.conv1.bias",
        "enc.norm.weight",
        "adapter.0.weight",
        "adapter.2.weight",
        "tok_embeddings.weight",
        "dec.norm.weight",
        "audio.mel_filters",
    ];
    out.extend([
        "enc.layers.0.attn_norm.weight",
        "enc.layers.0.attn.q.weight",
        "enc.layers.0.attn.k.weight",
        "enc.layers.0.attn.v.weight",
        "enc.layers.0.attn.o.weight",
        "enc.layers.0.ffn_norm.weight",
        "enc.layers.0.ffn.w1.weight",
        "enc.layers.0.ffn.w2.weight",
        "enc.layers.0.ffn.w3.weight",
        "dec.layers.0.attn_norm.weight",
        "dec.layers.0.attn.q.weight",
        "dec.layers.0.attn.k.weight",
        "dec.layers.0.attn.v.weight",
        "dec.layers.0.attn.o.weight",
        "dec.layers.0.ffn_norm.weight",
        "dec.layers.0.ffn.w1.weight",
        "dec.layers.0.ffn.w2.weight",
        "dec.layers.0.ffn.w3.weight",
    ]);
    out
}

pub fn shape_contract() -> Vec<(&'static str, usize)> {
    vec![
        ("enc_dim", VOX_ENC_DIM),
        ("enc_layers", VOX_ENC_LAYERS),
        ("enc_heads", VOX_ENC_HEADS),
        ("enc_head_dim", VOX_ENC_HEAD_DIM),
        ("enc_hidden", VOX_ENC_HIDDEN),
        ("enc_kv_heads", VOX_ENC_KV_HEADS),
        ("dec_dim", VOX_DEC_DIM),
        ("dec_layers", VOX_DEC_LAYERS),
        ("dec_heads", VOX_DEC_HEADS),
        ("dec_head_dim", VOX_DEC_HEAD_DIM),
        ("dec_hidden", VOX_DEC_HIDDEN),
        ("dec_kv_heads", VOX_DEC_KV_HEADS),
        ("num_mel_bins", VOX_NUM_MEL_BINS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_targets_voxtral_stt() {
        let config = VoxtralSttConfig::default();
        assert_eq!(config.model, VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL);
        assert_eq!(config.max_decode_tokens, 256);
    }

    #[test]
    fn transcribe_does_not_return_fake_text() {
        let model = VoxtralSttModel::new(VoxtralSttConfig::default(), VoxtralSttBackend::Cpu);
        let wav = tiny_wav();
        let err = model
            .transcribe(&TranscriptionRequest {
                audio_bytes: &wav,
                response_format: "json",
                max_tokens: None,
            })
            .expect_err("STT graph should not be faked");
        assert!(err.to_string().contains("not wired"));
    }

    fn tiny_wav() -> Vec<u8> {
        let mut out = Vec::new();
        let samples = [0i16; 320];
        let data_len = samples.len() * 2;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&16_000u32.to_le_bytes());
        out.extend_from_slice(&32_000u32.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }
}
