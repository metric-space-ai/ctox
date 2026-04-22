---
name: ticket-access-and-secrets
description: Use when CTOX needs credentials, access rights, or approval boundaries for ticket work and must handle them through encrypted local storage plus explicit access requests.
metadata:
  short-description: Request rights and store secrets safely for ticket work
cluster: ticket_integration
---

# Ticket Access And Secrets

Use this skill when ticket handling is blocked on missing credentials, missing permissions, or unclear approval boundaries.

## Core Rules

SQLite in CTOX may hold encrypted secret values through the dedicated secret store.

Ticket work and ticket knowledge may only hold references, scopes, channels, and rationale. They must never hold raw secret values.

The secret store and related runtime records are the durable source of truth. Workspace notes or copied credential instructions do not count as durable access knowledge by themselves.

Use the SQLite-backed secret store and runtime store as the only durable authority for access state.

## Commands

Inspect available secret metadata:

```sh
ctox secret list [--scope "<scope>"]
ctox secret show --scope "<scope>" --name "<name>"
```

Store a secret locally:

```sh
ctox secret put --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>'
```

Store a secret locally and immediately rewrite leaked memory references in one step:

```sh
ctox secret intake --scope "<scope>" --name "<name>" --value "<secret>" --description "<text>" --metadata-json '<json>' --db "<path-to-ctox.sqlite3>" --conversation-id "<id>" --match-text "<secret>" [--label "<human label>"]
```

Retrieve a secret only for explicit local execution:

```sh
ctox secret get --scope "<scope>" --name "<name>"
```

If a secret already leaked into the CTOX conversation memory, rewrite the LCM/continuity history to a stable keychain handle after storing the secret:

```sh
ctox secret memory-rewrite --db "<path-to-ctox.sqlite3>" --conversation-id "<id>" --scope "<scope>" --name "<name>" --match-text "<secret>" [--label "<human label>"]
```

Create an operator-visible access request in the ticket surface:

```sh
ctox ticket access-request-put --system "<system>" --title "<title>" --body "<text>" --required-scopes "<csv>" --secret-refs "<csv>" --channels "mail,jami" --publish
```

## Operating Pattern

1. Check whether the required secret or access grant already exists locally.
2. If not, create an explicit access request with scopes, secret references, and the preferred contact channels.
3. Once the operator supplies the secret, store it only through `ctox secret intake` or `ctox secret put`.
4. If the secret already entered CTOX memory, prefer `ctox secret intake` so storage and memory rewrite happen as one visible operation.
5. Refer back to the secret by scope and name in follow-up ticket work; do not paste the value into tickets or knowledge entries.

## Important Boundaries

- Do not store raw secrets in ticket metadata.
- Do not request broad access when a narrower scope is sufficient.
- Do not treat operator silence as implied approval.
