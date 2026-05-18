#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="docs/kernel-dev/autotune/INDEX.md"
mkdir -p "$(dirname "$out")"

{
  cat <<'HEADER'
# Autotune Index

Generated from `tools/list_autotune_records.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_autotune_index.sh
```

HEADER
  tools/list_autotune_records.sh --markdown
} > "$out"

echo "updated: $out"
