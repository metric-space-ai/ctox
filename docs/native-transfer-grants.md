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
The default daemon entry point now constructs `NativeTransferAccountHost` with
live credential callbacks keyed by saved target ID. Its query-only database is
opened lazily inside the lease-owning worker runtime, with no collections and
explicit deny hooks for collection reads/writes. HTTP-only operation opens no
native database or session. Missing credentials,
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
The daemon closes its owned database after transport shutdown. Cancelled waits
retain the database startup handle so shutdown can retrieve and close it; a
closed owner cannot reopen. These regressions require local verification before
runtime acceptance is claimed.

The native account-store consumer is now `NativeTransferAccountHost`. It reads
versioned native enrollment records from the existing encrypted secret store;
public pins/principal and bearer credentials occupy separate scopes, so restoring
ID callbacks or resolving query-only options never loads the bearer. Target IDs
are SHA-256 keyed, and credential records bind the complete original authority.
Each callback resolves current native account state again, binds credential use
to its concrete connection, loads the original enrolled signer and checks account
state before and after key-store work. Missing, corrupt, inactive, switched or
foreign records fail closed; reconnect never creates a key.

`transfers_native::daemon_peer` constructs the service host and options factory;
`start_daemon_with_account_host` also accepts an externally owned host. Provider callbacks are resolved
for the individual target when each session opens, so authenticated enrollment
after daemon boot does not require a cached-map refresh or restart. Other account
records are not enumerated on this path. The original job account is checked
before lookup and around options/credential resolution. `providers()` remains
available for explicit enumeration; neither API accepts renderer credentials.
Authenticated native provisioning now uses
`NativeTransferAccountHost::provision_from_session(scope, session, connection)`.
It requires an already admitted, ready native connection and proves the pinned
source again before requesting `ctox.transfer.account.v1`. The source derives
the current enrolled principal from its verified capability/device store,
renews only that assertion without changing users/roles/pairings, and returns
its own room, signaling endpoints, browser-role commitment and current ICE.
The source never returns its native-role token or room password. Recipient
provisioning loads the original scoped P-256 key and compares its identity to
the source-confirmed principal; missing keys never get prepared on this path.
The only writer consuming the reply is private to the host, not an IPC JSON API.

Authority, renewed capability and bound routing are encrypted and committed
atomically. An IMMEDIATE SQLite transaction compares both the original account
and original routing record across processes. Concurrent refreshes cannot
regress the tuple; a late enrollment cannot undo a disconnect or account switch.
`revoke(expected)` atomically persists the inactive authority and deletes that
exact credential/routing pair. Explicit account replacement requires a greater
local account epoch and an already prepared, source-enrolled matching key.

The options factory still owns isolated query-only persistence and admission.
The host replaces its remote room/ICE/signaling fields from the confirmed
source descriptor. Its reconnect callback re-reads live native authority,
uses only the browser role and creates a fresh signaling time window. Account
changes, room rotation, descriptor expiry or the original ICE snapshot deadline
remove the route instead of falling back to local daemon configuration.
An explicit no-ICE source configuration suppresses default public STUN servers.
`routing(target_id)` exposes native service deadlines: renew via the admitted
session after `refresh_after_ms` and recreate the session before
`expires_at_ms`. The account-host resolver captures deadlines from the same
descriptor used to construct its options. Before further operations, a due
refresh uses the admitted session and the original job's target/pins/account
epoch, rechecks its principal, then closes and replaces the transport. Idle
expired transports are retired before reuse; only a current native descriptor
can open a replacement. Authorization and range futures are bounded by the old
transport deadline and cannot return successful results across expiry.
Descriptor lifetime is at most 30 minutes and never exceeds a
known TURN credential expiry. Cold restart with expired routing requires the
existing authenticated native bootstrap to establish a fresh session first;
expired TURN material is not silently reused. The production factory/boot is
wired, including live renewal/session recreation; cold recovery integration
remains the Transfer owner’s responsibility. Newly authored
source/tuple/expiry/revocation regressions still require execution; formatting
alone is not acceptance.

This source authority, signer and concrete adapter do not by themselves prove
the production account enrollment, transfer service boot/recovery wiring,
or two-host acceptance. Those integrations must consume the existing native
secret/account/device authority without importing renderer tokens or bypassing
WebRTC/RxDB. These obligations remain open until exercised through the real
workflow. Source tests use isolated native store fixtures; no tenant mutation.
