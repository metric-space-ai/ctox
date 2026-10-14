#!/bin/bash
set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repo_dir=$(CDPATH= cd -- "$crate_dir/../../../.." && pwd)
upstream_dir=${UPSTREAM_CHECKOUT:-"$repo_dir/runtime/cliproxyapi-upstream"}
candidate_ref=${1:?usage: prepare_upstream_candidate.sh <candidate-ref> [output-dir]}
candidate=$(git -C "$upstream_dir" rev-parse "$candidate_ref^{commit}")
output_dir=${2:-"$repo_dir/runtime/cliproxyapi-upstream-reviews/$candidate"}

mkdir -p "$output_dir"
delta="$output_dir/upstream-delta.json"
review="$output_dir/upstream-review.json"
summary="$output_dir/impact-summary.json"

delta_tmp=$(mktemp "$output_dir/.upstream-delta.XXXXXX")
review_tmp=$(mktemp "$output_dir/.upstream-review.XXXXXX")
summary_tmp=$(mktemp "$output_dir/.impact-summary.XXXXXX")
trap 'rm -f "$delta_tmp" "$review_tmp" "$summary_tmp"' EXIT

"$crate_dir/scripts/build_upstream_delta.sh" "$candidate" "$delta_tmp" >/dev/null
"$crate_dir/scripts/init_upstream_review.sh" "$delta_tmp" "$review_tmp" >/dev/null

jq '{
  schema: "ctox.cliproxyapi.upstream-impact-summary.v1",
  base_commit,
  candidate_commit,
  clean,
  summary,
  modules: (
    [.changes[] | {
      module: .module,
      source_kind,
      required_action
    }]
    | sort_by(.module, .source_kind, .required_action)
    | group_by(.module)
    | map({
        module: .[0].module,
        changed_files: length,
        source_kinds: ([.[].source_kind] | unique),
        required_actions: ([.[].required_action] | unique)
      })
  )
}' "$delta_tmp" > "$summary_tmp"

json_matches() {
    existing=$1
    expected=$2
    jq -e --slurpfile expected "$expected" '. == $expected[0]' "$existing" >/dev/null 2>&1
}

review_matches_delta() {
    existing=$1
    expected_delta=$2
    jq -e --slurpfile delta "$expected_delta" '
      .schema == "ctox.cliproxyapi.upstream-review.v3" and
      .base_commit == $delta[0].base_commit and
      .candidate_commit == $delta[0].candidate_commit and
      ([.changes[] | {
        upstream,
        module: .module,
        kind,
        source_kind,
        required_action
      }] == [$delta[0].changes[] | {
        upstream,
        module: .module,
        kind,
        source_kind,
        required_action
      }])
    ' "$existing" >/dev/null 2>&1
}

fail_mismatch() {
    artifact=$1
    printf 'candidate preparation refused: existing %s does not match candidate %s\n' \
        "$artifact" "$candidate" >&2
    exit 1
}

# Preflight every existing artifact before publishing anything. A matching
# review may contain hours of dispositions and gate evidence, so identity is
# checked independently from its mutable progress fields and the file is never
# regenerated in place.
if [ -e "$delta" ] && ! json_matches "$delta" "$delta_tmp"; then
    fail_mismatch "$delta"
fi
if [ -e "$review" ] && ! review_matches_delta "$review" "$delta_tmp"; then
    fail_mismatch "$review"
fi
if [ -e "$summary" ] && ! json_matches "$summary" "$summary_tmp"; then
    fail_mismatch "$summary"
fi

install_absent_exact() {
    source=$1
    destination=$2
    artifact=$3
    validator=$4
    expected=$5
    if [ -e "$destination" ]; then
        return 0
    fi
    # A hard link is an atomic no-clobber publication because all temporary
    # files live beside their destination. If another preparer wins the race,
    # validate its result instead of replacing it.
    if ln "$source" "$destination" 2>/dev/null; then
        return 0
    fi
    if [ -e "$destination" ] && "$validator" "$destination" "$expected"; then
        return 0
    fi
    fail_mismatch "$artifact"
}

install_absent_exact "$delta_tmp" "$delta" "$delta" json_matches "$delta_tmp"
install_absent_exact "$review_tmp" "$review" "$review" review_matches_delta "$delta_tmp"
install_absent_exact "$summary_tmp" "$summary" "$summary" json_matches "$summary_tmp"

printf '%s\n' "$output_dir"
