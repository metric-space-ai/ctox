# Strang F – Shell-Rahmen v4 (frame/) – Bericht

> **Nachtrag Strang I (15.10.):** `host-stub.js` und `lab/stub-host.js` sind entfallen; der Rahmen bindet jetzt `../shell/app-host.js` ein, `window-manager.dispose()` existiert. Stand und Messwerte: `INTEGRATION.md`.


> **Nachtrag FR3 (15.10., Befunde BIND2 + K24-Vorbedingung V):**
> 1. **„Neue App“** im Launcher-Kopf (`launcher.setActions`), nur mit `apps.source.edit` (Spiegel aus
>    `appApi('shell').actor.permissions`, live) und nur, wenn der Kernel `source(appId)` hat. Öffnet ein Rahmen-Fenster
>    (`data-window-view="new-app"`, Zone wie App-Fenster, übersteht Shell-Tausch) mit der Neue-App-Fläche des
>    Quelltext-Modus (`../source/upload.js` `openAppUpload`: Ordner → App-ID → `draft.publish({base_app_ver: null})`).
>    Recht entzogen → Knopf weg, offene Ansicht schließt. `shellHandle().openNewApp()` für Tests.
> 2. **Doppelte Gruppe „DEVELOPMENT“**: Ursache `importer` mit `category: "development"` neben „Development“
>    (CSS `text-transform: uppercase` machte beide gleich). Gruppen jetzt nach `categoryKey()` (NFKC, trim, Leerraum,
>    klein); Überschrift = häufigste Schreibweise. `data-v4-group` = Überschrift, `data-v4-group-key` = Schlüssel.
> 3. **SVG-Icons ohne xmlns**: `icon_svg` bekommt xmlns beim Einlesen (`ensureSvgXmlns`, wie gen-manifest);
>    `icon_asset *.svg`, das nicht dekodiert, wird über `kernel.artifacts.manifest/read` (Rückfall: dieselbe URL)
>    gelesen, ergänzt und als data-URL gezeigt; nicht reparierbar → Monogramm.
> Abnahme `frame.test.mjs mock` 64/64, `stub` 80/80 (Pin 7706796); Bilder `lab/shots/integrated-stub-fr3-*.png`.


Stand 15.10.2026, 13:08. Klon `~/Documents/ctox-shell-v4`, alles unter `src/apps/business-os/v4/frame/`.
Kein git, `shared/*` nur importiert und nicht geändert, Kern-Stub nicht geändert.

## Was steht

| Datei | Inhalt |
|---|---|
| `index.js` | `startShell(kernel, {restored, root?, createWindowHost?})`, `stopShell() → Promise<snapshot>`, `describeShell()`, `resolveShellKernel()` |
| `launcher.js` | Desktop = Launcher: Kachelraster aus `kernel.apps`, Gruppen nach `layout.category`, Suche (Enter öffnet den ersten Treffer, Esc leert), Pin-Umschalter je Kachel |
| `taskbar.js` | Taskleiste: Desktop-Knopf, Pins, offene Fenster (Klick fokussiert, beim vorderen Fenster minimiert) |
| `host-stub.js` | Platzhalter für den P2-Host: `createWindowHost({root, appRecord, api, restored}) → {swap(nextVer), snapshot(), destroy()}`, mountet per `import('/~a/<app>@<ver>/<entry>')` |
| `frame.css` | Tokens hell/dunkel, Fenster-Chrome für den v2-Vertrag von `window-manager.js`, Container-Queries 1024/768 |
| `lab/` | `index.html` + `kernel-mock.js` (eigene Attrappe), `stub-lab.html` + `stub-host.js` (Kern-Stub `createStubKernel`), `serve.mjs`, `frame.test.mjs`, `apps/` (4 Lab-Apps, ledger in 2 Versionen), `shots/` |

Wiederverwendet (nur Import): `shared/window-manager.js` (Fenster, Chrome v2, Snap, Layout-Menü im Fenster, Persistenz-Haken),
`shared/taskbar-pins.js` (Pins kodieren/dekodieren, lokal gegen entfernt auflösen), `shared/launcher-icon.js` (Raster oder
Monogramm, Aufruf mit `kind:'app'`, damit der eingebaute Operator-Icon-Katalog nicht greift).
Nicht verwendet: `window-switcher.js` (das ist ein Overlay über der Shell, verboten), `workspace-session.js` (kennt nur
`desktop-app:`-Owner; der Shell-Snapshot ersetzt ihn), `window-layout-resolver.js` nur indirekt über window-manager.

