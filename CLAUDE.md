# CTOX Development Guide

## MANDATORY: Read Before Answering Anything Architectural

Before answering **any** question about CTOX's architecture, scope, what code does, what is "old" / "new" / "dead" / "expected", or before proposing refactors — you **must** first read **every** `*.md` file in the repository root:

- `README.md` — public product description, installation, feature scope
- `AGENTS.md` — internal agent architecture (harness flow, inference engine, proxy, SQLite stores, compact policy, file layout)
- `HARNESS.md` — harness-specific notes
- `CLAUDE.md` — this file

Do **not** rely on grep/memory/assumptions about what a given subsystem is supposed to be — the architecture docs in root are the source of truth. Specifically: `tools/agent-runtime/` is a **hard-fork** of the OpenAI Codex runtime (now called `ctox-core`) and is executed **in-process** via `InProcessAppServerClient`, not as an external `codex-exec` subprocess. Any remaining `codex-exec` references in CTOX production code are leftover to be cleaned up, not the intended architecture.

## Operator Guardrails (hard rules)

- **No compilation on the operator machine.** Use the GitHub Actions CI/CD pipeline (`.github/workflows/ci.yml`, `release.yml`) for any build or test that actually invokes `cargo build`/`cargo test`/`cargo check`. Triggering a release: push a tag `vX.Y.Z` on `main`.
- **No unsolicited branches or worktrees.** Work directly on `main` in the origin checkout. Do not create `claude/*` branches or `.claude/worktrees/*` directories without an explicit request.
- **No global env-var controls for runtime state.** Runtime configuration belongs in typed `AppConfig` / `engine.env` / `runtime_env::env_or_config(root, ...)`. Do not add new process-environment toggles for production behavior. Tests that need host-state overrides must write to the test-root's `engine.env`, not `std::env::set_var`.

## Project Overview
CTOX is an AI agent system for autonomous work on hosts and services. It uses ratatui + crossterm for the TUI, Rust 1.93, and SQLite for persistence.

## Build
```bash
cargo build          # debug
cargo build --release
```

## Test
```bash
cargo test                          # all tests
cargo test tui_smoke -- --nocapture # TUI rendering tests only
cargo clippy -- -D warnings         # lint
cargo fmt --check                   # format check
```

## TUI Development

### Architecture
- `src/ui/tui/mod.rs` — App state, event loop, key handling (~6K lines)
- `src/ui/tui/render.rs` — All ratatui rendering (~3.5K lines)
- 3 pages: Chat, Skills, Settings
- Header (7 lines) + Tabs (1 line) + Page content + Status bar

### Headless Smoke Rendering
Render any page without a terminal:
```bash
./target/debug/ctox tui-smoke <page> [width] [height]
# Examples:
./target/debug/ctox tui-smoke chat 120 40
./target/debug/ctox tui-smoke settings 80 24
```

### Snapshot Testing
```bash
./scripts/tui_debug.sh save chat 120x40     # save baseline
./scripts/tui_debug.sh diff chat 120x40     # compare to baseline
./scripts/tui_debug.sh smoke settings 80x24 # quick render
```

### Test Environment Variables
- `CTOX_ROOT` — workspace root override
- `CTOX_TEST_ENGINE_HOST_ACCELERATION` — fake acceleration backend

`CTOX_TEST_GPU_TOTALS_MB` used to be a process-env override but is now removed from the allowlist per the no-env-var-controls guardrail. Tests must inject fake GPU totals by writing them to the test-root's `engine.env` so parallel tests stay isolated.

### Skills (Claude Code)
- `/tui-build` — Build and optionally run the TUI
- `/tui-debug` — Debug TUI rendering/layout/crash issues
- `/tui-test` — Run TUI tests and snapshot comparisons
