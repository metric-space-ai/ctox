// Origin: CTOX
// License: AGPL-3.0-only
//! Server-side speech contract shared by meeting tools and native Workjet.
//! Caller owns meeting authorization and persistence; credentials never cross this API.
#[cfg(unix)]
#[path = "speech_computer.rs"]
pub mod computer;
#[path = "speech_prepare.rs"]
pub mod prepare;
#[cfg(unix)]
#[path = "speech_target.rs"]
pub(crate) mod target;
#[cfg(unix)]
#[path = "speech_target_policy.rs"]
pub mod target_policy;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig, Message,
    },
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;

const CONFIG_KEY: &str = "speech_gateway";
pub const MISTRAL_REALTIME_MODEL: &str = "voxtral-mini-transcribe-realtime-2602";
pub const MISTRAL_TTS_MODEL: &str = "voxtral-mini-tts-2603";
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_AUDIO_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

trait RuntimeSpeechIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> RuntimeSpeechIo for T {}
type RuntimeSocket = Box<dyn RuntimeSpeechIo>;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechBackend {
    #[default]
    Runtime,
    Mistral,
    Computer,
}

/// SQLite runtime configuration, independent of the primary chat model.
/// Saving requires the caller's existing Owner/Admin configuration boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechRuntimeConfig {
    pub synthesis: SpeechBackend,
    pub transcription: SpeechBackend,
    pub voice_id: Option<String>,
    /// Pitch-preserving playback rate applied after synthesis by the consumer.
    #[serde(default)]
    pub rate: SpeechRate,
}

/// A bounded scalar on the wire, stored without floating-point equality drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechRate(u16);
impl Default for SpeechRate {
    fn default() -> Self {
        Self(115)
    }
}
impl SpeechRate {
    pub fn value(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}
impl Serialize for SpeechRate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.value())
    }
}
impl<'de> Deserialize<'de> for SpeechRate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rate = f64::deserialize(deserializer)?;
        if !rate.is_finite() || !(0.8..=1.5).contains(&rate) {
            return Err(serde::de::Error::custom(
                "speech rate must be between 0.8 and 1.5",
            ));
        }
        Ok(Self((rate * 100.0).round() as u16))
    }
}

impl SpeechRuntimeConfig {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        Ok(crate::persistence::load_json_payload(root, CONFIG_KEY)?.unwrap_or_default())
    }

    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        if let Some(voice) = &self.voice_id {
            anyhow::ensure!(
                !voice.trim().is_empty() && voice.len() <= 256,
                "invalid speech voice"
            );
        }
        crate::persistence::store_json_payload(root, CONFIG_KEY, Some(self))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAudioFormat {
    Wav,
    Mp3,
    Pcm,
}

impl SpeechAudioFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Pcm => "pcm",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechRequest {
    pub text: String,
    pub format: SpeechAudioFormat,
    pub voice_id: Option<String>,
}

/// Audio bytes stay native. The meeting layer stores an artifact and exposes its reference.
pub struct SpeechOutput {
    pub audio: Vec<u8>,
    pub format: SpeechAudioFormat,
    pub model: String,
    pub input_characters: usize,
    pub elapsed_ms: u64,
}

/// Producer provenance, not a meeting permission or a playable-audio verdict.
/// Only `SpeechGateway::synthesize_verified` can construct this value. The
/// native meeting adapter must still validate/store the audio through its file
/// authority and derive duration from the actual bytes before publishing it.
pub struct VerifiedSpeechOutput {
    run_id: String,
    text_sha256: String,
    audio_sha256: String,
    output: SpeechOutput,
}

#[cfg(test)]
pub(crate) fn verified_fixture_output(text: &str, audio: Vec<u8>) -> VerifiedSpeechOutput {
    // Unit-test producer seam only. No constructor exists in a production build;
    // these bytes prove custody/fencing, not installed TTS or provider latency.
    VerifiedSpeechOutput {
        run_id: Uuid::new_v4().to_string(),
        text_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
        audio_sha256: format!("{:x}", Sha256::digest(&audio)),
        output: SpeechOutput {
            audio,
            format: SpeechAudioFormat::Wav,
            model: "fixture-native-producer".into(),
            input_characters: text.chars().count(),
            elapsed_ms: 12,
        },
    }
}