### Verhalten
- **App-Liste nur aus `kernel.apps`** (LiveQuery). Gezeigt werden Apps mit `activated_ver`, außer `id==='shell'`. Neue Apps erscheinen
  live, entfernte verschwinden samt offenem Fenster und Pin-Kachel. Kein Katalog, keine registry.json, kein HTTP vom Rahmen.
- **Icon** = `/~a/<app_id>@<activated_ver>/<layout.icon_asset>`. **Kategorie** = `layout.category || layout.group`. `layout` kommt aus
  `appEntry.layout` oder aus `kernel.artifacts.manifest(app, ver).layout`. Ohne Icon gibt es ein Monogramm.
- **Aktivierung**: Bei `activation.onChange` (state `activated`) oder einem neuen `activated_ver` in `kernel.apps` ruft der Rahmen
  `host.swap(ver)` für jedes offene Fenster dieser App. Auch das Launcher-Icon folgt der neuen Version.
- **Fenster**: Ein Fenster je App ist der Normalfall (ein zweites Öffnen fokussiert); `api.shell.navigate(app, params)` öffnet ein
  weiteres. Neue Fenster werden um die Mitte gestaffelt. Das Chrome bauen `window-manager` und der Rahmen. Der Inhaltsbereich
  `[data-window-content]` (contain: layout paint, overflow: clip) ist der `root` des Hosts. `api` an den Host:
  `{kernel, loaderKernel, shell:{setTitle, registerHeader, assetUrl, navigate, window:{close, mode, element}}, params}`.
- **Fensterwechsel**: über die Taskleiste und Alt+` (Shift rückwärts), ohne Overlay.
- **Shell-Tausch**: `stopShell()` liefert `{version:1, windows:[{ownerId, app_id, ver, title, x, y, width, height, state,
  snapZone, appMode, stored, z, focused, app_snapshot}], focus_order, pins, search, taken_at}`. Die App-Snapshots kommen
  aus `host.snapshot()`. Danach baut der Rahmen sich ohne Schließ-Animation ab und meldet alle Kernel-Abos ab.
  `startShell(kernel, {restored})` wartet auf die erste App-Liste samt Manifesten (höchstens 1,5 s). Dann legt er die Fenster
  in z-Reihenfolge mit derselben Geometrie, Snap-Zone und Maximierung wieder an, ohne Öffnungs-Animation, und stellt Fokus,
  minimierte Fenster, Pins und Suchtext wieder her. Das neue CSS wird geladen, bevor das alte entfernt wird.
- **Schmal (≤ 768 px Container)**: dieselbe Shell. Fenster liegen vollflächig über dem Desktop, die Taskleiste zeigt nur Icons,
  das Launcher-Raster hat 4 Spalten, und der Desktop-Knopf zeigt den Launcher. Unter 600 px nutzt window-manager zusätzlich seine Sheet-Geometrie.
  Das ersetzt `mobile-host.js`. **≤ 1024 px**: Ränder und Fensterknöpfe in der Taskleiste werden schmaler.
- **Keine Zustandstexte, keine Overlays.** Der Rahmen zeigt weder Pillen noch Banner, Zähler oder Platzhalter. Eine leere Suche zeigt eine leere Fläche.
  Das Layout-Menü des window-manager liegt im Fenster-Chrome.

## Messwerte (Playwright, chrome-headless-shell 1223, Playwright 1.60)

| Lauf | Ergebnis | Shell-Tausch (stop→start, 3 Fenster) | Snapshot |
|---|---|---|---|
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | **28/28 bestanden** | 10,7 ms | 980 B |
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs stub` (Kern-Stub, `kernel.sim.swapShell`) | **25/25 bestanden** | 6,0 ms | 980 B |

