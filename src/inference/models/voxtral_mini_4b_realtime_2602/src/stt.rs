use crate::audio;
use crate::consts::{
    VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL, VOX_DEC_DIM, VOX_DEC_HEADS, VOX_DEC_HEAD_DIM,
    VOX_DEC_HIDDEN, VOX_DEC_KV_HEADS, VOX_DEC_LAYERS, VOX_ENC_DIM, VOX_ENC_HEADS, VOX_ENC_HEAD_DIM,
    VOX_ENC_HIDDEN, VOX_ENC_KV_HEADS, VOX_ENC_LAYERS, VOX_NUM_MEL_BINS,
};
use crate::gguf;
use crate::kernels::VoxtralSttBackend;
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

type WgpuBackend = burn::backend::Wgpu;

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
    pub tokenizer_path: Option<PathBuf>,
    pub tensor_count: usize,
    pub architecture: Option<String>,
    pub required_tensors_present: bool,
    pub missing_required_tensors: Vec<String>,
}

pub struct VoxtralSttModel {
    config: VoxtralSttConfig,
    backend: VoxtralSttBackend,
    inspection: Option<VoxtralSttArtifactInspection>,
    runtime: Option<Arc<Mutex<VoxtralQ4Runtime>>>,
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

struct VoxtralQ4Runtime {
    model: voxtral_mini_realtime::gguf::model::Q4VoxtralModel,
    tokenizer: voxtral_mini_realtime::tokenizer::VoxtralTokenizer,
    device: <WgpuBackend as burn::tensor::backend::Backend>::Device,
}

impl std::fmt::Debug for VoxtralSttModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxtralSttModel")
            .field("config", &self.config)
            .field("backend", &self.backend)
            .field("inspection", &self.inspection)
            .field("runtime_loaded", &self.runtime.is_some())
            .finish()
    }
}

impl Clone for VoxtralSttModel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            backend: self.backend,
            inspection: self.inspection.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

