#!/bin/bash
set -eu

crate_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
delta=${1:-"$crate_dir/upstream-delta.json"}
output=${2:-"$crate_dir/upstream-review.json"}

jq '{
  schema: "ctox.cliproxyapi.upstream-review.v3",
  base_commit,
  candidate_commit,
  status: (if .clean then "ready_for_gate_evidence" else "in_progress" end),
  changes: [.changes[] | {
    upstream,
    module: .module,
    kind,
    source_kind,
    required_action,
    review_status: "pending",
    disposition: "",
    evidence: [],
    rust_evidence: [],
    upstream_evidence: []
  }],
  gates: {
    non_go_impact_review: false,
    dependency_audit: false,
    rust_no_default: false,
    rust_default: false,
    integrations: false,
    clippy_no_default: false,
    clippy_all_features: false,
    formatting: false,
    tracking: false,
    dashboard: false
  },
  gate_evidence: (
    [
      "non_go_impact_review",
      "dependency_audit",
      "rust_no_default",
      "rust_default",
      "integrations",
      "clippy_no_default",
      "clippy_all_features",
      "formatting",
      "tracking",
      "dashboard"
    ]
    | map({
        key: .,
        value: {
          status: "pending",
          command: [],
          completed_at: "",
          output_sha256: "",
          log: ""
        }
      })
    | from_entries
  )
}' "$delta" > "$output"

echo "$output"
