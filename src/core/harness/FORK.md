# CTOX Fork Record: openai-codex

Canonical path: `vendor/agent-runtime`

Origin:

- Source origin: `https://github.com/openai/codex.git`
- Imported snapshot: `c6ab4ee537e5b118a20e9e0d3e0c0023cae2d982`
- Integration mode: `hard_fork`

Fork policy:

- This tree is integrated directly into CTOX and is not treated as a package dependency.
- Local modifications inside this subtree belong to the CTOX fork state unless explicitly documented otherwise.
- CTOX must not auto-clone, auto-fetch, or auto-update this subtree from upstream.

## 2026-08 Required Plan and Stable Activity Events

CTOX service-owned queue turns use the upstream-compatible
`required_initial_tool` gate with `update_plan`. This is a CTOX fork integration
contract: before the first accepted plan call, no unrelated tool is available.
The existing `PlanUpdate` event remains the structured source for plan labels
and statuses; no plan is reconstructed from assistant prose.

Lean no-MCP authoring sessions retain `update_plan` alongside the shell and
patch tools. The service requires that plan as the first model-visible action,
so removing it from the lean surface would fail the turn before app authoring
can begin. Read-only lean surfaces remain plan-free.

The direct-session adapter additionally maps stable runtime boundaries into
CTOX worker events. A `PlanUpdate` is both `worker.plan_updated` and one tool
activity. Tool begin events use their call/item identifiers, and reasoning
section boundaries emit `worker.thinking_started` identifiers without emitting
reasoning content. CTOX persists and deduplicates these events outside the fork
in the core LCM tables. Delta fragments, tool completion events, and retries are
not turns. This preserves upstream response streaming while giving CTOX durable
progress evidence and restart-safe UI counters.

Attribution rule:

- When a file under this subtree differs from the imported snapshot, describe it as a CTOX fork delta, not as an ambiguous upstream version.

## 2026-07 Persistent CTOX Runtime Context

CTOX uses the existing turn-context and rollout machinery for a durable normal
worker thread. The app-server `turn/start` request accepts an optional
`developer_instructions` override, persists it in `TurnContextItem`, and emits a
developer update only when it changes. CTOX runtime context is wrapped in the
reserved `<ctox_runtime_context ...>` marker. Between compactions, request
normalization preserves prior marked sections and appends a changed snapshot;
the newest snapshot declares itself authoritative. This keeps the provider
prompt prefix stable for response chaining and prompt caching. Compaction is
the replacement boundary that collapses prior snapshots and re-injects one
canonical current context. Reviewer and deliberately isolated sessions remain
separate cache lineages.

This delta deliberately does not add a second scheduler, memory store, or
CTOX-specific response-item type. Resume and compaction continue to use the
fork's existing thread, rollout, `TurnContextItem`, and `ContextManager`
contracts.

Systematic-research attempts are a deliberate service-owned exception to normal
worker continuity. Each attempt uses a fresh non-persistent standard worker
session so the built-in typed CTOX Web tools remain available. Completion
validation reads the durable rollout and requires a persisted
`ctox_deep_research` receipt from the current attempt. The full Web Stack stays
visible from the first turn so the Systematic Research skill can inventory
Knowledge, choose focused scholarly/search/read tools, and use deep-research as
one adaptive discovery round rather than as a static workflow owner.

The structured compaction controller treats provider formatting failures as a
recoverable model-surface limitation. If a provider returns prose or an empty
payload instead of the requested JSON schema, the fork does not fail or replay
the Business task. It uses a deterministic conservative fallback: retain a
bounded recent narrative, keep the durable task as active focus, and decline
reprioritization. Transport, context-window, and interruption errors remain
fatal/retryable under their existing policies.
The first such format failure switches the remainder of that compaction run to
the deterministic path; later semantic stages are not called redundantly.

Compaction model tiers are diagnostic only. They never switch the model of an
already negotiated session: the global model catalog is not evidence that a
candidate is reachable through the active provider contract.

## 2026-07 Free Subagents Removed

The April subagent backport is not part of the CTOX execution contract.
CTOX-managed sessions force `multi_agent=false`, `enable_fanout=false`, and
`memories=false`. Tool construction and routing independently remove and reject
all free child-agent controls, including `spawn_agent`.

Work decomposition is owned by CTOX durable queue/work-item state. The Coding
Agents module is the sole external-agent exception and remains a distinct
policy-checked Business OS provider channel. Server-owned completion review is
an isolated read-only `Exec` gate, not a child session or parent capability.

When pulling future Codex changes, re-check `core/src/tools/spec.rs`,
`core/src/tools/router.rs`, and managed direct-session overrides. Any change
that makes a collaboration tool model-visible is a release-blocking regression.

Verification commands used for this slice:

```bash
cargo check --manifest-path src/core/harness/Cargo.toml -p ctox-core --tests
cargo test --manifest-path src/core/harness/Cargo.toml -p ctox-core removed_free_subagent -- --nocapture
cargo test --manifest-path src/core/harness/Cargo.toml -p ctox-core harness_subagent_spawn_model_forbids_free_subagents -- --nocapture
cargo fmt --manifest-path src/core/harness/Cargo.toml --all --check
git diff --check
```

## 2026-08-10 I-074 Typed CV Recovery Error Flow

Minimal fork deltas preserve the three CV compact-recovery runtime classes
without changing existing error Display strings or unrelated retry behavior:

- `ctox-api/src/sse/responses.rs`: preserve `incomplete_details.reason` as a
  typed response-incomplete reason before constructing the API error; includes
  origin tests for token-limit and other incomplete reasons.
- `ctox-api/src/error.rs`: add `ResponseIncompleteReason` and the
  class-preserving `ApiError::ResponseIncomplete` variant.
- `core/src/api_bridge.rs`: project the typed API incomplete response into the
  matching core error variant.
- `core/src/error.rs`: add the matching `CodexErr::ResponseIncomplete` variant
  and project both stream classes to the existing
  `CodexErrorInfo::ResponseStreamDisconnected` code.
- `core/src/response_debug_context.rs`: add the compiler-required telemetry
  projection for `ApiError::ResponseIncomplete`, preserving the prior message.
- `core/src/api_bridge_tests.rs`: verify reason and Display preservation across
  the API bridge.
- `core/src/error_tests.rs`: verify stream and incomplete errors retain their
  Display text while using the typed protocol projection.

Ticket: I-074.
