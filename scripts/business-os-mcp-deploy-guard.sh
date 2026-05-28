#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: business-os-mcp-deploy-guard.sh --binary <path> --install-bin <path> [--restart-user-units]

Validates a CTOX binary before installing it into a managed Business OS MCP
instance. The guard fails closed if the binary does not expose the MCP CLI or
the Business OS MCP command-status tool.
USAGE
}

binary=""
install_bin=""
restart_units=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary)
      binary="${2:-}"
      shift 2
      ;;
    --install-bin)
      install_bin="${2:-}"
      shift 2
      ;;
    --restart-user-units)
      restart_units=true
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$binary" || -z "$install_bin" ]]; then
  usage >&2
  exit 2
fi

if [[ ! -x "$binary" ]]; then
  echo "candidate binary is not executable: $binary" >&2
  exit 1
fi

help_output="$("$binary" business-os mcp --help 2>&1 || true)"
if ! grep -q "business-os mcp connect" <<<"$help_output"; then
  echo "candidate binary does not expose 'business-os mcp connect'" >&2
  echo "$help_output" >&2
  exit 1
fi

tools_output="$("$binary" business-os mcp tools 2>&1 || true)"
if ! grep -q "business_os.get_command_status" <<<"$tools_output"; then
  echo "candidate binary does not expose business_os.get_command_status" >&2
  echo "$tools_output" >&2
  exit 1
fi

backup="${install_bin}.pre-mcp-guard-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$(dirname "$install_bin")"
if [[ -e "$install_bin" ]]; then
  cp -a "$install_bin" "$backup"
  echo "backup=$backup"
fi

install -m 0755 "$binary" "$install_bin"
echo "installed=$install_bin"

if [[ "$restart_units" == true ]]; then
  systemctl --user daemon-reload || true
  systemctl --user restart ctox-business-os-web.service ctox-business-os-mcp.service ctox.service
  systemctl --user --no-pager --plain status ctox-business-os-mcp.service ctox.service | sed -n '1,80p'
fi
