# ZONE2 – Zonen-Garantie für jede App (K24, Befund T3-1)

Owner 15.10.: Die Kontroll-Icons schweben bewusst über der App. Darunter darf die Shell aber keine Schaltelemente
zulassen. K24 (T3, Bind-Klon) war deshalb ROT: Die K24-Probe kennt das Kit nicht, ihr Knopf „Agent“ lag unter dem
Knopfcluster. In der Code-Ansicht lag „Veröffentlichen“ unter `span.shell-window-layout-glyph`.

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf f237b4dea. Kein Commit, nur Arbeitsbaum.

## Ursache

- **App ohne Kit:** Bisher rückte der Rahmen nur Pane-Köpfe des Kits aus (`pane-heads.js markReserved`). Eine App mit
  eigenem `<form>` oben (wie die K24-Probe) wurde nicht erfasst.
- **Code-Ansicht:** Der Rahmen-Wirt `.v4-src-host` hält die Zone bereits per `padding-top: var(--shell-zone-height)`
  frei. Der K24-Lauf öffnet die Ansicht aber über den Test-Einstieg `openSource` in `v4/shell/kernel-k1.js` (nur im
  Bind-Klon). Der legt einen eigenen Wirt mit Inline-Stil und ohne Zonen-Abstand an. Die Ansicht selbst hielt die
  Zone also nicht frei, sie verließ sich auf den Wirt.
- Nebenbefund Bind-Klon, nicht behoben: Dort ist der Icon-Header nur 25 px groß. `window-manager.js` setzt
  `--shell-v2-icon-size` aus dem Launcher-Anker, gemessen mit einer Bind-Probe auf `notes`. Die Zonenmaße hängen
  davon nicht ab, sie sind in `frame.css` fest.

## Lösung

1. **`v4/frame/pane-heads.js`: `createZoneGuard`, `zoneRects`, `zoneHits`** (neu, allgemein).
   - Geprüft werden interaktive Elemente: `button, a[href], input, select, textarea, summary, [role=button]` und
     `[tabindex]` ≥ 0. Prüffrage: Schneiden sie das Rechteck der Icon-Zone oder der Cluster-Zone? Die Maße kommen aus
     den Zonen-Variablen des Fensters.
   - Ausgenommen ist, was das Kit behandelt: `.ctox-pane-header`, `.ctox-filterbar`, markierte Zeilen und flache
     Geschwisterzeilen direkt unter einem Kopf. Für die Geschwisterzeilen gilt dieselbe Regel wie in `markReserved`,
     unabhängig davon, ob dessen Markierung in diesem Bild schon steht.
   - Bei einem Treffer setzt der Rahmen `data-v4-zone-guard="top"` an die Wurzel. Schneidet danach noch etwas, kommt
     `"top right"` dazu.
   - Geprüft wird die Ruhelage, also das Rechteck plus `scrollTop`/`scrollLeft` der Vorfahren. Die Scrollposition
     entscheidet damit nicht über die Einrückung.
   - Gemessen wird ohne Rückkopplung: Das Attribut wird kurz entfernt und im selben Task neu gesetzt, ohne Bild
     dazwischen. Eine unsichtbare Wurzel (Code-Modus) wird nicht gemessen.
   - Auslöser: ResizeObserver und MutationObserver, höchstens eine Messung je Bild.
   - Bei einer neuen Wurzel (Remount) übernimmt der MutationObserver-Mikrotask den alten Zustand sofort. Die Messung
     im nächsten Bild bestätigt ihn.
   - `detach()` lässt das Attribut stehen. Beim Shell-Tausch (Doppelpuffer) bleibt der alte Rahmen bis zum Abbau
     sichtbar. Ohne diese Regel fiel die Probe „Doppelpuffer: Pixelprobe mitten im Tausch“ auf 95,22 %.
2. **`v4/frame/index.js`** (nur die Zonen-Stelle `attachChrome` und `window:closing`):
   - Je App-Fenster `createZoneGuard({ pick: (c) => c.querySelector(':scope > .v4-app-root') })` auf dem
     Inhaltsbereich. Er erfasst jeden Mount und Remount.
   - Abbau bei `window:closing` und beim Shell-Tausch (`state.unsubs`).
3. **`v4/frame/frame.css`:**
   - `.shell-window-content > .v4-app-root[data-v4-zone-guard~="top"] { top: var(--shell-zone-height) }`. Die ganze
     Content-Box beginnt unter der Zonenhöhe, auch absolut positionierte Kinder.
   - `~="right"` setzt `right: var(--shell-controls-inset)`.
4. **Code-Ansicht (`v4/source/source-mode.js` und `.css`):**
   - Die Ansicht hängt dieselbe Garantie an ihre eigene Wurzel `.v4-src`. Bei einem Treffer bekommt sie
     `padding-top: var(--shell-zone-height)`.
   - Im Rahmen-Wirt schneidet nichts, also kein Attribut und pixelgleich. In einem Wirt ohne Zonen-Abstand (wie
     `kernel-k1 openSource`) rückt der Kopf mit „Veröffentlichen“, „Verwerfen“, Diff und „…“ selbst unter die Zone.

