---
name: secret-hygiene
description: Use when CTOX notices or strongly suspects that a user pasted a credential, token, password, or private key into TUI, mail, chat, or ticket context and the secret must be moved into the encrypted secret store and replaced by a stable reference.
metadata:
  short-description: Vault leaked secrets and rewrite memory references
cluster: security_access
---

# Secret Hygiene

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Only the encrypted SQLite-backed secret store and related runtime records count as durable secret knowledge. Workspace notes or copied values do not count as durable knowledge by themselves.

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
ctox secret intake --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>' --db "<path-to-ctox.sqlite3>" --conversation-id "<id>" --match-text "<secret>" [--label "<human label>"]
```

This stores the secret in the encrypted SQLite secret store and rewrites the specified conversation memory to `[secret-ref:<scope>/<name>]`.

## Fallback Commands

If the secret is already stored, rewrite memory only:

```sh
ctox secret memory-rewrite --db "<path-to-ctox.sqlite3>" --conversation-id "<id>" --scope "<scope>" --name "<name>" --match-text "<secret>" [--label "<human label>"]
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