impl VoxtralSttModel {
    pub fn new(config: VoxtralSttConfig, backend: VoxtralSttBackend) -> Self {
        Self {
            config,
            backend,
            inspection: None,
            runtime: None,
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
        let tokenizer_path = inspection
            .tokenizer_path
            .clone()
            .ok_or_else(|| Error::Parse("missing tekken.json next to Voxtral GGUF".to_string()))?;
        let runtime = VoxtralQ4Runtime::load(&inspection.gguf_path, &tokenizer_path)?;
        Ok(Self {
            config: VoxtralSttConfig::default(),
            backend,
            inspection: Some(inspection),
            runtime: Some(Arc::new(Mutex::new(runtime))),
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

    pub fn transcription_graph_wired(&self) -> bool {
        self.runtime.is_some()
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
        let _ = request.max_tokens;
        let runtime = self.runtime.as_ref().ok_or(Error::Unsupported(
            "native Voxtral STT requires a Q4 GGUF model and tekken.json",
        ))?;
        let mut runtime = runtime
            .lock()
            .map_err(|_| Error::Runtime("native Voxtral STT runtime lock poisoned".to_string()))?;
        let text = runtime.transcribe_samples(wav.samples, wav.sample_rate as u32)?;
        Ok(TranscriptionResponse {
            model: self.config.model.clone(),
            text,
        })
    }
}

impl VoxtralQ4Runtime {
    fn load(gguf_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        let device = burn::backend::wgpu::WgpuDevice::default();
        let start = Instant::now();
        let mut loader = voxtral_mini_realtime::gguf::loader::Q4ModelLoader::from_file(gguf_path)?;
        let model = loader.load(&device)?;
        let tokenizer =
            voxtral_mini_realtime::tokenizer::VoxtralTokenizer::from_file(tokenizer_path)?;
        let elapsed = start.elapsed();
        eprintln!(
            "ctox_voxtral_stt: loaded Q4 GGUF from {} in {:.2}s",
            gguf_path.display(),
            elapsed.as_secs_f64()
        );
        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    fn transcribe_samples(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<String> {
        let mut audio = voxtral_mini_realtime::audio::AudioBuffer::new(samples, sample_rate);
        if audio.sample_rate != 16_000 {
            audio = voxtral_mini_realtime::audio::resample_to_16k(&audio)?;
        }
        audio.peak_normalize(0.95);

        let pad_config = voxtral_mini_realtime::audio::PadConfig::voxtral();
        let padded = voxtral_mini_realtime::audio::pad_audio(&audio, &pad_config);
        let mel_extractor = voxtral_mini_realtime::audio::MelSpectrogram::new(
            voxtral_mini_realtime::audio::MelConfig::voxtral(),
        );
        let mel = mel_extractor.compute_log(&padded.samples);
        let n_frames = mel.len();
        let n_mels = mel.first().map_or(0, Vec::len);
        if n_frames == 0 || n_mels == 0 {
            return Err(Error::InvalidFormat(
                "audio too short to produce mel frames",
            ));
        }

        let mut mel_transposed = vec![vec![0.0f32; n_frames]; n_mels];
        for (frame_idx, frame) in mel.iter().enumerate() {
            for (mel_idx, &value) in frame.iter().enumerate() {
                mel_transposed[mel_idx][frame_idx] = value;
            }
        }
        let mel_flat: Vec<f32> = mel_transposed.into_iter().flatten().collect();
        let mel_tensor = burn::tensor::Tensor::<WgpuBackend, 3>::from_data(
            burn::tensor::TensorData::new(mel_flat, [1, n_mels, n_frames]),
            &self.device,
        );

        let time_embed = voxtral_mini_realtime::models::time_embedding::TimeEmbedding::new(3072);
        let t_embed = time_embed.embed::<WgpuBackend>(6.0, &self.device);
        let generated = self.model.transcribe_streaming(mel_tensor, t_embed);
        let text_tokens = generated
            .iter()
            .filter(|&&token| token >= 1000)
            .map(|&token| token as u32)
            .collect::<Vec<_>>();
        let text = self.tokenizer.decode(&text_tokens)?;
        Ok(text.trim().to_string())
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
    let tokenizer_path = [root.join("tekken.json"), root.join("tokenizer.json")]
        .into_iter()
        .find(|path| path.is_file());
    Ok(VoxtralSttArtifactInspection {
        root,
        gguf_path,
        tokenizer_path,
        tensor_count: inspected.tensors.len(),
        architecture: inspected.architecture,
        required_tensors_present: missing_required_tensors.is_empty(),
        missing_required_tensors,
    })
}

pub fn required_tensors() -> Vec<&'static str> {
    let mut out = vec![
        "mm_streams_embeddings.embedding_module.whisper_encoder.conv_layers.0.conv.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.conv_layers.0.conv.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.conv_layers.1.conv.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.conv_layers.1.conv.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.norm.weight",
        "mm_streams_embeddings.embedding_module.audio_language_projection.0.weight",
        "mm_streams_embeddings.embedding_module.audio_language_projection.2.weight",
        "mm_streams_embeddings.embedding_module.tok_embeddings.weight",
        "norm.weight",
    ];
    out.extend([
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention_norm.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wq.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wq.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wk.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wv.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wv.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wo.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.attention.wo.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.ffn_norm.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.feed_forward.w1.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.feed_forward.w2.weight",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.feed_forward.w2.bias",
        "mm_streams_embeddings.embedding_module.whisper_encoder.transformer.layers.0.feed_forward.w3.weight",
        "layers.0.ada_rms_norm_t_cond.0.weight",
        "layers.0.ada_rms_norm_t_cond.2.weight",
        "layers.0.attention_norm.weight",
        "layers.0.attention.wq.weight",
        "layers.0.attention.wk.weight",
        "layers.0.attention.wv.weight",
        "layers.0.attention.wo.weight",
        "layers.0.ffn_norm.weight",
        "layers.0.feed_forward.w1.weight",
        "layers.0.feed_forward.w2.weight",
        "layers.0.feed_forward.w3.weight",
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
        assert!(err.to_string().contains("requires a Q4 GGUF"));
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
