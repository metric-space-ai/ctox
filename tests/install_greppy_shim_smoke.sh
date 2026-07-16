#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/ctox-greppy-shim.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

# shellcheck source=/dev/null
source "$repo_root/install.sh"

if [[ "$GREPPY_REPO@$GREPPY_REV" != \
  "https://github.com/metric-space-ai/greppy.git@b5a5582e37a02ebf9181423a91313b05d26220b6" ]]; then
  printf 'unexpected Greppy installer provenance: %s@%s\n' "$GREPPY_REPO" "$GREPPY_REV" >&2
  exit 1
fi

if ! grep -q 'fetch_model_assets.sh' "$repo_root/install.sh"; then
  printf 'Greppy installer does not fetch pinned model assets\n' >&2
  exit 1
fi
if grep -q 'write_greppy_shim "\$BIN_DIR/grep"' "$repo_root/install.sh"; then
  printf 'Greppy installer must not create a global grep shim\n' >&2
  exit 1
fi

target_one="$tmp_dir/greppy-real-one"
target_two="$tmp_dir/greppy-real-two"
bin_dir="$tmp_dir/bin"
mkdir -p "$bin_dir"

cat > "$target_one" <<'EOF'
#!/usr/bin/env bash
printf 'greppy-one:%s\n' "$*"
EOF
chmod +x "$target_one"

cat > "$target_two" <<'EOF'
#!/usr/bin/env bash
printf 'greppy-two:%s\n' "$*"
EOF
chmod +x "$target_two"

cat > "$bin_dir/grep" <<'EOF'
#!/usr/bin/env bash
printf 'existing-grep:%s\n' "$*"
EOF
chmod +x "$bin_dir/grep"

write_greppy_shim "$bin_dir/grep" "$target_one" 0
grep_output="$("$bin_dir/grep" context auth)"
if [[ "$grep_output" != "existing-grep:context auth" ]]; then
  printf 'expected existing grep to be preserved, got: %s\n' "$grep_output" >&2
  exit 1
fi
if grep -q 'CTOX managed greppy shim' "$bin_dir/grep"; then
  printf 'existing grep was overwritten by greppy shim\n' >&2
  exit 1
fi

grep_status="$(remove_managed_greppy_grep_shim "$bin_dir/grep")"
if [[ "$grep_status" != "preserved-existing" || ! -x "$bin_dir/grep" ]]; then
  printf 'expected existing grep to remain, status=%s\n' "$grep_status" >&2
  exit 1
fi

write_greppy_shim "$bin_dir/managed-grep" "$target_one" 1
managed_status="$(remove_managed_greppy_grep_shim "$bin_dir/managed-grep")"
if [[ "$managed_status" != "removed-managed" || -e "$bin_dir/managed-grep" ]]; then
  printf 'expected managed grep shim removal, status=%s\n' "$managed_status" >&2
  exit 1
fi

write_greppy_shim "$bin_dir/greppy" "$target_one" 1
greppy_output="$("$bin_dir/greppy" context auth)"
if [[ "$greppy_output" != "greppy-one:context auth" ]]; then
  printf 'expected greppy shim to call first target, got: %s\n' "$greppy_output" >&2
  exit 1
fi

write_greppy_shim "$bin_dir/greppy" "$target_two" 1
greppy_output="$("$bin_dir/greppy" context auth)"
if [[ "$greppy_output" != "greppy-two:context auth" ]]; then
  printf 'expected managed greppy shim to update target, got: %s\n' "$greppy_output" >&2
  exit 1
fi

printf 'greppy shim smoke ok\n'