## Labor (`frame.test.mjs`, Block `zone2()`, `ZONE2_ONLY=1` allein)

Lab-App `lab/apps/bare/1.0.0` kennt das Kit nicht, wie die K24-Probe:

- eine Formularzeile mit „Links“, Eingabe, „Tick“ und „Agent“;
- ein absolut positionierter Knopf oben rechts.

| Fall | Ergebnis |
|---|---|
| App ohne Kit: „Links“ (unter dem Icon), „Agent“, „Tick“ und „Abs“ (unter dem Cluster) sind frei | Treffertest am Mittelpunkt = Knopf, echter Mausklick zählt 1× je Knopf; 1280 und 900 px |
| Gegenprobe ohne Garantie (Stil neutralisiert) | „Links“ ist von `div.shell-window-v2-icon` verdeckt, „Abs“ von `button.shell-window-control` → rot |
| Kein Flackern | Mount: genau ein sichtbarer Wechsel `0px → 74px`. Danach Maximieren, Wiederherstellen und 20 DOM-Einfügungen: kein Wechsel (rAF-Abtastung von `top`/`right`) |
| Kit-App unberührt | studio mit Kit-Kopf: kein Attribut |
| Lab-Apps notes und crm 2.0.0 | Beide ohne Kit, mit Eingabe unter dem Icon (notes oben links, crm bei y 53 unter der v2-Kopfzeile). Die Garantie greift dort, das ist ein echter Fund in diesen Lab-Fixtures |
| Code-Ansicht im Rahmen-Wirt, 1280 und 900 (stub) | Veröffentlichen, Diff und „…“ frei; „Verwerfen“ ist ohne Entwurf ausgeblendet; kein Attribut nötig |
| Code-Ansicht im Wirt ohne Zonen-Abstand | Garantie `top`, Kopf frei, Diff per echter Maus umschaltbar |
| Gegenprobe Code-Ansicht | ohne Garantie liegt „Veröffentlichen“ unter `span.shell-window-layout-glyph`, exakt Befund T3-1 → rot |
| G8 (FR6a) studio ohne Kit | angepasst: Mit Garantie gibt es keine Treffer. Die bisherige Mutationsprobe läuft jetzt mit neutralisierter Garantie und findet die Knöpfe weiter |

Bilder in `lab/shots/`:

- `integrated-zone2-bare-1280.png`, `integrated-zone2-bare-900.png`
- `integrated-stub-zone2-source-1280.png`, `integrated-stub-zone2-source-900.png`
- `integrated-stub-zone2-source-hostless-1280.png`

## Abnahme

| Prüfung | Ergebnis |
|---|---|
| Rahmen-Labor mock | **174/174** (Basis 167/167 + 6 ZONE2 + 1 neuer G8-Fall) |
| Rahmen-Labor stub Pin 68b83a3 | **197/197** (davon 10 ZONE2 + 1 neuer G8-Fall) |
| Rahmen-Labor stub Pin 8489b4e | **219/219** (Basis 208/208 + 10 ZONE2 + 1 neuer G8-Fall) |
| G0 / FR6d-G0 / TOP-1 G0 | unverändert 1 bzw. 100 % je Bereich (mock und stub) |
| Shell-V2-Vertrag (`scripts/assert-shell-v2-contract.mjs`) | 38/38 |
| Geometrie-Labor v4, 1280/900/420 | 117/120, Zonen-Fälle 4/4, **G8 0 Fehler**. Rot sind nur desktop 900 (G6), invoices 900 (G1, G4) und invoices 420 (G4). Dieselben drei sind auch mit neutralisierter Garantie rot (Gegenlauf `--app invoices,desktop,notes,crm`) → **G4 2 → 2, nicht schlechter** |
| Ratchet (`v4/debt/ratchet.mjs`) | meldet FEHLER (guard.total 1838 → 1858), aber **nicht durch ZONE2**: Mein Diff enthält 0 Zeilen mit den gestiegenen Regeln (commandBus, requestNative, requireRevision, startCollection, until-accepted-terminal, vocab:en-*). Die Treffer liegen im fremden, nicht eingecheckten `v4/shell/workjet/` (Strang E24/WJB-0) |

## Grenzen und Übergabe

- Der K24-Lauf nutzt die Dateien des **Bind-Klons** (`~/Documents/ctox-shell-v4-bind`). Kriterium 8 wird dort erst
  grün, wenn `pane-heads.js`, `index.js`, `frame.css`, `source-mode.js` und `source-mode.css` aus diesem Strang
  übernommen sind. Danach K24 (8) mit Mausklick neu fahren.
- `position: fixed` in einer App wird nur zum Inhaltsbereich verschoben, weil `contain: layout paint` den Bezug
  bildet. Schneidet danach noch etwas, folgt die rechte Einrückung. Eine dritte Stufe gibt es nicht.
