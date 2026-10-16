# research – Port auf App-API 4.0 (Strang UD8, Welle D)

Auftrag: `~/.local/state/shell-v4/docs/ui-port-brief.md`; Vorlage `v4/tools/port/PORTING.md`, Pilot `modules/knowledge`.
Stub-Pin: `~/.local/state/shell-v4/stub-pins/cff5ba5` (bis 14:40 gegen 67229d2; Tests auf cff5ba5 umgestellt und erneut grün). `handlers/**`, `module.json`, `collections.schema.json` unberührt.

## Fortschritt (Berlin-Zeit, gemessen mit `date`)
- 13:32 Start, Brief + PORTING.md + app-api.d.ts + knowledge-Pilot + V4-HANDLERS.md gelesen.
- 13:32 Scanner vorher: **97 Stellen**, CSS 466 Selektoren ungescoped (23 mit Shell-Vorfahr). guard-v4 vorher: 97 Treffer in modules/research (ohne handlers/).

- 14:13 Bereich 1–3 fertig (index.js): Scanner index.js 0 Stellen; CSS mit scope-css.mjs gescoped (--check: gescoped).
- 14:14 schema.js gelöscht (gleichwertig zu collections.schema.json, s. u.); index.html/locales ohne Zustandstexte.
- 14:15 **Scanner 97 → 0 Stellen (Exit 0); guard-v4 97 → 0 Treffer** in modules/research (ohne handlers/).

- 14:17 `tests/v4-api.test.mjs` gegen Stub-Pin 67229d2: **8/8 grün**.

- 14:2x Screenshots 1280/420 (+ Dialog) gegen den Stub nach `tests/shots/` (`tests/v4-shots.mjs`), selbst angesehen:
  Layout ohne Überlauf (scrollWidth = Breite). Befund behoben: Schalter Liste/Karten war leer (Glyphen `list`/`grid`
  kamen in v3 aus `ctx.getActionIcon`) → app-lokale Pfade ergänzt (OFFEN-D).

## Stellen (Scanner vorher, je Regel)
R1-mount 1 · R2-host 16 · R3-db 12 · R4-sync 24 · R5-command 4 · R6-shell 3 · R8-i18n 3 · R9-actor 4 · R10-http 2 ·
R11-state 12 · R12-timer 9 · R13-nav 3 · R14-import 2 · R15-guard 1 · R16-css 1 · R17-schema 1 (schema.js).

## Bereiche (Reihenfolge der Arbeit)
1. Einstieg/Datenschicht (mount, Abos, Sync-Kopplung, Retry/Reprobe raus)
2. Befehle + Navigation + Shell (api.command, navigate, Drawer im root, Dialog)
3. Zustandstexte (Inhalt, index.html, locales) → api.status.report
4. CSS-Scope, schema.js
5. v4-Test, Screenshots, Legacy-Suite

- 15:04 Wiederaufnahme nach Sitzungslimit (Lauf ~14:40 abgebrochen). Stand gemessen: Scanner 0, guard 0, v4-Test 9/9
  gegen cff5ba5; offen waren Legacy-Suite und Abschluss.
- 15:1x Legacy-Suite `test.mjs` repariert und grün (2 Fehler aus dem abgebrochenen Lauf):
  - `onclick="event.stopPropagation()"` am externen Quellen-Link der Karte → Delegation im root-Klick-Handler
    (`.research-source-card-open` öffnet nur den Link, wählt die Karte nicht; Verhalten wie v3, ohne Inline-Handler).
  - Test „catalog tables never carry rows…“ prüfte `chunk_count` auf der falschen Ebene; v3-Verhalten (Chunk-Felder
    nur aus `payload` entfernt) unverändert, Test auf `catalog.payload.chunk_count` korrigiert.
  - toter Zweig `handleGraphCommand('retry')` entfernt (kein Knopf rendert ihn mehr); veralteter getActionIcon-Kommentar.
  - esbuild-Rückfall auf `~/Documents/ctox/src/apps/business-os/node_modules` + `ctox:shared/`-Plugin (Muster outbound).
- 15:1x Screenshots gegen cff5ba5 neu erzeugt und angesehen (s. Messwerte).

## Status: FERTIG (Abnahme gegen Stub-Pin cff5ba5)

## Messwerte (Abschluss)
| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Scanner | `node v4/tools/port/scan.mjs --app research` | 97 → **0 Stellen**, CSS ungescoped 0 |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json`, gefiltert auf modules/research ohne handlers/ | 97 → **0 Treffer** (3 Testtreffer separat: Testnamen/QA-Skript, keine UI) |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test modules/research/tests/v4-api.test.mjs` | **9/9 grün** |
| Legacy-Suite | `cd modules/research && node --test test.mjs tests/research-graph-data.test.mjs` | **68/68 grün** (57 + 11) |
| Screenshots | `CTOX_APP_API_STUB=… node modules/research/tests/v4-shots.mjs` | `tests/shots/research-1280.png`, `-420.png`, `-1280-dialog.png`, `-420-dialog.png`; scrollWidth = Breite (1280/420), kein Element über den Fensterrand; Dialog liegt im App-Fenster |
| Handler | nicht angefasst (Strang H6) | – |

