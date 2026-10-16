# WJB-M – Workjet-Mobil-Host im v4-Rahmen (Strangbericht)

Stand 16.10.2026, Klon `~/Documents/ctox-shell-v4` (Arbeitsbaum, kein git). V2-Quelle: `git show e35a116ae:src/apps/business-os/mobile-host.css`.
Geändert: `v4/frame/frame.css` (Abschnitt „WJB-M Mobil-Host“ am Ende, nur additiv), `v4/frame/lab/frame.test.mjs`
(Block `mobile()` vor TOPBAR, Schalter `MOBILE_ONLY`, Aufruf im Gesamtlauf nach ZONE2), dieser Bericht.
Nicht angefasst: `v4/frame/index.js`, `v4/shell/**`, `modules/**`, `v4/manifests/**`, Wächterregeln, `lab/index.html`.

## Ausgangslage (gemessen, vor der Änderung)

- Die Hostbrücke ist im Labor eingehängt: `startShell` lädt `../shell/workjet/index.js` dynamisch, auch mit der
  Kernel-Attrappe (`createV2Compat` wirft nicht). Beleg: `window.workjetHostBridge` gesetzt, Posts `shell.ready` +
  `catalog.replace`. Kein Haken nötig.
- Der Mobil-Katalog ist eine Momentaufnahme der ERSTEN App-Liste (`mobileCatalogOf(state.apps)` nach
  `appsLoaded`). Mit `lab/index.html` ist er leer (`apps: []`): die Attrappen-Zeilen tragen kein `layout`, das kommt
  erst asynchron aus dem Manifest. Das Labor baut deshalb eine eigene Seite (per `route`, wie die Z45-Probe-App) mit
  Zeilen `layout_known: true` + `layout.presentation.mobile` (Form wie §16.19 Nr. 6 / `v4/manifests/*`).
- Mit Attribut und OHNE WJB-M-CSS war der Bildschirm leer (weiß): die V2-Extraktion in `shared/shell-v2-topbar.css`
  hängt V2s `html, body, .app-shell { height: 100%; min-height: 100dvh; background }` an die Kopfleisten-Hülle
  `[data-v4-topbar]`; die Hülle (z-index 60) lag damit 844 px hoch über Desk und Fenstern. Gleiches für
  `[data-v4-v2]` (Startmenü-Hülle 844 px hoch unterhalb des Viewports).
- Das Touch-Raster (4/5/6 Spalten, 64-px-Glyphen, 44-px-Ziele innerhalb der Hüllen, reduzierte Bewegung innerhalb der
  Hüllen) bringt `shell-v2-desktop.css` bereits wörtlich aus V2 mit (Abschnitt „===== mobile-host.css =====“,
  generiert von `extract-desktop-css.mjs`). Es griff nur nicht sichtbar, weil die Kopfleisten-Hülle alles verdeckte.

## Selektoren: v4 ↔ V2

| V2 (`mobile-host.css`) | v4 (`frame.css` WJB-M bzw. Fundstelle) | Wirkung |
|---|---|---|
| `html, body, .app-shell` 100 %, overflow hidden | `.v4f[data-v4-frame]` Raster `minmax(0,1fr)`, `.v4f > .v4f-desk` `grid-row: 1 / -1` | Desk füllt den Viewport |
| `.topbar`, `.drawer`, `.drawer-backdrop`, `.shell-window-switcher`, `.ctox-maintenance-banner` | `.v4f > .v4f-top` (Hülle `[data-v4-topbar]`: Kopfleiste mit Reitern, Konto-/Einstellungsblatt, Fenstermenü `.v4f-task-menu`) | display none |
| `[data-shell-start-menu]` | `.v4f > .v4f-start` (Hülle von `[data-v4-launcher]`, launcher.js) | display none |
| `[data-taskbar]` | keine eigene Leiste seit FR6c (taskbar.js ohne DOM); Reiter in `.v4f-top` | mit der Kopfleiste aus |
| `[data-chat-dock]` | `.v4f > .v4f-crew` (crew-bar.js, `[data-v4-crew-bar]`) | display none |
| `.shell-notifications` | `.v4f .v4f-zone-slot`, `.v4f .v4-status-zone__list` (Meldungen am App-Icon, M-14) | display none |
| `.shell-window-layer` | `.v4f .v4f-layer` | inset 0, 100 % |
| `.shell-window` / `.shell-window[data-workjet-mobile-active="true"]` | `.v4f .shell-window:not([data-workjet-mobile-active="true"])` / `…[data-workjet-mobile-active="true"]` (Marke von host-bridge.js; Fenster `data-owner-id="v4app:<app>#n"`, `data-window-app`) | nur aktives Fenster, vollflächig, ohne V2-Rahmenkante/Schatten |
| `.shell-window-header`, `.shell-window-resize` | `.shell-window > .shell-window-v2-icon`, `> .shell-window-header`, `> .shell-window-resize` | display none |
| `.shell-window-content`, `> .shell-window-module-root` | `.shell-window[…active] > .shell-window-content` (darin `.v4-app-root`, schon inset 0) | füllt das Fenster |
| `.desktop-module`, `.desktop-icons`, `.desktop-icon …`, `@media 600/840` | unverändert aus `shell-v2-desktop.css` (`:root[data-workjet-mobile-host] :where([data-v4-v2]) …`, `.desktop-icon[data-target]` aus desktop.js) | Raster 4/5/6 Spalten |
| `:where(button, select, input, [role=button])` 44 px, Icon-Knopf min-width 44 | gleiche Regel wörtlich mit `html[data-workjet-mobile-host="true"]` (gleiche Spezifität 0-1-1) | gilt jetzt auch im Fensterinhalt |
| `@media (prefers-reduced-motion: reduce)` | gleiche Regel wörtlich | 0,01 ms für alles |
| — | `.v4f [data-v4-desktop-surface]:focus { outline: none }` | v4-Zusatz, s. Abweichungen |
| `[data-v4-retiring]` | keine Regel nötig: die Brücke filtert abgelöste Rahmen; deren aktive Marke bleibt bis zum Umschalten stehen | — |

