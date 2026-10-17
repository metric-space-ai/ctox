# PAL – Kantenfarben aus dem Logo (Owner-Mangel M-17)

Stand: 16.10.2026, Strang PAL. Arbeitsverzeichnis `src/apps/business-os` (Zweig `shell-v4/prototype`, Basis `51b796217`).

## 1. Ursache

- `shared/window-manager.js`, `framePaletteFromIcon` → `shellV2EdgeJointsFromRgba(rgba, w, h, bg)`: die Randfarben
  (`top_joint`, `left_joint`) entstehen aus einem 3 × 6 px-Streifen an der oberen rechten bzw. unteren linken Icon-Ecke
  (48 × 48 gerastert). Mit gesetztem `bg` (immer der Fall: berechneter Hintergrund des Icon-Halters
  `.shell-window-v2-icon`, `--shell-v2-surface-alt` = `#141b24`) wurde **jeder** Streifenpixel über `bg` verrechnet,
  auch völlig durchsichtige. Bei einem Logo mit runden, durchsichtigen Ecken liegt der ganze Streifen in der Rundung
  (Radius 14/64 → 10,5 px bei 48 px, Streifen 3 × 6 px) → Kante = Halter-Hintergrund `#141b24`.
- Eingeführt mit **`002b8ae5c`** („Shell v4 Welle 2: Rahmen = Shell-V2 exakt (FR6a) …“, 15.10.2026 23:35). Davor
  (`1d58d3d52`, Shell-V2 auf main) kamen die Fugenfarben aus `shellV2FramePaletteFromRgba` (`edgeBuckets`), die
  Pixel mit Alpha < 48 überspringt und nach Alpha gewichtet – durchsichtige Ecken flossen dort nicht ein. Belegt mit
  `git log -S shellV2EdgeJointsFromRgba` und `git log -S 'getComputedStyle(host).backgroundColor'` (je nur `002b8ae5c`).
  Auf `origin/main` gibt es `shellV2EdgeJointsFromRgba` nicht; der Bruch betrifft nur den v4-Zweig.
- Prüfung G9 (Rahmen-Labor `frame.test.mjs:3693`, ebenso `maengel.mjs` M17) misst den Eckpixel des Icons
  (`ir.right − 2, ir.top + 1`). Bei durchsichtigen Ecken zeigt der den Halter-Hintergrund; neben einer dunklen Kante
  ergibt das ΔE 1,8 → fälschlich „bestanden“.

## 2. Fix (`shared/window-manager.js`, nur die Palettenberechnung)

- `shellV2EdgeJointsFromRgba`: je Zeile (oben) bzw. Spalte (links) des 12-%-Streifens zählt nur der äußerste
  **deckende** Logopixel (Alpha ≥ 230 ≈ 0,9) und die `depth` Pixel dahinter, gesucht bis 34 % Tiefe. Kanalweiser Median.
  Vollflächiges Icon: das ist exakt der alte Streifen (gleiche Pixelmenge, gleicher Median) → pixelgleich. Logo mit
  runden, durchsichtigen Ecken: die Pixel direkt hinter der Rundung → Logofarbe.
- Nur für **Kacheln** (alle vier Randmitten deckend). Freigestellte Glyphen (durchsichtiger Rand) und Streifen ohne
  deckenden Pixel behalten die bisherige Rechnung (über `bg` verrechnet), weil dort tatsächlich der Halter an die Kante
  grenzt. Rückgabe bleibt `{ top, left }`, neu optional `surface`.
- `surface` (nur Kachel mit durchsichtigen Ecken): Mittel aus oberer und linker Randfarbe; `framePaletteFromIcon` gibt
  es als `surface_alt` mit (wirkt über das schon vorhandene `--shell-v2-surface-alt` nur auf den Icon-Halter). Ohne das
  blieben die Ecken als dunkle Keile zwischen Logo und Kante stehen. Bei vollflächigen Icons wird nichts gesetzt.
- Alles berechnet, nichts von Hand; `frame_palette` im Manifest bleibt Rückfall beim Laden.
- Grenze: eine Füllfarbe für beide Ecken. Bei Verlaufs-Logos weicht die Füllung in den abgerundeten Ecken oben links und
  unten rechts leicht vom Verlauf ab (`out/pal/neu-pal-gradient.png`). An den Fugen zur Kante ist es nahtlos
  (ΔE 1,3 / 2,7).

## 3. G9 neu: gemessen am deckenden Logopixel

- Neu `v4/tools/geometry/g9.mjs`: im Icon-Bitmap (Rendergröße) der deckende Pixel (Alpha ≥ 250), der der oberen
  rechten bzw. unteren linken Ecke am nächsten liegt, plus 1 px nach innen. ΔE im Screenshot gegen die Kante daneben,
  Grenze 12. Die alten Eckpunkte werden zum Vergleich mitgeliefert.
- `geometry-v4.mjs`: G9 misst jetzt auch diese Pixel, für jede App in jeder Breite (vorher nur die Quelle der Palette).
  `measure.mjs`: nur der Kommentar.
