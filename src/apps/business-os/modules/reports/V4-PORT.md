# reports – Port auf App-API 4.0 (Strang UC2, Welle C)

Stub-Pin cff5ba5 (Erstport gegen 67229d2). Start 13:26, fertig 14:11 (Berlin, gemessen).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app reports` | 60 → 0 Stellen, Exit 0 (CSS vorher 73 ungescoped) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs reports modules/reports/index.css --check` | gescoped |
| Wächter | `node v4/tools/guards/guard-v4.mjs --app modules/reports` | 53 → 0 Treffer (Testdateien getrennt) |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/reports/tests/v4-api.test.mjs` | 9/9 grün |
| Legacy-Suite | `node modules/reports/test.mjs` + `core/funnel.test.mjs` (aus Kopie mit node_modules-Link) | 9/9 + 4/4 grün |
| Screenshots | `tests/shots/reports-1280.png`, `reports-420.png` | angesehen: drei Spalten bzw. gestapelt, kein Überlauf |

v4-Test deckt ab: mount mit Live-Daten aus `business_module_reports` + `ctox_bug_reports` · `sim.push` erscheint,
Löschung verschwindet · leere Collections: keine Texte, kein Titel, keine Zone · Schreiben (Rückweg Coding-Turn →
`api.patch`, Outbox leer) · Befehl `ctox.module.rollback` mit `failCommand` → genau ein Zoneneintrag, Wiederholung
`done` räumt ihn · `navigate('ctox', {task_id, command_id, …})` · Delegationsdialog liegt im Fenster, snapshot →
Remount mit `restored` erhält Auswahl, offenen Auftrag, Ziel-App, Reviewer, offene Aktionsspalte · unmount löst alle
8 Abos und leert root.

## Entscheidungen

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; Abos je Montage lokal.
- **Daten**: 7 `api.query(<coll>, {})`-Abos statt `find().exec()` + `$`-Abos + Readiness/Restart-Sync; Ansicht wird
  entprellt (80 ms) aus dem Live-Stand berechnet. Fenster-Ereignis `ctox-business-os-reports-updated` entfällt.
- **Ziel-Apps der Delegation**: `ctx.modules` gibt es nicht mehr → `api.query('business_apps')` (System-Collection,
  §16.14), gleiche Filter (`hidden`, `editable`).
- **Zustandstexte entfernt**: Syncing-/Leer-/„nicht verfügbar“-Kästen, Diagnosezustand, „Rollback läuft/ausgeführt“,
  „Wird eingereicht …“, Platzhalter „nicht angelegt“, „Keine Beschreibung/Erwartung hinterlegt“, Change-Summary-Rückfall,
  „Noch nicht delegiert“, „keine gespeicherte Version“, Titel „Eintrag auswählen“ (auch in index.html). Leere Textfelder
  rendern keine Karte. Zone-Codes: `reports.command.<type>` (failed / lokal ungültig, `ref` = Befehls-ID),
  `reports.delegate.incomplete` (warn).
- **Befehle**: `api.command(type, {...payload, module, record_id, inbound_channel, client_context:{source, module_id,
  actor}}, {record_id})`, nie warten; Delegationsdialog schließt sofort, Freigabe-Stand erscheint live über
  `ctox_task_approval_requests`. `ctx.notifications`/`showReportNotification` entfallen (Export und Legacy-Test entfernt).
- **Navigation**: „CTOX Task zeigen“ → `api.shell.navigate('ctox', {task_id, command_id, task_status, source_module})`
  statt Fenster-Ereignis + `location.hash`; Links `#threads`/`#coding-agents` → Knöpfe mit `navigate(appId)`.
- **Dialog im Fenster**: `.ctox-modal` hängt in `[data-reports-root]`, CSS `[data-app="reports"] .ctox-modal
  {position:absolute}` + root `position:relative` (shared/base.css setzt `position:fixed` = Shell-Ebene).
