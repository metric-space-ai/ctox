# browser — Fachlogik als JS-Handler (Strang H7, Shell v4)

Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/rxdb_peer_browser.rs`
(Op-Dispatch `:1652-1717`, Controller-Lease `:1222-1309`, Lease-Gate `:1316-1339`, Laufzeit-Stopp `:937`,
auth-assist `:1330`); Analyse E (`E-befehlstypen.md` Z. 144–163), C1 §browser. Vertrag: §9.4, §16.13, §16.16.

Trennung (E): 20 Typen **FAEHIGKEIT** (eingebettete Browser-Engine). Die eigentliche Fachlogik ist schmal:
(a) die Controller-Lease-Buchhaltung auf `browser_sessions` (acquire/renew/release) und (b) das Lease-Gate,
das jede Engine-Operation vor dem Aufruf bestehen muss. Alles andere (Navigation, Tabs, Zwischenablage,
Dialog-/Rechteantwort, Credential-Fill, Frame-Pump, Sitzungsstopp) ist `ctx.cap.browser.*`.

## 1. Inventar der 20 Befehlstypen

| # | Befehlstyp | Rust-Ort | Ziel | Stand |
|---|---|---|---|---|
| 1 | `browser.navigate` | `:1655` | Lease-Gate + `cap.browser.op('navigate')` | portiert |
| 2 | `browser.session.start` | `:1655` | `cap.browser.op('navigate')` (ohne Gate) | portiert |
| 3 | `browser.reset` | `:1655` | Lease-Gate + `cap.browser.op('navigate')` | portiert |
| 4 | `browser.reload` | `:1658` | Lease-Gate + `cap.browser.op('reload')` | portiert |
| 5 | `browser.back` | `:1659` | Lease-Gate + `cap.browser.op('back')` | portiert |
| 6 | `browser.forward` | `:1660` | Lease-Gate + `cap.browser.op('forward')` | portiert |
| 7 | `browser.tab.open` | `:1665` | Lease-Gate + `cap.browser.op('tab_open')` | portiert |
| 8 | `browser.tab.activate` | `:1669` | Lease-Gate + `cap.browser.op('tab_activate')` | portiert |
| 9 | `browser.tab.close` | `:1670` | Lease-Gate + `cap.browser.op('tab_close')` | portiert |
| 10 | `browser.dialog.respond` | `:1674` | Lease-Gate + `cap.browser.op('dialog_respond')` | portiert |
| 11 | `browser.permission.respond` | `:1681` | Lease-Gate + `cap.browser.op('permission_respond')` | portiert |
| 12 | `browser.clipboard.copy` | `:1693` | Lease-Gate + `cap.browser.op('clipboard_copy')` | portiert |
| 13 | `browser.clipboard.paste` | `:1694` | Lease-Gate + `cap.browser.op('clipboard_paste')` | portiert |
| 14 | `browser.clipboard.clear` | `:1698` | Lease-Gate + `cap.browser.op('clipboard_clear')` | portiert |
| 15 | `browser.credential.fill` | `:1703` | Lease-Gate + `cap.browser.op('credential_fill')` | portiert |
| 16 | `browser.controller.acquire` | `:1222` | Handler (FACHLOGIK, kein Engine-Call) | portiert |
| 17 | `browser.controller.renew` | `:1222` | Handler (FACHLOGIK) | portiert |
| 18 | `browser.controller.release` | `:1222` | Handler (FACHLOGIK) | portiert |
| 19 | `browser.session.stop` | `:937` | `cap.browser.session_stop` | portiert |
| 20 | `web_stack.auth_assist.complete` | `:1330` | Handler-Gate + `cap.browser.auth_assist_complete` | portiert |

Alle 20 portiert; 0 legacy. 16 node-Tests.

## 2. Treue-Notizen
- Op-Übersetzung (`_lib/browser.js::engineOp`) 1:1 zum `match` `:1652-1717`, inkl. `timeoutMs:30000`,
  `tabId`-Durchreichung, `dialog_respond {accept,value}`, `permission_respond {accept}`.
- Ziel-URL: explizite `payload.url` (getrimmt), sonst zuletzt bekannte Tab-/Sitzungs-URL, sonst
  `https://example.com` (`:1210-1219`).
- Lease-Gate (`assertLeaseHolder`): für alles außer `session.start` und die `controller.*`-Arme —
  `controller_user_id == owner && requested lease == controller_lease_id && expires_at > now` sonst `forbidden`
  (`:1316-1339`).
