# outbound – Port auf App-API 4.0 (Strang UD1, Welle D)

Stand: laufend aktualisiert (Berlin-Zeit). Stub-Pin: `~/.local/state/shell-v4/stub-pins/67229d2`.

Fortschritt:
- 13:39 Start, Brief + PORTING.md + app-api.d.ts + knowledge-Pilot + V4-HANDLERS.md gelesen.
- 13:45 Scanner vorher: **125 Stellen**, CSS 424 Selektoren ungescoped (13 mit Shell-Vorfahr). guard-v4: 77 Treffer in UI-Dateien.
- 14:00 Bereich Engagements/Nachrichten, Freigaben/Versand, Scheduler, Skillbooks (`active-outreach.js`) portiert.
- 14:10 Bereich Kampagnen + Recherche/Quellen + Mailserver (`index.js`) portiert; Scanner 125 → 1 (nur R17 schema.js, s. OFFEN-1).
- 14:16 Zustandstexte entfernt (index.js, active-outreach.js, locales); guard-v4 UI-Dateien 77 → 1 (schema.js).
- 14:20 tests/v4-api.test.mjs gegen Stub-Pin 67229d2: 9/9 grün; Befund: Root-Listener überlebten den Remount
  (Klick feuerte 3×) → Listener je Montage über AbortController. Screenshots 1280/420 angesehen: Schmal-Layout
  braucht den Fenster-Container der Shell; Research-Zusammenfassung und Freigabe-Karte nachgebessert (CSS).
- 14:24 Legacy-Suiten auf v4-Vertrag umgestellt (nicht gelöscht): outbound.test.mjs 24/24 (inkl.
  focus-navigation, refresh), core/audience 4/4, handlers 100/100 (unverändert).

