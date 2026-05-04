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
- CTOX shell calls from subagents must carry thread/agent/turn identifiers so
  forensics can attribute nested CLI activity.

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
