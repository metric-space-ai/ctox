# Tools Tree

The `tools/` tree carries auxiliary dev-time code and small CTOX-owned utility
crates. Per-model inference engines do NOT live here — see
`src/inference/models/<model>/` instead.

Rules:

- Code under `tools/` is not described as loose third-party dependencies.
- Each integrated subtree keeps its own provenance and license files.
- CTOX-specific patches inside these trees are part of the CTOX fork state, not floating dependency overrides.
- These trees remain source-owned in the main repository, without nested `.git` metadata or automatic upstream sync paths.

The previously-carried `tools/model-runtime/` Candle-based inference engine
has been retired in favor of the per-model direct-call architecture under
`src/inference/models/`.
