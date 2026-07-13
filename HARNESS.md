# CTOX Harness

This document describes how the harness works in the current source tree.
There are three related layers:

1. The CTOX service harness in `src/core/service/service.rs` leases durable work,
   runs one bounded worker slice, reviews the result, records state, and then
   releases or requeues the durable item.
2. The forked Codex runtime under `src/core/harness/` provides the in-process agent
   runtime, tools, thread control, subagents, state store, hooks, policy checks,
   and API/client crates.
3. The harness-flow feature in `src/core/service/harness_flow.rs` is an
   observability renderer. It does not execute work. It reads runtime evidence
   and renders the current work path as JSON or ASCII for CLI, TUI, desktop, and
   web surfaces.

The core design rule is still durable-state first: prompts may describe work,
but completion, review, retries, subagent activity, spawn edges, and outcome
evidence must be explainable from persisted state, not from assistant prose.

## Runtime State

Core runtime state lives in `runtime/ctox.sqlite3`. The central path helper is
`paths::core_db(root)`. `mission_db(root)` and `lcm_db(root)` are aliases for the
same file, used for call-site clarity.

That single database contains mission-side and LCM-side tables: queue,
channels, tickets, governance, schedules, plans, approval nag state, knowledge,
messages, summaries, continuity documents, mission state, verification, claims,
process-mining tables, core transition proofs, spawn edges, and harness-flow
events.

Historical `runtime/cto_agent.db` and `runtime/ctox_lcm.db` paths are migration
inputs only. Tool-owned stores stay separate when the tool owns the lifecycle,
for example `runtime/ticket_local.db` and `runtime/ctox_scraping.db`.

## Business OS Command Architecture

Lifecycle-v2 command work is gated by three accepted decisions:

1. The canonical command inbox, lifecycle aggregate, command-to-queue link,
   transition/effect ledgers, and projection outbox belong in
   `runtime/ctox.sqlite3`. Queue-backed admission and its queue link must share
   one SQLite unit of work there. `runtime/business-os.sqlite3` remains domain
   storage plus a compatibility/materialized command view, and
   `runtime/business-os-rxdb.sqlite3` remains the replicated document store.
   Until that migration is complete, no path may claim cross-WAL atomicity.
2. The browser owns immutable command intent until native observation. Native
   owns lifecycle, result, errors, attempts, and projection version afterward.
   Whole-document LWW alone is insufficient: the collection must enforce that
   ownership boundary or split intent and native state. Reusing a command id
   with a different immutable payload hash is an idempotency conflict.
3. Lifecycle-v2 fields first travel as shadow fields under the existing
   `additionalProperties: true` Business OS schemas. No required field, index,
   or schema-version change is allowed during the mixed-version window. Peers
   use the optional `ctox-command-lifecycle-v2` handshake capability; a later
   declared schema change requires a coordinated cutover or a parallel v2
   collection because schema-hash skew quiesces replication.

The native router is the authoritative command classifier. Explicit control
arms and registered control-family predicates execute without an execution
queue task; all other commands pass the policy chokepoint and enter the
queue-backed `record_command` path. Runtime-installed modules may introduce
new queue-backed command types, so browser code must not hard-code a complete
type list or infer command mode from `task_id`. Lifecycle v2 uses
`execution_task_id` for the executing queue task and `target_task_id` or
`target_record_id` for domain targets.

## Service Boot

`run_service` initializes the durable stores, opens the LCM engine on
`runtime/ctox.sqlite3`, runs boot invariants, releases stale communication
leases, and starts the long-lived service loops:

- channel router
- channel syncer
- mission maintenance loop
- harness audit watcher
- working-hours dispatcher
- optional backend supervisor prewarm

The service listens on its IPC socket/HTTP endpoint and keeps the control plane
cheap while idle. Local model runtimes are usually started on demand by agent
turns, unless prewarm is explicitly enabled.

## Work Sources

The harness does not rely on an in-memory work queue as the source of truth.
Workers are derived from durable sources such as:

- communication messages and `communication_routing_state`
- queue tasks created by plans, schedules, channels, or maintenance paths
- ticket events and `ticket_self_work_items`
- founder/owner communication review and resend work
- guard or repair paths such as queue pressure, state invariant repair, timeout
  retry, and outcome-witness recovery

