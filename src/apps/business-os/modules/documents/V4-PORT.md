# documents – Port auf App-API 4.0 (Strang UD4, Welle D)

Fortschritt (gemessen `TZ=Europe/Berlin date`):
- 13:39 Start; Brief, PORTING.md, app-api.d.ts, Stub-README (Pin 67229d2), Pilot knowledge gelesen.
- 13:39 Scanner vorher: **141 Stellen** (R1 1, R2 24, R3 30, R4 9, R5 3, R6 20, R7 2, R8 3, R9 5, R10 4, R11 25,
  R12 7, R13 3, R14 4, R17 1), CSS 383 Selektoren ungescoped (13 mit Shell-Vorfahr). guard-v4: 81 Treffer.
- 14:14 index.js/Brücke/HTML/Locales/CSS portiert: Scanner 0, guard-v4 0.
- 14:2x v4-Test 12/12 grün (erster Lauf); Screenshot 1280 zeigte gestreckte Listenzeilen → CSS-Spezifität korrigiert
  (siehe Entscheidung 9), erneut 12/12 grün, Screenshots angesehen.
- 15:04 Wiederaufnahme nach Sitzungsabbruch: Stand vollständig (keine halben Edits), Scanner 0, guard-v4 0, scope-css gescoped;
  v4-Test gegen Pin **cff5ba5** 12/12 grün. Test-Standardpin auf cff5ba5 gesetzt. Test-Seite setzt
  `--shell-v2-icon-size: 28px` auf das Fenster (Wert aus `v4/frame/frame.css .shell-window`); vorher galt der
  v2-Rückfall 80px und die Screenshots zeigten 94px statt 42px linken Kopfeinzug. Screenshots neu, angesehen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app documents` | **0 Stellen**, Exit 0 (vorher 141) |
| Löschliste-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/documents` | total **0** (vorher 81) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs documents modules/documents/index.css --check` | gescoped |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/documents/tests/v4-api.test.mjs` | **12/12 grün** (15:06/15:10, Pin cff5ba5; vorher 67229d2 ebenfalls grün), keine pageerror |
| Screenshots | wie oben mit `CTOX_DOCUMENTS_SHOTS=1` | `tests/shots/documents-1280.png`, `documents-420.png` (angesehen) |
| schema.js | Tiefenvergleich (Schlüssel sortiert) gegen collections.schema.json | 7/7 Collections gleich, Migrationen beide leer → **gelöscht** |

Abgedeckt vom v4-Test: mount zeigt Live-Daten (Titel über `api.shell.setTitle`, `data-app`, Markdown-Version im
Editor) · `sim.push` erscheint live, `_deleted` verschwindet · leere Collections: Liste und Editor ohne Text, keine Zone ·
Schreiben: Markdown-Bearbeitung → `api.patch` auf `document_versions` + `documents` · Import über den Import-Drawer:
`api.files.put` (files-Dokument `id = blob_id`, `ref = sha256`, `owner_app = documents`), Version + Datensatz per
`api.put`, Bytes über `files.get(ref)` lesbar · Befehl (Import mit Runbook → `research.systematic.report.create`) legt
`business_commands`-Dokument an, `sim.failCommand` → genau ein Zoneneintrag `documents.command.<type>` · Löschen über
`api.shell.dialog` (default:false → nichts; mit Ja → `is_deleted`) · snapshot → structuredClone → Remount mit
`restored` erhält Auswahl und offenen Markdown-Entwurf · Deep-Link unbekannter Datensatz: kein Kasten im Inhalt ·
Office-Brücke: `prepare` löst über gepushten Status auf, `failed` lehnt ab, `loadVersion` liest Bytes über `api.files` ·
unmount löst alle 7 Abos und leert root.
**Nicht** abgedeckt: Word-Editor (vendor/ctox-office iframe) und SuperDoc-Legacy-Engine im Browser (brauchen die
Office-Laufzeit), Serienbrief-Navigator, Kontext-Chat (Code ist tot, s. u.), Locale-Wechsel, Geometrie-Labor.

## Bereiche und Entscheidungen

