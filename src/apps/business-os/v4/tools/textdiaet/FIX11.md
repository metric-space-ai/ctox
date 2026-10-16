# FIX11 – abgeschnittene Texte bei 420 px (G4), Shell v4

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf 6a59ed2f3, Basis `src/apps/business-os/`, 16.10.2026.
Kein git außer Lesen. Nicht angefasst: invoices, research, appsec-pentest, mail (FIX10), v4/frame (Kit), Kriterium G4 (`measure.mjs`).

Labor: `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9 node v4/tools/geometry/geometry-v4.mjs --widths 420,1280`
(alle 40 Apps). Ergebnisse: `shots/FIX11/results-vorher.json`, `results-nachher.json`, dazu
`results-conversations-nachher.json` (Nachlauf nach der letzten conversations-Korrektur).

## Ergebnis

| | vorher | nachher |
|---|---|---|
| G4 rot bei 420 | 14 Apps | 1 (invoices, FIX10) |
| G4 rot bei 1280 | 3 Apps (conversations, esign, iot) | 0 |
| ganz grün (App × Breite) | 62/80 | 77/80, rot nur desktop G8 (420 und 1280, schon vorher) und invoices G4 |

## Tabelle je App (G4, Zahl = abgeschnittene Texte)

| App | 420 vorher | 420 nachher | 1280 vorher | 1280 nachher |
|---|---|---|---|---|
| consent | 1 rot | grün | grün | grün |
| conversations | 7 rot | grün | 10 rot | grün |
| credentials | 1 rot | grün | grün | grün |
| customers | 1 rot | grün | grün | grün |
| cv-print-builder | 1 rot | grün | grün | grün |
| desktop | 1 rot | grün | grün | grün |
| documents | 2 rot | grün | grün | grün |
| esign | grün | grün | 2 rot | grün |
| explorer | 2 rot | grün | grün | grün |
| intake | 1 rot | grün | grün | grün |
| invoices (FIX10, nicht angefasst) | 1 rot | 1 rot | grün | grün |
| iot | grün | grün | 1 rot | grün |
| matching | 1 rot | grün | grün | grün |
| placements | 1 rot | grün | grün | grün |
| shiftflow | 1 rot | grün | grün | grün |
| spreadsheets | 1 rot | grün | grün | grün |

Die übrigen 24 Apps waren vorher und nachher bei beiden Breiten G4-grün.

## Befunde und Lösung je App

Alle Breitenregeln laufen über `@container business-app-window (max-width: 768px)`, also die Vertragsmarke. Es gibt kein `@media` und keine neue Haltestelle.

**Hauptursache (11 von 14 Apps): der Pane-Kopf bei 420 px.** Neben der Icon-Zone links (82 px) und der Knopfzone rechts (74 px) stehen noch zwei bis vier Kopfknöpfe. Für den Titel blieben ein bis acht Buchstaben, etwa „E.“, „Kun…“ oder „Bewerbu…“.
Lösung wie in Apple Mail und Dateien am iPhone: Schmal bricht die Titelzeile um. Der Titel behält die volle Breite und bricht nur an Wortgrenzen um (`overflow-wrap: normal`, keine Silbentrennung). Die Kopfknöpfe rutschen in eine zweite Zeile darunter. Breit bleibt die Kopfzeile unverändert.

- **consent:** Gilt nur schmal, damit die Direktive vom 31.08. („Titel kürzt in der 37-px-Zeile“) breit bestehen bleibt.
  - Kopf und Titelzeile: `height: auto`. Vorher blieben sie fest 37 px hoch, die Knöpfe lagen dann über dem Suchfeld (Zwischenstand gesehen und korrigiert).
- **conversations:**
  - Die bisherige Regel „Textdiät Regel 8 (schmal): Titel kürzt“ ist durch die umbrechende Titelzeile ersetzt.
  - Listen und Kopf sind einzeilig wie in Mail, mit Tooltip (`title`) und `data-v4-allow-ellipsis`. Das betrifft:
    - Teilnehmerliste der Zeile (`.conv-thread-name`)
    - Betreff (`.conv-thread-preview`)
    - Teilnehmer im Detailkopf (`[data-conv-detail-name]`)
    - Kontozeile „Kanal · Adresse“; der Tooltip zeigt die volle Zeile und den Anbieter (Textdiät Regel 4 bleibt erhalten).
- **credentials:** Detailtitel „Neue Zugangsdaten“ bricht schmal um.
- **customers:** Die R2-Regel „Spaltentitel einzeilig mit Ellipse“ vom 15.10. ist ersetzt.
  - Ihr Anlass war ein Bruch „Kunden / konten“ mitten im Wort. Der neue Weg vermeidet ihn: Der Titel hat die ganze Zeile und `hyphens: manual`.
