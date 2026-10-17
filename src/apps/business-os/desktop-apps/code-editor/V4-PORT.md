# code-editor – Port zur Editor-Bibliothek (Strang UC4, Welle C)

Status: **fertig als Bibliothek** (14:20 Berlin). Die App „Source Editor“ ist nicht mehr ladbar (`DESKTOP_APPS = []`);
`app.js` ist jetzt die Editor-Bibliothek des Quelltext-Modus.

## Ergebnis (gemessen)

| Prüfung | Befehl | Ergebnis |
|---|---|---|
| Port-Scanner | `node v4/tools/port/scan.mjs --modules desktop-apps --app code-editor` | vorher 34 Stellen (gemessen an der Ausgangsdatei, Kopie im Scratchpad) → **0** |
| Wächter | `node v4/tools/guards/guard-v4.mjs --json --app desktop-apps/code-editor` | vorher 16 → **0** |
| CSS-Scope | `node v4/tools/port/scope-css.mjs code-editor desktop-apps/code-editor/code-editor.css --check` | gescoped |
| Unit/Wächter | `node --test src/apps/business-os/desktop-apps/code-editor/code-editor.test.mjs` | 8/8 grün |
| Browser | `node --test src/apps/business-os/desktop-apps/code-editor/tests/v4-api.test.mjs` | 5/5 grün (Chromium 1223; kein Kern-Stub nötig, die Bibliothek hat keinen Datenweg) |
| Abnehmer | `node -e "import('./v4/source/editor.js')"` | lädt; `resolveMonacoBaseUrl` weiter exportiert |
| Screenshots | `tests/shots/code-editor-1280.png`, `tests/shots/code-editor-420.png` | angesehen; Fehler behoben: Container-Abfrage griff nicht (Container war das Element selbst) → Hülle ist Container, 420 stapelt Dateiliste über Editor |

## Schnittstelle

```js
import { createCodeEditor } from '<business-os>/desktop-apps/code-editor/app.js';
const ed = await createCodeEditor({
  root,                              // Element im App-Fenster; die Bibliothek hängt eine Hülle [data-app="code-editor"] an
  files: [{ path, content, language? }],
  onChange(path, content, { dirty }) {},   // je Eingabe, auch Verwerfen (dirty:false)
  readOnly: false,
  activePath: 'index.js',            // optional; sonst index.js → module.json → erste Datei
  prefer: 'auto' | 'textarea',       // Monaco aus <business-os>/vendor/monaco (6 s Frist), sonst textarea
  labels: { search, files, diff, format, revert },  // optional (de-Rückfall)
  restored,                          // optional: früheres ed.snapshot()
});
ed.kind; ed.open(path); ed.activePath(); ed.getValue(path?); ed.getFiles(); ed.setFiles(files) // behält Entwürfe
ed.setReadOnly(on); ed.isDirty(path?); ed.revert(path?); ed.format(path?); ed.toggleDiff(on?); ed.focus();
ed.snapshot();  // {v:1, activePath, search, diffOpen, drafts:{path:content}} – structured clone
ed.destroy();
```

Stylesheet `code-editor.css` lädt die Bibliothek als `<link>` **innerhalb** ihrer Hülle (nichts in `document.head`
außer dem Monaco-Loader-Skript); alle Selektoren unter `[data-app="code-editor"]`.

## Was entfallen ist (gehört jetzt v4/source bzw. anderen Apps)

- **Datenweg**: `business_module_source_files`, `business_module_commits`, `business_commands` (Lesen, `startCollection`),
  Befehle `ctox.source.load/save/commit` mit `until:'accepted'`, Warteschleifen `waitForSourceFiles/File`
  (300-ms-Takt, 45-s-Frist), Modulkatalog per `fetch('./modules/registry.json')`, Rechte über `shared/permissions.js`.
  In v4 liest/schreibt der Quelltext-Modus über `shell.source(appId)` (`read`, Entwurf, `draft.publish`).
- **UI-Teile der alten App**: App-Liste links, „App öffnen“, „Neu laden“, Speichern/Commit/History, pi-Agent-Panel
  (`ctox.coding.turn`, Link in die Coding-Agent-App über `ctx.openDesktopApp`), Statuszeile („Lade Source...“,
  „… wird geladen“, „Source nicht verfügbar“, „read-only fallback“), Platzhaltertext. Diff, Formatieren und Verwerfen
  bleiben als Editorfunktionen.
