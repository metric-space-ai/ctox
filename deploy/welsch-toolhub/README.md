# Toolhub auf einer CTOX-Instanz / Toolhub on a CTOX instance

**DE** — Die Toolhub ist Teil des `ctox`-Programms jeder Instanz. Workjet und die
Business-OS-Shell sprechen sie ausschließlich über ctox sync an (WebRTC/RxDB):
Befehle `ctox.toolhub.request`, `ctox.toolhub.credential.submit` und
`ctox.toolhub.oauth.complete` in der replizierten Sammlung `business_commands`.
Die Instanz führt sie aus und schreibt das Ergebnis in dasselbe Dokument zurück.
Es gibt keine HTTP-Route, keinen Gateway-Callback und kein SSH. Nur die Instanz
selbst ruft Anbieter-APIs und Token-Endpunkte per HTTPS auf.

- Geheimnisse (`secret`, `callback_url`) werden beim Eintreffen aus dem
  Dokument gelöst, bevor etwas gespeichert oder weiter repliziert wird, und
  landen nur im verschlüsselten Secret-Store der Instanz.
- OAuth wie bei „LLM einbinden“: Redirect auf `http://127.0.0.1:<port>/toolhub/oauth/callback`.
  Workjet fängt die Weiterleitung im In-App-Fenster ab und schickt sie per Sync.
  State und PKCE-Verifier liegen nur auf der Instanz.
- Rollout: normales Release + „Core anwenden“. Shell und Core müssen beide neu sein.

**EN** — The toolhub ships inside `ctox`. Clients reach it only via ctox sync
commands in `business_commands`; no HTTP route, no gateway callback, no SSH.
Secrets are detached before persistence. OAuth uses a loopback redirect that
Workjet captures and submits over sync. Roll out via release + "Core anwenden".