- **intake, placements, shiftflow, spreadsheets:** gemeinsamer Block am Dateiende (gleicher Wortlaut je App).
- **explorer:**
  - Titelzeile und Befehlsleiste dürfen schmal wachsen. Das Raster `.app-explorer` nutzt dafür `auto auto 1fr` statt `37px 37px`.
  - Die Adresse bekommt die Zeilenbreite neben ⌃, die Suche steht in einer eigenen Zeile.
  - Die Pfad-Anzeige kürzt tiefe Pfade wie der Finder: `data-v4-allow-ellipsis` plus `title` mit dem vollen Pfad (`index.js`).
- **matching:** Die Spaltenregel mit Spezifität 0,5,0 kürzte den Detailtitel. Schmal bricht „Anforderung auswählen“ jetzt in zwei Zeilen um.
- **cv-print-builder:**
  - Im Detailkopf neben „Zurück“ und vier Werkzeugen blieben „PD…“ (Kicker) und „Neuer …“ (Titel).
  - Schmal weicht der Kicker. Die Phase steht weiter in der Listenzeile, so werden die Spalten priorisiert. Der Titel bricht um.
- **documents:** Der Kopf zeigte „Angebot Wartungs…“ und „Mar…“. Schmal bricht der Titel um, der Typ („Markdown“) folgt ungekürzt.
- **esign (1280):** Kartentitel brechen um statt zu kürzen, wie in notes (V4-PORT). Die dichte Listenansicht bleibt einzeilig.
- **iot (1280):** „Assets & Signale“ wurde zu „Assets …“. Ursache war eine doppelte Einrückung (siehe Kit-Befund). Der Kopf rückt jetzt um `var(--shell-icon-inset)` ein statt um feste 80 px.
- **desktop:** Symbolbeschriftung „lib:ctox-office@1“ (unteilbarer Bezeichner) in einer Zwei-Zeilen-Beschriftung.
  - Gelöst wie im Finder: Kürzung mit Tooltip. `el.title` und `aria-label` waren schon da, neu ist `data-v4-allow-ellipsis` an `.desktop-icon-label`.
  - Offen, nicht G4: Eine Bibliothek als Desktop-Symbol mit Roh-ID als Namen ist ein Daten- bzw. Startprogramm-Befund, kein Textlängen-Befund.

## Kit-Befunde (nur vermerkt, nicht angefasst)

1. **Listen-Köpfe haben keine Schmal-Regel im Kit.**
   - `v4/frame/frame.css:335-340` gibt nur Detailköpfen (`[data-v4-md="detail"] … .ctox-pane-title-row`) bei schmaler Breite `flex-wrap: wrap` und eine Mindestbreite des Titels.
   - Listen-Köpfe (`[data-v4-md="list"]` bzw. App ohne md) bekommen nichts davon. Daher stammen 9 der 14 roten Apps mit fast gleichem Bild: Titel zwischen Icon-Zone, Kopfknöpfen und Knopfzone zerdrückt.
   - FIX11 löst das je App mit demselben Block.
   - Vorschlag an den Kit-Besitzer: dieselbe Regel generisch für jede `.ctox-pane-header > .ctox-pane-title-row` im schmalen Fenster. Die App-Blöcke würden dann überflüssig.
2. **Doppelte Einrückung unter dem Icon** (`v4/frame/pane-heads.js:81-138` `markReserved`, `frame.css:158`).
   - Schneidet eine Kopfzeile die Icon-Zone auch nur um 2 px, rückt der Rahmen sie um die volle `--shell-icon-inset` (82 px) ein. Die eigene Einrückung der App kommt noch dazu.
   - Beleg iot 1280 vorher (`shots/FIX11/iot-1280-vorher.png`): Kopf-padding 80 px + Rahmen 82 px = 162 px Einzug. Der Titel kürzte zu „Assets …“.
   - Vorschlag: um die tatsächliche Überdeckung einrücken (`inset − (row.left − content.left)`), nicht um die volle Breite.

## G4 und Ellipse mit Tooltip: Ist das Kriterium zu streng?

`measure.mjs:140-156` wertet jede Ellipse bzw. jedes line-clamp mit Überlauf als rot. Ein `title`/`aria-label` mit dem vollen Text ändert daran nichts. Einzige Ausnahme ist `[data-v4-allow-ellipsis]`.

- **Für Titel, Reiter und Feldbeschriftungen ist das richtig.** Die Kopf-Fälle oben („E.“, „Kun…“, „Neue Zugangsd…“) waren unlesbar, auch mit Tooltip. Auf dem Handy gibt es kein Hover. G4 war dort zu Recht rot.
- **Für Listenzeilen und Adress- bzw. Pfadzeilen ist es zu streng.** Dazu zählen Betreff, Teilnehmerliste, Pfad und Symbolbeschriftung. Dort ist die einzeilige Ellipse das gewohnte Muster (Mail, Finder, Dateien). Der volle Text steht beim Öffnen der Zeile bzw. im Tooltip. Ein Umbruch würde die Liste unruhig machen und die Dichte halbieren.
  - Heute muss jede App das per `data-v4-allow-ellipsis` freischalten. Das ist bewusst und prüfbar, aber das Labor sieht nicht, ob wirklich ein Tooltip da ist.
