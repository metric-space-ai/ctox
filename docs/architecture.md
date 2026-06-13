# CTOX Architecture

This document describes the current source-tree architecture after the repository
layout cleanup.

## Repository Shape

The active tree is intentionally narrow:

- `src/core/` contains the CTOX daemon, CLI, TUI, mission systems, service
  loops, harness integration, model gateway, and local inference source.
- `src/apps/` contains app surfaces: Desktop, Business OS, and the web app area.
- `src/tools/` contains supporting source packages such as web, PDF, document,
  and speech tooling.
- `src/skills/` contains the system and user skill catalog and pack management (which is embedded at compile-time and imported into SQLite at service start).
- `src/scripts/` contains only source-side build helpers that are still active.
- `docs/` contains technical documentation, RFCs, legal notices, and the
  GitHub Pages site under `docs/site/`.
- `tests/` contains integration, harness, fixture, and behavior tests.
- `runtime/` is root-level ignored runtime state: SQLite databases, generated
  state, caches, model artifacts, and build output.
- `archive/` is root-level ignored review storage for obsolete or uncertain
  material before deletion or reintegration.

`src/` is source code only. Runtime state, generated data, caches, compiled
artifacts, and archived material do not belong under `src/`.

## System Layers

CTOX has four main layers:

- the CTOX orchestration layer
- the forked in-process Codex harness
- the CTOX model gateway
- curated local inference crates

The persistent system is CTOX. The Codex fork is the current execution core
inside that system, not a separate `codex-exec` subprocess in the intended
service path.

## CTOX Orchestration Layer

The orchestration layer persists, routes, supervises, recovers, and verifies
work across multiple bounded execution slices.

Core responsibilities:

- persistent service and control-plane behavior
- external communication routing
- durable queue, plan, schedule, ticket, and follow-up execution
- long-run mission control
- long-context memory and continuity management
- context optimization and recovery logic
- governance, verification, assurance, and process evidence
- runtime mediation around model and harness execution

Core modules:

- `src/core/service/service.rs`
  Service loop, control surface, pending work, background loops, and prompt dispatch.
- `src/core/mission/channels.rs`
  Multi-channel communication substrate, routing state, sender policy, leasing, acking, and thread context.
- `src/core/mission/queue.rs`
  Durable queue tasks and queue management commands.
- `src/core/mission/plan.rs`
  Persistent multi-step plans, step lifecycle, and step emission.
- `src/core/mission/schedule.rs`
  Time-based work emission and recurring task management.
- `src/core/context/lcm.rs`
  Long-context memory, continuity documents, mission state, retrieval, compaction, and verification persistence.
- `src/core/context/context_health.rs`
  Context scoring, failure-memory checks, repetition detection, and repair guidance.
- `src/core/service/mission_governor.rs`
  Loop governance for repeated blockers and forced repair/replan slices.
- `src/core/mission/follow_up.rs`
  Post-slice follow-up decisions.
- `src/core/mission/review.rs`
  Completion review logic.
- `src/core/mission/verification.rs`
  Verification runs, claims, assurance, and closure-blocking evidence.

## Execution Layer

CTOX executes bounded agent turns through the in-process Codex fork under
`src/core/harness/`. CTOX prepares the context, mission contract, runtime
settings, workspace, and policy environment, then calls the turn loop in
`src/core/execution/agent/turn_loop.rs`.

The execution engine is intentionally treated as a component. CTOX owns
persistence, queueing, review, verification, and continuation around each
bounded turn.

## Model Gateway

The gateway layer gives CTOX and `ctox-core` one stable Responses-shaped model
contract while adapters and runtime control mediate backend differences.

Main responsibilities:

- canonical internal Responses-style contract
- backend-specific request rewriting at the adapter edge
- routing to generation, embeddings, STT, TTS, and vision-aux backends
- telemetry and live switching metadata
- runtime recovery and readiness handling

Core modules:

- `src/core/execution/responses/gateway.rs`
- `src/core/execution/models/supervisor.rs`
- `src/core/execution/models/runtime_env.rs`
- `src/core/execution/models/runtime_plan.rs`
- `src/core/execution/models/runtime_control.rs`
- `src/core/execution/models/runtime_state.rs`

## Local Inference

Curated local inference work lives under
`src/core/inference/models/<model_id>/`. Each curated model crate is intended to
be self-contained: Rust driver code, vendored kernels, loader/adapter code, and
model-specific glue live inside that model crate rather than in a shared engine
subtree.

The promoted local chat support surface is one Qwen model at a time. The
currently supported local chat model exposed by the runtime registry is
`Qwen/Qwen3.6-27B` for CUDA/NVIDIA. Additional Qwen Metal/CUDA crates in the
tree are development or transitional ports unless `model_registry` promotes the
model and `local_model` wires it to a verified server backend.

The TUI/config surface currently names the local runtime family `candle`.
That does not mean the old root-level `tools/model-runtime/` subtree is the
current architecture. Remaining references to that retired external engine
layout should be treated as cleanup work unless code inspection proves an
active migration path still needs them.

## Business OS Data Plane (CTOX DB / ctox-rxdb)

Business OS in the browser and the CTOX daemon exchange ALL business data over
a WebRTC-only replication plane called CTOX DB: the hard-fork Rust crate
`src/core/rxdb/` (rxdb-rs) on the daemon side, the package-manager-free
browser runtime `src/apps/business-os/rxdb/` (ctox-rxdb-js) on the other, and
`src/core/business_os/rxdb_peer.rs` as the supervised native peer. HTTP only
delivers the static shell and bootstrap config — never collection data.

The canonical, code-verified documentation (architecture, wire protocol,
lifecycle, failure semantics, build/test story, agent guardrails) is
`docs/ctox-rxdb.md`. Read it before changing anything in the data plane.

## Runtime State

Runtime state is centralized in `runtime/ctox.sqlite3`, reached through
`src/core/paths.rs::core_db(root)`. Legacy `runtime/cto_agent.db` and
`runtime/ctox_lcm.db` paths are migration inputs only.

Build output, model caches, generated runtime files, and local databases belong
under root-level `runtime/`, which is ignored by Git.

## Feature Placement Rule

Use this rule when deciding where a new feature belongs:

- put the feature in CTOX core if it changes persistence, routing, scheduling,
  continuity, governance, verification, communication, gateway behavior,
  Business OS coordination, web-path policy, or host-side runtime control
- put the feature in the Codex harness if it changes execution semantics inside
  the bounded agent run itself
- put app-specific UI behavior under `src/apps/`
- put source package support under `src/tools/`
- put generated state, caches, models, and local build output under `runtime/`
