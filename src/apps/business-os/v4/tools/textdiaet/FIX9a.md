# FIX9a – Sackgassen und Standardabweichungen aus dem UI/UX-Ranking (Shell v4, Teil a)

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf 2f006b168, Basis `src/apps/business-os/`. Stand 16.10.2026.
Kein git außer Lesen, keine Subagents, keine Handler angefasst.

Geändert sind nur UI-Dateien von mail, invoices, news, placements, submissions und interviews (index.js/css/html, mail-Locales, Tests).
Nicht angefasst: v4/frame, v4/shell, die Werkzeuge, die Baselines sowie die Apps von FIX9b, LOOP-2 und KIT2.

Quelle: `~/.local/state/shell-v4/analysis/ranking/RANK-{A,B,C}.json`, Felder worst/fix. Jeder Punkt wurde zuerst im v4-Stand geprüft, im Rahmen per rundgang2 (Bilder vorher unter `r2-base`).

## 1. mail: Verfassen-Schublade war eine Sackgasse

Im v4-Stand bestätigt: Esc tat nichts. Das × lag bei 1280 genau unter den Fensterknöpfen (Rechteck 1230–1256 × 26–52 in der 96 × 44-Schutzzone).

- **Esc** schließt die Schublade, wie in Outlook und Apple Mail.
  - Ist das Formular leer (An, Betreff und Text leer), schließt sie sofort.
  - Hat es Inhalt, fragt `api.shell.dialog` „Entwurf behalten?“ mit den Optionen „Als Entwurf speichern“ (vorgewählt) und „Verwerfen“.
  - „Abbrechen“ bzw. Esc im Blatt lässt die Schublade samt Inhalt offen. Bei einer reinen Ja/Nein-Abfrage hätte Esc im Blatt den Entwurf verworfen; deshalb ist es eine Auswahl.
  - Speichern läuft über den bestehenden Weg (`submitComposer('draft')`, ein `outbound.message.compose`). Absender und Gruppe zählen nicht als Inhalt, weil sie vorbelegt sind.
- **Das ×** folgt derselben Logik wie Esc.
- **Schutzzone:** Verfassen-Schublade und Mail-Einstellungen beginnen unter `--shell-controls-zone-height` (Rahmen-Variable, Rückfall 0). Gilt bei 1280 und in der schmalen Ansicht.
- Die Schublade lässt sich jetzt immer per Esc, × oder Speichern/Senden verlassen, ohne das App-Fenster zu schließen.
- Neue Texte stehen im eingebetteten Wörterbuch und in `locales/de|en.json`.

## 2. invoices

- **„Neue Rechnung“** legte sofort per `createDraft` einen Entwurf an (im v4-Stand bestätigt: ein `invoices.invoice.create` je Klick).
  - Jetzt öffnet `openNewDraft()` einen leeren Editor ohne Persistenz, wie lexoffice/sevDesk. Der Kunde ist leer, es gibt keine Positionen und keinen Lösch-Knopf.
  - Erst „Entwurf speichern“ legt die Rechnung an: ein `invoices.invoice.create` mit dem vollständigen Editorstand. Ohne Kunde ist der Knopf gesperrt, der Tooltip lautet „— bitte Kunde wählen —“.
  - „Buchen“ auf einer neuen Rechnung ruft erst create auf und hängt post an dessen done.
  - „Abbrechen“ und das × im Kopf verwerfen den Editor. Es bleibt kein Befehl und keine Zeile zurück.
  - Der Snapshot trägt `edit.isNew`, sodass ein offener neuer Entwurf einen Remount übersteht.
- **Suche ohne Treffer:** Im v4-Stand war die Liste nur leer, ohne Text (RANK-C sah noch „Noch keine Rechnungen vorhanden“).
  - Jetzt steht in der Liste „Keine Treffer“ mit dem Knopf „Filter zurücksetzen“. Er löst den vorhandenen `[data-pg-reset]` der Spaltengrammatik aus.
  - Die Meldung erscheint nur, wenn Daten da sind und Suche oder Art-Filter aktiv ist.
- **Status abgeschnitten:** Im v4-Stand nicht reproduzierbar. Im Rahmen bei 1280 in Listenansicht stand „Teilweise bezahlt“ vollständig da, ebenso im Test bei 230 px Spaltenbreite (auch gegen HEAD grün).
  - Keine CSS-Änderung. Die Prüfung bleibt als Regressionswächter im Test.
- **„Buchen (GoBD-post)“:** Deutsch hieß es auf HEAD schon „Buchen“. Englisch hieß es „Post (GoBD)“, jetzt „Post“.
- **Geprüft und schon behoben (TEXT-C):**
  - Menge in Stück, Einzelpreis in Euro (keine ‰- oder Cent-Spalten).
  - Beträge im Format „32.903,50 €“, Daten als TT.MM.JJJJ.
  - Die Adresse als Text, kein „[object Object]“.
