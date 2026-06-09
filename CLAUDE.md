# CTOX Development Guide (Instructions for Claude)

## MANDATORY: Read Before Answering Anything Architectural

Before answering **any** question about CTOX's architecture, scope, what code does, what is "old" / "new" / "dead" / "expected", or before proposing refactors — you **must** first read **every** `*.md` file in the repository root:

- `README.md` — public product description, installation, feature scope
- `HARNESS.md` — harness-specific notes
- `CLAUDE.md` — this file
- `docs/architecture.md` — architecture orientation

For anything touching the Business OS data plane (sync, replication, RxDB,
WebRTC, `src/core/rxdb/`, `src/apps/business-os/rxdb/`,
`src/core/business_os/rxdb_peer.rs`, `shared/sync.js`), additionally read
`docs/ctox-rxdb.md` — the canonical, code-verified description of CTOX DB and
its guardrails.

Do **not** rely on grep/memory/assumptions about what a given subsystem is supposed to be — the architecture docs in root are the source of truth. Specifically:

- `src/core/harness/` is the integrated in-process agent harness built around the hard-forked OpenAI Codex runtime (`ctox-core`). Executed **in-process** via `InProcessAppServerClient`, not as an external `codex-exec` subprocess.
- `src/core/inference/` holds per-model inference crates. Every curated model is intended to be **self-contained** — no code is shared across model crates. Each model should own its kernels, loader, and adapter code. CTOX reaches local inference through managed runtime control and IPC-backed local transports.
- The promoted local chat surface is deliberately one Qwen model at a time. At the moment, the root runtime registry promotes `Qwen/Qwen3.5-27B` for CUDA/NVIDIA. Metal Qwen crates in the tree are bring-up/transitional work until `model_registry` and `local_model` wire the same model to a verified backend.

Any `codex-exec` references and any remnants of the retired external `tools/model-runtime/` engine subtree in CTOX production code are leftover to be cleaned up, not the intended architecture. The current local runtime setting is still named `candle` in the TUI/config surface.

## Operator Guardrails (hard rules)

- **Local builds are allowed.** `cargo build`, `cargo test`, `cargo check`, `cargo clippy`, `cargo run`, and `cargo fmt` may be run on the operator machine for verification. Releases still go through the GitHub Actions pipeline (`.github/workflows/ci.yml`, `.github/workflows/release.yml`); trigger a release by pushing a tag `vX.Y.Z` on `main`. Inspect runs with `gh run list` / `gh run view`.
- **No unsolicited branches or worktrees.** Work directly on `main` in the origin checkout. Do not create `claude/*` branches or `.claude/worktrees/*` directories without an explicit request. The existing intended workflow is commit-to-main + push; branches are only for explicitly-requested PRs.
- **No global env-var controls for runtime state.** Runtime configuration belongs in typed `AppConfig` and CTOX's persisted SQLite runtime store via `runtime_env::env_or_config(root, ...)`. Do not add new process-environment toggles for production behavior. Tests that need host-state overrides must write to the test root's SQLite runtime config, not `std::env::set_var`.

## Inference-Engine Direction

These rules apply to new or actively refactored inference-capable model integrations (chat, embedding, STT, TTS, vision). Existing transitional ggml/llama/Candle-compatible paths are technical debt to isolate and retire, not patterns to copy into new model work.

1. **One crate per model.** Each curated model gets its own standalone Cargo crate at `src/core/inference/models/<model_id>/`. Examples: `qwen35_27b_q4km_dflash/`, `qwen3_embedding_0_6b/`, `voxtral_mini_4b_stt/`.

2. **Each model crate should be fully self-contained.** Avoid a shared model-runtime subtree or common helper crate between curated models unless there is an explicit architecture decision. Everything model-specific — Rust code, CUDA/Metal kernel source, FFI bindings, loader, graph, driver, adapter, server binary, vendor directory — should live inside that one crate. Duplication across crates is acceptable when it keeps model behavior isolated.

