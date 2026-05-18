#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="docs/kernel-dev/forensics/INDEX.md"
mkdir -p "$(dirname "$out")"

{
  cat <<'HEADER'
# Cache Forensics Index

Generated from `tools/list_cache_forensics.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_cache_forensics_index.sh
```

HEADER
  tools/list_cache_forensics.sh --markdown
} > "$out"

echo "updated: $out"