- **Offen:** Das native Datumsfeld im Editor zeigt im Headless-Chromium „10/16/2026“. Es folgt der Browser-Sprache, nicht der App. Ein eigenes Datumsfeld wäre ein eigener Strang.

## 3. news

- **Laufende Ausgabe:** Neu ist `displayedIssue()`. Hat die gewählte Ausgabe den Status queued oder running und noch keine Beiträge, zeigt der Hauptbereich die letzte fertige Ausgabe desselben Themas.
  - Das gilt für Feed, Titel, Kategoriezähler und Trends.
  - Der Erstellstatus steht weiter nur in der Zone (`news.issue.running`) und als Punkt am Ausgabeknoten. Erklärprosa gibt es im Hauptbereich keine.
  - „Ausgabe erzeugen“ bleibt für die laufende Ausgabe gesperrt.
- **Leere Kategoriereiter (0):** Sie werden ausgeblendet, „Alle“ bleibt. Steht die Auswahl auf einem leer gewordenen Reiter, gilt wieder „Alle“.
- **Leere Trend-Spalte:** Sie wird ausgeblendet, samt Teiler. Der Feed nimmt die Breite (`.is-no-trends`).
  - Die Spaltenvorlage steht in `:where()`, damit die schmalen Container-Regeln weiter greifen.

## 4. placements und submissions: vorbefülltes Anlegeformular über dem Datensatz

Im v4-Stand bestätigt: Ein Klick auf einen Datensatz befüllte das Formular per `fillForm`, und „Anlegen“/„Vorstellen“ stand direkt darüber.

- Das Formular ist jetzt ein eigener Modus `state.composing`.
  - Nur „+ Neu“ öffnet es, und zwar leer.
  - Ist ein Datensatz gewählt, ist es ausgeblendet. Das Detail zeigt dann nur den Datensatz.
- `fillForm` ist entfernt. Neu ist der Knopf „Abbrechen“ im Formular.
- Nach dem Anlegen gilt:
  - placements: das Formular schließt.
  - submissions: es bleibt leer im Anlegemodus, weil „Vorstellen“ dort bewusst Serienarbeit ist (bestehender Code-Kommentar).
- Der Snapshot trägt `composing`.
- Bestehende Tests angepasst:
  - „Befehl Anlegen“ öffnet vor der zweiten Eingabe wieder über „+ Neu“.
  - „snapshot/restored“ prüft die offene Eingabe jetzt im Anlegemodus und die Auswahl in einem zweiten Remount. Vorher füllte der Test das Formular über einem gewählten Datensatz, also genau den Mangel.

## 5. interviews: „01.01.70“

Im v4-Stand bestätigt: Alle Fixture-Termine zeigten „01.01.70, 01:00“ (Bild `r2-base/interviews/1280-01-open.png`).

- Neu ist `toEpochMs`, exportiert, nach dem Muster von knowledge aus FIX8.
  - Werte unter 1e11 gelten als Sekunden und werden umgerechnet.
  - Text läuft über Date.parse.
  - 0, Ungültiges und alles vor 2000 zählt als „kein Datum“.
