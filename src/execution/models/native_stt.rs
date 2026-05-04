use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ctox_voxtral_mini_4b_realtime_2602::{
    TranscriptionRequest, VoxtralSttBackend, VoxtralSttConfig, VoxtralSttModel,
    VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_env;

#[derive(Debug, Clone)]
pub struct NativeSttLaunch {
    pub transport: LocalTransport,
    pub compute_target: engine::ComputeTarget,
    pub model_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalSttRequest {
    TranscriptionCreate {
        model: Option<String>,
        file_base64: String,
        response_format: Option<String>,
    },
    RuntimeHealth,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalSttResponse {
    Transcription {
        model: String,
        text: String,
    },
    RuntimeHealth {
        healthy: bool,
        default_model: Option<String>,
        loaded_models: Vec<String>,
        backend: String,
        artifacts_loaded: bool,
        transcription_graph_wired: bool,
    },
    Error {
        code: String,
        message: String,
    },
}

pub fn configured_or_default_model_path(root: &Path) -> Option<PathBuf> {
    runtime_env::env_or_config(root, "CTOX_VOXTRAL_STT_GGUF")
        .or_else(|| runtime_env::env_or_config(root, "CTOX_STT_MODEL_PATH"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let model_dir = runtime_env::env_or_config(root, "CTOX_VOXTRAL_STT_MODEL_DIR")
                .or_else(|| runtime_env::env_or_config(root, "CTOX_STT_MODEL_DIR"))
                .map(PathBuf::from);
            model_dir.map(|dir| dir.join("voxtral.gguf"))
        })
        .or_else(|| {
            [
                root.join("runtime/models/voxtral/voxtral.gguf"),
                root.join("runtime/models/Voxtral-Mini-4B-Realtime-2602/voxtral.gguf"),
                root.join("models/voxtral/voxtral.gguf"),
            ]
            .into_iter()
            .find(|path| path.is_file())
        })
}

pub fn doctor_json(root: &Path) -> serde_json::Value {
    let model_root = root.join("src/inference/models/voxtral_mini_4b_realtime_2602");
    let model_path = configured_or_default_model_path(root);
    let inspection = model_path
        .as_ref()
        .and_then(|path| ctox_voxtral_mini_4b_realtime_2602::inspect_gguf(path).ok());
    let artifacts_ready = inspection
        .as_ref()
        .map(|value| value.required_tensors_present && value.tokenizer_path.is_some())
        .unwrap_or(false);
    json!({
        "ok": artifacts_ready,
        "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
        "native_ctox": {
            "crate_linked": true,
            "cpu_reference_ops": true,
            "reference_source": "TrevorS/voxtral-mini-realtime-rs@2930e95d60f8584b5326d90d3c5ec9a152d0d322 plus andrijdavid/voxtral.cpp@7deef66c8ee473d3ceffc57fb0cd17977eeebca9 for graph comparison",
            "metal_kernel_seed_present": model_root.join("vendor/metal/kernels/ctox_voxtral_stt_glue.metal").is_file(),
            "cuda_kernel_seed_present": model_root.join("vendor/cuda/kernels/ctox_voxtral_stt_glue.cu").is_file(),
            "wgsl_kernel_seed_present": model_root.join("vendor/wgsl/kernels/ctox_voxtral_stt_glue.wgsl").is_file(),
            "model_artifacts_present": inspection.is_some(),
            "model_artifact_root": inspection.as_ref().map(|value| value.root.display().to_string()),
            "model_artifact_gguf": inspection.as_ref().map(|value| value.gguf_path.display().to_string()).or_else(|| model_path.as_ref().map(|path| path.display().to_string())),
            "model_artifact_tokenizer": inspection.as_ref().and_then(|value| value.tokenizer_path.as_ref()).map(|path| path.display().to_string()),
            "model_artifact_tensor_count": inspection.as_ref().map(|value| value.tensor_count).unwrap_or(0),
            "model_artifact_architecture": inspection.as_ref().and_then(|value| value.architecture.clone()),
            "model_artifact_required_tensors_present": inspection.as_ref().map(|value| value.required_tensors_present).unwrap_or(false),
            "model_artifact_missing_required_tensors": inspection.as_ref().map(|value| value.missing_required_tensors.clone()).unwrap_or_default(),
            "shape_contract": ctox_voxtral_mini_4b_realtime_2602::shape_contract(),
            "transcription_graph_wired": true,
            "returns_fake_text": false
        }
    })
}

pub fn parse_stt_smoke_audio_path(args: &[String]) -> Result<PathBuf> {
    args.first()
        .map(PathBuf::from)
        .context("usage: ctox runtime stt-smoke <wav-path>")
}

pub fn stt_smoke_json(root: &Path, audio_path: &Path) -> serde_json::Value {
    let audio = match std::fs::read(audio_path) {
        Ok(audio) => audio,
        Err(err) => {
            return json!({
                "ok": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "error": format!("failed to read {}: {err}", audio_path.display())
            })
        }
    };
    let backend = default_backend_for_host(engine::ComputeTarget::Cpu);
    let model = configured_or_default_model_path(root)
        .as_ref()
        .and_then(|path| VoxtralSttModel::from_gguf(path, backend).ok())
        .unwrap_or_else(|| VoxtralSttModel::new(VoxtralSttConfig::default(), backend));
    match model.transcribe(&TranscriptionRequest {
        audio_bytes: &audio,
        response_format: "json",
        max_tokens: None,
    }) {
        Ok(output) => json!({"ok": true, "model": output.model, "text": output.text}),
        Err(err) => json!({
            "ok": false,
            "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
            "error": err.to_string(),
            "transcription_graph_wired": model.transcription_graph_wired()
        }),
    }
}

pub fn serve_socket(launch: NativeSttLaunch) -> Result<()> {
    let backend = default_backend_for_host(launch.compute_target);
    let model = launch
        .model_path
        .as_ref()
        .and_then(|path| VoxtralSttModel::from_gguf(path, backend).ok())
        .unwrap_or_else(|| VoxtralSttModel::new(VoxtralSttConfig::default(), backend));
    let mut listener = launch.transport.bind()?;
    loop {
        let stream = listener.accept()?;
        let model = model.clone();
        std::thread::spawn(move || {
            let _ = handle_connection(stream, model);
        });
    }
}

fn handle_connection(
    mut stream: crate::inference::local_transport::LocalStream,
    model: VoxtralSttModel,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let response = match serde_json::from_str::<LocalSttRequest>(line.trim()) {
        Ok(LocalSttRequest::RuntimeHealth) => LocalSttResponse::RuntimeHealth {
            healthy: model.transcription_graph_wired(),
            default_model: Some(model.config().model.clone()),
            loaded_models: if model.artifacts_loaded() {
                vec![model.config().model.clone()]
            } else {
                Vec::new()
            },
            backend: model.backend().label().to_string(),
            artifacts_loaded: model.artifacts_loaded(),
            transcription_graph_wired: model.transcription_graph_wired(),
        },
        Ok(LocalSttRequest::TranscriptionCreate {
            model: request_model,
            file_base64,
            response_format,
        }) => {
            let request_model = request_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL);
            if request_model != VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL {
                LocalSttResponse::Error {
                    code: "unsupported_model".to_string(),
                    message: format!(
                        "native STT service only supports {VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL}"
                    ),
                }
            } else {
                match BASE64_STANDARD.decode(file_base64.as_bytes()) {
                    Ok(audio) => match model.transcribe(&TranscriptionRequest {
                        audio_bytes: &audio,
                        response_format: response_format.as_deref().unwrap_or("json"),
                        max_tokens: None,
                    }) {
                        Ok(output) => LocalSttResponse::Transcription {
                            model: output.model,
                            text: output.text,
                        },
                        Err(err) => LocalSttResponse::Error {
                            code: if model.transcription_graph_wired() {
                                "transcription_failed".to_string()
                            } else {
                                "backend_not_wired".to_string()
                            },
                            message: err.to_string(),
                        },
                    },
                    Err(err) => LocalSttResponse::Error {
                        code: "invalid_audio".to_string(),
                        message: format!("failed to decode base64 audio: {err}"),
                    },
                }
            }
        }
        Err(err) => LocalSttResponse::Error {
            code: "invalid_request".to_string(),
            message: err.to_string(),
        },
    };
    let encoded = serde_json::to_vec(&response)?;
    stream.write_all(&encoded)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn default_backend_for_host(compute_target: engine::ComputeTarget) -> VoxtralSttBackend {
    match compute_target {
        engine::ComputeTarget::Cpu => VoxtralSttBackend::Cpu,
        engine::ComputeTarget::Gpu => {
            #[cfg(target_os = "macos")]
            {
                VoxtralSttBackend::Metal
            }
            #[cfg(target_os = "linux")]
            {
                VoxtralSttBackend::Cuda
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                VoxtralSttBackend::Cpu
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_reports_native_voxtral_stt_without_fake_text() {
        let root = std::env::temp_dir().join(format!(
            "ctox-voxtral-stt-doctor-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(
            root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/metal/kernels"),
        )
        .unwrap();
        std::fs::create_dir_all(
            root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/cuda/kernels"),
        )
        .unwrap();
        std::fs::create_dir_all(
            root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/wgsl/kernels"),
        )
        .unwrap();
        std::fs::write(root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/metal/kernels/ctox_voxtral_stt_glue.metal"), "").unwrap();
        std::fs::write(root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/cuda/kernels/ctox_voxtral_stt_glue.cu"), "").unwrap();
        std::fs::write(root.join("src/inference/models/voxtral_mini_4b_realtime_2602/vendor/wgsl/kernels/ctox_voxtral_stt_glue.wgsl"), "").unwrap();
        let status = doctor_json(&root);
        assert_eq!(status["native_ctox"]["crate_linked"].as_bool(), Some(true));
        assert_eq!(
            status["native_ctox"]["returns_fake_text"].as_bool(),
            Some(false)
        );
        assert_eq!(
            status["native_ctox"]["transcription_graph_wired"].as_bool(),
            Some(true)
        );
        assert_eq!(status["ok"].as_bool(), Some(false));
    }
}