The channel router does not lease new external work while a worker is active.
It first reconciles ticket state, runs dispatch preflights, emits due schedules,
syncs configured ticket sources, routes assigned internal work, and only then leases
bounded inbound work into `QueuedPrompt` slices.

Queue leasing is `pending`-only. The router writes the lease owner, lease time,
and 15-minute expiry; Business Command transitions to `leased` and `running`
validate and preserve those fields. A lease missing any of them is invalid and
is reconciled immediately: the linked command returns to `retry_wait` and the
same queue row returns to `pending`. Complete leases are renewed every 60
seconds while their worker remains active and are reclaimed after expiry
independent of the prior owner. Boot recovery uses that same typed boundary:
if a crash happened after a worker result reached `awaiting_review` but before
review completed, the command moves to `retry_wait` while the linked queue
lease, owner, timestamps, and expiry are cleared together. A restart can then
open a new finite attempt without reviving a terminal command or losing the
prior result evidence.

Idle routing uses source stamps for economy, but source stamps are never the
only wake signal. The lightweight preflight gate and the full router gate each
perform an uncached durable `pending` count at most every 30 seconds. This
bounds dispatch latency when a WAL write does not change the cached filesystem
stamp, without returning to continuous full-router scans.

## Worker Slice Flow

`start_prompt_worker(...)` is the outer harness entry point for a leased slice.
It runs in a thread and performs the following gate sequence before model
execution:

1. Suppress known fatal harness-loop prompts.
2. Hold work outside configured working hours.
3. Skip superseded internal work prompts.
4. Redirect owner-visible work that needs strategic setup.
5. Redirect platform work that needs expertise passes.
6. Build the execution prompt, root, workspace, and conversation id.
7. Call `turn_loop::run_chat_turn_with_events_extended_guarded(...)`.

Plan-step messages force continuity refresh because they are task boundaries.
Normal worker slices reuse one named, non-ephemeral harness thread. Its rollout
is resumed after a service restart. Jobs with a replacement base prompt or a
narrow no-MCP profile remain deliberately isolated sessions because they have a
different capability/instruction contract. A queue job's workspace is applied
as the typed per-turn cwd rather than encoded only in prompt prose.
Before reuse, the worker compares the current composed base instructions and
model with the live session contract. A mismatch rebuilds the process-local
client and resumes the durable thread with the new contract.

Turn timeout defaults follow the resolved provider boundary. Native local
inference keeps the long local budget; a local proxy/process that resolves to
a remote API provider (for example Pi driving MiniMax) uses the 180-second
remote budget. Process locality alone must not grant a remote API call the
one-hour local-inference timeout.

## Inner Agent Turn

`run_chat_turn_with_events_extended_guarded(...)` owns the inner turn:

1. Resolve runtime settings and ensure a local backend is ready when needed.
2. Start or reuse a `PersistentSession`.
3. Open `LcmEngine` on `runtime/ctox.sqlite3`.
4. Persist the user turn.
5. Build a turn plan and run pre-turn LCM compaction if the LCM snapshot is over
   threshold.
6. Render live context from messages, continuity, mission state, assurance,
   governance, and context-health evidence.
7. Send the exact current task as the user input and the freshly rebuilt CTOX
   runtime context as marked developer instructions.
8. Invoke the model through the persistent session.
9. Persist the assistant turn.
10. Detect durable state transitions caused by tool calls.
11. Refresh continuity if forced by a task boundary/state transition or by the
    configured legacy interval.

Direct-session model events write token and timing forensics to
`runtime/context-log.jsonl`. Worker failures are persisted as structured
`messages.agent_outcome` values rather than by scraping assistant text.

## Context Protection

There are three context-protection mechanisms in the current code:

- Live turns materialize only the current bounded LCM working set (referenced
  context items, active summaries, and fresh tail). Full message/commit history
  is reserved for retrieval, audit, and maintenance commands; recent forgotten
  context reads diff text only.
- LCM pre-turn compaction, controlled by `src/core/context/lcm.rs`, compacts stored
  conversation history before rendering a prompt when the LCM snapshot crosses
  the configured threshold. The default threshold is 0.75.
