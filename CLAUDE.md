# CTOX Development Guide

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
- `CTOX_TEST_GPU_TOTALS_MB` — fake GPU spec (e.g., `0:8192;1:8192`)
- `CTOX_TEST_ENGINE_HOST_ACCELERATION` — fake acceleration backend

### Skills (Claude Code)
- `/tui-build` — Build and optionally run the TUI
- `/tui-debug` — Debug TUI rendering/layout/crash issues
- `/tui-test` — Run TUI tests and snapshot comparisons