- Genutzt wird es in `fmtTime`, `fmtTimeShort`, `toLocalInput` und `durationMinutes`. Vorher ergab ein ungültiger Start eine Dauer von „0 Min“ oder Riesenwerte.
- Fehlt ein Datum, steht nichts da: kein Ersatztext, Schlüssel `withoutSchedule` entfernt.

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9`)

| Prüfung | Ergebnis |
|---|---|
| `node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/<app>/tests/*.test.mjs` | mail 42/43 (1 skip, MAIL_SHOTS), invoices 95/95, news 20/21 (1 skip, NEWS_SHOTS), placements 22/22, submissions 22/22, interviews 23/23 |
| `node v4/tools/guards/textdiaet-rule.mjs --app <6 Apps>` | 4 Treffer, alle unverändert (invoice-validate.js 3× Handler-Text, news `notify` 1×), unter der Baseline |
| `check-raw-ids.mjs --app <6 Apps>` bei 1280 und 420 | alle 6 Apps 0, Exit 0 |
| `v4/tools/geometry/contrast.mjs --app <6 Apps>` bei 1280 und 420 | 12/12 Läufe hell/dunkel OK, 0 zu schwach |
| `node v4/debt/ratchet.mjs` | OK (4 Zähler gesunken, nicht festgeschrieben, Sache des Baseline-Besitzers) |
| `rundgang2.mjs --app <6 Apps> --widths 1280,420` | 12× OK, 0 abgelehnt, 0 Fehler. 1 Geometrie: invoices 420 `ctox-table-wrap 547>380`, schon auf HEAD vorhanden (Positionstabelle scrollt seitlich) |

Bilder angesehen:

- rundgang2 lief unter `$SCRATCH/r2` (HEAD-Stand unter `$SCRATCH/r2-base`).
- Zusätzlich lief ein gezielter Bedienlauf im echten v4-Rahmen. Dafür diente eine Scratch-Kopie von rundgang2 mit Ablauf-Haken; das Werkzeug selbst ist unverändert. Bilder unter `$SCRATCH/flow/<app>/`.
- Gesehen bei 1280 und 420:
  - mail: Schublade unter der Schutzzone. „Entwurf behalten?“ mit Speichern/Verwerfen/Abbrechen. Esc im Blatt lässt die Schublade offen, Verwerfen schließt.
  - invoices: leerer Editor mit „Abbrechen | Entwurf speichern | Buchen“. „Keine Treffer“ + „Filter zurücksetzen“. Listenansicht mit vollem Status.
  - news: laufende Ausgabe „Do 01.01.“ mit Punkt, Feed zeigt „Mi 14.10.“, nur „Alle(1)“.
  - placements und submissions: Datensatz ohne Formular, „+ Neu“ leer mit Abbrechen.
  - interviews: kein Datum statt „01.01.70“.

`$SCRATCH` = `/private/tmp/claude-501/-Users-michaelwelsch-Documents-ctox/d0121889-1808-4958-a44a-06a489b1241e/scratchpad`

### Tests und Mutationsbelege

Jeweils so geprüft: die App-Dateien durch den HEAD-Stand ersetzt (`git show`, nur lesend), dann FIX9a-Test plus Mount-Test gefahren, danach zurückgestellt und per `cmp` bestätigt.

| Mangel | Test | Ohne Fix |
|---|---|---|
| 1 mail (CSS) | `modules/mail/tests/v4-api.test.mjs` „FIX9a: Verfassen-Schublade …“ (echtes Rahmen-Blatt `withWindowSheet`) | „Schließen-Knopf liegt in der Schutzzone bei 1280: [1230,26,1256,52]“ |
| 1 mail (JS) | dito | „Esc schließt die leere Schublade“ actual true |
| 2 invoices Neu | `modules/invoices/tests/v4-api.test.mjs` „FIX9a: „Neue Rechnung“ öffnet einen leeren Editor …“ | „Öffnen legt nichts an“: 1 Befehl statt 0 |
| 2 invoices Suche | „FIX9a: Suche ohne Treffer …“ | Liste leer statt /Keine Treffer/ |
| 3 news | `modules/news/tests/v4-api.test.mjs` „FIX9a: laufende Ausgabe …“ | JS+CSS alt: Feed leer. Nur CSS alt: Test rot (Trend-Spalte). Nur `hideEmptyCategoryTabs` entfernt: Test rot (Reiter) |
| 4 placements | `modules/placements/tests/v4-api.test.mjs` „FIX9a: Detail zeigt den Datensatz …“ | „kein Anlegeformular über dem gewählten Datensatz“ actual false |
| 4 submissions | `modules/submissions/tests/v4-api.test.mjs` „FIX9a: Detail zeigt den Datensatz …“ | dito |
| 5 interviews | `modules/interviews/tests/v4-api.test.mjs` „FIX9a: leere oder unplausible Daten …“ | „Nora Null … 01.01.70, 01:00 · 0 Min …“ |

Ohne Mutationsbeleg:

- **invoices Status:** Im v4-Stand nicht falsch, siehe oben.
- **„Buchen“ deutsch:** Auf HEAD schon richtig. Nur Englisch geändert, ohne eigenen Test.

## Hinweise für andere Stränge

- **KIT2/Rahmen:** Unter 600 px bleibt die Kit-Ansicht nach „Abbrechen“ einer neuen Rechnung auf dem nun leeren Detail stehen. „‹ Zurück“ führt zur Liste, es ist also keine Sackgasse. Dasselbe Verhalten gab es schon bei „Details schließen“. Die Ansicht wählt `pane-narrow.js`, nicht die App.
- **mail, bestehend:** Die Mail-Einstellungen bleiben im Rundgang offen über der Verfassen-Schublade liegen (gleicher z-index). Das ist nicht Teil von FIX9a. Esc für die Einstellungen wäre ein kleiner Folgeschritt.
- **Geänderte Dateien unter `modules/*/tests/shots/*.png`:** Sie stammen von den bestehenden Screenshot-Tests, die bei jedem Lauf neu schreiben.
