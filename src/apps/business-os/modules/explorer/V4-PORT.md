# explorer – Port auf App-API 4.0 (Strang UC4, Welle C)

Status: **fertig** (13:55 Berlin).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app explorer` | vorher 47 Stellen → **0**, Exit 0 |
| Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/explorer` | vorher 23 → **0** |
| CSS-Scope | `node v4/tools/port/scope-css.mjs explorer modules/explorer/index.css --check` | gescoped |
| v4-Test (Stub-Pin 67229d2) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/explorer/tests/v4-api.test.mjs` | **9/9 grün** |
| Bestehende Suite (umgestellt) | `node --test src/apps/business-os/modules/explorer/explorer.test.mjs` | 16/16 grün |
| Screenshots | `tests/shots/explorer-1280.png`, `tests/shots/explorer-420.png` | angesehen; Layoutfehler behoben: Sortierauswahl und „Details“ standen untereinander (`.app-explorer-heading > div:first-child {display:grid}` traf die Ansichtsleiste) → eigene Regel `inline-flex`. 420: Seitenleiste als waagerechter Streifen, kein Seitenüberlauf (Test prüft `scrollWidth`) |

v4-Test deckt ab: mount zeigt Files-Wurzel live aus `desktop_files`, Systemordner per `api.put` angelegt, kein
Zustandstext, keine Zone, Titel · leerer Ordner rendert nichts, `sim.push` erscheint live, Löschung verschwindet ·
Upload per Drop → `api.files.put` (Metadaten in `files`, `owner_app: explorer`) + `desktop_files`-Eintrag mit
`content_ref` = sha256-ref, Outbox leer, Vorschau über `api.files.get`, Öffnen `.txt` → `navigate('documents',
{open_file:{ref,…}})` · lazy CTOX-Datei → `navigate('file-viewer', {file_id, path, content_state})`, Route kommt an ·
Ordner anlegen (`put`), Papierkorb über `api.shell.menu` → `api.shell.dialog` (ohne Ja nichts; mit Ja
`patch is_deleted`) · foreign `documents` live, Öffnen → `navigate('documents', {record})` · unbekannte ref beim
Download → genau ein Zoneneintrag `explorer.download` · snapshot → Remount erhält Quelle, Ordner, Suchtext, Auswahl,
alte Abos abgemeldet · unmount: 0 Abos (`desktop_files`, `files`, `documents`), root leer, keine `pageerror`.

## Was sich geändert hat

- **Einstieg** `mount(root, api, {restored})` → `{unmount, snapshot}`; `snapshot()` = `{v:1, sourceId,
  currentFolderId, query, sort, selectedId}`; Deep-Link über `onRoute` (`params.source`, `params.folder_id`;
  unbekannte Quelle → `explorer.route.unknown_source`).
- **Daten** live: eine `api.query(<collection>, q).subscribe` je aktiver Quelle (Wechsel/Suche abonniert neu,
  Suche 220 ms entprellt, `$regex` + `$options:'i'`, `limit 250`), dazu `api.query('files', {})` für die ref je
  Datei-ID. Ordnerwechsel und „hoch“ rechnen lokal neu (kein Neuladen). Entfallen: `startCollection`,
  `awaitInSync`-Warten (20 s), `requireRevision`, `ctx.ensureModuleData`, Knopf „Aktualisieren“ (Daten sind live).
- **Dateien**: Upload = `api.files.put(file, {id, name, mime})` + `api.put('desktop_files', {…, content_ref: ref,
  content_hash: ref, content_hash_scheme: 'sha256-bytes-v1', content_state:'available'})` mit derselben `id`.
  Lesen (Vorschau, Download, Herausziehen) = `api.files.get(ref)`; ref aus `files[id].ref`, sonst `content_hash` mit
  Schema `sha256-bytes-v1`, sonst sha256-förmige `content_ref`. Entfallen: `desktop_file_chunks` (Schreiben in einem
  `bulkUpsert`, Base64-Chunks 16 KiB), `readStoredFileFromDemandChunks`, `shared/file-integrity.js`-Import,
  `ctx.reportFileIntegrityError`.
- **Öffnen**: Datei mit Fach-App (csv/xlsx/json → spreadsheets, docx/md/txt → documents) →
  `api.shell.navigate(app, {open_file:{ref, name, mime, source_file_id, source_path}})` statt `File`-Objekt über
  `ctx.openDesktopApp`; sonst `navigate('file-viewer', {ref?, file_id, name, mime, size, path, content_state})`;
  Datensatz → `navigate(moduleId, {record})` statt `location.hash`.
- **Shell**: Kontextmenü über `api.shell.menu({x, y, items})` (Antwort = `id`), zusätzlich Rechtsklick auf Zeilen;
  Papierkorb-Bestätigung über `api.shell.dialog({kind:'confirm', …, default:false})`; Namenseingabe und Upload-Dialog
  bleiben app-lokal **im root** (Validierung live, `dialog`-Form offen); `setTitle('Explorer')`; Sprache über
  `api.shell.locale` + `t('explorer.<key>')` mit Rückfall; Hinweis „zum Herausziehen bereit“ → Zone (info).
- **Zustandstexte raus**: „Lade Dateien...“, „Synchronisierung fehlgeschlagen“, „Collection … ist nicht verfügbar“,
  Leer-Texte („Dieser Ordner ist leer.“, „Keine Treffer …“, „Keine …-Einträge“, „Daten vorhanden, aber …“), „Keine
  Datei ausgewählt.“, „Keine integrierte Vorschau …“, „Der Inhalt wird beim Öffnen über CTOX geladen.“, Fehlerkästen
  in Tabelle/Vorschau, Fußzeile „N sichtbar · M geladen · Pfad“ (Element entfernt). Fehler →
  `explorer.source.<collection>`, `explorer.open`, `explorer.download`, `explorer.preview` (warn), `explorer.drag`,
  `explorer.upload`, `explorer.write.<folder|rename|trash|seed>`. Zähler „N Objekte“ im Kopf bleibt (Inhaltsangabe),
  bei 0 leer.
- **CSS** gescoped; tote Regeln (`.app-explorer-status`, `.app-explorer-message`, `.app-explorer-preview-empty`)
  entfernt, dritte Grid-Zeile der Hauptspalte entfernt.
- **schema.js gelöscht** (re-exportierte Schemas aus desktop/documents/knowledge/matching/outbound/spreadsheets;
  `collections.schema.json` ist die Quelle).
- **explorer.test.mjs** umgestellt (16 Fälle): Quelltext-Wächter auf v4, `ensureFileSystem(api, docs)`,
  `storeFile(api, …)` über `files.put`, Suchabfrage. Der Fall „legacy schema-registration capability guarded“
  (`ctx.ensureModuleData`) entfällt mit dem Aufruf.

## Befehlstypen

Keine. explorer schickt keine Befehle (Rust: `"explorer"` nur in der ID-Liste `store.rs`, laut Manifest-Generator).

## OFFEN

1. **Schreiben in eine Fremd-Collection**: explorer schreibt `desktop_files` (Eigentum desktop): Systemordner
   anlegen, Ordner, Umbenennen, Papierkorb, Upload-Einträge. Braucht `foreign_collections.desktop_files.ops =
   [read, write]` bzw. `exports` von desktop – oder `desktop_files` wechselt zu explorer (Baum ist Explorer-Fachlogik).
   Generiertes Manifest führt heute nur `read`.
2. **Systemordner-Seed** (`fs_root`, `fs_desktop`, `fs_documents`, `fs_spreadsheets`, `fs_downloads`, `fs_ctox`)
   schreibt die App bei jeder Montage, wenn sie fehlen/abweichen (vorher genauso). Besser als Handler/Core-Seed des
   Eigentümers.
3. **Dateibaum vs. `files`**: `files` (Plattform) ist flach `{id, name, mime, size, ref, owner_app}`. Ordner, Pfad,
   `local_path`, `content_state` leben weiter in `desktop_files`. Gleiche `id` in beiden verknüpft sie. Dateien, die
   andere Apps über `files.put` ablegen, erscheinen im Explorer erst mit einem `desktop_files`-Eintrag (Wunsch:
   Ordnerfeld in `files` oder Projektion).
4. **Altbestand**: Einträge mit `content_ref` = Datei-ID und Inhalt nur in `desktop_file_chunks` (Schema
   `sha256-bytes-v1` → `content_hash` dient als ref) sind nur lesbar, wenn der Core den Blob unter dieser sha256 in
   Spur 4 führt (Übernahme Chunks → Blobs, Kern).
5. **Fach-Apps öffnen Dateien**: documents/spreadsheets müssen `params.open_file.ref` (statt `args.openFile.file`)
   lesen – deren Port-Stränge; alternativ `api.shell.openFile(ref)` mit Zuordnung in der Shell.
6. **Fremd-Collections lesend**: `documents`, `spreadsheets`, `knowledge_items`, `matching_objects`,
   `outbound_companies`, `desktop_files`, `files` (Plattform). `module.json` führt noch `desktop_file_chunks`.
7. Herausziehen auf den Desktop des Betriebssystems nutzt weiter `globalThis.ctoxBusinessOsDesktop.startFileDrag`
   (Desktop-Hülle, kein Datenweg) – im Vertrag nicht vorgesehen.


## Nachtrag Strang VIS (Geometrie-Labor, 15.10.2026)

- G1 (420): Quellenleiste scrollte seitlich; Quellen brechen jetzt in Zeilen um.

Prüfung: `node v4/tools/geometry/geometry-v4.mjs --app explorer --shared` (Stub-Pin 9fa9948) → alle Läufe OK; `node --test modules/explorer/tests/v4-api.test.mjs` grün; `scan.mjs --app explorer` unverändert. Rechte Leerzone der Fenstersteuerung jetzt über `var(--shell-v2-control-inset, …)` (R18), wo die App sie reserviert.