impl VerifiedSpeechOutput {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn text_sha256(&self) -> &str {
        &self.text_sha256
    }
    pub fn audio_sha256(&self) -> &str {
        &self.audio_sha256
    }
    pub fn audio(&self) -> &[u8] {
        &self.output.audio
    }
    pub fn format(&self) -> SpeechAudioFormat {
        self.output.format
    }
    pub fn model(&self) -> &str {
        &self.output.model
    }
    pub fn input_characters(&self) -> usize {
        self.output.input_characters
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.output.elapsed_ms
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PcmFormat {
    pub sample_rate_hz: u32,
}

impl Default for PcmFormat {
    fn default() -> Self {
        Self {
            sample_rate_hz: 16_000,
        }
    }
}

impl PcmFormat {
    fn validate(self) -> Result<(), SpeechError> {
        if ![8_000, 16_000, 22_050, 44_100, 48_000].contains(&self.sample_rate_hz) {
            return Err(SpeechError::InvalidRequest);
        }
        Ok(())
    }
    fn validate_chunk(self, pcm: &[u8]) -> Result<(), SpeechError> {
        // Raw signed 16-bit little-endian MONO, at most 100 ms per append.
        if pcm.is_empty() || pcm.len() % 2 != 0 || pcm.len() > self.sample_rate_hz as usize / 10 * 2
        {
            return Err(SpeechError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptEvent {
    Partial {
        sequence: u64,
        text: String,
        received_after_start_ms: u64,
    },
    Final {
        sequence: u64,
        text: String,
        model: String,
        /// Server-side caller mark -> final event. Capture/network/VAD latency is separate.
        finish_to_final_ms: Option<u64>,
        audio_duration_ms: u64,
    },
}

/// Non-deserializable final receipt minted only by an actual gateway stream.
/// A caller must bind this stream ID to its authenticated meeting at open and
/// revalidate that same authority before persisting the receipt. This value
/// alone neither identifies the speaker nor authorizes any meeting mutation.
#[derive(Debug)]
pub struct VerifiedTranscriptFinal {
    stream_id: String,
    sequence: u64,
    text: String,
    model: String,
    finish_to_final_ms: Option<u64>,
    audio_duration_ms: u64,
}

impl VerifiedTranscriptFinal {
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn finish_to_final_ms(&self) -> Option<u64> {
        self.finish_to_final_ms
    }
    pub fn audio_duration_ms(&self) -> u64 {
        self.audio_duration_ms
    }
}

/// Partial text remains transient. Only the branded Final variant is suitable
/// for native speech persistence; deserializing TranscriptEvent grants none.
#[derive(Debug)]
pub enum VerifiedTranscriptEvent {
    Partial {
        stream_id: String,
        sequence: u64,
        /// Delta, accumulated by the native/UI adapter within this stream.
        text: String,
        received_after_start_ms: u64,
    },
    Final(VerifiedTranscriptFinal),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum SpeechError {
    ConfigurationUnavailable,
    /// Executor setup failed before any provider request was started.
    ExecutionUnavailable,
    MissingCredential,
    MissingVoice,
    UnsupportedBackend,
    InvalidRequest,
    Transport,
    TimedOut,
    InvalidResponse,
    Backpressure,
    Closed,
    ProviderRejected {
        http_status: Option<u16>,
    },
}
impl fmt::Display for SpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SpeechError {}

#[derive(Debug, Clone, Serialize)]
pub struct SpeechStatus {
    pub config: SpeechRuntimeConfig,
    pub mistral_credential_present: bool,
    pub mistral_voice_configured: bool,
    pub streaming_stt_selected: bool,
    /// Verified readiness, or unknown when only backend configuration is available.
    pub stt: SpeechAvailability,
    /// Verified readiness, or unknown when only backend configuration is available.
    pub tts: SpeechAvailability,
}

/// Three-state answer for one speech role. `Unknown` means the check could not run,
/// which is different from `Unavailable`, where the check ran and found nothing usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechAvailability {
    Unknown,
    Available,
    Unavailable,
}

/// Maps the outcome of a readiness check to a role availability.
/// `Err` means the check itself could not run.
pub fn availability_from_check(check: Result<bool, ()>) -> SpeechAvailability {
    match check {
        Ok(true) => SpeechAvailability::Available,
        Ok(false) => SpeechAvailability::Unavailable,
        Err(()) => SpeechAvailability::Unknown,
    }
}

fn availability_from_configuration(configuration: Result<bool, ()>) -> SpeechAvailability {
    match configuration {
        Ok(false) => SpeechAvailability::Unavailable,
        Ok(true) | Err(()) => SpeechAvailability::Unknown,
    }
}

/// Local operator configuration through the existing runtime store. This is not
/// a browser/control-plane endpoint; remote callers retain their Owner/Admin gate.
/// Parse and validate the complete bounded document before modifying persisted state.
pub fn configure_from_file(root: &Path, path: &Path) -> anyhow::Result<SpeechStatus> {
    let mut raw = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut raw)?;
    anyhow::ensure!(raw.len() <= 4096, "speech configuration exceeds 4096 bytes");
    let config: SpeechRuntimeConfig = serde_json::from_slice(&raw)?;
    config.save(root)?;
    Ok(SpeechGateway::from_root(root)?.status())
}

pub struct SpeechGateway {
    root: PathBuf,
    config: SpeechRuntimeConfig,
}

impl SpeechGateway {
    pub fn from_root(root: &Path) -> Result<Self, SpeechError> {
        let config =
            SpeechRuntimeConfig::load(root).map_err(|_| SpeechError::ConfigurationUnavailable)?;
        Ok(Self {
            root: root.to_owned(),
            config,
        })
    }

    pub fn status(&self) -> SpeechStatus {
        SpeechStatus {
            stt: self.role_availability(
                self.config.transcription,
                crate::inference::engine::AuxiliaryRole::Stt,
            ),
            tts: self.role_availability(
                self.config.synthesis,
                crate::inference::engine::AuxiliaryRole::Tts,
            ),
            config: self.config.clone(),
            mistral_credential_present: mistral_key(&self.root).is_some(),
            mistral_voice_configured: self.config.voice_id.is_some(),
            streaming_stt_selected: match self.config.transcription {
                SpeechBackend::Computer => {
                    #[cfg(unix)]
                    {
                        computer::SpeechComputerConfig::load(&self.root)
                            .ok()
                            .is_some_and(|c| c.transcription.is_some())
                    }
                    #[cfg(not(unix))]
                    {
                        false
                    }
                }
                SpeechBackend::Mistral => true,
                SpeechBackend::Runtime => {
                    crate::inference::runtime_kernel::InferenceRuntimeKernel::resolve(&self.root)
                        .ok()
                        .is_some_and(|runtime| {
                            runtime
                                .binding_for_auxiliary_role(
                                    crate::inference::engine::AuxiliaryRole::Stt,
                                )
                                .is_some()
                        })
                }
            },
        }
    }

    /// Configuration can establish absence, but cannot prove a provider or
    /// holding computer will answer. Present credentials and model bindings
    /// remain unknown until an actual request verifies that speech role.
    fn role_availability(
        &self,
        backend: SpeechBackend,
        role: crate::inference::engine::AuxiliaryRole,
    ) -> SpeechAvailability {
        match backend {
            SpeechBackend::Mistral => {
                availability_from_configuration(Ok(mistral_key(&self.root).is_some()))
            }
            SpeechBackend::Computer => match role {
                crate::inference::engine::AuxiliaryRole::Stt => {
                    #[cfg(unix)]
                    {
                        availability_from_configuration(
                            computer::SpeechComputerConfig::load(&self.root)
                                .map(|c| c.transcription.is_some())
                                .map_err(|_| ()),
                        )
                    }
                    #[cfg(not(unix))]
                    {
                        SpeechAvailability::Unavailable
                    }
                }
                _ => SpeechAvailability::Unavailable,
            },
            SpeechBackend::Runtime => availability_from_configuration(
                crate::inference::runtime_kernel::InferenceRuntimeKernel::resolve(&self.root)
                    .map(|runtime| runtime.binding_for_auxiliary_role(role).is_some())
                    .map_err(|_| ()),
            ),
        }
    }

    /// Complete a slide narration or short spoken answer using the selected adapter.
    pub fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechOutput, SpeechError> {
        self.synthesize_with_timeout(request, Duration::from_secs(60))
    }

    /// A short settings probe has a shorter IO budget than a full slide.
    pub(crate) fn synthesize_with_timeout(
        &self,
        request: &SpeechRequest,
        timeout: Duration,
    ) -> Result<SpeechOutput, SpeechError> {
        if request.text.trim().is_empty() || request.text.len() > MAX_TEXT_BYTES {
            return Err(SpeechError::InvalidRequest);
        }
        let voice = request.voice_id.as_ref().or(self.config.voice_id.as_ref());
        if voice.is_some_and(|s| s.trim().is_empty() || s.len() > 256) {
            return Err(SpeechError::InvalidRequest);
        }
        let started = Instant::now();
        let (audio, model) = match self.config.synthesis {
            // Native computer RPC is asynchronous. Never block a Tokio thread
            // by creating a nested runtime or silently use a different model.
            SpeechBackend::Computer => return Err(SpeechError::UnsupportedBackend),
            SpeechBackend::Runtime => {
                let model =
                    crate::inference::runtime_env::env_or_config(&self.root, "CTOX_TTS_MODEL")
                        .unwrap_or_default();
                let audio = crate::communication::gateway::synthesize_speech(
                    &self.root,
                    &request.text,
                    &model,
                    voice.map(String::as_str).unwrap_or(""),
                    request.format.label(),
                )
                .map_err(|_| SpeechError::Transport)?;
                (audio, model)
            }
            SpeechBackend::Mistral => {
                // This adapter uses saved voices, not reference-audio uploads.
                // A missing voice is a local prerequisite, never an upstream rejection.
                let voice = voice.ok_or(SpeechError::MissingVoice)?;
                let key = mistral_key(&self.root).ok_or(SpeechError::MissingCredential)?;
                let body = json!({
                    "model": MISTRAL_TTS_MODEL, "input": request.text,
                    "voice_id": voice, "response_format": request.format.label(), "stream": false,
                });
                let agent = ureq::AgentBuilder::new()
                    .timeout(timeout)
                    .redirects(0)
                    .build();
                let response = agent
                    .post(&mistral_speech_endpoint(&self.root))
                    .set("authorization", &format!("Bearer {key}"))
                    .set("content-type", "application/json")
                    .send_bytes(body.to_string().as_bytes())
                    .map_err(|e| match e {
                        ureq::Error::Status(http_status, _) => SpeechError::ProviderRejected {
                            http_status: Some(http_status),
                        },
                        _ => SpeechError::Transport,
                    })?;
                let mut encoded = Vec::new();
                response
                    .into_reader()
                    .take((MAX_AUDIO_BYTES * 2 + 1) as u64)
                    .read_to_end(&mut encoded)
                    .map_err(|_| SpeechError::Transport)?;
                let audio = decode_mistral_speech(&encoded)?;
                (audio, MISTRAL_TTS_MODEL.to_string())
            }
        };
        if audio.is_empty() || audio.len() > MAX_AUDIO_BYTES {
            return Err(SpeechError::InvalidResponse);
        }
        Ok(SpeechOutput {
            audio,
            model,
            format: request.format,
            input_characters: request.text.chars().count(),
            elapsed_ms: millis(started.elapsed()),
        })
    }

    /// Synthesize through the actual configured adapter and brand its result.
    pub fn synthesize_verified(
        &self,
        request: &SpeechRequest,
    ) -> Result<VerifiedSpeechOutput, SpeechError> {
        let output = self.synthesize(request)?;
        Ok(VerifiedSpeechOutput {
            run_id: Uuid::new_v4().to_string(),
            text_sha256: format!("{:x}", Sha256::digest(request.text.as_bytes())),
            audio_sha256: format!("{:x}", Sha256::digest(&output.audio)),
            output,
        })
    }

    /// Must run inside the daemon's existing Tokio runtime. No independent daemon or browser token.
    pub async fn synthesize_verified_async(
        &self,
        request: &SpeechRequest,
    ) -> Result<VerifiedSpeechOutput, SpeechError> {
        if self.config.synthesis == SpeechBackend::Computer {
            #[cfg(unix)]
            {
                return computer::synthesize(&self.root, request, self.config.voice_id.as_ref())
                    .await;
            }
            #[cfg(not(unix))]
            {
                return Err(SpeechError::UnsupportedBackend);
            }
        }
        let root = self.root.clone();
        let request = request.clone();
        tokio::task::spawn_blocking(move || {
            SpeechGateway::from_root(&root)?.synthesize_verified(&request)
        })
        .await
        .map_err(|_| SpeechError::Transport)?
    }

    /// Must run inside the daemon's existing Tokio runtime. No independent daemon or browser token.
    pub async fn open_transcription(
        &self,
        format: PcmFormat,
    ) -> Result<TranscriptionStream, SpeechError> {
        format.validate()?;
        if self.config.transcription == SpeechBackend::Computer {
            #[cfg(unix)]
            {
                return computer::open_transcription(&self.root, format).await;
            }
            #[cfg(not(unix))]
            {
                return Err(SpeechError::UnsupportedBackend);
            }
        }
        if self.config.transcription != SpeechBackend::Mistral {
            let root = self.root.clone();
            let binding = tokio::task::spawn_blocking(move || {
                let runtime =
                    crate::inference::runtime_kernel::InferenceRuntimeKernel::resolve(&root)
                        .map_err(|_| SpeechError::ConfigurationUnavailable)?;
                runtime
                    .binding_for_auxiliary_role(crate::inference::engine::AuxiliaryRole::Stt)
                    .cloned()
                    .ok_or(SpeechError::UnsupportedBackend)
            })
            .await
            .map_err(|_| SpeechError::Transport)??;
            return TranscriptionStream::open_runtime(
                binding.transport,
                binding.request_model,
                format,
            )
            .await;
        }
        let key = mistral_key(&self.root).ok_or(SpeechError::MissingCredential)?;
        let endpoint = format!(
            "wss://api.mistral.ai/v1/audio/transcriptions/realtime?model={MISTRAL_REALTIME_MODEL}"
        );
        let socket = connect(&endpoint, &key).await?;
        TranscriptionStream::open(socket, format).await
    }
}

pub(crate) fn mistral_key(root: &Path) -> Option<String> {
    // Existing encrypted credentials only. Never read a new ambient env switch.
    ["CTOX_MISTRAL_API_KEY", "MISTRAL_API_KEY"]
        .iter()
        .find_map(|k| crate::inference::runtime_env::env_or_config(root, k))
        .filter(|k| !k.trim().is_empty())
}

fn mistral_speech_endpoint(_root: &Path) -> String {
    #[cfg(test)]
    if let Some(endpoint) = tests::mistral_test_endpoint(_root) {
        return endpoint;
    }
    "https://api.mistral.ai/v1/audio/speech".to_owned()
}

#[cfg(test)]
pub(crate) use tests::MistralTestEndpoint;

fn decode_mistral_speech(encoded: &[u8]) -> Result<Vec<u8>, SpeechError> {
    if encoded.len() > MAX_AUDIO_BYTES * 2 {
        return Err(SpeechError::InvalidResponse);
    }
    let value: Value = serde_json::from_slice(encoded).map_err(|_| SpeechError::InvalidResponse)?;
    let data = value
        .get("audio_data")
        .and_then(Value::as_str)
        .ok_or(SpeechError::InvalidResponse)?;
    let bytes = BASE64
        .decode(data)
        .map_err(|_| SpeechError::InvalidResponse)?;
    if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
        return Err(SpeechError::InvalidResponse);
    }
    Ok(bytes)
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

async fn connect(endpoint: &str, key: &str) -> Result<Socket, SpeechError> {
    let mut request = endpoint
        .into_client_request()
        .map_err(|_| SpeechError::InvalidRequest)?;
    let mut auth =
        HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| SpeechError::InvalidRequest)?;
    auth.set_sensitive(true);
    request.headers_mut().insert("authorization", auth);
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_TEXT_BYTES))
        .max_frame_size(Some(MAX_TEXT_BYTES));
    let (socket, _) = tokio::time::timeout(
        IO_TIMEOUT,
        connect_async_with_config(request, Some(ws_config), false),
    )
    .await
    .map_err(|_| SpeechError::TimedOut)?
    .map_err(|e| match e {
        tokio_tungstenite::tungstenite::Error::Http(response) => SpeechError::ProviderRejected {
            http_status: Some(response.status().as_u16()),
        },
        _ => SpeechError::Transport,
    })?;
    Ok(socket)
}

enum Input {
    Audio(Vec<u8>),
    Flush,
    Finish(Instant),
    Cancel,
}

/// Owned bounded session. Drop aborts its sole task and drops the provider socket.
/// Backpressure never silently discards microphone audio or builds an unbounded queue.
pub struct TranscriptionStream {
    stream_id: String,
    input: mpsc::Sender<Input>,
    events: mpsc::Receiver<Result<TranscriptEvent, SpeechError>>,
    terminal: Option<oneshot::Receiver<Result<(), SpeechError>>>,
    task: JoinHandle<()>,
    format: PcmFormat,
    finished: bool,
}

impl TranscriptionStream {
    async fn open_runtime(
        transport: crate::inference::local_transport::LocalTransport,
        model: String,
        format: PcmFormat,
    ) -> Result<Self, SpeechError> {
        use crate::inference::local_transport::LocalTransport;
        use crate::inference::native_stt::{LocalSttRequest, LocalSttResponse};
        if format.sample_rate_hz != 16_000 {
            return Err(SpeechError::InvalidRequest);
        }
        let socket: RuntimeSocket = match transport {
            LocalTransport::UnixSocket { path } => {
                #[cfg(unix)]
                {
                    Box::new(
                        tokio::time::timeout(IO_TIMEOUT, tokio::net::UnixStream::connect(path))
                            .await
                            .map_err(|_| SpeechError::TimedOut)?
                            .map_err(|_| SpeechError::Transport)?,
                    )
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    return Err(SpeechError::UnsupportedBackend);
                }
            }
            LocalTransport::NamedPipe { name } => {
                #[cfg(windows)]
                {
                    Box::new(
                        tokio::net::windows::named_pipe::ClientOptions::new()
                            .open(LocalTransport::named_pipe_endpoint(&name))
                            .map_err(|_| SpeechError::Transport)?,
                    )
                }
                #[cfg(not(windows))]
                {
                    let _ = name;
                    return Err(SpeechError::UnsupportedBackend);
                }
            }
            LocalTransport::TcpLoopback { .. } => return Err(SpeechError::UnsupportedBackend),
        };
        let mut socket = BufReader::new(socket);
        let ready = runtime_roundtrip(
            &mut socket,
            &LocalSttRequest::TranscriptionOpen {
                model: Some(model.clone()),
                sample_rate_hz: format.sample_rate_hz,
            },
        )
        .await?;
        if !matches!(ready, LocalSttResponse::StreamReady { model: ref actual } if actual == &model)
        {
            return Err(SpeechError::InvalidResponse);
        }
        let (input, rx) = mpsc::channel(8);
        let (tx, events) = mpsc::channel(32);
        let (done_tx, terminal) = oneshot::channel();
        let task = tokio::spawn(async move {
            let result =
                tokio::time::timeout(SESSION_TIMEOUT, pump_runtime(socket, model, rx, &tx))
                    .await
                    .unwrap_or(Err(SpeechError::TimedOut));
            let _ = done_tx.send(result);
        });
        Ok(Self {
            stream_id: Uuid::new_v4().to_string(),
            input,
            events,
            terminal: Some(terminal),
            task,
            format,
            finished: false,
        })
    }

    async fn open(mut socket: Socket, format: PcmFormat) -> Result<Self, SpeechError> {
        let created = tokio::time::timeout(IO_TIMEOUT, receive_json(&mut socket))
            .await
            .map_err(|_| SpeechError::TimedOut)??;
        if created.get("type").and_then(Value::as_str) != Some("session.created") {
            return Err(SpeechError::InvalidResponse);
        }
        send_json(&mut socket, json!({
            "type": "session.update", "session": {
                "audio_format": { "encoding": "pcm_s16le", "sample_rate": format.sample_rate_hz },
                "target_streaming_delay_ms": 240,
            }
        })).await?;
        let updated = tokio::time::timeout(IO_TIMEOUT, receive_json(&mut socket))
            .await
            .map_err(|_| SpeechError::TimedOut)??;
        if updated.get("type").and_then(Value::as_str) != Some("session.updated") {
            return Err(SpeechError::InvalidResponse);
        }
        let (input, rx) = mpsc::channel(8);
        let (tx, events) = mpsc::channel(32);
        let (done_tx, terminal) = oneshot::channel();
        let task = tokio::spawn(async move {
            let result = tokio::time::timeout(SESSION_TIMEOUT, pump(socket, format, rx, &tx))
                .await
                .unwrap_or(Err(SpeechError::TimedOut));
            let _ = done_tx.send(result);
        });
        Ok(Self {
            stream_id: Uuid::new_v4().to_string(),
            input,
            events,
            terminal: Some(terminal),
            task,
            format,
            finished: false,
        })
    }

    /// Bind to the verified native meeting before accepting its first PCM.
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    pub fn append_pcm(&self, pcm: &[u8]) -> Result<(), SpeechError> {
        if self.finished {
            return Err(SpeechError::Closed);
        }
        self.format.validate_chunk(pcm)?;
        self.enqueue(Input::Audio(pcm.to_vec()))
    }

    pub fn flush(&self) -> Result<(), SpeechError> {
        if self.finished {
            return Err(SpeechError::Closed);
        }
        self.enqueue(Input::Flush)
    }

    /// Call immediately at the audio producer's sentence-end mark, after its final PCM chunk.
    /// New utterances use a new owned stream; flush alone does not promise a final sentence.
    pub fn finish_audio(&mut self) -> Result<(), SpeechError> {
        if self.finished {
            return Err(SpeechError::Closed);
        }
        self.enqueue(Input::Finish(Instant::now()))?;
        self.finished = true;
        Ok(())
    }

    fn enqueue(&self, input: Input) -> Result<(), SpeechError> {
        self.input.try_send(input).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => SpeechError::Backpressure,
            mpsc::error::TrySendError::Closed(_) => SpeechError::Closed,
        })
    }

