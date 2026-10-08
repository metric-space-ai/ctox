# Native speech computer adapter

The speech gateway now supports the explicit `computer` backend for
transcription and asynchronous synthesis. This source adapter borrows the
existing native control host; it creates no peer, provider account, HTTP
service or provider credential.

`SpeechGateway::open_transcription(PcmFormat)` keeps the same owned bounded
stream and producer-only final receipts. At16kHz, every100ms PCM chunk receives
a signed sequence ACK from the selected computer. Full remote snapshots become
UI deltas; a partial cannot become a verified final. Flush does not finish a
sentence. Finish latency includes the queued audio backlog and native network
round trip. The room still measures capture/VAD sentence-end separately.

`SpeechGateway::synthesize_verified_async(&SpeechRequest)` is the typed
narration/answer call. Existing synchronous local/cloud adapters run on a
blocking worker. The computer backend starts a bounded run, polls its signed
status, then reads bounded chunks. It verifies the complete hash, RIFF size,
mono PCM16/24kHz format and actual duration before minting a producer receipt.
The meeting adapter still authorizes the caller, stores its native file and
binds it to the exact meeting/project/deck/slide before exposing audio readiness.

## Operator configuration

Use the existing local Owner/operator CLI, not a browser data endpoint:
`ctox runtime speech-computer-configure <routes.json>`.
The typed document has optional `transcription` and `synthesis` routes.
Each route contains:
- `scope_id`, matching the configured native host;
- `native_peer_route`, an already accepted native peer route;
- independent `source_signing_identity` and `target_signing_identity` pins;
- `binding`: exact source instance, target instance, computer, owner,
  speech-only grant ID/revision, workload and model;
- `expires_at_unix_ms`, at most one day ahead.

The local source pin must match the provisioned secret-store identity.
Configuration saves under its existing issuer fence into SQLite and mints a
new private local authority epoch. Saving empty routes revokes the old source
configuration. Every physical send poll and response ingestion rechecks that
epoch, expiry and current issuer. Build permissions grant no speech access.
The remote computer independently needs a current speech-specific target
grant for exactly that binding and sender. This client cannot create it.

Select `{"synthesis":"computer","transcription":"computer","voice_id":"neutral_female"}`
through `ctox runtime speech-configure <speech-config.json>`.
The CLI speech-synthesize path uses the asynchronous producer call and outputs
non-secret run/audio hash metadata. Unsupported formats fail locally; no
fallback model or cloud account is selected.

Dropping or cancelling a stream/run sends bounded best-effort Cancel through
the same current route. An unavailable/revoked route cannot reconnect to
another host. Target-side lease expiry must reclaim effects whose ACK or
Cancel was lost. Each operation has a30s native control deadline; synthesis
poll/read has a120s overall deadline. No microphone/audio provider key is
persisted by this adapter.

## Delivery boundary

This PR supplies the source adapter, source route lifecycle and failure
regressions. The speech-specific receiver with current target grants, durable
intent/sequence tombstones and local runtime execution is the next Models
slice. It must register `ctox.native.speech.v1` on the same native host.
Until that receiver is installed, this change alone does not enable remote
speech. Installed two-host audio, intelligibility and latency acceptance remain
open; fixture tests and signed metadata are not installed product evidence.
