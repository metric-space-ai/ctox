---
name: service-adapters
description: Discover and call external service tools (GitHub and future toolkits) through the chainable `ctox toolhub` CLI with credentials managed by CTOX. Use when a task needs data from or an action in a connected third-party service.
cluster: security_access
---

# Service Adapters / Dienst-Adapter (`ctox toolhub`)

**DE** · CTOX kapselt externe Dienste als Toolkits. Zugangsdaten liegen verschlüsselt im
lokalen CTOX-Secret-Store und werden vom Host eingesetzt; sie erscheinen nie in Ausgaben.
Alles läuft über die CLI: JSON auf stdout, Logs auf stderr, stabile Exit-Codes, daher
verkettbar (`| jq`, `&&`, Skripte).

**EN** · CTOX wraps external services as toolkits. Credentials live encrypted in the local
CTOX secret store and are injected by the host; they never appear in output. Everything is
CLI: JSON on stdout, logs on stderr, stable exit codes, so it chains (`| jq`, `&&`, scripts).

## Entdecken / Discover

```
ctox toolhub catalog --json     # toolkits, tools, input JSON schemas, mutating flag
ctox toolhub list --json        # connected accounts and status (no secrets)
```

## Ausführen / Execute

```
ctox toolhub exec github.list_issues --json '{"owner":"metric-space-ai","repo":"ctox"}'
echo '{"owner":"metric-space-ai","repo":"ctox"}' | ctox toolhub exec github.get_repo
ctox toolhub exec github.list_issues --json '{"owner":"o","repo":"r"}' | jq -r '.data[].title'
```

Tool names: `<toolkit>.<action>` (also accepted: `GITHUB_LIST_ISSUES`). Output envelope:
`{"outcome": "success", "data": ...}`.

| Exit | outcome | Bedeutung / Meaning |
|---|---|---|
| 0 | `success` | ok |
| 2 | `validation_error` | Argumente/Tool ungültig – korrigieren / fix arguments |
| 3 | `credential_required` | Verbindung fehlt/abgelaufen – Nutzer fragen, `connect_hint` zeigen / ask the user to connect |
| 4 | `provider_error` | Fehler beim Anbieter (`status`, `data`) / upstream error |
| 5 | `approval_required` | Schreibendes Tool: erst Freigabe einholen, dann mit `--confirm` wiederholen / mutating tool: get approval, re-run with `--confirm` |
| 1 | `error` | interner Fehler / internal error |

`credential_required` contains `toolkit`, `auth_schemes`, `reason` (de/en) and
`connect_hint`. Never try to obtain or paste tokens yourself; ask the user.

## Verbinden (Nutzer/Operator) / Connect (user/operator)

```
ctox toolhub config-set --toolkit github --id gh-pat --scheme bearer
printf %s "$TOKEN" | ctox toolhub connect --auth-config gh-pat --credential-stdin
ctox toolhub config-set --toolkit github --id gh-oauth --scheme oauth2 --client-id <id> --client-secret-stdin
ctox toolhub connect --auth-config gh-oauth     # prints redirect_url; local callback server on 127.0.0.1
ctox toolhub complete --callback '<final redirect URL>'   # headless/remote instances
```

Connections belong to the CTOX project (`--project`, default `$CTOX_PROJECT` or `default`);
the creating actor is recorded. Tokens refresh automatically in the background.