    pub async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SpeechError>> {
        if let Some(event) = self.events.recv().await {
            return Some(event);
        }
        match self.terminal.take()?.await {
            Ok(Err(error)) => Some(Err(error)),
            Err(_) => Some(Err(SpeechError::Closed)),
            Ok(Ok(())) => None,
        }
    }

    /// Preferred producer API for meeting persistence. The event is consumed
    /// once from this stream's private gateway channel, never from caller JSON.
    pub async fn next_verified_event(
        &mut self,
    ) -> Option<Result<VerifiedTranscriptEvent, SpeechError>> {
        self.next_event().await.map(|result| {
            result.map(|event| match event {
                TranscriptEvent::Partial {
                    sequence,
                    text,
                    received_after_start_ms,
                } => VerifiedTranscriptEvent::Partial {
                    stream_id: self.stream_id.clone(),
                    sequence,
                    text,
                    received_after_start_ms,
                },
                TranscriptEvent::Final {
                    sequence,
                    text,
                    model,
                    finish_to_final_ms,
                    audio_duration_ms,
                } => VerifiedTranscriptEvent::Final(VerifiedTranscriptFinal {
                    stream_id: self.stream_id.clone(),
                    sequence,
                    text,
                    model,
                    finish_to_final_ms,
                    audio_duration_ms,
                }),
            })
        })
    }

