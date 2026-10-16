# FIX9b – UI/UX-Ranking Teil b (Shell v4): research, documents, spreadsheets, iot, appsec-pentest

Klon `~/Documents/ctox-shell-v4`, Basis `src/apps/business-os/`, Kopf 2f006b168, Stand 16.10.2026. Kein git außer Lesen, keine Handler angefasst.
Geändert wurden nur UI-Dateien (index.js/css/html, locales, tests) von research, documents, spreadsheets und iot, bei appsec-pentest nur `index.js`.
Nicht angefasst: v4/frame, Kit (shared/), vendor/ctox-office (Bibliothek), Werkzeuge, Baselines, Dateien von FIX9a/LOOP-2/KIT2.

Quelle: `~/.local/state/shell-v4/analysis/ranking/RANK-{A,B}.json` (worst/fix). Jeder Punkt wurde zuerst im v4-Stand geprüft: Vorher-Rundgang aller 5 Apps bei 1280 und 420, Bilder angesehen.

## 1. research (Vorbild Perplexity/Elicit)

Im v4-Stand gibt es kein „Zeilen werden geladen …“ mehr; die Zeilenzustände laufen schon über die Zone (`research.table.rows.*`). Folgende Punkte waren noch falsch:

- **Rohcode `invalid_graph_contract`** wurde als Text in der Graphbühne gerendert.
  - Jetzt steht im Inhalt nur noch eine verborgene Marke (`hidden`, Datenattribute für die QA bleiben).
  - Die Zone meldet `research.graph.contract` (warn): „Die Graphdaten sind unvollständig, das Diagramm bleibt leer.“
  - Mit gültigen Daten wird die Meldung geräumt.
  - `tests/research-graph-browser-qa.mjs` wartet deshalb auf `attached` statt `visible`.
- **Leere Fläche statt Leerzustand**: Die Quellenliste war ohne Quellen eine weiße Fläche.
  - Neu (nur bei gewählter Aufgabe): „Noch keine Quellen.“ und der Knopf „Nachrecherche“. Er nutzt die vorhandene Aktion `graph-ai/research`.
  - Bei aktiver Suche oder aktivem Filter steht nur „Keine Treffer.“
  - Ohne jede Aufgabe bleibt alles leer, wie es der Bestandstest „leere Collections“ verlangt.
- **Kennzahlen ohne Daten** wurden ausgeblendet:
  - „0 Elemente · 0 Verknüpfungen · 0 Themenfelder“ erscheint erst bei Knoten.
  - Der Abschnitt „Evidenz-Ranking · 0 verifiziert“ erscheint erst mit verifizierten Quellen.
  - „· 0 verifizierte Quellen“ bzw. „0 Quellen“ an den Aufgaben entfällt.
- **Kopf und Fachbegriffe übersetzt**:
  - Linke Spalte „Dashboards“ → „Recherchen“ (sie zeigt Aufgaben).
  - „Semantischer Research-Graph“ → „Themengraph“; „LIVE · CTOX aktualisiert“ → „Lauf aktiv“.
  - Knowledge → Wissen („Wissen aufbauen/aktualisieren“); „Scoring bearbeiten“ → „Bewertung bearbeiten“.
  - Insights → Erkenntnisse; „Dashboard anlegen“ → „Recherche anlegen“.
  - Live-Projektion → Abgeleitete Ansicht; Discovery Graph → Fundstellen-Graph; Evidence → Beleg.
  - Der Tooltip „Ziehen: drehen · Scrollen: zoomen“ ist weg; das aria-label bleibt.
  - Belegstatus der Quellkarten auf Deutsch, z. B. „Not verified“ → „Nicht verifiziert“. `gate.label` bleibt der englische Prüfcode, weil `test.mjs` darauf prüft.
- **Befehlskennungen (cmd_…)** erscheinen in v4 nicht in der Oberfläche. Sie stehen nur im Auftragstext an CTOX. Der Test sichert das ab.

## 2. documents und spreadsheets: Lesevorschau statt Wartezeit

- **Lesevorschau sofort**:
  - documents: Ein Word-Dokument rendert beim Öffnen sofort eine Lesevorschau.
    - Quelle ist die letzte Fassung: `document_versions.model_json` über `exportMarkdown` → `renderMarkdownPreview`.
    - Fallback ist `documents.index_text`.
  - spreadsheets: Die Vorschau ist ein schreibgeschütztes Raster aus `spreadsheet_versions.model_json.data`.
    - Höchstens 200 Zeilen × 26 Spalten, Zahlen rechtsbündig.
  - Ohne Daten bleibt die Vorschau verborgen. Es gibt keinen Wartetext.