- Direct-session compaction, controlled by `src/core/context/compact.rs`, watches
  model token events during the live turn. Emergency compaction fires when
  per-call input reaches 75 percent of the context window. Adaptive compaction
  defaults to a 15 percent visible-output/read-input drift threshold, with a
  minimum of 4096 visible output tokens. The semantic controller requests
  structured model output; provider-only schema/format failures fall back to a
  bounded recent narrative plus the unchanged active task. They do not fail or
  replay the Business task. Transport and context-window failures keep their
  normal retry/terminal behavior. Compaction cannot change the configured
  provider/model contract; model-tier candidates in its envelope are
  diagnostics only.

Before every model request, history normalization keeps only the newest marked
`<ctox_runtime_context ...>` developer section. It does not delete real user
turns, assistant/tool history, or unrelated developer policy. This gives the
per-turn context replace semantics without introducing a second history type or
making LCM and the harness compete as memory owners. The exact preflight counts
base instructions, the current runtime context, and the user input; on a reused
thread it also projects from the last model-reported input usage and compacts
before the safe boundary is crossed.

The build order, contribution contract, omission rules, and audit checks for
every context component are documented in the
[Context Build Contract](docs/context-build.md).

The default context window is 262144 tokens (256k) when no runtime/model value
overrides it. 256k is the only supported chat-context choice; the retired 128k
option is gone, and legacy persisted 128k values fall back to the 256k default.
The installer seeds `max_seq` per model from the manifest context caps in
`contracts/models/` (262144 for `Qwen/Qwen3.6-35B-A3B`, 131072 for the other
local profiles); the 256k chat-context default is independent of those engine
caps.

## Continuity Refresh

Continuity documents live in the consolidated runtime database. The model's
normal reply text does not update them.

Refresh builds prompts for:

- `narrative`
- `anchors`
- `focus`

The model is expected to call:

```bash
ctox continuity-update --conversation-id <id> --kind <narrative|anchors|focus> --mode <full|replace|diff>
```

The refresh driver verifies the head commit id before and after the model call.
If the CLI tool did not advance the persisted document, no refresh is counted.
Anchor refresh also preserves recent anchor literals after the tool-driven
update path.

Refresh demand and recovery live in `continuity_refresh_status`, keyed by
conversation and continuity kind. Task/plan/knowledge/focus/communication
boundaries set idempotent pending triggers; failures retain pending state with
backoff across restart, and consumption requires an actual head advance.
Communication-only threads make narrative and anchor refresh due no later than
their eighth successful turn. Process-local counters are display telemetry
only and do not decide refresh behavior.

## Review And Outcome Gates

A successful model turn does not automatically close work. The service starts a
completion review unless the source is internal queue-guard maintenance. The
reviewer runs as a separate skeptical pass over the worker result and returns a
typed disposition:

- `Approved`
- `Hold`
- `NoSend`
- `RequeueInternalWork`
- `FeedbackRetry`
- `TerminalQueueFailure`
- `None`

`Hold` always carries a typed `HoldReason`. `WaitingExternal(WaitRef)` is
persisted as dormant blocked work and can only be woken by its referenced
event. Technical, missing-review-evidence, and missing-artifact holds consume
the finite review budget with exponential backoff; they cannot cycle forever.
Their linked Business OS command remains `retry_wait`, not `blocked`, so the
same command/task identity can be leased again after the durable cooldown.
Only `WaitingExternal` remains dormant `blocked` until its referenced event.

The hold transition is one SQLite transaction for queue-backed Business OS
commands. It moves the command (`retry_wait`, `blocked`, or terminal `failed`),
the linked queue route, the lease fields, the retry counter/backoff, and the
typed hold metadata together. In particular, a successful model turn whose
typed result or review evidence cannot be persisted must not leave a
`running` command behind a `pending` queue row. Either the complete hold is
durable or none of it is.

Queue acquisition follows the same commit-boundary rule: ancillary thread
refresh runs before the lease transaction, and the leased task view is loaded
inside that transaction. A caller therefore cannot receive an error after the
lease has already committed and accidentally leave a durable task without an
active or buffered worker.

