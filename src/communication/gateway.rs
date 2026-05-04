use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_env;
use crate::inference::runtime_kernel;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommunicationAdapterBackend {
    NativeRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommunicationAdapterKind {
    Email,
    Jami,
    Meeting,
    Teams,
    Whatsapp,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommunicationAdapterSpec {
    pub kind: CommunicationAdapterKind,
    pub backend: CommunicationAdapterBackend,
    pub runtime_env_keys: &'static [&'static str],
}

const EMAIL_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_EMAIL_ADDRESS",
    "CTO_EMAIL_PROVIDER",
    "CTO_EMAIL_PASSWORD",
    "CTO_EMAIL_GRAPH_ACCESS_TOKEN",
    "CTO_EMAIL_GRAPH_BASE_URL",
    "CTO_EMAIL_GRAPH_USER",
    "CTO_EMAIL_EWS_URL",
    "CTO_EMAIL_OWA_URL",
    "CTO_EMAIL_EWS_VERSION",
    "CTO_EMAIL_EWS_AUTH_TYPE",
    "CTO_EMAIL_EWS_USERNAME",
    "CTO_EMAIL_EWS_BEARER_TOKEN",
    "CTO_EMAIL_ACTIVESYNC_SERVER",
    "CTO_EMAIL_ACTIVESYNC_USERNAME",
    "CTO_EMAIL_ACTIVESYNC_PATH",
    "CTO_EMAIL_ACTIVESYNC_DEVICE_ID",
    "CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE",
    "CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION",
    "CTO_EMAIL_ACTIVESYNC_POLICY_KEY",
    "CTO_EMAIL_VERIFY_SEND",
    "CTO_EMAIL_SENT_VERIFY_WINDOW_SECONDS",
];

const TEAMS_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_TEAMS_USERNAME",
    "CTO_TEAMS_PASSWORD",
    "CTO_TEAMS_TENANT_ID",
    "CTO_TEAMS_CLIENT_ID",
    "CTO_TEAMS_CLIENT_SECRET",
    "CTO_TEAMS_BOT_ID",
    "CTO_TEAMS_GRAPH_BASE_URL",
    "CTO_TEAMS_TEAM_ID",
    "CTO_TEAMS_CHANNEL_ID",
    "CTO_TEAMS_CHAT_ID",
];

const MEETING_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_MEETING_BOT_NAME",
    "CTO_MEETING_AUTO_JOIN_ENABLED",
    "CTO_MEETING_ALLOWED_INVITE_SENDERS",
    "CTO_MEETING_MAX_DURATION_MINUTES",
    "CTO_MEETING_AUDIO_CHUNK_SECONDS",
    "CTOX_STT_MODEL",
    "CTOX_TTS_MODEL",
    "CTO_MEETING_TTS_VOICE",
];

const JAMI_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTOX_STT_MODEL",
    "CTOX_TTS_MODEL",
    "CTO_JAMI_TTS_VOICE",
    "CTO_JAMI_DBUS_ENV_FILE",
    "CTO_JAMI_INBOX_DIR",
    "CTO_JAMI_OUTBOX_DIR",
    "CTO_JAMI_ARCHIVE_DIR",
    "CTO_JAMI_PROFILE_NAME",
    "CTO_JAMI_ACCOUNT_ID",
];

const WHATSAPP_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_WHATSAPP_DEVICE_DB",
    "CTO_WHATSAPP_PUSH_NAME",
    "CTO_WHATSAPP_PAIR_TIMEOUT_SECONDS",
    "CTO_WHATSAPP_SYNC_TIMEOUT_SECONDS",
    "CTO_WHATSAPP_HISTORY_SYNC_ENABLED",
];

impl CommunicationAdapterKind {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spec(self) -> CommunicationAdapterSpec {
        match self {
            Self::Email => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: EMAIL_RUNTIME_ENV_KEYS,
            },
            Self::Jami => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: JAMI_RUNTIME_ENV_KEYS,
            },
            Self::Meeting => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: MEETING_RUNTIME_ENV_KEYS,
            },
            Self::Teams => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: TEAMS_RUNTIME_ENV_KEYS,
            },
            Self::Whatsapp => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: WHATSAPP_RUNTIME_ENV_KEYS,
            },
        }
    }
}

