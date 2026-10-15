# Candidate: executable unary and streaming plugin transport

Depends on the quota RPC candidate (#752). This adds the previously missing
`PluginLoader` implementation on top of CTOX's existing authenticated local
process supervisor. It does not activate plugins in the installed daemon,
promote the accepted upstream revision, or claim live provider quota support.

The caller explicitly supplies the runtime directory, executable artifact and
host callback router. Each open has a unique private local endpoint; the child
receives no inherited environment and gets its one-shot handshake token through
stdin. Calls use a bounded queue and a response deadline that includes queue
waiting. Malformed caller input is rejected before dispatch. A dropped call,
protocol failure or expired response ends the child and closes the queue because
a partially consumed length-prefixed frame cannot safely be reused.

Host callbacks carry the current call's `host_callback_id`. The transport binds
the caller identity to the artifact's plugin ID and refuses other calls' context
IDs, even within the same plugin. Callback failures use a fixed error envelope;
credential values and arbitrary handler error strings are never echoed. The
host and transport can share the same explicit callback-context registry.

Streaming calls reuse the existing frame sequencer, including exact request IDs,
chunk ordering and terminal sequence checks. Two data chunks and a reserved error
slot bound buffering while preserving terminal failures under backpressure.
Streams have a 180-second total ceiling including queue time; unary calls retain
30 seconds. Dropping raw or typed streams cancels pending reads and reaps the
child. Explicit shutdown interrupts active streams and acknowledges cleanup.
Normal provider error ends remain visible without retiring a healthy child.

Production composition still needs the official CTOX credential resolver, typed
plugin runtime configuration and quota-service attachment during actual management
startup, followed by measurement through the installed management endpoint.
No empty/rejecting loader stands in for that work.


The Unix integration tests run the actual child binary: lifecycle, independent
reload endpoints, caller drop, idle-client drop, queued deadlines, invalid input,
and a complete quota fetch through the host with an account-scoped HTTP callback.
Denied account HTTP authority must not fall back to the broader host client.
Real-process streams also test malformed/truncated frames, exact request IDs,
backpressure timeouts, graceful error ends and typed-consumer cancellation. Linux
checks captured child PIDs disappear after cancellation/shutdown. Shared callback
tests reject cross-call and cross-plugin authority for unary and streaming calls.

Windows runtime acceptance is not claimed by these Linux results.
