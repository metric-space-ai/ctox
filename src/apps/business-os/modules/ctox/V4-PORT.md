# ctox (Crew) – Port auf App-API 4.0 (Strang UD3, Welle D)

Stub-Pin: `~/.local/state/shell-v4/stub-pins/67229d2` (`CTOX_APP_API_STUB`). Vorlage: `v4/tools/port/PORTING.md`,
Pilot `modules/knowledge`. `handlers/**`, `module.json`, `collections.schema.json`, `shared/**`, `v4/**` nicht angefasst.

## Fortschritt (Berlin-Zeit, gemessen mit `TZ=Europe/Berlin date`)

- 13:33 Start, Brief + PORTING.md + app-api.d.ts + knowledge-Pilot + V4-HANDLERS.md gelesen.
- 13:40 Vorher-Messung: `scan.mjs --app ctox` = **130 Stellen** (index.js 121, test.js 7, index.html 1, schema.js 1;
  CSS 490 Selektoren ungescoped, 21 mit Shell-Vorfahr). guard-v4 (`--app modules/ctox`, ohne Tests): 111 Treffer
  in index.js/locales/index.html. Ausgangsdateien im Scratchpad gesichert (Diff-Basis).
- 14:14 Bereiche 1–4 portiert; `tests/v4-api.test.mjs` 12/12 grün gegen Pin 67229d2.
- 14:19 Zustandstexte/Vokabular, CSS-Scope, Locales; Legacy-Suite `test.js` auf v4 umgestellt, grün.
- 14:22 `tests/layout.browser.mjs` auf v4 umgestellt (echtes mount gegen Stub, 5 Breiten): 9/9 grün.
- 14:24 Abschlussmessung (Tabelle unten).

## Ergebnis (gemessen 14:23)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app ctox` | **130 → 1 Stelle** (nur `schema.js vorhanden`, OFFEN-1) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs ctox modules/ctox/index.css --check` | gescoped (490 Selektoren, 21 Shell-Vorfahren ersetzt) |
| guard-v4 (ohne Tests) | `node v4/tools/guards/guard-v4.mjs --json --app modules/ctox` | **111 → 2 Treffer**, beide `schema.js` (`file-schema-js`, `ref-legacy-catalog-file`), OFFEN-1 |
| guard-v4 (mit Tests) | `… --include-tests` | zusätzlich 9 Vokabel-Treffer in `test.js` = Eingabedaten/Testnamen (Core-Texte wie „model API is temporarily unavailable“), bewusst belassen |
| v4-Test gegen Stub | `CTOX_APP_API_STUB=…/67229d2 node --test src/apps/business-os/modules/ctox/tests/v4-api.test.mjs` | **12/12 grün** (mit `CTOX_V4_SHOTS=1` inkl. Screenshots) |
| Layout/Bedienung v4 | `CTOX_APP_API_STUB=…/67229d2 node --test src/apps/business-os/modules/ctox/tests/layout.browser.mjs` | **9/9 grün** (430/630/768/1000/1280 + Editor/Drawer/Pause/Wesen-Klick) |
| Legacy-Suite | `node test.js` (aus dem Modulordner; esbuild/playwright aus `~/Documents/ctox/src/apps/business-os/node_modules` per Resolve-Hook, Klon hat keine node_modules) | 50 ok + 6 node:test, Exit 0 |
| Handler (fremd, nur Gegenprobe) | `node --test src/apps/business-os/modules/ctox/handlers/tests/*.test.mjs` | 16/16 grün (unverändert) |

Screenshots (selbst angesehen): `tests/shots/ctox-1280.png`, `ctox-420.png`, `ctox-1280-drawer.png`,
`layout-430|630|768|1000|1280.png`, `layout-1280-drawer.png`. Befund und Behebung: (a) ohne Shell-Container fiel die
App unter 768 px nicht auf eine Spalte → App-root deklariert jetzt `container: business-app-window / inline-size`
(wie calendar); (b) Drawer brauchten eigene Ebene im root → `.ctox-drawer` absolut im Fenster; (c) doppelter
Schließen-Knopf im Drawer entfernt. 420 px: Aufgabenliste oben, Karte darunter, keine horizontale Verschiebung.