- `export const manifest`, `mount(container, ctx)`, `ctx.getActionIcon`, `ensureStyles()` mit Inline-CSS.

## Befehlstypen (der alten App) – Einstufung

| Befehlstyp | Rust-Treffer (.rs) | Einstufung | v4 |
|---|---:|---|---|
| `ctox.source.load` | 6 (`rxdb_peer.rs`, `command_plane.rs`, `store.rs`) | Core-generisch | ersetzt durch `shell.source(appId).read` |
| `ctox.source.save` | 5 (dieselben) | Core-generisch | ersetzt durch Entwurf + `draft.publish` |
| `ctox.source.commit` | 3 (`store.rs`, `command_plane.rs`) | Core-generisch | ersetzt durch `draft.publish` (Version = Versionspunkt) |
| `ctox.coding.turn` | 23 (u. a. `crew/mod.rs`, `store.rs`, `mcp_channel.rs`) | Core-generisch (pi-Sidecar) | gehört der Coding-Agent-App, nicht der Editor-Bibliothek |

## OFFEN

1. **v4/source/editor.js** nutzt bisher nur `resolveMonacoBaseUrl` und baut den Editor selbst (Monaco/textarea). Ob
   Strang Q auf `createCodeEditor` umstellt, entscheidet Q (hier nichts geändert). Unterschiede: Q hat einen
   eigenen Monaco-Lader (HEAD-Prüfung, 6 s), ohne Dateiliste/Diff.
2. **Verweise auf die alte App außerhalb** (nicht geändert): `app.js:3055` importiert
   `./desktop-apps/code-editor/app.js?v=…` und erwartet `mount(container, ctx)` (Legacy-Shell);
   `v4/tools/guards/BASELINE.md:134` (Zählung 16).
3. Die Hülle trägt `data-app="code-editor"` innerhalb eines fremden App-Fensters (nur CSS-Scope). Prüfen, dass die
   Shell `[data-app]` nur am Fenster-root auswertet.
4. Monaco-Theme `business-os-dark` folgt `prefers-color-scheme`, nicht `api.shell.theme` (Bibliothek bekommt keine api).

# Gelöschte Altverzeichnisse (Auftrag Welle C)

`desktop-apps/browser/` (app.js), `desktop-apps/creator/` (app.js), `desktop-apps/explorer/` (app.js,
explorer.test.mjs), `desktop-apps/file-viewer/` (app.js, file-viewer.test.mjs) – gelöscht.

Stellen, die noch darauf zeigen (alle **nicht** geändert, außerhalb meiner Apps):

| Datei:Zeile | Verweis |
|---|---|
| `src/apps/business-os/scripts/assert-rxdb-only.mjs:688-689, 735, 740, 744` | liest `desktop-apps/file-viewer/app.js` und `desktop-apps/explorer/app.js` (Chunk-Integritäts-Wächter) – bricht mit ENOENT |
| `src/apps/business-os/scripts/command-consumer-inventory.json:8, 54` | listet `desktop-apps/file-viewer/app.js` |
| `src/apps/business-os/modules/browser/browser.test.mjs:244` | liest `../../desktop-apps/browser/app.js` (mit `.catch(() => '')`, bricht nicht) |
| `src/apps/business-os/v4/tools/guards/BASELINE.md:133, 135-137` | Zählzeilen der vier Verzeichnisse |
| `src/apps/business-os/app.js:4677-4680` | Kommentar „Legacy-Quellen unter desktop-apps/“, `DESKTOP_APPS = []` |
| `src/core/rxdb/tools/browser_rust_smoke.js:16070, 16162, 16230, 16371` | importiert `/desktop-apps/file-viewer/app.js?v=…` (Smoke-Modi `workspace-*-file-viewer`) |
| `src/core/rxdb/PORTING.md` (Zeilen 346, 347, 412-414, 435, 436, 519 u. a.) und `src/core/rxdb/revisions/PORTING.wave-269-….md:71` | historische Protokollzeilen, nennen die Pfade |