- **Container**: root ist `container: business-app-window` (die Breitenabfragen 1024/768 griffen sonst nicht).
- **i18n**: `locales/{de,en}.json` per relativem JSON-Modul-Import (`with {type:'json'}`, kein HTTP-Ladeweg),
  `api.shell.t('reports.<key>')` übersteuert, Locale live über `api.shell.locale.subscribe`. Locales auf die 37
  benutzten Schlüssel reduziert (46 Zustands-/Chat-/Altschlüssel entfernt); `delegateCodingRunning` → „Bereits
  übergeben.“, en `running` → „Working“ (Wächter-Vokabular).
- **Toter Code entfernt**: Kontextmenü-Chat (`initReportsContextMenu` wurde nie aufgerufen, nutzte
  `window`-Listener, `document.querySelector('[data-ctox-chat-root]')`, Ereignis `ctox-business-os-chat-submit`);
  `resolveReportsContextRecord` bleibt exportiert (getestet).
- **schema.js gelöscht**: re-exportierte das ctox-Schema (29 Collections) + `ctox_task_approval_requests` aus threads;
  `collections.schema.json` enthält dieselben 30 Definitionen. reports besitzt keine davon.

## Fremd-Collections (für den Manifest-Generator)

reports besitzt keine Collection. Tatsächlich gelesen (alle `foreign`):

| Collection | Eigentümer (Generator-Vorschlag) | ops |
|---|---|---|
| `business_module_reports` | ctox | read, **write** (`patch`: Rückweg `ctox_command_id`/`task_id`/`status`, OFFEN-1) |
| `ctox_bug_reports` | ctox | read |
| `business_module_releases` | ctox | read |
| `business_commands` | shell (System) | read |
| `ctox_queue_tasks` | shell/Core | read |
| `ctox_task_approval_requests` | threads | read |
| `business_users` | ctox | read |
| `business_apps` | System (§16.14) | read |

Die übrigen 22 Collections aus `collections.schema.json` (business_chats, workjet_*, ctox_crew_*, …) liest reports
nicht; sie gehören nicht ins reports-Manifest.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer `grep -rn '"<type>"' src/core` | Einstufung |
|---|---:|---|
| `threads.ctox_approval.request` | 17 | Fachlogik der App threads (Handler dort) – reports ist nur Absender |
| `ctox.module.rollback` | 13 | Core-generisch (Modul-Lebenszyklus, `module_lifecycle.rs`) |
| `ctox.coding.turn` (nur gelesen) | 23 | Core-generisch / Fähigkeit `ctx.cap.coding_agent` (startet threads nach Freigabe) |

## OFFEN

1. **Rückweg-Schreiben in fremde Collection**: `reconcileDelegationBackfill` schreibt per `api.patch` in
   `business_module_reports` (Eigentum ctox). Sauber wäre eine Projektion im Core/threads-Handler beim Abschluss von
   `ctox.coding.turn`; bis dahin braucht das Manifest `foreign_collections.business_module_reports.ops = [read, write]`.
   Je Montage höchstens ein Versuch je Zielstand (kein Schreibsturm bei Ablehnung).
2. **Navigationsparameter**: die CTOX-App muss `onRoute`-Parameter `task_id`, `command_id`, `task_status`,
   `source_module` lesen (Strang UD3); threads/coding-agents bekommen keine Parameter (wie vorher der Hash).
3. **Mehrere Fenster**: Modulzustand ist modulweit (wie knowledge OFFEN-4); eine zweite Montage übernimmt die
   Darstellung. Test montiert nach dem Leer-Test neu.
4. **Shell-Zonen**: Kopfgeometrie (Icon-Zone `--shell-v2-icon-size`, 70px rechts) unverändert übernommen; der v4-Rahmen
   setzt `--shell-v2-icon-size: 28px`. Ob v4 die Zonen überhaupt noch über den Inhalt legt, entscheidet der Rahmen-Strang.
5. **module.json** unverändert (`collections` listet 7 fremde Collections; `business_apps` fehlt) → Generator.
6. Kein git benutzt.

## Nachprüfung gegen Stub-Pin cff5ba5 (Wiederaufnahme 15.10., bis 15:10 Berlin, gemessen)

Nach dem Sitzungsabbruch ~14:40: keine Datei im Schreibbereich nach 14:25 geändert, keine halben Edits. Unverändert
gegen den neuen Pin neu gefahren:

