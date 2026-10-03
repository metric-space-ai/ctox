# CTOX Sync execution core

This crate contains execution authority and session checkpoint storage shared by
CTOX hosts and Workjet executors. It is an unfinished part of the coordinated
[Sync-core renewal](../../../docs/dev/ctox-sync-core-offensive.md), not a separately
deployable replacement daemon. Business records still replicate through RxDB.

## Responsibilities

- `native.rs`: shared native transport lifecycle, now called by the Business OS
  peer. The host supplies an explicit database identity, zero or more collections,
  a fresh signaling-URL provider and its
  admission/read/write predicates. The session owns signaling, the native
  handler and one multiplexed pool; the host owns its database and business
  workers. All supplied collections must belong to that database; invalid
  ownership is rejected before signaling starts. Coordination-only voters and
  Workjet control peers require no dummy Business OS collection. Their handshake
  advertises explicit empty schema/checkpoint maps and a null representative
  collection. Single-collection peers also advertise their complete maps.
  Modern peers compare schema entries by collection name, so different local
  collection counts or representatives do not invalidate room admission.
  Data peers register replication only for the advertised intersection;
  a missing schema proof remains distinct from an explicitly empty set.
  Failed admission, timeout and cancelled startup close the connection.
  Signaling receivers and pool subscribers are installed before joining the
  room, so an immediate native offer cannot arrive ahead of their consumers.
  `NativeSyncOptions.peer_role` selects the explicit native runtime role from
  the generated wire contract. Workjet hosts use `WorkjetExecutor`; the Business
  OS host explicitly uses `CtoxInstance`. The same role is used for inbound
  handshake answers and outbound protocol probes. Host admission and collection
  permissions remain independent and must validate the remote role and identity.
  Hosts await `shutdown` before closing persistence; Drop provides an unwind
  backstop. The default 20/20 batches and 5-second replication retry are retained.
- `host_config.rs`: versioned native host pins in the host runtime SQLite database.
  The singleton record contains the scope, local voter/worker identity, three
  distinct voter keys and Raft timing. Saving uses an immediate transaction and
  refuses identity, scope, role or capability rebinding; existing runtime tables
  remain untouched. Loading validates the stored record again. Signing material,
  signaling credentials, route hints and live IPC endpoints are excluded.
  Voter attachment options use a stable storage directory and both roles use the
  dedicated `ctox-execution:<scope>` room; the supplied key must match the pin.
  The host supplies its private IPC directory separately: a long persistent
  database path must not become a Unix socket path. The existing listener owns
  path-length, ownership, permissions and process-lock validation.
  A worker record is local configuration, never a quorum admission receipt.
  Host setup types now come from the same generated Rust/TypeScript contract
  as IPC. Native validation additionally enforces distinct pins and membership
  constraints; schema decoding alone never proves admission.
- `host_runtime.rs`: one foreground lifecycle for the CTOX service and `ctox sync
  run`. It validates the configured control-only database, starts the existing
  native session/attachment, publishes the actual listener, supervises it and
  shuts down the session before its caller closes storage.
- `../sync_host/`: the CTOX CLI/service adapter. Signing keys and explicit
  transport settings use the encrypted CTOX secret store; public pins use the
  runtime SQLite store. A common process lease precedes opening Raft or RxDB.
  `sync status` actively verifies the local listener's identity and protocol;
  it does not report membership or execution readiness. Production signaling
  grants and Workjet SSH/QR integration remain open. See the
  [host setup contract](../../../docs/dev/ctox-sync-worker-host-contract.md).
