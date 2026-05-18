# RFC: Local IPC Runtime Architecture Without Local HTTP Proxy

Status: Draft

Owner: CTOX

Last updated: 2026-04-18

## Summary

CTOX will remove the local HTTP proxy from its runtime architecture.

The current system exposes a local OpenAI-compatible HTTP surface on loopback and routes local and remote model work through that boundary. The target system replaces that boundary with a private local IPC gateway and a capability-oriented runtime contract.

After this change:

- CTOX will not expose a local model HTTP server on `127.0.0.1` or `localhost`
- local model runtimes will be reachable only over private local IPC
- remote/API-backed runtimes will also sit behind the same local IPC gateway contract
- `responses`, `embeddings`, `transcription`, `speech`, and `vision describe` will all use one internal runtime contract family
- runtime selection remains curated and slot-based rather than open-ended

This RFC is the implementation-preparation document for that migration.

## Decision

The local HTTP proxy is removed as a hard architectural requirement.

This is not a soft preference. The migration plan in this RFC assumes that the following current behaviors are transitional and will be deleted:

- `ctox serve-responses-proxy`
- local telemetry and health over HTTP
- local loopback routing for chat, embeddings, STT, TTS, or vision
- local `CTOX_PROXY_PORT` as the primary runtime integration boundary

## Goals

1. Replace the current local HTTP boundary with a private local IPC boundary.
2. Keep one canonical runtime capability surface for all internal inference work.
3. Support the same product capabilities across local and remote execution paths:
   - text in
   - vision in
   - speech in
   - text out
   - speech out
   - embeddings
4. Preserve fixed subprocess runtime management for local engines.
5. Make remote/API-backed runtimes explicit runtime roles rather than a special forwarding case.
6. Keep the curated model and provider matrix.
7. Preserve current high-level CTOX behavior during migration.

## Non-Goals

1. Introduce open model-loading or arbitrary provider composition.
2. Preserve compatibility for the old local HTTP surface.
3. Re-architect the orchestration layer, continuity layer, queue, or mission model.
4. Change the user-facing product promise around persistent missions.
5. Unify all execution engines into one binary if that increases deployment risk.

## Current State

The current runtime boundary is mixed:

- a local loopback HTTP proxy is a first-class runtime component
- local engines may also be reachable through Unix sockets or named pipes
- some API-backed flows are handled directly by execution-engine provider wiring rather than going through the same local gateway
- some product consumers call local STT/TTS endpoints directly over localhost HTTP

The current architecture is therefore only partially aligned with a strict single-boundary runtime model.

## Problems To Solve

### 1. Boundary inconsistency

CTOX currently has more than one effective inference boundary:

- local loopback HTTP
- local socket IPC
- direct API provider wiring

This increases complexity in routing, testing, telemetry, and failure handling.

### 2. Proxy-centered module coupling

Several modules assume the existence of a local HTTP proxy.

That makes proxy removal a cross-cutting change rather than a narrow refactor.

### 3. Capability fragmentation

Local IPC currently covers only part of the needed capability set cleanly.

`responses` and `embeddings` have a usable local IPC shape today. STT, TTS, and vision orchestration still lean on HTTP-era assumptions.

### 4. Remote/API path asymmetry

Remote providers are currently mostly modeled as upstream URLs plus adapter logic, not as an explicit runtime role with the same lifecycle semantics as local runtimes.

## Target Architecture

### Topology

```text
+------------------------------- CTOX ----------------------------------+
|                                                                       |
|  Session / Tools / UX / Agent Logic / Memory / Retrieval              |
|                                                                       |
|  +---------------------- Runtime Gateway --------------------------+   |
|  | capability router                                           |   |   |
|  | stack planner                                               |   |   |
|  | session canonicalizer                                       |   |   |
|  | execution orchestrator                                      |   |   |
|  | runtime supervisor                                          |   |   |
|  +---------------------------+---------------------------------+   |   |
|                              | private local IPC                  |   |
+------------------------------+------------------------------------+---+
                               |                                      |
                               v                                      v
                     +----------------+                    +----------------+
                     | Candle Runtime |                    | API Runtime    |
                     | subprocess     |                    | subprocess     |
                     +----------------+                    +----------------+
```

### Runtime roles

The target runtime layer has these roles:

- `gateway`
- `candle`
- `api`

The role names are architectural. They do not require these exact binary names, but they must exist as explicit runtime kinds in code and lifecycle handling.

### Transport rules

Allowed local transports:

- Unix domain sockets on Unix platforms
- named pipes on Windows

Disallowed for normal operation:

- loopback HTTP
- loopback TCP runtime endpoints

`TcpLoopback` may remain temporarily during migration behind an explicit compatibility path, but it is not valid in the final architecture.

