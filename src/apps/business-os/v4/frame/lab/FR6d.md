# FR6d – Startmenü, Desktop und Crew-Leiste wie in der Shell-V2

Stand 16.10.2026, 00:30. Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`, Kopf 002b8ae5c plus
Arbeitsstand anderer Stränge). Kein git außer Lesen.

Owner: „das bleibt natürlich alles erhalten“. Und zum Desktop: „wenn ich auf desktop klicke, komme ich nicht zum desktop,
hier müssten dann ja alle apps minimiert werden.“

## Referenz (welsch, nur angesehen)

- Ansehen per Tunnel `http://127.0.0.1:18765/`: Seite laden, Startmenü öffnen und mit Esc schließen.
- Den Desktop habe ich nur angesehen. Dafür war die Fensterebene per eingefügtem CSS `visibility:hidden` ausgeblendet.
  Es wurde nichts geklickt, gespeichert oder verschoben.
- **Startmenü:** `.shell-start-menu-panel` bei 16/48 mit 360 px Breite. Oben Suche („Suche nach Apps...“), darunter
  Kategorien mit Einträgen (Icon, Name, „+“/„−“). Im Fuß links „Desktop“, rechts „Einstellungen“.
- **Desktop:** Die Desktop-App (`modules/desktop`) mit `.desktop-module > .desktop-surface > .desktop-icons`.
  - App-Symbole 112 × 136 mit 80-px-Bild, Raster 120 × 140 ab 24/24.
  - Bei 420 px greift das CSS-Raster der Desktop-App (88 × 132, 4 Spalten).
  - Eine Widget-Ebene ist leer. Dateien oder Ordner liegen auf welsch nicht auf dem Desktop.
- **Crew-Leiste:** `.ctox-chat-root.is-collapsed` aus `shared/business-chat.js`.
  - Pille „Crew“ mit bis zu 6 Kreaturen (Lumi, Milo, Nori, Pico).
  - Daneben der Kalender-Navigator ‹ 📅 ›.
  - Sitzt unten links über allem.

## Umgesetzt

| Punkt | Umsetzung | Datei |
|---|---|---|
| **Startmenü** | `launcher.js` ist jetzt das Popup aus dem Raster-Knopf. Markup und Klassen sind die von app.js (`createStartMenuElement`, `buildStartMenuItem`). Suche fokussiert beim Öffnen. Gruppen nach `layout.category`, Einträge mit „+“/„−“ (Pin). Fuß links „Desktop“, rechts „Einstellungen“. Esc, Klick daneben, zweiter Klick auf den Raster-Knopf, ein Start, „Desktop“ oder „Einstellungen“ schließen das Menü. Beim Schließen wird die Suche geleert. Ziehen zum Umordnen (FR2-6) und Alt+Pfeil gehen weiter, ebenso Ziehen auf die Reiter (DRAG_APP). | `launcher.js`, `index.js` |
| **„Neue App“** | Die Shell-V2 hat dafür keine Stelle. Der Knopf sitzt deshalb im Kopf des Startmenüs neben der Suche, nur mit `apps.source.edit`. Er nutzt die Fußknopf-Klasse `start-menu-footer-btn` und schließt das Menü. | `launcher.js`, `frame.css` |
| **Desktop** | Neue Datei `desktop.js`: eigene Fläche hinter den Fenstern mit der DOM-Struktur der Desktop-App. Positionen wie `modules/desktop/index.js` `gridPosition`, ein Klick startet die App. Ein Reiter-Pin, der auf den Desktop gezogen wird, wird gelöst. Das Symbol einer laufenden App trägt `is-app-open`. | `desktop.js` |
| **„Desktop“ im Menü** | Schließt das Menü und ruft die FR6b-Funktion `showDesktop`: alle Fenster minimieren. Ein zweites „Desktop“ bzw. Strg+Alt+D stellt genau diese Fenster wieder her. Ist nichts zu minimieren, bekommt die Desktop-Fläche den Fokus statt wie bisher die Suche. | `index.js` |
| **Crew-Leiste** | Neue Datei `crew-bar.js` mit dem eingeklappten Dock aus business-chat.js: Pille „Crew“ mit Avataren (höchstens 6) und Kalender-Navigator. Ein Klick auf ein Mitglied holt dessen Chat nach vorn oder öffnet ihn; er läuft unter dem Reiter „Crew“. Ein Klick auf die Pille daneben holt das oberste Crew-Fenster nach vorn. ‹/› wechseln den Tag, nie über heute hinaus. 📅 öffnet die native Datumswahl. | `crew-bar.js`, `index.js` |
| **CSS wörtlich** | Neuer Extraktor `lab/extract-desktop-css.mjs` erzeugt `v4/frame/shell-v2-desktop.css` mit 159 Regeln und 71 Tokens. Quellen in Ladereihenfolge der Shell-V2: ui-contract, app.css, mobile-host.css, `modules/desktop/index.css`, business-chat.js `installChatStyles` (mit `CREW_CREATURE_BASE_CSS` eingesetzt). Der Extraktor nutzt den Parser und die Umschreibung von `extract-topbar-css.mjs`, mit der Hülle `[data-v4-v2]`. `[data-app="desktop"]` wird zu `[data-v4-desktop-v2]`. Die Datei wird als sechste Rahmen-CSS geladen. | `lab/extract-desktop-css.mjs`, `shell-v2-desktop.css`, `index.js` |
| **Extraktor TOPBAR** | `extract-topbar-css.mjs` ist jetzt importierbar: Lauf nur als Hauptmodul, `scopeSelector(s, hull)` nimmt die Hülle als Parameter. `:root:not([…])` bleibt an der Wurzel, statt an die Hülle zu wandern. Das war der Fehler, der die Crew-Leiste im hellen Schema dunkel färbte. Die erzeugte `shared/shell-v2-topbar.css` ist danach **byte-gleich** (per `cmp` geprüft). | `lab/extract-topbar-css.mjs` |
| **Einbettung** | `frame.css` regelt nur noch die Einbettung. Desktop absolut im Desk. Die Hüllen setzen `line-height: normal` (`.v4f` vererbt sonst 1.35, wie bei `.v4f-top`). Geschlossenes Menü ist `visibility:hidden` und damit nicht per Tab erreichbar. Dazu die Einfügemarke beim Ziehen. Die alten `.v4f-launcher`/`.v4f-tile`/`.v4f-grid`-Regeln sind weg. | `frame.css` |

