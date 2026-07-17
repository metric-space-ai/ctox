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

## Backport Notes

### 2026-04 Subagents Backport

This backport imports the useful shape of Codex subagents without letting them
become independent CTOX runtime owners.

Runtime invariants:

- Subagents are parallel work leaves. The parent agent owns review, rework,
  completion, and owner-visible claims.
- Subagents do not receive the full parent context by default. The parent must
  pass a task-specific prompt; subagents use the vanilla subagent profile plus
  local tools/skills to discover additional context.
- Thread-spawn subagents cannot recursively use collaboration-mode escalation
  or spawn further subagents.
- Local model providers run subagent work serially. API-backed providers may run
  parallel subagent work.
- The review state machine must see one parent result, not one independent
  review gate per subagent.
- Agent-job leaves expose workspace tools plus `report_agent_job_result`; they
  do not expose recursive spawn, channels, meetings, acknowledgement, or
  control-plane mutations. “Report-only” is not the fork contract.
- Session metadata records the typed capability profile. Explicit
  `dynamic_tools: []` is an authoritative no-tools contract and must not be
  repopulated from persisted thread state.
- Reviewer sessions are tagged `SubAgentSource::Review`. Their authoritative
  workspace/runtime stays read-only while a disposable scratch CWD can host
  copied inputs for write-producing checks; the reviewer tag keeps mutating
  tools unavailable even though that scratch CWD is writable.
- CTOX shell calls from subagents must carry thread/agent/turn identifiers so
  forensics can attribute nested CLI activity.
- CTOX-managed Linux direct sessions select the stable Landlock backend before
  thread creation. Root workers, thread-spawn leaves, and reviewers therefore
  inherit one host-independent sandbox contract even when a managed container
  has no system bubblewrap package or usable user namespace.

State and forensic fields:

- `threads.subagent_parent_thread_id`
- `threads.subagent_depth`
- `threads.agent_path`
- existing `threads.agent_nickname`
- existing `threads.agent_role`

When pulling future Codex changes:

1. Re-check `core/src/tools/handlers/multi_agents*` for protocol or lifecycle
   changes.
2. Re-check `core/src/agent/control*` for scheduling semantics, especially
   local-provider serialization.
3. Re-check `core/src/tools/spec.rs` for subagent prompt text and exposed
   model/reasoning options.
4. Re-check app-server thread summaries and state extraction before changing
   source metadata.
5. Preserve customized CTOX skill prompts; do not wholesale replace local
   skills with upstream Codex skills.

Verification commands used for this slice:

```bash
cargo check --manifest-path src/harness/Cargo.toml -p ctox-state --tests
cargo test --manifest-path src/harness/Cargo.toml -p ctox-state --lib --quiet
cargo check --manifest-path src/harness/Cargo.toml -p ctox-core --tests
cargo test --manifest-path src/harness/Cargo.toml -p ctox-core exec_env -- --nocapture
cargo test --manifest-path src/harness/Cargo.toml -p ctox-core multi_agents::tests -- --nocapture
cargo test --manifest-path src/harness/Cargo.toml -p ctox-core subagent -- --nocapture
cargo check --manifest-path src/harness/Cargo.toml -p ctox-app-server --tests
cargo test --manifest-path src/harness/Cargo.toml -p ctox-app-server --lib --quiet
cargo check --manifest-path src/harness/Cargo.toml -p ctox-app-server-client --lib
cargo fmt --manifest-path src/harness/Cargo.toml --all --check
git diff --check
```