Geprüft wird: Der Launcher zeigt die 3 Apps. Icon-URL und Kategorien stimmen (mock). Die Suche filtert, und eine leere Suche zeigt keinen Text.
Ein Pin erscheint in der Taskleiste. 3 Fenster sind offen und montiert, der Titel kommt aus `api.shell.setTitle`. Nach dem Shell-Tausch stimmen
Lage und Größe auf ±1 px, ebenso Snap-Zone, Fokus-Reihenfolge, fokussiertes Fenster, Formularwert und Pin. Es gibt genau einen Rahmen
und ein CSS-Link, die Kernel-Abos verdoppeln sich nicht (mock), und der Tausch dauert ≤ 2 s ohne Rückfall. Eine neue App erscheint ohne Reload (Marker auf `window`).
Eine entfernte App verschwindet samt Fenster. Bei Aktivierung tauscht der Host live auf 1.1.0. Fensterwechsel über Taskleiste und
Alt+` funktioniert. Bei 900 und 420 px gibt es keinen horizontalen Überlauf und kein Fenster über Taskleiste oder Rand. Schmal öffnet der Desktop-Knopf den Launcher.
Es gibt keine Seitenfehler. Ergebnisse liegen in `lab/shots/results-{mock,stub}.json`.

Screenshots (angesehen, korrigiert: Fenster überdeckten sich vollständig → Staffelung; Fenster-Icon fehlte nach Shell-Tausch
→ Manifeste vor dem Wiederherstellen laden; Taskleistenknöpfe mit Lücken → Breite nach Inhalt; Launcher zentriert):
`lab/shots/frame-1280.png`, `frame-900.png`, `frame-420.png`, `frame-420-launcher.png`, `frame-1280-desktop.png`,
dazu dieselben mit Präfix `stub-`.

## Abweichungen und Vertragslücken (Kern-Stub ↔ Rahmen)

1. **`startShell`-Argument**: §16.15 sagt Shell-Kernel-API. `kernel.sim.startShell/swapShell` des Stubs übergibt dagegen den
   **StubKernel** (die Shell-API liegt unter `.shell`). `resolveShellKernel()` nimmt beides an.
2. **`AppEntry`/`AppManifest` haben kein `layout`** (Icon, Kategorie). Im Stub-Lauf zeigt der Launcher deshalb Monogramme in einer
   einzigen Gruppe. Vorschlag an den Kern: `AppManifest.layout = {icon_asset, category, title?}` (aus `module.json` layout). Der Rahmen
   liest es schon heute aus Manifest oder Eintrag.
3. **Kein Ablageort für Shell-Präferenzen** (Pins) in der Kernel-API. Der Rahmen nutzt `kernel.prefs.get/set`, wenn es das gibt,
   sonst nur den Shell-Snapshot. Vorschlag: `kernel.prefs` (je Nutzer, synchronisiert) oder die App-API der Shell (`app_id 'shell'`,
   Collection `shell_prefs`).
4. **Apps ohne `activated_ver`** (state received/fetched/data-ready/failed) zeigt der Launcher nicht. Sie lassen sich nicht montieren.
   Ihr Aktivierungszustand gehört in die Zustandszone (P2), nicht in den Launcher.
5. `stub-host.js` gibt der App die Stub-App-API `kernel.appApi(app, ver)`. Nach `swap` bleibt sie an die alte Version
   gebunden (nur Labor; der P2-Host baut die API je Version neu).

## Schnittstelle zu P2 – Befund

P2 (`v4/shell/`) hat inzwischen eigene Dateien: `shell.js` mit eigenem `startShell/stopShell`, `app-host.js` mit
`createAppHost({kernel,…}).openWindow({windowId, appId, ver, restoredRec, onClose})`. Dieser Host **baut ein eigenes Fenster-Chrome**
(`section.v4-window`, Kopfzeile, Steuerung). Die vereinbarte Signatur `createWindowHost({root, appRecord, api, restored}) →
{swap, snapshot, destroy}` exportiert er nicht. Damit gibt es zwei Chrome-Besitzer und zwei `startShell`. Empfehlung: P2 exportiert
zusätzlich `createWindowHost`, das nur Inhalt + Zustandszone in `root` rendert. Swap, Snapshot, 10-s-Rückfall und `window_state`
bleiben bei P2, das Chrome hat der Rahmen. Der Rahmen bindet ihn dann über `startShell(kernel, {createWindowHost})` ein, ohne Codeänderung.
Bis dahin läuft `host-stub.js`.

## Ersetzte app.js-Funktionen (Ist-Aufnahme A1/A2)

- Fenster-Sitzung und Geometrie: `restoreWorkspaceSession`, `persistWorkspaceSession`, `scheduleWorkspaceSessionPersist`,
  `readWorkspaceSessionSnapshot`, `primeWindowGeometryCache`, `read/persistWindowGeometryLocalCache`, `mergeWindowGeometryCache`,
  `createWindowGeometryPersistence`, `flushGeometryPersist`, `currentWindowGeometryScope`, `windowGeometryRecordId`,
  `windowGeometryDocumentMatchesCurrentScope`, `isLegacyWindowGeometryDocument`. Ersatz ist der Shell-Snapshot (`stopShell`/`startShell`).
- Fokus-Serien: `beginPreferredDesktopAppFocus`, `schedulePreferredDesktopAppFocus` (11× setTimeout bis 42 s),
  `clear…`. Sie entfallen, weil die Fokus-Reihenfolge im Snapshot steht.
- Öffnen und Fenster: `openDesktopApp`, `openWindowedModule`, `createWindowedModuleHost`, `findDesktopWindow`, `restoreAndFocusWindow`,
  `dispatchDesktopAppLaunch`, `desktopIconElementForApp`, `desktopIconAnchorRect`, `syncDesktopOpenIconStates`, `windowHeaderOptionsForModule`
  (Titel/Kopfzeile kommen jetzt über `api.shell`).
- Launcher und Katalog: `listLaunchTargets`, `startMenuItemForTarget`, `openLaunchTarget`, `listDesktopApps`, `desktopAppTargetAvailable`,
  `moduleLaunchesAsDesktopApp`, `moduleAppearsAsWindowTarget`, `desktopAppDescriptorForModule`, `moduleAppearsInSwitcher`, `renderTabs`,
  `renderModuleTab`, `renderNavigationDrawer`, `refreshModules` (Katalog-Neuladen → Push von `kernel.apps`), `getRegisteredSvgIcon`,
  `registerCustomModuleIcons`.
- Taskleiste: `isTaskbarPinned`, `pin/unpin/toggleTaskbarPin`, `readTaskbarPins`, `persistTaskbarPins`, `normalizeTaskbarPins`,
  `looksLikeLegacyAllPins`, `runningDesktopAppTargets`, `desktopAppIsRunning`, `desktopAppIsFocused`, `DEFAULT_TASKBAR_PIN_IDS`
  (es gibt keine eingebauten Standard-Pins mehr), sowie die WEGFALL_SYNC-Hydration der Pins.
- Haltepunkte und Hülle: `compactShellMediaMatches` (matchMedia 900 → Container-Queries 1024/768), `ensureShellStylesheets`,
  `loadShellUiModules`, `mobile-host.js`/`mobile-apps.json` (schmale Darstellung = Layout derselben Shell).
- Nicht ersetzt (andere Stränge oder offen): Crew/Begleiter, Einstellungen/Konto-Drawer, Quelltext-Modus, Zustandszone,
  `buildWindowContextItems`/`handleWindowContextRequest` (Kontextmenü), `moveTaskbarPinBefore` (Pins ziehen), `onShellKeyboardShortcut`
  (nur Alt+` umgesetzt), Theme/Desktop-Stil, i18n über `api.shell.locale` (der Rahmen hat nur ein Mini-Wörterbuch de/en).

