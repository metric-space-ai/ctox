# FR6c – EINE Leiste: Reiter oben (Shell-V2) mit der FR6b-Gruppierung, Desktop im Launcher

Stand 15.10.2026, 23:30. Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`), kein git außer Lesen.
Owner: „das bleibt natürlich alles erhalten“. M-07 (Leiste wächst zu schnell auf volle Breite) und M-18 (Desktop).

## Umgesetzt

| Punkt | Umsetzung | Datei |
|---|---|---|
| Nur noch EINE Leiste | Die untere Taskleiste ist entfernt (`createTaskbar` gelöscht, `--v4f-bar-h: 0px`, alle `.v4f-bar`/`.v4f-task`-Regeln aus `frame.css` raus). Die Rasterzeile bleibt mit 0 px stehen, weil `shared/shell-v2-topbar.css` (TOPBAR, generiert) sie mit `var(--v4f-bar-h)` anlegt. | `index.js`, `frame.css` |
| Reiter mit FR6b-Logik | `topbar.js` rendert die Reiter aus `buildEntries` (ein Reiter je App bzw. Gruppe, `taskbar_group` → „Crew“, Pins zuerst und mit ihren Fenstern verschmolzen). Ab zwei Fenstern steht die Zahl am Reiter (`.module-tab-count`, Regel wörtlich aus `shared/shell-topbar.css`). Ein Klick wechselt reihum (`nextInGroup`), bei einem Fenster fokussiert bzw. minimiert er, bei einem Pin ohne Fenster öffnet er die App. Ein Rechtsklick öffnet die Fensterliste direkt unter dem Reiter (Pfeile, Esc, Klick daneben). Pins lassen sich auf die Reiter ziehen, dort umordnen und auf die Fläche ziehen (DRAG_APP/DRAG_PIN, vorher Taskleiste). | `topbar.js` |
| Überlauf | „+N“ wie Shell-V2 (`chooseVisibleTopbarApps`), aber über die gruppierten Einträge (Crew zählt als einer). Das aktive Fenster hat Vorrang vor dem Tastaturfokus und wird immer sichtbar gesetzt. Wechselt der aktive Reiter, wird neu verteilt. Damit wächst die Leiste nie über ihre Breite. | `topbar.js` |
| Shell-Ansichten | Auch die Ansicht „Neue App“ bekommt einen Reiter (ohne App-Eintrag). Ohne Taskleiste ist der Reiter der einzige Weg zu einem minimierten Fenster. | `topbar.js` |
| `taskbar.js` | Ist jetzt eine reine Logik-Datei: `taskGroupOf`, `buildEntries`, `groupWindows`, `nextInGroup`, ohne DOM. Nicht gelöscht, weil `v4/manifests/shell.manifest.json` die Datei führt und das Manifest nicht in meinem Schreibbereich liegt. | `taskbar.js` |
| Desktop (M-18) | Der Launcher hat jetzt einen Fuß wie das Startmenü auf welsch (`.start-menu-footer`, gemessen per DOM): links „Desktop“, rechts „Einstellungen“, gleiche Symbole, 12/16 px Innenabstand, 34 px Knopfhöhe, 12 px/650. „Desktop“ ruft die bestehende FR6b-Funktion `showDesktop`: Der erste Klick minimiert alle Fenster, der zweite (bzw. Strg+Alt+D) stellt genau diese wieder her. „Einstellungen“ öffnet das Einstellungsblatt der Kopfleiste. | `launcher.js`, `index.js` |
| Zahnrad < 560 px | Per `@media (max-width: 559.98px)` ausgeblendet. Die Einstellungen bleiben über den Avatar erreichbar: Im Konto-Blatt ist „Einstellungen“ der oberste Eintrag. Bei 420 px ist der aktive Reiter („Crew 6“) sichtbar. | `topbar.js` |

## Abweichungen und Offenes

- **„schließt den Launcher“:** Im v4-Rahmen IST der Launcher der Desktop (die Kachelfläche hinter den Fenstern, kein aufklappendes Startmenü wie in Shell-V2). „Desktop“ minimiert also alle Fenster und legt damit genau diese Fläche frei. Ein Menü, das zu schließen wäre, gibt es nicht. Der Startknopf oben links bleibt wie bisher die Desktop-Umschaltung (TOP-0). Bei 420 px decken Vollbildfenster den Launcher-Fuß zu, dort führt der Startknopf zum Launcher und zum Fuß (Bild `fr6c-launcher-420`).
- **welsch hat heute ein Zahnrad in der Kopfleiste, auch bei 420 px** (Vergleichsbild `fr6c-vs-welsch-420.png`). v4 blendet es unter 560 px wie beauftragt aus.
- **G0 Kopfleiste bei 420 px:** Die Referenz (app.js-Pfad) bekommt jetzt das *sichtbare* DOM. Ist das Zahnrad in v4 ausgeblendet, fehlt es auch im Referenz-DOM. Sonst verglich G0 ein absichtlich verborgenes Element (rechtes Drittel 96,6 %). Die Leisten-CSS ist unverändert; das Ergebnis ist wieder 100 %.
- **Manifest:** Die Hashes in `v4/manifests/shell.manifest.json` für `taskbar.js`, `topbar.js`, `launcher.js`, `index.js` und `frame.css` sind veraltet. Neu erzeugen mit `node v4/tools/manifest/gen-manifest.mjs --shell-libs`. Das ist nicht mein Schreibbereich und betrifft zugleich die Edits von AGENT in `index.js`.
- `INTEGRATION.md` (Abschnitt FR6b) beschreibt noch die untere Taskleiste. Die Datei gehört nicht zu meinem Schreibbereich.
- Pins ohne Fenster zeigen wie in Shell-V2 Icon **und** Titel (`data-pinned`). Die FR6b-Taskleiste zeigte dort nur das Icon.

## Labor

Neuer Block `fr6c()` (`FR6C_ONLY=1`): 3 Apps + 6 Crew-Chats, angeheftet `tasks` (mit Fenster) und `crew-anna`. Für jeden
Punkt läuft dieselbe Messung zusätzlich mit einer per `page.route` mutierten Rahmen-Datei; der Punkt muss dort scheitern:

| Punkt | Messung (unverändert grün) | Mutation → rot |
|---|---|---|
| gruppiert | 4 Reiter `tasks, crew, notes, ledger`, Crew = „Crew“, keine Taskleiste unten | `taskGroupOf` liefert immer null |
| Zahl | Crew zeigt 6, Einzelfenster ohne Zahl | `count.hidden = true` |
| Pin verschmolzen | `tasks` mit Pin läuft im selben Reiter, `crew-anna` hängt am Crew-Reiter | Pins als eigene Einträge |
| Überlauf/aktiv | Auf 200 px „+3“; Crew (letzter, aktiv) bleibt sichtbar, alles passt; breit kein „+N“ | Vorrang des aktiven Reiters entfernt |
| Desktop | Fuß = [Desktop, Einstellungen]; 9 → 0 → 9 sichtbare Fenster | Klick-Handler leer |
| Zahnrad | Bei 420: Zahnrad `display:none`, aktiver Reiter sichtbar, Konto-Blatt oben „Einstellungen“ → Blatt offen | Media-Grenze 1 px |

Die FR6b-Fälle laufen auf die Reiterleiste umgestellt mit: T1 (Gruppe, Zahl, keine untere Leiste), T2 (reihum), T3 (Fensterliste unter dem Reiter), T4 (260 px → „+N“, aktiver sichtbar), T5 (420, Zahnrad weg), P1 (Pin = Reiter), O1 („+N“ listet genau die verborgenen; die Auswahl aktiviert den Reiter und holt ihn aus dem Überlauf), D1 (Desktop-Knopf im Launcher-Fuß, frei klickbar, links neben „Einstellungen“) und D2/D3 (Strg+Alt+D). TOP-5 ist auf 420 ohne Zahnrad umgestellt, die Einstellungen werden dort über das Konto geöffnet. Fremdfälle sind nur in den Selektoren angepasst (`[data-v4-taskbar]` → `[data-module-tabs]`, „Fenster nicht über Taskleiste“ → „innerhalb des Desks“, „Schmal: Desktop“ über den Startknopf).

### Endlauf (selbst ausgeführt, 23:21–23:28)

| Befehl | Ergebnis |
|---|---|
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | **157/157** (davon 13 FR6c) |
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/7b9560d … frame.test.mjs stub` | **181/181** |
| `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 … frame.test.mjs stub` | **175/175** (`results-stub.json` = dieser Lauf) |
| G0 Fensterrahmen (alle drei Läufe) | icon/nw/ne/sw/se/cluster = **1** (100 %) |
| TOP-1 G0 Kopfleiste (alle drei Läufe) | light-1280, dark-1280, dark-420: links/Mitte/rechts = **1** (100 %) |
| `node --check` topbar.js, taskbar.js, launcher.js, index.js, frame.test.mjs | ok |

