#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/update_kernel_decision_index.sh

Regenerates docs/kernel-dev/decisions/INDEX.md from decision records.
USAGE
}

if [[ $# -ne 0 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

index="docs/kernel-dev/decisions/INDEX.md"
tmp="${index}.tmp"

{
  cat <<'HEADER'
# Kernel Decision Index

Generated from `tools/list_kernel_decisions.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_kernel_decision_index.sh
```

HEADER
  tools/list_kernel_decisions.sh --markdown
} > "$tmp"

mv "$tmp" "$index"
echo "updated: $index"
