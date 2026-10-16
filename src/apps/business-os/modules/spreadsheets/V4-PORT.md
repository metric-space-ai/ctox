# spreadsheets – Port auf App-API 4.0 (Strang UD4, Welle D)

Fortschritt (gemessen `TZ=Europe/Berlin date`):
- 13:39 Scanner vorher: **104 Stellen** (R1 1, R2 ~22, R3 ~16, R4 ~7, R5 1, R6 ~30, R7 2, R8 2, R9 5, R10 3, R11 4,
  R12 2, R14 3, R17 1), CSS 118 Selektoren ungescoped (19 mit Shell-Vorfahr). guard-v4 vorher: 12+ Treffer (Locales).
- ~14:15 Port nach dem documents-Muster (gleiche Brücke, gleiche Entscheidungen); Scanner 0, guard-v4 0.
- 14:2x v4-Test 9/9 grün; Screenshot 420 zeigte festen Bibliotheksstreifen und abgeschnittenen Editor → Fenster-
  Container am App-root ergänzt (Entscheidung 7), Ordner-Glyphe fehlte (`···`) → app-lokal ergänzt; erneut 9/9 grün,
  Screenshots angesehen.
- 15:04 Wiederaufnahme nach Sitzungsabbruch: Stand vollständig (keine halben Edits), Scanner 0, guard-v4 0, scope-css gescoped;
  v4-Test gegen Pin **cff5ba5** 9/9 grün. Test-Standardpin auf cff5ba5 gesetzt. Test-Seite setzt
  `--shell-v2-icon-size: 28px` auf das Fenster (Wert aus `v4/frame/frame.css .shell-window`); vorher galt der
  v2-Rückfall 80px und die Screenshots zeigten 94px statt 42px linken Kopfeinzug. Screenshots neu, angesehen.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app spreadsheets` | **0 Stellen**, Exit 0 (vorher 104) |
| Löschliste-Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/spreadsheets` | total **0** |
| CSS-Scope | `node v4/tools/port/scope-css.mjs spreadsheets modules/spreadsheets/index.css --check` | gescoped |
| v4-Test gegen Stub-Pin | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/spreadsheets/tests/v4-api.test.mjs` | **9/9 grün** (15:06/15:10, Pin cff5ba5; vorher 67229d2 ebenfalls grün), keine pageerror |
| Screenshots | wie oben mit `CTOX_SPREADSHEETS_SHOTS=1` | `tests/shots/spreadsheets-1280.png`, `spreadsheets-420.png` (angesehen) |
| schema.js | Tiefenvergleich gegen collections.schema.json | 4/4 Collections gleich, Migrationen beide leer → **gelöscht** |

Abgedeckt: mount zeigt Live-Daten (Titel `Tabellen`, `data-app`, kein Zustandskasten) · `sim.push` live, Löschung
verschwindet · leere Collections: Liste/Editor ohne Text, keine Zone · Import CSV über den Import-Drawer:
`api.files.put` (`files/<blob_id>`, `ref = sha256 = source_sha256`, `owner_app`), Version + Datensatz per `api.put`
(Zeilenzahl, Tags), Bytes über `files.get` · Befehle über die Office-Brücke: `office.spreadsheet.prepare` legt
`business_commands`-Dokument an (`record_id`, `module`) und löst über den gepushten Status auf; `failCommand` auf
`office.spreadsheet.commit` → Ablehnung, die Editor-Bytes liegen vorher als files-Dokument (`ref = editor_sha256`);
`loadVersion` liest kanonische und Editor-Bytes über `api.files` · Löschen über `api.shell.dialog` (ohne Ja nichts,
mit Ja `is_deleted`) · snapshot → structuredClone → Remount erhält Auswahl und offene Sucheingabe (pane-grammar
normalisiert die Suche auf Kleinbuchstaben – unverändertes Verhalten) · unmount löst alle Abos und leert root.
Beobachtet: Nach dem Import öffnet der echte Office-Editor (vendor/ctox-office, wasm) die Tabelle über die v4-Brücke
(`loadVersion` + `office.spreadsheet.prepare` als Befehl); der Stub hat keinen prepare-Handler, daher bleibt die
Editorfläche leer (erwartet). **Nicht** abgedeckt: Editor-Bearbeitung/Speichern im Browser, Kontext-Chat (tot),
Locale-Wechsel, Geometrie-Labor.

## Bereiche und Entscheidungen (wie documents/V4-PORT.md, hier nur Abweichungen)

1. **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`; Zustand je Montage. `ensureSpreadsheetRuntimeReady`
   (`ctx.actions.ensureRuntimeReady`) entfällt ohne Ersatz (4.0 hat keine Laufzeit-Bereitschaft; OFFEN-4). Titel
   `api.shell.setTitle('Tabellen')`. Rechte Shell-Spalte (`ctx.right`, `applyRightPaneState`) entfällt; `renderRight` leer.
