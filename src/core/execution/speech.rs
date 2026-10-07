// Origin: CTOX
// License: AGPL-3.0-only
//! Server-side speech contract shared by meeting tools and native Workjet.
//! Caller owns meeting authorization and persistence; credentials never cross this API.
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{
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

const CONFIG_KEY: &str = "speech_gateway";
pub const MISTRAL_REALTIME_MODEL: &str = "voxtral-mini-transcribe-realtime-2602";
pub const MISTRAL_TTS_MODEL: &str = "voxtral-mini-tts-2603";
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_TIMEOUT: Duration = Duration::from_secs(3600);
const MAX_AUDIO_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeechBackend {
    #[default]
    Runtime,
    Mistral,
}

/// SQLite runtime configuration, independent of the primary chat model.
/// Saving requires the caller's existing Owner/Admin configuration boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechRuntimeConfig {
    pub synthesis: SpeechBackend,
    pub transcription: SpeechBackend,
    pub voice_id: Option<String>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum SpeechError {
    ConfigurationUnavailable,
    MissingCredential,
    UnsupportedBackend,
    InvalidRequest,
    Transport,
    TimedOut,
    InvalidResponse,
    Backpressure,
    Closed,
    ProviderRejected { http_status: Option<u16> },
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
    pub streaming_stt_selected: bool,
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
            config: self.config.clone(),
            mistral_credential_present: mistral_key(&self.root).is_some(),
            streaming_stt_selected: self.config.transcription == SpeechBackend::Mistral,
        }
    }

    /// Complete a slide narration or short spoken answer using the selected adapter.
    pub fn synthesize(&self, request: &SpeechRequest) -> Result<SpeechOutput, SpeechError> {
        if request.text.trim().is_empty() || request.text.len() > MAX_TEXT_BYTES {
            return Err(SpeechError::InvalidRequest);
        }
        let voice = request.voice_id.as_ref().or(self.config.voice_id.as_ref());
        if voice.is_some_and(|s| s.trim().is_empty() || s.len() > 256) {
            return Err(SpeechError::InvalidRequest);
        }
        let started = Instant::now();
        let (audio, model) = match self.config.synthesis {
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
                let key = mistral_key(&self.root).ok_or(SpeechError::MissingCredential)?;
                let body = json!({
                    "model": MISTRAL_TTS_MODEL, "input": request.text,
                    "voice_id": voice, "response_format": request.format.label(), "stream": false,
                });
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(60))
                    .build();
                let response = agent
                    .post("https://api.mistral.ai/v1/audio/speech")
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

    /// Must run inside the daemon's existing Tokio runtime. No independent daemon or browser token.
    pub async fn open_transcription(
        &self,
        format: PcmFormat,
    ) -> Result<TranscriptionStream, SpeechError> {
        format.validate()?;
        if self.config.transcription != SpeechBackend::Mistral {
            return Err(SpeechError::UnsupportedBackend);
        }
        let key = mistral_key(&self.root).ok_or(SpeechError::MissingCredential)?;
        let endpoint = format!(
            "wss://api.mistral.ai/v1/audio/transcriptions/realtime?model={MISTRAL_REALTIME_MODEL}"
        );
        let socket = connect(&endpoint, &key).await?;
        TranscriptionStream::open(socket, format).await
    }
}

fn mistral_key(root: &Path) -> Option<String> {
    // Existing encrypted credentials only. Never read a new ambient env switch.
    ["CTOX_MISTRAL_API_KEY", "MISTRAL_API_KEY"]
        .iter()
        .find_map(|k| crate::inference::runtime_env::env_or_config(root, k))
        .filter(|k| !k.trim().is_empty())
}

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
    input: mpsc::Sender<Input>,
    events: mpsc::Receiver<Result<TranscriptEvent, SpeechError>>,
    terminal: Option<oneshot::Receiver<Result<(), SpeechError>>>,
    task: JoinHandle<()>,
    format: PcmFormat,
    finished: bool,
}

impl TranscriptionStream {
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
            input,
            events,
            terminal: Some(terminal),
            task,
            format,
            finished: false,
        })
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
mod tests;
