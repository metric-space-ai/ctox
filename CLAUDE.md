# CTOX Development Guide (Instructions for Claude)

## MANDATORY: Read Before Answering Anything Architectural

Before answering **any** question about CTOX's architecture, scope, what code does, what is "old" / "new" / "dead" / "expected", or before proposing refactors — you **must** first read **every** `*.md` file in the repository root:

- `README.md` — public product description, installation, feature scope
- `AGENTS.md` — internal agent architecture (harness flow, inference engine, model gateway, SQLite stores, compact policy, file layout)
- `HARNESS.md` — harness-specific notes
- `CLAUDE.md` — this file

Do **not** rely on grep/memory/assumptions about what a given subsystem is supposed to be — the architecture docs in root are the source of truth. Specifically:

- `src/harness/` is the integrated in-process agent harness built around the hard-forked OpenAI Codex runtime (`ctox-core`). Executed **in-process** via `InProcessAppServerClient`, not as an external `codex-exec` subprocess.
- `src/inference/` holds per-model inference crates. Every curated model (first one: `src/inference/models/qwen35_27b_q4km_dflash/`) is **self-contained** — no code is shared across model crates. Each vendors its own kernels and its own ggml/CUDA FFI in-tree. CTOX calls these crates directly; there is no separate model-serving subprocess.

Any `codex-exec` references and any remnants of the retired Candle-based `tools/model-runtime/` subtree in CTOX production code are leftover to be cleaned up, not the intended architecture.

## Operator Guardrails (hard rules)

- **Local builds are allowed.** `cargo build`, `cargo test`, `cargo check`, `cargo clippy`, `cargo run`, and `cargo fmt` may be run on the operator machine for verification. Releases still go through the GitHub Actions pipeline (`.github/workflows/ci.yml`, `.github/workflows/release.yml`); trigger a release by pushing a tag `vX.Y.Z` on `main`. Inspect runs with `gh run list` / `gh run view`.
- **No unsolicited branches or worktrees.** Work directly on `main` in the origin checkout. Do not create `claude/*` branches or `.claude/worktrees/*` directories without an explicit request. The existing intended workflow is commit-to-main + push; branches are only for explicitly-requested PRs.
- **No global env-var controls for runtime state.** Runtime configuration belongs in typed `AppConfig` and CTOX's persisted SQLite runtime store via `runtime_env::env_or_config(root, ...)`. Do not add new process-environment toggles for production behavior. Tests that need host-state overrides must write to the test root's SQLite runtime config, not `std::env::set_var`.

## What CTOX Is (one-paragraph orientation)

CTOX is an AI agent system for autonomous server and DevOps work. It combines (1) an orchestration layer with mission queue, continuity tracking, governance and communication routing, (2) an in-process agent harness under `src/harness/` built around the hard-forked OpenAI Codex runtime (`ctox-core`), (3) an internal model gateway that keeps CTOX on an OpenAI Responses-shaped contract, and (4) a set of curated per-model inference crates under `src/inference/models/<model>/`, each self-contained (Rust + vendored kernels per-model, no code sharing across models), called directly by CTOX for on-host inference. The two explicit provider modes are `ctox_core_local` for managed local inference and `ctox_core_api` for remote/API-backed providers normalized back to Responses at the adapter edge. TUI uses ratatui + crossterm. Persistence: a single SQLite runtime store at `runtime/ctox.sqlite3`. Rust toolchain: 1.93. Full architecture is in `AGENTS.md` — read it before making architectural claims.

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
| `src/harness/` | Agent harness (OpenAI-Codex fork, `ctox-core`) |
| `src/inference/models/<model>/` | Per-model self-contained inference crates (Rust + vendored kernels) |
| `runtime/ctox.sqlite3` | Unified runtime store: settings, runtime state, queue, continuity, secrets |
