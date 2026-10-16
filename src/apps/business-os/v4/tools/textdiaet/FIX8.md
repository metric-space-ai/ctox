# FIX8 – sechs kleine App-Mängel aus GATE und RANK-A (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Basis `src/apps/business-os/`, Stand 16.10.2026. Kein git außer Lesen, keine Handler angefasst.
Nur UI-Dateien von explorer, esign, file-viewer, calendar, browser und knowledge. Nicht angefasst: v4/frame (FR6d), glossary.js, die Werkzeuge und die Baselines.

Quelle der Mängel: `~/.local/state/shell-v4/analysis/ranking/RANK-A.json` und `v4/debt/GATE.md` (esign-Nebenbefunde).
RANK-A lief gegen einen älteren App-Stand. Zwei Befunde sehen im Klon anders aus (siehe 3 und 5). Behoben ist jeweils der Kern des Befunds.

## 1. explorer: Art-Spalte englisch

- `kind` der Quellen ist jetzt ein Textschlüssel (`kindKnowledge` usw.) statt eines englischen Rohworts.
  - DE: Dateisystem, Datei, Dokument, Tabelle, Wissen, Objekt, Unternehmen.
  - EN: File system, File, Document, …
- `mimeKind` läuft über die App-Texte: PDF-Dokument, Word-Dokument, Bild, Text, Markdown.
- Statuswerte der Datensätze (Spalte Größe/Status) gehen über einen Spiegel der Glossar-Begriffe `STATUSES`: draft → Entwurf, active → Aktiv usw.
  - Unbekannte Werte bleiben unverändert.
  - `glossary.js` ist nur gelesen. Apps importieren den Rahmen nicht.
- `kind` gilt nicht mehr als Status. Vorher standen dort „confirm“ und „folder“ roh.
- Datum im Format der App-Sprache (`de-DE`) statt Browser-Locale. Vorher erschien z. B. „1/1/70, 1:00 AM“.
- Die Vorschau zeigt „Ort“ bzw. „Unbenannt“ über die App-Texte.

## 2. esign: Status roh, Detailbereich dunkel auf dunkel

- `STATUS_KEY` kennt jetzt `done` → „Abgeschlossen“ und `failed` → „Fehlgeschlagen“, wie DocuSign.
  - Beide zählen als abgeschlossen (`TERMINAL_STATUSES`).
  - Badge: done grün, failed rot.
  - Alle Statuslabels beginnen groß: Angelegt, Gesendet, Teilweise signiert, Abgeschlossen, Abgelehnt, Abgelaufen.
  - Unterzeichner: Offen, Unterschrieben, Abgelehnt.
  - „Ausstehend“ habe ich vermieden, weil es im Zustandsvokabular `de-ausstehend` steht.
