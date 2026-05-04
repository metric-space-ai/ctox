#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="docs/kernel-dev/measurements/INDEX.md"
mkdir -p "$(dirname "$out")"

{
  cat <<'HEADER'
# Measurement Index

Generated from `tools/list_measurement_records.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_measurement_index.sh
```

HEADER
  tools/list_measurement_records.sh --markdown
} > "$out"

echo "updated: $out"