3. **Prefer bare-metal Rust + vendored CUDA/Metal kernels for new work.** Do not add a new dependency on `libggml-cuda.so`, `libggml.so`, `libllama.so`, Candle, mistralrs, vLLM, PyTorch, ONNX Runtime, or another pre-built inference framework without first documenting why the existing transitional paths cannot be retired. When practical, compile kernel source (vendored `.cu` / `.metal` / `.cuh` files) inside the model crate and drive kernels via CUDA Driver API / Metal API.

4. **Kernels are vendored 1:1 from upstream.** Take the kernel source files (`.cu`, `.cuh`) from the canonical upstream (llama.cpp / ggml-cuda is the current source of truth) and drop them into the crate's `vendor/` unmodified. Pin the upstream commit in `vendor/<source>.version`. Never hand-author CUDA kernels in CTOX — past attempts at self-authored kernels failed and were correctly deleted.

5. **Dispatcher is ported byte-for-byte into Rust.** The C++ dispatcher that picks which kernel variant to launch for a given op/shape/dtype (currently in upstream's `ggml-cuda.cu` and per-op `.cu` files) is translated line-for-line into Rust inside the model crate. Discipline identical to the Qwen3.5 graph port: `// ref: <upstream-file>:<line-range>` doc anchor on every ported function, variable names preserved, comments translated verbatim when they describe algorithm. This port lives inside the one model crate that needs it. Another model crate that needs the same dispatcher re-ports it from upstream into its own tree — no sharing.

6. **No process-env reads for model runtime state.** Weight paths, tokenizer paths, runtime toggles — everything CTOX-specific flows through the SQLite `runtime_env_kv` store via `runtime_env::env_or_config(root, …)`. `std::env::var` is only acceptable for OS-level things like `HOME` for path expansion, and even then the narrow pattern from `src/core/main.rs::home_dir` is preferred.

7. **Transport is Unix domain socket with line-delimited JSON, OpenAI Responses envelope.** No HTTP, no TCP (not even loopback), no TLS, no extra RPC frameworks. Peer UID check via `SO_PEERCRED` on Linux. Socket mode `0600`, parent dir `0700`. Wire-compatible with CTOX's existing client in `src/core/harness/core/src/client.rs::LocalIpcRequest`.

If a shortcut is tempting (for example "just add a shared helper crate" or "just read one env var"), treat it as an architecture decision, not a drive-by implementation detail. Existing compatibility bridges should be made explicit and contained.

## ctox-rxdb Data Plane (hard rules)

CTOX DB (`rxdb-rs` in the daemon, `ctox-rxdb-js` in the browser) is the
WebRTC-ONLY data plane for Business OS. These rules are enforced by guard
tests; every one of them encodes a real past regression caused by a
well-meaning agent. Full context: `docs/ctox-rxdb.md`.

1. **No HTTP fallback or bridge for Business OS records — ever.** Collections,
   `business_commands`, `ctox_queue_tasks`, desktop files/chunks, manifests and
   runtime status replicate only over RxDB/WebRTC. If sync is broken, fix it
   inside the WebRTC stack.
2. **Never patch `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` directly,
   and never edit its `src/` without rebuilding dist** (pinned esbuild command
   in `docs/ctox-rxdb.md`) **and bumping the `?v=` cache-busters**. src↔dist
   drift has shipped breakage in both directions.
3. **Never change wire-contract constants on one side.** The four
   `*generated*` contract files (two per side) are generated from
   `src/core/rxdb/tests/fixtures/*.json`; change the fixture and regenerate.
4. **No npm/bare imports in the browser runtime; no new process-env toggles
   anywhere in the data plane.**
5. **Keep the suites green and never delete/weaken a failing test:**
   `node src/apps/business-os/rxdb/tests/run-all.mjs` and
   `cargo test --manifest-path src/core/rxdb/Cargo.toml` (the crate is NOT a
   workspace member — root `cargo test` does not cover it).

## What CTOX Is (one-paragraph orientation)

CTOX is an AI agent system for autonomous server and DevOps work. It combines (1) an orchestration layer with mission queue, continuity tracking, governance and communication routing, (2) an in-process agent harness under `src/core/harness/` built around the hard-forked OpenAI Codex runtime (`ctox-core`), (3) an internal model gateway that keeps CTOX on an OpenAI Responses-shaped contract, and (4) curated local inference crates under `src/core/inference/models/<model>/` reached through CTOX-managed runtime control and IPC-backed local transports. The two explicit provider modes are `ctox_core_local` for managed local inference and `ctox_core_api` for remote/API-backed providers normalized back to Responses at the adapter edge. TUI uses ratatui + crossterm. Persistence: a single SQLite runtime store at `runtime/ctox.sqlite3`. Full architecture context is split across `README.md`, `HARNESS.md`, and `docs/architecture.md`.

## Repository Hygiene

The repository root is intentionally small. Keep source under `src/`, docs under `docs/`, tests under `tests/`, runtime state under root-level `runtime/`, and questionable or obsolete material under root-level `archive/`.

- `runtime/` is not source code. It is root-level ignored state for SQLite databases, caches, model artifacts, generated output, and build products such as Cargo target output.
- `archive/` is root-level ignored review storage. Move suspected legacy material there first instead of deleting it or hiding it inside `src/`.
- `src/` is source code only. Do not put runtime state, generated test data, caches, compiled artifacts, or archived material under `src/`.
- `install.sh` stays in the repository root because it is the public install entry point used by `curl .../main/install.sh | bash`.
- `src/scripts/` is only for source-side build helpers that are still actively used. Old benchmark, remote-host, model-recovery, qualification, and one-off maintenance scripts belong in `archive/`. The public installer is the root-level `install.sh`, not a `src/scripts` file.

## TUI Surface (orientation only)

- `src/core/ui/tui/mod.rs` — App state, event loop, key handling
- `src/core/ui/tui/render.rs` — ratatui rendering
- Three pages: `Chat`, `Skills`, `Settings` (enum at `mod.rs:365`)
- Settings sub-views: `Model`, `Communication`, `Secrets`, `Update`
- Layout: Header (7 lines) + Tabs (1 line) + Page content + Status bar

When the user asks for a TUI layout or rendering change, they are the ones who run the local snapshot/smoke tools if they want to preview it — you propose code edits, they verify on their machine or via CI.

## File Layout (key entry points)

| Path | Purpose |
|------|---------|
| `src/core/main.rs` | CLI entry point, mission loop |
| `src/core/ui/tui/` | Terminal UI |
| `src/core/context/lcm.rs` | Long-context memory engine |
| `src/core/context/compact.rs` | Compact policy (emergency + adaptive) |
| `src/core/execution/agent/direct_session.rs` | `PersistentSession` + in-process inference |
| `src/core/execution/agent/turn_loop.rs` | Turn planning, context rendering, continuity refresh |
| `src/core/execution/models/` | Model registry, adapters, runtime control, `runtime_env` gate |
| `src/core/execution/responses/` | Model gateway metadata and runtime control surface |
| `src/core/mission/` | Queue, tickets, plans, communication, review |
| `src/core/service/` | systemd service daemon |
| `src/core/harness/` | Agent harness (OpenAI-Codex fork, `ctox-core`) |
| `src/core/inference/models/<model>/` | Per-model self-contained inference crates (Rust + vendored kernels) |
| `src/apps/desktop/` | Desktop management app |
| `src/apps/business-os/` | Business OS app shell and modules |
| `src/tools/` | Supporting source packages |
| `install.sh` | Public installer entry point |
| `runtime/ctox.sqlite3` | Unified runtime store: settings, runtime state, queue, continuity, secrets |