    pub async fn cancel(mut self) {
        let _ = self.input.try_send(Input::Cancel);
        if tokio::time::timeout(Duration::from_secs(1), &mut self.task)
            .await
            .is_err()
        {
            self.task.abort();
        }
    }
}

impl Drop for TranscriptionStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn runtime_roundtrip(
    socket: &mut BufReader<RuntimeSocket>,
    request: &crate::inference::native_stt::LocalSttRequest,
) -> Result<crate::inference::native_stt::LocalSttResponse, SpeechError> {
    use crate::inference::native_stt::LocalSttResponse;
    tokio::time::timeout(IO_TIMEOUT, async {
        let mut bytes = serde_json::to_vec(request).map_err(|_| SpeechError::InvalidRequest)?;
        bytes.push(b'\n');
        socket
            .get_mut()
            .write_all(&bytes)
            .await
            .map_err(|_| SpeechError::Transport)?;
        socket
            .get_mut()
            .flush()
            .await
            .map_err(|_| SpeechError::Transport)?;
        let mut response = Vec::new();
        loop {
            let buffer = socket
                .fill_buf()
                .await
                .map_err(|_| SpeechError::Transport)?;
            if buffer.is_empty() {
                return Err(SpeechError::Closed);
            }
            let length = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map(|n| n + 1)
                .unwrap_or(buffer.len());
            let complete = buffer[length - 1] == b'\n';
            if response.len() + length > MAX_TEXT_BYTES {
                return Err(SpeechError::InvalidResponse);
            }
            response.extend_from_slice(&buffer[..length]);
            socket.consume(length);
            if complete {
                break;
            }
        }
        let response: LocalSttResponse =
            serde_json::from_slice(&response).map_err(|_| SpeechError::InvalidResponse)?;
        if let LocalSttResponse::Error { code, .. } = response {
            return Err(match code.as_str() {
                "invalid_request" => SpeechError::InvalidRequest,
                "unsupported_backend" | "backend_unavailable" => SpeechError::UnsupportedBackend,
                _ => SpeechError::Transport,
            });
        }
        Ok(response)
    })
    .await
    .map_err(|_| SpeechError::TimedOut)?
}