Nicht gelaufen: `tests/research-graph-browser-qa.mjs` (Playwright-QA gegen die alte v3-Shell-Seite, kein v4-Pfad;
nicht umgestellt).

## Entscheidungen
- Laufzustand nur live: `research_tasks`, `research_runs`, `business_commands`, `ctox_queue_tasks` per Abo; jede
  Änderung rechnet die Sicht nach (80-ms-Bündelung, kein Polling). `leaseCollection`, `reprobeFailedSyncCollections`,
  `refreshAllNow`, Leer-Retry der Knowledge-Tabellen, Zeilen-Retry-Knopf und Readiness-Probe entfernt.
- Befehle `research.systematic.run`, `research.knowledge.refresh`, `research.systematic.report.create` über
  `api.command` ohne Warten; `failed` → genau ein Eintrag in der Zustandszone (`research.command.<typ>`), `done` →
  Eintrag gelöscht, Erfolg still. Blockierte Starts (`research.run.blocked`, `research.knowledge.blocked`) als `warn`.
- Wie v3 schreibt die UI beim Start den `research_runs`-Kopf (`queued`) und setzt die Aufgabe auf `collecting`; das
  Ergebnis kommt asynchron über den Abschluss-Handler `research.systematic.run.complete` in dieselben Collections.
- schema.js gelöscht: inhaltlich gleichwertig zu `collections.schema.json` (research_tasks, research_runs,
  research_notes, documents, document_versions, document_blob_chunks, knowledge_tables, business_chats).
- Overlays, Dialoge, Drawer und Kontextmenü hängen in `[data-research-root]` im App-root; CSS unter
  `[data-app="research"]` (scope-css.mjs --check: gescoped).
- Icons app-lokal (`RESEARCH_ICON_FALLBACK_PATHS`), da 4.0 keine Icon-API hat.

## OFFEN
1. **knowledge_table_rows** (Katalog-Zeilen aus Parquet): gelesen wie im knowledge-Pilot über die Collection
   `knowledge_table_rows`; nicht in `collections.schema.json` von research. Das Manifest (Generator v4/manifests)
   muss research Lesezugriff auf `knowledge_table_rows`, `business_commands` und `ctox_queue_tasks` (fremd, read)
   geben – sonst bleiben Katalog-Tabellen und Laufzustand leer.
2. **Befehls-ID**: v3 vergab `command_id` selbst und band sie in Anweisung/Lineage. In 4.0 vergibt `api.command` die
   ID (`command(type, payload)` ohne ID-Option); die App führt die eigene Kennung `cmd_<uuid>` als
   `research_command_id` im Payload und in der Agenten-Anweisung. **Geprüft 15:2x:** der Abschluss-Handler
   (`handlers/_lib/systematic.js:54,101,186,413`) vergleicht Quittung/Manifest/Zeilen gegen `command_id` des
   Befehls (= `api.command`-ID). Damit passt die Lineage nie, und jede Quittung würde verworfen. Lösung braucht
   eines von: (a) `api.command(type, payload, {id})` im Vertrag, oder (b) Handler/`run`-Aufnahme nimmt
   `payload.research_command_id` als Lineage-Kennung. Gehört Strang H6 bzw. dem Vertrag – in der UI nicht lösbar.
3. **Chat-Übergabe entfällt (Fachverhalten-Abweichung)**: v3 öffnete bei vorhandener Chat-Fläche
   `ctx.openBusinessChat` (Bediener sieht und sendet den Prompt selbst) und nutzte den direkten Befehlsweg nur ohne
   Chat. 4.0 hat keine Chat-Übergabe-API; es gilt jetzt immer der direkte Weg. Owner-Entscheid/Vertragswunsch:
   `api.shell.openChat(spec)` o. ä.
4. OFFEN-B (Schlüsselraum `api.shell.t`), OFFEN-D (keine Drawer-/Icon-API), OFFEN-F (`ctox:shared/` nur über den
   Service Worker) aus `v4/tools/port/app-api.d.ts` betreffen research wie alle Apps.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G3: unbekannte Rohstatus aus den Daten (z. B. „loading“) erscheinen nicht mehr roh; `statusLabel` kennt nur die sieben Laufarten, sonst „Kein aktiver Lauf“ (deutsche Rückfälle).
- G4: Aufgaben-/Knowledge-Listentitel brechen um.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app research --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/research/tests/v4-api.test.mjs` grün; `scan.mjs --app research` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
