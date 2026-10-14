#!/bin/bash
# Origin: CTOX
# License: AGPL-3.0-only
set -euo pipefail
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
upstream_dir=${1:?usage: write_pinned_go_module_scope.sh <upstream-checkout> <commit> <new-output.json>}
pin=${2:?missing immutable commit}
output=${3:?missing new output path}
case "$pin" in *[!0-9a-f]*|"") echo 'expected immutable lowercase Git commit hash' >&2; exit 1 ;; esac
test "${#pin}" -eq 40 || { echo 'expected full Git commit hash' >&2; exit 1; }
git -C "$upstream_dir" cat-file -e "$pin^{commit}"
test ! -e "$output" || { echo 'pinned Go module inventory already exists' >&2; exit 1; }
# NUL-delimited Git paths retain spaces and Unicode. Unsupported line/control
# characters fail validation rather than silently disappearing from an awk scan.
(
    set -C
    git -C "$upstream_dir" ls-tree -r --name-only -z "$pin" |
        jq -Rs --arg pin "$pin" '{
          schema: "ctox.cliproxyapi.pinned-go-modules.v1",
          upstream_commit: $pin,
          modules: [split("\u0000")[] |
            select(. == "go.mod" or endswith("/go.mod")) |
            {go_mod: ., module: (if . == "go.mod" then "." else rtrimstr("/go.mod") end)}]
        }' > "$output"
)
jq -e -L "$script_dir" 'include "strict_go_module_scope";
  valid_pinned_go_scope' "$output" >/dev/null || {
    echo 'invalid pinned Go module inventory' >&2
    exit 1
}