## Breakpoints: Entscheidung mit Beleg

- `scripts/assert-shell-v2-contract.mjs` prüft nur `modules/*/index.css` (`moduleDirs('modules')`, Zeilen 25-37); die
  `@media`-Regel (Zeile 43) erreicht `v4/frame/frame.css` nicht. `scripts/assert-design-system.mjs` liest nur
  `app.css`, `shared/base.css`, `design-lab.html` und Modulmarkup. `guard-v4` lext CSS, prüft dort aber weder
  Vokabular noch Englisch (`ext === '.css'`-Ausnahmen, Zeilen 520/530). `@media` wäre in frame.css also erlaubt.
- frame.css selbst hält im Kopf fest „Haltepunkte nur ueber Container-Queries 1024/768“.
- Entscheidung: **WJB-M braucht keinen eigenen Breiten-Haltepunkt.** Die einzigen breitenabhängigen V2-Regeln sind
  (a) die Rasterspalten – die liefert die generierte `shell-v2-desktop.css` schon wörtlich mit V2s `@media` 600/840
  (Zeilen 664-677), gemessen 4/5/6 Spalten – und (b) die `list-detail`-Spalten auf `.shell-window-module-root`, die im
  v4-DOM kein Gegenstück haben (s. Abweichungen). Einzige `@media` im Abschnitt: `prefers-reduced-motion` (wie schon
  frame.css Zeile 205).

## Abweichungen von V2 (mit Grund)

1. **Zonenmaße 0 im aktiven Fenster** (`--shell-icon-inset`, `--shell-icon-zone-height`, `--shell-controls-inset`,
   `--shell-controls-zone-height`, `--shell-zone-height` = 0 px, `!important`, weil index.js `--shell-controls-inset`
   inline setzt). Ohne Fensterkopf gibt es nichts zu reservieren; sonst rückten Kit-Köpfe und die ZONE2-Garantie den
   Inhalt 74 px unter einen unsichtbaren Kopf ein. V2 hatte diese Zonen nicht.
2. **`list-detail`-Spalten (V2 `body[data-workjet-mobile-presentation="list-detail"] .shell-window-module-root`
   42/58 bzw. 38/62) entfallen.** v4 hat keinen Shell-Modulroot mit Spaltenraster; die Spaltenstruktur gehört der App
   (Kit `ctox-workspace`, `pane-narrow.js` Liste→Detail unter 600 px Fensterbreite, Container `business-app-window`
   1024/768). Weil das aktive Fenster = Viewport ist, greifen diese Fenster-Haltepunkte im Mobil-Host genau an der
   Viewportbreite. `body[data-workjet-mobile-presentation]` wird von der Brücke gesetzt (Labor: `form`) und steht Apps
   als Haken zur Verfügung.
3. **V2 `.workspace-frame`/`.workspace-pane-*`/`.module-content`-Regeln entfallen**: kein v4-Gegenstück (keine
   Shell-Workspace-Panes; Apps mounten in `.v4-app-root`).
