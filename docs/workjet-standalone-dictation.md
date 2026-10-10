# Standalone Workjet draft dictation

The Business OS guest exposes `speech.dictation` through its existing
`workjetProjectControl` port, independently of projects and Jour fixe meetings.
It uses the configured `SpeechGateway::open_transcription` backend
(Mistral, an authorized computer route, or the selected local runtime).
No environment setting, provider override, new credential or HTTP data bridge
is introduced.

The general native computer transport `ctox.native.speech.v1` already exists,
but it is a source-to-computer runtime API, not a renderer permission.
The standalone renderer transport is `ctox.workjet.speech.dictation.v1` with
capability `ctox-workjet-speech-dictation-v1`. The installed native peer and
Business OS shell must both contain this change.

## Renderer contract

Every request has `action: "speech.dictation"`, a fresh UUID `commandId`,
and one operation. It matches Workjet PR292's `workjetDictation.ts`.

| op | Additional fields |
| --- | --- |
| open | none |
| write | streamId (UUID), sequence (positive integer), pcmBase64 |
| read | streamId (UUID), afterSequence (nonnegative integer) |
| finish | streamId (UUID) |
| cancel | streamId (UUID) |

The guest supplies the selected native instance as private
`scope: {instanceId}`. Renderer-supplied scope, project, meeting, deck,
transcript, receipt, model, provider, credential and auto-send fields are
rejected. All responses echo the exact action, commandId, op and streamId.

```ts
type DictationResponse = {
  action: "speech.dictation";
  commandId: string;
  op: "open" | "write" | "read" | "finish" | "cancel";
  streamId: string;
  state: "open" | "finishing" | "finished" | "canceled" | "failed";
  events: Array<{ sequence: number; text: string }>;
  text: string | null;
  error: string | null;
};
```

`events` contains at most one coalesced **full partial text snapshot**, with a
monotonic event cursor; replace the partial display rather than accumulating
these snapshots as deltas. A slow reader can use cursor0 without replaying
old deltas or losing the final. Input PCM sequence and output event sequence
are separate. `text` is present only in state `finished` and comes from the
complete verified gateway final. It is transient draft text, not a meeting
receipt and not permission to append a transcript or send a message.

PCM is signed16-bit little-endian mono at16000Hz:1–3200bytes per write,
even length (up to100ms), at most60seconds of audio per recording.
Sequence starts at1; an identical retry of the latest sequence is idempotent.
A conflicting or skipped sequence terminates that recording.
Retrying `open` with the same commandId on the same current peer/token/instance
returns the retained stream and never starts a second backend.

## Authority, cancellation and bounds

The server binds each stream to the actual authenticated current peer, its
capability token and native instance. Ordinary current command-write
permission is required to consume the configured STT backend; this check
does not write a command. Current actor/device revocation and policy are
rechecked before opening, on operations, every250ms during the stream and
under the native response publication fence. Changed transcription backend
or Mistral credential retires the stream. The computer backend retains its
existing exact route/grant checks.

The consumer owns immutable draft/thread and selected-instance scope. On
scope change, composer retirement, microphone cancellation or abort it calls
`cancel`, including for an open response arriving after UI cancellation.
Cancellation clears partial and final text, fences final admission, and
notifies the owned backend. A successful final is never persisted server-side.
No meeting binding, speech receipt staging, business command or auto-send
occurs in this path.

There are2active streams and at most32retained stream entries per native
peer host; receipts expire100seconds after opening. Idle capture expires
after5seconds, the whole stream after80seconds, and finishing after15seconds.
Native open waits at most15seconds; queued PCM acknowledgement2seconds.
The guest call deadline is20seconds for open and5seconds for other operations.
All failures use fixed classes: missing_credential, configuration_unavailable,
backend_unavailable, credentials_rejected, access_denied, rate_limit, quota,
provider_rejected, transport, backpressure, invalid_response, timeout,
retired, invalid_sequence_or_audio, invalid_finish or missing_final.
Preflight/authority failures are never described as upstream rejection.
Provider bodies, keys, PCM and transcript text are never included in errors.

## Verification and acceptance

Native controlled-stream tests exercise a real WebSocket gateway pump with
no project/meeting configured, the verified final, command/stream correlation,
idempotent audio, conflicting audio, cancellation, backend changes, actor
revocation, idle expiry and unchanged meeting/command records. Browser unit
tests exercise the actual guest adapter and reject forged or mismatched wire
values. These fixtures do not measure installed Mistral/computer latency.
Installed acceptance belongs to the combined native/shell and Composer release:
dictate into a draft, verify text, cancel/switch drafts, and confirm no
meeting transcript or chat message was created.