async fn pump_runtime(
    mut socket: BufReader<RuntimeSocket>,
    model: String,
    mut input: mpsc::Receiver<Input>,
    events: &mpsc::Sender<Result<TranscriptEvent, SpeechError>>,
) -> Result<(), SpeechError> {
    use crate::inference::native_stt::{LocalSttRequest, LocalSttResponse};
    let started = Instant::now();
    let mut sequence = 0u64;
    let mut event_sequence = 0u64;
    let mut previous_text = String::new();
    while let Some(command) = input.recv().await {
        let (request, finish_mark) = match command {
            Input::Audio(pcm) => (
                LocalSttRequest::TranscriptionAppend {
                    pcm_base64: BASE64.encode(pcm),
                },
                None,
            ),
            Input::Flush => (LocalSttRequest::TranscriptionFlush, None),
            Input::Finish(mark) => (LocalSttRequest::TranscriptionFinish, Some(mark)),
            Input::Cancel => return Ok(()),
        };
        sequence += 1;
        match runtime_roundtrip(&mut socket, &request).await? {
            LocalSttResponse::StreamUpdate {
                sequence: actual,
                text,
                ..
            } if actual == sequence && finish_mark.is_none() => {
                if let Some(text) = text.filter(|t| t != &previous_text) {
                    let delta = text
                        .strip_prefix(&previous_text)
                        .ok_or(SpeechError::InvalidResponse)?
                        .to_owned();
                    previous_text = text.clone();
                    event_sequence += 1;
                    events
                        .try_send(Ok(TranscriptEvent::Partial {
                            sequence: event_sequence,
                            text: delta,
                            received_after_start_ms: millis(started.elapsed()),
                        }))
                        .map_err(|_| SpeechError::Backpressure)?;
                }
            }
            LocalSttResponse::TranscriptionFinal {
                sequence: actual,
                text,
                model: actual_model,
                audio_duration_ms,
            } if actual == sequence && actual_model == model && finish_mark.is_some() => {
                event_sequence += 1;
                events
                    .try_send(Ok(TranscriptEvent::Final {
                        sequence: event_sequence,
                        text,
                        model,
                        finish_to_final_ms: finish_mark.map(|mark| millis(mark.elapsed())),
                        audio_duration_ms,
                    }))
                    .map_err(|_| SpeechError::Backpressure)?;
                return Ok(());
            }
            _ => return Err(SpeechError::InvalidResponse),
        }
    }
    Ok(())
}