## Offene Punkte

1. `window-manager.js` hat kein `dispose()`. Je Shell-Tausch bleiben ein `resize`-Listener auf `window`/`visualViewport` und ein
   ResizeObserver auf abgehängten Elementen zurück. Das ist harmlos, aber ein Leck je Tausch. Braucht eine Ergänzung in `shared/` (nicht mein Whitelist).
2. Schmal (Touch) ist der Pin-Umschalter nur bei angehefteten Apps sichtbar, sodass sich schmal nichts neu anheften lässt. Lösung: langes Drücken
   im Fenster-Chrome oder in der Kachel (ohne Popup).
3. Pins lassen sich nicht umsortieren (`moveTaskbarPinBefore` fehlt).
4. Im Rahmen-Snapshot fehlt noch die Ablage nach `window_state` (IDB). Das macht heute der Loader/Stub (`swapShell` legt ihn ab).
   Fensteranordnung je Tab beim Neuladen (§9): offen, solange `window_state` keine Shell-API hat.
5. Die Shell-Imports gehen relativ (`../../shared/…`). Sobald der Service Worker (P1) `ctox:shared/…` umschreibt, auf diesen
   Spezifizierer umstellen.
6. Geometrie-Labor (`scripts/shell-v2-geometry-lab.mjs`) ist noch nicht gegen den v4-Rahmen gelaufen.