**Status: portiert.** Scanner 125 → 1 Stelle (R17 schema.js, bewusst stehen gelassen, OFFEN-1); guard-v4 UI 77 → 1
(dieselbe Datei). Nicht in diesem Werkzeug: Hot-Plug-Skript, Geometrie-Labor, Nicht-Admin-Abnahme.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 OUTBOUND_SHOTS=1 node --test src/apps/business-os/modules/outbound/tests/v4-api.test.mjs` | 9/9 grün (Chromium headless, storage memory) |
| Legacy-Suite | `node --test modules/outbound/outbound.test.mjs` (importiert focus-navigation + refresh) | 24/24 grün (esbuild-Rückfall auf ~/Documents/ctox/…/node_modules, `ctox:shared/`-Plugin) |
| Audience | `node --test modules/outbound/core/audience.test.mjs` | 4/4 |
| Handler (fremder Strang, unverändert) | `cd modules/outbound/handlers && node --test tests/*.test.mjs` | 100/100 |
| Scanner | `node v4/tools/port/scan.mjs --app outbound` | 1 Stelle (R17 schema.js), vorher 125 |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` (Filter modules/outbound/) | 1 Treffer (file-schema-js), vorher 77 UI + Handler |
| CSS | `node v4/tools/port/scope-css.mjs outbound modules/outbound/index.css --check` | gescoped (424 Selektoren) |
| Screenshots | `tests/shots/campaign-{1280,420}.png`, `tests/shots/approval-{1280,420}.png` | angesehen |

Abgedeckt vom v4-Test: mount zeigt Kampagnen + Firmen sofort aus dem Spiegel (kein Befehl, kein Zustandstext, keine
Zone) · `sim.push` erscheint live, Löschung verschwindet · leerer Kernel: kein Zustandstext/role=status/alert, keine
Zone, nur die fachliche Standard-Kampagne wird lokal angelegt · Freigabe `outbound.message.approve` mit `record_id`
(Dokument + payload), `sim.failCommand` → genau ein Zoneneintrag, kein Fehlerkasten, Erfolg räumt · Ablehnen fragt
über `api.shell.dialog` (ohne Antwort kein Befehl, mit Antwort `comment`) · Kette engagement.create → assign_sender →
draft.prepare: offline bleibt nur der erste Befehl `queued`, UI bedienbar; online folgen die Schritte am selben
Engagement · `api.put` → accepted, live sichtbar · snapshot → structured clone → Remount erhält offenen
Nachrichtenentwurf, Ansicht und Kampagne; Speichern sendet den restaurierten Text · unmount: 18 Abos 0, root leer.
**Nicht** abgedeckt: Import-Flow (shared universal-importer), Recherche-Aufträge, Mailserver-Tab, Kampagnen-Editor,
Skillbooks/Vorlagen, Audit-Download, Auth-Assist-Navigation, Locale-Wechsel, Deep-Link über `onRoute`.

## Was sich geändert hat

- **Neu `v4-bridge.js`**: einzige Stelle mit App-API-Zugriff (lokaler Spiegel `api.query/get`, `api.put/patch/remove`,
  `api.command` ohne Warten, Zustandszone, `api.shell.dialog`). `patchDocById` patcht wie früher `incrementalPatch` nur
  bestehende Dokumente; `upsertDocById` erhält `created_at_ms`.
- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; Modulzustand je Montage zurückgesetzt, Abos
  je Montage lokal (18 Collections: 6 Kampagnen/Recherche, `business_commands`, `ctox_queue_tasks`, 10 aktive
  Kommunikation). Live-Emissionen werden 60 ms gebündelt → `deriveState()` (synchron aus dem Spiegel) → `render()`.
  `loadAll()` ist nur noch ein Rückfall-Alias auf `deriveState()` (kein Nachladen, kein Netz).
- **Entfernt (R4/R12)**: Readiness-Abos, `collectionReadiness`, Reload-Schleife `scheduleDataRefresh/runDataRefresh`,
  10-s-Operational-Refresh, **2,5-s-Knowledge-Projektions-Schleife** (`startKnowledgeProjectionWatch`,
  `hydrateSelectedCampaignFromKnowledge`), **5-min-Scheduler-Intervall** (`runSchedulerTickQuiet`), Retry-Kette
  `scheduleCampaignKnowledgeSetup` (8×2,5 s), `waitForCampaignProjection` (400-ms-Poll), `withTimeout`,
  `waitForOutboundCommandProjection` (250-ms-Poll, 45 s) und die lokale Ergebnis-Projektion
  `projectOutboundCommandResult` (Handler schreiben selbst in die Collections).
- **Befehle (R5)**: alle über `sendCommand(type, payload, {record_id, client_context})` = `api.command(type,
  {...payload, record_id, client_context}, {record_id})`. Typen/Felder unverändert wie in V4-HANDLERS.md.
  `failed` → genau ein Zoneneintrag `outbound.command.<type>`, `done` → geräumt. Fachlich abhängige Ketten
  (z. B. engagement.create → assign_sender → draft.prepare, update_draft → approve, knowledge `data list` →
  `create/restore`) laufen im Hintergrund auf dem gepushten Status `done` weiter (`settled`); die UI wartet nie.
  Ergebnisse, die die UI braucht (audit.export-Download, scheduler.tick-Zusammenfassung, mailserver get_config,
  auth_assist-Startargumente), kommen aus `CommandState.result`.
- **Zustand (R11)**: Fehlerbanner `outbound-outreach-error`, Deep-Link-Statuszeile `[data-outbound-record-status]`,
  „Daten werden synchronisiert.“, Mailserver-Ladezustand + Fehlerkasten, Leertexte (13 Stellen) und „Run wird
  gestartet...“ entfernt. Zone-Codes: `outbound.command.<type>`, `outbound.outreach.error`,
  `outbound.scheduler.summary` (info), `outbound.route.record` (warn), `outbound.mailserver.input`,
  `outbound.research_settings.input`, `outbound.research_adapter.write`, `outbound.campaign.import`,
  `outbound.outreach.generate`, `outbound.write.<collection>` (lokal ungültig).
  Umbenannt (Wächter-Vokabular, Bedeutung gleich): „läuft gerade“ → „in Arbeit“, „wartet in CTOX“ → „eingereiht“,
  „Freigabe ausstehend“ → „Zur Freigabe“, Absender „Nicht verfügbar“ → „Gesperrt“, Research-Zusammenfassung
  „CTOX arbeitet: N läuft · M wartet“ → „Research: N in Arbeit · M eingereiht“.
- **Dialoge (R7)**: `showBusinessConfirm/Prompt/Alert`, `confirm()`, `prompt()`, `window.prompt` →
  `api.shell.dialog({kind:'confirm'|'prompt', …, default:false|null})`; Hinweis-Alerts → Zustandszone.
- **Shell (R6/R13)**: Titel `api.shell.setTitle('Outbound')`; Deep-Link über `api.shell.onRoute` (`record`/`record_id`,
  `return_thread_id`); Runbook öffnen → `api.shell.navigate('knowledge', {id:'runbook:<id>'})`; Business-Chat-Auftrag
  → `api.shell.navigate('ctox', detail)`; Auth-Assist-Browser → `api.shell.navigate('browser', {title, mode, args})`
  (kein vorab geöffnetes Fenster mehr); Brief-Druck über verstecktes iframe im App-root statt `window.open`.
  Icons app-lokal (9 Glyphen aus shared/icons.js, OFFEN-D).
- **i18n (R8)**: `api.shell.locale.subscribe`; Texte: `api.shell.t('outbound.<key>')` → `locales/<lang>.json`
  (relativ, ohne `?v=`) → eingebetteter Rückfall.
- **Imports (R14/R10)**: `ctox:shared/universal-importer.js`; `dialogs.js`, `i18n.js`, `?v=`-Buster, `ensureStyles`
  entfernt; `index.html` relativ geladen.
- **CSS (R16)**: `scope-css.mjs outbound … --write`: 424 Selektoren unter `[data-app="outbound"]`.
- **Nutzer-Vorlieben**: ausgeblendete Firmen je Kampagne und Spaltenverhältnis lagen in `ctx.storageScope`; jetzt
  Fensterzustand (`snapshot/restored`), OFFEN-6.

## Befehlstypen – Einstufung

Die 38 `outbound.*`-Typen der App sind in `V4-HANDLERS.md` (Strang H1) eingestuft und als Handler portiert; die UI
sendet sie mit unveränderten Typen/Feldern (34 Typen, 42 Sendestellen; `outbound.campaign.apply_setup` und
`outbound.pipeline.write_outreach_draft` stehen nur als Writeback-Vertrag im Auftrag an den Agenten).
Zusätzlich sendet die UI (Rust-Bezüge gemessen `grep -rn '"<type>"' src/core | grep .rs:`):

| Befehlstyp | Rust | Einstufung |
|---|---:|---|
| `knowledge.command` (DataFrames/Skillbook der Kampagne) | 3 | Fähigkeit `cap.knowledge.command` (V4-HANDLERS.md) |
| `ctox.mailserver.get_config/save_domain/delete_domain/save_user/delete_user` | 5/4/… | Fähigkeit `cap.mail.server` |
| `outbound.pipeline.outreach_draft` | 1 | seit SKL-O Handler mit `agent.task.enqueue` (Skill `outbound/message-drafting`), Rückruf `.done` → `write_outreach_draft` (V4-HANDLERS.md „Strang SKL-O“) |
| `outbound.campaign.setup.request` (Kampagnen-Setup) | 0 | seit SKL-O App-Befehl statt Chat-Auftrag; Rückruf `.done` → `apply_setup` (V4-HANDLERS.md „Strang SKL-O“) |

## OFFEN (nummeriert)

1. **schema.js nicht gelöscht**: inhaltlich gleich zu `collections.schema.json` (16 Collections, gleiche Versionen,
   `required` und Properties; Migration outbound_messages 1 = `set_from_first_truthy` in `migration_strategies`),
   aber `modules/conversations/schema.js:19` importiert `../outbound/schema.js` (Strang UC3). Löschen bricht
   conversations → erst nach dessen Port löschen. (Kurz gelöscht und aus der identischen Kopie
   `~/.local/state/shell-v4-src/4794af03d/…/schema.js` wiederhergestellt, sha256 = HEAD-Fassung, per lesendem
   `git show` verglichen.)
2. **Scheduler-Takt**: der 5-min-`outbound.scheduler.tick` der UI ist entfallen (keine Schleifen in Apps). Ohne
   Ersatz werden `auto_draft_pending`/`ooo_retry_at_ms` nur noch per Knopf „Scheduler-Tick“ verarbeitet → die
   Laufzeit muss den Tick zeitgesteuert auslösen (Manifest-Zeitplan/Trigger; Wunsch an „ctox sync v4“).
3. **Knowledge-Projektion**: die 2,5-s-Schleife, die Zeilen der Kampagnen-DataFrames (`knowledge.command data
   select`) in Firmen/Pipeline/Runs einmischte, ist entfallen. Sichtbar ist nur, was in den outbound-Collections
   steht (alle Handler schreiben dorthin). Rein in DataFrames geschriebene Agenten-Ergebnisse
   (`outbound.source.import` per URL, Writeback-Verträge `append_rows`) erscheinen erst, wenn eine Projektion
   DataFrame → Collection existiert (Entscheidung „ctox sync v4“, vgl. knowledge OFFEN-1). `readKnowledgeRows`
   bleibt nur für den Import-Zähler `countKnowledgeRowsForSource` (Befehlsergebnis im Hintergrund).
4. **Abhängige Befehlsketten** warten im Hintergrund auf den gepushten Status `done` (`settled`), die UI nie. Falls
   die Core-Laufzeit Befehle einer App ohnehin in Reihenfolge abarbeitet, können die Ketten später entfallen.
5. **Rückmeldung an den CTOX-Thread** (`return_thread_id`): Ereignis `ctox-business-os-record-focus` am App-root
   (bubbelt zur Shell). 4.0 hat dafür keine API; Shell-Strang muss es abnehmen oder `navigate` erweitern.
6. **Nutzer-Vorlieben** (ausgeblendete Firmen je Kampagne, Spaltenverhältnis) leben jetzt im Fensterzustand
   (snapshot/restored), nicht mehr nutzerweit dauerhaft. Dauerhaft bräuchte es eine App-Vorlieben-Collection.
7. **shared/universal-importer.js ist v3** (`ctx.db`, `ctx.sync`, `commandBus`, `?v=`-CSS, Ladetexte): outbound
   übergibt nur `{host: root}` und `dispatch:false`; der Explorer-Pfad (`showFileExplorer:false`) bleibt ungenutzt.
   Port im Shared-Strang.
8. **Mehrere Fenster**: Modulzustand ist modulweit (wie knowledge OFFEN-4); Abos und Listener sind je Montage, echte
   Mehrfenster-Fähigkeit braucht Zustand je Instanz (index.js ~8.900 Zeilen).
9. **Auth-Assist**: kein vorab geöffnetes Browserfenster mehr (4.0 ohne Fenstermanager); der Browser öffnet erst
   mit den Startargumenten aus dem Befehlsergebnis über `api.shell.navigate('browser', …)`. Ob die Browser-App
   `params.args` so annimmt, prüft Strang UD5.
10. **Manifest** (nicht geändert): `business_commands`/`ctox_queue_tasks` als Fremd-/Plattform-Collections, `handlers`,
    `capabilities_required`, `styles` – Vorschlag in V4-HANDLERS.md; `cap.mail.server` und Ketten-Typ `knowledge.command`
    fehlen dort noch.
11. **Schmal (420)**: die Filterleiste der Firmentabelle läuft rechts aus dem Fenster (Selects auf Pfeilbreite),
    vorhandenes Verhalten der CRM-Steuerleiste; nicht in diesem Port geändert.
12. **Ungenutzter Code**: Projektionsfunktionen für Knowledge-Zeilen (`projectCompaniesFromKnowledgeRows` …) sind
    ohne Aufrufer stehen geblieben (Fachlogik nicht gelöscht, bis OFFEN-3 entschieden ist).

## Wortlaut zurückgedreht (Strang LBL, 15.10.2026 15:45)

- Zurück: Research-Status je Datensatz „in Arbeit“/„eingereiht“ → **„läuft gerade“/„wartet in CTOX“** (`automationStatusLabel`) und Zähler **„läuft“/„wartet“**; Research-Zusammenfassung „Research: N in Arbeit · M eingereiht“ → **„CTOX arbeitet: N läuft · M wartet“** bzw. **„CTOX arbeitet gerade nicht: N wartet“** (EN „CTOX is working: N running · M waiting“ / „CTOX is currently idle: N waiting“); Freigabestatus „Zur Freigabe“ → **„Freigabe ausstehend“** (EN „Awaiting approval“); Absender „Gesperrt“ → **„Nicht verfügbar“** (`senderUnavailable`, EN „Unavailable“; `senderBlocked` „Gesperrt“ war upstream so); Sequenzhinweis „nutzt“ → **„läuft mit“** Sequenz-Version (Locale wie upstream „laeuft“, `verfuegbar` ebenfalls in ASCII wie upstream).
- Locale-Schlüssel `waiting` existierte upstream nicht (Rückfall „wartet“); jetzt DE „wartet“, EN „waiting“ statt „eingereiht“/„queued“.
- Test: `tests/v4-api.test.mjs` entfernt diese Fachbezeichnungen (`FACH_LABELS`) vor der Zustandstext-Prüfung, `STATE_TEXT` selbst unverändert.
- Original = `upstream-main`. Markierung `v4-allow: label` (JS: Kommentar in derselben Zeile, HTML: `<!-- v4-allow: label -->` davor, JSON: Markerschlüssel `"//<schlüssel>": "v4-allow: label"` davor) nur an Zeilen, die der Wächter sonst meldet. Prüfung: v4-api-Test gegen Stub-Pin cff5ba5 grün, `scan.mjs --app <id>` unverändert, guard-v4 ohne Treffer außerhalb `handlers/` (9/9; scan 1 = schema.js).


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G3: Research-Zähler und -Zusammenfassung („1 wartet“, „CTOX arbeitet gerade nicht: …“) sind laut Strang LBL Fachbezeichnungen: Container tragen `data-v4-allow-label`; Null-Zähler („0 läuft“, „0 Fehler“) entfallen ganz.
- G1: Funnel-Reiter und Werkzeugzeile brechen um (Kit-`min-width: max-content` aufgehoben).
- G4: Campaign-Name und Metazeile brechen um.
- Offen (Entscheidung nötig): die CRM-Tabelle (`table-layout: fixed`, Spaltenbreiten je Research-Feld) scrollt in ihrem eigenen Container seitlich (1382 px). Passend machen hieße Spalten ausblenden oder Kartenmodus – das ist eine Produktentscheidung, kein Darstellungsfix.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app outbound --shared` (Stub-Pin 9fa9948) → G3/G4 grün; G1 nur noch die CRM-Tabelle; `node --test modules/outbound/tests/v4-api.test.mjs` grün; `scan.mjs --app outbound` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

## Nachtrag Strang VIS2 (15.10.2026)

- Entscheidung umgesetzt: die CRM-Tabelle (1382 px) scrollt in ihrem eigenen Container `.outbound-table-scroll-unified`, der jetzt `data-scroll-x` trägt (Labor-Ausnahme G1, Strang GEO2). Fenster und App-Fläche scrollen nie seitlich.
- Spaltenköpfe sticky: `.crm-table` hatte `overflow: hidden` und war damit selbst der Bezugsrahmen für `position: sticky` – die Köpfe liefen beim Scrollen mit weg (gemessen 420 px: scrollTop 22 → Kopf −21 px). Jetzt `overflow: clip` (rundet weiter ab, ist aber kein Scroll-Container) → Kopf bleibt bei 0.

Prüfung: Labor `--shared` (Stub-Pin 9fa9948) → G1 grün, nur noch „G1 erlaubt: … [data-scroll-x]“; `node --test modules/outbound/tests/v4-api.test.mjs` 9/9, `outbound.test.mjs` 24/24.
