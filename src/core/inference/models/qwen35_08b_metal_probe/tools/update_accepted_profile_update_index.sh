#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

out="docs/kernel-dev/profile-updates/INDEX.md"
mkdir -p "$(dirname "$out")"

{
  cat <<'HEADER'
# Accepted Profile Update Index

Generated from `tools/list_accepted_profile_updates.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_accepted_profile_update_index.sh
```

HEADER
  tools/list_accepted_profile_updates.sh --markdown
} > "$out"

echo "updated: $out"
