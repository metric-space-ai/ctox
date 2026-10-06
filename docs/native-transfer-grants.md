# Source authority for native transfers

For first enrollment, obtain the source's public identity with
`ctox transfer source-identity` on that source and a fresh source-generated
native mobile invite JSON file. On the recipient run:

```
ctox transfer pair TARGET SOURCE_PUBLIC_IDENTITY INVITE_FILE
```

The source key is an explicit independently trusted pin; it is never learned
from the signaling peer or invite file. Pairing accepts the native one-time
invite secret, not a renderer bearer. The existing native core proves the
pinned source before the new scoped P-256 key signs its nonce or the invite
secret is released. Source admission binds the key thumbprint to the device;
`provision_from_session` then obtains the current principal and routing and
atomically persists them. The input is kept out of command-line arguments and
output. Initial invites use native ICE bootstrap defaults because the invite
schema supplies no ICE descriptor; source-confirmed routing governs subsequent
transfer sessions. Both active accounts and disconnect tombstones prevent
pairing over an existing target. Failed enrollment may retain its prepared key
for a retry with that same scope; reconnect cannot rotate it. Pairing is not a
way to recover an old job under a newly issued account.

On the source, explicitly publish each selected regular file with:

```
ctox transfer publish FILE
```

This local-operator action reuses the native desktop-file writer and its path
boundary, eagerly materializes and verifies chunks even above the normal lazy
file threshold, then compares the stored hash and size to an independent byte
hash. It returns `fileId`, `sha256`, `size`, `sourceInstanceId` and
`sourcePublicIdentity`. Keep the source artifact quiescent during publication;
a changed file fails verification. Directory publication is rejected. Repeated
publication uses the same canonical native file ID. Published content follows
the existing desktop-file read policy; this command grants no recipient access.
Payload remains exclusively on the native WebRTC file service.

The local operator command is:

```
ctox transfer peer-download ID TARGET SHA256 SIZE FILE_ID
```

`TARGET` selects an existing native enrollment; source pins, account epoch and
principal are read from that enrollment. The command authenticates through the
saved source route, requests a `desktop_files` grant and exercises the worker's
current source-grant/file-policy checks. It never accepts a caller-supplied
bearer, source pin or principal. A captured-account provider rejects account
changes before credential release. Admission is bounded to 60 seconds and uses
an isolated temporary query database under the installation's runtime directory,
not the daemon's database. Transport and database cleanup precede job insertion.
The daemon owns all payload downloads and revalidates the persisted original
account/grant before reading or publishing. Use `status`, `pause`, `resume` and
`cancel` on that job ID. Native enrollment must already exist. An expired source route triggers the
bounded authenticated recovery described below before grant admission.

`ctox.transfer.grant.v1` is an authenticated auxiliary method on the supervised
native RxDB/WebRTC peer. It is not an HTTP endpoint or a collection projection.
The daemon registers it before advertising the peer. Blocking policy, SQLite
and secret reads run off the transport executor.

Requests contain exactly one typed object (`issue`, `check`, or `revoke`).
Issue/check carry an immutable scope: transfer ID, enrolled source instance ID
and public key, collection, file ID, expected SHA-256 and size. Only
`desktop_files` with available, live `sha256-bytes-v1` metadata is supported;
other blob collections fail closed until their generation authority is wired.
A current enrolled device/proof-key binding is mandatory for issue, check and revocation; ordinary bearer-only sessions are denied. The auxiliary dispatcher admits only sessions accepted by the existing native nonce-bound P-256 validator. Registration retains the exact accepted recipient connection and original authenticated capability through the guarded auxiliary dispatcher. Every physical send poll rechecks the current issuer, source identity and encrypted record tuple while holding current native policy and, for active grants, the exact file projection. A zero-byte Pending releases those guards and requires fresh checks on the next poll; reconnecting under the same signaling ID cannot inherit the prepared reply. The source compares its own provisioned identity and native instance with the
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

Native session startup likewise retains its task across waiter deadlines. The
native routine owns its bring-up timeout and drains failed startup resources;
an outer timeout must not cancel that drain. A shutdown wait that expires while
startup is still running returns a cleanup error, leaving host storage open.
The CLI retains its startup task when its 60-second admission deadline expires
and allows a bounded cleanup wait before closing its isolated database.

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
known TURN credential expiry.

After a cold restart, the account host can extract retained signaling material
from an originally valid, encrypted source descriptor. This does not move its
expiry or make it valid for payload use. A distinct rendezvous value contains
no ICE credentials or payload deadline. The control-only recovery session retains
only credential-free STUN/STUNS discovery endpoints from that source snapshot.
It strips every TURN/TURNS endpoint and credential-bearing ICE server, and uses
an explicit empty entry when no public STUN was supplied, suppressing native
defaults. It can also use the source's currently advertised candidates.
Every reconnect rechecks the original account and unchanged rendezvous. Fresh
source proof and the original P-256 device credentials remain mandatory.

The worker resolver and new-job admission use that session only to request
source-confirmed routing through `provision_from_session`. They recheck the
original account, close the bootstrap transport, and open a new session using
the fresh descriptor before checking a persisted grant or issuing a new one.
Each resolution allows at most one bootstrap and one payload-session attempt.
Cancellation retains startup ownership and existing cleanup bounds. An expired
or revoked original file grant still fails; routing recovery does not issue a
replacement grant for a queued job or change its principal.

Recovery requires the source to be reachable through retained rendezvous and
currently advertised candidates. A rotated/deleted rendezvous, revoked device,
unavailable source, or a network requiring recipient-side TURN before that
source can be reached fails closed. Relay-only network coverage and real
restart/resume acceptance remain unverified. Newly authored recovery and
source/tuple/expiry/revocation regressions still require execution; formatting
alone is not acceptance.

This source authority, signer and concrete adapter do not by themselves prove
the production account enrollment, transfer service boot/recovery wiring,
or two-host acceptance. Those integrations must consume the existing native
secret/account/device authority without importing renderer tokens or bypassing
WebRTC/RxDB. These obligations remain open until exercised through the real
workflow. Source tests use isolated native store fixtures; no tenant mutation.
