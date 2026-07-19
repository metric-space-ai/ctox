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

Attribution rule:

- When a file under this subtree differs from the imported snapshot, describe it as a CTOX fork delta, not as an ambiguous upstream version.

## 2026-07 Persistent CTOX Runtime Context

CTOX uses the existing turn-context and rollout machinery for a durable normal
worker thread. The app-server `turn/start` request accepts an optional
`developer_instructions` override, persists it in `TurnContextItem`, and emits a
developer update only when it changes. CTOX runtime context is wrapped in the
reserved `<ctox_runtime_context ...>` marker. Request normalization retains only
the newest marked section while preserving all non-CTOX history. This is a
model-request projection rule; the rollout remains an append-only audit source.

This delta deliberately does not add a second scheduler, memory store, or
CTOX-specific response-item type. Resume and compaction continue to use the
fork's existing thread, rollout, `TurnContextItem`, and `ContextManager`
contracts.

Systematic-research attempts are a deliberate service-owned exception to normal
worker continuity. Each attempt uses a fresh non-persistent standard worker
session so the built-in typed CTOX Web tools remain available. Completion
validation reads the durable rollout and requires `ctox_deep_research` to be the
first external action. This prevents stale research context or an untracked
preliminary tool call from contaminating a new evidence run.

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