Abgedeckt vom v4-Test: mount zeigt Daten · `sim.push` erscheint live, Löschung verschwindet · leere Collections: kein
Text, kein `role=status`, keine Zone · `api.put` → `accepted` und live · `ctox.task.update` legt
`business_commands`-Dokument an (`payload.task_id/record_id/module`), `sim.failCommand` → genau ein Zoneneintrag
`ctox.command.ctox.task.update`, kein Statustext im Formular · Löschen fragt über `api.shell.dialog` (ohne Ja kein
Befehl, mit Ja `ctox.task.delete` → done) · Pause nur in der Zone · snapshot → structuredClone → Remount mit
`restored` erhält Auswahl, offenen Editor und Entwurf · Deep-Link über `navigate('ctox', {task_id})`/`onRoute` ·
unmount meldet alle Abos ab (inkl. `ctox_harness_events`, `ctox_runs`) und leert root · keine `pageerror`.
**Nicht** abgedeckt: Locale-Wechsel live, Web-Stack-Assistent, Mitglied anlegen/ändern (nur Befehlsweg, keine
eigene Prüfung), Hot-Plug-Skript der Shell, Geometrie-Labor der Shell.

## Bereiche (Teilschritte)

| # | Bereich | Status |
|---|---|---|
| 1 | Einstieg, Imports, Markup, CSS-Laden (R1/R2/R10/R14/R15) | erledigt 14:14 |
| 2 | Datenweg: Live-Abos statt RxDB/Sync/Readiness (R3/R4/R12) | erledigt 14:14 |
| 3 | Befehle (R5), Dialoge (R7), Rechte (R9) | erledigt 14:14 |
| 4 | Shell: Drawer im root, Navigation, Locale, Benachrichtigungen (R6/R8/R13) | erledigt 14:14 |
| 5 | Zustandstexte → Zustandszone (R11), Vokabular | erledigt 14:19 |
| 6 | CSS-Scope (R16), Locales; schema.js (R17) | CSS/Locales erledigt; schema.js OFFEN-1 |
| 7 | Tests (v4-api.test.mjs, test.js, layout.browser.mjs), Screenshots | erledigt 14:22 |

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`. Zustand je Montage (`state` lokal im mount,
  Abos in `state.disposers`) – zwei Montagen teilen nichts (Falle 1 aus PORTING.md entfällt hier).
- **Daten** (vorher ~25 RxDB-/Sync-Funktionen, Lese-Koaleszenz, Readiness, `subscribeTaskHistoryChanges`): je Collection
  ein Abo, jede Emission stößt eine entprellte (80 ms) synchrone Neuberechnung an:
  `business_commands`, `ctox_queue_tasks`, `ctox_bug_reports` (`sort updated_at_ms desc, limit 120`),
  `ctox_crew_members` (`{archived:false}, limit 64`), `communication_accounts` (`limit 200`, nur channel/enabled),
  `api.get('ctox_runtime_settings','runtime-settings')`, `api.get('ctox_harness_status','harness')`,
  `api.actor.permissions`. Ausgewählte Aufgabe: eigene Abos auf `ctox_harness_events` (`task_id`, Rückfall
  `command_id`, limit 200) und `ctox_runs` (`task_id`, limit 32), neu bei Auswahlwechsel; Mitglieds-Drawer: Abo auf
  `ctox_runs` (`crew_member_id`, limit 40) solange offen. `forbidden` → `taskSourceUnavailable` + Zone.
  Abweichung: Listen-Selektor `updated_at_ms > 0` entfällt (Sortierung bleibt; Dokumente ohne Zeitstempel stehen hinten).
- **Befehle**: `dispatchCtoxCommand` → `api.command(type, {...payload, record_id, module:'ctox', inbound_channel,
  client_context})`, nie warten; `failed`/lokal ungültig → `api.status.report({code:'ctox.command.<type>'})`,
  `done` → `clear`. UI arbeitet lokal weiter (Entwurf weg, Modell sofort angepasst wie bisher).
  `commandBus.cancel` → Plattformbefehl `ctox.command.cancel {target_command_id, reason}`.
- **Zustandstexte** raus aus Inhalt, index.html, labels, Locales: Lade-/Sync-/Offline-Zeilen, Platzhalter
  („Arbeitsstand wird geladen.“, Fußzeile „noch keine aktuellen Daten“), Wiederholen-Knopf, Leertexte („Hier gibt es
  keine Arbeit“, „Noch nichts gelernt“, Web-Stack-„keine Aufnahmen“), Formular-Statuszeilen („Gespeichert“, „Command
  angelegt“), Pausenhinweis im Kopf, Health-Tooltip am Fenster, Toasts. Zone-Codes: `ctox.command.<type>`,
  `ctox.harness.critical` (error), `ctox.harness.warning` (warn), `ctox.harness.paused` (info),
  `ctox.source.forbidden`, `ctox.route.unknown`, `ctox.route.task_missing`, `ctox.webstack.notice`,
  `ctox.tasks.import`, `ctox.render`. 24 Zustands-Schlüssel aus labels und Locales gelöscht.
- **Fachbezeichnungen** mit Zustandsvokabular umformuliert (Bedeutung gleich): „Waiting in queue“→„In queue“,
  „Wartet“→„Eingereiht“/„Queued“, „Wartet auf den Start“→„Noch nicht gestartet“, „Wird geprüft“→„In Prüfung“,
  „Prüfung nicht verfügbar“→„Keine Prüfung möglich“, „Nachweis wird geprüft“→„Nachweisprüfung“, „Läuft“→„Aktiv“,
  „wartet auf eine Rückmeldung“→„braucht eine Rückmeldung“, „wartet auf“→„hängt an“, „Checking evidence“→„Evidence
  check“, Fehlergründe „war nicht erreichbar“→„hat nicht geantwortet“, „nicht rechtzeitig“→„zu spät“.
- **Shell**: `setTitle('Crew')`; Drawer (`openLeftDrawer/closeDrawers`) und Kompaktmenüs (vorher Top-Layer-Popover)
  liegen im App-root; Spaltengriff verdrahtet die App selbst (Zeiger + Pfeiltasten, Breite im Snapshot);
  `autoWirePaneGrammar(root)` ruft die App; Bestätigung über `api.shell.dialog({kind:'confirm', …, default:false})`;
  Deep-Link über `api.shell.onRoute` (`task_id/command_id/drawer/return_thread_id`) statt sessionStorage, Hash,
  `postMessage`, `ctox-business-os-app-launch`; Sprache über `api.shell.locale` + `t('ctox.<key>')` mit
  eingebetteten Rückfalltexten (HTTP-Locales entfallen); Kanäle verwalten entfernt (keine v4-App trägt die Kanal-Einrichtung, FR4-5); Rechte als Spiegel aus `api.actor` (Nachbildung der bisherigen Rollenregeln).
- **Imports**: `ctox:shared/business-chat.js` (crewCreatureHtml, crewMemberExpression…), `ctox:shared/crew-motion.js`
  (`startCrewMotion(root)`, `syncCrewMotion`), `ctox:shared/crew-renderer.js`, `ctox:shared/pane-grammar.js`;
  entfallen: dialogs, list-state, permissions, task-history-native-changes. Keine `?v=`.
- **Uhr**: Sekundenanzeige laufender Arbeit als `setTimeout`-Kette mit `v4-allow: ui-animation` (kein Datenabruf).
- **Snapshot**: Auswahl, Ansicht/Filter/Sortierung, Zoom, Editor offen, Spaltenbreite, Pins, Drawer, Deep-Link-Stand,
  `taskEditDrafts`, offene Formulare (`data-ctox-draft-key`: neues Mitglied, Mitglied, Gedächtnis), Listen-Scroll.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen 14:23 mit `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon. Einstufung der 12
