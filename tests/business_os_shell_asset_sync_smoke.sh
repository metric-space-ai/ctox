#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/ctox-business-os-assets.XXXXXX")"
# Node resolves ESM entrypoints physically; use the same physical fixture path.
tmp_dir="$(cd "$tmp_dir" && pwd -P)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

# The native bootstrap copy is generic; app catalog data comes from the V4 store.
# Runtime app/user data must survive both direct sync and managed installation.
# shellcheck source=/dev/null
source "$repo_root/install.sh"

# This fixture verifies asset copying and managed paths, never host service
# lifecycle. The real installer stop hook must not signal another tenant.
stop_running_ctox_service() { :; }

stage_release_guard() {
  local fixture_root="$1"
  mkdir -p "$fixture_root/src/apps/business-os/scripts"
  cp "$repo_root/src/apps/business-os/scripts/assert-customer-app-isolation.mjs" \
    "$fixture_root/src/apps/business-os/scripts/assert-customer-app-isolation.mjs"
  git -C "$fixture_root" init -q
}

source_root="$tmp_dir/source"
state_root="$tmp_dir/state"
source_business_os="$source_root/src/apps/business-os"
state_business_os="$state_root/business-os"

mkdir -p \
  "$source_business_os/kernel" \
  "$source_business_os/v4" \
  "$source_business_os/shared" \
  "$source_business_os/node_modules/pkg" \
  "$source_business_os/notes" \
  "$source_business_os/app-creation-bench" \
  "$state_business_os/installed-modules/runtime-app" \
  "$state_business_os/installed-modules/source-template" \
  "$state_business_os/local-modules/runtime-private"

printf 'source-index\n' > "$source_business_os/index.html"
printf 'source-bootstrap\n' > "$source_business_os/kernel/index.mjs"
printf 'source-manifest\n' > "$source_business_os/v4/fixture-manifest.json"
printf 'source-theme\n' > "$source_business_os/shared/theme.css"
printf 'source-support\n' > "$source_business_os/shared/support.mjs"
printf 'source-transport\n' > "$source_business_os/kernel/transport.mjs"
printf 'source-activation\n' > "$source_business_os/kernel/activation.mjs"
printf 'source-icon\n' > "$source_business_os/v4/fixture-icon.svg"
printf 'must-not-copy\n' > "$source_business_os/node_modules/pkg/index.js"
printf 'must-not-copy\n' > "$source_business_os/notes/local.md"
printf 'must-not-copy\n' > "$source_business_os/app-creation-bench/run.json"

printf 'stale-manifest\n' > "$state_business_os/stale-bootstrap-marker"
mkdir -p "$state_business_os/v4" "$state_business_os/shared"
printf 'stale-manifest\n' > "$state_business_os/v4/fixture-manifest.json"
printf 'stale-theme\n' > "$state_business_os/shared/theme.css"
printf 'runtime-app-data\n' > "$state_business_os/installed-modules/runtime-app/module.json"
printf 'runtime-existing-data\n' > "$state_business_os/installed-modules/source-template/module.json"
printf 'runtime-private-data\n' > "$state_business_os/local-modules/runtime-private/module.json"

stage_release_guard "$source_root"
sync_business_os_shell_assets "$source_root" "$state_root"

assert_file_content() {
  local file="$1"
  local expected="$2"
  if [[ ! -f "$file" ]]; then
    printf 'expected file missing: %s\n' "$file" >&2
    exit 1
  fi
  local actual
  actual="$(cat "$file")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'unexpected content for %s: expected %q got %q\n' "$file" "$expected" "$actual" >&2
    exit 1
  fi
}

assert_missing() {
  local path="$1"
  if [[ -e "$path" ]]; then
    printf 'path should not have been copied: %s\n' "$path" >&2
    exit 1
  fi
}

assert_file_content "$state_business_os/index.html" "source-index"
assert_file_content "$state_business_os/kernel/index.mjs" "source-bootstrap"
assert_file_content "$state_business_os/v4/fixture-manifest.json" "source-manifest"
assert_file_content "$state_business_os/shared/theme.css" "source-theme"
assert_file_content "$state_business_os/shared/support.mjs" "source-support"
assert_file_content "$state_business_os/kernel/transport.mjs" "source-transport"
assert_file_content "$state_business_os/kernel/activation.mjs" "source-activation"
assert_file_content "$state_business_os/v4/fixture-icon.svg" "source-icon"
assert_file_content "$state_business_os/installed-modules/runtime-app/module.json" "runtime-app-data"
assert_file_content "$state_business_os/installed-modules/source-template/module.json" "runtime-existing-data"
assert_missing "$state_business_os/installed-modules/source-new"
assert_file_content "$state_business_os/local-modules/runtime-private/module.json" "runtime-private-data"

