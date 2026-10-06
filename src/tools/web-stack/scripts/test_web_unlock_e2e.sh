#!/usr/bin/env bash
#
# End-to-end integration test for the web-unlock skill loop.
#
# Stage 1 (default, ~30s): exercises the full registry / repair / signal
# lifecycle against a synthetically failing probe — no network or stealth
# changes required. Validates that:
#   * add-vector registers a new vector
#   * baseline --auto-repair detects regression and opens a pending repair
#   * repair complete --succeeded flips the vector status back to "working"
#   * signals record / resolve --repair links work and persist correctly
#   * cleanup removes test rows so the registry returns to its prior state
#
# Stage 2 (--stage2 or --full): copies the committed root and its resolved
# immutable Workjet dependency into a task-owned TMPDIR sandbox. It builds the
# daemon with that canonical source, requires an initial passing incolumitas
# probe, short-circuits the copied stealth IIFE and requires actual failed
# tests, then restores, rebuilds and requires a positive control. Shared Cargo
# caches, operator source, lock and runtime DB are never mutated. Network and
# isolated patchright + chromium preparation are required; admission remains
# the caller responsibility. Set TMPDIR on /Volumes/tmp on the operator Mac.
#
# Usage:
#   src/tools/web-stack/scripts/test_web_unlock_e2e.sh --stage1
#   src/tools/web-stack/scripts/test_web_unlock_e2e.sh --full
#   src/tools/web-stack/scripts/test_web_unlock_e2e.sh --stage2
#
# Exit codes: 0 = all passed, non-zero = a stage failed (see logs).

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../../.." && pwd)"
SQLITE_DB="$ROOT/runtime/ctox.sqlite3"


STAGE1=1
STAGE2=0
case "${1:-}" in
  --full)    STAGE1=1; STAGE2=1 ;;
  --stage1)  STAGE1=1; STAGE2=0 ;;
  --stage2)  STAGE1=0; STAGE2=1 ;;
  -h|--help)
    sed -n '2,20p' "$0" | sed 's/^# //;s/^#//'
    exit 0
    ;;
  "") ;;
  *) echo "Unknown arg: $1"; exit 2 ;;
esac

log()  { printf "\n\033[36m[%s]\033[0m %s\n" "$(date +%H:%M:%S)" "$*"; }
ok()   { printf "\033[32m  ok\033[0m %s\n" "$*"; }
die()  { printf "\033[31m  FAIL\033[0m %s\n" "$*" >&2; exit 1; }
assert_eq() {
  local actual="$1" expected="$2" label="$3"
  if [[ "$actual" != "$expected" ]]; then
    die "$label: expected '$expected', got '$actual'"
  fi
  ok "$label = $actual"
}

# ── Preconditions ───────────────────────────────────────────────────────────
# Stage 2 owns an isolated source/runtime copy and needs no operator DB or
# pre-existing binary. Only Stage 1 uses the source-verified daemon receipt.
if [[ $STAGE1 -eq 1 ]]; then
  CTOX="$(python3 "$ROOT/scripts/web_stack_source.py" daemon-path)"
  [[ -x "$CTOX" ]] || die "source-verified daemon is missing"
  command -v jq >/dev/null 2>&1 || die "jq is required"
  command -v sqlite3 >/dev/null 2>&1 || die "sqlite3 is required"
  cd "$ROOT"
  if [[ ! -f "$SQLITE_DB" ]]; then
    log "Seeding runtime DB via list-probes"
    "$CTOX" web unlock list-probes >/dev/null 2>&1 || die "failed to seed runtime DB"
  fi
fi

# ── Stage 1: synthetic regression through full lifecycle ───────────────────