Post-review writeback uses the inverse ordering. Chat/artifact data may be
staged while the command is `validating`, but active Business OS command/queue
compatibility projections are published only after the terminal owner commits
`validating -> terminal` and `queue=handled`. A native peer can therefore not
replay a stale leased projection as an illegal `validating -> leased` edge.

The router's one-hour unchanged-source idle gate has a separate, cheap durable
queue safety poll every 30 seconds. This uncached count is the WAL-safe wakeup
backstop: a pending queue row cannot wait for a main-database file stamp or for
the full idle-safety interval. When the count is zero, only the small poll timer
is renewed and the full router remains asleep.

Only approved work can move into terminal handling. For communication and other
artifact-producing work, the outcome witness checks that required durable
artifacts exist. The service then enforces reviewed terminal success through
the core state machine. If that proof is missing or rejected, the slice is not
closed merely because the assistant said it was done.

Answer-only work is still reviewed, but it does not require proof of a side
effect that the task never requested. A reviewer may return `PASS_PROOF:
direct` only after independently inspecting the task contract, source material,
and answer (`source=reviewer`, `method=inspect_artifact`). Worker prose alone,
including `PASS_PROOF: prose_only`, cannot pass. Any claim that a file, command,
message, deployment, record, or other state changed still requires
authoritative evidence from that system.

Typed Business OS `business_os.chat.task` commands in `mode=data`, without
dependencies or attachments, use a bounded semantic review scope: no tools,
no workspace/runtime/mission loading, and one 120-second review turn. This
narrows evidence gathering but does not skip review. The task goal and contract
come from the original durable command payload (`title` plus the first present
text contract in `instruction`, `prompt`, `user_message`, or `body`), never
from the worker prompt after workspace instructions, execution contracts, or
retry feedback have been added. If no durable text contract exists, the task
falls back to full evidence review. If the task or answer claims a side effect,
the semantic reviewer must fail it; action-mode commands remain on the full
evidence path.

Rejected or incomplete work is fed back into the same durable queue item or
internal work item where possible. The review path has finite retry budgets and eventually
fails terminally instead of creating unbounded review/rework cascades.

Transient model/API failures also keep the original durable identity. A typed
Business OS command moves from `running` to `retry_wait` before its linked queue
lease returns to `pending`; the same cooldown is persisted in queue metadata and
`communication_routing_state.retry_not_before`. After the cooldown, the same
task and command may enter a new attempt through `retry_wait -> leased -> running`.
The command is never reset to `queued`, duplicated, or terminalized merely
because the provider returned a retryable error such as HTTP 429.

## Core State Machine

`src/core/service/core_state_machine.rs` defines the static review-harness model.
The success path is:

```text
Queued -> Leased -> Running -> AwaitingReview -> ReviewQueued -> Reviewing
  -> ReviewPassed -> AwaitingValidation -> Validating -> Passed
```

Failure or non-terminal paths include model failure, infra failure, reviewer
unavailable retry, review rejection into rework, validator failure into rework,
and exhausted retry budgets.

The analyzer proves:

- `Passed` is the only terminal success state.
- Every path to `Passed` crosses both review pass and validator pass.
- reviewer unavailable cannot advance success.
- rejected review can only enter rework.
- rework can only requeue the same main work or end as model failure.
- every cycle consumes a finite budget.

## Durable Spawns

Durable internal spawns are checked by `src/core/service/core_transition_guard.rs`.
Accepted and rejected attempts are persisted in `ctox_core_spawn_edges`.

Every accepted spawn requires a registered contract, stable parent and child
entity ids, the registered parent/child type pair, and a finite budget when the
contract requires one. Cycles and review-named spawn kinds also require finite
budget evidence.

Current contract families:

| Pattern | Parent | Child | Bound |
| --- | --- | --- | --- |
| `self-work:*` | `ControlPlane`, `Message`, `QueueTask`, `Thread`, `WorkItem` | `WorkItem` | <= 64 |
| `self-work-queue-task` | `WorkItem` | `QueueTask` | <= 64 |
| `queue-task` | `ControlPlane`, `Message`, `Thread`, `WorkItem` | `QueueTask` | <= 64 |
| `plan-step-message` | `PlanStep` | `Message` | <= 8 |
| `schedule-run-message` | `ScheduleTask` | `Message` | <= 64 |