## Nachträge aus dem Strang

- **GATE (Ratchet):** In `topbar.js` stehen jetzt `// v4-allow: en` am Produktbegriff „Workspace“ (welsch zeigt
  wörtlich „Lokaler Workspace“) und am englischen Wörterbuch.
  - Rollennamen wie `shared/roles.js` `roleDisplayName`: chef/owner = „Owner“ (bleibt, mit Marker), admin = „Admin“,
    founder = „App-Verantwortliche:r“, user/member = „Teammitglied“.
  - `node v4/debt/ratchet.mjs`: Die Zeilen `guard.app.v4.*` steigen nicht mehr.
  - Rot sind zuletzt nur noch `guard.app.modules/esign|explorer` und `guard.rule.*` aus dem App-Bereich. Das ist nicht
    mein Schreibbereich.
- **Rundgang:** `v4/tools/rundgang/rundgang2.mjs` klickte die alte Launcher-Kachel `[data-v4-app] [data-v4-open]` an.
  - Jetzt klickt er mit echter Maus das Desktop-Symbol `[data-v4-desk-app]` an. Das ist der Weg auf der Fläche; das
    Startmenü ist ein Popup.
  - `node v4/tools/rundgang/rundgang2.mjs --app notes --widths 1280 --max-steps 3` → `OK notes 1280: 6 Schritte, 0 Fehler`.
- **MAN3 (srcset):**
  - `appView()` bildet `iconSrcSet` aus `layout.icon_asset_srcset`, jeder Pfad über `artifactUrl`, Deskriptoren bleiben.
  - `iconFor()` setzt `srcset` vor `src`. Das gilt für Startmenü, Desktop, Reiter, Fensterliste und Crew-Leiste, aber
    nur, wenn das Bild das Manifest-Icon selbst ist (nicht repariert, nicht generiert).
  - Das Fenster-Icon übergibt `iconSrcSet` als Funktion der aufgelösten URL.
  - `shared/window-manager.js` setzt srcset jetzt auch im Promise-Zweig (3 Zeilen).
  - **Nicht im Labor belegt:** Die Labor-Manifeste tragen kein `icon_asset_srcset`, und ein Versuch über die Attrappe
    kam nicht bis ins `appView` durch.
  - G0 und TOP sind unberührt, die Werte stehen unten.

## Abweichungen und Lücken

