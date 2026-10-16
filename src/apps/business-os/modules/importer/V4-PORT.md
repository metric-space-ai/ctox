# importer – Port auf App-API 4.0 (Strang UC2, Welle C)

Stub-Pin cff5ba5 (Erstport gegen 67229d2). Start 14:17, fertig 14:20 (Berlin, gemessen).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app importer` | 20 → 0 Stellen, Exit 0 (CSS vorher 134 ungescoped, 16 mit Shell-Vorfahr) |
| CSS-Scope | `node v4/tools/port/scope-css.mjs importer modules/importer/index.css --check` | gescoped |
| Wächter | `node v4/tools/guards/guard-v4.mjs --app modules/importer` | 28 → 0 Treffer |
| v4-Test | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/cff5ba5 node --test src/apps/business-os/modules/importer/tests/v4-api.test.mjs` | 8/8 grün |
| Legacy-Suite | `node modules/importer/importer.test.mjs` (aus Kopie mit node_modules-Link) | 7/7 grün |
| Screenshots | `tests/shots/importer-1280.png`, `importer-420.png` | angesehen, kein Überlauf |

v4-Test deckt ab: mount (Quellenansicht, Titel, keine Hinweisbox, keine Zone) · ungültige URL → genau ein
Zoneneintrag, kein Befehl · GitHub-Import: Fortschrittsansicht sofort (kein Warten), `ctox.business_os.app.create`
mit `import_source`, Ergebnis live aus dem Befehlsdokument, „App öffnen“ → `api.shell.navigate('<modul>')` (kein
Neuladen) · `failCommand` → Jobergebnis „nicht live“ mit Fehlertext · Datei-Import: Ordner/Datei/Chunk per `api.put`
in `desktop_files`/`desktop_file_chunks`, Outbox leer, Befehl trägt die Datei als `dependencies`, Fortschrittseintrag
der Zone geräumt · snapshot → Remount erhält URL-Eingabe und folgt einem laufenden Auftrag weiter · unmount löst das
Befehls-Abo und leert root.

## Entscheidungen

- **`location.reload()` (vorher index.js:712) ersatzlos entfernt** samt `location.hash = <modul>`: „App öffnen“ ruft
  `api.shell.navigate(moduleId)`; die neue App erscheint über die Aktivierung live (§16.10). Unbekanntes Ziel meldet
  die Shell selbst in die Zone.
- **Befehl**: `api.command('ctox.business_os.app.create', {...payload, module, record_id, dependencies,
  client_context}, {record_id})`, nie warten. Vorher: `commandBus.subscribe` + `dispatch(..., {until:'accepted',
  timeoutMs: 60000})` + `getStatus` + Sonderpfad „Job lokal gesichert; warte auf CTOX-Sync“ (`isRecoverableDispatchError`,
  WebRTC-Peer-Muster) – alles entfällt. Projektion live über `api.get('business_commands', id)`; v4-Status `done` =
  `completed`. `buildAppImportCommand` liefert keine `id`/`command_id` mehr (vergibt `api.command`) und keine
  Sync-Lease-Felder (`sync_collections`, `sync_flush_timeout_ms`, `allow_dependency_delivery_lag`); `dependencies`
  bleibt (Core wartet auf die Snapshot-Dokumente).
- **Snapshot**: `desktop_files`/`desktop_file_chunks` per `api.put(...).local` statt `upsert`/`bulkUpsert` +
  `confirmSnapshotDocuments` (Sync-Lease, gezielter Push, 120-s-Retry-Kette; Export samt 3 Legacy-Tests entfernt).
  Reihenfolge: die Outbox liefert in Schreibreihenfolge (Stub: `op_id` UUIDv7, sortiert) → Dokumente vor dem Befehl.
- **Zustandstexte**: Hinweiskasten `[data-imp-notice]` (HTML + CSS) entfernt; Fehler/Hinweise in die Zone
  (`importer.github.invalid` warn, `importer.dispatch`, `importer.file`, `importer.folder`, `importer.folder.no_picker`,
  `importer.command.local`); Datei-Fortschritt „n/m · Pfad“ als `importer.snapshot.progress` (info, danach geräumt)
  statt in `[data-imp-source-hint]`. Fortschrittsnotiz zeigt nur noch die Phase aus dem Befehlsdokument
  (`execution_progress.current_step`/`phase`), nicht den Status-Text.
- **Texte**: `sourceHint` „…synchronisiert“ → „die Dateien gehen als Snapshot an CTOX“, en „Read-only …“ → „read access
  only“, `phaseQueue` „Warteschlange“ → „Einreihen“; 35 ungenutzte Altschlüssel (Review-/Install-Fluss) entfernt.
- **Sonst**: `ensureStyles`/`moduleAssetUrl` (Cache-Buster über `?`-Suche) entfernt; `ctox:shared/file-integrity.js`;
  Actor aus `api.actor`; Titel `setTitle`; root ist `container: business-app-window`.
- **schema.js gelöscht**: leer (`collections = {}`), gleich `collections.schema.json`.

## Collections (für den Manifest-Generator)

Eigene: keine. Fremd:

| Collection | Eigentümer | ops |
|---|---|---|
| `desktop_files` | desktop | **write** (put: Snapshot-Ordner und -Dateien) |
| `desktop_file_chunks` | desktop | **write** (put: base64-Chunks) |
| `business_commands` | System | read (`api.get` des eigenen Befehls) |

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer | Einstufung |
|---|---:|---|
| `ctox.business_os.app.create` | 105 | Core-generisch (App-Erzeugung = Plattform) + Fähigkeit Agent-Lauf |

## OFFEN

1. **Dateiweg**: v4 hat `api.files.put(blob)` (Spur 4, Collection `files`). Der Snapshot bleibt hier bewusst im
   Desktop-Datenmodell (Chunks als Dokumente), weil der Core-Befehl `dependencies` auf `desktop_files` erwartet.
   Umstellung auf `files`-Refs ist eine Vertragsänderung zwischen importer und `ctox.business_os.app.create`.
2. **Outbox-Reihenfolge** als Zustellgarantie (Dokumente vor Befehl) ist im Stub so, im Vertrag zu bestätigen.
3. **Fremd-Schreibrechte** `desktop_files`/`desktop_file_chunks` (write) im Manifest; alternativ Schreiben über eine
   Desktop-Fähigkeit.
4. Kein git benutzt.

## Nachprüfung gegen Stub-Pin cff5ba5 (Wiederaufnahme 15.10., bis 15:10 Berlin, gemessen)

Nach dem Sitzungsabbruch ~14:40: keine Datei im Schreibbereich nach 14:25 geändert, keine halben Edits. Unverändert
gegen den neuen Pin neu gefahren:

- `scan.mjs --app importer`: 0 Stellen, CSS ungescoped 0, Exit 0; `scope-css.mjs … --check`: gescoped.
- `guard-v4.mjs --json`: 0 Treffer unter `modules/importer/` (nur getrennt gezählte Testdatei-Treffer).
- v4-Test gegen cff5ba5: 8/8 grün (mit `CTOX_APP_API_STUB` und ohne – Rückfallpfad im Test jetzt cff5ba5).
- Legacy-Suite (Kopie mit node_modules-Link): 7/7 grün.
- Screenshots 1280/420 vom Testlauf neu geschrieben und angesehen: unverändert gegenüber Erstport, kein Überlauf.