2. **Lesen**: Live-Abos `spreadsheets` (updated_at_ms desc), `spreadsheet_runbooks` (title asc), dazu ein Abo
   `spreadsheet_versions {spreadsheet_id}` je Auswahl; Versionen synchron über `api.get`/`query().get()` als
   `structuredClone`. Entfernt: `ctx.db`, Readiness-Fassade + „Tabellen werden synchronisiert“, `startCollection`/
   `waitForOpenPeerId`, `resolveSpreadsheetVersionLocalFirst` (2 Wiederholungsrunden, Fristen 4,5 s/60 s),
   `findOne({requireRevision})`. Fehlt der ausgewählte Datensatz in der Live-Liste, hält nur ein offener Editor-Entwurf
   die Auswahl (vorher: Revisionsabfrage am Core).
3. **Schreiben/Dateien**: wie documents (`api.put/patch(...).local`, `files.put(blob, {id: blob_id})`,
   `source_sha256` zusätzlich in der Version). Herkunftsdatensätze beim Öffnen einer Datei (`desktop_files`,
   verknüpfte Collection) über `api.get` (fremd, ohne Recht → `null`).
4. **Office-Brücke**: `office-bridge-v4.js` = treue Kopie aus documents (getrennte App, kein Cross-Import), `kind:
   'spreadsheet'` → `office.spreadsheet.{prepare,commit,export}`.
5. **Befehle**: `sendSpreadsheetsCommand` (Zone `spreadsheets.command.<type>`); `dispatchSpreadsheetRunbook` portiert,
   aber ohne Aufrufer (tot, wie vorher).
6. **Zustand/Texte**: Zone statt `ctx.notifications` und Fehlerkästen (`spreadsheets.create`, `.import`, `.export`,
   `.manage.save`, `.manage.delete`, `.switch`, `.file.open`, `.editor`, `.editor.save`, `.version.missing` (nur bei
   `metrics.backlog = 0`), `.format.unsupported`, `.route.unknown_record`, `.runbooks.seed`, `.render`); Erfolgsmeldungen
   still. Entfernt: Lade-/Sync-/Leer-/„Keine Tabelle ausgewählt“-Kästen in index.html und index.js, Speicherzustands-
   Texte im Kopf („Speichert...“, „Ungespeicherte Änderungen“, „Gespeichert“) – der Punkt `spreadsheets-dirty-badge`
   zeigt den Zustand nur noch über Klassen, `role="status"` entfernt. Locales: 18 Schlüssel entfernt.
7. **CSS**: wie documents (`ctox:shared/office-workspace.css`, `:where()` für die Original-Spezifität, 98 Zeilen;
   20 Shell-Vorfahr-Zeilen ausgenommen). Die `@container business-app-window`-Regeln (≤ 768 px) hingen am
   v2-Fenster der Shell (außerhalb von root) → die App setzt `container-name: business-app-window;
   container-type: inline-size` auf ihr root (gemessen: vorher blieb die Bibliothek bei 420 px fest stehen).
8. **Glyphen**: `ctx.getActionIcon` entfällt (OFFEN-D); fehlende `folder`-Glyphe app-lokal ergänzt.
9. **Deep-Link**: `api.shell.onRoute` (`record|spreadsheetId`, `file_ref|ref`, `name`, `mime`, `sourceFileId`) statt
   eventBus `desktop-app:open-file`/`ctx.args.openFile` und `spreadsheets:refresh-left`; Close-Guard des
   Fenstermanagers entfällt (4.0 hat keinen), `beforeunload` bleibt.
10. **Toter Code** (portiert, nicht gelöscht): `initSpreadsheetsContextMenu`/Kontext-Chat, `openNewSpreadsheetDrawer`,
    `dispatchSpreadsheetRunbook`.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust (`grep -rn '"<type>"' src/core --include='*.rs'`) | Einstufung | Ziel |
