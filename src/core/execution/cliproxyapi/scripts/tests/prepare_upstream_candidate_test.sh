#!/bin/bash
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
crate_dir=$(CDPATH= cd -- "$script_dir/.." && pwd)
prepare="$script_dir/prepare_upstream_candidate.sh"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/cliproxyapi-prepare-test.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

# Exercise the complete preparer on an isolated two-commit fixture. Regression
# checks must not depend on an operator's external checkout, shallow history or
# network access. Real candidate preparation is validated separately.
upstream_dir="$scratch/upstream"
mkdir -p "$upstream_dir/internal/review"
git -C "$upstream_dir" init -q
git -C "$upstream_dir" config user.name "CLIProxyAPI prepare test"
git -C "$upstream_dir" config user.email "prepare-test@invalid.example"
git -C "$upstream_dir" config commit.gpgSign false
git -C "$upstream_dir" config core.hooksPath /dev/null
printf 'package review\n// base\n' > "$upstream_dir/internal/review/review.go"
git -C "$upstream_dir" add .
git -C "$upstream_dir" commit -qm base
base=$(git -C "$upstream_dir" rev-parse HEAD)
printf 'package review\n// candidate\n' > "$upstream_dir/internal/review/review.go"
printf '# Candidate review fixture\n' > "$upstream_dir/README.md"
git -C "$upstream_dir" add .
git -C "$upstream_dir" commit -qm candidate
export UPSTREAM_LOCK_FILE="$scratch/lock.json"
export UPSTREAM_PORT_MAP="$scratch/port-map.json"
jq -n --arg base "$base" '{repository: "invalid.example/upstream", base_commit: $base}' > "$UPSTREAM_LOCK_FILE"
jq -n --arg base "$base" '{upstream_commit: $base, files: []}' > "$UPSTREAM_PORT_MAP"

hash_file() {
    git hash-object "$1"
}

assert_unchanged() {
    file=$1
    expected=$2
    actual=$(hash_file "$file")
    if [ "$actual" != "$expected" ]; then
        printf 'expected %s to remain unchanged: %s != %s\n' "$file" "$actual" "$expected" >&2
        exit 1
    fi
}

expect_failure() {
    if "$@" >"$scratch/unexpected.stdout" 2>"$scratch/expected.stderr"; then
        printf 'expected command to fail: %s\n' "$*" >&2
        exit 1
    fi
    if ! grep -q 'candidate preparation refused' "$scratch/expected.stderr"; then
        printf 'missing fail-closed diagnostic for: %s\n' "$*" >&2
        cat "$scratch/expected.stderr" >&2
        exit 1
    fi
}

candidate=$(git -C "$upstream_dir" rev-parse 'HEAD^{commit}')
previous=$(git -C "$upstream_dir" rev-parse 'HEAD~1^{commit}')

# A matching review is resume state, not generated output. Preserve arbitrary
# operator progress and gate evidence byte-for-byte across reruns.
resume_dir="$scratch/resume"
UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$resume_dir" >/dev/null
review="$resume_dir/upstream-review.json"
review_next="$scratch/review-next.json"
jq '.status = "in_progress" | .operator_note = "preserve me" | .gates.tracking = true' \
    "$review" > "$review_next"
mv "$review_next" "$review"
review_hash=$(hash_file "$review")
UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$resume_dir" >/dev/null
assert_unchanged "$review" "$review_hash"

# Reusing an output directory for another immutable candidate must not mutate
# any of the first candidate's artifacts.
delta_hash=$(hash_file "$resume_dir/upstream-delta.json")
summary_hash=$(hash_file "$resume_dir/impact-summary.json")
expect_failure env UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$previous" "$resume_dir"
assert_unchanged "$resume_dir/upstream-delta.json" "$delta_hash"
assert_unchanged "$review" "$review_hash"
assert_unchanged "$resume_dir/impact-summary.json" "$summary_hash"

# A same-candidate delta with altered content is also an identity mismatch.
delta_dir="$scratch/delta-mismatch"
UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$delta_dir" >/dev/null
delta="$delta_dir/upstream-delta.json"
delta_next="$scratch/delta-next.json"
jq '.summary.changed_files += 1' "$delta" > "$delta_next"
mv "$delta_next" "$delta"
delta_hash=$(hash_file "$delta")
review_hash=$(hash_file "$delta_dir/upstream-review.json")
expect_failure env UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$delta_dir"
assert_unchanged "$delta" "$delta_hash"
assert_unchanged "$delta_dir/upstream-review.json" "$review_hash"

# The impact summary is derived from the immutable delta. Treat edits as a
# mismatch rather than silently replacing the operator-visible review input.
summary_dir="$scratch/summary-mismatch"
UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$summary_dir" >/dev/null
summary="$summary_dir/impact-summary.json"
summary_next="$scratch/summary-next.json"
jq '.summary.changed_files += 1' "$summary" > "$summary_next"
mv "$summary_next" "$summary"
summary_hash=$(hash_file "$summary")
review_hash=$(hash_file "$summary_dir/upstream-review.json")
expect_failure env UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$summary_dir"
assert_unchanged "$summary" "$summary_hash"
assert_unchanged "$summary_dir/upstream-review.json" "$review_hash"

# A review whose candidate identity was edited must never be regenerated over.
review_dir="$scratch/review-mismatch"
UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$review_dir" >/dev/null
review="$review_dir/upstream-review.json"
review_next="$scratch/review-mismatch-next.json"
jq '.candidate_commit = "0000000000000000000000000000000000000000"' "$review" > "$review_next"
mv "$review_next" "$review"
review_hash=$(hash_file "$review")
expect_failure env UPSTREAM_CHECKOUT="$upstream_dir" "$prepare" "$candidate" "$review_dir"
assert_unchanged "$review" "$review_hash"

printf 'prepare_upstream_candidate resume tests passed\n'