- **Editor im Hintergrund**:
  - Der Office-Editor (ctox:lib/ctox-office) startet dahinter, mit Layout, aber unsichtbar (`.is-warming`, `visibility:hidden`).
  - Erst wenn `editor.open()` fertig ist, wird die Vorschau entfernt und der Editor sichtbar.
  - Scheitert der Editor, bleibt die Vorschau lesbar und der Fehler geht wie bisher in die Zone.
  - Wiederholungen beim Start (`onRetry`) lassen die Vorschau stehen.
- **Vorwärmen**: Liegt in der Liste ein Word-Dokument bzw. eine Tabelle, lädt das Bibliotheksmodul `ctox:lib/ctox-office@1/…mjs` im Leerlauf (`requestIdleCallback`). Die Bibliothek selbst ist unverändert.
  - Grenze: Die eigentliche Startzeit (iframe `frame.html` → `editor.ready` → Rust-prepare) liegt in der Bibliothek und im Kern. Sie lässt sich ohne Änderung der Bibliothek nicht vorziehen. Die Wartezeit für den Nutzer verschwindet trotzdem, weil die Vorschau sofort steht.
- **Zeilensymbol**: Das unerklärte Sonnen-/Zahnradsymbol an jeder Zeile ist jetzt „⋯“.
  - title „Weitere Aktionen“, aria-label „<Titel> – Weitere Aktionen“.
  - Die Aktion (Verwalten-Schublade) ist unverändert.
- **Abgeschnittener Platzhalter**: „Dokument such…“ bzw. „Tabelle suchen…“ → „Suchen“. Das aria-label „Dokumente/Tabellen suchen“ bleibt.

### Ladezeit-Messung (Rundgang-Labor, echte Maus, Stub a8765b9)

Werkzeug: Kopie von `rundgang2.mjs` im Scratchpad. Darin sind zwei Dinge anders als im Original:

1. Sie veröffentlicht die Bibliotheken wie das Geometrie-Labor (`lib:ctox-office@1.0`, Shell mit `libs`). Ohne das lädt `ctox:lib` im Rundgang gar nicht (siehe unten).
2. Sie klickt die zweite Listenzeile (Word bzw. XLSX).

Gemessen wird in der Seite per `requestAnimationFrame` ab dem Klick:

- erste sichtbare Vorschau,
- iframe vorhanden,
- Editor bereit (Maske weg und iframe-Inhalt ohne Ladetext),
- Editorfehler in der Zone.

„Vorher“ ist derselbe Lauf mit dem HEAD-Stand von index.js/css/html (zeitweise eingesetzt, danach per `cmp` zurückgestellt).

| App | Breite | Vorher: erste sinnvolle Anzeige | Nachher: erste sinnvolle Anzeige | Editor im Labor |
|---|---|---|---|---|
| documents | 1280 | keine (leere Fläche; Fehler nach 247 ms) | **14 ms** (Lesevorschau) | Fehler nach 230 ms (vorher 247 ms) |
| documents | 420 | keine (Fehler nach 246 ms) | **16 ms** | Fehler nach 217 ms |
| spreadsheets | 1280 | keine (Fehler nach 3190 ms) | **14 ms** (Raster) | Fehler nach 3182 ms |
| spreadsheets | 420 | keine (Fehler nach 3207 ms) | **23 ms** | Fehler nach 3171 ms |

Zur Tabelle:

- Im Labor wird der Editor nie bereit. Der Stub liefert kein Rust-prepare: „CTOX Rust prepare did not return a Euro-Office DOCY/XLSY editor payload“. Dieser Fehler geht in die Zone; die Vorschau bleibt stehen.
- Die 12–30 s aus dem Ranking (welsch) sind die echte Engine-Startzeit. Sie ist im Labor nicht nachstellbar. Nachher ist der Inhalt dort ebenso nach wenigen Millisekunden lesbar, weil die Vorschau nur lokale Daten braucht.
- App-Test-Labor (eigene FIX9b-Tests, ctox:lib → vendor/ctox-office): Vorschau nach 102 ms (documents, ab Mount inkl. Modul-Import) bzw. 32 ms (spreadsheets).

Bilder (angesehen):

