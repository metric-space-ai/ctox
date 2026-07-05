#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/ctox-grepplus-shim.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

# shellcheck source=/dev/null
source "$repo_root/install.sh"

target_one="$tmp_dir/grepplus-real-one"
target_two="$tmp_dir/grepplus-real-two"
bin_dir="$tmp_dir/bin"
mkdir -p "$bin_dir"

cat > "$target_one" <<'EOF'
#!/usr/bin/env bash
printf 'grepplus-one:%s\n' "$*"
EOF
chmod +x "$target_one"

cat > "$target_two" <<'EOF'
#!/usr/bin/env bash
printf 'grepplus-two:%s\n' "$*"
EOF
chmod +x "$target_two"

cat > "$bin_dir/grep" <<'EOF'
#!/usr/bin/env bash
printf 'existing-grep:%s\n' "$*"
EOF
chmod +x "$bin_dir/grep"

write_grepplus_shim "$bin_dir/grep" "$target_one" 0
grep_output="$("$bin_dir/grep" context auth)"
if [[ "$grep_output" != "existing-grep:context auth" ]]; then
  printf 'expected existing grep to be preserved, got: %s\n' "$grep_output" >&2
  exit 1
fi
if grep -q 'CTOX managed Grep+ shim' "$bin_dir/grep"; then
  printf 'existing grep was overwritten by Grep+ shim\n' >&2
  exit 1
fi

write_grepplus_shim "$bin_dir/grepplus" "$target_one" 1
grepplus_output="$("$bin_dir/grepplus" context auth)"
if [[ "$grepplus_output" != "grepplus-one:context auth" ]]; then
  printf 'expected grepplus shim to call first target, got: %s\n' "$grepplus_output" >&2
  exit 1
fi

write_grepplus_shim "$bin_dir/grepplus" "$target_two" 1
grepplus_output="$("$bin_dir/grepplus" context auth)"
if [[ "$grepplus_output" != "grepplus-two:context auth" ]]; then
  printf 'expected managed grepplus shim to update target, got: %s\n' "$grepplus_output" >&2
  exit 1
fi

printf 'grepplus shim smoke ok\n'
