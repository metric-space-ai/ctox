# Business OS MCP Channel v1 Security and Admin Guide

## Scope

Business OS MCP Channel v1 is a typed communication channel for agents. It is
not shell access, a raw database bridge, or an RxDB replication endpoint.

The channel exposes Business OS concepts through MCP tools:

- modules
- entities
- records
- runs
- artifacts
- approvals
- commands
- audit activity

## Data Flow

Local mode:

```text
Agent -> local CTOX MCP server -> local Business OS store
```

Managed mode:

```text
Agent -> mcp.ctox.dev -> connected CTOX instance -> local Business OS store
```

The managed gateway relays JSON-RPC bodies to the connected CTOX instance. It
must not mirror Business OS collections, RxDB documents, files, credentials, or
MCP response payloads into durable central storage.

## Admin Policy

Admins can inspect policy:

```bash
ctox business-os mcp policy
ctox business-os mcp policy keys
```

Recommended internal default:

```bash
ctox business-os mcp policy set \
  --enabled true \
  --allow-reads true \
  --allow-writes true \
  --allow-approvals true \
  --allow-external-effects false \
  --rate-limit-per-minute 120 \
  --audit-retention-days 90
```

Restrict scope for a specific agent/workspace:

```bash
ctox business-os mcp policy set \
  --allow-actor chatgpt:user \
  --allow-workspace workspace-a \
  --allow-module customers \
  --allow-collection customer_accounts
```

Emergency disable:

```bash
ctox business-os mcp policy set --enabled false
```

## Managed Gateway Secrets

Worker secrets:

```bash
npx wrangler secret put MCP_GATEWAY_TOKEN
npx wrangler secret put INSTANCE_CONNECT_TOKENS
```

Do not put these values into `wrangler.jsonc` `vars`.

Recommended `INSTANCE_CONNECT_TOKENS` format:

```text
desk_123=secret-a,org:desk.1=secret-b
```

For the Kunstmen managed instance, the instance id is:

```text
cto1.kunstmen.com
```

The gateway allowlist must include it:

```text
ALLOWED_INSTANCE_IDS=desk_123,cto1.kunstmen.com
```

The matching connect secret must be scoped to that id:

```text
INSTANCE_CONNECT_TOKENS=desk_123=secret-a,cto1.kunstmen.com=secret-b
```

Do not commit real token values. Store local operator copies outside the repo,
for example in `~/.codex/ctox-business-os-mcp-secrets.env` with mode `0600`.

## Codex Binding

Install the external agent skill into Codex:

```bash
cp -R skills/ctox-business-os-mcp ~/.codex/skills/ctox-business-os-mcp
```

Configure the managed MCP server for the Kunstmen instance:

```bash
codex mcp add cto1-kunstmen-business-os \
  --url https://mcp.ctox.dev/mcp/cto1.kunstmen.com \
  --bearer-token-env-var CTOX_BUSINESS_OS_MCP_TOKEN
```

The bearer token must be available to the Codex process environment as
`CTOX_BUSINESS_OS_MCP_TOKEN`. On macOS GUI launches, set it through launchd or
launch Codex from a shell that exports it.

In managed production mode this bearer token is not a generic shared gateway
password. The Cloudflare gateway must register it in `MCP_CLIENT_TOKENS` and map
it to an actor, workspace, role, tenant, and allowed instance ids. The gateway
then injects that context into the CTOX gateway envelope. The CTOX daemon treats
that envelope context as authoritative and overwrites any `_context` an agent
tries to send inside `tools/call` arguments.

The MCP server entry is useful only after the CTOX instance has connected
outbound:

```bash
export CTOX_BUSINESS_OS_MCP_CONNECT_TOKEN=<cto1-instance-connect-token>
ctox business-os mcp connect \
  --url wss://mcp.ctox.dev/connect/cto1.kunstmen.com
```

## Privacy Notice

When an agent calls MCP tools, the following may leave the local CTOX process:

- the tool name
- tool arguments
- bounded tool response summaries
- approval decisions and comments
- operational metadata such as actor, workspace, request id, status, and timing

The channel redacts obvious secret fields before returning MCP responses. Admins
must still avoid exposing unrestricted collections to external agents.

The managed gateway may observe request and response byte sizes, status codes,
connection state, and bounded operational counters. It must not persist Business
OS records or secrets.

## Audit

Export audit activity:

```bash
ctox business-os mcp audit --limit 100 --format jsonl --output business-os-mcp-audit.jsonl
```

Apply retention pruning:

```bash
ctox business-os mcp audit --prune
```

## Release Gate

Before production enablement:

```bash
cargo test mcp_channel
cargo test service::business_os
cd integrations/cloudflare/business-os-mcp-gateway
npm run check
```

For a deployed gateway:

```bash
GATEWAY_BASE_URL=https://mcp.ctox.dev \
INSTANCE_ID=desk_123 \
MCP_GATEWAY_TOKEN=<client-token-if-configured> \
npm run smoke
```

With a connected CTOX instance:

```bash
EXPECT_CONNECTED=true npm run smoke
```

## Review Checklist

- Channel can be disabled immediately.
- External effects are disabled unless explicitly approved by policy.
- Allowed actors, workspaces, modules, and collections are set for shared
  environments.
- Gateway route is constrained to the `mcp.ctox.dev` custom domain.
- Gateway secrets are Worker secrets, not `vars`.
- Replay guard is enabled for CTOX connect.
- Audit retention is configured.
- Smoke tests pass before user rollout.