run_stage1() {
  log "STAGE 1 — synthetic regression lifecycle"

  local stamp; stamp=$(date +%s%N)
  local PROBE_ID="e2e_failing_${stamp}"
  local VECTOR_ID="e2e_vector_${stamp}"
  local SCRIPT_DIR="$ROOT/runtime/web_unlock_e2e"
  local SCRIPT_PATH="$SCRIPT_DIR/probe_failing_${stamp}.js"
  mkdir -p "$SCRIPT_DIR"

  # Always-failing probe fixture — produces the shape the 'sannysoft' parser reads.
  # Stage 1 verifies the registry / repair lifecycle and should not depend on
  # browser runtime readiness; Stage 2 exercises the real browser path.
  cat > "$SCRIPT_PATH" <<EOF
{
  "site": "e2e-synthetic",
  "url": "data:,",
  "title": "e2e",
  "totals": { "headless": 0, "fingerprint": 0, "failed": 2 },
  "failed": [
    { "name": "synthetic.test.alpha", "cls": "result failed" },
    { "name": "synthetic.test.beta",  "cls": "result failed" }
  ],
  "results": { "headless": [], "fingerprint": [] }
}
EOF

  # Register the probe directly via SQL — we don't yet have a `unlock add-probe`
  # CLI; SQL keeps the test self-contained.
  sqlite3 "$SQLITE_DB" "
    INSERT INTO web_unlock_probes
      (probe_id, site_name, probe_url, script_path, parser_kind,
       expected_baseline_json, timeout_ms, enabled, created_at, updated_at)
    VALUES
      ('$PROBE_ID', 'e2e-synthetic', 'data:,',
       'runtime/web_unlock_e2e/probe_failing_${stamp}.js', 'sannysoft_fixture',
       '{\"failed_max\":0}', 30000, 1, datetime('now'), datetime('now'));
  "

  # Register a vector under that probe matching the first failed test name —
  # so baseline --auto-repair has something to map to.
  "$CTOX" web unlock add-vector \
    --id "$VECTOR_ID" \
    --probe "$PROBE_ID" \
    --test "synthetic.test.alpha" \
    --desc "E2E test vector" \
    --fix "n/a — synthetic" \
    --patch-files "n/a" >/dev/null

  # The vector starts in 'untested' state (add-vector default). Move it to
  # 'working' so the broken-after-failure transition is observable.
  "$CTOX" web unlock set-vector-status --id "$VECTOR_ID" --status working >/dev/null

  log "Running baseline --auto-repair (expect non-zero exit, opened repair)"
  set +e
  local OUTPUT
  OUTPUT=$("$CTOX" web unlock baseline "$PROBE_ID" --record --auto-repair 2>&1)
  local EXIT_CODE=$?
  set -e

  if [[ $EXIT_CODE -eq 0 ]]; then
    echo "$OUTPUT"
    die "baseline returned exit 0 but should have reported a regression"
  fi
  ok "baseline exited with code $EXIT_CODE as expected"

  local FAILED_COUNT
  FAILED_COUNT=$(echo "$OUTPUT" | jq -r '.probes[0].failed_count')
  assert_eq "$FAILED_COUNT" "2" "failed_count in baseline output"

  local REPAIR_ID
  REPAIR_ID=$(echo "$OUTPUT" | jq -r '.probes[0].opened_repairs[0] // empty')
  if [[ -z "$REPAIR_ID" || "$REPAIR_ID" == "null" ]]; then
    echo "$OUTPUT"
    die "auto-repair did not open any pending repair"
  fi
  ok "auto-opened repair_id = $REPAIR_ID"

  # Vector should now be 'broken'
  local STATUS
  STATUS=$(sqlite3 "$SQLITE_DB" "SELECT status FROM web_unlock_vectors WHERE vector_id='$VECTOR_ID';")
  assert_eq "$STATUS" "broken" "vector status after auto-repair"

  # Run history should have a row for this probe with passed_baseline = 0
  local RUN_FAILED
  RUN_FAILED=$(sqlite3 "$SQLITE_DB" "
    SELECT failed_count FROM web_unlock_test_runs
    WHERE probe_id='$PROBE_ID' ORDER BY run_id DESC LIMIT 1;
  ")
  assert_eq "$RUN_FAILED" "2" "persisted run.failed_count"

  log "Completing repair --succeeded"
  "$CTOX" web unlock repair complete \
    --id "$REPAIR_ID" --succeeded --commit "e2e-test-${stamp}" \
    --notes "synthetic regression closed" >/dev/null

  STATUS=$(sqlite3 "$SQLITE_DB" "SELECT status FROM web_unlock_vectors WHERE vector_id='$VECTOR_ID';")
  assert_eq "$STATUS" "working" "vector status after repair complete"

  local COMMIT_PERSISTED
  COMMIT_PERSISTED=$(sqlite3 "$SQLITE_DB" "SELECT resulting_commit FROM web_unlock_repairs WHERE repair_id=$REPAIR_ID;")
  assert_eq "$COMMIT_PERSISTED" "e2e-test-${stamp}" "resulting_commit persisted"

  log "Recording and resolving a manual signal"
  local SIGNAL_OUT
  SIGNAL_OUT=$("$CTOX" web unlock signals record \
    --source "e2e_test_$stamp" \
    --url "https://e2e.test/sorry/" \
    --evidence '{"reason":"synthetic","stamp":"'"$stamp"'"}')
  local SIGNAL_ID
  SIGNAL_ID=$(echo "$SIGNAL_OUT" | jq -r '.signal_id')
  [[ -n "$SIGNAL_ID" && "$SIGNAL_ID" != "null" ]] || die "signals record returned no signal_id"
  ok "recorded signal_id = $SIGNAL_ID"

  "$CTOX" web unlock signals resolve \
    --id "$SIGNAL_ID" --repair "$REPAIR_ID" \
    --notes "linked via e2e test" >/dev/null

  local SIGNAL_LINKED
  SIGNAL_LINKED=$(sqlite3 "$SQLITE_DB" "SELECT resolved_by_repair_id FROM web_unlock_signals WHERE signal_id=$SIGNAL_ID;")
  assert_eq "$SIGNAL_LINKED" "$REPAIR_ID" "signal resolved_by_repair_id link"
  local SIGNAL_RESOLVED
  SIGNAL_RESOLVED=$(sqlite3 "$SQLITE_DB" "SELECT resolved FROM web_unlock_signals WHERE signal_id=$SIGNAL_ID;")
  assert_eq "$SIGNAL_RESOLVED" "1" "signal.resolved flag"

  log "Cleanup — removing all e2e rows from DB"
  sqlite3 "$SQLITE_DB" "
    DELETE FROM web_unlock_signals WHERE signal_id=$SIGNAL_ID;
    DELETE FROM web_unlock_repairs WHERE repair_id=$REPAIR_ID;
    DELETE FROM web_unlock_test_runs WHERE probe_id='$PROBE_ID';
    DELETE FROM web_unlock_vectors WHERE vector_id='$VECTOR_ID';
    DELETE FROM web_unlock_probes WHERE probe_id='$PROBE_ID';
  "
  rm -f "$SCRIPT_PATH"
  rmdir "$SCRIPT_DIR" 2>/dev/null || true
  ok "cleanup complete"
  log "STAGE 1 PASSED"
}

# ── Stage 2: isolated real stealth regression against bot.incolumitas.com ──

run_stage2() {
  log "STAGE 2 — isolated canonical-source mutation and restored positive control"
  python3 "$ROOT/scripts/web_stack_source.py" mutation-probe
  log "STAGE 2 PASSED"
}

# ── Driver ─────────────────────────────────────────────────────────────────

[[ $STAGE1 -eq 1 ]] && run_stage1
[[ $STAGE2 -eq 1 ]] && run_stage2

log "All requested stages passed."
