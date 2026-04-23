# CTOX

CTOX brings autonomy to servers.

CTOX is an AI agent system for autonomous work on hosts and services. It is built for long-running build, operations, and infrastructure missions.

Install CTOX on a server and it acts as a persistent technical control layer around that host: planning work, continuing interrupted tasks, managing communication, supervising context, and driving execution until the mission is actually closed.


## What CTOX Is

CTOX combines four layers:

- a CTOX orchestration layer optimized for autonomous server and DevOps work
- Codex as the current execution engine
- a CTOX model gateway that keeps the internal model contract on OpenAI Responses
- a standalone local inference engine for on-host AI models


## The CTOX Orchestration Layer

CTOX adds system behavior that standalone execution CLIs do not provide:

- persistent service and control-plane behavior
- external communication routing across TUI, email, Jami, cron, and queue-backed work
- durable queue, plan, schedule, and follow-up execution
- long-run mission state with explicit blocker, next slice, and done gate
- long-context memory with continuity tracking, memory retrieval, long-run context optimization and self heal mechanisms
- mission watchdogs, timeout continuations, queue-pressure guards, and other background repair mechanisms
- completion review, verification runs, persistent claims, and mission assurance
- native integration for LLM, embeddings, STT, TTS
- a `WebSearch`, `WebRead`, `WebScrape`, and browser-automation stack
- built-in DevOps skills and tools
- built-in email and chat-app integration


## Execution Engine

CTOX currently uses Codex as its execution engine.

That means CTOX drives the bounded execution slice through the embedded `ctox-core` runtime, but wraps that slice in:

- persistent mission context
- durable routing and scheduling
- review and assurance
- model and backend control
- host-side operational policy

This is the intended split between orchestration and execution:

- CTOX owns persistence, orchestration, governance, communication, verification, and runtime control
- Codex owns execution semantics inside the bounded agent run

## Model Gateway

The CTOX model gateway serves local and adapter-mediated model workloads for LLM, embeddings, STT, and TTS. CTOX uses `responses` as its internal model contract; adapters translate to provider-native formats only at the outer edge. This is an internal gateway contract, not a public localhost inference proxy.

It provides:

- one internal Responses-shaped gateway surface
- adapter rewriting from `responses` into backend-specific upstream forms for GPT-OSS, Qwen, Nemotron, GLM, MiniMax, and the local engine bridges
- routing for generation, embeddings, transcription, speech, and other auxiliary runtime roles
- runtime telemetry and switch metadata for the CTOX control plane
- backend readiness checks, startup, and recovery behavior

The gateway gives CTOX and `ctox-core` one stable model contract while runtime control handles model-family differences and host-side behavior.

For execution, `ctox-core` runs in two explicit provider modes:

- `ctox_core_local` for managed local inference over private IPC
- `ctox_core_api` for remote/API providers whose edge adapters still normalize back to Responses

Both modes remain Responses-facing inside CTOX. `ctox_core_local` must execute over private IPC rather than machine-internal HTTP.

## Web Capability Model

CTOX uses four distinct web paths:

- `WebSearch` for current discovery and recent-information lookup
- `WebRead` for reading concrete sources well
- `interactive-browser` for real browser interaction when the page behavior itself matters
- `WebScrape` for durable, repeatable extraction

## Integrated Source Trees

CTOX carries integrated hard-fork and per-model source trees inside the project:

- `src/harness/` — integrated in-process **agent harness**, hard-forked from the OpenAI Codex runtime (`ctox-core`). CTOX drives its turn loop through this tree in-process via `InProcessAppServerClient`, not as an external subprocess.
- `src/inference/models/<model>/` — per-model, **self-contained** inference crates. Each curated model (first one: `qwen35_27b_q4km_dflash` — Qwen3.5-27B Q4_K_M target + z-lab DFlash draft) vendors its own Rust inference code plus its own kernels in-tree; no code is shared across model crates. CTOX calls these crates directly.

