# CTOX Business OS MCP Skill

External agent skill for using CTOX Business OS through the Business OS MCP
Channel.

This directory is intentionally located under top-level `skills/` instead of
`src/` or `integrations/agents/` because it is meant to be copied, vendored, or
published as a standalone GitHub-hosted skill for other agent runtimes.

## Contents

- `SKILL.md` - instructions for MCP-capable agents that should interact with
  CTOX Business OS.
- `scripts/validate-skill-contract.mjs` - validates that the skill documents
  the actual Rust MCP tool descriptors.
- `test/skill-contract.test.mjs` - contract tests for the skill text and tool
  surface.

## Install Shape

For an external agent runtime, install or vendor this directory as the
`ctox-business-os-mcp` skill and configure the runtime with a CTOX Business OS
MCP endpoint:

- local developer endpoint exposed through an HTTPS tunnel
- managed endpoint through `https://mcp.ctox.dev/mcp/<instance-id>`
- self-hosted customer endpoint

For the Example instance in Codex:

```bash
cp -R skills/ctox-business-os-mcp ~/.codex/skills/ctox-business-os-mcp

codex mcp add cto1-example-business-os \
  --url https://mcp.ctox.dev/mcp/cto1.example.com \
  --bearer-token-env-var CTOX_BUSINESS_OS_MCP_TOKEN
```

The skill does not provide CTOX access by itself. Access comes only through a
configured MCP server and the server-side Business OS MCP policy.

For local/same-host agents, copy the MCP configuration from Business OS
Settings -> MCP or the admin control-plane route
`/api/business-os/mcp/connect-info`. That payload includes the local
`http://127.0.0.1:8788/mcp` endpoint, the local inbound bearer token, and
Codex/Claude config snippets. Managed `mcp.ctox.dev` clients need a separate
managed MCP client token from ctox.dev/Web Auth; the local bearer token is not a
managed gateway token.

If a user provides a Business OS or ctox.dev host plus email/password, use those
credentials only as transient web-login credentials for setup. The `/ctox`
deploy skill can run `ctox/scripts/connect-business-os-mcp.mjs
--password-stdin` to authenticate, discover the tenant through
`/api/desktop/session-package`, call `/api/instances/<tenant-id>/managed-mcp`,
and rotate a one-time Agent Token. Without that bootstrap helper, open
`https://ctox.dev/dashboard?tenant=<tenant-id>#mcp`, then use **MCP**,
**Token rotieren**, and **Neuer Token** to copy the token. Direct Business OS
hosts use `/login` plus `/api/business-os/mcp/connect-info`.

MCP policy is only the channel gate. Remote agents still follow Business OS
roles and app/data grants: `Owner`/`chef`, `Admin`/`admin`,
`App-Verantwortliche:r`/`founder`, and `Teammitglied`/`user`; private
`0.x.y` apps require app visibility grants, `1.0.0+` apps are team-visible by
default unless restricted, and data reads/writes remain explicit.

App development goes through typed tools. Use `business_os.create_app` and
`business_os.modify_app`; their response includes `command_id`, `task_id`,
`app_directory`, and a `development_contract` with the runtime-installed app
source root, required files, `business-os-app-module-development` resources,
and validation/smoke/E2E commands. Agents must poll
`business_os.get_command_status` rather than inventing raw shell, SQL, or RxDB
write paths.

## Validation

From the repository root:

```bash
cd skills/ctox-business-os-mcp
node --test test/*.test.mjs
node scripts/validate-skill-contract.mjs
```
