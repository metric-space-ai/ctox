#!/bin/bash
set -eu

receipt=${1:?usage: check_strict_umbrella_receipt.sh <receipt.json> [expected-upstream-commit]}
receipt_dir=$(CDPATH= cd -- "$(dirname -- "$receipt")" && pwd)
expected_upstream_commit=${2:-$(jq -r .upstream_commit "$receipt")}
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

jq -e '
  (.schema == "ctox.cliproxyapi.strict-umbrella-receipt.v1" or
   .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v2" or
   .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v3") and
  (.upstream_commit | test("^[0-9a-f]{40}$")) and
  .upstream_clean_before == true and
  .upstream_clean_after == true and
  (.started_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
  (.completed_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
  (.go_version | test("^go version go1\\.26\\.")) and
  (.rustc_version | length) > 0 and
  (.cargo_version | length) > 0 and
  (.source_manifest | length) > 0 and
  (.source_manifest_sha256 | test("^[0-9a-f]{64}$")) and
  (.hydrated_go_manifest | length) > 0 and
  (.hydrated_go_manifest_sha256 | test("^[0-9a-f]{64}$")) and
  (.go_hydration | type) == "array" and
  (.go_hydration | length) > 0 and
  ([.go_hydration[].module] | unique | length) == (.go_hydration | length) and
  (if .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v3" then true
   else (.go_hydration | length) == 23 end) and
  ([.go_hydration[] | select(
    .status != "passed" or
    (.command | length) == 0 or
    (.output_sha256 | test("^[0-9a-f]{64}$") | not) or
    (.log | length) == 0
  )] | length) == 0 and
  (.go_modules | type) == "array" and
  (.go_modules | length) > 0 and
  ([.go_modules[].module] | unique | length) == (.go_modules | length) and
  (if .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v3" then true
   else (.go_modules | length) == 23 end) and
  ([.go_modules[] | select(.module == ".")] | length) == 1 and
  ([.go_modules[] | select(
    .status != "passed" or
    (.command | length) == 0 or
    (.output_sha256 | test("^[0-9a-f]{64}$") | not) or
    (.log | length) == 0
  )] | length) == 0 and
  (.gates | type) == "array" and
  ([.gates[].name] | sort) ==
    (if .schema == "ctox.cliproxyapi.strict-umbrella-receipt.v1" then
       (["rust_no_default", "rust_all_features",
         "clippy_no_default", "clippy_all_features",
         "formatting", "outer_ctox_check", "tracking", "dashboard"] | sort)
     else
       (["rust_no_default", "rust_all_features",
         "clippy_no_default", "clippy_all_features",
         "formatting", "tracking", "dashboard"] | sort)
     end) and
  ([.gates[] | select(
    .status != "passed" or
    (.command | length) == 0 or
    (.output_sha256 | test("^[0-9a-f]{64}$") | not) or
    (.log | length) == 0
  )] | length) == 0
' "$receipt" >/dev/null || {
    echo "invalid strict umbrella receipt: $receipt" >&2
    exit 1
}

test "$(jq -r .upstream_commit "$receipt")" = "$expected_upstream_commit" || {
    echo "strict umbrella receipt upstream pin mismatch: expected $expected_upstream_commit" >&2
    exit 1
}

manifest=$(jq -r .source_manifest "$receipt")
case "$manifest" in
    ""|/*|*..*)
        echo "unsafe strict umbrella source manifest path: $manifest" >&2
        exit 1
        ;;
esac
test -f "$receipt_dir/$manifest" || {
    echo "missing strict umbrella source manifest: $receipt_dir/$manifest" >&2
    exit 1
}
manifest_hash=$(hash_file "$receipt_dir/$manifest")
test "$manifest_hash" = "$(jq -r .source_manifest_sha256 "$receipt")" || {
    echo "strict umbrella source manifest hash mismatch" >&2
    exit 1
}
# This receipt proves the historical gate-time source snapshot. Candidate-port
# work may legitimately change the live Rust tree before pin promotion, so the
# historical manifest is hash-bound to the receipt but intentionally not
# compared with current working-tree files here.

hydrated_manifest=$(jq -r .hydrated_go_manifest "$receipt")
case "$hydrated_manifest" in
    ""|/*|*..*)
        echo "unsafe hydrated Go manifest path: $hydrated_manifest" >&2
        exit 1
        ;;
esac
test -f "$receipt_dir/$hydrated_manifest" || {
    echo "missing hydrated Go manifest: $receipt_dir/$hydrated_manifest" >&2
    exit 1
}
hydrated_hash=$(hash_file "$receipt_dir/$hydrated_manifest")
test "$hydrated_hash" = "$(jq -r .hydrated_go_manifest_sha256 "$receipt")" || {
    echo "hydrated Go manifest hash mismatch" >&2
    exit 1
}

# The hash-bound hydrated snapshot independently identifies every exercised
# module for all receipt versions; changing a count or schema cannot omit one.
jq -e -L "$script_dir" --rawfile hydrated "$receipt_dir/$hydrated_manifest" '
  include "strict_go_module_scope";
  hydrated_go_module_directories($hydrated) as $expected |
  ([.go_hydration[].module] | sort) == $expected and
  ([.go_modules[].module] | sort) == $expected
' "$receipt" >/dev/null || {
    echo "strict umbrella hydrated Go module scope mismatch" >&2; exit 1
}

if [ "$(jq -r .schema "$receipt")" = "ctox.cliproxyapi.strict-umbrella-receipt.v3" ]; then
    scope_manifest=$(jq -r '.go_module_scope_manifest // ""' "$receipt")
    case "$scope_manifest" in
        ""|/*|*..*) echo "unsafe pinned Go module manifest path" >&2; exit 1 ;;
    esac
    test -f "$receipt_dir/$scope_manifest" || {
        echo "missing pinned Go module manifest" >&2; exit 1
    }
    test "$(hash_file "$receipt_dir/$scope_manifest")" = \
         "$(jq -r .go_module_scope_manifest_sha256 "$receipt")" || {
        echo "pinned Go module manifest hash mismatch" >&2; exit 1
    }
    jq -e -L "$script_dir" --slurpfile scope "$receipt_dir/$scope_manifest" \
        --rawfile hydrated "$receipt_dir/$hydrated_manifest" '
      include "strict_go_module_scope";
      ($scope | length) == 1 and go_module_entries_match($scope[0]) and
      (hydrated_go_module_directories($hydrated) == ([$scope[0].modules[].module] | sort))
    ' "$receipt" >/dev/null || {
        echo "strict umbrella Go module scope mismatch" >&2; exit 1
    }
fi

jq -r '.go_hydration[], .go_modules[], .gates[] | [.log, .output_sha256] | @tsv' "$receipt" |
while IFS="$(printf '\t')" read -r log expected; do
    case "$log" in
        ""|/*|*..*)
            echo "unsafe strict umbrella log path: $log" >&2
            exit 1
            ;;
    esac
    path="$receipt_dir/$log"
    test -f "$path" || {
        echo "missing strict umbrella log: $path" >&2
        exit 1
    }
    actual=$(hash_file "$path")
    test "$actual" = "$expected" || {
        echo "strict umbrella log hash mismatch: $path" >&2
        exit 1
    }
done

echo "strict umbrella receipt valid: $(jq -r .upstream_commit "$receipt")"
