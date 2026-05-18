# Clean-Room Baseline

The first clean-room `CTOX` version starts from two integrated hard forks and nothing more:

1. `tools/agent-runtime`
   The canonical `codex-cli` / `codex-rs` execution baseline.
2. `tools/model-runtime`
   The canonical local runtime and OpenAI-compatible serving fork for local models.

## Source Tree Status

Installations and local builds now expect the integrated source trees to already be present inside the CTOX project tree:

- `tools/agent-runtime`
- `tools/model-runtime`

Run:

```sh
ctox source-status
```

This command only validates the integrated fork layout. It does not clone, fetch, or update any upstream repository.

Historical source origins:

- `tools/agent-runtime` derives from `https://github.com/openai/codex.git`
- `tools/model-runtime` is carried canonically as a CTOX Candle fork; older naming and import lineage passed through `mistral.rs` / `engine.rs`

Current hard-fork policy:

- `tools/agent-runtime` currently derives from the Codex fork state pinned at `c6ab4ee537e5b118a20e9e0d3e0c0023cae2d982`
- `tools/model-runtime` carries the Candle-derived serving fork state and still preserves historical `engine.rs` import lineage where provenance requires it
- CTOX treats both trees as source-owned hard forks; local customizations belong to the CTOX fork state unless explicitly re-attributed
- the main install script builds only from the integrated trees bundled in the CTOX checkout

## Runtime Families

The first clean-room runtime bridge supports two local model families:

- `GPT-OSS`
  - baseline model example: `openai/gpt-oss-20b`
  - served by the local Candle-derived serving fork with the GPT-OSS startup profile
  - codex-cli stays on the `responses` API
  - a narrow proxy may rewrite tool schemas into the exact serving-fork shape

- `Qwen3.5`
  - baseline model examples: `Qwen/Qwen3.5-27B`
  - served by the local Candle-derived serving fork vision startup profile
  - follows the serving-fork Qwen3.5 path for image URL, local path, or base64 inputs
  - stays separate from the codex-cli baseline and is attached through custom execution

## Compatibility Tests

The first hard compatibility gate lives in:

```sh
cargo test execution_baseline
```

These Rust unit tests assert that codex-cli-style tool requests are rewritten into a serving-fork-compatible `responses` shape:

- function tools are nested under `function`
- unsupported tool types are removed
- known serving-fork breakers can be filtered
- structured `input` is flattened
- `parallel_tool_calls=false` and `max_tool_calls` are normalized away

This is not yet the full `CTOX` loop. It is the dependency and runtime baseline that the clean-room runtime must stand on before any higher wrapper logic is allowed back in.
