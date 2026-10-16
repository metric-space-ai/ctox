# ctox-toolhub

**DE** · Dienst-Adapter für CTOX (Toolkits, verwaltete Anmeldung, verbundene Konten),
konzeptionell an Composio angelehnt (MIT, siehe `LICENSE.upstream`), aber vollständig
selbst gehostet: Tokens liegen verschlüsselt im CTOX-Secret-Store (`src/core/secrets.rs`),
Tool-Aufrufe führt der CTOX-Host aus. Workjet und Modelle sehen nie Zugangsdaten.

**EN** · Service adapters for CTOX (toolkits, managed auth, connected accounts),
modelled after Composio's concepts (MIT, see `LICENSE.upstream`) but fully self-hosted:
tokens are stored encrypted in the CTOX secret store, tool calls are executed by the CTOX
host. Workjet and models never see credentials.

## Konzepte / Concepts

| Composio | ctox-toolhub |
|---|---|
| Toolkit | `ToolkitSpec` + `toolkits::ToolkitAdapter` |
| Tool / Action | `ToolSpec` (JSON Schema, `mutating`) |
| Auth scheme | `AuthScheme::{OAuth2{pkce}, ApiKey, Basic, Bearer}` |
| Auth config | `AuthConfig` (client secret only in the secret store) |
| Connected account | `ConnectedAccount` (`INITIALIZING/ACTIVE/EXPIRED/FAILED`) + `AccountSecret` |
| Connection request | `ConnectionRequest` |

## Speicherlayout / Storage layout

- `toolhub:<toolkit>:<account>` / `token` – value: token JSON; metadata: account (status, scopes, `expires_at`, `next_refresh_after`)
- `toolhub:<toolkit>:<account>` / `pending` – state + PKCE verifier (single use, 15 min TTL)
- `toolhub-config:<toolkit>` / `<auth_config_id>` – value: OAuth client secret; metadata: auth config

## OAuth

**DE** · Wie im cliproxyapi-Port läuft OAuth direkt auf der CTOX-Instanz: lokaler
Callback-Server `http://127.0.0.1:<port>/toolhub/oauth/callback`
(`CTOX_TOOLHUB_LOOPBACK_PORT`, Standard 8765), PKCE (S256), Tokens nur in `secrets.rs`.
Es gibt keinen zentralen OAuth-Dienst. Headless/entfernt: URL in einem beliebigen Browser
öffnen, danach die finale Redirect-URL (auch wenn die Seite nicht lädt) mit
`ctox toolhub complete --callback '<url>'` einfügen.

**EN** · Like the cliproxyapi port, OAuth runs on the CTOX instance itself: local callback
server, PKCE (S256), tokens only in `secrets.rs`; no central OAuth service. Headless/remote:
open the URL in any browser, then paste the final redirect URL (even if the page fails to
load) into `ctox toolhub complete --callback '<url>'`.

Refresh: `next_refresh_after = expires_at - 300s`; on HTTP 401 one refresh + one replay,
otherwise the account becomes `EXPIRED` and the tool returns `credential_required`.

## CLI

```
ctox toolhub catalog
ctox toolhub config-set --toolkit github --id gh-pat --scheme bearer
printf %s "$GITHUB_PAT" | ctox toolhub connect --auth-config gh-pat --owner <actor> --credential-stdin
ctox toolhub config-set --toolkit github --id gh-oauth --scheme oauth2 --client-id <id> --client-secret-stdin
ctox toolhub connect --auth-config gh-oauth --owner <actor> [--wait-loopback 300]
ctox toolhub complete --callback '<redirect url>'
ctox toolhub exec --owner <actor> --tool GITHUB_LIST_ISSUES --args '{"owner":"metric-space-ai","repo":"ctox"}'
ctox toolhub list | status | refresh | disconnect --account <id> | refresh-due
```
