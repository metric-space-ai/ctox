---
name: zammad-rest
description: Use when CTOX needs direct REST access to a Zammad helpdesk for tickets, users, groups, tokens, or instance health. The skill provides inspectable helper scripts for authenticated API requests, but the agent must still read the recent operator context, choose the correct endpoint, and decide whether to use the helper, patch it, or issue raw HTTP itself.
cluster: vendor
---

# Zammad REST

This skill is for direct API work against a running Zammad instance.

Use it when you need to:
- inspect Zammad health or setup state
- read or mutate tickets, users, groups, organizations, or tokens
- provision API access for CTOX
- validate that a Zammad deployment is actually usable

Do not treat the bundled scripts as black boxes. They are open helper resources under `scripts/` and may be read, patched, bypassed, or replaced if the environment requires it.

## Contracts

Preferred helper resources:
- `scripts/zammad_request.py`
  - generic authenticated HTTP helper
  - emits raw JSON or response text
- `scripts/test_zammad_request.py`
  - helper regression tests

Load these references when needed:
- `references/api-access.md`
  - auth headers, secret expectations, common endpoints

## Inputs

Preferred secret sources:
- `ZAMMAD_BASE_URL`
- `ZAMMAD_API_TOKEN`
- optional fallback:
  - `ZAMMAD_USER`
  - `ZAMMAD_PASSWORD`

Preferred runtime secret file on the target host:
- `runtime/secrets/zammad-admin.env`

If the secret file is missing, discover whether the values already exist elsewhere before asking the owner again.

## Workflow

1. Read the recent operator context first.
2. Confirm the target Zammad base URL and auth material.
3. Prefer `scripts/zammad_request.py` for routine requests.
4. If the helper does not fit the case, patch it or use raw `curl`.
5. Persist any newly created durable access material in the existing secret location or an equivalent protected secret reference.
6. Verify the intended endpoint actually worked before reporting success.

## Output Rules

When reporting back:
- say what was checked or changed
- cite the concrete endpoint or object
- separate `proposed`, `prepared`, `executed`, and `blocked`
- do not claim API readiness until an authenticated request succeeded

## Guardrails

- Never log or echo secrets into operator-facing output.
- Never create or rotate tokens without persisting a secret reference.
- Never answer from memory alone when the live API can be queried.
- If setup is incomplete, state exactly which Zammad readiness step is still open.