App-Befehle übernommen aus `V4-HANDLERS.md` (Strang H6, alle portiert, Legacy 0); hier nur die UI-Seite.

| Befehlstyp | Rust | Einstufung | UI-Aufruf |
|---|---:|---|---|
| `ctox.task.update` | 6 | Fähigkeit (Handler H6) | Editor speichern |
| `ctox.task.delete` | 4 | Fähigkeit (Handler H6) | Löschen nach `api.shell.dialog` |
| `ctox.queue.pause` / `.capacity` | 6 / 5 | Fähigkeit (Handler H6) | Crew-Menü |
| `ctox.queue.block` / `.release` / `.retry` | 5 / 5 / 7 | Fähigkeit (Handler H6) | Aufgaben-Steuerung |
| `ctox.crew.assign` | 5 | Fähigkeit (Handler H6) | Zuweisen-Auswahl |
| `ctox.crew.member.create` / `.update` | 3 / 3 | Fähigkeit (Handler H6) | Mitglied anlegen/ändern/archivieren |
| `ctox.crew.memory.update` | 4 | Fähigkeit (Handler H6) | Gedächtnis speichern/bestätigen |
| `business_os.chat.task` | 154 | Core-generisch (reine Queue, H6: nicht ctox) | Fortsetzen, Import |
| `ctox.command.cancel` | 6 | Core-generisch (Plattform-Abbruch) | Abbrechen laufender Arbeit |
| `web_stack.auth_assist.request` | 6 | Fähigkeit browser (H6: nicht ctox) | Web-Stack-Assistent |

