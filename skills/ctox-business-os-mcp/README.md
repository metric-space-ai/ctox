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

For the Kunstmen instance in Codex:

```bash
cp -R skills/ctox-business-os-mcp ~/.codex/skills/ctox-business-os-mcp

codex mcp add cto1-kunstmen-business-os \
  --url https://mcp.ctox.dev/mcp/cto1.kunstmen.com \
  --bearer-token-env-var CTOX_BUSINESS_OS_MCP_TOKEN
```

The skill does not provide CTOX access by itself. Access comes only through a
configured MCP server and the server-side Business OS MCP policy.

## Validation

From the repository root:

```bash
cd skills/ctox-business-os-mcp
node --test test/*.test.mjs
node scripts/validate-skill-contract.mjs
```
