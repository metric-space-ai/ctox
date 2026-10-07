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

To select the existing Mistral account, an authorized native handler saves:

```rust
SpeechRuntimeConfig {
    synthesis: SpeechBackend::Mistral,
    transcription: SpeechBackend::Mistral,
    voice_id: None, // or an existing approved preset/saved voice
}.save(root)?;
```

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
- `open_transcription(PcmFormat)` connects an authenticated provider WebSocket
  and confirms session creation/update. Input is raw signed16 little-endian mono
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

Source inventory (2026-10-07): native Voxtral STT exposes a whole-file
transcription operation; its runtime streaming implementation returns false.
Native Voxtral TTS reports synthesis not wired and fails closed. Existing Piper
and Qwen runtime selections require separately healthy installed local models;
a catalog entry is not proof of working audio.

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
