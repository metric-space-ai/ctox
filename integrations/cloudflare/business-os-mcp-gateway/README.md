# CTOX Business OS MCP Gateway

Cloudflare Worker gateway for `mcp.ctox.dev`.

This is a public HTTPS edge for MCP clients that cannot reach a local CTOX
instance directly. It is not a Business OS data store and not an RxDB HTTP
proxy.

## Modes

### Explicit HTTP Relay

```text
ChatGPT / Agent -> https://mcp.ctox.dev/mcp -> configured CTOX MCP upstream
```

Configure `UPSTREAM_MCP_URL` to a reachable MCP endpoint, usually a local CTOX
MCP server exposed through a tunnel during development:

```text
ctox business-os mcp serve --addr 127.0.0.1:8788
```

### Managed Rendezvous

The managed route is scoped by instance id:

```text
Agent        -> POST /mcp/<instance-id>
CTOX daemon  -> GET  /connect/<instance-id> with WebSocket upgrade
Operator     -> GET  /status/<instance-id>
```

Local CTOX connector:

```bash
ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id>
```

The connector reconnects with bounded exponential backoff by default. Use
`--once` for a single connection attempt during tests.

If `INSTANCE_CONNECT_TOKEN` is configured on the Worker, pass the same value via
`--token` or `CTOX_BUSINESS_OS_MCP_CONNECT_TOKEN`.

When a connect token is configured, the CTOX connector also sends
`x-ctox-mcp-timestamp` and `x-ctox-mcp-nonce`. The gateway rejects stale
connect attempts and reused nonces for the active Durable Object session.

The Durable Object class `BusinessOsMcpSession` keeps only session state and
pending request metadata. It forwards JSON-RPC request bodies to the connected
CTOX instance and returns the response body to the MCP client. It does not store
Business OS collections or mirror RxDB.

On connect, CTOX sends a `ctox_hello` envelope with CTOX version, MCP protocol
version, and connector capabilities. `GET /status/<instance-id>` exposes that
metadata while the instance is connected.

`GET /status/<instance-id>` also exposes bounded session counters such as
accepted, completed, failed, rejected, backpressure, oversized payloads,
disconnects, replacements, and last activity timestamps. It does not include
MCP payloads or Business OS records.

`GET /health` is unauthenticated and returns only non-secret operational
posture: mode, whether auth/allowlists are configured, and active limits. It
does not return token values, instance ids, MCP payloads, or Business OS data.
All JSON responses use `cache-control: no-store`.

From CTOX, operators can query the same status endpoint:

```bash
ctox business-os mcp gateway-status --url https://mcp.ctox.dev/status/<instance-id>
```

If `MCP_GATEWAY_TOKEN` is configured, pass it via `--token` or
`CTOX_BUSINESS_OS_MCP_GATEWAY_TOKEN`.

## Limits

Defaults:

```text
MAX_MCP_BODY_BYTES=1000000
MAX_MCP_RESPONSE_BYTES=1000000
MCP_SESSION_TIMEOUT_MS=30000
MAX_PENDING_REQUESTS=16
CONNECT_REPLAY_WINDOW_MS=300000
MAX_CONNECT_NONCES=128
REQUIRE_CONNECT_REPLAY_GUARD=true
```

Oversized requests return `request_too_large`. Oversized CTOX responses return
`response_too_large`. Both are gateway errors; Business OS data remains local to
the connected CTOX instance.

When too many MCP calls are already pending for one CTOX instance, the gateway
returns `backpressure` with HTTP 429.

## Security

Set `ALLOWED_INSTANCE_IDS` as a comma-separated allowlist to constrain the
managed routes:

```text
ALLOWED_INSTANCE_IDS=desk_123,org:desk.1
```

If unset, valid instance id syntax is accepted and access is controlled only by
the configured bearer tokens.

For production, set `MCP_REQUIRE_CLIENT_IDENTITY=true` and configure
`CTOX_MANAGED_MCP_AUTH_URL=https://ctox.dev/api/managed-mcp/client-auth`.
Tokens minted in the ctox.dev tenant dashboard are then validated by ctox.dev,
and the gateway receives the authoritative Business OS MCP context plus the
token policy. If a shared secret is configured on ctox.dev, set
`CTOX_MANAGED_MCP_AUTH_TOKEN` as a Worker secret; do not put it in
`wrangler.jsonc`.