async fn send_json(socket: &mut Socket, value: Value) -> Result<(), SpeechError> {
    tokio::time::timeout(
        IO_TIMEOUT,
        socket.send(Message::Text(value.to_string().into())),
    )
    .await
    .map_err(|_| SpeechError::TimedOut)?
    .map_err(|_| SpeechError::Transport)
}

async fn receive_json(socket: &mut Socket) -> Result<Value, SpeechError> {
    loop {
        let message = socket
            .next()
            .await
            .ok_or(SpeechError::Closed)?
            .map_err(|_| SpeechError::Transport)?;
        match message {
            Message::Text(text) => {
                if text.len() > MAX_TEXT_BYTES {
                    return Err(SpeechError::InvalidResponse);
                }
                let value: Value =
                    serde_json::from_str(&text).map_err(|_| SpeechError::InvalidResponse)?;
                if value.get("type").and_then(Value::as_str) == Some("error") {
                    // Provider messages may contain submitted speech/secrets; do not expose them.
                    return Err(SpeechError::ProviderRejected { http_status: None });
                }
                return Ok(value);
            }
            Message::Ping(_) | Message::Pong(_) => {
                socket.flush().await.map_err(|_| SpeechError::Transport)?;
            }
            Message::Close(_) => return Err(SpeechError::Closed),
            _ => return Err(SpeechError::InvalidResponse),
        }
    }
}

