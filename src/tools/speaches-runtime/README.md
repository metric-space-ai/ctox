## Speaches Runtime Artifact

This directory defines CTOX's pinned CPU speech runtime artifact.

- Install-time owner: `install.sh`
- Runtime owner: `src/core/execution/models/supervisor.rs`
- Artifact root: `src/tools/speaches-runtime/.venv`

CTOX does not resolve the Speaches backend dynamically in the productive runtime
path anymore. The installer prepares a local virtual environment from the locked
requirement set in `requirements.lock`, and the supervisor launches only the
local `uvicorn` and `speaches-cli` entrypoints from that artifact.