Static/legacy deployments may configure `MCP_CLIENT_TOKENS` as a Worker secret.
This turns bearer tokens into registered agent identities and lets the gateway
inject an authoritative Business OS MCP context:

```json
{
  "<client-token>": {
    "actor": "ctox-dev:user:<user-id>",
    "workspace": "tenant:<tenant-id-or-domain>",
    "tenant_id": "<tenant-id-or-domain>",
    "role": "owner",
    "client_id": "codex-local",
    "allowed_instances": ["cto1.example.com"]
  }
}
```

The gateway forwards this context to the connected CTOX daemon and ignores any
agent-supplied `_context` values for `tools/call`.

Legacy deployments may set `MCP_GATEWAY_TOKEN` as a Worker secret to require:

```text
Authorization: Bearer <token>
```

Set `UPSTREAM_AUTHORIZATION` as a Worker secret if the upstream MCP endpoint
requires its own credential.

Set `INSTANCE_CONNECT_TOKEN` as a Worker secret to require the same bearer-token
shape for CTOX daemon outbound connections to `/connect/<instance-id>`.

For per-instance connect credentials, set `INSTANCE_CONNECT_TOKENS` instead:

```text
INSTANCE_CONNECT_TOKENS=desk_123=secret-a,org:desk.1=secret-b
```

JSON object syntax is also accepted. A matching scoped token takes precedence
over `INSTANCE_CONNECT_TOKEN`.

Replay controls:

```text
CONNECT_REPLAY_WINDOW_MS=300000
MAX_CONNECT_NONCES=128
REQUIRE_CONNECT_REPLAY_GUARD=true
```

Replay protection is automatically required whenever `INSTANCE_CONNECT_TOKEN` or
`INSTANCE_CONNECT_TOKENS` is configured. `REQUIRE_CONNECT_REPLAY_GUARD=true`
enforces the same timestamp/nonce headers even in tokenless development setups.

## Test

```bash
npm test
npm run check
```

`npm run check` runs the unit tests, validates the smoke-test script syntax, and
fails if `wrangler.jsonc` is missing the managed Durable Object binding, the
`mcp.ctox.dev` custom domain route, replay defaults, numeric limits, or if Worker secrets are
accidentally placed into `vars`.

## Managed Smoke Test

The smoke test validates the deployed managed rendezvous contract without
dumping Business OS data.

Without a connected CTOX instance, it expects `/health` and `/status` to work
and `/mcp/<instance-id>` to return `runtime_unavailable`:

```bash
GATEWAY_BASE_URL=https://mcp.ctox.dev \
INSTANCE_ID=desk_123 \
MCP_GATEWAY_TOKEN=<client-token-if-configured> \
npm run smoke
```

With a local CTOX connector running:

```bash
export CTOX_BUSINESS_OS_MCP_CONNECT_TOKEN=<connect-token>
ctox business-os mcp connect \
  --url wss://mcp.ctox.dev/connect/desk_123

GATEWAY_BASE_URL=https://mcp.ctox.dev \
INSTANCE_ID=desk_123 \
MCP_GATEWAY_TOKEN=<client-token-if-configured> \
EXPECT_CONNECTED=true \
npm run smoke
```

## Deploy

Production deploy checklist:

1. Confirm `wrangler.jsonc` points at the `mcp.ctox.dev` custom domain.
2. Set `ALLOWED_INSTANCE_IDS` for known production instances.
3. Set `MCP_GATEWAY_TOKEN` for MCP client access.
4. Set `INSTANCE_CONNECT_TOKENS` for per-instance CTOX daemon access.
5. Keep `REQUIRE_CONNECT_REPLAY_GUARD=true`.
6. Run `npm test`.
7. Deploy.
8. Run `npm run smoke` against `https://mcp.ctox.dev`.

```bash
npx wrangler deploy
```

Secret setup examples:

```bash
npx wrangler secret put MCP_GATEWAY_TOKEN
npx wrangler secret put INSTANCE_CONNECT_TOKENS
```

`INSTANCE_CONNECT_TOKENS` can use either `desk_123=secret-a,org:desk.1=secret-b`
or JSON object syntax.
