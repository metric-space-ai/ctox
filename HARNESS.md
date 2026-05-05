# CTOX Harness

CTOX uses an integrated, in-process agent harness to execute bounded work slices
inside the long-running daemon. The harness is responsible for assembling
runtime context, running an agent turn, recording evidence, refreshing continuity
state when needed, and returning control to the durable mission loop.

The important design rule is that prompts may describe work, but progress,
spawns, review gates, retries, and completion must remain explainable through
durable state transitions and process-mining evidence.

## Runtime Flow

```text
ctox start
  -> daemon/service loop
  -> runtime/ctox.sqlite3
  -> queue, channels, tickets, plans, schedules, governance, LCM state

Producers:
  - TUI and ctox chat input
  - native email routing
  - schedules
  - plan steps
  - ticket synchronization, including Zammad
  - runtime guards such as queue pressure and mission idle watchdog

For each leased work item:
  1. Build the context package.
  2. Run one bounded ctox-core turn slice.
  3. Optionally refresh continuity documents.
  4. Record verification, governance, process, and queue state.
  5. Complete, block, defer, requeue, or lease the next item.
```

The service stores core runtime state in `runtime/ctox.sqlite3`. The central
path helper is `paths::core_db(root)`, and `mission_db(root)` and `lcm_db(root)`
are aliases for the same file. Historical `runtime/cto_agent.db` and
`runtime/ctox_lcm.db` paths are legacy migration inputs only.

Tool-owned stores stay separate when their tool owns the lifecycle, for example
`runtime/ticket_local.db`, `runtime/ctox_scraping.db`, and
`runtime/documents/ctox_doc.db`.

## Context Assembly

Each worker slice receives a bounded context package:

- System prompt: the CTOX runtime contract, tool descriptions, and governance
  rules.
- Focus: the current task, status, next step, blockers, and done gate.
- Anchors: durable facts, paths, decisions, constraints, and external state.
- Narrative: compressed history of prior turns.
- Task prompt: the newly leased work item or continuation instruction.

The continuity documents are persisted in the consolidated runtime database.
The agent's normal reply text does not update them by itself.

## Turn Execution

The inner harness turn is the ctox-core loop:

```text
think -> tool call -> tool result -> think -> tool call -> ... -> turn complete
```

The current service path starts a worker with `start_prompt_worker(...)`. A
single `run_chat_turn_with_events_extended(...)` call owns the main turn and,
when triggered, the continuity refresh calls that reuse the same client for that
slice.

The direct session writes turn and token forensics to
`runtime/context-log.jsonl`. Operator settings and LCM state live in
`runtime/ctox.sqlite3`.

## Compaction And Continuity

CTOX uses two layers of context protection:

- Emergency compaction: `DEFAULT_CONTEXT_THRESHOLD = 0.75`, so compaction starts
  when the call input reaches 75 percent of the configured context window.
- Adaptive continuity refresh: `CTOX_REFRESH_OUTPUT_BUDGET_PCT` defaults to 15,
  so continuity refresh is considered when model output consumes enough of the
  per-call context budget or when a durable state transition is detected.

The default context window seed is `CTOX_CHAT_MODEL_MAX_CONTEXT=131072`, which
is 128K tokens. The value is configurable, and thresholds are computed relative
to the configured window.

Continuity refresh builds prompts for Narrative, Anchors, and Focus. The model
must call:

```bash
ctox continuity-update --kind <narrative|anchors|focus> --mode <full|replace|diff>
```

No update is recorded unless the CLI call changes the persisted document.

## Completion And Continuation

After a turn, the service decides what happens next from durable state:

- completed work is closed only after the relevant state and evidence gates pass
- blocked work is acknowledged or deferred with the blocker recorded
- queued work is leased into another worker slice
- mission-idle watchdog and queue-pressure guard paths can enqueue bounded
  follow-up slices
- legacy timeout continuation jobs are suppressed before model execution because
  recursive timeout continuations can restart timed-out harness turns indefinitely

The timeout path records a `turn_timeout_continuation` governance event and
returns control to the original queue scope instead of blindly spawning another
same-prompt continuation.

## Review Gate, Spawner, And Subagent Liveness

The harness is modeled as a controlled state machine. Review, rework, task
spawns, subagents, and completion must remain bounded and visible in the durable
process graph.

### Roles

- Main Agent: owns the user-visible task, final conclusion, visible claims, and
  completion decision. Review feedback is input to the same parent task.
- Reviewer: acts as a quality gate. It classifies the current result but must
  not own an independent self-work cascade.
- Spawner: creates durable child work and must register a parent-child edge in
  the core process graph.
- Subagent: runs as a parallel leaf worker. Subagents must not become their own
  review and rework orchestrators.

### Review As A Checkpoint

The reviewer can return three finding classes:

- `wording`: the substance is correct, but language or presentation needs
  revision.
- `substantive`: content, evidence, implementation, or reasoning is incomplete.
- `stale`: world state or queue state changed and needs refresh, obsoletion, or
  consolidation.

After review, the Main Agent continues the parent task. The process must not
model review as an unbounded `review -> self-work -> review -> self-work` loop.

Every repeated review pass needs a witness of progress:

- wording rework: a new `body_hash`
- substantive rework: a new substance, evidence, or implementation pointer
- stale rework: a new world pointer, queue consolidation, or terminal no-send /
  no-action decision

Without a witness, the path is a loop candidate, not progress.

### Core Spawner Contract

Every durable internal spawn requires a registered core spawner contract:

- stable `spawn_kind`
- allowed parent entity types
- allowed child entity type
- finite budget requirement
- maximum budget
- intervention skill
- finite, non-spawning intervention effects

Accepted and rejected spawn attempts are persisted in `ctox_core_spawn_edges`.
The kernel rejects unregistered, unstable, cyclic, budgetless, over-budget, and
budget-exhausted spawns.

Current contract families:

| Pattern | Parent | Child | Bound |
| --- | --- | --- | --- |
| `self-work:*` | ControlPlane, Message, QueueTask, Thread, WorkItem | WorkItem | <= 64 |
| `self-work-queue-task` | WorkItem | QueueTask | <= 64 |
| `queue-task` | ControlPlane, Message, Thread, WorkItem | QueueTask | <= 64 |
| `plan-step-message` | PlanStep | Message | <= 8 |
| `schedule-run-message` | ScheduleTask | Message | <= 64 |

Any spawn kind whose name contains `review` also requires a finite budget.

### Subagents

Subagents are leaf-only:

- `SessionSource::SubAgent(_)` sessions lose recursive collaboration and spawn
  tools.
- Subagents do not receive `spawn_agents_on_csv`.
- Agent-job workers keep only `report_agent_job_result`.
- The parent agent owns review, rework, completion, and owner-visible claims.
- The review state machine sees one parent result, not a separate review gate
  per subagent.

Thread-spawn subagents are bounded by `agents.max_depth` and
`agents.max_threads`. Their rank is:

```text
depth_remaining = agents.max_depth - child_depth
```

Agent-job workers are bounded by a finite persisted item table and the
concurrency limit. Their rank is:

```text
pending_agent_job_items
```

## Executable Liveness Proof

Run:

```bash
ctox process-mining spawn-liveness
```

The command emits a JSON report with:

- `core_spawn_liveness`: registered durable spawners, budgets, intervention
  skills, and graph-cycle checks
- `harness_subagent_liveness`: depth and count bounds plus leaf-only tool
  surfaces

The command returns `ok: true` only when both layers are provably bounded.

This proof intentionally does not run in every `rustc` compilation through
`build.rs`. It is a repository conformance and release-safety check, not a type
check. The intended gates are:

- unit tests for normal test runs
- CI gate for pull and main changes
- release gate with a built binary before packaging

If `ctox process-mining spawn-liveness` fails, do not paper over it with prompt
text. Fix the process graph, transition guard, budget contract, or tool surface.

## Verified Code References

These references were checked against the current repository layout.

| Element | File |
| --- | --- |
| CLI dispatch for `process-mining` | [src/main.rs](src/main.rs#L436) |
| CLI dispatch for `continuity-update` | [src/main.rs](src/main.rs#L737) |
| `ctox continuity-update` usage and modes | [src/main.rs](src/main.rs#L1221) |
| Service boot and loop setup | [src/service/service.rs](src/service/service.rs#L626) |
| Channel router and mission maintenance startup | [src/service/service.rs](src/service/service.rs#L635) |
| Worker dispatch function | [src/service/service.rs](src/service/service.rs#L2632) |
| Queue task creation | [src/mission/channels.rs](src/mission/channels.rs#L1661) |
| Inbound message leasing | [src/mission/channels.rs](src/mission/channels.rs#L1455) |
| Ticket-event leasing | [src/mission/tickets.rs](src/mission/tickets.rs#L3400) |
| Consolidated runtime DB path | [src/paths.rs](src/paths.rs#L34) |
| Core spawn-edge table | [src/service/core_transition_guard.rs](src/service/core_transition_guard.rs#L104) |
| Core spawner contracts | [src/service/core_transition_guard.rs](src/service/core_transition_guard.rs#L654) |
| Spawn-liveness report assembly | [src/service/process_mining.rs](src/service/process_mining.rs#L943) |
| Harness subagent liveness analyzer | [src/harness/core/src/harness_spawn_liveness.rs](src/harness/core/src/harness_spawn_liveness.rs#L20) |
| Emergency compaction threshold | [src/context/lcm.rs](src/context/lcm.rs#L18) |
| Default context window fallback | [src/context/compact.rs](src/context/compact.rs#L174) |
| Installer seed for `CTOX_CHAT_MODEL_MAX_CONTEXT` | [install.sh](install.sh#L1600) |
| Continuity refresh driver | [src/execution/agent/turn_loop.rs](src/execution/agent/turn_loop.rs#L634) |
| Direct-session context log | [src/execution/agent/direct_session.rs](src/execution/agent/direct_session.rs#L1189) |
| Timeout-continuation suppression path | [src/service/service.rs](src/service/service.rs#L9599) |

## Consistency Notes

The previous German draft mixed `runtime/ctox.sqlite3` and `runtime/ctox.db`.
The current path helper and active call sites use `runtime/ctox.sqlite3`.

The previous draft also described timeout continuation as if the harness always
created another queue task. The current code suppresses legacy timeout
continuation jobs before model execution and records the guard decision in
governance state.