`ctox.crew.learning.*` und `ctox.queue.abort_turn` haben Handler, aber keinen UI-Aufruf (auch vorher nicht).

## OFFEN (nummeriert)

1. **schema.js nicht gelöscht** (Scanner 1 Stelle, guard 2 Treffer). Inhalt ist gleichwertig zu
   `collections.schema.json` (gemessen: 29 Collections, Versionen/`required`/Properties 0 Abweichungen, Migrationen
   business_commands 1–2, ctox_queue_tasks 1–3, ctox_runs 1, workjet_computers 1, workjet_projects 1–5 deckungsgleich).
   Löschen bricht fremde Importeure außerhalb meines Schreibbereichs: `app.js`, `modules/mail/schema.js`,
   `modules/cv-print-builder/schema.js`, `modules/mail/tests/mail.test.mjs`,
   `modules/kundenpipeline/tests/kundenpipeline.test.mjs`, `rxdb/tests/{workjet-project-configuration,
   workjet-project-migration,crew-identity,workjet-computer-migration}-smoke.mjs`,
   `shared/workjet-{project-control,supervisor-luma-contract}.test.mjs`, `scripts/assert-module-conformance.mjs`.
   Löschung erst, wenn diese auf das Manifest umgestellt sind (Koordinator / Stränge UD5, UD7, Shell).
2. **Manifest** (`v4/manifests/ctox.manifest.json`, Generator): führt `workjet_*` (9) als eigene Collections –
   die ctox-UI liest und schreibt keine davon (0 Treffer in index.js); gehören dem Kern-Strang Phase 11 und sollten
   aus ctox fallen. Tatsächlich gelesen: `business_commands`, `ctox_queue_tasks`, `ctox_runtime_settings`,
   `communication_accounts` (foreign, dort schon so), dazu eigene `ctox_bug_reports`, `ctox_crew_members`,
   `ctox_harness_status`, `ctox_harness_events`, `ctox_runs`. `documents`/`communication_messages` im Manifest nur
   per Text-Literal erkannt, kein Lesen. `handlers` leer, `styles` fehlt (index.css).
3. **Chat-Übergabe**: 4.0 hat keine Chat-API. „In Chat öffnen“ sendet `ctox-business-os-open-chat` als bubbelndes
   DOM-Ereignis aus root (vorher `postMessage`/window-Ereignis). Ebenso bleibt `publishCrewWorkload`
   (`window.__ctoxCrewWorkload` + `ctox-crew-workload`) für die Crew-Leiste der Shell. Das frühere
   `ctox-business-os-record-focus` an die Quell-Unterhaltung entfällt (Ersatz: Zone `ctox.route.task_missing`).
   Shell-Strang: Vertrag für Chat-Öffnen und Crew-Auslastung festlegen.
4. **Kanäle verwalten**: Knopf entfernt (Strang NAV, FR4-5): Ziel `shell`/settings gibt es in v4 nicht, und keine v4-App
   richtet Kommunikationskonten ein. Wiederkehr erst mit einer Kanal-App (dann `api.shell.navigate('<app>', …)`).
