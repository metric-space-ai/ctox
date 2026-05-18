use anyhow::{Context, Result};
use ctox_voxtral_4b_tts_2603::{
    speech, SpeechRequest, VoxtralTtsBackend, VoxtralTtsConfig, VoxtralTtsModel,
    VOXTRAL_4B_TTS_2603_CANONICAL_MODEL,
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
pub struct NativeTtsLaunch {
    pub transport: LocalTransport,
    pub compute_target: engine::ComputeTarget,
    pub model_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalTtsRequest {
    SpeechCreate {
        model: Option<String>,
        input: String,
        voice: Option<String>,
        response_format: Option<String>,
    },
    RuntimeHealth,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LocalTtsResponse {
    Speech {
        model: String,
        audio_base64: String,
        response_format: String,
    },
    RuntimeHealth {
        healthy: bool,
        default_model: Option<String>,
        loaded_models: Vec<String>,
        backend: String,
        artifacts_loaded: bool,
        speech_synthesis_wired: bool,
    },
    Error {
        code: String,
        message: String,
    },
}

pub fn doctor_json(root: &Path) -> serde_json::Value {
    let model_root = root.join("src/core/inference/models/voxtral_4b_tts_2603");
    let model_dir = configured_or_default_model_dir(root);
    let inspection = model_dir
        .as_ref()
        .and_then(|dir| speech::inspect_model_dir(dir).ok());
    json!({
        "ok": false,
        "model": VOXTRAL_4B_TTS_2603_CANONICAL_MODEL,
        "native_ctox": {
            "crate_linked": true,
            "cpu_reference_ops": true,
            "metal_kernel_seed_present": model_root.join("vendor/metal/kernels/ctox_voxtral_tts_glue.metal").is_file(),
            "cuda_kernel_seed_present": model_root.join("vendor/cuda/kernels/ctox_voxtral_tts_glue.cu").is_file(),
            "wgsl_kernel_seed_present": model_root.join("vendor/wgsl/kernels/ctox_voxtral_tts_glue.wgsl").is_file(),
            "model_artifacts_present": inspection.is_some(),
            "model_artifact_root": inspection.as_ref().map(|value| value.root.display().to_string()),
            "model_artifact_weights": inspection.as_ref().map(|value| value.weights_path.display().to_string()),
            "model_artifact_tensor_count": inspection.as_ref().map(|value| value.tensor_count).unwrap_or(0),
            "model_artifact_required_tensors_present": inspection.as_ref().map(|value| value.required_tensors_present).unwrap_or(false),
            "model_artifact_missing_required_tensors": inspection.as_ref().map(|value| value.missing_required_tensors.clone()).unwrap_or_default(),
            "speech_synthesis_wired": false,
            "text_to_audio_graph_wired": false,
            "returns_fake_audio": false
        }
    })
}

pub fn parse_tts_smoke_text(args: &[String]) -> Result<String> {
    if let Some(index) = args.iter().position(|arg| arg == "--text") {
        return args
            .get(index + 1)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .context("usage: ctox runtime tts-smoke [--text <text>]");
    }
    Ok("CTOX Meeting TTS smoke test.".to_string())
}

pub fn tts_smoke_json(root: &Path, text: &str) -> serde_json::Value {
    let backend = default_backend_for_host(engine::ComputeTarget::Cpu);
    let model = configured_or_default_model_dir(root)
        .as_ref()
        .and_then(|dir| VoxtralTtsModel::from_model_dir(dir, backend).ok())
        .unwrap_or_else(|| VoxtralTtsModel::new(VoxtralTtsConfig::default(), backend));
    match model.synthesize(&SpeechRequest {
        input: text,
        voice: None,
        response_format: "wav",
    }) {
        Ok(output) => json!({
            "ok": true,
            "model": output.model,
            "response_format": output.response_format,
            "audio_bytes": output.audio.len()
        }),
        Err(err) => json!({
            "ok": false,
            "model": VOXTRAL_4B_TTS_2603_CANONICAL_MODEL,
            "error": err.to_string(),
            "speech_synthesis_wired": false
        }),
    }
}

pub fn serve_socket(launch: NativeTtsLaunch) -> Result<()> {
    let backend = default_backend_for_host(launch.compute_target);
    let model = launch
        .model_dir
        .as_ref()
        .and_then(|dir| VoxtralTtsModel::from_model_dir(dir, backend).ok())
        .unwrap_or_else(|| VoxtralTtsModel::new(VoxtralTtsConfig::default(), backend));
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
    model: VoxtralTtsModel,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let response = match serde_json::from_str::<LocalTtsRequest>(line.trim()) {
        Ok(LocalTtsRequest::RuntimeHealth) => LocalTtsResponse::RuntimeHealth {
            healthy: false,
            default_model: Some(model.config().model.clone()),
            loaded_models: if model.artifacts_loaded() {
                vec![model.config().model.clone()]
            } else {
                Vec::new()
            },
            backend: model.backend().label().to_string(),
            artifacts_loaded: model.artifacts_loaded(),
            speech_synthesis_wired: false,
        },
        Ok(LocalTtsRequest::SpeechCreate {
            model: request_model,
            input,
            voice,
            response_format,
        }) => {
            let request_model = request_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(VOXTRAL_4B_TTS_2603_CANONICAL_MODEL);
            if request_model != VOXTRAL_4B_TTS_2603_CANONICAL_MODEL {
                LocalTtsResponse::Error {
                    code: "unsupported_model".to_string(),
                    message: format!(
                        "native TTS service only supports {VOXTRAL_4B_TTS_2603_CANONICAL_MODEL}"
                    ),
                }
            } else {
                match model.synthesize(&SpeechRequest {
                    input: &input,
                    voice: voice.as_deref(),
                    response_format: response_format.as_deref().unwrap_or("wav"),
                }) {
                    Ok(output) => LocalTtsResponse::Speech {
                        model: output.model,
                        audio_base64: base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            output.audio,
                        ),
                        response_format: output.response_format,
                    },
                    Err(err) => LocalTtsResponse::Error {
                        code: "backend_not_wired".to_string(),
                        message: err.to_string(),
                    },
                }
            }
        }
        Err(err) => LocalTtsResponse::Error {
            code: "invalid_request".to_string(),
            message: err.to_string(),
        },
    };
    let mut payload = serde_json::to_vec(&response)?;
    payload.push(b'\n');
    stream.write_all(&payload)?;
    stream.flush()?;
    Ok(())
}

pub fn configured_or_default_model_dir(root: &Path) -> Option<PathBuf> {
    configured_model_dir(root).or_else(|| {
        default_model_dirs(root)
            .into_iter()
            .find(|dir| dir.join("consolidated.safetensors").is_file())
    })
}

pub fn configured_model_dir(root: &Path) -> Option<PathBuf> {
    [
        "CTOX_VOXTRAL_TTS_MODEL_DIR",
        "CTOX_TTS_MODEL_DIR",
        "CTOX_SPEECH_MODEL_DIR",
    ]
    .into_iter()
    .find_map(|key| runtime_env::env_or_config(root, key))
    .map(|value| PathBuf::from(value.trim()))
    .filter(|path| !path.as_os_str().is_empty())
}

fn default_model_dirs(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("runtime/models/Voxtral-4B-TTS-2603"),
        root.join("runtime/models/engineai--Voxtral-4B-TTS-2603"),
        root.join("models/Voxtral-4B-TTS-2603"),
        root.join("models/engineai--Voxtral-4B-TTS-2603"),
    ]
}

fn default_backend_for_host(compute_target: engine::ComputeTarget) -> VoxtralTtsBackend {
    match compute_target {
        engine::ComputeTarget::Cpu => VoxtralTtsBackend::Cpu,
        engine::ComputeTarget::Gpu => {
            #[cfg(target_os = "macos")]
            {
                VoxtralTtsBackend::Metal
            }
            #[cfg(target_os = "linux")]
            {
                VoxtralTtsBackend::Cuda
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            {
                VoxtralTtsBackend::Cpu
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_reports_native_voxtral_without_fake_audio() {
        let root = std::env::temp_dir().join(format!(
            "ctox-voxtral-tts-doctor-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let status = doctor_json(&root);
        assert_eq!(
            status["model"].as_str(),
            Some(VOXTRAL_4B_TTS_2603_CANONICAL_MODEL)
        );
        assert_eq!(
            status["native_ctox"]["speech_synthesis_wired"].as_bool(),
            Some(false)
        );
        assert_eq!(
            status["native_ctox"]["returns_fake_audio"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn tts_smoke_fails_explicitly_until_graph_is_wired() {
        let root = std::env::temp_dir().join(format!(
            "ctox-voxtral-tts-smoke-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let status = tts_smoke_json(&root, "Hallo.");
        assert_eq!(status["ok"].as_bool(), Some(false));
        assert!(status["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not wired"));
    }
}
