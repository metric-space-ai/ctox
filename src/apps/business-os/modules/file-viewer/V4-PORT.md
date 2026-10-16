# file-viewer – Port auf App-API 4.0 (Strang UC4, Welle C)

Status: **fertig** (13:35 Berlin, gemessen `TZ=Europe/Berlin date`).

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --app file-viewer` | vorher 37 Stellen (CSS 22 ungescoped) → **0**, Exit 0 |
| Wächter | `node v4/tools/guards/guard-v4.mjs --json --app modules/file-viewer` | vorher 25 → **0** |
| CSS-Scope | `node v4/tools/port/scope-css.mjs file-viewer modules/file-viewer/index.css --check` | gescoped (3 `.shell-window[data-shell-contract="v2"]`-Vorfahren ersetzt) |
| v4-Test (Stub-Pin 67229d2) | `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/67229d2 node --test src/apps/business-os/modules/file-viewer/tests/v4-api.test.mjs` | **9/9 grün** (Chromium headless 1223) |
| Bestehende Suite (umgestellt) | `node --test src/apps/business-os/modules/file-viewer/file-viewer.test.mjs` | 6/6 grün |
| Screenshots | `tests/shots/file-viewer-1280.png`, `tests/shots/file-viewer-420.png` | angesehen: Kopf, Metadatenzeile, Textvorschau; 420 ohne horizontalen Überlauf (Test prüft `scrollWidth`) |

v4-Test deckt ab: mount ohne Datei (kein Text, keine Zone, Titel) · Öffnen per `navigate(ref)` über `api.files.get` ·
große Textdatei 256-KiB-Vorschau + vollständiger Download (300 KiB) · nur `file_id` → Metadaten kommen live aus
`files` (Core-Push) → laden, Metadaten-Abo danach beendet · unbekannte `ref` → genau ein Zoneneintrag
`file-viewer.open` · lazy Datei → `ctox.file.materialize` ohne Warten, `sim.failCommand` → ein Zoneneintrag, `done`
mit `result.ref` → Inhalt, Eintrag geräumt · Format ohne Vorschau → Inhalt leer, `info` in der Zone ·
snapshot → structured clone → Remount mit `restored` zeigt dieselbe Datei · unmount: 0 Abos, root leer, keine
`pageerror`.

## Was sich geändert hat

- **Einstieg**: `mount(root, api, {restored})` → `{unmount, snapshot}`. Eingang über `api.shell.onRoute(ev.params)`
  statt `ctx.args`; Parameter `{ref, file_id|fileId|id, name, mime|mimeType, size|sizeBytes, path, content_state}`
  (alte camelCase-Namen werden noch angenommen). `snapshot()` = `{v:1, target}`.
- **Dateien**: nur `api.files.get(ref)` (§10.2a Nr. 4). Entfallen: `desktop_files`/`desktop_file_chunks`-Lesen,
  `bridge.state.demandFileLoader`, `readStoredFileFromDemandChunks` (`shared/file-integrity.js`), Hash-/Generations-
  Prüfung im Browser (macht laut Stub-Vertrag Spur 4 beim Empfang), Warteschleifen 300 ms/30 s/90 s,
  `restartCollection`, `touchCommandDocument`.
- **Nur Datei-ID**: Metadaten live über `api.get('files', id)` (Plattform-Collection); sobald `ref` da ist, wird
  geladen und das Abo beendet.
- **Textvorschau**: früher Bereichsabruf 256 KiB über den Demand-Loader; jetzt `blob.slice(0, 256 KiB)` nach
  `files.get` (gleiche sichtbare Begrenzung, aber der ganze Blob wird geholt – OFFEN-2).
- **Befehl** `ctox.file.materialize` (lazy Datei mit `path`): `api.command(type, {record_id, file_id, path, title,
  client_context})`, nie warten; `failed`/lokal ungültig → `file-viewer.command.ctox.file.materialize`, `done` →
  `clear` und, falls `result.ref` geliefert wird, laden. `module`/`command_type`/`inbound_channel:'desktop'` nicht mehr
  im Dokument.
- **Zustandstexte** raus: „Lade Datei...“ (index.html), Fehlerkasten „Datei konnte nicht geöffnet werden: …“ und
  „Keine Vorschau verfügbar …“ im Inhalt → Zone `file-viewer.open` (error), `file-viewer.download` (error),
  `file-viewer.preview.unsupported` (info). CSS-Regeln dafür (`.file-viewer-content > p`, `.file-viewer-unsupported`)
  entfernt.
- **Shell**: `api.shell.setTitle(name)`; Sprache über `api.shell.locale` + `t('file-viewer.<key>')` mit eingebetteten
  Rückfalltexten (de/en); `ensureModuleStyles()` entfernt (CSS lädt die Shell); Markup per
  `fetch(new URL('./index.html', import.meta.url))` ohne `?v=`.
- **schema.js gelöscht** (re-exportierte `desktop_files` aus `../desktop/schema.js` und `business_commands` aus
  `../knowledge/schema.js`, letzteres existiert seit dem Pilot schon nicht mehr). `collections.schema.json` unverändert.
- **file-viewer.test.mjs** auf v4-Vertrag umgestellt (Quelltext-Wächter: `mount(root, api…)`, `api.shell.setTitle`,
  `api.files.get`, kein `ctx.`/Chunk/`?v=`); Demand-Loader-Tests ersetzt durch `previewBlobFor`/`normalizeTarget`.

## Befehlstypen – Einstufung (§16.13)

| Befehlstyp | Rust-Treffer `grep -rn '"ctox.file.materialize"' src/core` (.rs) | Einstufung | Ziel |
|---|---:|---|---|
| `ctox.file.materialize` | 6 in 4 Dateien (`store.rs` `materialize_desktop_file_command` :4336, `command_plane.rs`, `rxdb_peer.rs`, `rxdb_peer_intake.rs`) | **Fähigkeit** | Host-Datei (CTOX-Workspace, `local_path`) lesen und als Datei-Blob bereitstellen: `ctx.cap.files.materialize({file_id, path}) → {ref}`; Ergebnis `ref` im Befehlsergebnis oder im `files`-Dokument |

## OFFEN

1. **Wer öffnet den Viewer?** `api.shell.openFile(ref)` (Explorer, Desktop) muss in der Shell auf
   `navigate('file-viewer', {ref, name, mime, size})` abbilden (Zuordnung Dateityp → App ist Shell-Sache). Heute
   liefern Explorer/Desktop zusätzlich `file_id`/`path`/`content_state` für lazy Dateien mit; ob `openFile` Parameter
   über `ref` hinaus trägt, ist im Vertrag offen.
2. **Bereichsabruf**: `files.get(ref)` kennt keinen Bereich; große Textdateien (> 256 KiB) werden ganz geholt und nur
   die Vorschau gekürzt. Wunsch an Kern: `files.get(ref, {range})` oder Blob-Stream.
3. **Ergebnisform von `ctox.file.materialize`**: Die App nimmt `result.ref` (falls vorhanden) oder das live erscheinende
   `files`-Dokument mit gleicher `id`. Der Core-Handler muss eines davon liefern (heute schreibt er `desktop_files` +
   `desktop_file_chunks`).
4. **Fremd-Collections**: liest `files` (Plattform) und `business_commands` (Plattform) – keine App-Collection mehr.
   `module.json` führt noch `business_commands`, `desktop_files` (Manifest-Strang v4/manifests).
5. **Verweise auf gelöschte schema.js** außerhalb dieser App: keine (file-viewer/schema.js wurde nirgends importiert;
   `v4/manifests/file-viewer.manifest.json` listet sie noch unter `files`).
6. **Hash-Prüfung**: Die Browser-seitige Inhaltsprüfung (`content_hash`, `content_generation_id`) entfällt; im
   Stub prüft `files.get` den sha256 beim Empfang. Im echten Kern zu bestätigen.