pub(crate) fn runtime_settings_from_root(
    root: &Path,
    kind: CommunicationAdapterKind,
) -> BTreeMap<String, String> {
    let mut settings = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    apply_kernel_runtime_settings(kind, root, &mut settings);
    settings
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn runtime_settings_from_settings(
    root: &Path,
    kind: CommunicationAdapterKind,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    merged.extend(settings.clone());
    apply_kernel_runtime_settings(kind, root, &mut merged);
    merged
}

fn apply_kernel_runtime_settings(
    kind: CommunicationAdapterKind,
    root: &Path,
    settings: &mut BTreeMap<String, String>,
) {
    if !matches!(
        kind,
        CommunicationAdapterKind::Jami | CommunicationAdapterKind::Meeting
    ) {
        return;
    }
    let Ok(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root) else {
        return;
    };
    if let Some(model) = resolved
        .gateway
        .transcription_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.insert("CTOX_STT_MODEL".to_string(), model.to_string());
    }
    if let Some(model) = resolved
        .gateway
        .speech_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.insert("CTOX_TTS_MODEL".to_string(), model.to_string());
    }
}

const AUXILIARY_IPC_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn transcription_backend_reachable(root: &Path) -> bool {
    let Ok(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root) else {
        return false;
    };
    let Some(binding) = resolved.binding_for_auxiliary_role(engine::AuxiliaryRole::Stt) else {
        return false;
    };
    binding.transport.is_private_ipc()
        && auxiliary_runtime_health(&binding.transport)
            .unwrap_or_else(|_| binding.transport.probe())
}

pub(crate) fn transcribe_audio_file(root: &Path, audio_path: &Path, model: &str) -> Result<String> {
    let file_bytes = fs::read(audio_path)
        .with_context(|| format!("failed to read audio attachment {}", audio_path.display()))?;
    let resolved = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for transcription")?;
    if let Some(binding) = resolved.binding_for_auxiliary_role(engine::AuxiliaryRole::Stt) {
        if !binding.transport.is_private_ipc() {
            anyhow::bail!(
                "ctox_core_local requires private IPC for local transcription inference; loopback HTTP transport is not allowed"
            );
        }
        return transcribe_via_local_ipc(&binding.transport, &file_bytes, model).with_context(
            || {
                format!(
                    "failed to reach transcription transport {}",
                    binding.transport.display_label()
                )
            },
        );
    }
    let base_url = resolved
        .auxiliary_base_url(engine::AuxiliaryRole::Stt)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("transcription runtime is not resolved"))?;
    transcribe_via_http(&base_url, audio_path, &file_bytes, model)
}

pub(crate) fn synthesize_speech(
    root: &Path,
    input: &str,
    model: &str,
    voice: &str,
    response_format: &str,
) -> Result<Vec<u8>> {
    let resolved = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .context("failed to resolve runtime kernel for speech synthesis")?;
    if let Some(binding) = resolved.binding_for_auxiliary_role(engine::AuxiliaryRole::Tts) {
        if !binding.transport.is_private_ipc() {
            anyhow::bail!(
                "ctox_core_local requires private IPC for local speech inference; loopback HTTP transport is not allowed"
            );
        }
        return synthesize_via_local_ipc(&binding.transport, input, model, voice, response_format)
            .with_context(|| {
                format!(
                    "failed to reach speech transport {}",
                    binding.transport.display_label()
                )
            });
    }
    let base_url = resolved
        .auxiliary_base_url(engine::AuxiliaryRole::Tts)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("speech runtime is not resolved"))?;
    synthesize_via_http(&base_url, input, model, voice, response_format)
}

fn transcribe_via_local_ipc(
    transport: &LocalTransport,
    file_bytes: &[u8],
    model: &str,
) -> Result<String> {
    let mut request = json!({
        "kind": "transcription_create",
        "file_base64": BASE64_STANDARD.encode(file_bytes),
        "response_format": "json",
    });
    if !model.trim().is_empty() {
        request["model"] = Value::String(model.trim().to_string());
    }
    let response = local_ipc_roundtrip(transport, &request)?;
    parse_transcription_text(&response)
}

fn synthesize_via_local_ipc(
    transport: &LocalTransport,
    input: &str,
    model: &str,
    voice: &str,
    response_format: &str,
) -> Result<Vec<u8>> {
    let mut request = json!({
        "kind": "speech_create",
        "input": input,
        "response_format": response_format,
    });
    if !model.trim().is_empty() {
        request["model"] = Value::String(model.trim().to_string());
    }
    if !voice.trim().is_empty() {
        request["voice"] = Value::String(voice.trim().to_string());
    }
    let response = local_ipc_roundtrip(transport, &request)?;
    parse_speech_audio(&response)
}

