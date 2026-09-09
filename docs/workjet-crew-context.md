# Workjet Crew context restoration

`business_os.get_crew_context` is a read operation on an already admitted native
Crew attempt. Its only domain argument is `attempt_id`; MCP transport context is
not an authority supplied by the caller. This is the context-restoration part of
Dev/Ops unification, not external execution admission.

The request must carry an instance-signed Business OS command-session token
with a native `crew_binding`: exact attempt, task, member, lease owner and
`leased_at` generation. Generic command sessions cannot read Crew context.
Native queue-worker session setup attaches this binding from its explicitly
prepared worker attempt when a Crew persona is present. It preserves the existing
command/writeback eligibility gates; it does not issue sessions for external
workers or create external admission. Both token verification and the reader's
transaction compare the binding with current durable state. Extending only lease
expiry preserves access; a new owner or lease generation invalidates the old token,
even when the same member and command remain assigned.
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
member identifiers, member name, `persona`, optional `memory_block`, the current
`execution_plan` (or null if none is persisted for this task/command/attempt), and an
opaque `context_version` hash over this content. The two text lanes use the
same native renderers and byte limits (2,400 / 6,000) as native Crew execution.
Clients must place persona in the identity instructions and memory in the
runtime context as knowledge, without granting additional tool permissions.
Reload on start, resume and compaction, retaining the explicit attempt binding.
A changed version means that the rendered snapshot changed; it is not a new
execution, lease renewal or evidence that learning occurred. The plan carries its
native revision, steps, phase, percent and review status. Completed model steps
remain at 90 percent until native review passes; this reader does not infer
completion. Plan JSON is limited to 64 KiB and oversized/corrupt plans fail the
read instead of being silently omitted. Plan changes also change the context
version. A plan from another task or command cannot be selected by attempt id
alone.

The read uses one read-only core-database transaction for binding, persona and
LCM heads. Missing continuity documents mean no stored memory; an unavailable
store or dangling head is an error. No memory schema, member or attempt is
created by the reader. It does not finalize work, write learning candidates,
renew leases, or retarget the selected member.

`business_os.update_crew_plan` accepts only `steps` and optional `explanation`.
Task, command, attempt and the native worker `work_key` come from the signed
session, with the work key supplied by native session setup. The caller cannot
choose another plan identity. The write respects channel write policy and the
same private Crew permissions as restoration. Requests are bounded to 64 KiB
and 1–100 steps, using the existing native step validator. The existing LCM
plan implementation computes revisions, progress and review state. Its new
guarded entry point rechecks the exact live binding after obtaining the write
lock and before changing the plan. Rejected authority rolls back the transaction.
A completed plan remains at 90 percent pending native review.

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
memory, expired lease and finalized attempt. It also checks generic-token denial,
lease renewal, same-owner re-leasing, changed owners and unknown-attempt binding. The Crew liveness workflow includes
the new test filter. Local Cargo verification is pending while the shared host
admission gate is closed; CI evidence is required before integration.

The first campaign CI run (34361263572, before the MCP additions) passed Rust
compilation, Clippy, native RxDB and browser checks but failed the new Crew replay
test with SQLite error 517 and two outbound reconciliation tests. Stage-specific
context is added to the Crew fixture to locate that conflict. These failures are
still open; they are not evidence that the new context/plan tests passed.

Crew finalization now starts with an immediate write transaction before reading
`finalized_at`. This prevents a concurrent writer from invalidating that read
snapshot before finalization writes. The regression uses SQLite's existing
authorizer hook to attempt a second-connection write exactly when finalization
prepares its first UPDATE, after its SELECT. It requires that competing write to
be denied, finalization to succeed, and replay to leave counts at one. This is a
source-level fix for an identified race; it is not yet proof that the original
CI error was at this location or that the new Rust test passes.

## Native external execution handoff (implementation pending verification)

An authorized `business_os.chat.task` may explicitly carry
`payload.external_executor = {executor_id, harness, timeout_seconds}`. Supported
route names are codex, claude, opencode, grok and cursor; this is a route contract,
not proof that Workjet implements all consumers. Timeout is 1–600 seconds. After
normal native admission, Crew selection and eligible command-session setup, the
worker persists an offer and waits under its existing capacity reservation and
lease heartbeat. Absence of the field preserves native execution. Invalid or
unserviceable explicit external requests fail; they never silently run natively.

`business_os.list_crew_executions` discovers live offers for an owned command and
executor using a read-only connection. It returns bounded metadata only, without
prompts or session grants, and does not initialize the handoff store. Lost native
leases are excluded; SQL failures are propagated rather than converted to empty
results. Listing and claiming with a command-session token cannot escape that
token's command or collection scope; an attempt-bound token cannot claim another
attempt. A claimed grant expires no later than the external offer deadline.

`business_os.claim_crew_execution` requires the exact command, executor and
attempt identifiers, current command ownership and private Crew permissions. It
returns a scoped command session, the original job prompt, native Crew context
and external tool instructions. The grant is for the Workjet controller's MCP
transport, not for copying into a model prompt or logs. Claims rather than bearer
tokens are stored in the core handoff table. Repeated live claims return the same
authority. Claims recheck the lease inside the state-change transaction.

`business_os.report_crew_execution` takes exactly a reply or error candidate under
that signed session. It checks the current lease in its write transaction and
accepts identical repeated evidence while rejecting conflicts. The native worker
returns the candidate into its existing finalization/review path. Reporting does
not itself finalize the Crew attempt or mark a review passed. Expiry and lease
changes close the offer; no external API acquires or renews an independent lease.

Remaining integration: Workjet submission, consumption of attempt discovery, controller
claim and secure transport setup, harness execution and result reporting; native
service restart/recovery and explicit cancellation verification; artifact and
structured retrospective handling; token eligibility for general projects without
app writeback contracts. The current production hook preserves existing command
session eligibility. The fixture now covers native offer publication, real MCP
claim/report, repeated claims/results, wrong executor/conflicting results and
retention of native finalization ownership. It has not yet passed CI.