|---|---:|---|---|
| `office.spreadsheet.prepare` | 1 | Fachlogik + Fähigkeit | Handler + `cap.office.prepare` (V4-HANDLERS.md) |
| `office.spreadsheet.commit` | 2 | Fachlogik + Fähigkeit | Handler + `cap.office.apply` |
| `office.spreadsheet.export` | 1 | Fachlogik + Fähigkeit | Handler + `cap.office.render` |
| `spreadsheet.summarize`, `spreadsheet.audit-formulas`, `spreadsheet.risk-review` (Seed-Runbooks, ohne Aufrufer) | 0 | Fachlogik + Fähigkeit | generischer Weg Befehl → Queue-Task |
| `business_os.chat.task` / `ctox.business_os.app.modify` (Kontext-Chat, tot) | 154 / 26 | Core-generisch | bleibt |

## OFFEN (nummeriert)

1. **cap.office und files** wie documents OFFEN-1 (Handler übergeben `chunks_collection: 'spreadsheet_blob_chunks'`;
   cap muss `files/<blob_id>` lesen/schreiben; Altbestand-Migration im Core).
2. **Manifest**: `spreadsheet_blob_chunks` entfällt, `business_commands` ist System-Collection; `desktop_files` und
   über `linked_collection` erreichte Collections werden gelesen → `foreign_collections` (nur lesen). module.json/
   collections.schema.json nicht geändert (Manifest-Strang).
3. **vendor-Import** `../../vendor/ctox-office/ctox-office-spreadsheet.mjs` greift aus der App heraus (wie documents
   OFFEN-4).
4. **Laufzeit-Bereitschaft**: `ctx.actions.ensureRuntimeReady()` vor dem Mount hat in 4.0 kein Gegenstück; ob der
   Office-Editor eine Core-Laufzeit braucht, die erst gestartet werden muss, ist eine Frage an „ctox sync v4“.
5. **Schreibrecht-Spiegel**: wie documents OFFEN-7 (`canWrite` immer an).
6. **Bestehende Suiten** `spreadsheets.test.mjs`, `spreadsheets-version.browser.test.mjs` nicht umgestellt und nicht
   gelaufen (Klon ohne node_modules/esbuild; Zeitbudget). Entfernte Hooks: `withSpreadsheetVersionTimeout`,
   `isTransientSpreadsheetVersionReadError`, `resolveSpreadsheetVersionLocalFirst`, `awaitSpreadsheetVersionReplication`,
   `ensureSpreadsheetRuntimeReady`, `shouldRenderSpreadsheetsSyncing`; `saveBlobChunks(state, …)` neue Signatur.
7. **Werkzeug scope-css**: Spezifitätsanstieg und Shell-Container wie documents OFFEN-8.
8. **Kopfeinzug `--shell-v2-icon-size`**: Listen- und Dokumentkopf reservieren links `icon-size + 14px` für das
   v2-App-Icon, das im v2-Fenster über dem App-Kopf lag. Der v4-Rahmen zeichnet das Icon in einer eigenen
   Kopfzeile (36px-Spalte) und setzt die Variable auf 28px → 42px leerer Einzug im App-Kopf. Gleiches Muster im
   Pilot knowledge (+8px) und vielen Apps; unverändert gelassen, Entscheidung gehört Rahmen/PORTING (Variable im
   v4-Rahmen auf 0 oder Regel R-neu).

## Dateien
- `index.js`, `office-bridge-v4.js` (neu), `index.html`, `index.css`, `locales/de.json`, `locales/en.json`,
  `tests/v4-api.test.mjs` (neu), `tests/shots/spreadsheets-{1280,420}.png`, `V4-PORT.md`; `schema.js` gelöscht.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G4: Kartentitel brechen um (Zwei-Zeilen-Kappung entfernt, nowrap aus shared/office-workspace.css app-seitig überstimmt).
- Hinweis: im Endlauf G3 durch „Editor konnte nicht geladen werden: … ctox:lib/ctox-office@1 …“ – neue Import-Zeile des VENDOR-Strangs, die das Labor (noch) nicht auflöst; nicht aus diesem Strang.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app spreadsheets --shared` (Stub-Pin 9fa9948) → G4 grün (Lauf vor der VENDOR-Änderung 3/3 OK); `node --test modules/spreadsheets/tests/v4-api.test.mjs` grün; `scan.mjs --app spreadsheets` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
