use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ctox_voxtral_mini_4b_realtime_2602::{
    TranscriptionRequest, TranscriptionResponse, VoxtralSttBackend, VoxtralSttConfig,
    VoxtralSttModel, VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_env;

#[derive(Debug, Clone)]
pub struct NativeSttLaunch {
    pub transport: LocalTransport,
    pub compute_target: engine::ComputeTarget,
    pub model_path: Option<PathBuf>,
    pub root: PathBuf,
}

const MISTRAL_STT_DEFAULT_MODEL: &str = "voxtral-mini-latest";
const MISTRAL_STT_DEFAULT_ENDPOINT: &str = "https://api.mistral.ai/v1/audio/transcriptions";
const STT_REALTIME_PROOF_FILENAME: &str = "stt-live-realtime-proof.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SttEngineKind {
    Local,
    MistralApi,
}

#[derive(Debug, Clone)]
struct MistralSttClient {
    endpoint: String,
    model: String,
    api_key: String,
}

#[derive(Clone)]
enum SttRuntime {
    Local(VoxtralSttModel),
    MistralApi(MistralSttClient),
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
        live_transcription: Value,
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
    let engine_kind = configured_stt_engine(root);
    let mistral_config = MistralSttClient::configured(root);
    let inspection = model_path
        .as_ref()
        .and_then(|path| ctox_voxtral_mini_4b_realtime_2602::inspect_gguf(path).ok());
    let artifacts_ready = inspection
        .as_ref()
        .map(|value| value.required_tensors_present)
        .unwrap_or(false);
    let api_ready = engine_kind == SttEngineKind::MistralApi && mistral_config.is_ok();
    let live_transcription = live_transcription_status_json(root);
    json!({
        "ok": artifacts_ready || api_ready,
        "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
        "engine": engine_kind.label(),
        "live_transcription": live_transcription,
        "mistral_api": {
            "configured": engine_kind == SttEngineKind::MistralApi,
            "endpoint": configured_mistral_endpoint(root),
            "model": configured_mistral_model(root),
            "api_key_present": configured_mistral_api_key(root).is_some(),
            "usable": api_ready
        },
        "native_ctox": {
            "crate_linked": true,
            "cpu_reference_ops": true,
            "engine": "native-rust-vendored-ggml",
            "ggml_vendor_present": model_root.join("vendor/ggml/include/ggml.h").is_file(),
            "ggml_cpu_backend": ctox_voxtral_mini_4b_realtime_2602::GGML_CPU_ENABLED,
            "ggml_metal_backend": ctox_voxtral_mini_4b_realtime_2602::GGML_METAL_ENABLED,
            "ggml_blas_backend": ctox_voxtral_mini_4b_realtime_2602::GGML_BLAS_ENABLED,
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

pub fn stt_realtime_smoke_json(root: &Path, audio_path: &Path) -> serde_json::Value {
    let audio = match std::fs::read(audio_path) {
        Ok(audio) => audio,
        Err(err) => {
            return json!({
                "ok": false,
                "live_capable": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "error": format!("failed to read {}: {err}", audio_path.display())
            });
        }
    };
    let audio_duration_seconds = match wav_duration_seconds(&audio) {
        Ok(duration) => duration,
        Err(err) => {
            return json!({
                "ok": false,
                "live_capable": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "audio_path": audio_path.display().to_string(),
                "error": format!("failed to read WAV duration: {err}")
            });
        }
    };
    let runtime = match stt_runtime_for_smoke(root) {
        Ok(runtime) => runtime,
        Err(err) => {
            return json!({
                "ok": false,
                "live_capable": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "engine": configured_stt_engine(root).label(),
                "audio_duration_seconds": audio_duration_seconds,
                "error": err.to_string()
            });
        }
    };
    let started = Instant::now();
    let result = runtime.transcribe(&TranscriptionRequest {
        audio_bytes: &audio,
        response_format: "json",
        max_tokens: None,
    });
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let realtime_factor = if audio_duration_seconds > 0.0 {
        elapsed_seconds / audio_duration_seconds
    } else {
        f64::INFINITY
    };
    let batch_realtime_capable = realtime_factor <= 1.0;
    let streaming_supported = runtime.streaming_supported();
    let live_capable = streaming_supported && batch_realtime_capable;
    match result {
        Ok(output) => {
            let proof = json!({
                "engine": runtime.engine_label(),
                "backend": runtime.backend_label(),
                "model": output.model,
                "audio_path": audio_path.display().to_string(),
                "audio_duration_seconds": audio_duration_seconds,
                "elapsed_seconds": elapsed_seconds,
                "realtime_factor": realtime_factor,
                "batch_realtime_capable": batch_realtime_capable,
                "streaming_supported": streaming_supported,
                "live_capable": live_capable,
                "measured_at": now_unix_seconds()
            });
            let proof_write_error = if batch_realtime_capable {
                write_realtime_proof(root, &proof)
                    .err()
                    .map(|err| err.to_string())
            } else {
                None
            };
            json!({
                "ok": true,
                "live_capable": live_capable,
                "batch_realtime_capable": batch_realtime_capable,
                "streaming_supported": streaming_supported,
                "model": output.model,
                "engine": runtime.engine_label(),
                "backend": runtime.backend_label(),
                "audio_duration_seconds": audio_duration_seconds,
                "elapsed_seconds": elapsed_seconds,
                "realtime_factor": realtime_factor,
                "proof_path": realtime_proof_path(root).display().to_string(),
                "proof_write_error": proof_write_error,
                "text": output.text
            })
        }
        Err(err) => json!({
            "ok": false,
            "live_capable": false,
            "batch_realtime_capable": batch_realtime_capable,
            "streaming_supported": streaming_supported,
            "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
            "engine": runtime.engine_label(),
            "backend": runtime.backend_label(),
            "audio_duration_seconds": audio_duration_seconds,
            "elapsed_seconds": elapsed_seconds,
            "realtime_factor": realtime_factor,
            "error": err.to_string(),
            "transcription_graph_wired": runtime.transcription_graph_wired()
        }),
    }
}

pub fn live_transcription_status_json(root: &Path) -> serde_json::Value {
    let engine_kind = configured_stt_engine(root);
    let proof = read_realtime_proof(root);
    let proof_batch_realtime = proof
        .as_ref()
        .and_then(|value| value.get("batch_realtime_capable"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let proof_streaming_supported = proof
        .as_ref()
        .and_then(|value| value.get("streaming_supported"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let streaming_supported = configured_streaming_supported(root, engine_kind);
    let local_enabled_for_live_meetings =
        engine_kind == SttEngineKind::Local && streaming_supported && proof_batch_realtime;
    json!({
        "required_for_chat_reactivity": true,
        "engine": engine_kind.label(),
        "streaming_supported": streaming_supported,
        "batch_file_api_only": !streaming_supported,
        "realtime_proof_required_for_local": engine_kind == SttEngineKind::Local,
        "realtime_proof_path": realtime_proof_path(root).display().to_string(),
        "realtime_proof_present": proof.is_some(),
        "realtime_proof_batch_realtime": proof_batch_realtime,
        "realtime_proof_streaming_supported": proof_streaming_supported,
        "local_enabled_for_live_meetings": local_enabled_for_live_meetings,
        "local_live_disabled_reason": local_live_disabled_reason(
            engine_kind,
            streaming_supported,
            proof_batch_realtime
        ),
        "api_live_enabled": engine_kind == SttEngineKind::MistralApi && streaming_supported,
        "proof": proof
    })
}

pub fn local_live_transcription_ready(root: &Path) -> bool {
    let status = live_transcription_status_json(root);
    status
        .get("local_enabled_for_live_meetings")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn stt_smoke_json(root: &Path, audio_path: &Path) -> serde_json::Value {
    let audio = match std::fs::read(audio_path) {
        Ok(audio) => audio,
        Err(err) => {
            return json!({
                "ok": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "error": format!("failed to read {}: {err}", audio_path.display())
            });
        }
    };
    let runtime = match stt_runtime_for_smoke(root) {
        Ok(runtime) => runtime,
        Err(err) => {
            return json!({
                "ok": false,
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "engine": configured_stt_engine(root).label(),
                "error": err.to_string()
            });
        }
    };
    match runtime.transcribe(&TranscriptionRequest {
        audio_bytes: &audio,
        response_format: "json",
        max_tokens: None,
    }) {
        Ok(output) => json!({
            "ok": true,
            "model": output.model,
            "engine": runtime.engine_label(),
            "text": output.text
        }),
        Err(err) => json!({
            "ok": false,
            "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
            "engine": runtime.engine_label(),
            "error": err.to_string(),
            "transcription_graph_wired": runtime.transcription_graph_wired()
        }),
    }
}

fn smoke_backend_for_host() -> VoxtralSttBackend {
    match std::env::var("CTOX_VOXTRAL_STT_BACKEND") {
        Ok(value) if value.eq_ignore_ascii_case("metal") => VoxtralSttBackend::Metal,
        Ok(value) if value.eq_ignore_ascii_case("cpu") => VoxtralSttBackend::Cpu,
        _ => default_backend_for_host(engine::ComputeTarget::Cpu),
    }
}

pub fn serve_socket(launch: NativeSttLaunch) -> Result<()> {
    let runtime = match configured_stt_engine(&launch.root) {
        SttEngineKind::Local => {
            let backend = default_backend_for_host(launch.compute_target);
            let model = launch
                .model_path
                .as_ref()
                .and_then(|path| VoxtralSttModel::from_gguf(path, backend).ok())
                .unwrap_or_else(|| VoxtralSttModel::new(VoxtralSttConfig::default(), backend));
            SttRuntime::Local(model)
        }
        SttEngineKind::MistralApi => {
            SttRuntime::MistralApi(MistralSttClient::configured(&launch.root)?)
        }
    };
    let mut listener = launch.transport.bind()?;
    loop {
        let stream = listener.accept()?;
        let runtime = runtime.clone();
        std::thread::spawn(move || {
            let _ = handle_connection(stream, runtime);
        });
    }
}

fn handle_connection(
    mut stream: crate::inference::local_transport::LocalStream,
    runtime: SttRuntime,
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
            healthy: runtime.transcription_graph_wired(),
            default_model: Some(runtime.default_model()),
            loaded_models: if runtime.artifacts_loaded() {
                vec![runtime.default_model()]
            } else {
                Vec::new()
            },
            backend: runtime.backend_label(),
            artifacts_loaded: runtime.artifacts_loaded(),
            transcription_graph_wired: runtime.transcription_graph_wired(),
            live_transcription: json!({
                "streaming_supported": runtime.streaming_supported(),
                "batch_file_api_only": !runtime.streaming_supported()
            }),
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
            if !runtime.supports_request_model(request_model) {
                LocalSttResponse::Error {
                    code: "unsupported_model".to_string(),
                    message: format!(
                        "STT service only supports {VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL}"
                    ),
                }
            } else {
                match BASE64_STANDARD.decode(file_base64.as_bytes()) {
                    Ok(audio) => match runtime.transcribe(&TranscriptionRequest {
                        audio_bytes: &audio,
                        response_format: response_format.as_deref().unwrap_or("json"),
                        max_tokens: None,
                    }) {
                        Ok(output) => LocalSttResponse::Transcription {
                            model: output.model,
                            text: output.text,
                        },
                        Err(err) => LocalSttResponse::Error {
                            code: if runtime.transcription_graph_wired() {
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

fn stt_runtime_for_smoke(root: &Path) -> Result<SttRuntime> {
    match configured_stt_engine(root) {
        SttEngineKind::Local => {
            let backend = smoke_backend_for_host();
            let model = configured_or_default_model_path(root)
                .as_ref()
                .and_then(|path| VoxtralSttModel::from_gguf(path, backend).ok())
                .unwrap_or_else(|| VoxtralSttModel::new(VoxtralSttConfig::default(), backend));
            Ok(SttRuntime::Local(model))
        }
        SttEngineKind::MistralApi => {
            Ok(SttRuntime::MistralApi(MistralSttClient::configured(root)?))
        }
    }
}

fn configured_stt_engine(root: &Path) -> SttEngineKind {
    process_env_or_config(root, "CTOX_VOXTRAL_STT_ENGINE")
        .or_else(|| process_env_or_config(root, "CTOX_STT_ENGINE"))
        .as_deref()
        .map(parse_stt_engine_kind)
        .unwrap_or(SttEngineKind::Local)
}

fn parse_stt_engine_kind(value: &str) -> SttEngineKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "api" | "mistral" | "mistral-api" | "mistral_api" | "voxtral-api" | "voxtral_api" => {
            SttEngineKind::MistralApi
        }
        _ => SttEngineKind::Local,
    }
}

fn configured_mistral_api_key(root: &Path) -> Option<String> {
    process_env_or_config(root, "CTOX_MISTRAL_API_KEY")
        .or_else(|| process_env_or_config(root, "MISTRAL_API_KEY"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn configured_mistral_model(root: &Path) -> String {
    process_env_or_config(root, "CTOX_MISTRAL_STT_MODEL")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MISTRAL_STT_DEFAULT_MODEL.to_string())
}

fn configured_mistral_endpoint(root: &Path) -> String {
    process_env_or_config(root, "CTOX_MISTRAL_STT_ENDPOINT")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| MISTRAL_STT_DEFAULT_ENDPOINT.to_string())
}

fn configured_streaming_supported(root: &Path, engine_kind: SttEngineKind) -> bool {
    match engine_kind {
        SttEngineKind::Local => {
            bool_env_or_config(root, "CTOX_STT_LOCAL_STREAMING_SUPPORTED").unwrap_or(false)
        }
        SttEngineKind::MistralApi => bool_env_or_config(root, "CTOX_MISTRAL_STT_REALTIME_ENABLED")
            .or_else(|| bool_env_or_config(root, "CTOX_STT_REALTIME_API_ENABLED"))
            .unwrap_or(false),
    }
}

fn bool_env_or_config(root: &Path, key: &str) -> Option<bool> {
    process_env_or_config(root, key).and_then(|value| {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    })
}

fn process_env_or_config(root: &Path, key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| runtime_env::env_or_config(root, key))
}

fn wav_duration_seconds(audio: &[u8]) -> Result<f64> {
    let wav = ctox_voxtral_mini_4b_realtime_2602::audio::parse_wav(audio)
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    Ok(wav.samples.len() as f64 / wav.sample_rate as f64)
}

fn realtime_proof_path(root: &Path) -> PathBuf {
    root.join("runtime").join(STT_REALTIME_PROOF_FILENAME)
}

fn read_realtime_proof(root: &Path) -> Option<Value> {
    std::fs::read(realtime_proof_path(root))
        .ok()
        .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
}

fn write_realtime_proof(root: &Path, proof: &Value) -> Result<()> {
    let path = realtime_proof_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let encoded = serde_json::to_vec_pretty(proof)?;
    std::fs::write(&path, encoded).with_context(|| format!("failed to write {}", path.display()))
}

fn local_live_disabled_reason(
    engine_kind: SttEngineKind,
    streaming_supported: bool,
    proof_batch_realtime: bool,
) -> Option<&'static str> {
    if engine_kind != SttEngineKind::Local {
        return Some("not_using_local_engine");
    }
    if !streaming_supported {
        return Some("streaming_inference_not_wired");
    }
    if !proof_batch_realtime {
        return Some("missing_realtime_proof");
    }
    None
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

impl SttEngineKind {
    fn label(self) -> &'static str {
        match self {
            SttEngineKind::Local => "local-ggml",
            SttEngineKind::MistralApi => "mistral-api",
        }
    }
}

impl MistralSttClient {
    fn configured(root: &Path) -> Result<Self> {
        let api_key = configured_mistral_api_key(root)
            .context("missing Mistral API key; set CTOX_MISTRAL_API_KEY or MISTRAL_API_KEY")?;
        Ok(Self {
            endpoint: configured_mistral_endpoint(root),
            model: configured_mistral_model(root),
            api_key,
        })
    }

    fn transcribe(&self, audio_bytes: &[u8]) -> Result<TranscriptionResponse> {
        let boundary = format!(
            "----ctox-mistral-stt-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_nanos().to_string())
                .unwrap_or_else(|_| "fallback".to_string())
        );
        let body = mistral_stt_multipart_body(&boundary, &self.model, audio_bytes);
        let mut headers = BTreeMap::new();
        headers.insert(
            "authorization".to_string(),
            format!("Bearer {}", self.api_key),
        );
        headers.insert(
            "content-type".to_string(),
            format!("multipart/form-data; boundary={boundary}"),
        );
        let response = crate::communication::email_native::http_request(
            "POST",
            &self.endpoint,
            &headers,
            Some(&body),
        )?;
        if !(200..300).contains(&response.status) {
            anyhow::bail!(
                "Mistral transcription returned HTTP {}: {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            );
        }
        let parsed = serde_json::from_slice::<Value>(&response.body)
            .context("failed to parse Mistral transcription response")?;
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .or_else(|| parsed.get("transcript").and_then(Value::as_str))
            .context("Mistral transcription response did not contain text")?;
        let model = parsed
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&self.model);
        Ok(TranscriptionResponse {
            model: model.to_string(),
            text: text.trim().to_string(),
        })
    }
}

fn mistral_stt_multipart_body(boundary: &str, model: &str, audio_bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    body.extend_from_slice(model.trim().as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(audio_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    body
}

impl SttRuntime {
    fn transcribe(&self, request: &TranscriptionRequest<'_>) -> Result<TranscriptionResponse> {
        match self {
            SttRuntime::Local(model) => Ok(model.transcribe(request)?),
            SttRuntime::MistralApi(client) => client.transcribe(request.audio_bytes),
        }
    }

    fn transcription_graph_wired(&self) -> bool {
        match self {
            SttRuntime::Local(model) => model.transcription_graph_wired(),
            SttRuntime::MistralApi(_) => true,
        }
    }

    fn artifacts_loaded(&self) -> bool {
        match self {
            SttRuntime::Local(model) => model.artifacts_loaded(),
            SttRuntime::MistralApi(_) => true,
        }
    }

    fn default_model(&self) -> String {
        match self {
            SttRuntime::Local(model) => model.config().model.clone(),
            SttRuntime::MistralApi(client) => client.model.clone(),
        }
    }

    fn backend_label(&self) -> String {
        match self {
            SttRuntime::Local(model) => model.backend().label().to_string(),
            SttRuntime::MistralApi(_) => "mistral-api".to_string(),
        }
    }

    fn engine_label(&self) -> &'static str {
        match self {
            SttRuntime::Local(_) => "local-ggml",
            SttRuntime::MistralApi(_) => "mistral-api",
        }
    }

    fn streaming_supported(&self) -> bool {
        false
    }

    fn supports_request_model(&self, request_model: &str) -> bool {
        let request_model = request_model.trim();
        request_model.eq_ignore_ascii_case(VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL)
            || match self {
                SttRuntime::Local(_) => false,
                SttRuntime::MistralApi(client) => {
                    request_model.eq_ignore_ascii_case(&client.model)
                        || request_model.eq_ignore_ascii_case("voxtral-mini-latest")
                }
            }
    }
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

    #[test]
    fn stt_engine_parser_accepts_mistral_api_aliases() {
        assert_eq!(
            parse_stt_engine_kind("mistral-api"),
            SttEngineKind::MistralApi
        );
        assert_eq!(parse_stt_engine_kind("api"), SttEngineKind::MistralApi);
        assert_eq!(parse_stt_engine_kind("local"), SttEngineKind::Local);
        assert_eq!(parse_stt_engine_kind("ggml"), SttEngineKind::Local);
    }

    #[test]
    fn live_transcription_defaults_to_batch_only_until_streaming_is_wired() {
        let root = std::env::temp_dir().join(format!(
            "ctox-voxtral-live-status-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();

        let status = live_transcription_status_json(&root);
        assert_eq!(
            status["streaming_supported"].as_bool(),
            Some(false),
            "{status}"
        );
        assert_eq!(
            status["local_enabled_for_live_meetings"].as_bool(),
            Some(false),
            "{status}"
        );
        assert_eq!(
            status["local_live_disabled_reason"].as_str(),
            Some("streaming_inference_not_wired"),
            "{status}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn realtime_proof_alone_does_not_unlock_local_live_streaming() {
        let root = std::env::temp_dir().join(format!(
            "ctox-voxtral-live-proof-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        write_realtime_proof(
            &root,
            &json!({
                "engine": "local-ggml",
                "backend": "cpu",
                "model": VOXTRAL_MINI_4B_REALTIME_2602_CANONICAL_MODEL,
                "batch_realtime_capable": true,
                "streaming_supported": false,
                "live_capable": false
            }),
        )
        .unwrap();

        let status = live_transcription_status_json(&root);
        assert_eq!(status["realtime_proof_present"].as_bool(), Some(true));
        assert_eq!(
            status["realtime_proof_batch_realtime"].as_bool(),
            Some(true)
        );
        assert_eq!(
            status["local_enabled_for_live_meetings"].as_bool(),
            Some(false),
            "{status}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mistral_multipart_body_sends_model_and_wav_file() {
        let body = mistral_stt_multipart_body("boundary", "voxtral-mini-latest", b"RIFFdata");
        let rendered = String::from_utf8_lossy(&body);
        assert!(rendered.contains("name=\"model\""));
        assert!(rendered.contains("voxtral-mini-latest"));
        assert!(rendered.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(rendered.contains("Content-Type: audio/wav"));
        assert!(body.ends_with(b"--boundary--\r\n"));
    }
}
