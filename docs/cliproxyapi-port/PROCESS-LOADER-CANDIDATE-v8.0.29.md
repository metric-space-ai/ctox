# Candidate: executable unary plugin transport

Depends on the quota RPC candidate (#752). This adds the previously missing
`PluginLoader` implementation on top of CTOX's existing authenticated local
process supervisor. It does not activate plugins in the installed daemon,
promote the accepted upstream revision, or claim live provider quota support.

The caller explicitly supplies the runtime directory, executable artifact and
host callback router. Each open has a unique private local endpoint; the child
receives no inherited environment and gets its one-shot handshake token through
stdin. Calls use a bounded queue and a bounded response deadline. A dropped call,
protocol failure or expired response ends the child and closes the queue because
a partially consumed length-prefixed frame cannot safely be reused.

Host callbacks carry the current call's `host_callback_id`. The transport binds
the caller identity to the artifact's plugin ID and refuses other calls' context
IDs, even within the same plugin. Callback failures use a fixed error envelope;
credential values and arbitrary handler error strings are never echoed. The
host and transport can share the same explicit callback-context registry.

Streaming capability calls are explicitly unsupported in this slice. Before
production composition, complete streaming transport, wire the official CTOX
credential resolver and typed plugin runtime configuration, attach the quota
service during actual management startup, then measure through the installed
management endpoint. No empty/rejecting loader stands in for that work.