1. **Einstieg/Shell**: `mount(root, api, {restored})` → `{unmount, snapshot}`; je Montage eigener `state` mit
   `disposers` (kein modulweiter Zustand). `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`;
   `ensureStyles`/`revisionedModuleAssetUrl` entfernt (CSS lädt die Shell). `setTitle('Dokumente')`. Linke/rechte
   Shell-Panes (`ctx.left/right`) entfallen – die App ist schon zweispaltig in root.
2. **Texte**: `loadModuleMessages` (HTTP) → `api.shell.t('documents.<key>')`, sonst eingebetteter Rückfall;
   `api.shell.locale` live. Locales: 21 Zustands-/Leertexte entfernt (u. a. `listLoading`, `loadingDocument`,
   `documentPreparing`, `officeEditorRetry`, `taskCreating`, `chatOpening`, `documentSwitchRunning`).
3. **Lesen**: Live-Abos `documents` (sort updated_at_ms desc), `document_runbooks` (title asc), `knowledge_items/
   _runbooks/_tables` (fremd, nur lesen), `ctox_runtime_settings/runtime-settings` (`api.get`, Office-Engine-Wahl),
   dazu **ein** Abo `document_versions {document_id: <Auswahl>}` je Auswahl. 80 ms gebündeltes Rendern statt
   `createCoalescedRefresh` (office-engine). Versionen werden synchron mit `api.get(...).get()` gelesen und als
   `structuredClone` gehalten (Live-Zeilen sind eingefroren, der Editor mutiert seine Kopie). Fehlt die Version lokal,
   bleibt der Editor leer und sie erscheint live; Meldung `documents.version.missing` nur, wenn `metrics.backlog = 0`.
   Entfernt: `ctx.db`, `collectionReadiness`/`subscribeCollectionReadiness`, `startCollection('document_versions',
   {forceDirect})`, `waitForOpenPeerId`, `resolveLocalFirst`, Lesefristen 4,5 s/60 s, Listen-Zustände loading/
   offline-pending/error.
4. **Schreiben**: `insert/incrementalPatch` → `api.put/patch(...).local` (lokal sofort, Core über Outbox);
   `patchIfPresent` hält die alte Semantik „nur vorhandenes Dokument patchen“. Seed-Runbooks per `api.put`.
5. **Dateien**: ausschließlich `api.files`. `blob_id` bleibt der Name im Datenmodell und ist die **id des
   files-Dokuments** (`files.put(blob, {id: blob_id, name, mime})`); Lesen über `files/<blob_id>.ref` →
   `files.get(ref)` (Rückfall: sha256 aus der Version, mit Hashprüfung). Neu im Versionsdokument: `source_sha256`
   (Feld war im Datensatz schon vorhanden; Schema hat `additionalProperties: true`). `document_blob_chunks`, Leases,
   Demand-File-Loader und `bulkUpsert`-Chunking entfallen. Überholte Entwurfsblobs: `api.remove('files', id)` statt
   Chunk-Löschung.
6. **Office-Brücke** `office-bridge-v4.js` (app-lokal, ersetzt `office-engine/src/business-os-bridge.mjs` für diese
   App; gleiche RPC-Schnittstelle für vendor/ctox-office): `office.document.{prepare,commit,export,
   freeze_email_content}` als `api.command(type, payload, {record_id})`. Der Editor braucht das Ergebnis, daher löst die
   Brücke über den **gepushten** `status` auf (`done` → `result`, `failed` → Fehler) – kein Polling, keine Frist, keine
   Wiederholung; offene Abos löst `dispose()` bei unmount. Integritätsfehler → Zustandszone `documents.office.integrity`.
7. **Befehle** (`sendDocumentsCommand`): nie warten; `failed`/lokal ungültig → `documents.command.<type>`, `done` →
   clear. `ctox.knowledge.runbooks.manage` behält `module: 'ctox'`. Sprung in die CTOX-App über
   `api.shell.navigate('ctox', {focus:'knowledge-runbooks', command_id})` statt `location.hash` + `sessionStorage`.
   Die `focusTask`-Übergabe nach „Neues Dokument“ (sessionStorage) entfällt (OFFEN-6).