The previously-carried Candle-based `tools/model-runtime/` subtree has been retired in favor of the per-model direct-call architecture. Run `ctox source-status` to validate the source layout and provenance markers. CTOX's context system, orchestration, governance, routing, verification, and runtime mediation live in the main repository code.


## Installation

### One-liner (remote server)

```sh
curl -fsSL https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh | bash
```

### From a checked-out repository

```sh
./install.sh
```

### Installer options

The installer accepts the following flags:

| Flag | Default | Description |
| --- | --- | --- |
| `--backend=<cuda\|metal\|cpu>` | auto-detected | Compute backend (skip interactive selection) |
| `--model=<model>` | `google/gemma-4-E4B-it` | Default model for local inference |
| `--features=<features>` | auto | Override engine features (comma or space separated) |
| `--branch=<branch>` | `main` | Git branch to install from |
| `--repo=<url>` | `metric-space-ai/ctox` | Git repository URL |
| `--install-root=<path>` | `~/.local/lib/ctox` | Installation directory |
| `--state-root=<path>` | `~/.local/state/ctox` | State directory |
| `--cache-root=<path>` | `~/.cache/ctox` | Cache directory |
| `--bin-dir=<path>` | `~/.local/bin` | Binary symlink directory |
| `--rebuild` | | Rebuild in-place (used by `ctox update`) |

All flags can also be set as environment variables:

| Variable | Equivalent flag |
| --- | --- |
| `CTOX_BACKEND` | `--backend` |
| `CTOX_MODEL` | `--model` |
| `CTOX_INSTALL_ROOT` | `--install-root` |
| `CTOX_STATE_ROOT` | `--state-root` |
| `CTOX_CACHE_ROOT` | `--cache-root` |
| `CTOX_BIN_DIR` | `--bin-dir` |
| `CTOX_REPO` | `--repo` |
| `CTOX_BRANCH` | `--branch` |

### Example: install with CUDA backend and a specific model

```sh
curl -fsSL https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh \
  | bash -s -- --backend=cuda --model=openai/gpt-oss-20b
```


## Quick Start

```sh
ctox version          # verify installation
ctox start            # start the persistent loop
ctox status           # check service status
ctox                  # open the TUI
ctox stop             # stop the persistent loop
```

## macOS Build Note

This repository now ships a macOS-specific Cargo `rustc-wrapper` in
`.cargo/config.toml` that clears `com.apple.quarantine` from the active Cargo
registry/git caches and the current `target/` tree before invoking `rustc`.

That is necessary because macOS may otherwise reject generated proc-macro and
build dylibs with `library load disallowed by system policy` during local Rust
builds.

If an older build tree is already poisoned, you can still force a manual reset:

```sh
xattr -dr com.apple.quarantine target ~/.cargo/registry ~/.cargo/git
```

## Updates

Upgrade existing installations through the managed release layout:

```sh
ctox update channel set-github --repo metric-space-ai/ctox
ctox update apply --latest
ctox update status
```

Or adopt an existing checkout into the managed layout:

```sh
ctox update adopt --install-root ~/.local/lib/ctox --state-root ~/.local/state/ctox
ctox update apply --source /path/to/new/ctox-checkout
```


## Supported Local 128k Models

<!-- BEGIN GENERATED 128K README SUMMARY -->

These are the current minimum `128k` entry points for local CTOX models. Multi GPU minima are only shown when the model also has a working power-of-two NCCL performance path. Above these minima, CTOX uses all available VRAM on the target host to optimize the selected preset.

| Model | Single GPU Minimum | Multi GPU Minimum |
| --- | --- | --- |
| Qwen/Qwen3.5-4B | 1x21.3 GB | 2x16.6 GB |
| openai/gpt-oss-20b | 1x37.1 GB | 2x20.5 GB |

<!-- END GENERATED 128K README SUMMARY -->


## License

[Apache License 2.0](LICENSE)

See [NOTICE](NOTICE) for attribution of integrated source trees.
