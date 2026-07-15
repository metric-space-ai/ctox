#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/ctox-business-os-assets.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

# shellcheck source=/dev/null
source "$repo_root/install.sh"

source_root="$tmp_dir/source"
state_root="$tmp_dir/state"
source_business_os="$source_root/src/apps/business-os"
state_business_os="$state_root/business-os"

mkdir -p \
  "$source_business_os/modules/desktop" \
  "$source_business_os/modules/invoices" \
  "$source_business_os/shared" \
  "$source_business_os/installed-modules/source-template" \
  "$source_business_os/installed-modules/source-new" \
  "$source_business_os/local-modules/source-private" \
  "$source_business_os/node_modules/pkg" \
  "$source_business_os/notes" \
  "$source_business_os/app-creation-bench" \
  "$state_business_os/installed-modules/runtime-app" \
  "$state_business_os/installed-modules/source-template" \
  "$state_business_os/local-modules/runtime-private"

printf 'source-index\n' > "$source_business_os/index.html"
printf 'source-app\n' > "$source_business_os/app.js"
printf 'source-registry\n' > "$source_business_os/modules/registry.json"
printf 'source-window-manager\n' > "$source_business_os/shared/window-manager.js"
printf 'source-chat-composition\n' > "$source_business_os/shared/shell-chat-composition.js"
printf 'source-desktop\n' > "$source_business_os/modules/desktop/index.js"
printf 'source-icon-drag\n' > "$source_business_os/modules/desktop/iconDrag.js"
printf 'source-invoice-icon\n' > "$source_business_os/modules/invoices/icon.svg"
printf 'source-installed-module\n' > "$source_business_os/installed-modules/source-template/module.json"
printf 'source-new-module\n' > "$source_business_os/installed-modules/source-new/module.json"
printf 'must-not-copy\n' > "$source_business_os/local-modules/source-private/module.json"
printf 'must-not-copy\n' > "$source_business_os/node_modules/pkg/index.js"
printf 'must-not-copy\n' > "$source_business_os/notes/local.md"
printf 'must-not-copy\n' > "$source_business_os/app-creation-bench/run.json"

printf 'stale-registry\n' > "$state_business_os/modules-registry-stale"
mkdir -p "$state_business_os/modules" "$state_business_os/shared"
printf 'stale-registry\n' > "$state_business_os/modules/registry.json"
printf 'stale-window-manager\n' > "$state_business_os/shared/window-manager.js"
printf 'runtime-app-data\n' > "$state_business_os/installed-modules/runtime-app/module.json"
printf 'runtime-existing-data\n' > "$state_business_os/installed-modules/source-template/module.json"
printf 'runtime-private-data\n' > "$state_business_os/local-modules/runtime-private/module.json"

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
assert_file_content "$state_business_os/app.js" "source-app"
assert_file_content "$state_business_os/modules/registry.json" "source-registry"
assert_file_content "$state_business_os/shared/window-manager.js" "source-window-manager"
assert_file_content "$state_business_os/shared/shell-chat-composition.js" "source-chat-composition"
assert_file_content "$state_business_os/modules/desktop/index.js" "source-desktop"
assert_file_content "$state_business_os/modules/desktop/iconDrag.js" "source-icon-drag"
assert_file_content "$state_business_os/modules/invoices/icon.svg" "source-invoice-icon"
assert_file_content "$state_business_os/installed-modules/runtime-app/module.json" "runtime-app-data"
assert_file_content "$state_business_os/installed-modules/source-template/module.json" "runtime-existing-data"
assert_file_content "$state_business_os/installed-modules/source-new/module.json" "source-new-module"
assert_file_content "$state_business_os/local-modules/runtime-private/module.json" "runtime-private-data"

assert_missing "$state_business_os/local-modules/source-private"
assert_missing "$state_business_os/node_modules"
assert_missing "$state_business_os/notes"
assert_missing "$state_business_os/app-creation-bench"

managed_source_root="$tmp_dir/managed-source"
managed_install_root="$tmp_dir/managed-install"
managed_state_root="$tmp_dir/managed-state"
managed_cache_root="$tmp_dir/managed-cache"
managed_bin_dir="$tmp_dir/managed-bin"
managed_source_business_os="$managed_source_root/src/apps/business-os"

mkdir -p "$managed_source_business_os/modules/desktop" "$managed_source_business_os/shared"
printf '[package]\nname = "ctox-fixture"\nversion = "9.8.7"\n' > "$managed_source_root/Cargo.toml"
printf 'managed-index\n' > "$managed_source_business_os/index.html"
printf 'managed-app\n' > "$managed_source_business_os/app.js"
printf 'managed-registry\n' > "$managed_source_business_os/modules/registry.json"
printf 'managed-desktop\n' > "$managed_source_business_os/modules/desktop/index.js"
printf 'managed-window-manager\n' > "$managed_source_business_os/shared/window-manager.js"

INSTALL_ROOT="$managed_install_root"
STATE_ROOT="$managed_state_root"
CACHE_ROOT="$managed_cache_root"
BIN_DIR="$managed_bin_dir"
TOOLS_ROOT="$managed_state_root/tools"
DEPENDENCIES_ROOT="$managed_state_root/dependencies"
CTOX_RELEASE_RETENTION=2

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
assert_file_content "$managed_state_root/business-os/app.js" "managed-app"
assert_file_content "$managed_state_root/business-os/modules/registry.json" "managed-registry"
assert_file_content "$managed_state_root/business-os/shared/window-manager.js" "managed-window-manager"
assert_file_content "$release_dir/business-os/modules/desktop/index.js" "managed-desktop"

printf 'business os shell asset sync smoke ok\n'