- `authority/node.rs`: pinned OpenRaft 0.9.25 adapter. Three configured peers
  confirm job ownership and generation. `local_job` is a projection;
  `validate_ownership` requires a linearizable quorum read.
  `AdmitWorker` / `RevokeWorker` commit additional executors separately from
  voting membership. Their public keys and never-reused node IDs persist in
  state-machine snapshots. Only configured voters can admit/revoke; additional
  workers may propose execution commands and validate their own ownership, but
  cannot issue Raft RPCs. Pinned revoked workers can still request validation of
  their own ownership. The quorum read precedes the active-membership check:
  revocation returns a typed denial, while an isolated voter cannot claim a
  confirmed decision from its cached tombstone. Unknown identities, validation
  for another executor and proposals from revoked workers remain rejected at
  admission. Native private IPC exposes these commands with distinct worker receipts;
  replayed admission cannot reactivate a revoked entry. Product invitation,
  administrator authorization and remote key-possession proof remain host work.
- `authority/client.rs`: nonvoting `WorkerAuthorityClient` implements the same
  `ExecutionAuthority` interface as `AuthorityNode`. `AuthorityIpc` and the private
  local listener now consume that interface. The worker keeps no Raft store or
  local ownership writer. It signs requests with its pinned key and contacts only
  its three configured voters, remembering a successful route as a hint. Every
  retry retains the request ID; a replay never becomes a fresh applied receipt.
  Unknown outcomes remain unavailable. Explicit local permission failures are
  rejected. Shutdown prevents late replies from authorizing a stopped client.
  The signed cluster/IPC dispatch test passes, including a lost committed reply,
  leader replacement, membership revocation and shutdown. This is not a Workjet
  Desktop/Mobile onboarding acceptance.
- `native_execution.rs`: `NativeSyncSession::attach_execution` attaches one
  configured authority group to the session's actual replication pool. It checks
  room, local signing identity and routing configuration, registers the signed
  control receiver and discovers native candidates admitted to the room. The session stops
  the group before closing transport. Membership never changes host data-access
  predicates. Discovery probes signaling routes advertised as `ctox_instance`
  or `workjet_executor`; a role alone never adds a trusted key. The native-only
  `ctox.sync.authority.route.v1` method reuses the signed envelope, with distinct
  request/reply kinds. Fresh nonce, scope, recipient and the current signaling
  address bind the proof. Only the three pinned voter keys may update routing;
  the connection must still be the same admitted lifetime after verification.
  A route proof neither invokes Raft nor grants membership or execution.
  Authority control waits for reciprocal protocol/token admission;
  receiving an inbound probe alone is insufficient. Production provisioning and
  executor/gateway enforcement are still required.
  The group also owns exactly one private local authority listener, selected by
  `ExecutionGroupOptions.ipc_directory`; `ipc_endpoint()` returns its actual path.
  Discovery and the listener run under one supervisor. Their unexpected exit
  shuts down Raft and releases the IPC endpoint; hosts observe `wait_stopped()`
  to report that failure. Normal session shutdown awaits the group before
  closing replication. An occupied endpoint fails attachment and tears down the
  new session without replacing its existing owner. Platforms without an
  implemented local listener reject attachment explicitly.
  `NativeSyncSession::attach_worker` now owns a `NativeExecutionWorker` using
  this same generic `NativeExecutionHost` supervisor. Worker options contain a
  confirmed identity pin and exactly three voters, with no Raft store path.
  Route maps are optional startup hints; an empty map uses authenticated discovery.
  Exactly one voter or worker attachment may occupy a native session. Workers
  and voters use one rule: the lower signaling ID alone initiates. The former
  worker-only override is removed now that both ends discover candidates.
  Role, room, route and local-key checks precede
  activation. Admission and business collection gates remain host-owned.
  A local signaling reconnect keeps the signing identity, membership and IPC
  endpoint. Discovery reads the current signaling ID for each initiator decision
  and waits for re-admission before negotiating when no local ID is available.
  SignedTransport continues to verify the configured recipient key and nonce on
  every exchange, including newly opened channels. Loss of signaling alone does
  not revoke an otherwise quorum-confirmed execution.
  Each transport generation captures the local signaling identity at creation.
  A fresh offer addressed to a changed local identity replaces the old responder
  even if its DataChannel remains open; it cannot renegotiate against the retired
  connection. Removal is generation-bound, so delayed closes cannot erase the
  replacement. The worker reconnect test keeps old channels open until all three
  new routes are mutually admitted, then closes the retained old handles and
  validates the same quorum-confirmed membership and job ownership over IPC.
  The real four-peer WebRTC/Unix-IPC test passes: admission, execution ownership,
  replay, revocation, denied business reads and retained-handle shutdown. The
  worker has the greatest signaling ID and opens no Raft store. This is a native
  control-path acceptance, not a completed coding-agent turn. Product onboarding,
  signaling grants and harness supervision remain open.