8. **Shell-Ersatz**: Drawer im root (`.documents-app-drawer`, OFFEN-D), `closeDrawers`/`openDrawer` app-lokal,
   MutationObserver beobachtet root statt `document.body`. Bestätigung über `api.shell.dialog({kind:'confirm', …,
   default:false})`. Benachrichtigungen → Zustandszone; Erfolg bleibt still. `renderError` schreibt nicht mehr in den
   Inhalt, sondern `api.status.report` (Codes `documents.editor`, `documents.editor.office`, `documents.draft.save`,
   `documents.draft.pending`, `documents.file.open`, `documents.import.unsupported`, `documents.document.create`,
   `documents.route.unknown_record`, `documents.version.missing`, `documents.read.<collection>`,
   `documents.runbooks.seed`, `documents.render`); die „Erneut versuchen“-Knöpfe im Inhalt entfallen (erneuter Klick
   auf das Dokument). Deep-Link über `api.shell.onRoute` (`record|documentId`, `version|versionId`,
   `return_thread_id`, `file_ref|ref`) statt eventBus `desktop-app:open-file` und `ctox-business-os-app-launch`.
   Close-Guard des Fenstermanagers entfällt (4.0 hat keinen); `beforeunload`-Schutz bleibt.
9. **CSS**: `scope-css --write`; zwei Werkzeugfolgen korrigiert: (a) `:root[data-theme="dark"] .x` wurde zu
   `[data-app][data-theme="dark"] .x` (trifft nie, Attribut sitzt an html) → `[data-app="documents"]:is(:root[data-
   theme="dark"] *) .x`; (b) der Scope erhöht jede Spezifität um (0,1,0), dadurch schlugen App-Regeln die gemeinsamen
   `shared/office-workspace.css`-Regeln (gemessen: Listenzeilen auf volle Höhe gestreckt) → erstes Klassen-/Attribut-
   Glied je Selektor in `:where()` (374 Zeilen), Spezifität = Original, Shell-Vorfahr-Ersatz ausgenommen.
   `@import url('ctox:shared/office-workspace.css')` (ohne `?v=`). Drawer-CSS ergänzt.
10. **Rechte**: `canModifyDocumentsApp` aus `api.actor` (`apps.source.edit` oder Rolle admin/chef, nur Spiegel);
    Editor-Rechte `write/comment/review` aus `api.actor` (Rolle owner/chef/admin oder `data.write` in der Rechteliste,
    nur Spiegel; FIX5 statt „immer an“, OFFEN-7 Modul-Scope bleibt offen).
11. **Zustandstexte** entfernt: Platzhalter in index.html, Listen-Lade/Leer/Filter-Leer-Kästen, Editor-Lade-
    Kästen, „Empfänger x von n wird geladen …“ (jetzt `aria-busy`), Startwiederholung des Office-Editors still.
12. **Toter Code** (nicht aufgerufen, mechanisch portiert, nicht gelöscht): Kontextmenü/Kontext-Chat
    (`initDocumentsContextMenu` wird nirgends gerufen, `handleContextMenu` nie registriert), `renderRight`
    (Slot `[data-documents-actions-content]` fehlt in index.html), `openNewDocumentDrawer`, `renderWorkflowPanel`,
    `bindWorkflowControls`, `createMarkdownDocument`.

## Befehlstypen – Einstufung (§16.13)

Rust-Bezüge gemessen mit `grep -rn '"<type>"' src/core --include='*.rs' | wc -l` im Klon (14:2x):

| Befehlstyp | Rust | Einstufung | Ziel |
|---|---:|---|---|
| `office.document.prepare` | 1 | Fachlogik + Fähigkeit | Handler `handlers/office.document.prepare.js` + `cap.office.prepare` (V4-HANDLERS.md) |
| `office.document.commit` | 2 | Fachlogik + Fähigkeit | Handler + `cap.office.apply` |
| `office.document.export` | 1 | Fachlogik + Fähigkeit | Handler + `cap.office.render` |
| `office.document.freeze_email_content` | 1 | Fachlogik + Fähigkeit | Handler + `cap.office.render` |
| `research.systematic.report.create` | 8 | Fachlogik + Fähigkeit | Handler (H7) + `cap.office.render`/`cap.doc.extract` |
| `ctox.office.settings.save` | 3 | Fähigkeit | Handler + `cap.office.settings` (von der UI nicht gesendet) |
| `document.summarize`, `document.extract-requirements`, `document.review-risks` (Seed-Runbooks) | 0 | Fachlogik + Fähigkeit | generischer Weg Befehl → Queue-Task (kein Handler im Strang H7) |
| `ctox.knowledge.runbooks.manage` (`module:'ctox'`) | 0 | Fachlogik + Fähigkeit | generischer Weg; gehört App ctox/knowledge |
| `business_os.chat.task` (Kontext-Chat, tot) | 154 | Core-generisch | bleibt |
| `ctox.business_os.app.modify` (Kontext-Chat Modus app, tot) | 26 | Core-generisch | bleibt |

