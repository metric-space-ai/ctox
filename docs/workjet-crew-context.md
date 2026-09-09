# Workjet Crew context restoration

`business_os.get_crew_context` is a read operation on an already admitted native
Crew attempt. Its only domain argument is `attempt_id`; MCP transport context is
not an authority supplied by the caller. This is the context-restoration part of
Dev/Ops unification, not external execution admission.

The request must carry an instance-signed Business OS command-session token
with a native `crew_binding`: exact attempt, task, member, lease owner and
`leased_at` generation. Generic command sessions cannot read Crew context.
Native queue-worker session setup attaches this binding from its explicitly
prepared worker attempt when a Crew persona is present. Explicit external
requests without executable app writeback receive a Crew-only session after
native Crew admission. Ordinary tasks retain their existing eligibility; any
supplied writeback contract is validated before session creation. Both token
verification and the reader's transaction compare the binding with current durable state. Extending only lease
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

Still required for external harness use: Workjet start/resume/compaction
consumers, execution evidence and verified native review/learning integration.
The native bounded handoff below provides the admission/session side, but its
current tests have not yet passed CI. An advertised reader is not proof of an implemented
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
not itself finalize the Crew attempt or mark a review passed. A result accepted
before the external deadline is consumed even if the native poll happens later,
provided the native lease still matches. Both reply and error receipts become
`returned`, so an identical retry can still be acknowledged under a live session.
Polling validates the stored receipt hash and checks the lease in the same write
transaction used to decide receipt consumption or timeout closure. Timeout closes
only offers without accepted evidence; cleanup never overwrites a reported or
returned receipt. No external API acquires or renews an independent lease.

Receipt regressions deterministically exercise a late poll, both success and
error replay, timeout without a receipt and rejection of corrupted evidence.
These cover receipt consumption, not full daemon restart recovery: re-entering
`run` with an already stored attempt still needs an explicit resume contract.

Remaining integration: Workjet submission, consumption of attempt discovery, controller
claim and secure transport setup, harness execution and result reporting; native
service restart/recovery and explicit cancellation verification; artifact and
structured retrospective handling; a full general-project submission path.
Session setup no longer requires an app writeback contract for explicitly external,
natively admitted Crew work. Its signed `crew_only` restriction permits only the
five Crew operations documented here. Both `tools/list` and dispatch apply the
same restriction; client arguments cannot disable it. Existing command, attempt,
collection and private Crew policy checks still apply. This grants no app CRUD,
writeback, shell, SQL or browser authority. It does not turn a general project into
an app or supply the still-missing general-project submission path.

A service regression uses real command admission, queue leasing and Crew
preparation to exercise session setup without writeback. Missing Crew, missing
worker attempt and expired admission cannot create a new session. The MCP fixture
uses the restricted signed grant for context, plan, claim and report, verifies the
advertised tool set, and rejects all other registered operations even when client
context claims `crew_only: false`. Existing app-writeback setup remains in the CI
filter to check compatibility. These new tests are unverified.

The fixture also covers native offer publication, real MCP
claim/report, repeated claims/results, wrong executor/conflicting results and
retention of native finalization ownership. It has not yet passed CI.


## Existing private project chats and native Crew selection

Private Workjet project tasks now resolve their Crew member from the canonical
command/task link and the command's `payload.thread_id`. The reserved
`workjet_private_` identity follows the existing project-chat contract. A missing
or ambiguous relationship fails rather than selecting a member from prompt text.
The resolver checks current command authorization, project ownership/activity,
private chat/history, project membership, profile binding, assigned computer and
an existing non-archived Crew member. It reads relationships in one read-only
Business OS store snapshot; this is not a cross-store atomic transaction with
Core admission.

The queue service treats this explicit identity as mandatory. It propagates
lookup/preparation failure instead of continuing without Crew. Native
`prepare_attempt` uses the resolved member as the explicit selection, rejects
conflicting manual assignments, batches and mismatched resumed identities, and
retains its existing immutable-attempt/held-lease transaction. Crew context
restoration rechecks the project binding, so removal or rebinding cannot silently
expose another member's memory through an old session. Plan and result operations
already pass through that context authorization.

A new native project-chat fixture exercises actual authenticated command
admission, queue lease and Crew preparation, repeat preparation, foreign-owner
rejection, missing Crew mapping, manual-assignment conflict and membership
revocation without rewriting the admitted member. This test is included in Crew
CI and is not yet verified. A legacy profile without a Crew mapping must be
explicitly mapped before it can run through this private native Crew path; the
profile migration and its UI remain open.

This is selection/authorization for already admitted private project-chat tasks.
The Workjet general-project producer and external controller are still missing;
this does not claim that the full Dev submission flow is available. Group-chat
routing is not reduced to the first worker in its membership list.


## MCP ingress for private project execution

`business_os.start_crew_execution` accepts an existing private `thread_id`,
`title`, `instruction`, a supported external `harness`, `timeout_seconds` and
`idempotency_key`. It resolves the authenticated actor and checks the current
native project, private chat, membership, profile, computer and Crew records.
Callers cannot supply a Crew member, executor identity or session grant. The
existing `ctox` system command namespace carries `business_os.chat.task`; no
application module is invented for a repository project.

The command ID derives from owner, private chat and retry key. A stored request
fingerprint rejects a retry with changed text, harness or native target. The
existing command plane owns queueing and receipts. The response contains command,
chat, Crew and computer identifiers plus native status/task reference; it does
not contain execution credentials. Native admission and later context access
compare the pinned Crew member and executor with the current project binding.
Command-scoped sessions cannot start independent project work, including when
they are not marked Crew-only.

The project regression now enters through the real MCP dispatcher, repeats the
request, rejects changed intent and caller-supplied identity, checks the canonical
command, then performs native queue leasing and Crew preparation. This is test
source pending CI, not evidence of a running Workjet controller. The Workjet
request ledger, submission client, offer consumer, secure session transport and
harness context delivery remain to be connected. The general-project gap stated
above is therefore narrowed on the native side only.

The ingress also compares the canonical fingerprint after command acceptance.
This covers an intake race where another request wins after the preflight read
and the command plane returns that request's existing receipt. The ingress must
not acknowledge a competing intent as its own success. Current sequential retry
coverage does not constitute a deterministic concurrent-intake regression; that
additional runtime evidence remains outstanding.
