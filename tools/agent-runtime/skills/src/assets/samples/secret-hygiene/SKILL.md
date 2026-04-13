---
name: secret-hygiene
description: Use when CTOX notices or strongly suspects that a user pasted a credential, token, password, or private key into TUI, mail, chat, or ticket context and the secret must be moved into the encrypted secret store and replaced by a stable reference.
metadata:
  short-description: Vault leaked secrets and rewrite memory references
---

# Secret Hygiene

Use this skill whenever a raw secret appears in active CTOX context.

This skill does not decide by kernel heuristic that some string is a secret. The skill makes that judgment from context, then uses explicit kernel primitives to protect the value.

## Core Rules

- Do not leave raw secrets in ordinary follow-up messages, ticket notes, or knowledge entries.
- Store the value in the encrypted CTOX secret store.
- Rewrite conversation memory to a stable reference handle when the raw literal already entered LCM/continuity.
- Keep the replacement handle human-readable enough for operators to understand what happened.

## Primary Command

Prefer the one-step intake path:

```sh
ctox secret intake --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>' --db "<path-to-ctox_lcm.db>" --conversation-id "<id>" --match-text "<secret>" [--label "<human label>"]
```

This stores the secret in the encrypted SQLite secret store and rewrites the specified conversation memory to `[secret-ref:<scope>/<name>]`.

## Fallback Commands

If the secret is already stored, rewrite memory only:

```sh
ctox secret memory-rewrite --db "<path-to-ctox_lcm.db>" --conversation-id "<id>" --scope "<scope>" --name "<name>" --match-text "<secret>" [--label "<human label>"]
```

If the secret was supplied outside the current conversation and no rewrite is needed:

```sh
ctox secret put --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>'
```

## Operating Pattern

1. Decide whether the pasted value is actually a secret that needs protection.
2. Choose a stable scope and name.
3. Store it through `ctox secret intake` when the current conversation already contains the literal.
4. Confirm that future work refers only to the secret handle.
5. Use `ctox secret get` only for bounded local execution steps that truly require the raw value.

## Boundaries

- Do not invent fake secret rotations or claim revocation if none happened.
- Do not silently rewrite unrelated text.
- Do not expose the raw secret again after the rewrite step.