4. **Aktives Fenster ohne V2-Rahmenkante**: zusätzlich `background: var(--v4f-surface) !important` und
   `transform: none !important`, weil das Shell-V2-Chrome den 6-px-Verlauf als `border-box`-Hintergrund mit
   `!important` malt (bei border 0 sonst ein 6-px-Streifen oben/links) und der Fenstermanager beim Öffnen per transform
   animiert. `display` des aktiven Fensters bleibt beim Fenstermanager (Vertrag-v2-Fenster sind `block`; ein minimiertes
   behält sein inline `display: none`). V2 erzwang `display: flex`.
5. **Fokusrahmen der Desktop-Fläche aus** (nur Mobil-Host): `showDesktopOnly` fokussiert `[data-v4-desktop-surface]`
   (tabindex -1); im Screenshot zog das bei jeder Rückkehr einen blauen Rahmen um den ganzen Bildschirm. V2 unterdrückte
   ebenso die Fokus-Halos der Symbole.
6. **Hash statt Hostbefehl**: Schritt 3 geht per Hostbefehl `app.open` mit `appId: 'desktop'`; v4 hat keinen
   Hash-Router (host-bridge.js ruft `shell().showDesktop()`).

## Labor-Block MOBILE (`MOBILE_ONLY=1`, nur mock)

Eigene Seite je Breite 390×844, 600×960, 900×1200, Attribut per `addInitScript` vor jedem Skript (bei fehlendem
`documentElement` über MutationObserver), Antworten über `workjetBusinessOsPostMessage` mitgeschrieben. Apps: notes,
tasks, ledger (desktopOnly), bare (Lab-App mit vier Knöpfen und Klickzähler), crm, studio, alpha, beta, mira (Crew).

- Schritt 0: Brücke eingehängt, Katalog enthält die Mobil-Apps, nicht `alpha`.
- Schritt 1 (nach `app.open desktop`): `.v4f-top`, `.topbar`, `.module-tabs`, `[data-shell-start]`, `[data-v4-launcher]`,
  `[data-v4-crew-bar]` – Fläche 0 bzw. display none in der Vorfahrenkette, `checkVisibility` false; Desk = Viewport;
  Raster aus Layout-Lage der Symbole (`offsetTop`/`offsetLeft`, Hover hebt per transform) 4/5/6 Spalten, zweite Zeile
  links bündig, alle im Viewport, erstes Symbol per `elementFromPoint` erreichbar, erste Zeile oben (< 48 px).
- Schritt 2 (`app.open notes`, dann `app.open bare`): genau ein aktives Fenster (bare), Rechteck = Viewport ±1 px;
  Icon-Header, Kopfzeile, Griffe, Meldungs-Badge unsichtbar; notes verborgen; Chrome bleibt aus; alle vier Knöpfe frei
  am Mittelpunkt, per `page.mouse.click` getroffen (Zähler je 1), je ≥ 44 px hoch; `body` trägt `bare`/`form`.
- Schritt 3 (`app.open desktop`): alle Fenster verborgen, Raster wieder da, Chrome aus.
- app.state-Folge an Workjet: `notes:opening`, `notes:active`, `bare:opening`, `bare:active`, `desktop:active`.
- reduzierte Bewegung (`reducedMotion: 'reduce'`): Fenster, Symbol, App-Knopf mit `1e-05s`.
- Gegenprobe ohne Attribut (390, 900): Kopfleiste, Reiter, Startknopf, Crew-Leiste sichtbar, Fenster mit Kopf, keine
  Brücke, keine Posts.
- CSS-Gegenprobe (einmalig von Hand, nicht im Block): frame.css ohne den WJB-M-Abschnitt → 25 von 36 Prüfungen FAIL
  (u. a. Schritt 1 „Chrome unsichtbar“, Rechteck, Kopf, andere Fenster, Knöpfe, reduzierte Bewegung `0s`).

## Befunde an der Hostbrücke (nicht Rahmen-CSS; im Block als `BEFUND`-Zeile statt `check()`)

**WJB-M-1** – nach `app.open desktop` bleibt `data-workjet-mobile-active="true"` am zuletzt aktiven Fenster (bare).
Ursache: `openApp('desktop')` ruft `markActiveWindow` nicht; der MutationObserver beobachtet nur `childList`, Minimieren
ändert nur `style`. Sichtbar folgenlos (Fenster minimiert), Zustand falsch.