async fn pump(
    mut socket: Socket,
    format: PcmFormat,
    mut input: mpsc::Receiver<Input>,
    events: &mpsc::Sender<Result<TranscriptEvent, SpeechError>>,
) -> Result<(), SpeechError> {
    let started = Instant::now();
    let mut finish: Option<Instant> = None;
    let mut text = String::new();
    let mut bytes = 0u64;
    let mut sequence = 0;
    loop {
        let deadline = finish
            .map(|t| t + IO_TIMEOUT)
            .unwrap_or(started + SESSION_TIMEOUT);
        tokio::select! {
            command = input.recv(), if finish.is_none() => {
                match command {
                    Some(Input::Audio(pcm)) => {
                        bytes += pcm.len() as u64;
                        send_json(&mut socket, json!({"type":"input_audio.append","audio":BASE64.encode(&pcm)})).await?;
                    }
                    Some(Input::Flush) => send_json(&mut socket, json!({"type":"input_audio.flush"})).await?,
                    Some(Input::Finish(mark)) => {
                        finish = Some(mark);
                        send_json(&mut socket, json!({"type":"input_audio.flush"})).await?;
                        send_json(&mut socket, json!({"type":"input_audio.end"})).await?;
                    }
                    Some(Input::Cancel) | None => {
                        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
                        return Ok(());
                    }
                }
            }
            event = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), receive_json(&mut socket)) => {
                let value = event.map_err(|_| SpeechError::TimedOut)??;
                match value.get("type").and_then(Value::as_str) {
                    Some("transcription.text.delta") => {
                        let delta = value.get("text").and_then(Value::as_str).ok_or(SpeechError::InvalidResponse)?;
                        if text.len() + delta.len() > MAX_TEXT_BYTES { return Err(SpeechError::InvalidResponse); }
                        text.push_str(delta);
                        sequence += 1;
                        events.try_send(Ok(TranscriptEvent::Partial { sequence, text: delta.to_owned(),
                            received_after_start_ms: millis(started.elapsed()) }))
                            .map_err(|_| SpeechError::Backpressure)?;
                    }
                    Some("transcription.done") => {
                        if finish.is_none() { return Err(SpeechError::InvalidResponse); }
                        if let Some(final_text) = value.get("text").and_then(Value::as_str) {
                            if final_text.len() > MAX_TEXT_BYTES { return Err(SpeechError::InvalidResponse); }
                            text = final_text.to_owned();
                        }
                        sequence += 1;
                        events.try_send(Ok(TranscriptEvent::Final { sequence, text, model:MISTRAL_REALTIME_MODEL.to_owned(),
                            finish_to_final_ms:finish.map(|mark| millis(mark.elapsed())),
                            audio_duration_ms:bytes * 1000 / (format.sample_rate_hz as u64 * 2),
                        })).map_err(|_| SpeechError::Backpressure)?;
                        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
                        return Ok(());
                    }
                    Some("session.updated" | "transcription.language" | "transcription.segment") => {}
                    Some(_) => {} // Future provider metadata is never exposed as transcript content.
                    None => return Err(SpeechError::InvalidResponse),
                }
            }
        }
    }
}