Unregistered, unstable, cyclic-without-budget, over-budget, and exhausted-budget
spawns are rejected and recorded as evidence.

## Subagents

Subagents are implemented in the forked Codex runtime under `src/core/harness/core`.
The CTOX fork record is `src/core/harness/FORK.md`.

Subagents are leaf workers:

- The parent owns the user-visible task, review, rework, completion, and
  owner-visible claims.
- Subagent sessions do not get recursive collaboration/spawn tools.
- `spawn_agents_on_csv` is removed from subagent sessions.
- Agent-job workers keep workspace tools plus `report_agent_job_result`; they
  do not receive spawn, channel, meeting, acknowledgement, or control-plane
  mutation tools.
- Thread-spawn subagents are bounded by `agents.max_depth` and
  `agents.max_threads`.
- Local model providers serialize subagent work; API-backed providers may run
  parallel work.

The static liveness analyzer in
`src/core/harness/core/src/harness_spawn_liveness.rs` checks thread-spawn,
agent-job-worker, and internal-subagent contracts. Its ranking functions are
`max_depth - child_depth`, `pending_agent_job_items`, and single internal task
invocation respectively.

## Session Capability Profiles

Every new forked-runtime session records a `SessionCapabilityProfile` in its
session metadata. The enforced surfaces are:

| Profile | Effective boundary |
| --- | --- |
| `WorkspaceWorker` | workspace write, network as configured; `runtime/`, `.ctox`, `.codex`, `.agents`, and Git metadata are read-only sandbox subpaths |
| `Reviewer` | authoritative workspace/runtime read-only; a disposable scratch CWD is writable for copied build/check inputs; no patch, channel, meeting, artifact, collaboration, or mutating MCP surface |
| `Planner` | read-only planning surface; no active mutation tools |
| `Summarizer` | explicit `Some([])` dynamic-tool contract and no active tools |
| `AgentJobLeaf` | workspace tools plus `report_agent_job_result`; no spawn/channel/meeting/control-plane mutation |

An explicitly persisted empty dynamic-tool list is authoritative and deletes
older restored dynamic-tool state; it never means “restore defaults”. Durable
proofs, review evidence, approval decisions, secrets, and queue state live
under the protected runtime store and are written only by server-side typed
state-machine commands.

Agent-facing typed Business OS mutations follow the same boundary. In
particular, `ctox business-os commands dispatch` parses the command document in
the worker process but sends it over the local service socket for validation,
admission, projection, and evidence persistence by the daemon. The worker CLI
does not open the CLI turn ledger or write either runtime database directly.
If a service socket exists but cannot be reached, dispatch fails closed; direct
in-process dispatch is reserved for explicit offline use without a service
socket.

## Harness Flow Renderer

`ctox harness-flow` is an evidence renderer, not the executor.

CLI shape:

```bash
ctox harness-flow [--latest] [--message-key <key>] [--work-id <id>] [--width <n>] [--json]
ctox harness-flow init
ctox harness-flow events [--message-key <key>] [--work-id <id>] [--ticket-key <key>] [--limit <n>]
```

The renderer opens `runtime/ctox.sqlite3` read-only for normal rendering. It
selects a seed message or internal work item, loads related queue routing,
internal work state, review approvals, core proofs, process-mining violations, continuity
counts, knowledge counts, verification counts, and recent harness-flow ledger
events. It also reads the latest token usage event from
`runtime/context-log.jsonl`.

The ASCII flow keeps the main work on the left spine:

```text
TASK
  -> QUEUE PICKUP
  -> CONTEXT
  -> KNOWLEDGE
  -> HARNESS LEDGER
ATTEMPT 1
  -> REVIEW
  -> TICKET BACKLOG
ATTEMPT 2 / REWORK
  -> SOURCE FROM TICKET BACKLOG
  -> QUEUE RELOAD
FINISH / CURRENT STATE
  -> HARNESS STATE MACHINE
  -> SEND / CLOSE GUARD
  -> VERIFICATION
  -> PROCESS MINING
```

The flow ledger table is `ctox_harness_flow_events`. It is written from queue,
ticket, knowledge, and communication review paths through
`record_harness_flow_event_lossy(...)`. Current event families include review
approval/no-send, queue cleanup, queue spill/restore, knowledge load,
internal work create/publish/transition/state-set. The renderer also appends a
synthetic token-usage event from the context log when one is available.

