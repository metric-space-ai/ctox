# Source authority for native transfers

`ctox.transfer.grant.v1` is an authenticated auxiliary method on the supervised
native RxDB/WebRTC peer. It is not an HTTP endpoint or a collection projection.
The daemon registers it before advertising the peer. Blocking policy, SQLite
and secret reads run off the transport executor.

Requests contain exactly one typed object (`issue`, `check`, or `revoke`).
Issue/check carry an immutable scope: transfer ID, enrolled source instance ID
and public key, collection, file ID, expected SHA-256 and size. Only
`desktop_files` with available, live `sha256-bytes-v1` metadata is supported;
other blob collections fail closed until their generation authority is wired.
A current enrolled device/proof-key binding is mandatory for issue, check and revocation; ordinary bearer-only sessions are denied. The auxiliary dispatcher admits only sessions accepted by the existing native nonce-bound P-256 validator. Registration additionally resolves the current recipient connection and requires its captured authenticated token to match before and after blocking work. The source compares its own provisioned identity and native instance with the
scope, verifies the current capability actor/device/authorization epoch and
applies the same native collection read policy used by file demand fetches.
It compares content hash/size with the authoritative file metadata and captures
its generation in one read snapshot. Issuance rechecks principal and generation
before persisting. Caller-supplied principals or extra fields are rejected.

Grants have random 256-bit IDs and a fixed one-hour lifetime. The complete scope,
principal, generation, expiry and revocation state live encrypted in CTOX's
existing secret store; only version metadata is plaintext. IDs are references,
not bearer credentials. Check reopens the durable record and validates exact
scope, current principal/device/epoch, expiry, revocation, current policy and
current file generation. Revocation persists and is idempotent for the same
current principal. A job resume cannot replace the original grant/account
snapshot with the newly logged-in account. Removing/replacing content,
revoking the device/account/policy or expiring a grant denies reuse, including
completed-content cache hits.

`NativeTransferGrantAdmission::new(Arc<NativeSyncSession>)` is the concrete
`NativePeerJobAdmission` adapter. `issue(connection, request)` obtains the
source's grant reply. Capture its ID via the existing `enqueue_enrolled_peer`
account snapshot, then supply this admission to `NativePeerRangeSource::bind_enrolled`.
Every check uses the actual current, ready NativeSyncSession connection, proves
the pinned source identity, exchanges the source check under a ten-second bound,
and rechecks connection currency after the await. Pool cancellation aborts the
exchange. The existing enrolled adapter additionally rechecks the durable
account/principal around awaits and probes current file policy before reads.

`NativeDeviceProofKey` supplies the native recipient signer from CTOX's existing
encrypted secret store. Explicit preparation scopes the key to the saved target,
source identity/instance and local account epoch. Reconnect loads the original
key; missing, corrupt or differently scoped records fail instead of generating
or replacing identity. First creation uses a single SQLite create-if-absent
statement and always reloads the stored winner, so independent processes cannot
replace each other's recipient key. Only public JWK/device binding and nonce proof leave the
key owner. The source-store tests restore this signer before exercising the
existing nonce-bound P-256 admission path. The signer itself grants no account,
collection or file authority; the native target provider must prove the current
source connection before invoking it.

This source authority, signer and concrete adapter do not by themselves create the
production saved-target/credential host, transfer service boot/recovery wiring,
or two-host acceptance. Those integrations must consume the existing native
secret/account/device authority without importing renderer tokens or bypassing
WebRTC/RxDB. These obligations remain open until exercised through the real
workflow. Source tests use isolated native store fixtures; no tenant mutation.
