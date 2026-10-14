#!/bin/bash
set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo_dir=$(CDPATH= cd -- "$crate_dir/../../../.." && pwd)
output=${1:-"$repo_dir/runtime/cliproxyapi-porting-dashboard.html"}
state_file=${PROJECT_STATE_FILE:-"$crate_dir/project-state.json"}
map_file=${PORT_MAP_FILE:-"$crate_dir/port-map.json"}
closure_file=${CLOSURE_FILE:-"$crate_dir/mirror-closure.json"}
module_file=${MODULE_FILE:-"$crate_dir/module-map.json"}
audit_file=${STRICT_AUDIT_FILE:-"$crate_dir/strict-credit-audit.json"}
integration_file=${CTOX_INTEGRATION_FILE:-"$repo_dir/src/core/execution/cliproxyapi_integration/provider-integration.json"}
review_root=${CANDIDATE_REVIEW_ROOT:-"$repo_dir/runtime/cliproxyapi-upstream-reviews"}

jq -e --slurpfile state "$state_file" '
  def require($condition; $message):
    if $condition then . else error($message) end;
  def safe_relative_path:
    type == "string" and length > 0 and
    (startswith("/") | not) and
    (split("/") | all(. != ".." and . != ""));
  . as $integration |
  ["antigravity_subscription", "claude_subscription", "codex_subscription",
   "kimi_coding_plan", "kimi_subscription", "minimax_coding_plan"] as $expected_modes |
  ["portable_capability", "account_config", "credential_lifecycle",
   "runtime_route", "format_translation", "business_os",
   "pi_model_selection", "live_provider_e2e"] as $expected_gates |
  (.gate_definitions | map(.id)) as $gate_ids |
  (.status_vocabulary) as $statuses |
  (.evidence_registry) as $registry |
  ([.provider_modes[].gates | to_entries[] |
    select(.value != "not_applicable") | .value] | all(. == "verified")) as $derived_complete |
  require(.schema == "ctox.cliproxyapi-provider-integration.v1";
    "CTOX integration schema mismatch") |
  require(.lane == "ctox_product_integration";
    "CTOX integration lane identity mismatch") |
  require((.updated_at | type) == "string" and
          (.updated_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"));
    "CTOX integration update date is invalid") |
  require((.accepted_upstream_commit | test("^[0-9a-f]{40}$")) and
          .accepted_upstream_commit == $state[0].upstream_commit;
    "CTOX integration Accepted Pin differs from Track A") |
  require(($statuses | sort) == (["not_applicable", "partial", "pending", "verified"] | sort);
    "CTOX integration status vocabulary mismatch") |
  require((.required_provider_modes | sort) == ($expected_modes | sort) and
          (.provider_modes | map(.id) | sort) == ($expected_modes | sort);
    "CTOX integration provider inventory is incomplete") |
  require(($gate_ids | sort) == ($expected_gates | sort) and
          (.gate_definitions | all((.acceptance | type) == "string" and
                                   (.acceptance | length) > 0));
    "CTOX integration gate contracts are incomplete") |
  require(($registry | type) == "object" and ($registry | length) > 0 and
          ($registry | to_entries | all(
            .value as $evidence |
            $evidence.kind == "hashed_receipt" and
            $evidence.accepted_upstream_commit == $integration.accepted_upstream_commit and
            ($evidence.path | safe_relative_path) and
            ($evidence.sha256 | test("^[0-9a-f]{64}$")) and
            (($evidence.receipt_schema // "") as $schema |
              $schema == "ctox.cliproxyapi.strict-umbrella-receipt.v1" or
              $schema == "ctox.cliproxyapi.strict-umbrella-receipt.v2" or
              $schema == "ctox.cliproxyapi.strict-umbrella-receipt.v3" or
              $schema == "ctox.cliproxyapi.track-b-receipt.v1") and
            ($evidence.valid_gates | type) == "array" and
            ($evidence.valid_gates | length) > 0 and
            ($evidence.valid_gates | all(. as $gate | $expected_gates | index($gate) != null)) and
            (if ($evidence.receipt_schema // "") == "ctox.cliproxyapi.track-b-receipt.v1" then
               ($evidence.mode_gates | type) == "object" and
               ($evidence.mode_gates | keys | sort) == ($expected_modes | sort) and
               ($evidence.mode_gates | to_entries | all(
                 (.value | type) == "array" and
                 (.value | all(. as $gate |
                   ($expected_gates | index($gate) != null) and
                   ($evidence.valid_gates | index($gate) != null)))))
             else true end) and
            ($evidence.assertion | type) == "string" and ($evidence.assertion | length) > 0));
    "CTOX integration evidence registry is malformed") |
  require((.provider_modes | all(
    . as $mode |
    (.gates | keys | sort) == ($gate_ids | sort) and
    (.gates | to_entries | all(.value as $value | $statuses | index($value) != null)) and
    (.implementation_paths | type) == "array" and
    (.implementation_paths | length) > 0 and
    (.implementation_paths | all(safe_relative_path)) and
    (.gate_evidence | type) == "object" and
    (.gate_evidence | to_entries | all(
      .key as $gate |
      ($mode.gates[$gate] == "verified") and
      (.value | type) == "array" and (.value | length) > 0 and
      (.value | all(. as $id |
        $registry[$id] != null and
        ($registry[$id].valid_gates | index($gate) != null) and
        (if ($registry[$id].receipt_schema // "") == "ctox.cliproxyapi.track-b-receipt.v1" then
           (($registry[$id].mode_gates[$mode.id] // []) | index($gate) != null)
         else true end))))) and
    (.gates | to_entries | all(
      if .value == "verified" then
        .key as $gate |
        ($mode.gate_evidence[$gate] | type) == "array" and
        ($mode.gate_evidence[$gate] | length) > 0
      else true end)) and
    (.next_gate | type) == "string"));
    "CTOX integration provider gate matrix is malformed") |
  require(.completion_rule.kind == "boolean_conjunction" and
          .completion_rule.required_status == "verified" and
          .completion_rule.exclude_status == "not_applicable" and
          .completion_rule.complete == $derived_complete;
    "CTOX integration completion predicate is inconsistent")
' "$integration_file" >/dev/null || {
    echo "CTOX provider integration validation failed" >&2
    exit 1
}

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

while IFS=$'\t' read -r evidence_id receipt_path expected_sha receipt_schema; do
    receipt="$repo_dir/$receipt_path"
    test -f "$receipt" || {
        echo "CTOX integration evidence is missing: $evidence_id ($receipt_path)" >&2
        exit 1
    }
    actual_sha=$(hash_file "$receipt")
    test "$actual_sha" = "$expected_sha" || {
        echo "CTOX integration evidence hash mismatch: $evidence_id" >&2
        exit 1
    }
    if [ "$receipt_schema" = "ctox.cliproxyapi.track-b-receipt.v1" ]; then
        "$repo_dir/src/core/execution/cliproxyapi_integration/check_track_b_receipt.sh" \
            "$receipt" "$(jq -r .accepted_upstream_commit "$integration_file")" >/dev/null
        ledger_mode_gates=$(jq -S -c --arg evidence_id "$evidence_id" \
            '.evidence_registry[$evidence_id].mode_gates' "$integration_file")
        receipt_mode_gates=$(jq -S -c '.verified_mode_gates' "$receipt")
        test "$ledger_mode_gates" = "$receipt_mode_gates" || {
            echo "CTOX integration mode/gate evidence mismatch: $evidence_id" >&2
            exit 1
        }
    elif [ "$receipt_schema" = "ctox.cliproxyapi.strict-umbrella-receipt.v1" ] ||
         [ "$receipt_schema" = "ctox.cliproxyapi.strict-umbrella-receipt.v2" ] ||
         [ "$receipt_schema" = "ctox.cliproxyapi.strict-umbrella-receipt.v3" ]; then
        "$crate_dir/scripts/check_strict_umbrella_receipt.sh" \
            "$receipt" "$(jq -r .accepted_upstream_commit "$integration_file")" >/dev/null
    fi
done < <(jq -r '.evidence_registry | to_entries[] |
  select(.value.kind == "hashed_receipt") |
  [.key, .value.path, .value.sha256, (.value.receipt_schema // "")] | @tsv' "$integration_file")

promotion_pending_full_gate=$(jq -r '
  .upstream_candidate.promoted == true and
  .project_completion.candidate_promoted == true and
  .project_completion.post_promotion_full_gate == false and
  .project_completion.complete == false and
  .project_completion.accepted_pin_complete == false
' "$state_file")
if [ "$promotion_pending_full_gate" != true ]; then
    "$crate_dir/scripts/check_strict_credit_audit.sh" \
        "$audit_file" "$state_file" "$map_file" >/dev/null
fi

candidate=$(jq -r '.upstream_candidate.commit' "$state_file")
review="$review_root/$candidate/upstream-review.json"
test -f "$review" || {
    echo "candidate review ledger is missing: $review" >&2
    exit 1
}

# The dashboard is a projection, never a second progress ledger. Every visible
# candidate counter and the completion label are derived from and checked
# against the commit-local review before the existing output is replaced.
jq -e --slurpfile review "$review" --slurpfile audit "$audit_file" --slurpfile map "$map_file" '
  def require($condition; $message):
    if $condition then . else error($message) end;
  . as $state |
  $review[0] as $review |
  $audit[0] as $audit |
  $map[0] as $map |
  ($review.changes | length) as $total |
  ([$review.changes[] | select(.review_status == "complete")] | length) as $reviewed |
  ([$review.changes[] | select(.review_status == "pending")] | length) as $pending |
  ($review.gates | length) as $gates_total |
  ([$review.gates[] | select(. == true)] | length) as $gates_passed |
  (if ($state.upstream_candidate.promoted and
       ($state.project_completion.post_promotion_full_gate | not)) then
     false
   else
     ($audit.strict_closure.production == $audit.strict_closure.production_total and
      $audit.strict_closure.tests == $audit.strict_closure.tests_total)
   end) as $accepted_complete |
  ($accepted_complete and
   $reviewed == $total and $pending == 0 and
   $gates_passed == $gates_total and
   $state.upstream_candidate.promoted and
   $state.project_completion.post_promotion_full_gate) as $derived_complete |
  require($review.schema == "ctox.cliproxyapi.upstream-review.v3";
    "candidate review schema mismatch") |
  require(
    (if $state.upstream_candidate.promoted then
       $review.candidate_commit == $state.upstream_commit and
       $review.candidate_commit == $state.upstream_candidate.commit and
       $review.base_commit != $state.upstream_commit
     else
       $review.base_commit == $state.upstream_commit and
       $review.candidate_commit == $state.upstream_candidate.commit and
       $review.candidate_commit != $state.upstream_commit
     end);
    "candidate review identity mismatch") |
  require(($review.changes | map(.upstream) | unique | length) == $total;
    "candidate review paths are not unique") |
  require(($review.changes | all(
    .review_status == "complete" or .review_status == "pending"));
    "candidate review contains an unsupported status") |
  require($reviewed + $pending == $total;
    "candidate review counters do not conserve inventory") |
  require(($review.gates | type) == "object" and
          ($review.gates | to_entries | all(.value | type == "boolean"));
    "candidate review gates are malformed") |
  require($state.upstream_candidate.inventory_total == $total and
          $state.upstream_candidate.reviewed == $reviewed and
          $state.upstream_candidate.pending == $pending and
          $state.upstream_candidate.gates_total == $gates_total and
          $state.upstream_candidate.gates_passed == $gates_passed;
    "project-state candidate counters drift from review ledger") |
  require($state.project_completion.accepted_pin_complete == $accepted_complete and
          $state.project_completion.candidate_promoted == $state.upstream_candidate.promoted and
          $state.project_completion.complete == $derived_complete;
    "project completion predicate is inconsistent") |
  require(
    (if ($state.upstream_candidate.promoted and
         ($state.project_completion.post_promotion_full_gate | not)) then
       $audit.upstream_commit == $review.base_commit and
       $map.upstream_commit == $state.upstream_commit and
       $state.mirror_verification.verified_classified_production_files ==
         ($map.summary.production_go_files - $map.summary.production_open_files) and
       $state.mirror_verification.verified_test_files ==
         ($map.summary.test_go_files - $map.summary.test_open_files) and
       $state.mirror_verification.strict_production_files == 0 and
       $state.mirror_verification.strict_test_files == 0
     else true end);
    "promoted post-gate transition counters are inconsistent")
' "$state_file" >/dev/null || {
    echo "dashboard candidate/completion validation failed" >&2
    exit 1
}

mkdir -p "$(dirname -- "$output")"
state_b64=$(base64 < "$state_file" | tr -d '\n')
summary_b64=$(jq -c '.summary' "$map_file" | base64 | tr -d '\n')
closure_b64=$(base64 < "$closure_file" | tr -d '\n')
module_b64=$(base64 < "$module_file" | tr -d '\n')
audit_b64=$(base64 < "$audit_file" | tr -d '\n')
integration_b64=$(base64 < "$integration_file" | tr -d '\n')
updated=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
artifact_json=$(jq -cn \
    --arg schema "ctox.cliproxyapi.dashboard-snapshot.v1" \
    --arg generated_at "$updated" \
    --arg state_sha256 "$(hash_file "$state_file")" \
    --arg map_sha256 "$(hash_file "$map_file")" \
    --arg closure_sha256 "$(hash_file "$closure_file")" \
    --arg module_sha256 "$(hash_file "$module_file")" \
    --arg audit_sha256 "$(hash_file "$audit_file")" \
    --arg review_sha256 "$(hash_file "$review")" \
    --arg integration_sha256 "$(hash_file "$integration_file")" \
    '{schema: $schema, generated_at: $generated_at, input_sha256: {
      project_state: $state_sha256, port_map: $map_sha256,
      mirror_closure: $closure_sha256, module_map: $module_sha256,
      strict_credit_audit: $audit_sha256, candidate_review: $review_sha256,
      ctox_integration: $integration_sha256
    }}')
artifact_b64=$(printf '%s' "$artifact_json" | base64 | tr -d '\n')
output_dir=$(dirname -- "$output")
tmp_output=$(mktemp "$output_dir/.cliproxyapi-dashboard.XXXXXX")
trap 'rm -f "$tmp_output"' EXIT

sed \
    -e "s|__STATE_B64__|$state_b64|" \
    -e "s|__SUMMARY_B64__|$summary_b64|" \
    -e "s|__CLOSURE_B64__|$closure_b64|" \
    -e "s|__MODULE_B64__|$module_b64|" \
    -e "s|__AUDIT_B64__|$audit_b64|" \
    -e "s|__INTEGRATION_B64__|$integration_b64|" \
    -e "s|__ARTIFACT_B64__|$artifact_b64|" \
    -e "s|__UPDATED__|$updated|" \
    "$crate_dir/dashboard.template.html" > "$tmp_output"

mv "$tmp_output" "$output"
trap - EXIT

echo "$output"