- Scratchpad `timing/{before,after}/{documents,spreadsheets}/{1280,420}-01-timing.png`.
- Vorher: leere Fläche, Sonnensymbol, „Dokument such…“.
- Nachher: Text bzw. Raster sofort, „⋯“, „Suchen“.

## 3. iot: Detailtitel bei 420 mit Ellipse (G4)

- Im schmalen Fenster (`@container business-app-window (max-width: 600px)`, Liste→Detail) bricht der Detailtitel um, statt abgeschnitten zu werden.
  - Höchstens drei Zeilen, mit Silbentrennung (`hyphens:auto`, `overflow-wrap:break-word`). Die Kopfzeile wächst mit.
- Nur iot/index.css; der Kit bleibt unverändert.
- Vorher „Budgetplan…“, nachher „Budgetpla-/nung Vertrieb/Süd“.
  - Ein erster Versuch mit zwei Zeilen und `anywhere` brach mitten im Wort („Budgetplanun/g“) bzw. kürzte auf „Vertrie…“. Verworfen nach Ansicht des Bilds.
- Bei 1280 bleibt der Titel einzeilig.

## 4. appsec-pentest (nur Anzeige)

- `friendlyProfile()` gibt für unbekannte Profilwerte nichts mehr zurück. Vorher kam der Rohwert, z. B. „projektplan“, als Kicker „PROJEKTPLAN“ und als Pille.
  - Der Kicker entfällt, wenn kein lesbares Profil vorliegt.
- Neu `readableTestTitle()`: Kleinschrift-Kennungen (`/^[a-z0-9_-]+$/`, ohne Punkt, Leerzeichen und Großbuchstaben) sind keine Titel.
  - Reihenfolge: Ziel, Name, sonst „Unbenannter Test“.
  - Gilt für Listentitel und Detailtitel.
