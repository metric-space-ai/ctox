# Native guest command emission witness

A prepared native provider proves the live worker, routing attempt, provider
session and actual bound turn. Its `command_provenance` describes the command
that started the worker. It does not authorize later `ctox.guest.observe` or
`ctox.guest.input` commands. A Business OS user session also does not establish
which worker emitted a command.

`NativeProviderTurnOwner::admit_emitted_guest_command` is a bounded producer
primitive. Only the retained native turn owner can call it. The native emitter
supplies the actual turn and exact trusted command; the synchronous callback
must admit that command and reject refused or replayed admission. The callback
holds the existing worker/provider IMMEDIATE transaction and lifetime locks.
It must not await, reopen the channel store, or re-enter lifecycle callbacks.
A private `NativeProviderCommand` is returned only after successful transaction
commit. IDs are reserved before admission and cannot be reminted after an
uncertain failure. Each turn admits at most 1024 emission attempts.

The witness binds command ID, type, module, record, canonical payload and client
context to the actual provider/turn. It cannot be constructed from serialized
facts, a session token or model-provided labels. JSON object ordering is ignored;
changed values and replicated commands are rejected.

`with_current_command_transaction` consumes the witness at the effect boundary.
The callback must check current policy/controller and perform the bounded effect
under that same live worker/provider guard. Clones share one consumption flag.
Admission failure produces no witness; effect failure burns the witness and
requires reconciliation rather than repeating an uncertain effect. Cancellation,
lease replacement/expiry, provider closure, witness-row changes and replaced
stores retain the underlying provider guard's fail-closed behavior. Returning
identity for an effect performed later is not guarded admission.

This primitive is not a wired MCP emitter. The direct-session tool-begin event
is asynchronous, and the MCP HTTP endpoint's signed session token proves session
scope rather than an individual later call. Neither may mint this witness. The
real native emitter/admission connector, VM `GuestCommandOwner::caller` consumer,
policy/controller integration and installed guest acceptance remain required.
The command owner must reject calls lacking their retained per-command witness.

Regression tests use real native queue/worker/provider guards and isolated
admission/effect tables. They cover envelope changes, canonical JSON equivalence,
pre-turn/foreign-turn rejection, replicated/invalid input, uncertain admission,
shared one-shot consumption and revocation. Their results must be recorded
separately; source tests do not prove the live MCP/VM workflow.