- `scan.mjs --app reports`: 0 Stellen, CSS ungescoped 0, Exit 0; `scope-css.mjs … --check`: gescoped.
- `guard-v4.mjs --json`: 0 Treffer unter `modules/reports/` (nur getrennt gezählte Testdatei-Treffer).
- v4-Test gegen cff5ba5: 9/9 grün (mit `CTOX_APP_API_STUB` und ohne – Rückfallpfad im Test jetzt cff5ba5).
- Legacy-Suite (Kopie mit node_modules-Link): 9/9 (+ core/funnel 4/4) grün.
- Screenshots 1280/420 vom Testlauf neu geschrieben und angesehen: unverändert gegenüber Erstport, kein Überlauf.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Listentitel `.report-row-title` bricht um statt Ellipse.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app reports --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/reports/tests/v4-api.test.mjs` grün; `scan.mjs --app reports` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.


## Strang SKL-A2 – Ergebnis per Rückruf statt Abgleich (15.10., Kern §16.30)

- Vorher: `reconcileDelegationBackfill` las bei jeder Änderung `business_commands`, suchte `ctox.coding.turn` zum Report
  und schrieb `ctox_command_id`/`task_id`/`status` per `api.patch` aus dem Browser in die fremde Collection
  `business_module_reports` (OFFEN 1). Nachher: entfernt (samt `state.backfillAttempts`). Der Delegationsdialog sendet
  `threads.ctox_approval.request` mit `callback:{on_complete:'reports.delegation.complete',
  on_failed:'reports.delegation.failed'}`; threads leitet das Ergebnis der freigegebenen Arbeit genau einmal weiter.
- Neue Handler `handlers/reports.delegation.complete.js`, `.failed.js` (Logik `handlers/_delegation.js`): setzen
  `ctox_command_id`, `task_id`, Status mit den Bestandsregeln (completed → `completed`, failed → `blocked`, cancelled →
  Status bleibt; Status nur aus `open|running`), schreiben nur Änderungen (`{status:'ignored'}` bei Doppelzustellung).
  Damit wandert das Schreiben in `business_module_reports` vom Browser in einen Handler (foreign write bleibt nötig).
- Kein eigenes enqueue, keine Skills/Prompts (der Auftragstext ist Nutzereingabe im Dialog).
- Tests: `cd handlers && node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` → 4/4 (Kette
  Dialog-Payload → threads request/approve → Ergebnis → Weiterleitung → Report, Agenten- und Coding-Ziel, failed/
  cancelled, Doppelzustellung, ungültige Rückrufe). `tests/v4-api.test.mjs` (Stub 7706796): 9/9; der Schreibtest prüft
  jetzt, dass der Browser bei einem Coding-Turn **nichts** schreibt und das Handler-Ergebnis live erscheint; der
  Delegationstest prüft das `callback`-Feld.
- Kern-Abbau: kein Kernstück nur für reports; Lücke W-6 (siehe threads `V4-HANDLERS.md` §7): ein `ctox.coding.turn`,
  der nach Annahme fehlschlägt oder asynchron endet, kommt erst mit dem Befehls-Rückruf des Kerns zurück. Bis dahin
  kommt nur ein synchron abgeschlossener Turn als `reports.delegation.complete` an.
- Manifest (Generator): `handlers` bekommt die zwei Typen; `foreign_collections.business_module_reports` Beleg
  wechselt von `index.js` zu `handlers/_delegation.js`.

## Strang FIX10 – Rückruf-Kette entfernt (16.10.2026)

- Seit LOOP Runde 2 sendet „Bugs & Features“ keine `threads.ctox_approval.request` mehr (Übergabe nur über die
  Code-Ansicht, `v4/source/source-mode.js`). Die Rückruf-Kette aus SKL-A2 hatte damit keinen Absender und ist entfernt:
  `handlers/_delegation.js`, `handlers/reports.delegation.complete.js`, `.failed.js`, `handlers/tests/delegation.test.mjs`,
  `commands_in` in `v4-manifest.json`; in threads der Listeneintrag in `APPROVAL_CALLBACKS` (Liste jetzt leer) und
  `commands_in from:['reports']`. Die Abschnitte oben zu SKL-A2 beschreiben den früheren Stand.