- Controller: Lease-Fenster fest 120 000 ms; `acquire` nimmt die Lease, `renew` erneuert sie (erfordert
  passende aktuelle Lease, sonst `conflict`), `release` leert `controller_user_id/lease_id` und setzt
  `expires=0`. Fehlende Sitzung → `not_found`; fehlende `lease_id` → `invalid`.
- Zwischenablage: der kopierte Text bleibt im Sitzungsspeicher der Capability und wird bewusst **nicht** nach
  RxDB projiziert (kann eine Zugangskennung sein). `paste` holt den Wert in der Capability, nicht im Handler.
- `credential.fill`: der Handler reicht nur die Credential-Referenz (`credential_id`) + Selektor/Feldrolle
  durch; das Geheimnis löst die Capability aus dem Credential-Store auf — der Handler sieht es nie.
- `web_stack.auth_assist.complete` verlangt `payload.confirmed === true` (`:1333`).

## 3. Capability-Vertragsvorschlag — `ctx.cap.browser`

### `op(req) → nav_state`
- Deckt heute: `manager.request(&session, op, op_params)` (`:1719`) für die 15 Engine-Ops.
- `req`: `{ session_id, tab_id:string|null, op:'navigate'|'reload'|'back'|'forward'|'tab_open'|'tab_activate'
  |'tab_close'|'dialog_respond'|'permission_respond'|'clipboard_copy'|'clipboard_paste'|'clipboard_clear'
  |'credential_fill', params:object }`
- `nav_state`: der Navigationszustand (URL, Tab, Frame-Seq, Status), den die Engine zurückmeldet und der
  nach `browser_sessions`/`browser_tabs` projiziert wird (Capability-Seite).
- `credential_fill`-`params`: `{ credential_ref, selector, field_role }` — die Capability löst Benutzer/Passwort auf.
- Fehler: `not_found` (Sitzung/Tab), `forbidden`, `unavailable`, `conflict`.
- Beispiel: `await ctx.cap.browser.op({ session_id, tab_id:null, op:'navigate', params:{ url, timeoutMs:30000 } })`.

### `session_stop(req) → { ok }`
- Deckt heute: Laufzeit-Stopp (`:937`). `req`: `{ session_id }`.

### `auth_assist_complete(req) → { ok }`
- Deckt heute: `web_stack.auth_assist.complete`-Laufzeitpfad (`:1330-…`). `req`: `{ session_id }` (nach dem
  Confirmation-Gate des Handlers).

Hinweis: Die Controller-Lease liegt **nicht** in der Capability, sondern als Datenpflege in den drei
`controller.*`-Handlern; die Capability vertraut dem Lease-Gate des Handlers (wie heute der Peer dem
Session-Dokument vertraut).

## 4. Dateien
- `handlers/_lib/browser.js`, `handlers/_lib/run.js`
- 20 × Handler (`browser.*.js`, `web_stack.auth_assist.complete.js`)
- `handlers/tests/_ctx.mjs`, `handlers/tests/browser.test.mjs`

## Skills und Agenten-Übergabe (Strang SKL-A4, Kern §16.30)

- `index.js` `sendBrowserContextToCtox` schickte `required_skills: ['browser-context', ('web-stack',) 'ctox']` mit dem
  Befehl `ctox.browser_context.capture`. Keiner der drei Namen existiert als Skill: weder unter `src/skills/**` (System-
  Skills des Core) noch im App-Paket noch in der Git-Historie (`git log --all -- '*/web-stack/SKILL.md' …` leer); der Core
  hat sie nur als Text in den Queue-Prompt geschrieben (`store.rs:27070-27083`). Ergebnis: Feld entfernt,
  `check-skills.mjs` meldet für browser 0 Phantome (Baseline −3). Die System-Skill-Liste der Simulation bleibt
  unverändert (nichts davon ist ein generischer Core-Skill).
- Auslöser bleibt `ctox.browser_context.capture` (V4-PORT.md: Core-generische Übergabe an die CTOX-Queue). Er sendet kein
  `business_os.chat.task` (Bestandsaufnahme F: browser hat keinen Agenten-Auslöser), daher in diesem Strang kein Umbau auf
  eigenen Handler mit enqueue + Callback. Offen für den Kern-Abbau: der Typ läuft weiterhin über die Core-Queue-Route
  (`store_projections.rs:895` hängt `browser_context_artifact` an), die Diagnose liest `ctox_queue_tasks`
  (`latestBrowserHandoffTasks`). Ein Umbau bräuchte eine Festlegung, wohin das Agentenergebnis in browser-Collections geht.
