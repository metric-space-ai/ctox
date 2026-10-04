#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$repo_root/install.sh"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/ctox-cutover.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
INSTALL_ROOT="$fixture/install"
BIN_DIR="$fixture/public-bin"
STATE_ROOT="$fixture/state"
TOOLS_ROOT="$STATE_ROOT/tools"
DEPENDENCIES_ROOT="$STATE_ROOT/dependencies"
old="$INSTALL_ROOT/releases/old"
next="$INSTALL_ROOT/releases/new"
mkdir -p "$old/bin" "$next/target/release" "$BIN_DIR" "$INSTALL_ROOT/bin"
printf '#!/usr/bin/env bash\nprintf "old|%%s\\n" "$CTOX_ROOT"\n' > "$BIN_DIR/ctox-real"
chmod +x "$BIN_DIR/ctox-real"
write_managed_launch_wrapper "$BIN_DIR/ctox" "$old" "$BIN_DIR/ctox-real"
write_managed_launch_wrapper "$INSTALL_ROOT/bin/ctox" "$old" "$BIN_DIR/ctox-real"
printf 'old-desktop\n' > "$INSTALL_ROOT/bin/ctox-desktop-host"
printf '{"current_release":"old"}\n' > "$INSTALL_ROOT/install_manifest.json"
ln -s "$old" "$INSTALL_ROOT/current"
cp "$BIN_DIR/ctox" "$fixture/old-wrapper"
cp "$BIN_DIR/ctox-real" "$fixture/old-real"
cp "$INSTALL_ROOT/bin/ctox" "$fixture/old-managed-wrapper"
printf '#!/usr/bin/env bash\nprintf "new|%%s\\n" "$CTOX_ROOT"\n' > "$next/target/release/ctox"
printf '#!/usr/bin/env bash\nexit 0\n' > "$next/target/release/ctox-desktop-host"
chmod +x "$next/target/release/ctox" "$next/target/release/ctox-desktop-host"
# Keep the real preparation and wrapper functions, replace only expensive
# build/runtime provisioning. No installer command may touch real services.
detect_platform() { PLATFORM=linux; }
detect_engine_features_auto() { :; }
detect_cuda_home() { :; }
configure_cuda_env() { :; }
build_ctox() { :; }
cleanup_source_build_artifacts() { :; }
sync_business_os_shell_assets() { :; }
ensure_web_runtime_defaults() { :; }
build_google_fetch_helper() { :; }
codesign_binary() { :; }
resolve_ctox_binary_path() { printf '%s/target/release/ctox\n' "$1"; }
resolve_ctox_desktop_host_binary_path() { printf '%s/target/release/ctox-desktop-host\n' "$1"; }
setup_browser_runtime() {
  # A preparation helper must use the release-local executable.
  [[ "$("$1/bin/ctox" --version)" == "new|$next" ]]
}
systemctl() { printf 'unexpected service control during rebuild\n' >&2; exit 91; }
launchctl() { printf 'unexpected launchd control during rebuild\n' >&2; exit 92; }
assert_old_release() {
  cmp "$fixture/old-wrapper" "$BIN_DIR/ctox"
  cmp "$fixture/old-real" "$BIN_DIR/ctox-real"
  cmp "$fixture/old-managed-wrapper" "$INSTALL_ROOT/bin/ctox"
  [[ "$(readlink "$INSTALL_ROOT/current")" == "$old" ]]
  [[ "$(cat "$INSTALL_ROOT/install_manifest.json")" == '{"current_release":"old"}' ]]
  [[ "$(cat "$INSTALL_ROOT/bin/ctox-desktop-host")" == old-desktop ]]
  # Simulate the watchdog's fresh launch at the exact pre-switch boundary.
  [[ "$("$BIN_DIR/ctox" service --foreground)" == "old|$old" ]]
}
run_rebuild "$next"
assert_old_release
[[ "$("$next/bin/ctox" --version)" == "new|$next" ]]
# Failure before staging must preserve the same public launch boundary.
build_ctox() { return 7; }
set +e
(set -e; run_rebuild "$next")
failed=$?
set -e
[[ "$failed" -eq 7 ]]
assert_old_release
printf 'PASS: rebuild prepares candidate without publishing it; failure and watchdog launch preserve old release\n'
