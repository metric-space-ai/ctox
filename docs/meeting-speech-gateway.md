# Meeting speech gateway

Jour-fixe tools and native Workjet use `execution::speech::SpeechGateway`.
The calling meeting layer checks actor/project permissions, owns microphone consent,
stores audio artifacts and transcript events, and exposes those records through
CTOX DB / WebRTC. This module performs inference only. It does not expose an HTTP
collection bridge, browser API key, or public unauthenticated audio endpoint.

## Runtime configuration

`SpeechRuntimeConfig::load/save(root)` uses the existing SQLite payload store,
key `speech_gateway`. Only an authorized Owner/Admin configuration handler may
call save. Default stays with existing local TTS runtime; streaming STT fails
explicitly until a supported backend is configured. There is no new process-env
switch and no automatic paid-provider substitution.

The approved Jour-fixe setup uses local speech, with no paid account. Select
`engineai/Voxtral-Mini-4B-Realtime-2602` for transcription and
`engineai/Voxtral-4B-TTS-2603` for synthesis in the existing local-inference
runtime configuration. Select the admitted GPU through its typed runtime plan;
do not introduce process environment switches. The meeting layer uses the same
`SpeechGateway` contract. Saving speech backend selection does not download
weights, acquire a GPU, or establish a remote-computer route.

```json
{"synthesis":"runtime","transcription":"runtime","voice_id":null}
```

Private local IPC remains the runtime boundary. A meeting host on a different
computer also needs an authenticated managed-computer route to this gateway;
a filesystem socket path on gpu3 is not a usable remote route on the host.
That installed routing path and its latency must be proved separately.

Mistral is an optional fallback only when the owner supplies an account/key.
To select it, an authorized native handler saves:

```rust
SpeechRuntimeConfig {
    synthesis: SpeechBackend::Mistral,
    transcription: SpeechBackend::Mistral,
    voice_id: Some("<approved saved voice ID>".into()),
}.save(root)?;
```

A local operator with authority over the CTOX runtime root can also run
`ctox runtime speech-configure <speech-config.json>` with the same typed fields:

```json
{"synthesis":"mistral","transcription":"mistral","voice_id":"<approved saved voice ID>"}
```

The entire document (at most4096bytes) is parsed and validated before persistence.
Unknown fields, credential values and invalid backend names are rejected. This
command neither writes credentials nor starts paid inference. Remote meeting
handlers still require their existing Owner/Admin authorization before saving.

Mistral TTS in this adapter requires a saved voice ID, in configuration or on the
request. Missing voice returns `MissingVoice` before any provider request; the
adapter does not upload reference audio or assume a default voice. Speech status
reports voice configuration and credential presence separately, neither proves
that the provider accepted a call.

Mistral credentials resolve through the existing encrypted
`CTOX_MISTRAL_API_KEY` / `MISTRAL_API_KEY` runtime path. The config and status
contain no secret value. Neither a chat subscription nor primary-chat API model
implies access to paid speech.

## Calls and lifecycle

- `synthesize(&SpeechRequest)` returns native audio bytes, format, actual model,
  input character count and full request latency. Store the resulting slide or
  reply audio as an artifact. Runtime backend reuses the existing communication
  adapter and private local IPC. Mistral returns JSON base64 audio; decode it
  before playback, never label that JSON as a WAV.
- `open_transcription(PcmFormat)` connects the selected private runtime IPC
  or authenticated provider WebSocket and confirms stream readiness. Input is raw signed16 little-endian mono
  PCM (default16kHz), at most100ms per chunk; recommended capture cadence20ms.
- `append_pcm` / `flush` deliver audio as it arrives. `next_event` emits
  ordered partial text fragments, then exactly one final utterance transcript.
  Partial text is a delta, not a replacement of the complete transcript.
- `finish_audio` marks the producer's sentence end and sends flush/end.
  The final event reports elapsed time from that mark. Reopen a stream for the
  next utterance; flush alone is not a provider sentence-final event.
- `cancel` closes within a bounded deadline; dropping the stream aborts its
  one owned task and drops its provider socket. No detached daemon persists.
- Input8/output32 bounded queues fail explicitly on backpressure. Chunk/frame,
  transcript and audio limits bound memory. Provider message/error bodies are
  not returned as error text. HTTP status remains available for actionable
  authentication, rate-limit or quota errors.

Use the daemon's existing Tokio runtime. Workjet and meeting tools must retain
one owner per utterance and cancel when the meeting ends.

## Measurements and acceptance

`ctox runtime speech-status` reports selected configuration and credential
presence, not a successful provider call. `ctox runtime speech-benchmark
<fixture.pcm>` explicitly replays at most15s of 16kHz mono PCM at20ms cadence and
reports whether partial text arrived before capture ended and gateway
finish-to-final latency. A file uploaded after recording is not streaming.

The gateway measurement excludes microphone capture, VAD delay and the client
return transport. Meeting acceptance additionally measures capture sentence-end
to rendered/persisted transcript in Workjet; it must not claim the full <1.5s
target from a fixture, advertised model latency or batch realtime factor.

Installed baseline inventory (2026-10-07, before the local speech PRs): native
Voxtral STT exposes a whole-file transcription operation and does not advertise
streaming readiness; native Voxtral TTS reports synthesis not wired and fails
closed. Existing Piper and Qwen selections require separately healthy installed
models. No catalog entry, model file, or successful build proves working audio.

The local STT candidate adds a bounded persistent-decoder stream over the same
private IPC (16kHz mono, utterances at most15s). It recomputes causal encoder
features for the growing utterance; it does not claim an incremental encoder.
Partial snapshots become ordered text deltas at the gateway boundary. Final
health acceptance still requires an actual model/audio streaming proof rather
than the whole-file smoke test. Linux CUDA is an explicit build capability;
requesting an unavailable backend fails before model loading without a CPU
substitution. See the model crate README for a paced direct-model benchmark;
that benchmark is explicitly separate from installed gateway/meeting acceptance.

The native TTS candidate returns a complete24kHz WAV after synthesis. Its first
available audio is therefore completion latency; it must not be reported as
streamed first audio. Real GPU latency, concurrent STT/TTS memory use, and
installed meeting-host routing remain unmeasured until their evidence is filed.

The implemented Mistral routes are
`voxtral-mini-transcribe-realtime-2602` (WebSocket STT) and
`voxtral-mini-tts-2603` (speech generation). Official listed prices inspected
2026-10-07: realtime STT $0.006/audio minute, file Transcribe2 $0.003/minute,
TTS $16/million characters. These are list prices, not a measured customer bill;
local provider fee is zero, local compute cost is not measured.

Sources:
- https://docs.mistral.ai/studio/audio/speech_to_text/realtime_transcription
- https://docs.mistral.ai/models/voxtral-mini-transcribe-realtime-26-02
- https://docs.mistral.ai/studio/audio/text_to_speech/speech
- https://docs.mistral.ai/inference/pricing

No real provider latency or installed meeting acceptance has yet been established.
The WebSocket regression fixture verifies protocol, ordering, timing boundaries,
cancellation and privacy; its injected45ms delay is not an inference measurement.
