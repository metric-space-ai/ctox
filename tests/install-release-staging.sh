#!/usr/bin/env bash
# Exercise the real --rebuild function with only expensive/tenant side effects
# stubbed. A failed staging release must never become the global command.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/ctox-release-staging.XXXXXX")"
trap 'rm -rf "$fixture_root"' EXIT
export HOME="$fixture_root/home"
export SHELL=/bin/zsh
export CTOX_INSTALL_ROOT="$fixture_root/install"
export CTOX_STATE_ROOT="$fixture_root/state"
export CTOX_BIN_DIR="$fixture_root/global-bin"
export CTOX_TOOLS_ROOT="$fixture_root/tools"
mkdir -p "$HOME" "$CTOX_BIN_DIR" "$CTOX_STATE_ROOT"

# Sourcing only defines installer functions; its main() is guarded.
source "$repo_root/install.sh"

old_release="$INSTALL_ROOT/releases/old"
new_release="$INSTALL_ROOT/releases/new"
mkdir -p "$old_release/bin" "$new_release/target/release" "$INSTALL_ROOT/bin"
cat > "$old_release/bin/ctox-real" <<'SH'
#!/usr/bin/env bash
printf 'old root=%s\n' "$CTOX_ROOT"
SH
cat > "$new_release/target/release/ctox" <<'SH'
#!/usr/bin/env bash
printf 'new root=%s\n' "$CTOX_ROOT"
SH
printf 'old desktop host\n' > "$INSTALL_ROOT/bin/ctox-desktop-host"
printf 'new desktop host\n' > "$new_release/target/release/ctox-desktop-host"
chmod +x "$old_release/bin/ctox-real" "$new_release/target/release/ctox" \
  "$new_release/target/release/ctox-desktop-host"
ln -s "$old_release" "$INSTALL_ROOT/current"
write_managed_launch_wrapper "$BIN_DIR/ctox" "$INSTALL_ROOT/current" \
  "$INSTALL_ROOT/current/bin/ctox-real"
write_managed_launch_wrapper "$INSTALL_ROOT/bin/ctox" "$INSTALL_ROOT/current" \
  "$INSTALL_ROOT/current/bin/ctox-real"
cp "$old_release/bin/ctox-real" "$BIN_DIR/ctox-real"
cp "$BIN_DIR/ctox" "$fixture_root/original-global-ctox"
cp "$INSTALL_ROOT/bin/ctox" "$fixture_root/original-managed-ctox"
cp "$BIN_DIR/ctox-real" "$fixture_root/original-global-real"
cp "$INSTALL_ROOT/bin/ctox-desktop-host" "$fixture_root/original-desktop-host"

# Keep the actual run_rebuild/publication flow; replace only heavy build,
# tenant-sync and network preparation with isolated no-ops.
detect_platform() { PLATFORM=linux; }
detect_engine_features_auto() { :; }
detect_cuda_home() { :; }
configure_cuda_env() { :; }
build_ctox() { :; }
cleanup_source_build_artifacts() { :; }
sync_business_os_shell_assets() { :; }
ensure_web_runtime_defaults() { :; }
setup_browser_runtime() { :; }
build_google_fetch_helper() { :; }

run_rebuild "$new_release"
[[ "$("$new_release/bin/ctox")" == "new root=$new_release" ]]
[[ "$(readlink "$INSTALL_ROOT/current")" == "$old_release" ]]
cmp -s "$BIN_DIR/ctox" "$fixture_root/original-global-ctox"
cmp -s "$INSTALL_ROOT/bin/ctox" "$fixture_root/original-managed-ctox"
cmp -s "$BIN_DIR/ctox-real" "$fixture_root/original-global-real"
cmp -s "$INSTALL_ROOT/bin/ctox-desktop-host" "$fixture_root/original-desktop-host"
[[ ! -e "$HOME/.zshrc" ]]

# A switch that never happened may remove the candidate without leaving a
# global launcher pointed at a now-missing release root.
rm -rf "$new_release"
[[ "$("$BIN_DIR/ctox")" == "old root=$INSTALL_ROOT/current" ]]
[[ "$("$INSTALL_ROOT/bin/ctox")" == "old root=$INSTALL_ROOT/current" ]]
printf 'release staging preserves the active launcher after failed switch\n'
