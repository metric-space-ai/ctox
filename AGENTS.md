# CTOX Coding Agent Instructions

This file is the canonical repository guidance for coding agents. Claude Code
compatibility is handled by `CLAUDE.md`, which imports this file with
`@AGENTS.md`; do not duplicate root instructions there.

## First Rule

Read before changing:

- For repository-wide or architectural work: `README.md`, `HARNESS.md`, and
  `docs/architecture.md`.
- For Business OS, RxDB, WebRTC sync, browser data, module records, commands,
  files, runtime status, or anything under `src/core/rxdb/`,
  `src/apps/business-os/rxdb/`, `src/core/business_os/`, or
  `src/apps/business-os/shared/sync.js`: read `docs/ctox-rxdb.md` and the
  nearest directory `AGENTS.md`.
- For the forked execution harness: read `HARNESS.md` and
  `src/core/harness/FORK.md`.
- For local inference/model-runtime work: read `docs/architecture.md`, the
  relevant model crate docs, and any porting notes next to that crate before
  changing kernels, dispatchers, loaders, or runtime wiring.

If docs and code disagree, inspect the code and update the doc as part of the
same change when the behavior is intentionally different.

## Operating Guardrails

- Work on `main` in the origin checkout unless the user explicitly asks for a
  branch, PR, or worktree.
- Local verification is allowed: `cargo build`, `cargo check`, `cargo test`,
  `cargo clippy`, `cargo fmt`, node smoke tests, and app-specific checks may be
  run on the operator machine.
- The worktree may already contain user changes. Do not revert or tidy files
  outside your task. If an existing change touches your area, understand it and
  work with it.
- Do not add new process-environment toggles for production/runtime behavior.
  Runtime configuration belongs in typed config, the SQLite runtime store, or
  the CTOX secret store via the existing `runtime_env`/secrets paths. Tests may
  use isolated test-root configuration.
- Do not weaken, delete, or bypass guard tests. A red guard is a finding; fix
  the change.

## Current System Map

CTOX is the durable work daemon. It owns queueing, tickets, schedules, plans,
communication, context/continuity, governance, review, verification, process
evidence, runtime state, and continuation across bounded agent slices.

The forked Codex runtime under `src/core/harness/` is an in-process execution
component, not the owner of CTOX persistence or completion semantics. CTOX
prepares context and policy, runs bounded turns through the harness, then uses
durable review, validation, and outcome evidence to decide whether work closes,
requeues, waits, or blocks.

The model gateway under `src/core/execution/` keeps CTOX on an internal
Responses-shaped contract while adapters and runtime control handle OpenAI,
Anthropic, OpenRouter, MiniMax, Azure Foundry, local inference, embeddings,
STT, TTS, and vision support.

Local inference code lives under `src/core/inference/models/<model>/`.
Curated model integrations should be self-contained model crates. Managed local
IPC uses `LocalTransport`: Unix sockets on Unix, named pipes on Windows, and
TCP loopback only as a legacy fallback, not as a new managed-inference pattern.

Business OS is the browser/app surface for CTOX plus a separate MCP channel for
external agents. Browser business data is not an HTTP API surface; it syncs
through CTOX DB over WebRTC.

Business OS app/module work now includes shell-delivered database handles,
runtime-installed modules, app lifecycle/release metadata, role/permission
policy, founder/module assignment, data-access review, command dispatch, MCP
delegation, and projections from CTOX core into the RxDB store. Keep policy and
projection decisions server-authoritative; browser helpers can mirror UX state
but must not become the source of truth for permissions or persistence.

## Business OS Data Boundary

Business OS collections, module runtime data, `business_commands`,
`ctox_queue_tasks`, desktop files/chunks, module manifests, and native runtime
status must never be proxied through HTTP between the browser and CTOX.

HTTP may serve static shell assets, bootstrap configuration, status, auth, and
explicit control-plane endpoints. It must not become a data bridge or fallback.
If sync is broken, fix the WebRTC/RxDB path.

For CTOX DB:

- Browser runtime: `src/apps/business-os/rxdb/` (`ctox-rxdb-js`, browser ESM,
  no package manager).
- Native runtime: `src/core/rxdb/` plus `src/core/business_os/rxdb_peer.rs`.
- Canonical reference: `docs/ctox-rxdb.md`.
- Generated wire contracts come from `src/core/rxdb/tests/fixtures/*.json`.
  Change fixtures, regenerate both sides, and rebuild consumers; never edit one
  generated side by hand.
- Do not patch `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` directly. Edit
  `src/`, rebuild with the pinned esbuild command in `docs/ctox-rxdb.md`, and
  bump all three identical `?v=` cache-busters.

For Business OS policy and modules:

- `src/core/business_os/policy.rs` is the native permission model. Keep roles,
  scopes, decisions, and audit reasons in sync with browser helpers such as
  `src/apps/business-os/shared/permissions.js`.
- App/module install, release, rollback, ownership, source-view, data-access,
  and CTOX task actions must go through explicit permission checks. Do not add
  a UI-only gate for an action that mutates server state.
- Business OS MCP is a typed external-agent channel into the local store and
  policy gate. It is not the Browser Business OS data path and must not bypass
  RxDB/WebRTC replication or server-side policy.

## Inference Model Rules

These rules apply to new or actively refactored inference-capable model
integrations under `src/core/inference/models/<model>/`.

- Keep curated model crates self-contained. Model-specific kernels, loader,
  graph, adapter, server binary, build glue, and vendor sources belong inside
  the model crate unless there is an explicit architecture decision.
- Prefer vendored upstream CUDA/Metal kernels and Rust host dispatchers for
  curated local model work. Do not add a new dependency on ggml, llama.cpp,
  Candle, PyTorch, ONNX Runtime, vLLM, or similar frameworks as a drive-by
  shortcut.
- When porting upstream dispatchers, preserve upstream structure closely:
  `// ref: <upstream-file>:<line-range>` anchors, original variable names where
  practical, and comments where they explain algorithmic behavior.
- Do not hand-author CUDA kernels for production paths. Custom Metal candidates
  are allowed only when isolated from the vendored correctness baseline and
  paired with verifier/bench evidence before promotion.
- Runtime state such as model choice, weight paths, tokenizer paths, backend
  toggles, and runtime readiness must flow through the existing runtime
  state/config/secret stores, not new ambient env-var toggles.
- Managed local inference transports should use platform IPC (`UnixSocket` or
  Windows `NamedPipe`). Treat TCP loopback as legacy fallback unless a specific
  architecture decision says otherwise.

## Repository Placement

- `src/core/`: daemon, CLI, TUI, mission systems, service loops, harness
  integration, model gateway, local inference source, Business OS native side.
- `src/apps/`: app surfaces, including Business OS and Desktop.
- `src/tools/`: supporting source packages.
- `src/scripts/`: active source-side build helpers only.
- `docs/`: technical documentation, RFCs, legal notices, and GitHub Pages site.
- `tests/`: integration, harness, fixture, and behavior tests.
- `runtime/`: ignored local runtime state, databases, model/cache/build output,
  and generated state.
- `archive/`: ignored review area for obsolete or uncertain material before
  deletion or reintegration.

`src/` is source code only. Do not put runtime state, caches, generated output,
compiled artifacts, or archival material there. `install.sh` stays at the root
as the public installer entry point.

## Subsystem Rules

- Harness changes must preserve durable-state-first behavior: prompts can
  describe work, but completion, review, retries, subagent activity, spawn
  edges, and outcomes must be explainable from persisted evidence.
- Subagents in the CTOX Codex fork are leaf workers. The parent owns
  user-visible completion, review, rework, and claims.
- Do not auto-clone, auto-fetch, or auto-update `src/core/harness/` from
  upstream Codex. Treat it as a hard fork and document fork deltas.
- Business OS native peer lifecycle invariants in `rxdb_peer.rs` are
  load-bearing: supervised respawn owns peer start state, bring-up failure is
  fatal for the run, heartbeats distinguish process liveness from
  `replicationUp`, and signaling URLs are re-derived per reconnect.
- Browser Business OS module/app work must respect the shell/runtime split:
  apps receive database handles from the shell; they must not import upstream
  `rxdb` or invent their own sync path.
- Coding-agent provider integration under `src/core/coding_agents/` is a
  Business OS control surface for external tools such as Codex, Claude Code,
  Antigravity, and the mock provider. Keep provider command execution bounded,
  persisted, and reflected through Business OS policy/command status rather
  than ad hoc background processes.

## Validation

Choose the narrowest useful checks for the files changed:

- General Rust: `cargo fmt --check`, `cargo check`, targeted `cargo test`.
- Release/liveness-sensitive work: `cargo run -- process-mining spawn-liveness`
  or the specific process-mining/harness command guarding the change.
- CTOX DB / Business OS data plane:
  `node src/apps/business-os/rxdb/tests/run-all.mjs`,
  `cargo test --manifest-path src/core/rxdb/Cargo.toml`,
  `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- Browser RxDB runtime `src/` changes: rebuild `dist/` and bump the three
  cache-busters before running the JS suite.
- Business OS native/server changes that affect projections, policy, commands,
  or status should also run a relevant `cargo check` or targeted Rust test.

If you cannot run the right checks, say exactly what was not run and why.