## Internal Runtime Contract

The current local IPC envelope should be generalized into the canonical runtime contract family.

### Request envelope

All runtime requests use a typed envelope:

```json
{
  "kind": "<operation>",
  "request_id": "<uuid-or-similar>",
  "session_id": "<optional-session-id>",
  "payload": { ... }
}
```

### Required operations

The first supported operation set is:

- `responses_create`
- `embeddings_create`
- `transcription_create`
- `speech_create`
- `vision_describe`
- `runtime_health`
- `runtime_telemetry`

### Responses create

This remains the canonical agent-facing execution operation.

Requirements:

- supports streaming
- supports tool calls
- supports multimodal input blocks
- supports model selection only through curated runtime selection

### Embeddings create

Requirements:

- batch inputs
- typed success response
- usage counters

### Transcription create

Requirements:

- file bytes or chunk bytes input
- optional language hint
- optional prompt hint
- structured text response

### Speech create

Requirements:

- text input
- voice selection within curated runtime config
- output format metadata
- binary or chunked audio payload support

### Vision describe

Requirements:

- image bytes or image reference input
- deterministic descriptive text output for downstream non-vision runtimes
- explicit failure result rather than silent image dropping

This operation keeps the current useful guarantee:

- tools can always evaluate images

But it models that path honestly as an auxiliary capability rather than pretending every runtime is natively vision-complete.

### Health and telemetry

Health and telemetry must move off HTTP.

The final system must support:

- `runtime_health`
- `runtime_telemetry`

through the same local IPC family used for model work.

## Stack Policy

The stack planner remains curated.

The target system evaluates only pre-approved stack combinations.

Policy modes to support:

- `local_only`
- `local_preferred`
- `remote_preferred`
- `auto`
- `offline_only`

Selection inputs:

- required capabilities
- host platform
- runtime availability
- locality policy
- lifecycle state
- latency and cost policy
- locale and voice requirements

## Lifecycle Model

Support is managed by stack slots and provider packages, not by permanent commitment to individual checkpoint names.

Lifecycle states:

- `candidate`
- `canary`
- `active`
- `maintenance`
- `deprecated`
- `removed`

This lifecycle layer is required so CTOX can replace underlying models without changing the orchestration contract.

## Planned Code Changes

### A. Contract layer

Extend the local IPC schema and runtime abstractions:

- `tools/model-runtime/server/src/local_ipc.rs`
- `src/execution/models/local_transport.rs`
- `src/execution/models/runtime_kernel.rs`

Needed changes:

- add typed operations for transcription, speech, vision, health, telemetry
- define streaming and non-streaming result semantics
- define runtime error codes

### B. Gateway layer

Refactor the current gateway logic away from `tiny_http` and URL routing:

- `src/execution/responses/gateway.rs`

Needed changes:

- split HTTP-serving concerns from routing/orchestration logic
- preserve adapter rewriting logic
- expose the gateway over local IPC instead of HTTP
- use transport-agnostic request dispatch internally

### C. Supervisor layer

Update runtime startup and ownership handling:

- `src/execution/models/supervisor.rs`
- `src/execution/models/runtime_contract.rs`
- `src/execution/models/runtime_control.rs`

Needed changes:

- remove proxy-process lifecycle management
- add lifecycle management for IPC gateway runtime
- add explicit API runtime role
- stop persisting proxy-port assumptions as the primary boundary

### D. Remote/API runtime layer

Turn remote provider execution into an explicit runtime role rather than a gateway forwarding special case.

Likely files:

- `src/execution/models/runtime_state.rs`
- `src/execution/models/model_adapters/*`
- `src/execution/agent/turn_loop.rs`
- `src/execution/agent/direct_session.rs`

Needed changes:

- represent API runtime as a real runtime binding
- stop pointing provider config at `http://127.0.0.1:12434/v1`
- route adapter-mediated remote work through the IPC gateway contract

### E. Capability consumers

Migrate direct localhost consumers:

- `src/mission/communication_jami_native.rs`
- `src/mission/communication_meeting_native.rs`
- any other local STT/TTS callers

Needed changes:

- replace local HTTP requests with IPC client helpers

### F. Cleanup

Delete legacy proxy artifacts:

- CLI command `serve-responses-proxy`
- proxy PID handling
- proxy telemetry endpoint
- proxy health checks
- `CTOX_PROXY_PORT` as the primary runtime integration concept

## Migration Plan

### Phase 0: Freeze and prepare

Goal:

- stop adding new local HTTP runtime consumers

Deliverables:

- this RFC
- implementation checklist
- module ownership map

Exit criteria:

- the target contract and phase boundaries are accepted

### Phase 1: Capability-complete IPC contract

Goal:

- extend local IPC to cover every required runtime capability

