# FRX – Rahmen-Befunde der Mängel-Abnahme (16.10.)

Strang FRX. Drei Rest-Befunde aus `MAENGEL.md`: M-02 (Esc im Editor), M-11 (Vollbild im Layout-Menü), Nebenbefund 420 px
(Kopfleiste). Geändert: `v4/frame/index.js`, `v4/frame/topbar.js`, `v4/frame/frame.css` (neuer Abschnitt am Ende),
`v4/frame/lab/frame.test.mjs` (Block `frx()`, `FRX_ONLY=1`; Raster-Prüfung FR6a um „Vollbild“ erweitert).
`shared/window-manager.js` und alles unter `shared/**`, `modules/**`, `v4/shell/**`, `v4/source/**` sind unverändert.

## M-02 Esc in der Code-Ansicht

**Ursache.** Monaco verbraucht Esc immer, auch ohne offene Überlagerung: Das Ereignis kommt am Fenster mit
`defaultPrevented = true` an (Messung mit Lauschern in Capture und Bubble: `win-cap dp=false` →
`shellwin-bub dp=true`). Bei offenen Vorschlägen und offener Suche ruft Monaco zusätzlich `stopPropagation` auf; das
Esc erreicht das Fenster dann gar nicht. Der Rahmen-Handler lief in der Bubble-Phase und verlangte
`!ev.defaultPrevented` (vorher `v4/frame/index.js:720-726`). Mit Fokus in Monaco (`TEXTAREA.inputarea`) konnte er also
nie greifen. Der textarea-Rückfall verhindert Esc nicht. Dort ging der Rückweg schon vorher (Gegenprobe: grün auch ohne
Fix).

**Fix** (`v4/frame/index.js`):
- `EDITOR_OVERLAY_SEL` und `editorOverlayOpen()` stehen auf Modulebene. Als offene Überlagerung zählen
  `.suggest-widget.visible`, `.find-widget.visible`, `.parameter-hints-widget.visible`, `.monaco-hover:not(.hidden)`,
  Umbenennen, Peek und das Kontextmenü. Das Kontextmenü rendert Monaco in einen Shadow-Root (`.shadow-root-host`,
  Höhe 0) im Editor; gezählt wird nur ein sichtbares `.monaco-menu`.
- In `attachChrome` hängt ein **Capture-Lauscher** am Fensterelement. Er entscheidet, bevor Monaco das Ereignis
  sieht. Er greift nur, wenn alles davon gilt:
  - Ziel ist der Eingang des Monaco-Editors der Code-Ansicht dieses Fensters (`textarea.inputarea` bzw.
    `.native-edit-context` in `.v4-src-ed--monaco`, kein Shadow-Host).
  - Das Layout-Menü ist zu.
  - Die Statuszone bzw. das App-Menü ist zu.
  - Es ist keine Überlagerung offen.

  Dann ruft er `preventDefault` und `stopPropagation` auf und schaltet mit `showApp(rec)` zurück. Monaco bekommt dieses
  Esc nicht, Auswahl und Cursor bleiben. Ist eine Überlagerung offen, tut der Lauscher nichts: Monaco schließt sie, und
  erst das nächste Esc führt zurück.
- Der bisherige Bubble-Lauscher bleibt für alle anderen Ziele (textarea-Rückfall, Reiter, Dateibaum). Die Bedingung
  teilen sich beide Lauscher (`escBackAllowed`). Wer Esc selbst braucht, verhindert weiter den Standard:
  - Agent-Eingabe mit Text: `agent-panel.js:304`
  - Umbenennen: `source-mode.js:689`
  - „…“-Menü: `source-mode.js:825`
  - Blätter

**Prüfung** (`frx()`, stub, echte Maus und Tastatur, Monaco echt über `?shell=bound`):
- Klick in den Editor, tippen, Esc → App. Strg+Umschalt+E → der Entwurf (`/* frx-entwurf */`) steht noch im Editor.
- Vorschläge (Strg+Leertaste), Suche (Cmd+F, Fokus im Suchfeld `TEXTAREA.input`), Hover (Cmd+K Cmd+I) und
  Kontextmenü (Rechtsklick): Das erste Esc schließt die Überlagerung, die Ansicht bleibt `code`. Das zweite Esc → `app`.