assert_missing "$state_business_os/node_modules"
assert_missing "$state_business_os/notes"
assert_missing "$state_business_os/app-creation-bench"

# Runtime applications in an incoming release are rejected by the actual
# release guard, before copying or changing the existing tenant state.
mkdir -p "$source_business_os/installed-modules/source-new"
printf '{"id":"source-new"}\n' > "$source_business_os/installed-modules/source-new/module.json"
set +e
bash -e -c 'source "$1"; sync_business_os_shell_assets "$2" "$3"' \
  fixture "$repo_root/install.sh" "$source_root" "$state_root" > "$tmp_dir/rejected-release.log" 2>&1
rejected=$?
set -e
if [[ "$rejected" -eq 0 ]] || ! grep -q 'Customer app isolation failed' "$tmp_dir/rejected-release.log"; then
  cat "$tmp_dir/rejected-release.log" >&2
  printf 'runtime application release must fail at its isolation guard\n' >&2
  exit 1
fi
assert_missing "$state_business_os/installed-modules/source-new"
assert_file_content "$state_business_os/installed-modules/runtime-app/module.json" "runtime-app-data"
rm -rf "$source_business_os/installed-modules"

managed_source_root="$tmp_dir/managed-source"
managed_install_root="$tmp_dir/managed-install"
managed_state_root="$tmp_dir/managed-state"
managed_cache_root="$tmp_dir/managed-cache"
managed_bin_dir="$tmp_dir/managed-bin"
managed_source_business_os="$managed_source_root/src/apps/business-os"

mkdir -p "$managed_source_business_os/kernel" "$managed_source_business_os/v4" "$managed_source_business_os/shared"
printf '[package]\nname = "ctox-fixture"\nversion = "9.8.7"\n' > "$managed_source_root/Cargo.toml"
printf 'managed-index\n' > "$managed_source_business_os/index.html"
printf 'managed-bootstrap\n' > "$managed_source_business_os/kernel/index.mjs"
printf 'managed-manifest\n' > "$managed_source_business_os/v4/fixture-manifest.json"
printf 'managed-transport\n' > "$managed_source_business_os/kernel/transport.mjs"
printf 'managed-theme\n' > "$managed_source_business_os/shared/theme.css"

INSTALL_ROOT="$managed_install_root"
STATE_ROOT="$managed_state_root"
CACHE_ROOT="$managed_cache_root"
BIN_DIR="$managed_bin_dir"
TOOLS_ROOT="$managed_state_root/tools"
DEPENDENCIES_ROOT="$managed_state_root/dependencies"
CTOX_RELEASE_RETENTION=2

stage_release_guard "$managed_source_root"
setup_managed_install "$managed_source_root"

release_dir="$managed_install_root/releases/v9.8.7"
if [[ ! -d "$release_dir" ]]; then
  printf 'managed release dir missing: %s\n' "$release_dir" >&2
  exit 1
fi
if [[ "$(readlink "$managed_install_root/current")" != "$release_dir" ]]; then
  printf 'current symlink does not point to release dir\n' >&2
  exit 1
fi
if [[ "$(readlink "$release_dir/business-os")" != "$managed_state_root/business-os" ]]; then
  printf 'release business-os is not a symlink to managed state\n' >&2
  exit 1
fi
if [[ "$(readlink "$release_dir/runtime")" != "$managed_state_root" ]]; then
  printf 'release runtime is not a symlink to managed state\n' >&2
  exit 1
fi

assert_file_content "$managed_state_root/business-os/index.html" "managed-index"
assert_file_content "$managed_state_root/business-os/kernel/index.mjs" "managed-bootstrap"
assert_file_content "$managed_state_root/business-os/v4/fixture-manifest.json" "managed-manifest"
assert_file_content "$managed_state_root/business-os/shared/theme.css" "managed-theme"
assert_file_content "$release_dir/business-os/kernel/transport.mjs" "managed-transport"

printf 'business os shell asset sync smoke ok\n'
