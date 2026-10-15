#!/usr/bin/env bash
# Canonical filtered Harness gate for the GPU lane and CI.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
scratch="$(mktemp -d "${TMPDIR:?set TMPDIR to the lane scratch directory}/ctox-harness-gate.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

run_test() {
  local package="$1" filter="$2"
  local result="$scratch/result"
  cargo test --locked --manifest-path src/core/harness/Cargo.toml --jobs 2 \
    -p "$package" --lib "$filter" -- --color=never --test-threads=2 | tee "$result"
  python3 - "$result" "$filter" <<'HARNESS_SUMMARY'
import pathlib, re, sys
result = pathlib.Path(sys.argv[1]).read_text()
summaries = re.findall(r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;[^\n]*$", result, re.M)
if len(summaries) != 1 or int(summaries[0][0]) == 0 or int(summaries[0][1]) != 0:
    raise SystemExit("Harness filter did not execute passing tests: " + sys.argv[2])
HARNESS_SUMMARY
}

if [ "$#" -eq 2 ]; then
  run_test "$1" "$2"
  exit 0
elif [ "$#" -ne 0 ]; then
  echo "usage: $0 [PACKAGE FILTER]" >&2
  exit 2
fi

run_test ctox-core compact_controller
run_test ctox-core harness_subagent_spawn_model_forbids_free_subagents
for test_name in \
  exact_turn_interrupt_and_completion_have_one_atomic_terminal_winner \
  exact_turn_interrupt_receipt_waits_for_its_own_stop \
  exact_turn_interrupt_rejects_delayed_request_after_replacement \
  exact_turn_interrupt_rejects_unknown_empty_and_finished_turns \
  exact_turn_interrupt_removes_cancelled_and_failed_receipts \
  exact_turn_interrupt_session_exit_rejects_unprocessed_request \
  abort_regular_task_emits_turn_aborted_only
do
  run_test ctox-core "$test_name"
done
run_test ctox-app-server exact_turn_interrupt_rpc_preserves_identity_and_successor
run_test ctox-app-server test_handle_turn_interrupted_emits_interrupted_with_error
run_test ctox-app-server-protocol late_turn_complete_does_not_close_active_turn
