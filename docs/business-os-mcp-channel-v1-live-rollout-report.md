# Business OS MCP Channel v1 Live Rollout Report

Date: 2026-05-28

## Result

Status: **live for internal managed-gateway validation**

The Cloudflare Worker gateway is deployed on `https://mcp.ctox.dev` and routes
managed MCP traffic for the internal validation instance `desk_123` and the
Kunstmen instance `cto1.kunstmen.com`.

This is ready for internal production validation with one controlled instance.
It is not yet a broad public rollout because production actor, workspace,
module, and collection allowlists still need to be finalized by the admin.

## Deployed Gateway

- Worker: `ctox-business-os-mcp-gateway`
- Account: `06367c9055361cea847cc53e7289cc1b`
- Custom domain: `mcp.ctox.dev`
- Mode: `managed_rendezvous`
- Instance allowlist: `desk_123`, `cto1.kunstmen.com`
- Client MCP auth: enabled through `MCP_GATEWAY_TOKEN`
- Instance connect auth: enabled through `INSTANCE_CONNECT_TOKENS`
- Connect replay guard: enabled
- Latest deployed version during rollout: `873944e7-226b-4476-918e-9af890c541ca`

## Evidence

Local MCP channel tests:

```bash
cargo test mcp_channel
```

Result:

- `36 passed`
- `0 failed`

Gateway checks:

```bash
cd integrations/cloudflare/business-os-mcp-gateway
npm run check
```

Result:

- Node test suite passed
- Worker config validation passed

Gateway secrets:

```bash
npx wrangler secret list
```

Result:

- `INSTANCE_CONNECT_TOKENS`
- `MCP_GATEWAY_TOKEN`

Unconnected managed smoke:

```bash
MCP_GATEWAY_TOKEN=<client-token> INSTANCE_ID=desk_123 npm run smoke
```

Result:

- `/health` returned Worker gateway JSON with `mode: managed_rendezvous`
- `/status/desk_123` was reachable and reported no connected CTOX instance
- `/mcp/desk_123` returned JSON-RPC `runtime_unavailable`

Connected managed smoke:

```bash
export CTOX_BUSINESS_OS_MCP_CONNECT_TOKEN=<instance-connect-token>
ctox business-os mcp connect \
  --url wss://mcp.ctox.dev/connect/desk_123 \
  --once
```

Then:

```bash
MCP_GATEWAY_TOKEN=<client-token> \
INSTANCE_ID=desk_123 \
EXPECT_CONNECTED=true \
npm run smoke
```

Result:

- `/health` passed
- `/status/desk_123` reported `connected: true`
- `/mcp/desk_123` forwarded `tools/list` through the connected CTOX instance
- `tools/list` returned `23` tools

Kunstmen Codex binding:

```bash
codex mcp get cto1-kunstmen-business-os
```

Result:

- `transport: streamable_http`
- `url: https://mcp.ctox.dev/mcp/cto1.kunstmen.com`
- `bearer_token_env_var: CTOX_BUSINESS_OS_MCP_TOKEN`
- Skill installed at `~/.codex/skills/ctox-business-os-mcp`

Kunstmen unconnected managed smoke:

```bash
MCP_GATEWAY_TOKEN=<client-token> INSTANCE_ID=cto1.kunstmen.com npm run smoke
```

Result:

- `/health` passed
- `/status/cto1.kunstmen.com` was reachable and reported no connected CTOX
  instance
- `/mcp/cto1.kunstmen.com` returned JSON-RPC `runtime_unavailable`

Direct connected status sample:

```json
{
  "ok": true,
  "connected": true,
  "pending": 0,
  "session": {
    "ctox_version": "0.3.22",
    "mcp_protocol_version": "2025-06-18",
    "capabilities": [
      "business_os_mcp_channel_v1",
      "managed_gateway_connector",
      "typed_tools",
      "audit_events",
      "approval_gated_actions"
    ]
  }
}
```

## Go/No-Go

Decision: **Go for controlled internal validation**

Go criteria met:

- Local MCP channel tests pass.
- Cloudflare Worker is deployed on `mcp.ctox.dev`.
- Worker route returns gateway health JSON.
- Managed smoke passes without a connected CTOX instance.
- Managed smoke passes with a connected CTOX instance.
- Gateway secrets and instance allowlist are configured.
- Codex is configured with the `cto1-kunstmen-business-os` MCP server and the
  external `ctox-business-os-mcp` skill.

Remaining before broader production:

- Finalize Business OS MCP policy allowlists for production actors, workspaces,
  modules, and collections.
- Start and supervise the outbound `ctox business-os mcp connect` process on the
  actual `cto1.kunstmen.com` CTOX instance. SSH access from this environment was
  not authenticated during rollout, so the gateway currently reports
  `runtime_unavailable` for `cto1.kunstmen.com` until that connector runs.
- Rotate rollout tokens if the initially generated validation tokens should not
  become long-lived production credentials.
- Keep external effects disabled until the admin explicitly approves a narrower
  external-effect policy.
