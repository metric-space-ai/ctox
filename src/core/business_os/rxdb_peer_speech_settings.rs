// Origin: CTOX
// License: AGPL-3.0-only
//! Transient Owner/Admin speech control over the authenticated native DataChannel.
//! Credential inputs never become a business command, projection, response or log.
use super::store;
use crate::execution::speech::{SpeechAudioFormat, SpeechError, SpeechGateway, SpeechRequest, SpeechRuntimeConfig};
use anyhow::{ensure, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use rxdb::plugins::replication_webrtc::{index_mod::GuardedAuxiliaryResponse, WebRTCPublicationGuard};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, path::{Path, PathBuf}, sync::{Arc, atomic::Ordering}, time::Duration};
use zeroize::Zeroizing;

pub(super) const METHOD: &str = "ctox.workjet.speech.settings.v1";
pub(super) const CAPABILITY: &str = "ctox-workjet-speech-settings-v1";
const CHECK_KEY: &str = "speech_gateway_checks";

#[derive(Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
enum Request {
    #[serde(rename = "speech.settings.read")]
    Read { #[serde(rename = "commandId")] command_id: String },
    #[serde(rename = "speech.settings.configure")]
    Configure { #[serde(rename = "commandId")] command_id: String, config: SpeechRuntimeConfig },
    #[serde(rename = "speech.settings.key")]
    Key { #[serde(rename = "commandId")] command_id: String, secret: String },
    #[serde(rename = "speech.settings.voices")]
    Voices { #[serde(rename = "commandId")] command_id: String },
    #[serde(rename = "speech.settings.check")]
    Check { #[serde(rename = "commandId")] command_id: String },
    #[serde(rename = "speech.settings.playback")]
    Playback { #[serde(rename = "commandId")] command_id: String },
}
impl Request {
    fn command_id(&self) -> &str {
        match self { Self::Read { command_id } | Self::Configure { command_id, .. } | Self::Key { command_id, .. } | Self::Voices { command_id } | Self::Check { command_id } | Self::Playback { command_id } => command_id }
    }
    fn action(&self) -> &'static str {
        match self { Self::Read {..} => "speech.settings.read", Self::Configure {..} => "speech.settings.configure", Self::Key {..} => "speech.settings.key", Self::Voices {..} => "speech.settings.voices", Self::Check {..} => "speech.settings.check", Self::Playback {..} => "speech.settings.playback" }
    }
}
struct Authority { root: PathBuf, token: String, public_read: bool, current: Arc<dyn Fn() -> bool + Send + Sync> }
impl Authority {
    fn check(&self) -> anyhow::Result<()> {
        ensure!((self.current)(), "speech settings connection retired");
        let claims = store::verified_webrtc_capability_claims(&self.root, &self.token).context("speech settings authority unavailable")?;
        ensure!(self.public_read || matches!(claims.role.as_str(), "chef" | "admin"), "speech settings requires Owner/Admin");
        Ok(())
    }
}
impl WebRTCPublicationGuard for Authority {
    fn with_current(&self, publish: &mut dyn FnMut() -> rxdb::rx_error::RxResult<()>) -> rxdb::rx_error::RxResult<()> {
        // The guarded native responder already holds the actual peer-generation fence.
        let claims = store::verified_webrtc_capability_claims(&self.root, &self.token);
        if !claims.is_some_and(|c| self.public_read || matches!(c.role.as_str(), "chef" | "admin")) {
            return Err(rxdb::rx_error::new_rx_error("SPEECH_SETTINGS_RETIRED", None));
        }
        publish()
    }
}
fn binding(root: &Path) -> anyhow::Result<String> {
    let config = SpeechRuntimeConfig::load(root)?;
    let key = Zeroizing::new(crate::execution::speech::mistral_key(root).unwrap_or_default());
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&config)?);
    digest.update(key.as_bytes());
    Ok(format!("{:x}", digest.finalize()))
}
fn snapshot(root: &Path) -> anyhow::Result<Value> {
    let status = SpeechGateway::from_root(root)?.status();
    let checks: Option<Value> = crate::persistence::load_json_payload(root, CHECK_KEY)?;
    let current_binding = binding(root)?;
    let tts = checks.filter(|c| c.get("binding").and_then(Value::as_str) == Some(current_binding.as_str()))
        .and_then(|c| c.get("tts").cloned());
    Ok(json!({"status":status, "ttsCheck":tts}))
}
fn error_class(error: &SpeechError) -> &'static str {
    match error {
        SpeechError::MissingCredential => "missing_credential",
        SpeechError::MissingVoice => "missing_voice",
        SpeechError::ProviderRejected { http_status: Some(401) } => "credentials_rejected",
        SpeechError::ProviderRejected { http_status: Some(403) } => "access_denied",
        SpeechError::ProviderRejected { http_status: Some(429) } => "rate_limit",
        SpeechError::ProviderRejected { http_status: Some(402) } => "quota",
        SpeechError::ProviderRejected { .. } => "provider_rejected",
        SpeechError::TimedOut => "timeout",
        SpeechError::UnsupportedBackend => "backend_unavailable",
        SpeechError::ConfigurationUnavailable => "configuration_unavailable",
        SpeechError::InvalidResponse => "invalid_audio",
        _ => "transport",
    }
}
fn voices(root: &Path) -> anyhow::Result<Value> {
    let key = Zeroizing::new(crate::execution::speech::mistral_key(root).context("missing_credential")?);
    let response = ureq::AgentBuilder::new().redirects(0).timeout(Duration::from_secs(10)).build()
        .get("https://api.mistral.ai/v1/audio/voices?limit=100&offset=0")
        .set("authorization", &format!("Bearer {}", key.as_str())).call()
        .map_err(|_| anyhow::anyhow!("voice_list_unavailable"))?;
    let mut raw = Vec::new();
    response.into_reader().take(65537).read_to_end(&mut raw)?;
    ensure!(raw.len() <= 65536, "voice list exceeds budget");
    let value: Value = serde_json::from_slice(&raw)?;
    let items = value.get("items").and_then(Value::as_array).context("invalid_voice_list")?;
    ensure!(items.len() <= 100, "voice list exceeds budget");
    let mut result = vec![];
    for item in items {
        let id = item.get("id").and_then(Value::as_str).context("invalid_voice_list")?;
        let name = item.get("name").and_then(Value::as_str).context("invalid_voice_list")?;
        ensure!(!id.is_empty() && id.len() <= 256 && name.len() <= 256, "invalid_voice_list");
        result.push(json!({"id":id,"name":name}));
    }
    Ok(json!(result))
}
async fn handle(authority: Arc<Authority>, params: Vec<Value>) -> anyhow::Result<GuardedAuxiliaryResponse> {
    ensure!(params.len() == 1 && serde_json::to_vec(&params)?.len() <= 8192, "invalid speech settings request");
    let request: Request = serde_json::from_value(params.into_iter().next().unwrap())?;
    ensure!(uuid::Uuid::parse_str(request.command_id()).is_ok(), "invalid speech settings command");
    let command_id = request.command_id().to_owned();
    let action = request.action();
    authority.check()?;
    if matches!(request, Request::Playback {..}) {
        return Ok(GuardedAuxiliaryResponse {
            result: json!({"action":action,"commandId":command_id,"rate":SpeechRuntimeConfig::load(&authority.root)?.rate}),
            publication: authority,
        });
    }
    let before = binding(&authority.root)?;
    let mut extras = json!({});
    match request {
        Request::Read {..} => {},
        Request::Playback {..} => unreachable!(),
        Request::Configure {config,..} => { authority.check()?; config.save(&authority.root)?; },
        Request::Key {secret,..} => {
            let secret = Zeroizing::new(secret);
            ensure!(!secret.trim().is_empty() && secret.trim() == secret.as_str() && secret.len() <= 4096 && !secret.contains(['\r','\n']), "invalid speech credential");
            authority.check()?;
            crate::secrets::set_credential(&authority.root, "CTOX_MISTRAL_API_KEY", secret.as_str())?;
        },
        Request::Voices {..} => {
            let root = authority.root.clone();
            extras["voices"] = tokio::task::spawn_blocking(move || voices(&root)).await??;
            authority.check()?;
            ensure!(binding(&authority.root)? == before, "speech configuration changed during voice discovery");
        },
        Request::Check {..} => {
            let root = authority.root.clone();
            let result = tokio::task::spawn_blocking(move || {
                let gateway = SpeechGateway::from_root(&root)?;
                // API acceptance is bounded independently of full narration and
                // never silently chooses a different backend.
                if gateway.status().config.synthesis != crate::execution::speech::SpeechBackend::Mistral {
                    return Err(SpeechError::UnsupportedBackend);
                }
                gateway.synthesize_with_timeout(&SpeechRequest {
                    text: "Die Sprachausgabe von Workjet ist bereit.".into(), format: SpeechAudioFormat::Wav, voice_id: None,
                }, Duration::from_secs(20))
            }).await?;
            authority.check()?;
            ensure!(binding(&authority.root)? == before, "speech configuration changed during check");
            let check = match result {
                Ok(output) => {
                    // A successful HTTP response or JSON body alone cannot be a green audio check.
                    let audio = &output.audio;
                    let wav = super::project_chats::jour_fixe_local_narration::wav_duration(audio).is_ok();
                    if wav {
                        extras["audioBase64"] = json!(STANDARD.encode(audio));
                        json!({"state":"ok", "checkedAt":chrono::Utc::now().to_rfc3339(), "latencyMs":output.elapsed_ms, "errorClass":null})
                    } else { json!({"state":"error", "checkedAt":chrono::Utc::now().to_rfc3339(), "latencyMs":null, "errorClass":"invalid_audio"}) }
                },
                Err(error) => json!({"state":"error", "checkedAt":chrono::Utc::now().to_rfc3339(), "latencyMs":null, "errorClass":error_class(&error)}),
            };
            crate::persistence::store_json_payload(&authority.root, CHECK_KEY, Some(&json!({"binding":before,"tts":check})))?;
        },
    }
    // Playback returns above and never exposes configuration/credential presence.
    authority.check()?;
    let mut result = snapshot(&authority.root)?;
    result["action"] = json!(action);
    result["commandId"] = json!(command_id);
    for (key,value) in extras.as_object().unwrap() { result[key] = value.clone(); }
    Ok(GuardedAuxiliaryResponse { result, publication: authority })
}
pub(super) fn register(pool: &ctox_sync::native::NativePool, root: &Path) -> rxdb::rx_error::RxResult<()> {
    use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
    let weak_pool = Arc::downgrade(pool);
    let root = root.to_path_buf();
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    pool.register_guarded_auxiliary_request_handler(METHOD, Arc::new(move |peer, token, params| {
        let root = root.clone(); let weak_pool = weak_pool.clone(); let slots = slots.clone();
        Box::pin(async move {
            let _permit = slots.try_acquire_owned().map_err(|_| "speech settings check already running".to_owned())?;
            let peer_for_check = peer.clone(); let token_for_check = token.clone();
            let current = Arc::new(move || weak_pool.upgrade().is_some_and(|p|
                !p.canceled.load(Ordering::SeqCst) && p.connection_handler.is_peer_current(&peer_for_check)
                && p.connection_handler.peer_capability_token(&peer_for_check).as_deref() == Some(token_for_check.as_str())));
            let public_read = params.first().and_then(|v| v.get("action")).and_then(Value::as_str) == Some("speech.settings.playback");
            let authority = Arc::new(Authority { root, token, public_read, current });
            // Fixed diagnostics only: never echo caller inputs, credentials or provider responses.
            handle(authority, params).await.map_err(|_| "speech settings unavailable, denied or changed".to_owned())
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authority(root: &Path, role: &str, current: bool) -> anyhow::Result<Arc<Authority>> {
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root, "speech-operator", "Speech operator", role, chrono::Utc::now().timestamp_millis(),
        )?;
        Ok(Arc::new(Authority { root: root.into(), token, public_read: false, current: Arc::new(move || current) }))
    }
    fn command(action: &str) -> Value { json!({"action":action,"commandId":uuid::Uuid::new_v4().to_string()}) }

    #[tokio::test]
    async fn playback_exposes_only_rate_to_current_authenticated_members() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut auth = authority(root.path(), "user", true)?;
        Arc::get_mut(&mut auth).unwrap().public_read = true;
        let response = handle(auth, vec![command("speech.settings.playback")]).await?.result;
        assert_eq!(response["rate"], 1.15);
        assert_eq!(response.as_object().unwrap().len(), 3);
        assert!(response.get("status").is_none());
        let mut retired = authority(root.path(), "user", false)?;
        Arc::get_mut(&mut retired).unwrap().public_read = true;
        assert!(handle(retired, vec![command("speech.settings.playback")]).await.is_err());
        Ok(())
    }

    fn wav() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&68u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&32000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 32]);
        bytes
    }

    #[tokio::test]
    async fn genuine_audio_and_upstream_status_define_persisted_probe_result() -> anyhow::Result<()> {
        use std::io::Write;
        for (status, audio, expected) in [
            (200, wav(), None),
            (200, b"not-a-wave".to_vec(), Some("invalid_audio")),
            (401, Vec::new(), Some("credentials_rejected")),
        ] {
            let root = tempfile::tempdir()?;
            let auth = authority(root.path(), "chef", true)?;
            crate::secrets::set_credential(root.path(), "CTOX_MISTRAL_API_KEY", "private-fixture")?;
            let mut config = SpeechRuntimeConfig::default();
            config.synthesis = crate::execution::speech::SpeechBackend::Mistral;
            config.voice_id = Some("fixture-saved-voice".into());
            config.save(root.path())?;
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let endpoint = format!("http://{}/speech", listener.local_addr()?);
            let _endpoint = crate::execution::speech::tests::MistralTestEndpoint::new(root.path(), endpoint);
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 1024];
                    let len = stream.read(&mut chunk).unwrap();
                    if len == 0 { break; }
                    request.extend_from_slice(&chunk[..len]);
                    if let Some(offset) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&request[..offset]);
                        let length: usize = header.lines().find_map(|line| line.to_lowercase()
                            .strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap();
                        if request.len() >= offset + 4 + length { break; }
                    }
                }
                let offset = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                let body: Value = serde_json::from_slice(&request[offset + 4..]).unwrap();
                assert_eq!(body["voice_id"], "fixture-saved-voice");
                assert!(body.get("rate").is_none());
                let response = if status == 200 { json!({"audio_data":STANDARD.encode(audio)}).to_string() }
                    else { "{\"error\":\"private-provider-detail\"}".into() };
                write!(stream, "HTTP/1.1 {} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", status,response.len(),response).unwrap();
            });
            let response = handle(auth.clone(), vec![command("speech.settings.check")]).await?.result;
            server.join().unwrap();
            assert_eq!(response["ttsCheck"]["errorClass"], expected.map_or(Value::Null, |v| json!(v)));
            assert_eq!(response["ttsCheck"]["state"], if expected.is_none() {"ok"} else {"error"});
            assert_eq!(response.get("audioBase64").is_some(), expected.is_none());
            assert!(!response.to_string().contains("private-"));
            let readback = handle(auth.clone(), vec![command("speech.settings.read")]).await?.result;
            assert_eq!(readback["ttsCheck"], response["ttsCheck"]);
            // A replacement credential invalidates prior acceptance without inventing a new result.
            crate::secrets::set_credential(root.path(), "CTOX_MISTRAL_API_KEY", "replacement-fixture")?;
            let changed = handle(auth, vec![command("speech.settings.read")]).await?.result;
            assert!(changed["ttsCheck"].is_null());
        }
        Ok(())
    }
    #[tokio::test]
    async fn speech_config_and_secret_are_native_and_configuration_is_not_readiness() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let auth = authority(root.path(), "chef", true)?;
        let mut key = command("speech.settings.key");
        key["secret"] = json!("private-fixture-mistral-key");
        let response = handle(auth.clone(), vec![key]).await?.result;
        ensure!(!response.to_string().contains("private-fixture"), "credential leaked");
        ensure!(response["ttsCheck"].is_null(), "credential was treated as inference proof");
        assert_eq!(crate::secrets::get_credential(root.path(), "CTOX_MISTRAL_API_KEY").as_deref(), Some("private-fixture-mistral-key"));
        let mut request = command("speech.settings.configure");
        request["config"] = json!({"synthesis":"mistral","transcription":"mistral","voice_id":"approved-voice","rate":1.25});
        let response = handle(auth.clone(), vec![request]).await?.result;
        assert_eq!(response["status"]["config"]["rate"], 1.25);
        assert_eq!(SpeechRuntimeConfig::load(root.path())?.rate.value(), 1.25);
        assert!(response["ttsCheck"].is_null());
        let mut invalid = command("speech.settings.configure");
        invalid["config"] = json!({"synthesis":"mistral","transcription":"mistral","voice_id":"approved-voice","rate":1.51});
        assert!(handle(auth, vec![invalid]).await.is_err());
        assert_eq!(SpeechRuntimeConfig::load(root.path())?.rate.value(), 1.25);
        Ok(())
    }
    #[tokio::test]
    async fn ordinary_or_retired_peer_cannot_read_or_change_speech() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        for (role,current) in [("user",true),("chef",false)] {
            let auth = authority(root.path(), role, current)?;
            assert!(handle(auth,vec![command("speech.settings.read")]).await.is_err());
        }
        Ok(())
    }
    #[tokio::test]
    async fn missing_voice_is_persisted_as_prerequisite_not_provider_rejection() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let auth = authority(root.path(), "chef", true)?;
        let mut request = command("speech.settings.configure");
        request["config"] = json!({"synthesis":"mistral","transcription":"mistral","voice_id":null});
        handle(auth.clone(), vec![request]).await?;
        let response = handle(auth, vec![command("speech.settings.check")]).await?.result;
        assert_eq!(response["ttsCheck"]["state"], "error");
        assert_eq!(response["ttsCheck"]["errorClass"], "missing_voice");
        assert!(response.get("audioBase64").is_none());
        Ok(())
    }
}
