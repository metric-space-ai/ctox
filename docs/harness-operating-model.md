# CTOX Harness Operating Model

This document describes how the CTOX harness is expected to behave when work
flows through review, rework, task spawning, subagents, and process-mining
checks.

This page defines the orchestration and liveness contract. For the end-to-end
runtime path, persistent worker-session behavior, recovery, and outcome gates,
see [`HARNESS.md`](../HARNESS.md). For the exact model-input construction and
long-session hygiene rules, see [Context Build Contract](context-build.md).

The short version:

- the state machine owns orchestration
- the reviewer is a quality gate, not an independent work orchestrator
- every durable spawn is represented as a parent-child process edge
- every accepted spawn path has a finite budget or a decreasing rank
- subagents are leaf workers
- process mining provides the executable proof that these invariants still hold

The implementation must treat this model as a release-blocking contract. If a
new harness behavior cannot be expressed here, it is not ready to ship as a
hidden side path.

## Runtime Roles

### Main Agent

The main agent owns the user-visible task. It may ask for review after it
believes a work slice is complete, but it remains the owner of the result,
follow-up work, final claims, and closure.

Review feedback is input to the main agent. It is not a handoff to a separate
infinite rework system.

### Reviewer

The reviewer is a checkpoint. It classifies the current artifact and returns a
bounded finding set:

- wording: the artifact is substantively correct but needs language cleanup
- substantive: the artifact needs real content, implementation, evidence, or
  reasoning changes
- stale: the world or queue changed and the artifact must be refreshed,
  obsoleted, or consolidated before continuing

The reviewer must not be modeled as a producer of unbounded internal work. A review
failure can require the main agent to revise the artifact, but the revision path
must remain tied to the same parent entity and must be bounded by the state
machine.

### Spawners

A spawner is any runtime path that creates durable child work: internal work,
queue tasks, plan-step messages, schedule messages, or similar child entities.

Every spawner must have a core contract:

- stable spawn kind
- allowed parent entity types
- allowed child entity type
- finite budget requirement
- maximum budget
- intervention skill
- finite non-spawning intervention effects

The runtime persists accepted and rejected spawn edges in
`ctox_core_spawn_edges`. This makes the process graph inspectable and lets the
kernel reject unregistered, unstable, cyclic, or over-budget spawns.

### Subagents

Subagents are parallel work leaves. They help the parent agent do bounded work;
they do not become independent runtime owners.

Subagent invariants:

- subagents do not receive recursive collaboration tools
- subagents do not receive `spawn_agents_on_csv`
- agent-job workers receive only `report_agent_job_result`
- the parent agent owns review, rework, completion, and owner-visible claims
- the review state machine sees one parent result, not independent review gates
  for each child

Thread-spawn subagents are bounded by `agents.max_depth` and
`agents.max_threads`. Agent-job workers are bounded by a finite persisted item
table and the same concurrency cap.

## Review Gate Flow

The protected review loop is:

```text
Work/Draft Ready
  -> Review Requested
  -> Reviewing
  -> Approved
  -> Continue/Send/Close
```

If review fails, control returns to the main agent with a classified finding:

```text
Reviewing
  -> Wording Rework Required
  -> Drafting same substance, new body hash
  -> Review Requested
```

```text
Reviewing
  -> Substantive Rework Required
  -> Drafting with new substance pointer or new evidence
  -> Review Requested
```

```text
Reviewing
  -> Stale Refresh/Obsolete/Consolidate
  -> Context or queue consolidation
  -> Drafting or Terminal
```

The important property is that the reviewer does not own a new durable cascade.
It may cause a bounded revision, but it cannot recursively create review-owned
internal work that then creates further review-owned internal work.

### Answer-only review

Read, explain, classify, summarize, calculate, and draft-without-send tasks have
no external effect to prove. Their reviewer compares the task contract, source,
and answer directly and records `PASS_PROOF: direct` with
`source=reviewer` and `method=inspect_artifact`. This is independent review,
not acceptance of worker prose. `PASS_PROOF: prose_only` remains invalid, and
the direct method is never sufficient for claimed file, command, message,
deployment, record, or other state changes.

