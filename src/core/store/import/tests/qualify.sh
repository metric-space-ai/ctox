#!/usr/bin/env bash
# Compatibility entry point for the integrated store; no pinned donor composition.
set -euo pipefail
exec bash scripts/sync-v4/qualify-import.sh "${IMPORT_INPUT_ROOT:?set copied release root}" "${IMPORT_PROOF_DIR:?set proof directory}" "${M7_INPUT_ROOT:?set copied frozen M7 release root}"
