# Business OS MCP Channel v1 Release Notes

## Summary

Business OS MCP Channel v1 exposes CTOX Business OS as a typed MCP channel for
external agents. It is designed as a controlled communication path for querying
Business OS state, delegating typed actions, tracking command status, and
handling approval decisions.

It is not a shell, raw SQL bridge, browser remote control surface, or RxDB
replication endpoint.

## Included

- Local MCP server via `ctox business-os mcp serve`
- Managed `mcp.ctox.dev` Cloudflare Gateway
- External GitHub-installable Agent Skill
- Runtime policy gate
- Actor, workspace, module, and collection allowlists
- Tool deny-list
- Response redaction for obvious secret fields
- Response size limits
- Per actor/workspace rate limit
- Audit events, JSON/JSONL export, and retention pruning
- Stable JSON-RPC error contract via `error.data.code`
- Gateway Durable Object session routing
- Gateway per-instance connect tokens
- Gateway connect replay protection
- Gateway health/status/smoke/config validation

## Tool Surface

Read tools:

- `business_os.status`
- `business_os.list_modules`
- `business_os.get_module`
- `business_os.list_entities`
- `business_os.search_records`
- `business_os.query_records`
- `business_os.get_record`
- `business_os.get_record_context`
- `business_os.list_record_activity`
- `business_os.list_runs`
- `business_os.get_run`
- `business_os.list_artifacts`
- `business_os.get_artifact`
- `business_os.list_approvals`
- `business_os.open_link`
- `business_os.list_mcp_activity`

Action tools:

- `business_os.list_module_actions`
- `business_os.propose_action`
- `business_os.execute_action`
- `business_os.approve`
- `business_os.reject`
- `business_os.request_changes`
- `business_os.get_command_status`

## Known Limits

- No ChatGPT widget is included in v1. The channel is headless.
- Managed gateway does not store or index Business OS records.
- The gateway smoke test needs production tokens and a connected CTOX instance
  for full end-to-end validation.
- External-effect approvals remain policy-gated and are disabled by default at
  the external-effect level.
- Tool responses are summary/bounded responses; dense inspection should happen
  through Business OS deep links.
- The managed gateway is live for `desk_123` and allowlisted for
  `cto1.kunstmen.com`. Codex has a `cto1-kunstmen-business-os` MCP entry using
  `https://mcp.ctox.dev/mcp/cto1.kunstmen.com`.
- `cto1.kunstmen.com` still requires a supervised outbound connector on the
  actual CTOX instance before Codex can read data or delegate actions through
  MCP.
- Broad production actor/workspace/module/collection allowlists must be chosen
  by the admin before wider rollout.
- Clean-install and upgrade smoke tests still need to be run against the target
  release artifact.

## Release Gate

Required local checks:

```bash
cargo test mcp_channel
cargo test service::business_os
cargo fmt --check

cd integrations/cloudflare/business-os-mcp-gateway
npm run check

cd skills/ctox-business-os-mcp
node --test test/*.test.mjs
node scripts/validate-skill-contract.mjs
```

Required managed checks:

```bash
cd integrations/cloudflare/business-os-mcp-gateway
GATEWAY_BASE_URL=https://mcp.ctox.dev \
INSTANCE_ID=<instance-id> \
MCP_GATEWAY_TOKEN=<token-if-configured> \
npm run smoke
```

With the local CTOX connector online:

```bash
EXPECT_CONNECTED=true npm run smoke
```

## Rollout Recommendation

Start with one internal workspace and a narrow allowlist:

```bash
ctox business-os mcp policy set \
  --enabled true \
  --allow-reads true \
  --allow-writes true \
  --allow-approvals true \
  --allow-external-effects false \
  --rate-limit-per-minute 120 \
  --audit-retention-days 90

ctox business-os mcp policy set \
  --allow-actor <actor-id> \
  --allow-workspace <workspace-id> \
  --allow-module customers \
  --allow-collection customer_accounts
```

Expand module and collection scope only after audit review.
