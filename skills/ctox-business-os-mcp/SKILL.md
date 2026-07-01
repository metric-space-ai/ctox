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

## MCP Configuration And Auth

The skill does not grant access by itself. The agent runtime must have a CTOX
Business OS MCP server configured before the tools are available.

For a local/same-host agent, an admin can open Business OS Settings -> MCP or
read the admin-only control-plane route:

```text
GET /api/business-os/mcp/connect-info
```

That response contains the local endpoint, the local inbound bearer token, and
ready-to-copy Codex/Claude MCP server snippets. The local bearer token is the
CTOX secret-store value `business_os/mcp_inbound_auth_token`; it is valid for
`http://127.0.0.1:8788/mcp` or an operator-managed tunnel to that local MCP
server. Do not use that local token as a managed `mcp.ctox.dev` client token.

For a managed remote agent, configure the agent client with:

```json
{
  "mcpServers": {
    "<instance>-business-os": {
      "url": "https://mcp.ctox.dev/mcp/<instance-id>",
      "headers": {
        "Authorization": "Bearer <managed MCP client token>"
      }
    }
  }
}
```

The CTOX instance must also connect outbound to the managed gateway with the
instance connect token issued by ctox.dev/Web Auth:

```text
ctox business-os mcp connect --url wss://mcp.ctox.dev/connect/<instance-id>
```

If the managed endpoint returns `runtime_unavailable`, the agent is configured
but the CTOX instance is not currently connected. Report that state instead of
trying shell, SQL, browser-control, or raw HTTP fallbacks.

## Web-Login Bootstrap

If the user supplies a Business OS or ctox.dev host plus email/password and
asks to connect an agent, do not stop at "need a bearer token". Treat those
credentials as transient web-login credentials for MCP setup, not as the MCP
credential itself.

Rules:

- Never repeat, log, store, or put the password in command arguments.
- Prefer the `/ctox` deploy skill bootstrap script when it is available:
  `ctox/scripts/connect-business-os-mcp.mjs --password-stdin`.
- For ctox.dev managed tenants, authenticate to `https://ctox.dev`, read
  `/api/desktop/session-package`, select the matching tenant, then use
  `/api/instances/<tenant-id>/managed-mcp` to enable Managed MCP and rotate a
  one-time Agent Token when the authenticated actor is Owner/Admin.
- For direct Business OS hosts, authenticate with `/login`, then call the
  admin-only `/api/business-os/mcp/connect-info` route.
- If the actor lacks rights or the endpoint is not present, open the exact
  dashboard MCP location instead of sending the user hunting:
  `https://ctox.dev/dashboard?tenant=<tenant-id>#mcp`. In that panel the user
  must open **MCP**, enable Managed MCP, press **Token rotieren**, and copy the
  one-time token shown under **Neuer Token**.

Email/password can therefore start the connection flow. The final configured
agent still uses the supported MCP bearer token and typed MCP endpoint. Do not
drive the Browser Business OS shell as an API, and do not create an HTTP data
path for Business OS records.

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
business_os.list_app_files
business_os.read_app_file
business_os.search_app_source
```

Action tools:

```text
business_os.list_module_actions
business_os.propose_action
business_os.create_app
business_os.modify_app
business_os.prepare_app_source
business_os.write_app_file
business_os.validate_app
business_os.smoke_app
business_os.e2e_app
business_os.execute_action
business_os.approve
business_os.reject
business_os.request_changes
business_os.get_command_status
```

If the server exposes fewer tools, use only the advertised tools.

## Business OS App Development Via MCP

Use the app-source tools when the connected coding agent should build or edit
the app itself. These are typed, policy-gated, app-scoped source tools; they do
not expose generic shell commands, SQL, raw RxDB writes, browser remote control,
or arbitrary filesystem access.

Direct coding flow:

1. Call `business_os.status` and confirm the expected actor, workspace, and
   policy.
2. Call `business_os.list_modules` so you know the current app catalog.
3. For a new runtime-installed app source workspace, call
   `business_os.prepare_app_source` with `module_id`, `title`, `description`,
   `category`, and `instruction`. For an existing app, call
   `business_os.get_module`.
4. Call `business_os.list_app_files`, `business_os.read_app_file`, and
   `business_os.search_app_source` to inspect the current source.
5. Write complete source files with `business_os.write_app_file`. Browser ESM
   dependencies must be checked in as relative `.mjs` files under the app
   source root, for example `vendor/<name>.mjs` or `lib/<name>.mjs`, then
   imported relatively. Do not run npm, fetch packages through shell, or add a
   package-manager bridge.
6. Run `business_os.validate_app`. For UI-critical changes, run
   `business_os.smoke_app` and, when persistence/command-bus behavior matters,
   `business_os.e2e_app`.
7. Use `business_os.open_link` for a Business OS deep link after the app is
   visible in the catalog.

Delegated app-work flow:

Use `business_os.create_app` and `business_os.modify_app` when the user wants
CTOX to enqueue app work for a CTOX worker instead of having the connected
agent edit files directly. Those tools return `command_id`, `task_id`, and the
same `development_contract`; poll `business_os.get_command_status` until the
command reaches a terminal state. Do not claim the app is changed until the
command is terminal or the source tools have written and validated the app.

The canonical runtime-installed app source root is:

```text
runtime/business-os/installed-modules/<module_id>
```

The canonical files under that root are:

```text
module.json
collections.schema.json
schema.js
index.html
index.css
index.js
icon.svg
core/records.mjs
core/automation.mjs
lib/*.mjs
vendor/*.mjs
locales/en.json
locales/de.json
tests/*.test.mjs
```

The coding agent must use the `business-os-app-module-development` skill and
the resource files listed in `development_contract.skill_resources`. It must
validate with `business_os.validate_app` or the returned validation command,
normally:

```text
ctox business-os app validate <module_id> --installed
```

If the MCP response does not include the `development_contract`, or the
`business-os-app-module-development` skill/resources are unavailable, stop and
report the missing contract. Do not improvise a raw web app, copy a reference
app shell, or treat browser automation as an app-authoring substitute.

For normal business applications, default to the standard Business OS app
shape unless the user explicitly asks for a shell/developer control surface:

- `module.json` must set `layout.shell` to `full-workspace`.
- Do not leave the app framed by generic shell side panes such as `Kontext` and
  `Themen`, and do not duplicate empty left/right columns inside the app.
- Use Business OS theme tokens for surfaces, text, borders, and controls so the
  app works in light and dark theme. Do not force `color-scheme` or hard-code a
  dark-only/light-only root palette.
- Browser ESM dependencies must be checked in as relative `.mjs` files under
  the app source root and imported relatively.

When the app finalizes, CTOX records a runtime module version and refreshes the
native Business OS RxDB peer when schemas changed. Module records and app data
still replicate through CTOX DB/WebRTC; MCP remains the control channel only.

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
