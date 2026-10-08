# Native checkpoint authority refresh

A native checkpoint copy can exceed the 60-second lifetime of an individual signed permit. The receiver previously pinned that first permit's expiry for every chunk; the private IPC server and CLI independently ended the entire copy after60/65seconds. A real stopped-Core/Git capture in VM#404 failed with target_authority_changed after its first small components.

The long-lived local operation now pins the exact binding, audience, job/session/scope, checkpoint digest/sequence, ownership generation, principal epoch and binding revision. Before each signature, local write, artifact ingest and final manifest publication it resolves fresh native authority under the existing account/issuer/policy/host/peer/lifetime fences. A changed identity/revision, revoked grant/account, retired host/peer or disconnected caller still denies publication.

Individual remote challenge, receive permit, request signature and already-prepared response lifetimes remain unchanged. The wire publication path still rejects expiry of the original remote response; a fresh local decision cannot renew it. This distinction also applies to local checkpoint protection/import, whose quorum and import operations retain their own fresh-permit checks.

The private handoff-copy operation has a fixed30-minute maximum and the CLI allows the same maximum plus5seconds for framing. Other checkpoint controls retain60/65seconds. Every chunk exchange retains its15-second maximum. Caller EOF cancels the operation and retires queued blocking publication through CopyLifetime. This is a bounded transfer deadline, not a30-minute authorization grant or an automatic retry.

The actual stopped-Core/Git fixture deliberately starts with an expired local witness and copies every protected component; it retains the existing revocation/account/cancellation assertions and dirty-effects rejection. A separate regression checks authority changes and ensures expired wire publication remains denied. These tests do not claim installed transport performance, clean effects or resumed execution.

Validation is pending the single composed #396/#421/follow-up GPU run. No credentials or checkpoint payload travels over SSH.
