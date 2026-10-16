#!/usr/bin/env bash
# Explicit immutable train and deadline; no previous-order receipt reuse.
set -euo pipefail
export PATH=/home/metricspace/.nvm/versions/node/v22.23.1/bin:$PATH
export RUST_TEST_THREADS=2 PYTHONPYCACHEPREFIX="$CARGO_TARGET_DIR/p6-pycache"
ctox-prep.sh
train="${1:?immutable product train revision required}"
deadline="${2:?absolute Unix deadline required}"
python3 tests/sync-v4/p6-review-4/run.py --train "$train" --only all --expect-red --deadline "$deadline"
