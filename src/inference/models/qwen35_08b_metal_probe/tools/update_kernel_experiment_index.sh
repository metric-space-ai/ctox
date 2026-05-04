#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/update_kernel_experiment_index.sh

Regenerates docs/kernel-dev/experiments/INDEX.md from generated experiment
records. This does not run performance benchmarks.
USAGE
}

if [[ $# -ne 0 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

index="docs/kernel-dev/experiments/INDEX.md"
tmp="${index}.tmp"

{
  cat <<'HEADER'
# Kernel Experiment Index

Generated from `tools/list_kernel_experiments.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_kernel_experiment_index.sh
```

HEADER
  tools/list_kernel_experiments.sh --markdown
} > "$tmp"

mv "$tmp" "$index"
echo "updated: $index"