- Nicht kaputt:
  - Agent-Eingabe: Esc leert nur den Text.
  - Layout-Menü per Hover offen, Fokus im Editor: Esc schließt nur das Menü.
  - App-Menü am Icon offen: Esc schließt nur die Liste.

  Die Ansicht bleibt dabei jeweils `code`.
- textarea-Rückfall (Rohpfade, kein Monaco): Klick, tippen, Esc → App, Entwurf erhalten.
- Mock: Die Attrappe hat kein `kernel.source`, es gibt also keine Code-Ansicht. M-02 läuft dort nicht, der
  textarea-Fall läuft im Stub ohne Bindung.

**Gegenprobe** (`FRX_GEGENPROBE=1`: Die Prüfung liefert `index.js` mit `return;` statt der Überlagerungsprüfung aus,
d. h. ohne Rückweg aus Monaco; die Dateien bleiben unberührt): Alle fünf Monaco-Prüfungen fallen. Beispiel:
`{"focusA":"TEXTAREA.inputarea","a":{"mode":"code"}}`, zweites Esc bei Vorschlägen `b2:"code"`. Grün bleiben
„nicht kaputt“ und der textarea-Fall. Das ist erwartet, denn dort war der Rückweg nie defekt.

Screenshots: `shots/integrated-stub-frx-m02-monaco-code-1280.png`, `…-monaco-esc-app-1280.png`,
`…-monaco-suggest-1280.png`, `…-monaco-find-1280.png`, `…-monaco-kontextmenue-1280.png`,
`…-textarea-esc-app-1280.png`.

## M-11 „Vollbild“ im Layout-Menü

**Ursache.** Das Layout-Menü baut allein `shared/window-manager.js` (`V2_LAYOUT_OPTIONS`, 3×3 + „Freie Größe“ +
„Minimieren“). Die Vollbild-Funktion des Rahmens (`enterFullscreen`/`exitFullscreen` in `v4/frame/index.js`, F11,
Strg+Cmd+F) hatte dort keinen Eintrag.

**Fix** (`v4/frame/index.js` `attachChrome`, ohne Änderung am Fenstermanager): Der Rahmen hängt nach dem Erzeugen des
Fensters einen Knopf ans vorhandene `[data-window-layout-menu]`. Er trägt dieselben Klassen und Attribute wie „Freie
Größe“ und „Minimieren“: `is-wide`, `role="menuitem"`, `tabIndex -1`, `data-window-layout-control="fullscreen"`,
`data-row` = letzte Zeile + 1, `data-col="1"`, `grid-column 1 / -1`, Glyph `.shell-window-layout-glyph` und
`.shell-window-layout-label`. Damit übernimmt die Menüsteuerung des Fenstermanagers Pfeiltasten (ab von „Minimieren“,
auf zurück), Hover und Schließen nach der Wahl. `applyLayoutControl('fullscreen')` kennt die Aktion nicht und schließt
nur das Menü. Die Wirkung hängt am Knopf:
- Klick: Toggle.
- Enter/Leertaste: Toggle direkt, weil die Menüsteuerung den Standard verhindert.

Die Beschriftung kommt aus dem Wörterbuch (`windowFullscreen` „Vollbild“, `windowFullscreenExit` „Vollbild beenden“,
en: „Full screen“/„Exit full screen“) über `t()`. Ein einziger `fullscreenchange`-Lauscher des Rahmens (abgebaut über
`state.unsubs`) setzt die Beschriftung aller Fenster nach dem Zustand. Dazu kommt ein Glyph
`.shell-window-layout-glyph--fullscreen` (vier Ecken im Strich der Raster-Symbole) in `frame.css`.

**Prüfung** (mock und stub):
- Hover öffnet das Menü. Letzter Eintrag ist „Vollbild“ unter „Minimieren“, mit gleicher Höhe, Breite und Schrift wie
  „Minimieren“, `role=menuitem`.
- Tastatur: Pfeil ab erreicht „Vollbild“ nach „Minimieren“ und bleibt dort, Pfeil auf führt zurück, Esc schließt.
- Echter Klick: `document.fullscreenElement` ist der Fensterinhalt (`shell-window-content v4f-content`) dieses
  Fensters, das Menü ist zu, der Eintrag heißt „Vollbild beenden“. Spy: genau ein `requestFullscreen` am Inhalt.
