# Native speech target policy

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

Before an Open or synthesis Start can perform an effect, the receiver must
reserve its intent through the verified target policy. The bounded SQLite
runtime ledger stores request digests and opaque object IDs, not PCM, raw text,
voices or credentials. A retry returns its existing object ID; it must resolve
to the same live host generation and exact native connection. Changed request
content is rejected. Host restart leaves the old intent tombstoned and cannot
restart it. Expired claims are reclaimed; live claims are never evicted to
admit a replay. Reconfiguration/issuer removal invalidates prepared authority.

This policy is the foundation for the Models receiver on
`ctox.native.speech.v1`. The receiver must additionally fence host lifetime and
the exact native connection, enforce Append/Finish sequence replay protection,
expire/cancel IPC streams and bound pending synthesis/audio artifacts. No
handler or inference effect is activated by the policy module alone.
Installed two-host meeting acceptance is still open.
