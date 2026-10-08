# Native local speech RPC

The execution SpeechGateway uses the configured local speech models. Remote
computer inference reuses the running native control-only CTOX Sync pool;
the signed wire contract lives in
`ctox_sync::authority::auth::speech_wire` and method
`ctox.native.speech.v1`. It introduces no account, credential transfer,
HTTP data bridge, independent peer or runtime environment switch.

## Contract and authority

`SignedSpeechRequest::new` pins the target signing identity and authority scope.
Its request binds the source instance, target instance/computer, owner, speech
grant ID/revision, workload and exact model. `verify_request` returns a private,
non-deserializable verified envelope. This verifies authorship only.

The native receiver must independently compare the binding and authenticated
sender with its CURRENT speech-only grant in the native store. Build or storage
permission is insufficient. It must bind the exact admitted native connection,
retain a bounded intent/stream ledger, validate operation sequence and expiry,
and use a mandatory publication guard that rechecks that same grant/connection
before returning any audio or text. A grant revision change invalidates the
prepared response; obtaining a fresh grant must not bless an old response.
The host's current encrypted signing identity signs the reply. The source
verifies issuer, scope, nonce, exact binding, model, operation and byte range.

The source meeting adapter still performs current Owner/peer/session/project/
Supervisor/live-meeting/deck checks before and after inference. Wire signatures
do not prove meeting membership or replace Crew's live binding and verified
producer receipts.

## Bounded operations

STT accepts mono signed little-endian PCM16 at16kHz, at most100ms/3200bytes per
Append. Open uses a stable intent ID, then Append and Finish use the server
stream ID and explicit sequence. Retries of an already accepted sequence must
return its ACK without appending twice; gaps or different content at the same
sequence fail. The host retains intent tombstones until grant expiry so a
delayed Open replay cannot create a second stream. Cancellation, connection
retirement, lease expiry or host shutdown closes its owned runtime stream.
The current model's15s utterance bound remains enforced by the execution host.

Replies carry full current transcript snapshots; the source derives deltas for
the existing gateway event contract. An Append reply cannot impersonate a Final.

TTS Start uses a stable intent ID, bounded4096-byte text and saved voice.
Status returns pending or complete metadata. Audio transfers in at most8192-byte
reads, with exact offset and total length, up to8MiB. The source validates the
complete hash and actual playable WAV/duration before native file publication;
a Ready DTO alone is insufficient. Cancel releases the owned run/artifact.
The complete signed envelope budget is32KiB, including JSON and hex expansion.
Denied replies contain typed native failure classes, not provider-auth guesses.

## Delivery boundary

This PR supplies protocol framing, request/reply validation and cryptographic
regressions. Architecture owns the thin running-ServiceHost/control-pool seam;
Models owns current speech grant admission, runtime proxy and real GPU inference.
It does not register a public handler, create a grant, start a model or establish
installed meeting readiness. Those integrations and measured sentence-end to
transcript latency remain required before goal20 can pass.