- **Kategorieüberschrift ohne Versalien:** app.css setzt `.start-menu-category-title` in Großbuchstaben. Der bestehende
  Fall FR5-3 verlangt „Überschrift ohne Versalien-Umformung“ (Glossar).
  - Ich habe FR5-3 grün gehalten: `text-transform: none`, die einzige bewusste Abweichung von app.css.
  - Ein Owner-Entscheid ist nötig: pixelgleich (Versalien) oder FR5-3. Dafür ist nur die eine Zeile am Ende von
    `frame.css` zu löschen und FR5-3 anzupassen.
- **Kategorien:** Die Shell-V2 hat feste Kategorien mit Emoji (🗂️ Arbeitsbereich, ⚡ Produktivität …). v4 gruppiert nach
  `layout.category` (FR3-2) ohne Emoji. Bei aktiver Suche bleibt die Gruppierung erhalten; die Shell-V2 zeigt dort eine
  flache Liste „Suchergebnisse“ (FR3-2 verlangt die Gruppen). Leere Treffer zeigen keinen Text; die Shell-V2 zeigt
  „Keine passenden Apps gefunden.“, die Zustandstext-Regel geht vor.
- **Crew-Daten (Lücke):** Die Shell-V2 liest die Crew aus ihren Chat-Collections (`crewMembers` mit Gestalt, Farbe,
  Stimmung, Last) und zeichnet sie mit `crew-renderer.js`. Der v4-Kernel liefert dem Rahmen nichts davon.
  - Die Leiste zeigt deshalb, was da ist: die Apps mit `layout.presentation.taskbar_group = "crew"`, mit ihrem App-Bild
    als Avatar.
  - Ohne Crew-Apps ist die Leiste ausgeblendet; die Shell-V2 zeigt dann eine Platzhalter-Kreatur.
  - Der Kalender-Navigator hält den Tag (`data-v4-crew-day`), aber im v4-Kernel gibt es noch keinen Verbraucher
    (Chats eines Tages).
  - Das ausgeklappte Dock (Chat-Streifen, ‹ ›, „+“) fehlt.
- **Desktop-Daten (Lücke):** Gespeicherte Symbolpositionen, Ordner, Dateien und Widgets der Desktop-App (ihre
  Collections) erreichen den Rahmen nicht. Der Desktop zeigt die Apps aus `kernel.apps` in `launcher_order` und
  alphabetisch. Umordnen geht im Startmenü (FR2-6), nicht durch Ziehen auf dem Desktop.
- **Glas-Panel:** Das Startmenü ist wie in der Shell-V2 halbtransparent mit Unschärfe (`--glass-bg`, `--glass-blur`).
  Auf leerem Desktop scheinen die Symbole farbig durch, auf welsch liegt meist ein Fenster dahinter.
- **Extraktor TOPBAR, Befund für dessen Besitzer:** `declsOf` nimmt einen Kommentar vor einer Deklaration in den
  Namen auf.
  - Dadurch fehlten z. B. `--elev-float` und `--elev-float-2` aus app.css.
  - Im neuen Extraktor ist das behoben (Kommentare werden vor dem Zerlegen entfernt).
  - In `extract-topbar-css.mjs` habe ich es **nicht** geändert, weil `shared/shell-v2-topbar.css` sonst nicht mehr
    byte-gleich wäre. Bitte im Strang TOPBAR nachziehen und mit TOP-1 prüfen.
- **Manifest:** Für `launcher.js`, `index.js`, `frame.css` und `topbar.js` sowie die neuen Dateien `desktop.js`,
  `crew-bar.js` und `shell-v2-desktop.css` fehlen bzw. veralten die Einträge in `v4/manifests/shell.manifest.json`.
  Neu erzeugen mit `node v4/tools/manifest/gen-manifest.mjs --shell-libs`; das ist nicht mein Schreibbereich.

## Labor

Die FR6c-Seite (`FR6C_ONLY=1`) ist um FR6d erweitert. Für jeden Punkt läuft die Messung zusätzlich mit einer per
`page.route` mutierten Datei, und der Punkt muss dort rot werden:

| Punkt | Messung | Mutation → rot |
|---|---|---|
| menu | Zu beim Laden. Raster-Knopf → offen bei 16/48, 360 px, Suche fokussiert. Esc → zu. Wieder offen, Klick daneben → zu | Klick-daneben-Listener entfernt |
| menuEsc | Esc schließt | Esc-Zweig leer |
| desktop | Fuß = [Desktop, Einstellungen]. Klick schließt das Menü, 9 → 0 sichtbare Fenster, zweiter Klick → 9 | Klick-Handler ohne `onDesktop` |
| surface | 6 Symbole, das erste bei 24/72 mit 112 × 136, frei klickbar hinter minimierten Fenstern | Desktop nicht eingehängt |
| crew | Mitglieder Anna, Ben, Cleo, Pille „Crew“, Navigator mit 3 Teilen, unten links. Klick auf Ben → Chat Ben vorn, Reiter „Crew“ aktiv. Tag zurück, dann zweimal vor → wieder heute | Mitglieder-Klick leer |
| G0 | Startmenü, Desktop und Crew-Leiste im v4-Rahmen gegen den app.js-Pfad (ui-contract, app.css, base.css, mobile-host.css, Desktop-CSS, Chat-Stil) mit demselben DOM | Rumpf des Startmenüs um 40 px eingerückt |

Bisherige Fälle mit angepasstem Weg (öffnen das Menü vor Suche, Pins und „Neue App“):

- Suche, Pin und Öffnen im Grundlauf
- FR2-5/-6 (Ziehen untereinander; Lösen per Ziehen auf den **Desktop**)
- FR3-1/-2, FR5-3, FR6b-D1, TOP-0 (Raster-Knopf öffnet und schließt das Menü, Fenster unberührt)
- „Schmal: Desktop“ (Menü → Desktop)
- Theme-Schnittstelle und Shell-Tausch zählen 6 Rahmen-CSS

Fremdfälle sind nur im Selektor angepasst: G0 Shell-Treue blendet `[data-v4-desktop]` und die Crew-Leiste aus statt der
alten Launcher-Fläche.

### Endlauf

Selbst ausgeführt, 16.10. 00:15–00:45:

| Befehl | Ergebnis |
|---|---|
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | **167/167** |
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9 … frame.test.mjs stub` | **197/197** |
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 … frame.test.mjs stub` | **185/185** (`results-stub.json` = dieser Lauf) |
| FR6d-G0 (alle drei Läufe) | desktop = **1**, crew = **1**, start = **0,9966**. Der Rest ist die Kategorieüberschrift ohne Versalien (FR5-3, s. o.); „Neue App“ ist im Vergleich ausgeblendet, weil sie in der Shell-V2 keine Entsprechung hat. Mutation 0,9429 → rot |
| G0 Fensterrahmen, TOP-1 Kopfleiste (alle drei Läufe) | jeweils **1** (100 %) in allen Bereichen |
| `node v4/frame/lab/extract-topbar-css.mjs` + `cmp` | `shared/shell-v2-topbar.css` byte-gleich |
| `node v4/frame/lab/extract-desktop-css.mjs` | 159 Regeln, 71 Tokens, 2 Keyframes |
| `node v4/debt/ratchet.mjs` | `guard.app.v4.*` nicht gestiegen; rot nur `guard.app.modules/esign`, `…/explorer` und `guard.rule.*` (App-Bereich, nicht meiner) |
| `node v4/tools/rundgang/rundgang2.mjs --app notes --widths 1280 --max-steps 3` | OK, 0 Fehler |
| `node --check` launcher.js, desktop.js, crew-bar.js, index.js, topbar.js, shared/window-manager.js, beide Extraktoren, rundgang2.mjs, frame.test.mjs | ok |

### Bilder (angesehen)

- Neben welsch, links welsch (dunkel), rechts v4 (mock, dunkel, 4 Crew-Apps):
  - `shots/fr6d-start-vs-welsch-1280.png`: gleiches Panel 16/48 × 360, Suche, Kategorien, Fuß; Crew-Leiste unten links.
  - `shots/fr6d-desktop-vs-welsch-1280.png`: gleiches Raster (24/24, 120 × 140, 80-px-Bilder, Beschriftung).
  - `shots/fr6d-start-vs-welsch-420.png`, `shots/fr6d-desktop-vs-welsch-420.png`: 4-Spalten-Raster wie welsch,
    Crew-Leiste und Navigator gleich groß.
- G0-Paare: `shots/integrated[-stub]-fr6d-g0-{start,crew,desktop}-{v4,ref}.png`.
- Labor-Bilder:
  - `integrated[-stub]-fr6d-start-1280.png`
  - `integrated[-stub]-fr6d-crew-1280.png`
  - `integrated[-stub]-fr6c-launcher-desktop-1280.png` (Desktop nach „Desktop“)
  - `integrated[-stub]-fr6c-launcher-420.png`