## OFFEN (nummeriert)

1. **Fähigkeit cap.office und Dateien**: Die Handler (Strang H7, nicht angefasst) übergeben `chunks_collection:
   'document_blob_chunks'` an `cap.office.*`. Die UI legt Bytes jetzt nur über `api.files` ab (`files/<blob_id>`,
   `ref = sha256`). `cap.office` muss `*_blob_id` über die Collection `files` auflösen und Ergebnisse (`target_blob_id`,
   E-Mail-Blobs, Export) ebenfalls als files-Dokumente mit `id = blob_id` schreiben; sonst findet `readFileBytes` sie
   nicht. Altbestand in `document_blob_chunks` braucht eine Core-Migration nach `files` (nicht App-Sache).
2. **Fremd-Collections**: `knowledge_items`, `knowledge_runbooks`, `knowledge_tables` gehören der App knowledge und
   werden nur gelesen → im Manifest als `foreign_collections` (nur lesen), nicht unter `collections`. module.json und
   collections.schema.json (nicht geändert) führen sie noch als eigene, dazu `document_blob_chunks` (entfällt) und
   `business_commands` (System-Collection). Manifest-Strang.
3. **ctox_runtime_settings** (`runtime-settings.office.documents_engine`) ist Core-/Plattformstand, nicht im Manifest;
   braucht `foreign_collections` oder einen Plattform-Lesepfad. Ohne Recht: Zone `documents.read.ctox_runtime_settings`,
   Engine bleibt `ctox_documents`.
4. **vendor-Importe** `../../vendor/document-format.mjs`, `vendor/ctox-office/ctox-office-document.mjs`,
   `vendor/superdoc.mjs`(+css, nur Legacy-Engine, `<link>` in head) greifen aus der App heraus; ob der SW `vendor/`
   ausliefert oder ein `ctox:vendor/`-Schema kommt, ist offen (wie notes OFFEN-3).
5. **Datei-Aufbewahrung**: Entwurfsblobs werden über `api.remove('files', id)` freigegeben; ob Apps files-Dokumente
   löschen dürfen und wann der Inhalt geräumt wird, regelt der Kern.
6. **Übergabe an die CTOX-App**: Die frühere `sessionStorage['ctox.businessOs.focusTask']`-Übergabe nach „Neues
   Dokument“ entfällt; die CTOX-App findet die Aufgabe über `business_commands`. Rückmeldung an einen Chat-Thread
   (`ctox-business-os-record-focus` mit `returnThreadId`) hat in 4.0 keinen Weg – heute nur Zone.
7. **Schreibrecht-Spiegel**: `ctx.permissions.canWriteCollection` hat in 4.0 kein Gegenstück (`actor.permissions` ist
   eine Fähigkeitsliste); der Editor zeigt Schreiben immer, der Core lehnt ab (Kernel-Zone).
8. **Werkzeug scope-css**: (a) `:root[data-theme…]`-Präfix und (b) Spezifitätsanstieg (Entscheidung 9) betreffen alle
   Apps mit gemeinsamem Shell-CSS (knowledge u. a. prüfen).
9. **Bestehende Suiten** `documents.test.mjs` (1340 Z., esbuild-Bundle), `mail-merge-recipient-switch.test.mjs`,
   `documents-layout.browser.test.mjs` sind **nicht** umgestellt und nicht gelaufen (Klon ohne node_modules/esbuild;
   Zeitbudget). Sie testen u. a. die entfernten Readiness-/Replikationspfade (Hooks `wireDocumentReadiness`,
   `resolveLocalFirst`, `awaitDocumentVersionReplication`, `withDocumentVersionTimeout`, `refreshKnowledge` entfernt;
   `loadBlobBytes(state, …)`, `saveBlobChunks(state, …)`, `ctoxDocumentsPermissions()` mit neuer Signatur,
   `mailMergeRecipientLoading` → `mailMergeRecipientPending`). Die reinen Fachtests (Gruppierung, Knowledge-
   Aggregation, Serienbrief, Filter, Validierung) nutzen unveränderte Hooks.