/// Replay a supplied 16 kHz mono s16le PCM fixture at its actual capture cadence.
/// This is an explicit operator smoke, never microphone capture or automatic production inference.
pub async fn benchmark_pcm(root: &Path, pcm_path: &Path) -> anyhow::Result<Value> {
    let file = std::fs::File::open(pcm_path)?;
    let mut pcm = Vec::new();
    file.take((16_000 * 2 * 15 + 1) as u64)
        .read_to_end(&mut pcm)?;
    anyhow::ensure!(
        !pcm.is_empty() && pcm.len() % 2 == 0 && pcm.len() <= 16_000 * 2 * 15,
        "benchmark requires at most 15 seconds of 16kHz mono s16le PCM"
    );
    let gateway = SpeechGateway::from_root(root)?;
    let mut stream = gateway.open_transcription(PcmFormat::default()).await?;
    let started = Instant::now();
    let mut partial_before_end = false;
    let mut next_chunk = 0;
    let mut finished = false;
    let mut clock = tokio::time::interval(Duration::from_millis(20));
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => anyhow::bail!("speech benchmark timed out"),
            _ = clock.tick(), if !finished => {
                if next_chunk == pcm.len() {
                    stream.finish_audio()?;
                    finished = true;
                } else {
                    let end = (next_chunk + 640).min(pcm.len());
                    stream.append_pcm(&pcm[next_chunk..end])?;
                    next_chunk = end;
                }
            }
            event = stream.next_event() => match event {
                Some(Ok(TranscriptEvent::Partial { .. })) => {
                    if !finished { partial_before_end = true; }
                }
                Some(Ok(TranscriptEvent::Final { text, model, finish_to_final_ms, audio_duration_ms, .. })) => return Ok(json!({
                    "model": model, "audio_duration_ms":audio_duration_ms,
                    "replayed_wall_ms":millis(started.elapsed()),
                    "finish_to_final_ms":finish_to_final_ms,
                    "partial_before_audio_end":partial_before_end, "text":text,
                    "measurement_boundary":"server capture-cadence finish mark to provider final; excludes microphone/VAD and client return transport",
                    "meets_gateway_1500ms":finish_to_final_ms.is_some_and(|ms| ms < 1500),
                })),
                Some(Err(error)) => return Err(error.into()),
                None => anyhow::bail!("speech stream ended without a final transcript"),
            }
        }
    }
}

#[cfg(test)]
#[path = "speech_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "speech_runtime_tests.rs"]
mod runtime_tests;

#[cfg(test)]
mod speech_availability_tests {
    use super::{
        availability_from_check, availability_from_configuration, SpeechAvailability,
        SpeechBackend, SpeechGateway, SpeechRuntimeConfig,
    };

    #[test]
    fn status_never_promotes_credential_presence_to_verified_provider_readiness() {
        let root = tempfile::tempdir().unwrap();
        let gateway = SpeechGateway {
            root: root.path().to_owned(),
            config: SpeechRuntimeConfig {
                transcription: SpeechBackend::Mistral,
                synthesis: SpeechBackend::Mistral,
                ..Default::default()
            },
        };
        let status = gateway.status();
        let expected = availability_from_configuration(Ok(status.mistral_credential_present));
        assert_eq!(status.stt, expected);
        assert_eq!(status.tts, expected);
        assert_ne!(status.stt, SpeechAvailability::Available);
        assert_ne!(status.tts, SpeechAvailability::Available);
        assert_eq!(
            availability_from_configuration(Ok(true)),
            SpeechAvailability::Unknown,
        );
    }

    #[test]
    fn a_completed_check_maps_to_available_or_unavailable() {
        assert_eq!(
            availability_from_check(Ok(true)),
            SpeechAvailability::Available
        );
        assert_eq!(
            availability_from_check(Ok(false)),
            SpeechAvailability::Unavailable
        );
    }

    #[test]
    fn a_check_that_could_not_run_is_unknown_not_unavailable() {
        assert_eq!(
            availability_from_check(Err(())),
            SpeechAvailability::Unknown
        );
    }

    #[test]
    fn availability_serializes_to_the_contract_values() {
        let values: Vec<String> = [
            SpeechAvailability::Unknown,
            SpeechAvailability::Available,
            SpeechAvailability::Unavailable,
        ]
        .iter()
        .map(|value| serde_json::to_string(value).unwrap())
        .collect();
        assert_eq!(values, ["\"unknown\"", "\"available\"", "\"unavailable\""]);
    }
}