- Die Zweitzeile der Karte nutzt den Namen nur, wenn er lesbar ist.
- Keine Handler und kein CSS angefasst.

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9`)

| Prüfung | Ergebnis |
|---|---|
| `node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/<app>/tests/*.test.mjs` | research 81/81, documents 15/15 (+1 opt-in-Skip Screenshots), spreadsheets 12/12 (+1 opt-in-Skip), iot 12/12, appsec-pentest 11/11 |
| Wurzeltests `research/test.mjs`, `appsec-pentest.test.mjs`, `documents.test.mjs`, `spreadsheets.test.mjs`, `iot.test.mjs` | 143/143. Ein Zwischenstand brach „spreadsheet csv editor failure …“, weil die Attrappe ohne `querySelector`/`prepend` arbeitet; die Vorschau-Hilfen prüfen das jetzt (optional chaining) |
| `rundgang2.mjs --app <5 Apps> --widths 1280,420` (Bilder `shots/FIX9b/<app>/`) | 10× OK, 0 abgelehnte Schreibvorgänge, Geometrie wie vorher (research-table-host 1280, iot filterbar 420 – beide schon vorher, offen bei Orchestrator) |
| `check-raw-ids` 1280 und 420 | alle 5 Apps 0 |
| `contrast.mjs` (G10) 1280 und 420, hell/dunkel | 20/20 OK, 0 zu schwach |
| `textdiaet-rule.mjs --app <5 Apps>` | 44 Treffer (HEAD 44), alle help-text in research-Bestand, unter Baseline |
| `node v4/debt/ratchet.mjs` | OK (4 Zähler gesunken, nicht festgeschrieben). Zwischenstand R16-css research 0→2 durch ungescopte `.research-empty`-Selektoren behoben |
| `check-i18n-keys` | Locales vollständig für alle neuen Schlüssel (de/en). Die Zähler „embedded“ steigen um die neuen Schlüssel (research 144→147, documents 117→118, spreadsheets 46→47): Die drei Apps haben kein eingebettetes Wörterbuch, der Prüfer zählt dort jeden Schlüssel. Alle neuen `state.t`-Aufrufe tragen einen deutschen Rückfalltext, roh erscheint kein Schlüssel |

Zur Rundgang-Zeile: Die Konsolenfehler bei documents und spreadsheets sind der bekannte Werkzeugbefund aus RUND2: rundgang2 entfernt die Shell-Bindung, deshalb lädt `ctox:lib` dort nicht. spreadsheets hat nachher mehr Schritte (39 statt 27) und mehr dieser Fehler. Grund: Die Zeilenknöpfe tragen jetzt je Zeile ein eigenes aria-label, und der Rundgang bedient jede Verwalten-Schublade statt nur einer.

### Tests und Mutationsbelege

Neue Testdateien:

- `modules/research/tests/fix9b.test.mjs`
- `modules/documents/tests/fix9b.test.mjs`
- `modules/spreadsheets/tests/fix9b.test.mjs`
- `modules/iot/tests/fix9b.test.mjs`
- `modules/appsec-pentest/tests/fix9b.test.mjs`

Mutationsprobe:

- App-Dateien durch den HEAD-Stand ersetzt (`git show`, nur lesend). Bei JS sind nur die neuen Test-Hooks als Stub ergänzt.
- Volle Testdatei gefahren, danach zurückgestellt und per `cmp` bestätigt.

| Punkt | Test | Ohne Fix |
|---|---|---|
| 1 research | 4 Tests (Kopf/Begriffe, Kennzahlen, Rohcode→Zone, Leerzustand) | 4/4 rot: `'Dashboards'`, „0 Elemente · 0 Verknüpfungen …“, `invalid_graph_contract` im Inhalt, kein `data-research-empty` |
| 2 documents | Lesevorschau < 3 s, Editor unsichtbar dahinter; index_text-Fallback; ⋯ + Platzhalter | 3/3 rot (Vorschau fehlt, Fallback leer, title „… verwalten“) |
| 2 spreadsheets | Raster-Vorschau, Editor unsichtbar dahinter; ⋯ + Platzhalter | 2/3 rot (Vorschau fehlt, „Tabelle verwalten“). Der dritte Test („ohne Zellinhalt keine Vorschau“) ist ein Wächter gegen leere Raster, mit Stub naturgemäß grün |
| 3 iot | Titel 420 ohne Ellipse, ≤ 3 Zeilen; 1280 einzeilig | rot: `{"sw":315,"cw":298,"ellipsis":"ellipsis"}` |
| 4 appsec | keine Kennung als Titel/Kicker/Pille, „Unbenannter Test“, Kicker bei bekanntem Profil | rot: Liste zeigt `projektplan`, `lieferantenbewertung` |

Angepasste Bestandstests:

- research `v4-api.test.mjs` „leere Collections“: `.research-ranking-list` darf fehlen, weil der Abschnitt ohne Daten nicht mehr gerendert wird.
- `research-graph-browser-qa.mjs`: wartet auf `attached` statt `visible`.

## Offen bzw. gemeldet (nicht FIX9b)

- **appsec-pentest/index.css**: Im Arbeitsbaum fehlt die DARK-Regel `.ctox-run-control { color: var(--surface); }` (4 Zeilen gegenüber HEAD). Das stammt nicht von FIX9b. Meine Sicherung vor der Mutationsprobe (02:01) hatte die Zeilen schon nicht mehr. Ein paralleler Strang hat sie entfernt. Bitte prüfen; Kontrast G10 war trotzdem grün.
- **rundgang2.mjs** (Werkzeug, nicht geändert): Ohne Bibliotheksbindung lädt `ctox:lib` nicht. Bindung wie in `geometry-v4.mjs` (Bibliotheken plus Shell mit `libs` veröffentlichen) würde Editorläufe im Rundgang ermöglichen. Die Scratch-Kopie zeigt, dass es so geht.
- **Echte Editor-Startzeit** (12–30 s auf welsch): Sie lässt sich nur in der Bibliothek oder im Kern senken, etwa durch ein vorgestartetes iframe oder ein gecachtes Rust-prepare. Die Bibliothek bot keinen Vorwärm-Einstieg, und sie war nicht zu ändern.
- **research**:
  - Der Knopf „Research fortsetzen“ und der Kicker „Web Research“ bleiben, weil „Web Research“ der App-Name ist.
  - Die Messpunkte-Tabelle scrollt weiter seitlich (Befund aus RUND2).
  - Die Erklär-Tooltips der Messspalten stehen im Bestand.
- **appsec-pentest**:
  - Die Profil-Auswahl im Formular zeigt die Rohwerte „standard/full“.
  - Das Datum erscheint als „1/1/1970“ (US-Format, Fixture ohne Zeit).
  - Beides ist Anzeige außerhalb von Punkt 4 und aus RANK-A bekannt.
- **spreadsheets 420**: Das App-Symbol und der rote Zonen-Zähler des Rahmens überdecken die Ecke der Spaltenköpfe (A1). Gleiches gilt für den Editor; das ist Rahmengeometrie.