5. **Kopfzonen**: index.css hält die V2-Zonen (80 px Icon links, 70 px Fenstersteuerung rechts) wie knowledge/calendar.
   Der v4-Rahmen (`v4/frame`) hat eine eigene Kopfzeile; ob die Zonen entfallen, entscheidet der Shell-Strang.
6. **Rechte-Spiegel**: Grants aus `governance` (explizite Freigaben je Scope) sieht die App in 4.0 nicht mehr; der
   Spiegel nutzt Rolle (chef/admin), `api.actor.permissions` und Eigentum der Aufgabe. Entscheidung bleibt im Core.
7. **Telemetrie-Hinweise** „nicht live“, „noch kein Plan“, „keine Live-Messwerte“ im Verlauf bleiben (Fakten der
   Aufgabe, kein Vokabel-Treffer); Owner-Sichtung empfohlen.
8. Kein git benutzt außer einem lesenden `git status --short` auf modules/ctox zu Beginn (leer).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Zurück (DE+EN, `index.js` + `locales/*.json`): Ansicht/Pipeline „Eingereiht“/„Queued“ → **„Wartet“/„Waiting“** (`viewWaiting`; `pipelineQueued` DE, EN war upstream „Queued“); Statuswort `queued` in der Status-Übersetzung → **„Wartet“/„Waiting“**; „braucht eine Rückmeldung“/„needs external input“ → **„wartet auf eine Rückmeldung“/„waiting for external input“**; „hängt an“ → **„wartet auf“**. Harness-Phasen (Knoten + Phasentitel): „In queue“ → **„Waiting in queue“**, „Review queued“ → **„Review waiting“**, „No review possible“ → **„Review unavailable“** (Zeile „The reviewer was unavailable.“), „Evidence check“ → **„Checking evidence“**, „Noch nicht gestartet“/„Not started yet“ → **„Wartet auf den Start“/„Waiting to start“**, „In Prüfung“ → **„Wird geprüft“**, „Keine Prüfung möglich“ → **„Prüfung nicht verfügbar“**, „Nachweisprüfung“ → **„Nachweis wird geprüft“**; Kommunikationsphasen „Acknowledgement pending“ → **„Awaiting acknowledgement“** samt Zeilentexten („Send is in progress …“, „CTOX is waiting for acknowledgement.“).
- **Nicht** zurückgedreht (System-/Harness-Zustand oder unklar, Frage an Owner): `harnessRunning` „Aktiv“ (Harness-Dienstzustand, `harnessStatusText` derzeit ohne Aufrufer), `sourceUnavailable` „Quelle ohne Kanal“ (Kanal-/Verbindungsbegriff), `harnessCriticalProjection` „… fehlt noch“ und `harnessWarningTitle` „Aufgaben stehen für die Crew an“ (Harness-Gesundheit, Zone), Fehlergründe „Der Modelldienst hat nicht geantwortet.“/„… hat zu spät geantwortet.“ (Original „war nicht erreichbar“/„nicht rechtzeitig“).
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (11/11; scan 1 = schema.js); Legacy `node test.js` 50 ok + 6 node:test.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G3: Rohstatus `in_progress` erschien als „In Progress“; `displayStatus` kennt jetzt in_progress (Arbeitet/Working), active (Aktiv) und cancelled (Abgebrochen).
- G4: Aufgabentitel brechen um (Metazeile bleibt einzeilig).
- G1: Ablaufkarte scrollte seitlich, weil das Einpassen bei MIN_ZOOM 0.72 stoppte; Einpassen in die Breite darf jetzt bis 0.3 (`FIT_MIN_ZOOM`), die Höhe weiter nur bis MIN_ZOOM, manuelles Zoomen unverändert.
- Offen (Labor): 420 G6 „Spalten 1 vs 2“ – das freie Vergleichsfenster in 1280 ist ~640 px breit (Mindestbreite), nicht 420; die nun breitenabhängige Karte unterscheidet sich deshalb.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app ctox --shared` (Stub-Pin 9fa9948) → 1280/900 OK, 420 nur G6 (Laborartefakt, s. o.); `node --test modules/ctox/tests/v4-api.test.mjs` grün; `scan.mjs --app ctox` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
