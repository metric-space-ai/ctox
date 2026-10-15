#!/usr/bin/env bash
# P8-W is integrated: exercise the current kernel and real OrderedWriter.
set -euo pipefail
ctox-prep.sh
cargo build --locked --manifest-path tests/sync-v4-e2e-path/native/Cargo.toml -j2
node tests/sync-v4-e2e-path/source-hash.mjs
export E2E_OUT="$CARGO_TARGET_DIR/evidence/native-path-final"
bash tests/sync-v4-e2e-path/run.sh
E2E_NATIVE_PROOF=1 E2E_MUTATIONS_NATIVE_ONLY=1 node tests/sync-v4-e2e-path/mutation-proof.mjs
node tests/sync-v4-e2e-path/source-hash.mjs