- Neu `v4/tools/geometry/g9.test.mjs` mit Gegenprobe. Das Rahmen-Labor läuft mit dem Arbeitsbaum („neu“) und mit
  `window-manager.js` aus `51b796217` („alt“, per Route ersetzt). Apps: Lab-notes (rx 14, durchsichtige Ecken),
  `pal-gradient` (Verlauf, rx 14, im Test als `icon_svg` veröffentlicht, keine neue Fixture-Datei nötig), Bugs &
  Features und IoT (echte `assets/icon.jpg`).
- Nicht geändert, weil Strang FRX die Datei besitzt: `frame.test.mjs:3693` misst weiter den Eckpixel. Mit dem Fix ist der
  Eckpixel Logofarbe (Halter gefüllt), deshalb bleibt die Prüfung grün. Sie sollte trotzdem auf `g9Pixel` aus
  `v4/tools/geometry/g9.mjs` umgestellt werden. Dasselbe gilt für die Zeilen in `maengel.mjs` M17 (M17z misst schon
  deckend).

## 4. Messwerte (1280 × 800, Fenster frei 520 × 360, ΔE CIE76 deckendes Logopixel ↔ Kante, oben / links)

| App | vorher | nachher | alter Eckpixel vorher → nachher | Kante vorher → nachher |
|---|---|---|---|---|
| notes (rx 14, durchsichtig) | **82,0 / 83,7** | **0 / 0** | 1,8 / 0 → 0 / 0 | #141b24 → #c58a2b |
| pal-gradient (Verlauf, rx 14) | **47,3 / 48,6** | **1,3 / 2,7** | 0,9 / 0 → 0,1 / 0 | #141b24 → #196daa / #1a6daa |
| Bugs & Features (JPG) | 1,9 / 0,9 | 1,9 / 0,9 | gleich | #b85929 / #b85928 unverändert |
| IoT (JPG) | 2,7 / 1,9 | 2,7 / 1,9 | gleich | #436150 / #506d50 unverändert |

Pixelgleich belegt: `out/pal/vorher-real-{reports,iot}.png` = `neu-…` (cmp), maengel-Bilder für alle Raster-Icons
und crm/studio byte-gleich vorher/nachher; nur `kante-notes` und `zoom-notes` unterscheiden sich.
Geometrie-Labor nachher: G9-Pixel für 120/120 App×Breite gemessen, Maximum ΔE 6,8 (kundenpipeline 1280).

Gegenprobe (`g9.test.mjs`, 5/5 grün): alt → notes und pal-gradient FAIL (ΔE ≥ 12 oben und links). Der alte Eckpixel
hätte mit „alt“ bestanden (1,8 / 0). Neu → alle 4 Apps PASS. Die Kantenfarben der Raster-Icons sind in beiden Varianten
gleich.

Screenshots (selbst angesehen): `v4/tools/geometry/out/pal/{vorher,neu,alt}-{notes,pal-gradient,real-reports,real-iot}.png`,
`v4/frame/lab/shots/maengel-M-17-zoom-notes.png` (nachher: nahtlos, keine dunklen Ecken),
`maengel-M-17-kante-*.png`.

## 5. Abnahme

| Befehl | vorher | nachher |
|---|---|---|
| `node v4/frame/lab/frame.test.mjs mock` | 218/218 | **227/227** (FRX hat parallel 9 Prüfungen ergänzt), 0 FAIL |
| `CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/frame.test.mjs stub` | 222/222 | **240/240**, 0 FAIL |
| `node v4/tools/geometry/geometry-v4.mjs --widths 1280,900,420` (Pin 68b83a3) | 117/120, Zonen 4/4 | **117/120, Zonen 4/4**, dieselben FAIL: desktop 900 G6, invoices 900 G1+G4, invoices 420 G4 – keine neuen |
| `CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/maengel.mjs M17,M17z` | 2/3 JA (M17z notes NEIN, ΔE 83,7) | **3/3 JA** (M17z notes ΔE 0) |
| `CTOX_APP_API_STUB=…/637caf6 node --test v4/tools/geometry/g9.test.mjs` | – (neu) | 5/5 |
| `node --test v4/tools/geometry/geometry-v4.test.mjs` | 23/23 | 23/23 |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 | 38/38 |
| `node v4/debt/ratchet.mjs` | FEHLER (+4) | **FEHLER (+2), nicht von PAL** |

Ratchet: Die Zunahme kommt aus `v4/frame/lab/maengel.mjs`, Zeilen 100 („Coding Agents“) und 527 („Bugs & Features“),
Regel english-raw-term. Die Datei kam mit Commit `51b796217` (MAENGEL) und wird gerade von einem anderen Strang
bearbeitet. Die PAL-Dateien bringen 0 Treffer. Vorher standen dort zusätzlich 2× vocab:de-synchronisier aus
derselben Datei; die sind inzwischen weg.
