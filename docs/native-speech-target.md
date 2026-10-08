# Native speech computer and target policy

The local operator configures a bounded `SpeechTargetConfig` through
`ctox runtime speech-computer-authorize <grants.json>`. It stores no provider
credential. Each of up to eight grants binds the exact source signing identity,
current target identity, native scope, source/target instance, computer, owner,
speech workload, model and grant revision. Grants expire within one day and
only permit the two approved local Voxtral models.

Saving uses the provisioned native identity's existing secret mutation fence,
creates a new private configuration epoch and leaves all identities and other
client tokens intact. Saving an empty grant list revokes the previous target
authority. Signature verification uses the current target identity and scope;
current policy independently compares every binding field and source pin.

The native Sync host registers `ctox.native.speech.v1` on its existing control
pool. It creates no HTTP endpoint or additional pool, and forwards no provider
credential. The source adapter uses the same channel and independently verifies
the target's pinned reply identity. Target private IPC runs only when the
existing speech runtime selects the local runtime and its auxiliary binding
matches the exact approved model. No cloud or alternative model fallback runs.

Before an Open or synthesis Start performs an effect, the receiver reserves
its intent through verified target policy. The bounded SQLite runtime ledger
stores request digests and opaque object IDs, not PCM, raw text, voices or
credentials. A retry resolves to the same live host generation and exact
native connection. Changed content is rejected. Host restart tombstones the
old intent and cannot restart it. Expired claims are reclaimed; live claims
are never evicted to admit a replay.

Every private IPC read/write poll and native reply publication rechecks current
grant configuration, issuer, expiry, live host and exact native connection.
Pending I/O retains no authority locks. Revocation closes the next private
I/O poll; host drop or object removal aborts owned synthesis tasks and closes
streams. Already-admitted native GPU compute may finish its current bounded
operation, but its result cannot be ingested or published after revocation.
No model daemon or foreign GPU job is stopped.

A computer admits at most two active speech operations. STT accepts ordered
100 ms PCM16 frames at 16 kHz, at most 15 s per utterance, with matching retry
digests and a cached Finish response. Finish releases its active permit.
TTS has a 120 s deadline and an 8 MiB audio bound; actual WAV bytes, PCM format,
duration and SHA-256 are checked before publication. The source releases the
target's audio buffer after verified transfer. Streams expire after 90 s;
synthesis artifacts after 150 s. Disconnected peers and revoked grants are reaped within
approximately two seconds, releasing their active permits. Cached objects are separately bounded to 64.

The operator speech-synthesize and speech-benchmark CLI commands start the
configured native host in their own process when the computer backend is
selected. They await its existing peer attachment for at most15s before audio
timing and drop the host on completion. Use an isolated operator root without
another running native host; the directory lease refuses a duplicate host.
Readiness creates no peer or grant and never retries an inference effect.

This is a native transport implementation. Installed two-host meeting
acceptance, microphone sentence-end latency and first TTS playback latency
remain required product evidence; unit tests do not establish those results.
