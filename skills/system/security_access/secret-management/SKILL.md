---
name: secret-management
description: Classify, generate, reference, and persist service credentials as open helper-driven local secret material plus shared-kernel metadata. Use when CTOX must decide whether a credential is generated, discovered, owner-supplied, or an external reference, and when generated admin access must not be forgotten.
cluster: security_access
---

# Secret Management

Only the encrypted SQLite-backed secret store and related runtime records count as durable secret knowledge. Workspace notes, flat files, or copied secret prose do not count as durable knowledge by themselves.

Use this skill when the job requires credentials, tokens, passwords, or endpoint references.

Do not use it as the full deployment skill. Pair it with `service-deployment` or another sibling skill when the broader job is service rollout.

## Operating Model

Treat this skill as:

1. credential classification
2. secret generation or owner-supplied intake
3. secret metadata classification
4. durable SQLite secret-store reference output

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
4. Store the secret material in the encrypted CTOX SQLite secret store.
5. Store secret metadata that says:
   - kind
   - status
   - accepted reply path such as `tui_only` or `email_safe`
   - service or deployment bindings
6. Return the secret handle (`scope/name`) and the relevant metadata, not vague prose.

## Primary Commands

Store a secret that did not already leak into active memory:

```sh
ctox secret put --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>'
```

Store a secret and immediately rewrite the leaked literal from active runtime memory:

```sh
ctox secret intake --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>' --db "<path-to-ctox.sqlite3>" --conversation-id "<id>" --match-text "<secret>" [--label "<human label>"]
```

Inspect metadata without exposing the value:

```sh
ctox secret list [--scope "<scope>"]
ctox secret show --scope "<scope>" --name "<name>"
```

Retrieve the raw value only for the bounded local step that truly needs it:

```sh
ctox secret get --scope "<scope>" --name "<name>"
```

## Guardrails

- Do not print live secret material into owner-facing reports unless explicitly required for handoff.
- Do not forget generated admin credentials. Persist them in the encrypted SQLite secret store before reporting success.
- Do not ask the owner for a secret unless the value truly cannot be generated or discovered locally.
- Secret-bearing inbound mail must move to TUI; do not normalize it as regular email work.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/secret-rules.md](references/secret-rules.md)
