# Native workload control channel

Execution adapters can borrow the existing running native control host with
`crate::sync_host::native_control_channel(runtime_root)`, or use
`ServiceHost::native_control_channel()`. Neither call starts a host, discovers
a new peer, provisions an account or grants permission. The handle retains no
pool or host authority after retirement.

`NativeControlChannel::request(route, target_signing_identity, method,
signed_envelope, deadline, source_publication_guard, reply_verifier)` runs
on the existing host runtime and uses one exact accepted connection. Methods
must equal `ctox.native.speech.v1` (Models #416), or be in the reserved
`ctox.sync.workload.` namespace; authority, collection and identity methods
cannot be routed through this adapter. Requests are at most32KiB, responses
128KiB, deadlines at most30s, with32 simultaneous calls and immediate typed
capacity failure. The adapter's current source grant guard is mandatory at
physical send polls and response ingestion. The mandatory
`NativeControlReplyVerifier` authenticates the response against the independent
target signing pin and fresh request nonce. A route/room handshake is no proof
of that identity. Raw JSON is never an execution or speech grant.

`register_handler(method, GuardedAuxiliaryRequestHandler<WebRTCRsConnection>)`
installs a receiver on that same pool. The workload owner must validate the
signed sender, nonce/replay state, exact workload/model and current target
grant before effects, then return `GuardedAuxiliaryResponse` with its own
current publication guard. The host wraps that guard with its lifetime and
exact connection fence; wire fields cannot opt into an unguarded response.
Handler registration is unique and cannot replace another owner.

ServiceHost drop invalidates the channel synchronously before signaling stop.
Runtime exit/unwind also retires it. Pending calls race retirement/disconnect
and their deadline; dropping a call aborts its owned I/O task. Retired handles
cannot switch to a replacement host/connection. Workload cancellation of an
already accepted remote effect is the workload protocol's responsibility.

Models owns speech grants, signed Open/Append/Finish/Cancel and TTS
start/status/read/cancel, sequence ACKs and audio backpressure. Build grants
and EndpointUse::Build confer no speech authority. This seam provides bounded
single-response RPC, not a second WebRTC pool or an audio service. No business
records or model credentials are copied through an HTTP fallback.

The focused tests exercise real empty native pool registration, missing
peers, bounds, namespace rejection, duplicate handler ownership, host
replacement and retirement. They do not prove an installed two-host speech
workflow or authenticate a mocked endpoint as a production peer.