- **Ursache dunkel auf dunkel:** `index.css` setzte Modul, Pane und Kopfzeilen auf `--shell-v2-surface(-alt)`.
  - Diese Flächen sind im Shell-v2-Fenster fest dunkel (#10151c/#141b24), auch im hellen Theme.
  - Text und Felder nutzen dagegen die hellen Theme-Tokens.
  - Gemessen ohne Fix im Shell-v2-Fenster: Kopfzeilen „Anfragen“ und „Vertrag …“ mit Kontrast 1,16, Detailwerte mit 2,24.
- Fix: Flächen über `--bg/--surface` und `color: var(--text)`, nach dem Muster von knowledge.
- Zusätzlich hat `.esign-shard-meta--secondary` jetzt volles `--muted`. Auf der gewählten Zeile lag der Kontrast bei 2,89.
- Gleiche Ursache, mitbehoben:
  - **calendar**: Kopfzeilen „Meine Kalender“/„Kalender“ und der Kopf des Seitenblatts waren dunkel auf dunkel.
  - **browser**: gleiche Kopfzeilen. Dazu zeigte der aktive Reiter dunklen Text auf dunklem Grund, weil die Rückfallfarben `--panel/--border/--hover` im Rahmen fehlen. Sie sind jetzt durch `--surface-2/--line/--surface` ersetzt.

## 3. file-viewer: ohne Datei

- RANK-A sah „Datei konnte nicht geöffnet werden: Dateiinhalt fehlt.“ Im Klon war die Fläche nur leer.
- Neu ist ein Leerzustand wie bei Vorschau und Fotos: „Keine Datei geöffnet“ mit der Aktion „Datei öffnen…“.
- Die Aktion öffnet im Fenster eine Auswahl aus der erlaubten Collection `files` (lesend, deklariert in `v4-manifest.json`).
  - Suche über Name und MIME, höchstens 200 Einträge.
  - Abbrechen per Knopf oder Esc.
  - Das Abo läuft nur, solange die Auswahl offen ist.
  - Die Wahl öffnet die Datei über `open({ref, file_id, …})` wie eine Route.
- Der Leerzustand liegt neben `[data-file-content]` und nicht darin. Bestehende Tests („Inhalt leer“) gelten deshalb unverändert.
- Das eingebettete Wörterbuch heißt jetzt `labels` statt `FALLBACK`, damit `check-i18n-keys` es findet. Die Locales sind ergänzt.

## 4. calendar: Klick auf Termin

- Ein Einfachklick öffnet nach der Doppelklickzeit (260 ms) das Seitenblatt „Termindetails“, nur zum Lesen.
  - Inhalt: Zeit (de-DE, z. B. „Fr., 16.10.2026 10:00 – 11:00“), Kalendername, Ort, Beschreibung. Leere Felder fehlen.
  - Unten rechts der Knopf „Bearbeiten“.
- Ein Doppelklick (`event.detail ≥ 2`) öffnet direkt das Bearbeitungsformular.
- Die Wartezeit ist nötig, weil das Seitenblatt sonst den zweiten Klick abfängt. So hat es die Probe im echten Rahmen gezeigt.
- Das Seitenblatt setzt keinen Bearbeitungszustand. Es schreibt nichts und landet nicht im Snapshot. Der Timer wird beim Unmount geräumt.
- Im aktuellen Rahmen öffnete ein Klick vorher sofort das Formular. RANK-A sah im älteren Stand gar keine Wirkung.

## 5. browser: Leerzustand

- Der Text „Öffne oben rechts eine neue“ aus RANK-A steht im Klon nicht mehr; die Textdiät hatte ihn entfernt. Übrig blieb eine Sackgasse ohne jeden Hinweis.
- Jetzt erscheint ohne jede Sitzung genau einmal auf der Bühne „Keine Browser-Sitzung.“ mit dem Knopf „Neue Sitzung“, ohne Ortsangabe.
  - Der Knopf löst den vorhandenen Kopfknopf aus. Die Start-Logik bleibt damit eine.
  - Sobald eine Sitzung existiert oder die Skript-Ansicht offen ist, verschwindet der Leerzustand.
- Neu ist ein eingebettetes Wörterbuch `labels` (de/en, Inhalt wie die Locales). Der Übersetzer fällt sprachrichtig darauf zurück. Vorher bekam die englische Oberfläche deutsche Rückfalltexte.
- 14 fehlende Locale-Schlüssel sind ergänzt.

## 6. knowledge: „21.01.1970“

- Neu ist `toEpochMs` (exportiert):
  - Sekunden (< 1e11) werden mit 1000 multipliziert.
  - Text geht über `Date.parse`.
  - 0, Ungültiges und alles vor 2000 gilt als „kein Datum“.
- Genutzt in `shortDay`, `groupRecency` und `groupCreated`, damit die Sortierung zur Anzeige passt.
- Fehlt ein Datum, steht nichts da: kein Ersatztext „Noch nicht bearbeitet“ (Schlüssel entfernt) und kein loser Trenner.

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9`)

| Prüfung | Ergebnis |
|---|---|
| `node --import ./v4/tools/node/ctox-shared.mjs --test modules/<app>/tests/*.test.mjs` | explorer 11/11, esign 21/21, file-viewer 10/10, calendar 11/11, browser 11/11, knowledge 15/15 |
| Wurzeltests der Apps | explorer.test 16/16, file-viewer.test 6/6, browser.test 1/1, browser-start-contract 1/1, knowledge test.mjs 1/1, calendar.test 14/16 |
| `node v4/tools/guards/textdiaet-rule.mjs --app <6 Apps>` | 0 Treffer |
| `node v4/tools/textdiaet/check-i18n-keys.mjs --app <6 Apps>` | 0 Lücken (exit 0) |
| `check-raw-ids` 1280 und 420 | alle 6 Apps 0 (mit Ersatzlauf, siehe unten) |
| `rundgang2.mjs --app <6 Apps> --widths 1280,420` | 12× OK, 0 Fehler, 0 Geometrie |
| `node v4/debt/ratchet.mjs` | OK. Meine Zwischenstände hatten ihn erhöht; behoben, siehe unten |

Zu den Zeilen der Tabelle:

- **calendar.test 14/16**: Die 2 Fehler (Buchungsseiten-Slug, `data-count-view-month`) gibt es genauso auf HEAD. Gegengeprüft mit HEAD-index.js/css: 14/2.
- **i18n explorer**: Der Prüfer sieht 0 Schlüssel, weil er das Muster `T.<key>` nicht erfasst. Die Art-Texte deckt der FIX8-Test ab.
- **check-raw-ids**: Das Werkzeug klickt noch `[data-v4-app] [data-v4-open]`. Diesen Launcher gibt es seit FR6d nicht mehr, deshalb laufen alle Apps in einen Timeout.
  - Ich habe das Werkzeug nicht geändert.
  - Stattdessen lief eine Scratch-Kopie mit dem Desktop-Selektor `[data-v4-desk-app]` (wie rundgang2). Ergebnis: 0 bei 1280 und 420.
  - Bitte an FR6d/RUND melden.
- **ratchet**: Zwischenzeitlich stieg er.
  - browser: englische Wörterbuchzeilen ohne `v4-allow: en`, „not available“.
  - esign: „Ausstehend“.
  - explorer: „Läuft“ und „Warteschlange“ im Status-Spiegel.
  - Behoben durch Zeilenmarker, Umformulierung, „Offen“ und `v4-allow: label` am Status-Spiegel.
  - Die gesunkenen Zähler habe ich nicht festgeschrieben, das ist Sache des Baseline-Besitzers.
- **textdiaet knowledge**: Eine Zeile (index.js:3261, Marker in 3260; Spaltenhilfe zum Propeller) stand schon auf HEAD. Sie ist nur ein Tooltip (`columnHeaderHelp` → title/aria-label/data-tooltip) und trägt jetzt `// v4-allow: text`.

### Tests und Mutationsbelege

Jeweils so geprüft: die App-Datei durch den HEAD-Stand ersetzt (`git show`, nur lesend), die volle Testdatei gefahren, danach zurückgestellt und per diff bestätigt.

| Mangel | Test | Ohne Fix |
|---|---|---|
| 1 explorer | `tests/v4-api.test.mjs` „FIX8: Art und Status deutsch …“ | 1 fail: `"KNO Leitfaden Onboarding Knowledge 1/1/70, 1:00 AM active"` |
| 2 esign | „FIX8: done/failed deutsch …; Kopf- und Detailbereich im Shell-v2-Fenster hell und dunkel lesbar (Kontrast ≥ 3)“ | CSS alt: 1 fail (`H2 "Anfragen" 1.16`, `DD … 2.24`). JS alt: 2 fail (`/Abgeschlossen/`, `/Teilweise signiert/`) |
| 3 file-viewer | „FIX8: ohne Datei Leerzustand + „Datei öffnen…“; Auswahl aus `files` …“ | 1 fail: „Leerzustand sichtbar“ |
| 4 calendar | „FIX8: Klick auf Termin → Details (nur lesen), Doppelklick → Bearbeiten; keine Schreibvorgänge“ (echte Maus) | 1 fail: „Termindetails statt Formular“ |
| 5 browser | „leere Collections: … Bühne zeigt genau einen Satz + „Neue Sitzung“ (FIX8)“ | 1 fail: „Bühne zeigt den Leerzustand“ |
| 6 knowledge | „FIX8: Datum aus Sekunden, Millisekunden und ISO richtig; ohne Datum nichts“ | 1 fail: `"Sekunden Skill … 21.01.1970"`, `"… Noch nicht bearbeitet"` |

Was die Tests im Einzelnen prüfen:

- **esign**: Der Test legt das Fenster als `.shell-window[data-shell-contract="v2"]` an. Damit gelten die echten Shell-v2-Flächen aus app.css. Gemessen wird der Kontrast aller Textknoten im hellen und im dunklen Theme, mit Canvas-Farbnormalisierung und Alpha-Mischung.
  - Ausgenommen sind nur Primärknöpfe, weil deren Akzent zur Fensterpalette des Rahmens gehört.
- **calendar**: Nach dem Klick prüft der Test „Bearbeiten“ im Seitenblatt, nach dem Doppelklick das Formular. Am Ende dürfen sich die `updated_at_ms` der Termine nicht verändert haben.
- **browser**: Der Test prüft, dass keine Ortsangabe im Text steht, dass der Leerzustand genau einmal erscheint und dass die Aktion `browser.session.start` auslöst.
- Angepasste Bestandstests:
  - browser: Der Leerzustandstest erwartet jetzt den einen Satz statt „nichts“.
  - esign: `/teilweise signiert/` heißt jetzt `/Teilweise signiert/`.

### Screenshots (selbst angesehen)

App-Tests:

- `modules/esign/tests/shots/esign-v2-light-1280.png` und `esign-v2-dark-1280.png`
- `modules/calendar/tests/shots/calendar-details-1280.png`
- `modules/file-viewer/tests/shots/file-viewer-picker-1280.png`
- `modules/browser/tests/shots/browser-empty-900.png`

Rundgang im echten Rahmen, Fenster öffnen über Desktop-Symbole: `v4/tools/textdiaet/shots/FIX8/<app>/<breite>-NN-*.png`. Angesehen:

- esign 1280/420: Kopf „Anfragen“ und „Signatur anfordern“ lesbar, heller Arbeitsbereich, Status deutsch.
- calendar 1280/420: Kopfzeilen lesbar.
- browser 1280/420: Kopf lesbar, aktiver Reiter lesbar.
- explorer 1280/420, Wissen: Art „Wissen“, keine Rohwerte.
- file-viewer 1280/420: Leerzustand und Auswahl.
- knowledge 1280: Daten 14.10./09.10./08.10.2026.

Vorher-Bilder mit dunkel auf dunkel (esign, calendar) habe ich im Scratchpad angesehen. Die Probe im echten Rahmen hat gezeigt, dass ein Klick auf einen Termin vorher sofort das Formular öffnete.

## Offen (nicht FIX8, nur vermerkt)

- **Gleiche Ursache in weiteren Apps:** customers, mail, news, intake, interviews, invoices, iot, buchhaltung, consent, conversations, app-store, importer setzen dieselben fest dunklen `--shell-v2-surface`-Flächen. Ihre Kopfzeilen sind im hellen Theme vermutlich ebenfalls dunkel auf dunkel. Das ist ein eigener Strang. Alternativ könnte der Rahmen die Tokens je Theme setzen (FR6d/Shell-v2).
- **esign**: Der Untertitel „string“ kommt aus der Fixture (`subject_kind`).
- **knowledge**:
  - Kopf und Fenstertitel „Knowledge“ sind englisch. Der bestehende Test erwartet „Knowledge“; das Glossar kennt „Wissen“.
  - Dazu kommen die Rohtitel und englischen Fachbegriffe aus RANK-A.
- **browser 420**: Der Kopf „Sitzungen“ bricht zu „Sit/zun/gen“ um. Das ist vorhanden und gehört zum Layout.
- **explorer**: Die Systemordner heißen Desktop, Documents, Downloads (englische Ordnernamen). macOS zeigt dort lokalisierte Namen.
- **calendar**: Das Datums-Eingabefeld zeigt im Headless-Chromium das US-Format (`datetime-local` folgt der Browsersprache). Das steht in RANK-A, ist aber kein FIX8-Mangel.
