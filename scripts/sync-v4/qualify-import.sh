#!/usr/bin/env bash
# Qualification inputs are copied, immutable release metadata and an allowlisted
# profile, never a live tenant. Run through gpu-build-run.sh.
set -euo pipefail
if (( $# < 3 || $# > 4 )); then
  echo "usage: $0 INPUT_ROOT PROOF_DIR M7_INPUT_ROOT [STORE_MANIFEST]" >&2
  exit 2
fi
export IMPORT_INPUT_ROOT=$(realpath "$1")
mkdir -p "$2"
export IMPORT_PROOF_DIR=$(realpath "$2")
export M7_INPUT_ROOT=$(realpath "$3")
manifest=${4:-src/core/store/Cargo.toml}
for file in core-allowlisted-v2.sqlite3 ctox-platform.manifest.json outbound-tracking.schema.json; do
  test -f "$IMPORT_INPUT_ROOT/$file"
done
# Fail closed if this store build does not expose the import module (a zero-
# selection cargo filter is not qualification).
cargo test --manifest-path "$manifest" --lib -- --list > "$IMPORT_PROOF_DIR/registered-tests.log"
for name in \
  import::tests::files::copied_profile_files_tracking_and_counting_are_complete_and_replay_is_seq_neutral \
  import::tests::profile::devops_profile_copy_imports_crew_and_replay_does_not_emit_changes \
  import::tests::release_input::actual_a9_base_is_activated_through_registry_and_replay_has_no_new_seq \
  import::tests::release_input::frozen_m7_libraries_remain_artifacts_outside_app_schema_publication; do
  grep -Fx "$name: test" "$IMPORT_PROOF_DIR/registered-tests.log" > /dev/null
done
# Record metadata custody; profile row payloads never go into this inventory.
sha256sum "$IMPORT_INPUT_ROOT/core-allowlisted-v2.sqlite3"   "$IMPORT_INPUT_ROOT/ctox-platform.manifest.json"   "$M7_INPUT_ROOT/v4/manifests/lib-ctox-office.manifest.json"   "$M7_INPUT_ROOT/v4/manifests/lib-monaco.manifest.json" > "$IMPORT_PROOF_DIR/inputs.sha256"
env -u IMPORT_INPUT_ROOT -u IMPORT_PROOF_DIR -u M7_INPUT_ROOT   cargo test --manifest-path "$manifest" 2>&1 | tee "$IMPORT_PROOF_DIR/default.log"
cargo test --manifest-path "$manifest" --lib import::tests -- --include-ignored --nocapture   2>&1 | tee "$IMPORT_PROOF_DIR/qualified.log"
node --test src/core/store/import/tests/import-report.test.mjs   2>&1 | tee "$IMPORT_PROOF_DIR/input-contract.log"
node scripts/sync-v4/import-report.mjs --v4-root "$M7_INPUT_ROOT/v4"   --platform "$M7_INPUT_ROOT/ctox-platform.manifest.json"   --output "$IMPORT_PROOF_DIR/m7-inventory.json"
