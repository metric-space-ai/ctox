# Native guest command emission

A prepared provider proves the live worker, routing attempt, provider session
and actual bound turn. Its `command_provenance` describes the command that
started the worker; it does not authorize later guest commands. A signed
Business OS session token likewise does not prove an individual later call.

For a native-admitted direct session, the native turn owner now registers an
in-process dispatcher for its actual harness thread before `turn/start`.
The fork's MCP handler consults it after the existing configuration, argument,
approval and safety checks, immediately before ordinary MCP transport. Both the
approved and no-approval paths use this boundary; refused calls never reach it.
`NativeMcpInvocation` is constructed privately from the actual core Session and
TurnContext. Model arguments and asynchronous tool-begin events cannot construct
this invocation. Dropping the registration removes it; retaining a dispatcher
Arc cannot prolong registration or the separate native owner lifetime.

The direct-session guest dispatcher intercepts only
`ctox-business-os/business_os.execute_action` for `ctox.guest.observe` and
`ctox.guest.input`. Other tools retain ordinary transport. It requires the
verified initiating actor and workspace, rejects client-supplied identity and
extra arguments, and creates a TrustedLocal command ID from the actual
binding/thread/turn/call/server/tool tuple. A reused call ID cannot mint another
command even with changed arguments. Identity in client context is attribution,
not a permit. Missing or foreign actual turn, revoked worker and ended provider
fail closed before guest consumption.

Only the retained native turn owner can issue a `NativeProviderCommandEmitter`.
Its `admit_emitted_guest_command` binds command ID, type, module, record,
canonical payload and client context to the actual bound turn. It holds the
existing worker/provider IMMEDIATE transaction and lifetime locks; the private
`NativeProviderCommand` is returned only after transaction commit. Admission IDs
are reserved before the callback and cannot be reminted after uncertain failure.
Each turn permits at most 1024 emission attempts, with a 32 KiB command envelope.
The emitter does not extend its owner's live lifetime.

`NativeProviderAdmission::execute_guest_command` is deliberately denied by
default. A production VM consumer must explicitly implement it and consume the
exact witness via `with_current_command_transaction`. Its callback must check
current account/policy/controller and persist the bounded effect under that
same live native worker transaction. It must not reopen the channel store,
await, or re-enter `NativeGuestExecution::with_current` or another provider
guard: those acquire the same transaction/lifetime locks. Clones share one
consumption flag. An uncertain effect burns the witness and requires
reconciliation, rather than repeating the effect.

The in-process callback proves emission, not durable guest admission or a
successful VM effect. The initiating provenance is not current account
authorization. The VM caller/controller, authenticated Raft job, atomic effect
and fresh checkpoint, installed command/frame/input workflow and platform
acceptance remain required. HTTP callers, tokens and event observers without
this retained per-command witness must remain denied.

Source regressions cover the real core MCP handler's actual Session/TurnContext,
invalid JSON before dispatch, model identity labels, registry scoping/teardown,
native owner revocation, default-consumer denial, envelope mutation, canonical
JSON equivalence, uncertain admission and shared one-shot consumption. Compiler
and test results must be recorded on the final source separately; these tests
do not establish the installed MCP/VM workflow.