Desktop reads the flow by executing the local `ctox harness-flow` CLI and falls
back to a reduced SQL renderer when the binary is unavailable. Business OS
serves the same evidence through `/api/business-os/ctox/harness-flow` and its
CTOX module renders that payload inside the app shell.

## Process Mining And Harness Mining

Process mining records compact command and state evidence in
`ctox_process_events` and related views. SQLite read events are off by default;
write and transition evidence remains active. The harness-flow process-mining
branch reports total process events and sqlite-access debug events.

The service also starts a harness audit watcher. It periodically builds a
harness-mining brief and writes confirmed findings to `ctox_hm_findings` through
a two-tick gate. The audit watcher is read-only against domain tables and writes
only harness-mining findings and audit-run records.

Useful commands:

```bash
ctox process-mining core-liveness
ctox process-mining spawn-liveness
ctox process-mining spawn-edges --limit 50
ctox process-mining guidance --limit 50
ctox process-mining prune --sqlite-access-window 200000
```

`spawn-liveness` combines the core durable-spawn analyzer with the forked
Codex subagent analyzer and exits non-zero when either layer is not provably
bounded.

## Business OS Acceptance Bench

`ctox business-os harness-bench` provides 100 small live tasks across ten
Business OS families. Eighty cases must finish with a reviewed chat answer;
ten must create a durable approval plus Threads notification; ten must create
a visible escalation plus notification. `status` reports any terminal task
without its required route as `lost_between_chairs`.

Human-route evidence reads are scoped to the selected run's creation time.
They must not use the collection-wide 2,000-record window, because a long-lived
instance can otherwise omit a fresh notification and produce a false loss.
The inverse false positive is also forbidden: approval/escalation evidence by
itself does not settle a case while its command is still `running` or
`awaiting_review`. `awaiting_human` requires the durable Threads evidence and
the routing command/queue boundary to be non-executing: either the command owns
the typed `blocked` wait, or it has completed after durably creating the
separate approval/escalation aggregate.

```bash
ctox business-os harness-bench catalog
ctox business-os harness-bench run --confirm-live --run-id <id> --actor <requester> --reviewer <other-user>
ctox business-os harness-bench status --run-id <id> --fail-on-inflight
```

## Source References

These are the main implementation files for the current harness behavior:

| Area | File |
| --- | --- |
| Runtime DB paths | `src/core/paths.rs` |
| Service boot and worker dispatch | `src/core/service/service.rs` |
| Inner turn loop and continuity refresh | `src/core/execution/agent/turn_loop.rs` |
| LCM history compaction | `src/core/context/lcm.rs` |
| Direct-session compaction policy | `src/core/context/compact.rs` |
| Core transition and spawn guard | `src/core/service/core_transition_guard.rs` |
| Review-harness state model | `src/core/service/core_state_machine.rs` |
| Process-mining CLI | `src/core/service/process_mining.rs` |
| Harness-flow CLI and renderer | `src/core/service/harness_flow.rs` |
| Desktop harness-flow view | `src/apps/desktop/src/views/harness_flow.rs` |
| Desktop CLI/fallback flow reader | `src/apps/desktop/src/db_reader.rs` |
| Business OS app shell | `src/apps/business-os/` |
| Business OS harness-flow endpoint | `src/core/business_os/server.rs` |
| Forked Codex runtime record | `src/core/harness/FORK.md` |
| Subagent liveness analyzer | `src/core/harness/core/src/harness_spawn_liveness.rs` |
| Subagent thread control | `src/core/harness/core/src/agent/control.rs` |
| Subagent spawn guards | `src/core/harness/core/src/agent/guards.rs` |
| Tool surface selection | `src/core/harness/core/src/tools/spec.rs` |
| Agent-job workers | `src/core/harness/core/src/tools/handlers/agent_jobs.rs` |
| Harness-flow event writers | `src/core/mission/channels.rs`, `src/core/mission/queue.rs`, `src/core/mission/tickets.rs` |

If this document drifts, update it from these files first. Do not paper over
missing liveness, review, outcome, or spawn evidence with prompt text.