**WJB-M-2** – Desktop-Symbol antippen (echte Maus) nach der Rückkehr zum Desktop: die getippte App (crm) erscheint
NICHT; Workjet erhält `bare:active` (zweimal). Ursache: der Fenstermanager lässt `is-focused` am minimierten Fenster
stehen (gemessen: `shell-window is-mobile-sheet is-focused`, `display:none`), `markActiveWindow` nimmt
`shellWindows('.shell-window.is-focused')[0]` (erstes im DOM = bare) und befördert es, weil `activeAppId` durch den
Klick schon `crm` ≠ `desktop` ist. Beleg: `shots/integrated-mobile-icon-tap-390.png` (Kontakte als „offen“ markiert,
Bildschirm zeigt nur den Desktop).

Kleinster Fix in `v4/shell/workjet/host-bridge.js` (Strang D), im Labor per `route` gegen die echte Datei erprobt
(beide Befunde behoben, Folge `desktop:active` → `tasks:active`):

```js
// markActiveWindow: nur sichtbare, oberste Fokus-Fenster zählen
const focused = shellWindows('.shell-window.is-focused').filter((el) => el.style.display !== 'none')
  .sort((x, y) => (Number(x.style.zIndex) || 0) - (Number(y.style.zIndex) || 0)).at(-1) || null;
// openApp('desktop'): nach shellHandle()?.showDesktop?.();
queueMicrotask(markActiveWindow);
```

Danach im Block die beiden `BEFUND`-Ausgaben in `check()` umwandeln (Bedingungen stehen schon: `stale.length === 0`
bzw. `tapOk`).

## Abnahme (selbst gelaufen, aus src/apps/business-os)

```
MOBILE_ONLY=1 node v4/frame/lab/frame.test.mjs mock      → 36/36 bestanden (BEFUND WJB-M-1/-2 je Breite „offen“)
node v4/frame/lab/frame.test.mjs mock                    → 210/210 bestanden (Basis 174 + 36 MOBILE), 0 FAIL
                                                           darunter PASS G0 Shell-Treue, G0 Mutationsprobe,
                                                           PASS FR6d-G0 Startmenü/Desktop/Crew = app.css-Pfad, FR6d-G0 Mutationsprobe
CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8489b4e node v4/frame/lab/frame.test.mjs stub
                                                         → 219/219 bestanden, 0 FAIL (MOBILE läuft nur mock)
node scripts/assert-shell-v2-contract.mjs                → Shell-V2-Vertrag (statischer Teil) OK: 38/38 Apps
node v4/debt/ratchet.mjs                                 → ratchet: guard 1838 · scan 3 · ratchet: OK
node scripts/assert-design-system.mjs                    → Business OS design-system contract OK   (Design-System-Wächter)
node --test v4/tools/guards/guard-v4.test.mjs            → tests 74 · pass 74 · fail 0
guard-v4 --json --include-tests, Treffer in frame.css + MOBILE-Block → 0
```

## Screenshots (`v4/frame/lab/shots/`, alle selbst angesehen)

- `integrated-mobile-desktop-390.png`, `-600.png`, `-900.png` – Desktop-Raster 4/5/6 Spalten, keine Chrome
- `integrated-mobile-app-390.png`, `-600.png`, `-900.png` – bare vollflächig, Knöpfe 44 px, kein Kopf
- `integrated-mobile-home-390.png`, `-600.png`, `-900.png` – zurück auf dem Desktop (offene Apps grau wie V2 `is-app-open`)
- `integrated-mobile-icon-tap-390.png` – Beleg WJB-M-2
- `integrated-mobile-control-390.png`, `-900.png` – Gegenprobe ohne Attribut (Kopfleiste, Reiter, Crew-Leiste, Fensterkopf)

## Was der Orchestrator noch einbauen muss

1. host-bridge.js: Fix oben (WJB-M-1, WJB-M-2), dann BEFUND → `check()` im MOBILE-Block.
2. Mobil-Katalog: `mobileCatalogOf` liest einmalig die erste App-Liste. Kommt `layout` erst aus dem Manifest (Kern
   ohne `layout` in kernel.apps) oder wird eine App später aktiviert, fehlt sie im Katalog bis zum nächsten
   `startShell`. Entweder Katalog bei `applyApps` neu melden (`catalog.replace`, V2 kannte nur die statische Datei)
   oder sicherstellen, dass kernel.apps `layout` immer mitliefert (§16.19 Nr. 6).
3. Optional Stub-Variante des Blocks (Kern-Stub liefert layout über publishAppVersion nach dem Start → derselbe
   Katalog-Punkt; daher heute nur mock).
4. Kein Haken in index.js nötig: Brücke, `shell: () => shellHandle()`, `navigate`, `showDesktop` und Mobil-Katalog sind
   eingehängt und im Labor wirksam.
