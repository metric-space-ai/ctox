# Toolhub: Rollout und Betrieb pro Instanz (ohne SSH, ohne Gateway-Änderung)

Die Toolhub steckt im normalen CTOX-Release: Sie ist ins `ctox`-Binary
einkompiliert (Crate `ctox-toolhub`), die Skills sind eingebettet, und das
Pi-Sidecar-Bundle baut der Release-Workflow (`.github/actions/build-pi-sidecar`).
Ausgerollt wird nur über „Core anwenden“ bzw. `ctox update apply --latest`/`--version <tag>`.
Kein SSH, keine Env-Datei, keine Änderung am ctox.dev-Gateway.

## Was pro Instanz gilt

Jede Instanz hat eigene Zugangsdaten, OAuth-States, PKCE-Verifier,
DCR-Registrierungen und einen eigenen Callback. Alles liegt in ihrer eigenen
Secret-Datenbank unter ihrem eigenen Root.

- **Öffentliche URL:** wird automatisch abgeleitet. Der verwaltete MCP-Connector
  der Instanz (`ctox-business-os-mcp.service`, vom ctox.dev-Control-Plane pro
  Instanz eingerichtet) startet mit `wss://mcp.ctox.dev/connect/<host>`. Beim Start
  schreibt er `https://<host>` nach `runtime/toolhub-public-origin`, nach jedem
  Release-Wechsel also neu. Gibt es die Datei noch nicht, liest CTOX die eigene
  Unit-Datei. Request-Header werden nie ausgewertet. `CTOX_PUBLIC_BASE_URL` bleibt
  als Override für selbst gehostete Instanzen, ist aber nicht nötig.
- **OAuth-Callback:** `https://<host>/toolhub/oauth/callback`. Das Gateway
  beantwortet heute schon jeden Seitenaufruf eines angemeldeten Nutzers mit dem
  `index.html` **dieser** Instanz. Dabei reicht es Query (`state`, `code`) und die
  Capability des Nutzers durch. Die Instanz erkennt den Callback an `state` und
  `code`, schließt den eigenen Flow ab (Admin/Chef nötig) und zeigt die Ergebnisseite.
  Anonyme Aufrufe bekommen die normale Shell und verbrauchen nichts. Bei direkt
  erreichbaren, selbst gehosteten Instanzen bedient die Instanz denselben Pfad selbst.
- **Grenze:** Wer im Browser nicht bei der Instanz angemeldet ist, landet nach dem
  Anbieter auf der Login-Seite. Nach dem Login geht es auf `/`, der Code ist verloren.
  Dann einmal anmelden und „Verbinden“ erneut klicken (neuer State).

Geprüft am Gateway-Code (`ctox-dev`, `app/instance/[tenant]/[[...path]]/route.ts`,
Stand 916c317): Pfade ohne Dateiendung außerhalb von `api/`, `login/` und den
Asset-Präfixen gehen bei gültiger Sitzung an `nativeBusinessOsShellResponse`, das
`index.html` mit `request.nextUrl.search` und `Authorization: Bearer <capability>`
bei der Instanz abruft. / Verified against the gateway code: the instance receives
`GET /index.html?state=…&code=…` with the user's capability; no gateway change needed.

## Sync-Störung (#743)

„Core anwenden“ (`POST /api/business-os/ctox/update/apply`) und alle
Toolhub-Aufrufe (`/api/business-os/toolhub/*`, Callback) laufen über HTTP durch
das Gateway, nicht über WebRTC/RxDB-Replikation. #743
(`ctox_signaling_socket_not_open`) blockiert den Rollout per Update daher nicht.
Ein Rollout über Browser-Sync wäre betroffen; der wird nicht gebraucht. /
#743 does not block the update-based rollout; it only affects browser sync.

## Rollout

1. Release mit diesem Stand bauen: Tag auf main nach dem Merge von #689/#708,
   dann baut `release.yml` die Binär-Bundles. Das macht der Maintainer (Michael),
   nicht ein Agent.
2. Auf der Instanz „Core anwenden“, oder für eine bestimmte Version
   `ctox update apply --version <tag>` in der Instanz-Konsole.
3. Prüfen: `./smoke.sh https://<instanz>.ctox.dev` (Teil A).

## Smoke-Test (live, keine Mocks)

```bash
./smoke.sh https://welsch.ctox.dev                      # Teil A, ohne Login
SESSION_COOKIE='…' ./smoke.sh https://welsch.ctox.dev   # Teil B, mit Admin-Sitzung: Callback erreicht die Instanz
```

## Rollback

„Core anwenden“ auf das vorige Release (oder `ctox update rollback`). Gespeicherte
Verbindungen bleiben verschlüsselt in der Secret-Datenbank der Instanz;
`ctox toolhub mcp-disconnect --server <x>` bzw. „Trennen“ entfernt sie.
