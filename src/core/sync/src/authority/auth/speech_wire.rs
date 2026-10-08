//! Authenticated native speech RPC framing, not a speech permission.
//!
//! The host MUST check its current speech grant, exact admitted connection,
//! operation sequence and publication guard. Signature verification alone
//! authorizes neither model execution nor disclosure of audio/transcripts.
//! Open/start intents are idempotent within the grant lifetime; hosts retain
//! their bounded intent ledger until expiry. Append retries never append twice.
use super::{invalid, public_key, verify, Body, Envelope, SigningIdentity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

pub const METHOD: &str = "ctox.native.speech.v1";
pub const MAX_WIRE_BYTES: usize = 32 * 1024;
pub const MAX_AUDIO_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_READ_BYTES: u32 = 8192;
const REQUEST_KIND: &str = "ctox.sync.speech.request.v1";
const REPLY_KIND: &str = "ctox.sync.speech.reply.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechWorkload {
    Transcription,
    Synthesis,
}

/// Exact persisted route and grant revision. These are claims until the
/// receiver compares them with current native policy; no endpoint URL or key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechComputerBinding {
    pub source_instance_id: String,
    pub target_instance_id: String,
    pub computer_id: String,
    pub owner_user_id: String,
    pub grant_id: String,
    pub grant_revision: u64,
    pub workload: SpeechWorkload,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpeechOperation {
    OpenTranscription {
        intent_id: String,
        sample_rate_hz: u32,
    },
    Append {
        stream_id: String,
        sequence: u64,
        pcm16_hex: String,
    },
    Finish {
        stream_id: String,
        sequence: u64,
    },
    CancelTranscription {
        stream_id: String,
    },
    StartSynthesis {
        intent_id: String,
        text: String,
        voice_id: String,
    },
    SynthesisStatus {
        run_id: String,
    },
    ReadSynthesis {
        run_id: String,
        offset: u64,
        max_bytes: u32,
    },
    CancelSynthesis {
        run_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechComputerRequest {
    pub binding: SpeechComputerBinding,
    pub operation: SpeechOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechDenial {
    GrantDenied,
    GrantExpired,
    RouteRetired,
    Busy,
    UnsupportedModel,
    InvalidSequence,
    Backpressure,
    InferenceFailed,
    TimedOut,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpeechComputerReply {
    TranscriptionOpened {
        stream_id: String,
        model: String,
    },
    /// Current full text snapshot. The execution adapter derives UI deltas.
    Transcript {
        stream_id: String,
        sequence: u64,
        text: String,
        model: String,
        audio_duration_ms: u64,
        is_final: bool,
    },
    TranscriptionCancelled {
        stream_id: String,
    },
    SynthesisStarted {
        run_id: String,
        model: String,
    },
    SynthesisPending {
        run_id: String,
    },
    /// Metadata is not an audio-readiness verdict. The source still verifies
    /// the complete bytes/hash and parses playable WAV before publishing.
    SynthesisReady {
        run_id: String,
        model: String,
        audio_sha256: String,
        audio_bytes: u64,
        sample_rate_hz: u32,
        audio_duration_ms: u64,
    },
    SynthesisChunk {
        run_id: String,
        offset: u64,
        total_bytes: u64,
        hex: String,
    },
    SynthesisCancelled {
        run_id: String,
    },
    Denied {
        reason: SpeechDenial,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    binding: SpeechComputerBinding,
    reply: SpeechComputerReply,
}

fn ensure(ok: bool, message: &str) -> io::Result<()> {
    if ok {
        Ok(())
    } else {
        Err(invalid(message))
    }
}
fn opaque(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn hex_bytes(value: &str, cap: usize, empty: bool) -> io::Result<usize> {
    ensure(
        (empty || !value.is_empty())
            && value.len() % 2 == 0
            && value.len() / 2 <= cap
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid bounded speech hex",
    )?;
    Ok(value.len() / 2)
}
fn bounded(value: &Value) -> io::Result<()> {
    ensure(
        serde_json::to_vec(value).map_err(io::Error::other)?.len() <= MAX_WIRE_BYTES,
        "speech envelope exceeds its budget",
    )
}
impl SpeechComputerRequest {
    pub fn validate(&self) -> io::Result<()> {
        let b = &self.binding;
        ensure(
            [
                &b.source_instance_id,
                &b.target_instance_id,
                &b.computer_id,
                &b.owner_user_id,
                &b.grant_id,
                &b.model,
            ]
            .into_iter()
            .all(|s| opaque(s))
                && b.grant_revision > 0,
            "invalid speech binding",
        )?;
        let expected = match &self.operation {
            SpeechOperation::OpenTranscription {
                intent_id,
                sample_rate_hz,
            } => {
                ensure(
                    opaque(intent_id) && *sample_rate_hz == 16_000,
                    "speech stream requires mono PCM16 at16kHz",
                )?;
                SpeechWorkload::Transcription
            }
            SpeechOperation::Append {
                stream_id,
                sequence,
                pcm16_hex,
            } => {
                let bytes = hex_bytes(pcm16_hex, 3200, false)?;
                ensure(
                    opaque(stream_id) && *sequence > 0 && bytes % 2 == 0,
                    "invalid100ms speech chunk or sequence",
                )?;
                SpeechWorkload::Transcription
            }
            SpeechOperation::Finish {
                stream_id,
                sequence,
            } => {
                ensure(opaque(stream_id) && *sequence > 0, "invalid speech finish")?;
                SpeechWorkload::Transcription
            }
            SpeechOperation::CancelTranscription { stream_id } => {
                ensure(opaque(stream_id), "invalid speech stream")?;
                SpeechWorkload::Transcription
            }
            SpeechOperation::StartSynthesis {
                intent_id,
                text,
                voice_id,
            } => {
                ensure(
                    opaque(intent_id)
                        && opaque(voice_id)
                        && !text.trim().is_empty()
                        && text.len() <= 4096,
                    "invalid bounded speech synthesis",
                )?;
                SpeechWorkload::Synthesis
            }
            SpeechOperation::SynthesisStatus { run_id }
            | SpeechOperation::CancelSynthesis { run_id } => {
                ensure(opaque(run_id), "invalid synthesis run")?;
                SpeechWorkload::Synthesis
            }
            SpeechOperation::ReadSynthesis {
                run_id,
                offset,
                max_bytes,
            } => {
                ensure(
                    opaque(run_id)
                        && *offset <= MAX_AUDIO_BYTES
                        && (1..=MAX_READ_BYTES).contains(max_bytes),
                    "invalid bounded synthesis read",
                )?;
                SpeechWorkload::Synthesis
            }
        };
        ensure(
            b.workload == expected,
            "speech operation differs from granted workload",
        )
    }
    fn validate_reply(&self, reply: &SpeechComputerReply) -> io::Result<()> {
        use SpeechComputerReply as R;
        use SpeechOperation as O;
        let model = &self.binding.model;
        let valid = match (&self.operation, reply) {
            (_, R::Denied { .. }) => true,
            (
                O::OpenTranscription { .. },
                R::TranscriptionOpened {
                    stream_id,
                    model: actual,
                },
            ) => opaque(stream_id) && actual == model,
            (
                O::Append {
                    stream_id,
                    sequence,
                    ..
                },
                R::Transcript {
                    stream_id: actual,
                    sequence: seq,
                    text,
                    model: actual_model,
                    audio_duration_ms,
                    is_final,
                },
            ) => {
                actual == stream_id
                    && seq == sequence
                    && !is_final
                    && actual_model == model
                    && text.len() <= 8192
                    && *audio_duration_ms <= 15_000
            }
            (
                O::Finish {
                    stream_id,
                    sequence,
                },
                R::Transcript {
                    stream_id: actual,
                    sequence: seq,
                    text,
                    model: actual_model,
                    audio_duration_ms,
                    is_final,
                },
            ) => {
                actual == stream_id
                    && seq == sequence
                    && *is_final
                    && actual_model == model
                    && text.len() <= 8192
                    && *audio_duration_ms <= 15_000
            }
            (
                O::CancelTranscription { stream_id },
                R::TranscriptionCancelled { stream_id: actual },
            ) => actual == stream_id,
            (
                O::StartSynthesis { .. },
                R::SynthesisStarted {
                    run_id,
                    model: actual,
                },
            ) => opaque(run_id) && actual == model,
            (O::SynthesisStatus { run_id }, R::SynthesisPending { run_id: actual }) => {
                actual == run_id
            }
            (
                O::SynthesisStatus { run_id },
                R::SynthesisReady {
                    run_id: actual,
                    model: actual_model,
                    audio_sha256,
                    audio_bytes,
                    sample_rate_hz,
                    audio_duration_ms,
                },
            ) => {
                actual == run_id
                    && actual_model == model
                    && hex_bytes(audio_sha256, 32, false).is_ok_and(|n| n == 32)
                    && (1..=MAX_AUDIO_BYTES).contains(audio_bytes)
                    && *sample_rate_hz == 24_000
                    && (1..=40_960).contains(audio_duration_ms)
            }
            (
                O::ReadSynthesis {
                    run_id,
                    offset,
                    max_bytes,
                },
                R::SynthesisChunk {
                    run_id: actual,
                    offset: at,
                    total_bytes,
                    hex,
                },
            ) => {
                actual == run_id
                    && at == offset
                    && *total_bytes <= MAX_AUDIO_BYTES
                    && hex_bytes(hex, *max_bytes as usize, true).is_ok_and(|n| {
                        offset
                            .checked_add(n as u64)
                            .is_some_and(|end| end <= *total_bytes)
                            && (n > 0 || offset == total_bytes)
                    })
            }
            (O::CancelSynthesis { run_id }, R::SynthesisCancelled { run_id: actual }) => {
                actual == run_id
            }
            _ => false,
        };
        ensure(valid, "speech reply differs from the exact request")
    }
}

/// The receiver adds current grant/connection authority before using this.
pub struct VerifiedSpeechRequest {
    request: SpeechComputerRequest,
    sender: String,
    recipient: String,
    scope: String,
    nonce: String,
}
impl VerifiedSpeechRequest {
    pub fn request(&self) -> &SpeechComputerRequest {
        &self.request
    }
    pub fn sender(&self) -> &str {
        &self.sender
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    /// Only within the host's current signing-identity fence. Its native
    /// publication guard must revalidate the same grant/connection afterward.
    pub fn reply(
        &self,
        identity: &SigningIdentity,
        reply: SpeechComputerReply,
    ) -> io::Result<Value> {
        ensure(
            identity.public_identity() == self.recipient,
            "speech issuer changed",
        )?;
        self.request.validate_reply(&reply)?;
        let data = serde_json::to_value(Response {
            binding: self.request.binding.clone(),
            reply,
        })
        .map_err(io::Error::other)?;
        let value = serde_json::to_value(identity.sign(Body {
            version: 1,
            sender: self.recipient.clone(),
            recipient: self.sender.clone(),
            scope_id: self.scope.clone(),
            nonce: self.nonce.clone(),
            kind: REPLY_KIND.into(),
            data,
        })?)
        .map_err(io::Error::other)?;
        bounded(&value)?;
        Ok(value)
    }
}
pub fn verify_request(
    value: Value,
    recipient: &str,
    scope: &str,
) -> io::Result<VerifiedSpeechRequest> {
    bounded(&value)?;
    let envelope: Envelope = serde_json::from_value(value).map_err(io::Error::other)?;
    verify(&envelope, recipient, scope, REQUEST_KIND)?;
    let request: SpeechComputerRequest =
        serde_json::from_value(envelope.body.data).map_err(io::Error::other)?;
    request.validate()?;
    Ok(VerifiedSpeechRequest {
        request,
        sender: envelope.body.sender,
        recipient: recipient.into(),
        scope: scope.into(),
        nonce: envelope.body.nonce,
    })
}
pub struct SignedSpeechRequest {
    pub envelope: Value,
    request: SpeechComputerRequest,
    sender: String,
    recipient: String,
    scope: String,
    nonce: String,
}
impl SignedSpeechRequest {
    pub fn new(
        identity: &SigningIdentity,
        recipient: &str,
        scope: &str,
        request: SpeechComputerRequest,
    ) -> io::Result<Self> {
        request.validate()?;
        public_key(recipient)?;
        ensure(opaque(scope), "invalid speech authority scope")?;
        let nonce = super::handoff_wire::fresh_nonce()?;
        let sender = identity.public_identity();
        let envelope = serde_json::to_value(identity.sign(Body {
            version: 1,
            sender: sender.clone(),
            recipient: recipient.into(),
            scope_id: scope.into(),
            nonce: nonce.clone(),
            kind: REQUEST_KIND.into(),
            data: serde_json::to_value(&request).map_err(io::Error::other)?,
        })?)
        .map_err(io::Error::other)?;
        bounded(&envelope)?;
        Ok(Self {
            envelope,
            request,
            sender,
            recipient: recipient.into(),
            scope: scope.into(),
            nonce,
        })
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    pub fn verify_reply(&self, value: Value) -> io::Result<SpeechComputerReply> {
        bounded(&value)?;
        let envelope: Envelope = serde_json::from_value(value).map_err(io::Error::other)?;
        verify(&envelope, &self.sender, &self.scope, REPLY_KIND)?;
        ensure(
            envelope.body.sender == self.recipient && envelope.body.nonce == self.nonce,
            "speech reply issuer or nonce mismatch",
        )?;
        let response: Response =
            serde_json::from_value(envelope.body.data).map_err(io::Error::other)?;
        ensure(
            response.binding == self.request.binding,
            "speech grant or route changed",
        )?;
        self.request.validate_reply(&response.reply)?;
        Ok(response.reply)
    }
}

#[cfg(test)]
#[path = "speech_wire_tests.rs"]
mod tests;