- Esc: Headless Chromium gibt die Taste nicht an die Browser-UI weiter, das Element-Vollbild bleibt. Die Prüfung beendet
  es dann wie der Browser bei Esc mit `document.exitFullscreen()` und vermerkt das im Prüfnamen. Danach heißt der
  Eintrag wieder „Vollbild“ (`fullscreenchange`).
- Tastatur: Enter öffnet das Menü, sechsmal Pfeil ab, Enter → Vollbild im Fenster. Spy gesamt:
  `request, exit, request`.

**Gegenprobe** (Eintrag ausgeliefert ohne `if`-Zweig): Alle fünf M-11-Prüfungen fallen. Die Reihenfolge endet bei
`minimize`, Pfeil ab bleibt auf `minimize`, der Klick findet den Eintrag nicht.

Screenshots: `shots/integrated-frx-m11-layout-menue-1280.png`, `…-m11-vollbild-1280.png` (mock und `-stub-`).

## Nebenbefund 420 px Kopfleiste

**Ursache.** Shell-V2 (`shared/shell-topbar.css` über `shared/shell-v2-topbar.css:1297`) hält Reiter auf
`min-width: max-content` mit `overflow: visible`. Was nicht passt, schiebt `chooseVisibleTopbarApps` in „+N“. Der
v4-Rahmen erzwingt zusätzlich den Reiter des vorderen Fensters (FR6c, vorher `v4/frame/topbar.js:392`
`if (priority >= 0) visible.add(priority)`), auch wenn er nicht passt. Der Reiter ragte dann ungekürzt über Punkt und
Avatar. Ein langer Workspace-Name (`geo-credentials-…`) nimmt der Spalte der Reiter zusätzlich Platz, weil die linke
Gruppe eine `auto`-Spalte ist.

**Fix:**
- `topbar.js` `layout()`:
  - Vor dem Messen wird `data-v4-tab-shrink` entfernt, gemessen werden also die natürlichen Breiten.
  - Liegt der vordere Reiter nicht in der V2-Auswahl, bleibt nur er sichtbar (übrige in „+N“) und bekommt
    `data-v4-tab-shrink`.
- `frame.css`, neuer Abschnitt „Kopfleiste: Überlauf bei schmaler Breite“:
  - `[data-v4-tab-shrink]` lässt den Reiter schrumpfen (`flex: 0 1 auto; min-width: 0; overflow: hidden`). Die
    Beschriftung kürzt mit `text-overflow: ellipsis`, die Zahl schrumpft nicht.
  - Der Workspace-Name kürzt mit „…“ (`min-width: 0` in der Kette). Er hat höchstens `max(120px, 100vw − 300px)`:
    „Lokaler Workspace“ (113 px) bleibt bei 420 px ungekürzt, dem vorderen Reiter bleibt Platz.
  - Keine `@media`-Regel.
  - Ohne Engpass greift keine der Regeln sichtbar, G0 bleibt grün.

**Prüfung** (erweitert TOP-5, mock und stub, 420 × 800): Lab-App mit dem Titel „Zugangsdaten“ und langer
Workspace-Name `geo-credentials-1792128575178-ein-sehr-langer-workspace-name`. Gemessen werden die Rects der sichtbaren
Reiter, des Punkts, des Avatars und der linken Gruppe, außerdem `scrollWidth` von Kopfleiste und Dokument.

| Fall | Reiter (l–r) | Punkt l | Avatar l | Name | Reiter gekürzt |
|---|---|---|---|---|---|
| langer Reiter allein (mock) | 189–329 | 359 | 382 | 113 px, ungekürzt | nein |
| + langer Workspace-Name | 196–335 | 359 | 382 | 120 px, „…“ | nein |
| + zweites Fenster (Zugangsdaten vorn) | 196–313, „+1“ 319–355 | 359 | 382 | 120 px, „…“ | ja, „…“ |
| ohne Fenster, langer Name | – | 359 | – | rechts 187 | – |