- `authority/store.rs`: SQLite Raft log, vote, state machine and snapshots.
  Receipts and state changes commit atomically. A duplicate command returns
  `Replayed`; it never grants permission to perform another external effect.
  Local diagnostics separate blocking-pool queueing, connection-mutex waiting
  and execution for each internal operation. They retain aggregate counts and
  maximum phase durations, never request payloads or an event history. A dropped
  async waiter does not cancel or misreport blocking storage that still runs.
  `AuthorityNode::diagnostics` adds adapter-owned log/apply/quorum observations
  and client-write/read timings. These observations cannot authorize effects;
  an interrupted write observation does not imply a rolled-back Raft command.
- `authority/auth.rs`: the same configured Ed25519 identities sign control
  envelopes and independently domain-separated durable checkpoint receipts.
  `from_existing_pkcs8` imports Workjet's existing Node/OpenSSL key and checks
  the pinned public identity. It accepts PKCS#8 v1/v2 without generating a
  replacement key. The host must obtain the key from its existing local secret
  store, never from a replicated session or a remote request.
- `authority/webrtc.rs`: control method `ctox.sync.authority.v5` over the existing
  native RxDB WebRTC pool. Replication master/fork election does not decide
  execution ownership. Routing hints do not authenticate a peer.
  Version 5 adds a quorum-confirmed current worker-membership read to the typed
  failures introduced in version 4. Voters can inspect worker state; a worker
  can inspect only its own record, including a revoked tombstone. That read-only
  exception grants no execution or Raft permission. Reads do not replay an old
  admission receipt and cannot succeed from an isolated leader's local store.
  Leader hints, temporary unavailability and terminal rejections remain typed.
  Previous control versions are rejected.
  IPC framing remains version 1 with additive typed operations/results;
  an old host rejects unknown enrollment operations instead of falling back.
- `authority/routing.rs`: one recovery boundary for both voting executors and
  nonvoting workers, replacing their separate routing paths. Within the existing
  five-second deadline, attempts retain the exact request and durable ID. Only
  configured voter keys may be contacted; leader hints reorder remaining routes
  but never authorize execution. Typed rejections and committed receipts are
  terminal. Replays remain replays. Failed rounds use bounded backoff; successful
  requests add no delay. Shutdown rejects even an in-flight confirmation.
- `ipc.rs`: framed local control messages; caller-supplied actors are rejected.
  `local_host.rs` owns the Unix listener, its exclusive process lock, private
  directory/socket permissions, same-user peer check and bounded connections.
  `NativeExecutionGroup` supervises the listener and awaits its shutdown.
  Failed listener tasks are consumed even when joining them fails, so subsequent
  cleanup never polls an already-completed failed task. Existing live sockets
  and non-socket files are never replaced.
  Windows named-pipe hosting remains to be implemented. This crate does not open
  an HTTP or TCP execution endpoint.
  `protectCheckpoint` now forwards signed durable-copy receipts to the existing
  authority, and `takeOver` forwards the expected ownership and protected digest.
  Both are generated additions to IPC version 1; older hosts reject unknown
  operations. The native listener derives the actor and the takeover target from
  its own identity. Callers cannot supply another owner. Consensus still enforces
  two suitable verified copies, current generation and reconciled effects;
  replayed requests return historical receipts without fresh execution authority.
  The private-IPC/WebRTC partition test and actual Workjet client fixture cover
  this path, including control latency measurements. They execute no VM/harness:
  guest freeze, target restore verification and effect-boundary enforcement
  remain adapter integration work.