For a typed Business OS data-chat command with no dependencies or attachments,
that comparison runs in a tool-free semantic review session. Its prompt carries
only the original durable command title/text contract and the answer; it does
not reuse the worker prompt after workspace instructions, execution contracts,
or retry feedback were added, and it does not load workspace, runtime,
continuity, mission, or the general review skill. A missing durable text
contract, action-mode command, or data command with dependencies or attachments
stays on the full evidence reviewer.

## Witness Of Progress

Every rejected review must have a witness before the same artifact can pass
through the same edge again:

- wording rework requires a changed body hash
- substantive rework requires changed substance, evidence, or implementation
  pointer
- stale rework requires a changed world pointer, queue consolidation, or a
  terminal no-send/no-action decision

Without a witness, the next transition is not progress; it is a loop candidate.
The kernel must reject or block that path rather than letting the harness
compose the same artifact again.

## Spawn Liveness Model

The mathematical liveness argument uses two mechanisms.

### Finite Budget

Durable internal spawns consume a finite budget keyed to the parent/child
relationship. A path that tries to exceed the contract is rejected with a
bounded intervention message.

The current core model registers these spawn contract families:

| Pattern | Parent | Child | Bound |
|---|---|---|---|
| `self-work:*` | control/message/queue/thread/work item | work item | budget <= 64 |
| `self-work-queue-task` | work item | queue task | budget <= 64 |
| `queue-task` | control/message/thread/work item | queue task | budget <= 64 |
| `plan-step-message` | plan step | message | budget <= 8 |
| `schedule-run-message` | schedule task | message | budget <= 64 |

Any spawn kind that contains `review` also requires a finite budget. Cycles in
the process graph require a finite budget and are rejected when the budget is
exhausted.

### Decreasing Rank

Subagent spawning uses a decreasing rank:

```text
depth_remaining = agents.max_depth - child_depth
```

A child can only exist while the rank is non-negative, and subagent tool
surfaces remove recursive spawn tools. Therefore a subagent path is either a
bounded leaf execution or a rejected spawn request.

Agent-job workers use a different rank:

```text
pending_agent_job_items
```

The job has a finite item table, workers are concurrency-capped, and workers
can only report results. They cannot create more job items by delegation.

## Intervention Contract

When the core spawn guard rejects a path, the intervention is deliberately
small:

- block child
- consolidate into parent
- requeue parent
- mark terminal

Approved intervention skills must state a non-spawning contract. In practice
that means they must not create new durable work, new queue tasks, schedules, or
plan ingests while resolving a spawn violation.

This matters because an intervention that spawns new work would become a second
unmodeled loop source.

## Executable Proof

The executable proof is:

```sh
ctox process-mining spawn-liveness
```

The command returns a combined JSON report:

- `core_spawn_liveness`: registered durable spawn contracts, budgets,
  intervention skill checks, and graph-cycle checks
- `harness_subagent_liveness`: subagent depth/concurrency bounds and leaf-only
  tool-surface checks
- `ok`: true only when both layers pass

The proof is intentionally not a `build.rs` compile-time step. It is a
repository-conformance and release-safety proof, not type checking. Running it
on every `rustc` compile would make normal local iteration slower without
improving the type system.

The correct gates are:

- unit test: catches invariant drift during normal test runs
- CI: blocks changes that break the harness proof
- release workflow: blocks packaged binaries that fail the proof using the
  built runtime

## Developer Checklist

Before adding or changing harness behavior:

1. Model new states and transitions in the state machine.
2. Register every new durable spawner with parent type, child type, finite
   budget, intervention skill, and intervention effects.
3. Keep review as a checkpoint owned by the parent task.
4. Keep subagents leaf-only unless the liveness proof is extended first.
5. Add deterministic tests for new protected edges.
6. Run:

```sh
cargo fmt --check
cargo check
cargo test
cargo run -- process-mining spawn-liveness
```

If `process-mining spawn-liveness` fails, do not paper over the failure with
prompt text. Fix the process model, the transition guard, or the tool surface.
