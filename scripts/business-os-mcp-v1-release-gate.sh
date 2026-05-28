#!/usr/bin/env bash
# Origin: CTOX
# License: AGPL-3.0-only

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root"
cargo test mcp_channel
cargo test service::business_os
cargo fmt --check

cd "$repo_root/integrations/cloudflare/business-os-mcp-gateway"
npm run check

cd "$repo_root/skills/ctox-business-os-mcp"
node --test test/*.test.mjs
node scripts/validate-skill-contract.mjs

echo "ok Business OS MCP Channel v1 local release gate passed"