- **Vorschlag zur Präzisierung (nicht umgesetzt, Kriterium unverändert):** G4 bleibt rot für Ellipse oder Clamp in Pane-Köpfen (`.ctox-pane-header`), Reitern, Feldbeschriftungen (`label`, `dt`) und Knöpfen. Für Listenzeilen bzw. die Ausnahme `[data-v4-allow-ellipsis]` gilt eine Zusatzbedingung: `title` oder `aria-label` am Element oder an der Zeile enthält den vollen `textContent`. Fehlt das, ist es rot („Ellipse ohne Tooltip“).
  - So wird die heutige Freischaltung prüfbar, statt nur Behauptung zu sein.
  - In FIX11 trägt jede neue `data-v4-allow-ellipsis`-Stelle den vollen Text im `title`. Ausnahme ist desktop: dort hängt der `title` am Symbol `.desktop-icon`, also an der Zeile.

## Abnahme (selbst ausgeführt)

| Befehl | Ergebnis |
|---|---|
| `geometry-v4.mjs --widths 420,1280` (40 Apps), vorher | 62/80 OK; G4 rot: 14 Apps bei 420, 3 bei 1280 |
| `geometry-v4.mjs --widths 420,1280` (40 Apps), nachher, plus Nachlauf conversations | 77/80 OK; G4 rot nur invoices 420 (FIX10); desktop G8 wie vorher |
| App-Tests, je `node --import ./v4/tools/node/ctox-shared.mjs --test modules/<app>/tests/*.test.mjs` (Stub a8765b9) | consent 24, conversations 22, credentials 25, customers 17, intake 25, placements 22, explorer 27, shiftflow 9 (+1 skip), spreadsheets 43 (+1 skip), matching 14 (+1 skip), esign 21, iot 23, desktop 13, cv-print-builder 11, documents 67 (+1 skip) bestanden, 0 fail |
| `scripts/assert-shell-v2-contract.mjs` | **36/38**. Rot sind nur invoices und research mit Haltepunkt `600px` (`invoices/index.css:294`, `research/index.css:2716`). Beides sind FIX10-Dateien, die parallel bearbeitet werden (mtime 02:34/02:38); FIX11 hat sie nicht berührt. Beim ersten Lauf in diesem Strang war nur invoices rot (37/38). Alle 15 FIX11-Apps sind konform. |
| `v4/debt/ratchet.mjs` | OK (guard 1838 · scan 3, Baseline unverändert) |
| `contrast.mjs --strict --app <15 Apps> --width 1280` | G10 OK (30 App/Theme-Läufe) |
| `contrast.mjs --strict --app <15 Apps> --width 420` (zusätzlich) | 1 Fund: conversations dunkel, Detail `span.conv-message-task-chip-kind` „Arbeitsauftrag“ 4.35 < 4.5. Das ist ein Chip im Nachrichtenverlauf, den FIX11 nicht berührt; die Standard-Abnahme (KIT2) prüft nur 1280. Nicht behoben, Vermerk für den Kontrast-Besitzer. |

Bilder, selbst angesehen, in `shots/FIX11/`. Je App vorher/nachher bei 420: consent, customers, explorer, cv-print-builder, documents, matching. Dazu conversations 420 nachher und iot 1280 vorher/nachher.

- consent: „Einwilligungen“ ganz, die Knöpfe stehen darunter, das Suchfeld ist frei.
- customers: „Kunden“ steht, die Knöpfe darunter.
- explorer: „Dateien 5 Objekte“, Adresse „Dateien“ ganz, die Suche in eigener Zeile.
- cv-print-builder: „Neuer CV“, Werkzeuge in der zweiten Zeile.
- documents: „Angebot Wartungsvertrag / Halle 4“ und „Markdown“.
- matching: „Anforderung / auswählen“. Der Titel steht dicht über der Filterzeile, ist aber lesbar.
- iot 1280: „Assets & Signale“ ganz, ohne doppelten Einzug.

## Geänderte Dateien

- `modules/consent/index.css`
- `modules/conversations/index.css`, `index.html`, `index.js`
- `modules/credentials/index.css`
- `modules/customers/index.css`
- `modules/intake/index.css`
- `modules/placements/index.css`
- `modules/explorer/index.css`, `index.html`, `index.js`
- `modules/shiftflow/index.css`
- `modules/spreadsheets/index.css`
- `modules/matching/index.css`
- `modules/esign/index.css`
- `modules/iot/index.css`
- `modules/desktop/index.js`
- `modules/cv-print-builder/index.css`
- `modules/documents/index.css`

Die App-Tests haben ihre eigenen `tests/shots/*.png` neu geschrieben, wie bei jedem Testlauf.
