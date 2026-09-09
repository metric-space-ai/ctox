# Workjet Crew context restoration

`business_os.get_crew_context` is a read operation on an already admitted native
Crew attempt. Its only domain argument is `attempt_id`; MCP transport context is
not an authority supplied by the caller. This is the context-restoration part of
Dev/Ops unification, not external execution admission.

The request must carry an instance-signed Business OS command-session token.
The transport verifies that token, and the handler revalidates the command's
current actor and role. The native command/task link, immutable payload hash,
open Crew attempt and current queue lease bind the read. A raw local/operator
bearer or ordinary managed-agent identity alone does not grant this operation.
No caller-selected member, task, root path or conversation id is accepted.

Existing private Crew-read policy applies: public member visibility does not
release persona or memory. Channel enablement, read policy, actor/workspace
policy, collection and module restrictions still apply. A restricted signed
collection scope must include `ctox_crew_members`. No new role grant is created.

The response has schema `ctox.crew_context.v1`, command/task/attempt/module and
member identifiers, member name, `persona`, optional `memory_block`, and an
opaque `context_version` hash over this content. The two text lanes use the
same native renderers and byte limits (2,400 / 6,000) as native Crew execution.
Clients must place persona in the identity instructions and memory in the
runtime context as knowledge, without granting additional tool permissions.
Reload on start, resume and compaction, retaining the explicit attempt binding.
A changed version means that the rendered snapshot changed; it is not a new
execution, lease renewal or evidence that learning occurred.

The read uses one read-only core-database transaction for binding, persona and
LCM heads. Missing continuity documents mean no stored memory; an unavailable
store or dangling head is an error. No memory schema, member or attempt is
created by the reader. It does not finalize work, write learning candidates,
renew leases, or retarget the selected member.

Still required for external harness use: server-authorized external admission
and bounded lease ownership, issuance of the appropriate scoped session,
Workjet start/resume/compaction consumers, execution evidence and native review/
learning integration. An advertised reader is not proof of an implemented
external Crew execution lifecycle. General projects and ordinary users must not
be marked supported by deriving broader access from this read operation.

Regression coverage lives in `mcp_crew_context.rs` and uses the MCP dispatcher,
real command-session signing/verification, native command admission and Crew
preparation. It checks native rendering parity with real LCM knowledge, excludes another
member’s knowledge, observes newly persisted knowledge, and checks stable and changed versions,
foreign-command and argument denial, channel collection restrictions, corrupt
memory, expired lease and finalized attempt. The Crew liveness workflow includes
the new test filter. Local Cargo verification is pending while the shared host
admission gate is closed; CI evidence is required before integration.