10. **Toolbar-Sichtbarkeit** der Legacy-Engine war in `ctx.storageScope` je Nutzer gespeichert; jetzt nur im Snapshot
    (Fensterzustand). Dauerhafte Nutzerpräferenz bräuchte `shell_preferences` (Plattform).
11. **Mehrfenster**: `multi_instance:false`; Zustand ist je Montage, zwei Montagen teilen nur den Modul-Cache des
    Format-Moduls nicht (state-lokal).
12. **Kopfeinzug `--shell-v2-icon-size`**: Listen- und Dokumentkopf reservieren links `icon-size + 14px` für das
   v2-App-Icon, das im v2-Fenster über dem App-Kopf lag. Der v4-Rahmen zeichnet das Icon in einer eigenen
   Kopfzeile (36px-Spalte) und setzt die Variable auf 28px → 42px leerer Einzug im App-Kopf. Gleiches Muster im
   Pilot knowledge (+8px) und vielen Apps; unverändert gelassen, Entscheidung gehört Rahmen/PORTING (Variable im
   v4-Rahmen auf 0 oder Regel R-neu).

## Dateien
- `index.js` (portiert), `office-bridge-v4.js` (neu), `index.html`, `index.css`, `locales/de.json`, `locales/en.json`,
  `tests/v4-api.test.mjs` (neu), `tests/shots/documents-{1280,420}.png`, `V4-PORT.md`; `schema.js` gelöscht.

## VENDOR (Strang VENDOR, 15.10.2026)
- `document-format.mjs` liegt in `modules/documents/vendor/` (Import `./vendor/document-format.mjs`); Office-Engine über `ctox:lib/ctox-office@1/…`.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G3: Zonentext enthielt Datensatz-/Versions-ID („(current-version-001)“); Text jetzt nur Meldung + Titel, Zuordnung über `ref`. Die Zone selbst bleibt im Labor sichtbar: die Fixture hat zu keinem Dokument eine passende Version – korrekte Warnung, kein Darstellungsmangel.
- G4: Kartentitel brechen um (Zwei-Zeilen-Kappung entfernt, nowrap aus shared/office-workspace.css app-seitig überstimmt).

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app documents --shared` (Stub-Pin 9fa9948) → G4 grün; G3 nur noch die berechtigte Fixture-Warnung; `node --test modules/documents/tests/v4-api.test.mjs` grün; `scan.mjs --app documents` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.

## Nachtrag Strang VIS2 (15.10.2026)

- SuperDoc-Reste aus `index.css` entfernt: 27 Regeln `.documents-superdoc-{controls,toolbar-toggle,toolbar,ruler,editor,mount}` samt Vendor-Klassen darunter (`.superdoc-toolbar`, `.toolbar-*`, `.presentation-editor__viewport` …), der Dunkel-Kommentar und die drei Touch-Regeln im `@media (pointer: coarse)`-Block. Vorher per grep belegt: keiner dieser Selektoren kommt in `index.js`/`index.html`/`office-bridge-v4.js`/`shared/`/`v4/` vor (nur `vendor/superdoc.*` selbst und die Testprüfung). Die dazwischen liegenden Nicht-SuperDoc-Regeln (`.documents-module`, `.documents-explorer…`, `.documents-context-menu`) bleiben.
- `.documents-superdoc-frame` bleibt (`mountCtoxDocuments` setzt die Klasse, `index.js:3761`).
- `documents.test.mjs`: drei Prüfungen auf die SuperDoc-Touch-Regeln entfernt. Der Test selbst läuft in diesem Klon nicht (fehlendes Paket `esbuild`, unverändert gegenüber HEAD).
- Offen (JS, nicht VIS2): Zustandsfelder `state.superdocSaveTimer/superdocSavePromise` in `index.js` tragen noch den alten Namen.

Prüfung: Labor `--shared` (Stub-Pin 9fa9948) → unverändert nur G3 = berechtigte Fixture-Zonenwarnung; `node --test modules/documents/tests/v4-api.test.mjs` 11/11.