fn transcribe_via_http(
    base_url: &str,
    audio_path: &Path,
    file_bytes: &[u8],
    model: &str,
) -> Result<String> {
    let boundary = format!(
        "----ctox-transcription-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_millis().to_string())
            .unwrap_or_else(|_| "fallback".to_string())
    );
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            audio_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("audio.wav")
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(b"\r\n");
    if !model.trim().is_empty() {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(model.trim().as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type".to_string(),
        format!("multipart/form-data; boundary={boundary}"),
    );
    let response = crate::communication::email_native::http_request(
        "POST",
        &format!("{}/v1/audio/transcriptions", base_url.trim_end_matches('/')),
        &headers,
        Some(&body),
    )?;
    if !(200..300).contains(&response.status) {
        bail!(
            "audio transcription returned HTTP {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        );
    }
    let parsed = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    Ok(parsed
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| parsed.get("transcript").and_then(Value::as_str))
        .unwrap_or("")
        .trim()
        .to_string())
}

fn synthesize_via_http(
    base_url: &str,
    input: &str,
    model: &str,
    voice: &str,
    response_format: &str,
) -> Result<Vec<u8>> {
    let mut payload = json!({
        "input": input,
        "response_format": response_format,
    });
    if !model.trim().is_empty() {
        payload["model"] = Value::String(model.trim().to_string());
    }
    if !voice.trim().is_empty() {
        payload["voice"] = Value::String(voice.trim().to_string());
    }
    let body = serde_json::to_vec(&payload).context("failed to encode speech request")?;
    let mut headers = BTreeMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    let response = crate::communication::email_native::http_request(
        "POST",
        &format!("{}/v1/audio/speech", base_url.trim_end_matches('/')),
        &headers,
        Some(&body),
    )?;
    if !(200..300).contains(&response.status) {
        bail!(
            "voice synthesis returned HTTP {}: {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        );
    }
    Ok(response.body)
}

fn local_ipc_roundtrip(transport: &LocalTransport, request: &Value) -> Result<Value> {
    let mut stream = transport
        .connect_blocking(AUXILIARY_IPC_TIMEOUT)
        .with_context(|| format!("failed to connect via {}", transport.display_label()))?;
    let mut payload = serde_json::to_vec(request).context("failed to encode IPC request")?;
    payload.push(b'\n');
    stream
        .write_all(&payload)
        .with_context(|| format!("failed to write via {}", transport.display_label()))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush via {}", transport.display_label()))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read via {}", transport.display_label()))?;
    if line.trim().is_empty() {
        bail!("local IPC transport returned an empty response");
    }
    serde_json::from_str(line.trim()).context("failed to parse local IPC response")
}

fn auxiliary_runtime_health(transport: &LocalTransport) -> Result<bool> {
    let response = local_ipc_roundtrip(transport, &json!({ "kind": "runtime_health" }))?;
    if response.get("kind").and_then(Value::as_str) == Some("runtime_health") {
        return Ok(response
            .get("healthy")
            .and_then(Value::as_bool)
            .unwrap_or(false));
    }
    Ok(false)
}

fn parse_transcription_text(response: &Value) -> Result<String> {
    match response.get("kind").and_then(Value::as_str) {
        Some("transcription") => Ok(response
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()),
        Some("error") => bail!(
            "{}: {}",
            response
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("transcription_error"),
            response
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown transcription error")
        ),
        other => bail!(
            "unexpected transcription response kind: {}",
            other.unwrap_or("unknown")
        ),
    }
}

fn parse_speech_audio(response: &Value) -> Result<Vec<u8>> {
    match response.get("kind").and_then(Value::as_str) {
        Some("speech") => BASE64_STANDARD
            .decode(
                response
                    .get("audio_base64")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .context("failed to decode speech audio"),
        Some("error") => bail!(
            "{}: {}",
            response
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("speech_error"),
            response
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown speech error")
        ),
        other => bail!(
            "unexpected speech response kind: {}",
            other.unwrap_or("unknown")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        runtime_settings_from_settings, CommunicationAdapterBackend, CommunicationAdapterKind,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn communication_adapter_specs_use_native_backends() {
        assert_eq!(
            CommunicationAdapterKind::Email.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Jami.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Meeting.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Teams.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Whatsapp.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
    }

    #[test]
    fn runtime_settings_preserve_explicit_email_configuration() {
        let root = PathBuf::from("/tmp/ctox-root");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTO_EMAIL_ADDRESS".to_string(),
            "owner@example.com".to_string(),
        );
        let merged =
            runtime_settings_from_settings(&root, CommunicationAdapterKind::Email, &settings);
        assert_eq!(
            merged.get("CTO_EMAIL_ADDRESS").map(String::as_str),
            Some("owner@example.com")
        );
    }
}
