# WJB-D – Grok, Provider, Rechner, Sitzung, Gerät, Hostbrücke (Strangbericht)

Stand 16.10.2026, 05:30, Klon `~/Documents/ctox-shell-v4` (Branch shell-v4/prototype, Basis ee0ac39fb + Arbeitsbaum).
V2-Quelle: Pin `e35a116ae` (app.js = `/Volumes/tmp/app-main.js`, cmp identisch; shared/*.mjs, mobile-host.js per `git show`).
Workjet-Abgleich: `/Volumes/tmp/wj-measure` eb65947d (`CtoxGuestManager.ts`, `packages/contracts/src/*`). Kein git, nichts committet.

## Ergebnis je Aktion (25 von 27 freigeschaltet, 2 unsupported)

| Aktion | Stand | Orakel / Grund |
|---|---|---|
| instance.grok.read, .start, .poll, .cancel, .check, .remove | freigeschaltet | `workjet-grok-native.test.mjs` (4/4 V2-Proben) + Stub-Probe |
| instance.providers.read, .adopt, .observe, .models.select, .models.exclude, .account.enable, .account.remove | freigeschaltet | `workjet-provider-native.test.mjs` (14/15, 1 Quelltext-Probe) + Stub-Probe |
| instance.providers.secret | unsupported | V2 kennt die Aktion nicht (TypeError „Invalid native provider metadata…“); Workjets eigener Vertrag verbietet sie (`workjetNativeProviders.test.ts:164`), Workjet sendet sie nie |
| instance.providers.models.check | unsupported | V2-Shell am Pin hat keinen Rumpf (Workjet sendet sie, `workjetNativeProviders.ts:136`); Kern-Befehl unbekannt → Kern/V2 muss erst liefern |
| computer.list, .assign, .unassign, .endpoint.upsert, .endpoint.disable, .ssh_key.ensure | freigeschaltet | `workjet-computer-control.test.mjs` (10/12, 2 Quelltext-Proben) + 2 Stub-Proben |
| session.list, .create, .transfer.start, .transfer.status, .transfer.abort, .transfer.pause_ack | freigeschaltet (vollständig) | `workjet-session-control.test.mjs` (1/3, 2 Quelltext-Proben; die eine Verhaltensprobe deckt alle 6 Aktionen ab) + Stub-Probe |
| workjetSessionEvents (register/unregister/snapshot) | freigeschaltet | `workjet-session-events.test.mjs` (3/4, 1 Quelltext-Probe) + Stub-Probe über kernel.query |
| binding.list, binding.revoke, invite.create, invite.revoke | freigeschaltet | kein V2-Verhaltenstest vorhanden → Differenz-Orakel gegen den V2-Rumpf app.js:7844-7877 (4 Nativ-Varianten × 7 Anfragen = 28 Vergleiche) + Stub-Probe |
| Port mobile-host.js → `host-bridge.js` | portiert, braucht 2 Rahmen-Haken + 1 Kern-Haken | Differenz-Orakel gegen V2 mobile-host.js (20 Host-Befehlsschritte, identische Nachrichten/Attribute) |

Globals nach `installWorkjetBridges`: alle fünf (`workjetProjectControl`, `workjetComputerControl` mit
`supportsOperationalDetails`, `workjetSessionControl`, `workjetSessionEvents`, `workjetBusinessOsDeviceControl`).
Sitzungssteuerung + Ereignisstrom hängen nur ein, wenn alle 6 Sitzungsaktionen der Inventarliste freigeschaltet sind
(Probe „Sitzung nur vollständig“).

## „Nicht unterstützt“ je Brücke (Kern-Route fehlt → unknown_command)

- Grok/Provider (workjetProjectControl): die V2-Rümpfe fangen jeden Transportfehler ab („Grok control unavailable…“,
  PROVIDER_CONTROL_UNAVAILABLE → Workjet `core_outdated`). Die Weiche in instance.js merkt sich einen unsupported-Fehler
  des Transports und wirft danach `unsupportedError('project', action)` → Workjet-Regex → unsupported. Andere
  Kernfehler bleiben V2-Text (Probe: `forbidden` → „Grok control unavailable.“, nicht unsupported).
- Rechner: Nachricht exakt `workjet_computers collection is not registered.` (computer.js-Hülle). Fachfehler bleiben
  „Workjet computer command did not complete successfully.“ (command_failed).
- Gerät: `code = 'CTOX_WEBRTC_CAPABILITY_MISSING'` (device.js-Hülle).
- Sitzung: nur „Global fehlt“. Fehlt dem Kern eine ctox.workjet.session.*-Route, sieht Workjet „fehlgeschlagen“;
  besser geht es mit der Workjet-Seite nicht (kein try/catch dort). Abhängigkeit an den Kern: alle sechs Routen.

## Ersetzungen über Regel 2 hinaus (nötig, alle mit Kommentar am Ort)

- Hilfen mit V2-`state` bekommen `env` als ersten Parameter (requireWorkjet…DataPlane, listProjected…, waitForProjected…).
- `waitForSyncBridgeReady(b, ms)` → `waitForSyncBridgeReady(env, b, ms)` (V2-Hilfe app.js:15458 liegt außerhalb der
  Abschnitte; lokale wörtliche Kopie in computer.js, env darf sie ersetzen – das Ereignis-Orakel zählt ihre Aufrufe).
- `window.setTimeout` → `setTimeout`; `globalThis.workjetHostBridge` → `hostBridge(env)` (Orakel-vm-Realm ≠ Modul-Realm;
  Produktion: `target.workjetHostBridge`).
- V2-Hilfe „Datenebene bereit“ → `kernelReady(env)` (v4: Kernel beim Einhängen bereit; Wächter-Begriff vermieden).
- Grok-Modul: `const actions` → `GROK_ACTIONS` (beide V2-Module in einer Datei).
- Sitzung: `dispatchWorkjetSessionCommand`/`readTerminalCommandOutcome` heißen im Rumpf …V2; gleichnamige Hüllen merken
  die Kern-Quittung und lesen das Enddokument unter `kernel_command_id` (K-WJ-4), Kernstatus done → completed;
  fehlt das Dokument lokal, gilt der gepushte Endstatus der Quittung.
- Provider: `record_id: null` wird nicht gesendet (v2-compat machte daraus `'null'`), und nur Quittungen mit
  `kernel_command_id` bekommen `transport: 'rxdb-command-bus'` (kernel.command ist die RxDB/WebRTC-Befehlsebene).
- Hostbrücke: Seiteneffekt → `installWorkjetHostBridge({target, doc, shell, catalog})` mit `uninstall()`;
  Fenster-App aus `data-window-app` bzw. `v4app:<app>#n`; Fenster abgelöster Rahmen (`[data-v4-retiring]`) zählen nicht;
  `location.hash = appId` → `shell().navigate(appId)` / `shell().showDesktop()`; Katalog über `catalog()` statt fetch.

## Gehört nach v2-helpers / v2-compat (lokal angelegt)

- `waitForSyncBridgeReady` (computer.js, wörtlich app.js:15458-15474) → v2-helpers (A und B nutzen es ebenfalls als V2-Global).
- v2-compat `dispatch`: `record_id: null` nicht als `'null'` senden; Quittung mit `transport: 'rxdb-command-bus'`
  (instance.js `kernelProviderDispatch`).
- v2-compat `db.collection('business_commands').findOne(<V2-commandId>)`: Abbildung V2-ID → kernel_command_id und
  Kernstatus → V2-Status (session.js `kernelCommandView`).
- Hinweis Orchestrator (ok = status ∉ {failed, cancelled}): Strang D hatte keine eigene `ok`-Angleichung; mit der
  Änderung bleibt alles grün.

## Braucht Rahmen-Haken (v4/frame, nicht geändert)

1. `v4/frame/index.js` Zeile 159 (`startShell`): `installWorkjetBridges({ kernel, shellVer, shell: shellHandle, mobileCatalog })`
   – `shell` ist eine Funktion, die den Griff liefert (index.js reicht sie an host-bridge.js weiter). Ohne den Griff
   meldet `app.open` `shell.error 'mobile-presentation-unavailable'` (kein falsches „geöffnet“).
2. `shellHandle()` (v4/frame/index.js:1262) braucht `showDesktop()` ohne Umschalten (alle Fenster minimieren, Desktop
   fokussieren; heute nur intern `state.showDesktop`, das bei wiederholtem Aufruf zurückschaltet). `navigate(appId)`
   existiert bereits.
3. CSS für `html[data-workjet-mobile-host="true"]` (V2 `mobile-host.css`: Kopf-/Taskleiste, Startmenü, Chat-Dock
   ausblenden; `.shell-window[data-workjet-mobile-active]` vollflächig; Desktop-Raster 4 Spalten) in `v4/frame/frame.css`.
   Selektoren im v4-DOM: `.shell-window`, `.is-focused`, `data-owner-id="v4app:<app>#n"`, `data-window-app`,
   `.desktop-icon[data-target]` (v4/frame/desktop.js), `[data-v4-retiring]`.
4. `v4/manifests/shell.manifest.json` (generiert): neue Dateien `actions/{instance,computer,session,device}.js`,
   `host-bridge.js` aufnehmen (gen-manifest), sonst lädt der dynamische Import im SW-Artefakt nicht.

## Offene Fragen an den Kern

- **Mobil-Katalog (neu, K-WJ-D1)**: V2 liest die statische Altdatei (in v4 verboten). Es fehlt eine Quelle für
  `mobilePresentation`/`desktopOnly`/Titel je App (z. B. Layout-Feld im App-Manifest oder Kern-Collection). Bis dahin
  meldet die Hostbrücke den V2-Fehlerpfad `catalog-unavailable` (retryable).
- Routen: `ctox.workjet.grok.v1`, `ctox.workjet.device.v1` (E25), `ctox.workjet.providers.*`, `ctox.workjet.computer.*`,
  `ctox.workjet.session.*` müssen im Kern existieren; Sitzung ohne Route = Workjet „fehlgeschlagen“.
- K-WJ-6 (Stub streicht `null` aus command.result) trifft auch D: Grok `login/check: null`, Provider
  `lastAttempt.failure: null`, `selection: null`, Sitzung `thread_id: null` würden den Workjet-Vertrag brechen. Die
  Stub-Proben verwenden deshalb Ergebnisse ohne null.
- requestNative-Fehlertext: V2 reichte Nativfehler roh durch (Workjet vergleicht beim Gerät
  „workjet device management is not allowed“ → forbidden); v2-compat formt „Native request … failed: …“ → Workjet
  sieht guest_failed statt forbidden. Gehört zu v2-compat/E25.
- index.js-Router (nicht Strang D): Gerät-Nicht-Objekt/unbekannte Aktion tragen andere Texte als V2
  („Workjet device request must be an object.“ / „Workjet device action is unsupported.“); Workjet sendet beides nie,
  liest beides gleich (guest_failed).
- V2-Parität bewusst beibehalten: `computer.assign` lehnt Workjet-Felder `deviceBindingId`, `roles`, `active`,
  `detectedFacts`, `hostKeyFingerprint` ab (wie V2 am Pin).

## Orakel-Anzahl

Vorher (Strang-D-Anteil): 0. Nachher: 32 V2-Verhaltensproben gegen v4 (grok 4, provider 14, computer 10, session 1,
events 3), 2 Differenz-Orakel gegen V2-Rümpfe (Gerät 28 Vergleiche, Hostbrücke 20 Schritte), 5 Zähl-/Vollständigkeits-
prüfungen, 12 v4-Proben (10 gegen den Kern-Stub 8489b4e, 2 Hostbrücke). Nicht portiert (Quelltext-Regex, kein Verhalten):
6 Proben, Liste in oracle.test.mjs Block Strang D. Gesamtlauf vorher 51 Tests (WJB-0-Stand), jetzt 254 (alle Stränge, letzter Lauf).

## Abnahme (selbst gelaufen, 05:30, aus src/apps/business-os)

```
node --test 'v4/shell/workjet/tests/*.test.mjs'   → tests 254 · pass 251 · fail 0 · todo 3 (todo: K-WJ-B1, K-WJ-B2, K-WJ-6 der Stränge A/B; letzter Lauf 05:32)
node v4/os-primitives/check-caps.mjs --calls      → calls: 202 · ABWEICHUNG 0 · HUELLE 0 · UNGEPRUEFT 3 · OK 195 · OHNE_VERTRAG 4
node v4/debt/ratchet.mjs                          → ratchet: guard 1838 · scan 3 · ratchet: OK
node --test v4/tools/guards/guard-v4.test.mjs     → tests 74 · pass 74 · fail 0
guard-v4 --json, Treffer in den fünf D-Dateien    → 0 (alle V2-Namen/-Texte mit v4-allow-Marker)
```
Rahmen-Labor nicht gelaufen (Brief: läuft in anderem Strang).

## Geänderte Dateien

- neu (Strang D): `actions/instance.js`, `actions/computer.js`, `actions/session.js`, `actions/device.js`, `host-bridge.js`, `WJB-D.md`
- additiv, markiert `// Strang D`: `index.js` (Importe, ACTIONS-Zeilen, Sitzung-vollständig-Regel, Ereignisstrom,
  Hostbrücke, `installWorkjetBridges`-Parameter `shell`, `mobileCatalog`, `doc`), `tests/oracle.test.mjs` (3 Blöcke am Ende),
  `tests/contract-checkers.mjs` (29 Prüfer + Block am Ende)
- `tests/bridge.test.mjs` (nicht in der Additiv-Liste, unvermeidbar): Test 1 und 2 prüfen „leere Brücke hängt nicht
  ein“ bzw. Router-Merkmale jetzt mit einer Tabelle nur aus Projektaktionen (`PROJECT_ONLY`); sonst wären sie durch die
  Freischaltung rot geworden. Absicht der Tests unverändert.