### Bilder (angesehen)

- `shots/integrated-stub-fr6c-tabs-1280.png`: Reiter Aufgaben (angeheftet), Crew 6 (aktiv), Notizen, Buchhaltung. Unten keine Leiste, Launcher-Fuß hinter den Fenstern.
- `shots/integrated-stub-fr6c-tabs-420.png`: „Crew 6“ aktiv und sichtbar, „+3“, kein Zahnrad.
- `shots/integrated-stub-fr6c-launcher-desktop-1280.png`: nach „Desktop“ alle Fenster minimiert, Fuß mit Desktop/Einstellungen.
- `shots/integrated-stub-fr6c-launcher-420.png`: Launcher bei 420 mit Fuß.
- Neben welsch (links welsch, rechts v4): `fr6c-vs-welsch-1280.png`, `fr6c-vs-welsch-420.png`, `fr6c-launcher-vs-welsch-1280.png`, `fr6c-launcher-vs-welsch-420.png`. welsch wurde nur angesehen: Seite laden, Startmenü öffnen. Fuß auf welsch = [Desktop, Einstellungen], bei 1280 und 420.
- Dieselben Bilder ohne `stub-` stammen aus dem mock-Lauf. Die überholten Bilder `*fr6b-taskbar-overflow*` sind gelöscht.
