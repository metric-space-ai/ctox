# Direct Session + Compact Policy Refactor

Status: Phases 1–5 shipped, Phase 4b (mission-queue follow-up enqueue) and
full Phase 6 (subprocess deletion) deferred pending production mileage.

## Summary

CTOX's inference path historically spawned `codex-exec` as a subprocess per
slice and parsed its JSON event stream from stdout. This refactor replaces
that with an in-process `InProcessAppServerClient` call, giving CTOX
event-level hooks between codex-core model-API roundtrips. On top of that,
a two-axis compact policy decides when and how to compact the running
session context.

## Axes

### Trigger (when)

`CTOX_COMPACT_TRIGGER`

| Value       | Behavior                                                          |
|-------------|-------------------------------------------------------------------|
| `off`       | never compact (default)                                           |
| `adaptive`  | fire when observed `total_tokens / model_context_window` crosses `CTOX_COMPACT_ADAPTIVE_THRESHOLD` (default `0.70`) |
| `fixed`     | fire every `CTOX_COMPACT_FIXED_INTERVAL` completed turns          |

Adaptive rearms at each new `TurnStarted` event — one fire per turn max.

### Mode (how)

`CTOX_COMPACT_MODE`

| Value              | Behavior                                                         |
|--------------------|------------------------------------------------------------------|
| `mid-task`         | issue `ThreadCompactStart` on the live thread, continue the turn |
| `forced-followup`  | `ThreadUnsubscribe` the thread + write a signal file for the mission-loop to enqueue a fresh follow-up slice |

## Gate

`CTOX_USE_DIRECT_SESSION=true` routes `invoke_codex_exec_with_timeout_and_instructions_inner`
through the in-process path (`src/execution/agent/direct_session.rs`). Default
off — the subprocess path is still the active default and can be re-engaged
by clearing the variable.

## Files

- `src/execution/agent/direct_session.rs` — in-process session runner
  (Config build, InProcessAppServerClient::start, ThreadStart, TurnStart,
  event loop, final assistant message collection).
- `src/context/compact.rs` — `CompactTrigger`, `CompactMode`,
  `CompactPolicy::evaluate(EventMsg) -> CompactDecision`. 3 unit tests.
- `src/execution/agent/turn_loop.rs` — env-gate dispatch in
  `invoke_codex_exec_with_timeout_and_instructions_inner`. Subprocess path
  annotated DEPRECATED (Phase 6).
- `src/execution/models/runtime_env.rs` — allow-list extended for
  `CTOX_USE_DIRECT_SESSION`, `CTOX_COMPACT_*`, `CTOX_DEBUG_DIRECT_SESSION`.
- `src/ui/tui/mod.rs` — 5 new `SettingItem`s on the Model tab for the gate
  and compact knobs.

## Cargo

- New path-deps: `codex-core`, `codex-app-server-client`,
  `codex-app-server-protocol`, `codex-protocol`, `codex-arg0`,
  `codex-feedback`, `codex-cloud-requirements`, `codex-utils-absolute-path`,
  `tokio` (rt-multi-thread + macros).
- `[patch.crates-io]` mirrors `tools/agent-runtime/Cargo.toml` so forks of
  crossterm / ratatui / tokio-tungstenite / tungstenite resolve identically.
- `rusqlite` bumped 0.31 → 0.32 in root + `tools/doc-stack` so the whole
  graph agrees on `libsqlite3-sys 0.30.1` (aligns with `sqlx-sqlite 0.8.6`
  pulled in via `codex-state`).

## Verified

| Phase | Verification                                                              |
|-------|---------------------------------------------------------------------------|
| 1a    | Gate reachable, default-off subprocess path unchanged                     |
| 1b    | Service running, then `ctox chat --wait "say hi"` with gate on: request completes successfully |
| 2     | 3 unit tests in `compact::tests`; `compact-decision fixed=1` fires        |
| 3     | `thread/compact/start ok` after adaptive fire; turn continues             |
| 4     | `thread/unsubscribe ok` + signal file written, session returns partial    |
| 5     | 5 new rows render in TUI Model tab (`Direct Session`, `Compact Trigger`, `Compact Mode`, `Compact Fixed N`, `Compact Threshold`) |

## Deferred

- **Phase 4b**: `invoke_codex_exec_with_timeout_and_instructions` currently
  returns `Result<String>`. To let the mission-loop act on a forced-followup
  signal, the return type needs a richer outcome (e.g. an enum carrying
  `{ reply, followup_requested, reason }`) or the signal file must be
  consumed from the mission-loop tick. A sentinel file at
  `${CTOX_ROOT}/runtime/compact-followup-requested` is written for now.
- **Phase 6 completion**: actually delete `invoke_codex_exec_*` and the
  `CodexExecInvocation` / `CodexExecConfigSpec` plumbing once DirectSession
  covers every path in production (including `mission/review.rs` review
  parsing, tool-verification retry, and local-provider socket restart).
