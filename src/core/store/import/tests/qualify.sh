#!/usr/bin/env bash
set -euo pipefail
export RUST_TEST_THREADS=2 CMAKE_BUILD_PARALLEL_LEVEL=2 MAKEFLAGS=-j2
SOURCE=$(pwd)
PROOF=${IMPORT_PROOF_DIR:?set IMPORT_PROOF_DIR outside disposable TMPDIR}
mkdir -p "$PROOF"
# Integration's registry is still a placeholder. Compose only in isolated scratch,
# pinning the actual P3-06 source; never alter another strand's checkout files.
REGISTRY_SOURCE=535d6deb3da5ee87623cf143e142e5e30d666271
mkdir -p "$TMPDIR/import-registry"
git --git-dir=/mnt/nvme1/build-lane/mirror/metric-space-ai__ctox.git archive "$REGISTRY_SOURCE" src/core/store src/core/paths.rs tests/sync-v4-m2 | tar -x -C "$TMPDIR/import-registry"
STORE="$TMPDIR/import-registry/src/core/store"
printf '\n#[path="%s/src/core/store/import/mod.rs"] pub mod import;\n' "$SOURCE" >> "$STORE/lib.rs"
rustfmt --edition 2021 src/core/store/import/{mod,base_release,core_import,tests}.rs src/core/store/import/tests/{release_input,profile,legacy}.rs
rustfmt --edition 2021 --check src/core/store/import/{mod,base_release,core_import,tests}.rs src/core/store/import/tests/{release_input,profile,legacy}.rs
export IMPORT_INPUT_ROOT=${IMPORT_INPUT_ROOT:-/mnt/sata8t/lane/artifacts/v4-import-input}
export PATH=/mnt/nvme1/build-lane/cache/node-v24.13.1-linux-x64/bin:$PATH
node --check scripts/sync-v4/import-report.mjs
bash -n src/core/store/import/tests/qualify.sh
node --test src/core/store/import/tests/import-report.test.mjs 2>&1 | tee "$PROOF/input-contract.log"
node scripts/sync-v4/import-report.mjs --v4-root "$IMPORT_INPUT_ROOT/v4" --platform "$IMPORT_INPUT_ROOT/ctox-platform.manifest.json" --output "$PROOF/input-inventory.json"
cargo test --manifest-path "$STORE/Cargo.toml" --lib import::tests -- --nocapture 2>&1 | tee "$PROOF/component.log"
cargo check --manifest-path "$STORE/Cargo.toml" 2>&1 | tee "$PROOF/check.log"
python3 src/core/store/import/tests/mutations.py "$STORE"
cargo test --manifest-path "$STORE/Cargo.toml" --lib import::tests -- --nocapture 2>&1 | tee "$PROOF/restored.log"
sha256sum src/core/store/import/*.rs scripts/sync-v4/import-report.mjs > "$PROOF/source.sha256"
printf '%s\n' "$REGISTRY_SOURCE" > "$PROOF/registry-source.txt"
