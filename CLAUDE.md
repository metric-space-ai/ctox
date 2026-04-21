# CTOX Development Guide (Instructions for Claude)

## MANDATORY: Read Before Answering Anything Architectural

Before answering **any** question about CTOX's architecture, scope, what code does, what is "old" / "new" / "dead" / "expected", or before proposing refactors — you **must** first read **every** `*.md` file in the repository root:

- `README.md` — public product description, installation, feature scope
- `AGENTS.md` — internal agent architecture (harness flow, inference engine, model gateway, SQLite stores, compact policy, file layout)
- `HARNESS.md` — harness-specific notes
- `CLAUDE.md` — this file

Do **not** rely on grep/memory/assumptions about what a given subsystem is supposed to be — the architecture docs in root are the source of truth. Specifically: `src/inference/` is the integrated in-process inference workspace built around the hard-forked OpenAI Codex runtime (`ctox-core`). It is executed **in-process** via `InProcessAppServerClient`, not as an external `codex-exec` subprocess. Any remaining `codex-exec` references in CTOX production code are leftover to be cleaned up, not the intended architecture.

## Operator Guardrails (hard rules)

- **Local builds and tests are allowed on this machine.** `cargo build`, `cargo test`, `cargo check`, `cargo clippy`, `cargo run`, and `cargo fmt` may be run directly on the operator machine for verification, including on the `tools/model-runtime/` workspace (Metal on macOS, ask the remote GPU host for CUDA verification). Releases still go through the GitHub Actions pipeline (`.github/workflows/ci.yml`, `.github/workflows/release.yml`); trigger a release by pushing a tag `vX.Y.Z` on `main`. Inspect runs with `gh run list` / `gh run view`.
- **Work directly on main. Branches and worktrees only when explicitly asked.** The existing intended workflow is commit-to-main + push; branches and `.claude/worktrees/*` directories are reserved for explicitly-requested PRs — don't spin one up on your own judgement.
- **No global env-var controls for runtime state.** Runtime configuration belongs in typed `AppConfig` and CTOX's persisted SQLite runtime store via `runtime_env::env_or_config(root, ...)`. Do not add new process-environment toggles for production behavior. Tests that need host-state overrides must write to the test root's SQLite runtime config, not `std::env::set_var`.

## What CTOX Is (one-paragraph orientation)

CTOX is an AI agent system for autonomous server and DevOps work. It combines (1) an orchestration layer with mission queue, continuity tracking, governance and communication routing, (2) an in-process inference workspace under `src/inference/` built around the hard-forked OpenAI Codex runtime (`ctox-core`), (3) an internal model gateway that keeps CTOX on an OpenAI Responses-shaped contract, and (4) an optional internal on-host model-serving runtime (`tools/model-runtime/`). The two explicit provider modes are `ctox_core_local` for managed local inference over private IPC and `ctox_core_api` for remote/API-backed providers normalized back to Responses at the adapter edge. TUI uses ratatui + crossterm. Persistence: a single SQLite runtime store at `runtime/ctox.sqlite3`. Rust toolchain: 1.93. Full architecture is in `AGENTS.md` — read it before making architectural claims.

## TUI Surface (orientation only)

- `src/ui/tui/mod.rs` — App state, event loop, key handling (~7.1K lines)
- `src/ui/tui/render.rs` — All ratatui rendering (~3.8K lines)
- Three pages: `Chat`, `Skills`, `Settings` (enum at `mod.rs:365`)
- Settings sub-views: `Model`, `Communication`, `Secrets`, `Update`
- Layout: Header (7 lines) + Tabs (1 line) + Page content + Status bar

When the user asks for a TUI layout or rendering change, they are the ones who run the local snapshot/smoke tools if they want to preview it — you propose code edits, they verify on their machine or via CI.

## File Layout (key entry points)

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point, mission loop |
| `src/ui/tui/` | Terminal UI |
| `src/context/lcm.rs` | Long-context memory engine |
| `src/context/compact.rs` | Compact policy (emergency + adaptive) |
| `src/execution/agent/direct_session.rs` | `PersistentSession` + in-process inference |
| `src/execution/agent/turn_loop.rs` | Turn planning, context rendering, continuity refresh |
| `src/execution/models/` | Model registry, adapters, runtime control, `runtime_env` gate |
| `src/execution/responses/` | Model gateway metadata and runtime control surface |
| `src/mission/` | Queue, tickets, plans, communication, review |
| `src/service/` | systemd service daemon |
| `src/inference/` | Integrated in-process inference workspace around `ctox-core` |
| `tools/model-runtime/` | Local model serving engine — integrated source tree |
| `runtime/ctox.sqlite3` | Unified runtime store: settings, runtime state, queue, continuity, secrets |
