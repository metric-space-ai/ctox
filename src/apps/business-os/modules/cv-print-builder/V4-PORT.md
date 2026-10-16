# cv-print-builder – Port auf App-API 4.0 (Strang UD5, Welle D)

Fortschritt (gemessen, `TZ=Europe/Berlin date`): 15:28 Start nach browser · 15:30 Scanner vorher **66 Stellen**, CSS
246 Selektoren ungescoped (13 mit Shell-Vorfahr) · 15:45 index.js portiert, Scanner 0, schema.js gelöscht, CSS gescoped ·
15:50 Stub-Pin auf 9fa9948 umgestellt (Brief geändert), v4-Test 10/10 · 15:53 Bildschirmfotos angesehen, ein
Layoutfehler behoben · 15:55 Abschluss.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app cv-print-builder` | **0 Stellen**, Exit 0 (vorher 66 + CSS 246 ungescoped) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs cv-print-builder modules/cv-print-builder/index.css --check` | gescoped |
| guard-v4 | `node v4/tools/guards/guard-v4.mjs --json` | **0 Treffer** in `modules/cv-print-builder/` (vorher 57 in index.js, 6 in schema.js, 1 in locales/en.json); Testdatei: 1 vorbestehender `registry.json`-Bezug (zählt getrennt) |
| v4-Test gegen Kern-Stub | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948 node --test src/apps/business-os/modules/cv-print-builder/tests/v4-api.test.mjs` | **10/10 grün** (Chromium headless 1223, storage:memory) |
| Bildschirmfotos | `SHOTS=1 …v4-api.test.mjs` | `tests/shots/cv-print-builder-1280.png`, `tests/shots/cv-print-builder-420.png` |
| Quelltext-Wächter | `node modules/cv-print-builder/tests/cv-print-builder.test.mjs` | grün, auf v4-Vertrag umgestellt (nicht gelöscht) |
| Handler-Tests (Strang H8, unverändert) | `node --test modules/cv-print-builder/handlers/tests/*.test.mjs` | 5/5 grün |

Abgedeckt vom v4-Test: mount zeigt nur `document_type = cv_print_profile`, wählt das jüngste Profil, Titel über
`api.shell.setTitle`, 4 Live-Abos je 1 · Original-PDF über `api.files.get(ref)` als `blob:`-iframe, kein Platzhaltertext ·
`sim.push` erscheint live (Bandzähler), `_deleted` verschwindet · leere Collections: Liste, Bühne, Steuerzeile leer, kein
`.ctox-empty/.ctox-syncing/[role=status]`, keine Zone · „Parsing starten“ → `ctox.cv_print.apply_parse` liegt **sofort**
lokal (kein Warten), `record_id` als Befehls-Option, Payload mit `source_file_ref`/`source_file{kind:'file'}`, keine
`source_file_id` bei neuen Profilen, Version auf `parsing` mit `command_id = h.id`; `sim.failCommand` → genau ein
Zoneneintrag `cv-print-builder.command.ctox.cv_print.apply_parse` (`ref` = Befehls-ID), Phase „Fehler“ aus der
Live-Projektion; keine Dokumente in `business_chats`/`desktop_files`/`desktop_file_chunks` · `done` mit
`{task_id}` → Zone geräumt, „Flow öffnen“ → `api.shell.navigate('ctox', {task_id, command_id, source_module})` (Route
kommt in der ctox-App an) · Editorfeld blur → `api.patch` auf `document_versions` + `documents` (`accepted`) ·
Freigeben → `approved`, Drucken → verstecktes iframe **im App-root** mit nur dem Druckbogen · „Alle reparsen“ ohne
Antwort → `api.shell.dialog({kind:'confirm', default:false})`, kein Befehl · snapshot → structured clone → Remount:
offene Editor-Eingabe (auch über ein Live-Neuzeichnen hinweg), Auswahl, Suche, Band; Abos wieder 1 · unmount: alle
Abos 0, root leer, keine `pageerror`.
**Nicht** abgedeckt: PDF-Import über den Dateiwähler (Datei-Dialog), JSON-Import/-Export, Logo-Upload,
Locale-Wechsel, tatsächlicher Druckdialog, ältere Profile mit `desktop_file_chunks` (Lesepfad nur im Quelltext-Wächter).

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; `ensureStyles`/`?v=`-Buster/`BUILD` entfallen,
  Markup über `fetch(new URL('./index.html', import.meta.url))`; `autoWirePaneGrammar(root)` ruft die App selbst;
  Titel `api.shell.setTitle('CV Print Builder')`; Zustand je Montage, alle Listener/Abos in `state.disposers`.
- **Daten (R3/R4)**: Live-Abos auf `documents`, `document_versions`, `business_commands`, `ctox_queue_tasks`
  (entprellt 80 ms); `refresh` liest synchron über `api.query(…).get()`. Entfallen ersatzlos: Readiness
  (`collectionReadiness`, `waitForRequiredCollectionsReady`, `scheduleReadinessRetry`, `state.ready`-Sperren),
  Sync-Wärmen/-Flush/-Leases, Demand-Loader, `requireCollections`. Schreiben: `api.put`/`api.patch` mit Warten nur
  auf den lokalen Commit (`h.local`).
- **Dateien über `api.files` statt fremder Collections** (Brief): Ein importiertes PDF geht als
  `api.files.put(file, {id:'cv_file_<doc>', name, mime})`; das Profil trägt `source.file_id` + `source.file_ref`
  (sha256), `document_versions.blob_id = ref`, `documents.linked_records = [{collection:'files', id, ref,
  role:'source_pdf'}]`. **Kein** Schreiben mehr in `desktop_files`/`desktop_file_chunks` (v3: Dateidokument + base64-
  Chunks mit Hashes). Original-Ansicht: `api.files.get(ref)` → Blob → Objekt-URL. Ältere Profile (nur
  `desktop_file_id`) werden weiter gelesen: lokale Abfrage `desktop_file_chunks {file_id}` +
  `readStoredFileFromChunks` (`ctox:shared/file-integrity.js`), Hash-Prüfung wie bisher.
- **Befehl (R5, Brief)**: statt `business_os.chat.task` mit `writeback_contract` jetzt
  `api.command('ctox.cv_print.apply_parse', payload, {record_id})` (Handler fertig, H8). Payload: `title`,
  `instruction` (unverändert `buildParserTaskInstruction`, plus Zeile `source_file_ref`), `chat_id`, `skill`,
  `document_id`, `version_id`, `record_snapshot`, `expected_model_schema`, Dateiangaben (neu: `source_file_ref` +
  `source_file{kind:'file', ref, file_id, name, mime_type}`; alt: `source_file_id` + `generation_id`). Nie warten:
  Profil sofort auf `parsing` mit `command_id = h.id`; `failed` → Zone, `done` → clear; lokale Ablehnung (`h.local`) →
  Zone + Profil `error/dispatch_failed` (wie v3 bei Versandfehler). Entfallen: `upsertBusinessChat` (Chat-Dokument in
  der fremden Collection `business_chats`), `dispatchCvParserCommand` mit 90-s-Warten und „pending_sync“-Rückfall,
  `ensureParseSourceReady` (60-s-Sync-Vorbereitung), `sync_collections`, `source_prepare`.
- **Live-Status**: `isCvPrintParseCommandProjection` erkennt das v4-Befehlsdokument (`app_id`, `type`);
  `applyLiveWorkflowState` liest `task_id` zusätzlich aus `result.task_id` (Handler-Rückgabe) und die Befehls-ID aus
  `id`. v3-Projektionen bleiben lesbar.
- **Shell (R6/R7/R13)**: `window.confirm` → `api.shell.dialog({kind:'confirm', …, default:false})`; „Flow öffnen“ →
  `api.shell.navigate('ctox', {task_id, command_id, source_module})` statt `sessionStorage` + `parent.location.hash`;
  `window.print()` → `printApprovedSheet`: verstecktes iframe im App-root mit den Stylesheets des Dokuments und nur
  dem Druckbogen (`@media print` entsprechend, kein `body *`-Verstecken der Shell mehr).
- **Zustandstexte (R11)**: entfernt „Daten werden synchronisiert.“ (`.ctox-syncing`), „Keine Kandidaten für diesen
  Filter.“, „Noch keine CVs…“, „Kein CV ausgewählt…“, „CV-Bühne ausgeblendet.“ (Name + „Anzeigen“-Knopf bleiben),
  „PDF wird geladen.“, „Original-PDF ist noch nicht lokal materialisiert.“, Fehlerkasten im Original-Bereich,
  Fußnote „PDF wird an CTOX synchronisiert…“ (`task_status 'syncing'` entsteht in v4 nicht mehr) und „Task läuft.
  Flow wird angelegt.“ (ohne ID gibt es nichts anzuzeigen), alle Erfolgsmeldungen (PDF/Profile importiert/exportiert,
  Parser-Tasks gestartet, CV-Daten synchronisiert) und die „Synchronisierung läuft“-Sperrhinweise. In die Zone
  (`api.status.report`, Code `cv-print-builder.<bereich>`): `import.pdf`, `import.profiles`, `export`, `action`,
  `original`, `file.upload` (Core lehnt Upload ab), `reparse` (info: keine PDFs; error: Teilfehler), `write`,
  `command.ctox.cv_print.apply_parse`; Erfolg räumt den eigenen Code.
  Mit `// v4-allow: label` gekennzeichnet (fachliche Datensatz-Phase, kein App-Zustand): Band „In Arbeit / In
  progress“ (Code + Locale), Phasenlabel „Parsing läuft“, Fußnote „Task läuft · Flow öffnen“. In `locales/*.json`
  steht dafür je eine Markierungszeile `_bandProgress_v4` (JSON hat keine Kommentare; Schlüssel in de/en gleich).
- **i18n (R8)**: Sprache aus `api.shell.locale` (live); `applyStaticLocale` übersetzt in beide Richtungen.
- **Fensterzustand (snapshot v1)**: `selectedId`, `selectedPhase`, `viewMode`, `userCollapsed`, Listenzustand
  (`search`, `band`, `sort`, `template`, `view`), `editorDraft {documentId, fields}`. Offene Editor-Eingaben werden je
  Feld bei `input` gemerkt und beim `focusout` (= Speichern per blur, unverändert) freigegeben; sie überleben dadurch
  auch ein Live-Neuzeichnen der Bühne (v3 verlor sie bei jeder Änderung einer Live-Collection – Verhalten nur
  insofern geändert, als ungespeicherte Eingaben nicht mehr verschwinden).
- **CSS (R16)**: `scope-css --write` (246 Selektoren, 13 Shell-Vorfahren ersetzt); root ist selbst
  `container: business-app-window` (Haltepunkte 1024/768 greifen im Inhaltsbereich, wie browser).
  **Layoutfehler aus dem Bildschirmfoto behoben**: die Kandidatenspalte (344 px) hatte keine explizite Spur; die
  Reiterleiste (`min-width: max-content`, 345 px + 24 px) weitete sie auf 369 px, Karten, Kopf-Icons und Reiter wurden
  rechts abgeschnitten (gemessen per Playwright) → `grid-template-columns: minmax(0, 1fr)`, Reiterleiste scrollt.
- **schema.js gelöscht**: Es exportierte nur Schemas anderer Apps (`ctox`, `desktop`, `documents`) und war nicht
  ladbar (`ERR_MODULE_NOT_FOUND …/modules/desktop/schema.js`, siehe `v4/manifests/cv-print-builder.manifest.json`
  `_todo`); ein Gleichwertigkeitsvergleich ist deshalb nicht messbar. `collections.schema.json` enthält dieselben 5
  Collections (`business_chats`, `desktop_file_chunks`, `desktop_files`, `document_versions`, `documents`).
- **Quelltext-Wächter `tests/cv-print-builder.test.mjs`**: v3-Zusicherungen durch v4-Entsprechungen ersetzt
  (Befehlsweg, `api.files`, keine Fremdschreibzugriffe, keine Sync-/Readiness-/Timeout-Ketten, Dialog/Druck/Route im
  Fenster, Markup ohne Buster, keine Zustandstexte); Fachprüfungen (Templates, Druckbogen, Editor, Prototyp-Schutz,
  Parser-Anweisung, Band, Kachel/Liste, Auswahl ohne Neuaufbau) unverändert.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen mit `grep -rn '"<typ>"' src/core --include='*.rs' | wc -l` im Klon (15:55).

| Befehlstyp | Rust | Einstufung | Stand |
|---|---:|---|---|
| `ctox.cv_print.apply_parse` | 7 | GEMISCHT: Fachlogik (Prompt, Rückschreiben) + Fähigkeiten `cap.agent.task.enqueue`, `cap.doc.extract` | Handler portiert (H8); **UI sendet ihn jetzt direkt** |
| `business_os.chat.task` | 154 | Core-generisch (PLATTFORM) | wird von der App nicht mehr gesendet |

## OFFEN (nummeriert)

1. **Handler liest die Datei-Ref noch nicht (an Strang H8)**: `handlers/ctox.cv_print.apply_parse.js` kennt nur
   `desktop_files`: `sourceFileId()` findet bei neuen Profilen `source_file.file_id = 'cv_file_<doc>'`, liest
   `ctx.get('desktop_files', …)` (→ null, Größenprüfung entfällt) und ruft `cap.doc.extract({source:{kind:
   'desktop_file', file_id}})` – das trifft keine Datei. Nötig: bei `payload.source_file_ref` bzw.
   `source_file.kind === 'file'` → `cap.doc.extract({source:{kind:'file', ref}, …})`, Größe aus dem `files`-Dokument.
   Bis dahin endet Parsing neuer Profile als `failed` (Zone + Phase „Fehler“); ältere Profile (`source_file_id`)
   laufen unverändert.
2. **Manifest (Generator-Strang)**: `desktop_files` nur noch `read` (statt `read, write`), `business_chats` entfällt,
   `files` (Plattform, über `api.files`) ergänzen; `desktop_file_chunks` `read` nur für ältere Profile;
   `command_rights` für `ctox.cv_print.apply_parse`.
3. **Ältere Profile**: Original-PDF nur, wenn die `desktop_file_chunks` lokal vorliegen (v3 zog sie über den
   Demand-Loader nach). Fehlt der Inhalt → Zone `cv-print-builder.original`. Entscheidung Kern: Chunks als normale
   Abfrage verfügbar machen oder die Altbestände nach `files` migrieren.
4. **Chat-Verlauf**: v3 legte für jedes Parsing ein `business_chats`-Dokument (`chat_cv_<doc>`) mit der Anweisung an.
   Der Handler gibt `chat_id` als `thread_key` an die Agenten-Queue; ob der Kern dazu einen Chat-Eintrag erzeugt, ist
   offen (Fachentscheidung).
5. **Logo** liegt weiter als Data-URL im Profil (`print.logoDataUrl`, v3-Verhalten). Kandidat für `api.files`, nicht
   geändert (Fachverhalten).
6. **Druck**: iframe-Druck verifiziert bis „Bogen im versteckten iframe im App-root“; der echte Druckdialog ist im
   Headless-Test nicht prüfbar. Das iframe übernimmt die Stylesheets des Dokuments (Shell lädt `index.css` aus dem
   Manifest).
7. **Geometrie**: 1280: Reiterleiste (345 px, Kit-Element `.ctox-pane-tabs` mit `min-width: max-content`) ist breiter
   als der Platz in der Spalte (320 px) und scrollt jetzt waagrecht („Freigegeben (0)“ angeschnitten). 420: Spaltentitel „Kandidaten“ → „Kan…“ neben vier 44-px-Tippflächen und der
   80-px-Iconzone; liegt unter `minimum_size.width` 640 des Manifests. Beides vorbestehendes Kit-/Mobilverhalten.
8. **Mehrere Fenster**: `ACTIVE_LOCALE` ist modulweit (wie knowledge OFFEN-4); Manifest `multi_instance: false`.
9. **Quelltext-Wächter** liest weiter `../registry.json` (Manifest/Katalog-Abgleich) → guard-Testtreffer
   `ref-legacy-catalog-file`; fällt mit dem Katalog-Strang.
10. Kein git benutzt; Originale lesend gesichert im Sitzungs-Scratchpad.
