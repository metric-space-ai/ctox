# Jour fixe gateway speech transport

The renderer uses the selected guest's existing `requestProjectControl` port.
Browser hosts may supply the same Shell control port to
`openJourFixeGatewaySpeech`; desktop uses the existing preload port. No Rust
symbol, subprocess, credential or HTTP data endpoint is exposed to the renderer.

Shell action: `project.jour_fixe.speech`. Native WebRTC method:
`ctox.workjet.jour_fixe.speech.v1`, capability
`ctox-workjet-jour-fixe-speech-v1`. The Shell supplies its trusted native
instance ID. The native adapter authenticates the current accepted connection
and original capability, existing business_commands write permission, project
Owner, live meeting, immutable Supervisor binding and narrated deck revision.

Operations:

- `open`: UI UUID requestId; immutable projectId/meetingId/deckRevision.
  A repeated nonce on the same connection/scope returns the same owned stream.
- `write`: streamId, increasing sequence, base64 mono PCM16LE at 16 kHz,
  at most 3200 bytes (100 ms). The last identical sequence is idempotent.
  A conflicting/omitted sequence or overflow cancels the sentence.
- `read`: streamId and afterSequence cursor. Partial text is a delta, never a
  persisted transcript. Main accumulates it per stream into its full UI snapshot.
- `finish`: after the final frame, once per sentence. Returns finishing while
  draining the real gateway final. A read becomes committed only after the
  private BoundTranscription receipt and domain transcript commit.
- `cancel`: discard pending audio and partials. Scope, meeting/deck or connection
  retirement also cancels. Cancellation winning the native commit fence prevents
  final admission; an already committed turn remains durable.

The committed reply has only a random private handle and meetingRevision, never
caller text or a serializable producer proof. Workjet keeps that handle in a
non-serializable branded receipt and exposes `isJourFixePrivateFinalReceipt`.
Main invokes its committed callback only after finish resolves with that receipt,
then refreshes the ordinary native meeting projection. It must not send a
Speech turn through Owner-text append or local_candidate.

Limits: two active gateway sentences per native pool, eight queued commands,
15 s PCM, 32 replayable partials / 64 KiB retained text, 5 s audio idle deadline,
15 s final drain, 40 s overall stream lifetime. Connection/authority/deck checks
run around each operation and at 250 ms intervals; guarded physical publication
retains native issuer/policy/connection authority. Runtime model/voice/credentials
remain in existing typed configuration and secret stores.

Main owns capture permission, resampling, VAD and 100 ms framing. A room scope
transition aborts its AbortSignal. The existing room mic remains unavailable
until this transport is installed, authorized and wired. This source contract
does not establish installed speech or the <=~1.5 s sentence-end acceptance.

## WELSCH production prerequisites

The measured public state at 2026-10-08T12:02:37.953657Z is native
`native-main-11ac94eeb045`, PID 70754, state root
`/home/ctox/.local/state/ctox`. Normal `ctox sync status` reported
`configured:false` / `listener:inactive`; `sync identity` exited 1.
The actual source instance, public Sync signing identity and scope remain
unknown. The canonical Owner from existing project evidence is
`196a89ba-ee86-4413-885c-04ca60e6f291`; that does not supply a source identity.

The published `native-main-7c94f7aab837` (includes #455) is not evidence of
WELSCH activation or of an installed renderer speech ingress. Instances owns
the real source/registered-target binding dependency. The existing writer
applies reviewed source/target configuration through normal Owner authority
and the local secret store only after those real identities are supplied,
preserving current grants and keys. Never substitute a tenant workspace,
initialize a synthetic production identity or read a denied computer collection
through SQL/SSH.

Keep the production mic unavailable until source configuration, target binding,
installed ingress and room capture/VAD are verified. Classify this measured
state as missing transport configuration, never as a provider credential failure
or successful production speech. Isolated Models bootstraps do not establish
production readiness.

Evidence:
`~/.codex/task-evidence/devops/welsch-speech-public-host-metadata-20261008.json`.