**Gegenprobe:**
- Ohne Fix, `topbar.js` und der `frame.css`-Abschnitt ausgeliefert ohne Änderung (`FRX_GEGENPROBE=1`): Mit langem
  Namen ragt der Reiter bis x = 456, mit zwei Fenstern bis 455,7. Er liegt über Punkt (359) und Avatar (382), die
  Prüfung fällt.
- Mutationsprobe im normalen Lauf (Stil neutralisiert): Die Prüfung findet die Überlappung (`tabsRight` 455,7).

Screenshots: `shots/integrated-frx-420-langer-reiter.png`, `…-420-langer-reiter-langer-workspace.png`,
`…-420-zwei-fenster-langer-workspace.png`, `…-420-mutation-ohne-fix.png` (mock und `-stub-`).

## Fundstellen nach dem Fix

- `v4/frame/index.js`:
  - `:122` `EDITOR_OVERLAY_SEL` und `:124` `editorOverlayOpen`
  - `:734` `escBackAllowed`, Capture-Lauscher bis `:749`, danach der Bubble-Lauscher
  - `:762` Vollbild-Eintrag im Layout-Menü
  - `:856` `fullscreenchange`
- `v4/frame/topbar.js:382` und `:398` (`data-v4-tab-shrink`)
- `v4/frame/frame.css:464-499` (Kopfleiste, Glyph)

## Abnahme (16.10., eigene Läufe)

```
FRX_ONLY=1 node v4/frame/lab/frame.test.mjs mock                         → 9/9 bestanden
FRX_ONLY=1 CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/frame.test.mjs stub → 18/18 bestanden
node v4/frame/lab/frame.test.mjs mock                                    → 227/227 bestanden (Basis 218 + 9 FRX), exit 0
CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/frame.test.mjs stub         → 240/240 bestanden (Basis 222 + 18 FRX), exit 0
node scripts/assert-shell-v2-contract.mjs                                → Shell-V2-Vertrag (statischer Teil) OK: 38/38 Apps
node v4/debt/ratchet.mjs                                                 → ratchet: guard 1838 · scan 3 · OK
CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/maengel.mjs M02            → 13/14 JA; M-02 alle JA, darunter
    „Esc (nach Klick in den Editor, Monaco) führt zurück zur App“ {"mode3":"app","mode3b_zweitesEsc":"app",
    "focusEd":"TEXTAREA.inputarea …|inEditor=true"}. Das eine NEIN ist M-06 (Fensterbild, nicht FRX).
CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/maengel.mjs M11            → 9/9 JA; „Vollbild im Layout-Menü“ JA
MG_APPS=credentials …/maengel.mjs M13 (enthält NEBEN)                    → 5/5 JA; NEBEN credentials 420
    {"tabsRight":335,"syncLeft":359,"avatarLeft":382} (vorher tabsRight 411)
FRX_GEGENPROBE=1 FRX_ONLY=1 … stub / mock                                → 7/18 bzw. 3/9 bestanden (erwartet rot)
```

G0 bleibt grün in mock und stub: G0 Shell-Treue 1/1/1/1/1/1, TOP-1 G0 Kopfleiste hell/dunkel 1280 und 420 je Drittel
1, FR6d-G0 desktop/crew/start 1. Die Raster-Prüfung FR6a erwartet jetzt die Reihenfolge `…,free,minimize,fullscreen`.

Hinweis zum Ratchet: Ein Lauf, während die vollen Labor-Suiten liefen, meldete kurz
`guard.app.v4.english-raw-term 0 -> 2`. Ohne parallelen Lauf ist das Ergebnis OK (1838). Keine Fundstelle stammt aus
FRX: Die englischen Wörterbuchwerte stehen im `en`-Block von `MESSAGES`, wie die bestehenden.

## Offen

- Esc beendet das Vollbild im echten Browser über die Browser-UI. Headless ist das nicht prüfbar; belegt sind der
  Aufruf (Spy) und der Zustand nach `exitFullscreen()`. Gegenprobe im echten Browser bei T4.
- Im Vollbild liegt der Fensterinhalt im Element-Vollbild, der Knopfcluster liegt außerhalb. „Vollbild beenden“ ist
  deshalb nur der korrekte Zustand des Eintrags. Beendet wird das Vollbild im Vollbild über Esc bzw. F11, wie bisher.
