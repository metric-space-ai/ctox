---
name: secret-management
description: Classify, generate, reference, and persist service credentials as open helper-driven local secret material plus shared-kernel metadata. Use when CTOX must decide whether a credential is generated, discovered, owner-supplied, or an external reference, and when generated admin access must not be forgotten.
---

# Secret Management

Use this skill when the job requires credentials, tokens, passwords, or endpoint references.

Do not use it as the full deployment skill. Pair it with `service-deployment` or another sibling skill when the broader job is service rollout.

## Operating Model

Treat this skill as:

1. credential classification
2. local secret material generation or reference capture
3. secret metadata classification
4. durable secret reference output

Preferred helper script under `scripts/`:

- `secret_material.py`

The helper is inspectable. Read or patch it when the secret shape is unusual.

## Classification Rules

For every credential-like value, decide one of:

- `generated`
- `discovered`
- `owner_supplied`
- `external_reference`

Track secret status as well:

- `present`
- `missing`
- `rotated`
- `invalid`

Never default to `owner_supplied` when CTOX can safely generate a local admin secret itself.

## Workflow

1. Name the exact credential requirement.
2. Decide whether it is local or external.
3. Generate a local secret when safe.
4. Store the secret material in a local secret file reference.
5. Store secret metadata that says:
   - kind
   - status
   - accepted reply path such as `tui_only` or `email_safe`
   - service or deployment bindings
6. Return the secret reference path and keys, not vague prose.

## Guardrails

- Do not print live secret material into owner-facing reports unless explicitly required for handoff.
- Do not forget generated admin credentials. Persist a local reference before reporting success.
- Do not ask the owner for a secret unless the value truly cannot be generated or discovered locally.
- Secret-bearing inbound mail must move to TUI; do not normalize it as regular email work.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/secret-rules.md](references/secret-rules.md)
