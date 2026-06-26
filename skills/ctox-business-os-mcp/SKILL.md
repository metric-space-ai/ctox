---
name: ctox-business-os-mcp
description: Use when an external agent should connect to CTOX Business OS through the Business OS MCP Channel to query modules, records, runs, artifacts, approvals, or delegate validated Business OS actions. Trigger when setting up or using CTOX from ChatGPT, Codex, another MCP-capable agent, or an agent runtime that can install GitHub-hosted skills.
---

# CTOX Business OS MCP

Use this skill to connect an external agent to CTOX Business OS through the
Business OS MCP Channel.

## Core Rule

Treat CTOX Business OS MCP as a typed communication channel, not as terminal
access to CTOX.

Use the MCP server's Business OS objects:

- `Module`
- `Entity`
- `Record`
- `Action`
- `Command`
- `Run`
- `Artifact`
- `Approval`
- `Activity`
- `DeepLink`

Do not invent generic tools such as:

```text
run_cli
run_shell
write_sql
push_rxdb_record
remote_control_browser
execute_raw_business_command
```

## Connection Modes

Prefer the user's configured MCP server.

Supported shapes:

- Local developer mode: `ctox business-os mcp serve`, exposed through an HTTPS
  tunnel for hosted clients.
- Managed mode: `https://mcp.ctox.dev/mcp`, routed to a CTOX instance that has
  explicitly connected outbound.
- Self-hosted mode: a customer-controlled HTTPS MCP endpoint.

For managed production gateways, prefer per-instance connect tokens. The local
CTOX connector sends timestamp/nonce replay-protection headers automatically.

If no CTOX Business OS MCP server is available, say that CTOX MCP is not
connected. Do not pretend to have CTOX access.

## Safe Workflow

1. Call status and module discovery first.
2. Use read tools before write tools.
3. Search or query records with narrow limits.
4. Fetch record context only when needed.
5. Propose actions before executing them.
6. Treat external effects as blocked unless the server explicitly advertises a
   narrower approval tool for that effect.
7. Return Business OS deep links for dense inspection.

## Expected Tool Classes

Read tools:

```text
business_os.status
business_os.list_modules
business_os.get_module
business_os.list_entities
business_os.search_records
business_os.query_records
business_os.get_record
business_os.get_record_context
business_os.list_record_activity
business_os.list_runs
business_os.get_run
business_os.list_artifacts
business_os.get_artifact
business_os.list_approvals
business_os.open_link
business_os.list_mcp_activity
```

Action tools:

```text
business_os.list_module_actions
business_os.propose_action
business_os.create_app
business_os.modify_app
business_os.execute_action
business_os.approve
business_os.reject
business_os.request_changes
business_os.get_command_status
```

If the server exposes fewer tools, use only the advertised tools.

## Managed Identity Context

For managed `mcp.ctox.dev` servers, do not set or rely on `_context` yourself.
The gateway authenticates the bearer token and injects the authoritative
Business OS context:

```text
actor
workspace
client_id
tenant_id
role
instance_id
```

If the response reports a different actor/workspace than expected, treat it as
the server's identity decision. Do not try to spoof `_context` through tool
arguments.

## Business OS Roles And App/Data Scope

Business OS access is two-layered. MCP channel policy decides whether a remote
actor may use the channel at all. Business OS roles, app lifecycle visibility,
and explicit grants then decide whether that actor may see an app, read data,
write data, modify apps, approve work, or perform external effects.

Use the business labels with humans, and the stored role names in policy/audit
output:

```text
Owner -> chef
Admin -> admin
App-Verantwortliche:r -> founder
Teammitglied -> user
```

Runtime app visibility is version-aware: `0.x.y`, missing, or invalid SemVer
apps are private unless the actor is responsible for the app or has explicit
`apps.view`; `1.0.0+` apps are team-visible by default unless restricted.

Do not infer data access from app visibility:

- app visibility for private/preview/restricted apps requires `apps.view`
- app details, entities, actions, and record reads require `data.read`
- module action execution requires app visibility plus `data.write`
- app creation/modification requires `apps.install` or `apps.modify`
- approvals require `external.approve`
- MCP status/audit access requires `mcp.manage` unless the actor is admin/owner

`data.read` or `data.write` must not make a hidden app visible. `apps.view`
must not expose record data by itself. If the server returns
`permission_denied` with field `business_os_policy`, treat it as an
authoritative role/grant denial.

## Runtime Policy

The server can reject tools by policy even when they are advertised.

Relevant policy switches:

```text
CTOX_BUSINESS_OS_MCP_ENABLED
CTOX_BUSINESS_OS_MCP_ALLOW_READS
CTOX_BUSINESS_OS_MCP_ALLOW_WRITES
CTOX_BUSINESS_OS_MCP_ALLOW_APPROVALS
CTOX_BUSINESS_OS_MCP_ALLOW_EXTERNAL_EFFECTS
CTOX_BUSINESS_OS_MCP_RATE_LIMIT_PER_MINUTE
CTOX_BUSINESS_OS_MCP_ALLOWED_ACTORS
CTOX_BUSINESS_OS_MCP_ALLOWED_WORKSPACES
CTOX_BUSINESS_OS_MCP_ALLOWED_MODULES
CTOX_BUSINESS_OS_MCP_ALLOWED_COLLECTIONS
CTOX_BUSINESS_OS_MCP_DENY_TOOLS
```

Local admins can inspect or change these through:

```text
ctox business-os mcp policy
ctox business-os mcp policy keys
ctox business-os mcp policy set --enabled true --allow-reads true --allow-writes true --rate-limit-per-minute 120
ctox business-os mcp policy set --allow-actor chatgpt:user --allow-workspace workspace-a --allow-module customers --allow-collection customer_accounts
ctox business-os mcp policy set --audit-retention-days 90
```

Local admins can export MCP channel audit data without going through an agent:

```text
ctox business-os mcp audit --limit 100 --format jsonl --output business-os-mcp-audit.jsonl
ctox business-os mcp audit --prune
```

Treat `channel_disabled`, `permission_denied`, `rate_limited`, and
`response_too_large` as authoritative. Do not retry through shell, SQL, raw RxDB
writes, or other side channels. For `rate_limited`, wait or narrow the workflow.
For `response_too_large`, reduce limits or fetch a narrower record/context.

When receiving JSON-RPC errors, prefer `error.data.code` and
`error.data.field` over free-form message text. `error.data.code` is the stable
Business OS MCP error code.

By default, external-effect approval tools can be policy-blocked separately from
ordinary write tools.

Server-side redaction of sensitive response fields is authoritative. Do not try
to recover redacted API keys, tokens, passwords, credentials, authorization
headers, cookies, or private/access keys through alternate tools or side
channels.

## Approval Boundary

Never assume approval for:

- sending email, messages, or outreach
- changing external CRM/ticket systems
- publishing or exporting final documents
- deleting, archiving, or bulk-updating records
- starting high-cost or long-running autonomous work

For these, create or inspect an approval and wait for explicit confirmation via
the MCP approval tools.

## Response Style

When reporting CTOX data:

- summarize first
- include stable IDs
- mention command/run status
- include evidence or artifact references when available
- include a Business OS deep link when the user may need dense UI inspection

Keep outputs bounded. Do not dump entire tables or record collections.
If a tool reports `request_too_large` or `response_too_large`, narrow the query,
lower the limit, or ask for a more specific record/context.

## Implementation Reference

For building or updating the MCP server itself, follow:

```text
docs/business-os-mcp-channel-v1-implementation-plan.md
```
