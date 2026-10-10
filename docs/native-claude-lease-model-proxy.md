# Native Claude lease model proxy

`NativeClaudeLeaseModelProxy::reserve` accepts Crew's actual claimed
`NativeSupervisorHoldingController`, which owns an admitted native peer and
the original signed command or confirmed-plan execution lease. It cannot be
created from browser facts, a controller ID, or an arbitrary selected account.
`reserve_shared` accepts the same genuine `Arc<NativeSupervisorHoldingController>`
retained by Crew's original native service; sharing it does not create a new lease.

The holder reads the encrypted credential/configuration snapshot outside
source, issuer and policy fences. The current callback enters the actual
controller once and rechecks the exact sealed model against its borrowed
policy connection. It never reacquires transport authority inside that fence.
Account/catalog/configuration changes, a replaced or expired execution lease,
revocation, and controller cancellation reject further dispatch or publication.

Only the registered private Source broker receives a random per-controller
capability. OAuth access/refresh secrets, the private account binding and
controller object remain on the holder. Capability delivery is a bounded
synchronous private callback, with no cancellation or proxy/controller reentry.
Capabilities and request headers must not be logged, serialized into public
receipts, or replicated as provider metadata.

`invoke` forwards genuine Claude SDK Messages or CountTokens JSON to the
official HTTPS Anthropic endpoint, using the existing fingerprinted portable
Claude transport. The selected live model must match exactly. The caller
cannot choose another account, destination, proxy, or credential. There is one
in-flight request per controller, a five-minute request/publication deadline,
and an 8 MiB request/response bound. There is no account rotation, automatic
credential refresh, failover, or shared gateway cooldown mutation.

The proxy re-enters the current lease/account fence before and after every
await and polls current authority during an upstream wait. Cancellation drops
the portable upstream future/body, retires this capability and exact account,
and cancels only its controller. Completion/error drops the stream's concurrency
permit. `is_streaming` chooses `publish_next` or `publish_buffered`; an upstream
non-success streaming response becomes a bounded buffered error retaining its
real HTTP status, body, headers and retry delay.

Publication callbacks enqueue a bounded native result while the real current
fence is held. They cannot await or re-enter network, secret, controller or
transport code. `publication_for(actualIncomingAdmittedConsumerAuthority)` prepares
a retained account metadata checker before responder locks and composes it with
Crew's sealed `NativeSupervisorCurrentPublication` scope. At each physical send
it requires the exact original Arc controller, current selected-account policy,
exact native runtime configuration and original encrypted-record generation.
The existing issuer fence excludes credential rotation; an additional native
runtime writer reservation excludes account edits during publication. There is
no nested secret decryption, controller or transport entry. Restoring identical
credential plaintext does not resurrect a previously pinned encrypted generation.
A Rust enqueue alone does not prove a remote subscriber received it.

The private exchange witness contains the real selected model/account binding,
controller, outgoing client request ID, upstream HTTP status and dispatch
latency. It is not serializable, debuggable or cloneable, and establishes no
Claude SDK session, native turn completion or SDK/process-stop receipt.
An arbitrary SDK correlation string is not producer registration.

## Remaining integration and acceptance

This is a private Rust model dispatcher. It does not add a public listener,
Source-local SDK HTTP broker, native control receiver, SDK session registry or
a genuine turn witness. SourceRoot's registered ClaudeDriver/ClaudeAdapter,
Crew's guarded native handler, and the durable admitted Root connection must
integrate it before installed Supervisor acceptance can pass. Supervisor state
continues through the guest/native control channel; only LLM payloads may use
this private capability route.

The raw SDK body is forwarded without gateway prompt/tool rewriting. No live
provider compatibility or installed execution is claimed by the component
regressions. Those exercise exact capability/model/protocol checks, upstream
error preservation, held-policy model pins, existing encrypted account
retirement and the actual controller's native lease guards.