- `checkpoint.rs`: immutable content-addressed artifacts and manifests. Restore
  creates a fresh directory and checks all hashes, paths and pending effects.

## Protecting a checkpoint

The owner transfers all manifest-referenced artifacts through CTOX Sync to data
peers. Each receiving peer publishes the verified manifest in its own store and
calls `SigningIdentity::acknowledge_checkpoint` off the async control loop. This
operation revalidates the complete contents, flushes files and directory entries,
and binds the receipt to the job specification, current owner/generation,
manifest digest and journal sequence.

`Command::ProtectCheckpoint` accepts these receipts, never an asserted replica
list. At least two distinct configured data peers, including the owner, must
agree. The state machine checks signatures and matching metadata before publishing
any protected checkpoint. A coordination-only voter cannot count as a data copy.
The receipts remain in the checkpoint state after log compaction. Takeover still
requires the matching protected checkpoint, current generation and no unresolved
external effect. A receiving executor must reverify its local data before resume;
a past receipt does not prove that its disk is still available now.

The unsigned version-1 protection command is rejected, as is authority protocol
version 1. Experimental authority stores containing old unsigned checkpoint
states need an explicit migration. They are not silently trusted or deleted.
Non-Unix platforms currently refuse durable-copy receipts until their directory
flush implementation has been implemented and certified.

## Wire source and checks

The single schema source is
`../rxdb/tests/fixtures/ctox_execution_contract.json`. The generator produces Rust
structures, TypeScript types and Effect schemas, including the Workjet copies.
Never change generated output by hand. The authority-control, local-IPC and
checkpoint-manifest protocol versions are distinct contracts.

Run from the CTOX root through `greppy bash-smart --`. On the operator machine,
keep Cargo artifacts and test scratch files in the agreed external build root;
check free capacity before starting a build:

```sh
node src/core/sync/tools/generate-contracts.mjs --check --workjet-root ../workjet
node src/core/sync/tools/assert-host-contracts.mjs ../workjet
TMPDIR=/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/tmp CARGO_TARGET_DIR=/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/cargo-target cargo test --manifest-path src/core/sync/Cargo.toml --features webrtc -j 1
TMPDIR=/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/tmp CARGO_TARGET_DIR=/Volumes/tmp/dev-artifacts/ctox/sync-core-offensive/cargo-target cargo clippy --manifest-path src/core/sync/Cargo.toml --features webrtc --all-targets -- -D warnings
```

The IPC integration test requires the sibling Workjet checkout and its installed
Node dependencies; it loads the actual Workjet client rather than a rewritten
fixture client. The integration tests use independently persisted synthetic session files. One
suite uses actual localhost WebRTC channels through the pool's admission and
auxiliary dispatcher. Other tests run three complete native session/group
lifecycles, verify denial of business-record reads, and drive the actual Workjet
IPC client through the group's owned endpoint. They cover an occupied endpoint,
quorum loss, normal shutdown and revocation while other voters remain available.
Two further scenarios exercise discovery with three `workjet_executor` signaling
roles and with two such peers plus a `ctox_instance` coordinator. Admission also
checks the expected protocol role for each test session, so silently advertising
a worker as a production instance fails. They retain the signed-key checks and
denial of business-record access. The group still requires exactly three voting
peers, and host admission is a fixture. Additional enrolled workers use the
confirmed worker directory separately from that three-voter configuration.
The `host_cli_acceptance` example accepts an actual CTOX binary and a private
fixture work directory. It runs separate processes with generated keys and
local signaling; it never executes a coding harness or uses production state.

This is not certification of WAN,
harness export/import, credentials, a mobile host or the production signaling
admission path. Provisioning, full production-service acceptance, continuous process supervision,
gateway/tool fencing, real Codex/Claude resume and coordinated migrations remain
required before replacing active Workjet execution and mailbox paths.
