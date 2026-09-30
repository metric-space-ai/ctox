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

`NativeTransferPeerResolver` owns at most one session across durable worker jobs.
Native account bootstrap supplies the existing `BusinessDataSessionHost` and
live `NativeSessionTargetProvider` callbacks keyed by saved target ID through
`start_daemon_with_native_accounts`. The default daemon entry point remains
HTTP-only until that production bootstrap is connected. Missing credentials,
changed source pins, account epochs or principals fail closed; neither reconnect
nor retry issues a new grant or changes the persisted binding.

The resolver uses query-only `NativeSyncSession::start_data_client`, checks the
original account around provider resolution and credential release, waits for
one ready connection, then installs the concrete source grant admission. A
cancelled authorization waiter retains ownership of the bounded startup task.
Switching jobs closes the previous session; errors retire it. Daemon shutdown
awaits provider cleanup before its Tokio runtime exits, under a thirty-second
bound. Native startup has a twenty-second bound and session close five seconds;
the native transport's existing cancellation/drop cleanup remains its backstop.
These lifecycle and authority regressions require local verification before
production wiring or acceptance is claimed.

The native account-store consumer is now `NativeTransferAccountHost`. It reads
versioned native enrollment records from the existing encrypted secret store;
public pins/principal and bearer credentials occupy separate scopes, so restoring
ID callbacks or resolving query-only options never loads the bearer. Target IDs
are SHA-256 keyed, and credential records bind the complete original authority.
Each callback resolves current native account state again, binds credential use
to its concrete connection, loads the original enrolled signer and checks account
state before and after key-store work. Missing, corrupt, inactive, switched or
foreign records fail closed; reconnect never creates a key.

The service owner constructs this host with a native query-only options factory
and passes it to `start_daemon_with_account_host`. Provider callbacks are resolved
for the individual target when each session opens, so authenticated enrollment
after daemon boot does not require a cached-map refresh or restart. Other account
records are not enumerated on this path. The original job account is checked
before lookup and around options/credential resolution. `providers()` remains
available for explicit enumeration; neither API accepts renderer credentials.
This consumer is not an enrollment API: the authenticated native provisioning
writer, atomic account-switch/revocation persistence, live options factory and
production bootstrap call still need wiring. Its authored restart, revocation,
foreign-credential, real-signature and missing/corrupt-key tests have not run.

This source authority, signer and concrete adapter do not by themselves prove
the production account enrollment, transfer service boot/recovery wiring,
or two-host acceptance. Those integrations must consume the existing native
secret/account/device authority without importing renderer tokens or bypassing
WebRTC/RxDB. These obligations remain open until exercised through the real
workflow. Source tests use isolated native store fixtures; no tenant mutation.