Deliverables:

- typed request and response envelopes
- health and telemetry operations
- STT, TTS, and vision operations

Exit criteria:

- every local capability can be exercised without localhost HTTP

### Phase 2: Transport-agnostic gateway core

Goal:

- make gateway orchestration independent of HTTP serving

Deliverables:

- routing logic callable from IPC server
- adapter rewriting logic retained
- local socket roundtrip path generalized for all relevant operations

Exit criteria:

- gateway logic works with IPC transport only in tests

### Phase 3: Explicit API runtime role

Goal:

- make remote execution a first-class runtime role

Deliverables:

- API runtime binding in runtime kernel/state
- API runtime lifecycle and telemetry
- adapter-mediated remote work reachable through the same gateway contract

Exit criteria:

- MiniMax/OpenRouter/OpenAI/Anthropic/Azure Foundry routing no longer depends on local HTTP proxy

### Phase 4: Consumer migration

Goal:

- remove direct localhost runtime consumers

Deliverables:

- Jami, meeting, and auxiliary capability callers use IPC helpers

Exit criteria:

- repository search no longer finds product runtime calls to `http://127.0.0.1` for local inference capabilities

### Phase 5: Proxy removal

Goal:

- delete the legacy local HTTP runtime boundary

Deliverables:

- remove `serve-responses-proxy`
- remove proxy PID and supervisor paths
- remove proxy health and telemetry endpoints

Exit criteria:

- local runtime system boots and serves only via private IPC

## First Implementation Slice

The first coding slice after this RFC should be:

1. Generalize `local_ipc.rs` from `responses + embeddings` to a capability-complete typed envelope.
2. Refactor `gateway.rs` so the request-routing core no longer depends on `tiny_http::Request`.
3. Add IPC client helpers for:
   - responses
   - embeddings
   - transcription
   - speech
   - vision describe
4. Keep the HTTP proxy temporarily only as a thin compatibility shell over the new gateway core during the migration window.

That last step is transitional only. It exists to reduce migration risk while Phase 3 and Phase 4 land. The final architecture still deletes the shell.

## Implementation Checklist

- [ ] Define the canonical runtime IPC envelope and operation set
- [ ] Extend local IPC server support to `transcription_create`
- [ ] Extend local IPC server support to `speech_create`
- [ ] Extend local IPC server support to `vision_describe`
- [ ] Add IPC `runtime_health`
- [ ] Add IPC `runtime_telemetry`
- [ ] Split `gateway.rs` into transport-neutral orchestration and transport binding
- [ ] Implement IPC gateway server
- [ ] Implement IPC gateway client helpers
- [ ] Add explicit API runtime role to runtime state and kernel
- [ ] Migrate adapter-mediated remote execution to the API runtime role
- [ ] Migrate mission communication STT/TTS consumers off localhost HTTP
- [ ] Remove proxy lifecycle handling from supervisor
- [ ] Remove `serve-responses-proxy`
- [ ] Remove proxy-specific config and readiness assumptions

## Risks

### High risk

- gateway refactor touches the main inference boundary
- API-provider routing has hidden assumptions in direct-session and turn-loop setup
- consumer migration may uncover ad hoc localhost callers outside the main inference tree

### Medium risk

- keeping streaming semantics identical across the old HTTP path and new IPC path
- preserving adapter-specific response rewriting behavior during gateway refactor
- keeping health, readiness, and recovery behavior operational without HTTP probes

### Low risk

- curated model registry and auxiliary role model can remain structurally intact

## Acceptance Criteria

This RFC is implemented when all of the following are true:

1. CTOX no longer exposes a local model HTTP proxy.
2. All local runtime capabilities use private local IPC.
3. Remote/API-backed model work is mediated through the same local runtime gateway contract.
4. No product runtime path requires `CTOX_PROXY_PORT`.
5. Repository search finds no remaining local inference calls to `http://127.0.0.1` or `http://localhost` except tests or explicitly deprecated compatibility fixtures.
6. Startup, health, telemetry, and recovery remain functional.

## Open Questions

These questions should be resolved before Phase 2 is complete:

1. Should the IPC gateway be an in-process CTOX component or its own managed subprocess?
2. Should `vision_describe` remain a dedicated operation, or be modeled as a constrained multimodal `responses_create` call?
3. Does Windows support require a named-pipe-first implementation immediately, or can the first implementation target Unix and keep Windows blocked until its pipe path is ready?
4. How much of the current gateway module should be split into reusable routing submodules before feature migration begins?

## Recommended Immediate Next Step

Start with the contract refactor, not with deletion.

The first code change should create the new capability-complete IPC contract and make gateway routing callable without HTTP. Once that exists, implementation can migrate callers one by one without losing the target shape.
