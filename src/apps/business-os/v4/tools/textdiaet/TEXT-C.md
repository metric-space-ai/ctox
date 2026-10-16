# TEXT-C – Text- und Gestaltungsdiät, Strang C

Apps: intake, interviews, placements, submissions, matching, cv-print-builder, nachweise, consent, esign,
buchhaltung, invoices, shiftflow. Rahmen: `~/.local/state/shell-v4/briefs/text-diet-common.md` (Regeln 1–8).
Pfade relativ zu `src/apps/business-os/modules/`. Zeilennummern = Stand vor der Änderung.

Screenshots (Rundgang-Labor `v4/tools/rundgang/rundgang.mjs --max 1`, Stub-Pin 68b83a3):
vorher `v4/tools/textdiaet/shots/TEXT-C-before/<app>/`, nachher `v4/tools/textdiaet/shots/TEXT-C-after/<app>/`
(je `1280-*.jpg`, `420-*.jpg`, `sheet-<w>.png`).

## Sofort-Befunde aus G-standard-audit-C (UI)

| App | Befund | Behebung | Nachweis |
|---|---|---|---|
| consent | Art.-17-Löschung (`ats.subject.erase`) ohne Bestätigung (`index.js:545`, Detail-Icon `:202`, Formular `:600`) | `dispatchSubject` fragt vor jeder Löschung über `api.shell.dialog({kind:'confirm', danger:true, default:false})`, Text nennt die Person und „nicht rückgängig“; ohne Ja kein Befehl | neuer Test `consent/tests/v4-api.test.mjs` „Art.-17-Löschung nur nach Bestätigung“: Stub-Antwort false → 0 Löschbefehle, Antwort true → genau 1 |
| buchhaltung | `seedMockDataIfEmpty()` schrieb beim ersten Öffnen Musterbuchungen (Stammkapital usw.) ins echte Journal (`index.js:318`, `:3283-3533`) | Aufruf und Funktion entfernt, dazu die nur dafür genutzten `ACCOUNTING_SEED_WRITE_COLLECTIONS`/`canWriteSeedData` | `buchhaltung/tests/v4-api.test.mjs` „leere Collections“: nach Mount auf leerem Kern 0 Journal-Einträge/-Zeilen, kein Mustertext |
| buchhaltung | `ASSETS_MOCK` (MacBook, Aeron-Stuhl, Server-Cluster) als Anlagenspiegel (`:128-132`, `:2035`, `:2073`) | ersetzt durch `assetRows()` aus `state.assets` (echte Anlagen; leer = leere Tabelle), Datum deutsch | dito (`/MacBook|Aeron|Server Cluster/` nicht im Text) |
| buchhaltung | OCR-Karte zeigt immer „99.4% Konfidenz“ (`:1567`) | Badge nur, wenn der Beleg `ocr_confidence` trägt, dann deutsch „99,4 %“ (`formatPercentDe`) | dito (`/99\.4/` nicht im Text) |
| matching | „Neue Anforderung anlegen“ als V2-Schicht `business-dialog-layer` (ungestylt im v4-Rahmen, `ui/index.js:9051-9130`) | `openCreateRequirementForm` fragt über `api.shell.dialog` (Blatt im App-Fenster): Titel (Pflicht) → Quelle → Beschreibung (mehrzeilig); Abbrechen verwirft; `matchingPrompt` reicht `required`/`multiline` durch (`ui/v4Runtime.js`) | `matching/tests/v4-api.test.mjs` „Schreiben“: drei Dialog-Specs (prompt, „Neue Anforderung“, Titel/Quelle/Beschreibung), keine `.business-dialog-layer`, Anforderung landet per `api.put`; `matching/test.mjs` prüft statt `root.append(layer)` jetzt `matchingPrompt('Titel'` und Abwesenheit von `business-dialog-layer` |
| invoices | Menge in Promille („Menge (‰)“), Einzelpreis/Zahlung/Skonto in Cent (`index.js:52,59,1300-1302,1496-1497`) | Eingabe in Stück und Euro (Textfeld `inputmode=decimal`, „1,5“ und „1.234,56“ werden verstanden), Umrechnung intern über `unitsToMilli`/`euroToCents`; Spalten „Menge“, „Einzelpreis (€)“, „Betrag (€)“, „Skonto (€)“ | `invoices/tests/v4-api.test.mjs` Entwurf: Felder zeigen „2“ und „25,00“, Kopf ohne ‰/Cent, Befehl trägt weiter 2000/2500; Zahlung „50,00“ → `amount_cents` 5000; `ui-totals.test.mjs` neue Umrechnungsprobe |
| invoices | Beträge „119.00 EUR“, Datum ISO im Detail (`:374-377`, `:1355-1356`) | `formatCents` → „119,00 €“ (de-DE Währung), Detail-Datum TT.MM.JJJJ (`formatDateDe`) | Test prüft Summen „59,50 €“ |

Fachverhalten unverändert: Befehlsarten und Nutzlasten (Cent/Tausendstel) bleiben, nur die Eingabe/Anzeige ist umgerechnet.

## Entfernte Texte und Boxen je App

### consent
- `index.js:11/34` Untertitel „Generisches DSGVO-Einwilligungs-/Rechtsgrundlagen-Register mit Zweck und Löschfrist.“ (Tooltip am Kicker) und Kicker „Prüfung“ (`index.js:297-300`, `index.html:68`) – entfernt.
- `index.html:9` Kicker „ATS“ – entfernt.
- `index.js:199` Detailzeile „ID <c-1>“ (technische ID) – entfernt.
- `index.js:193`/`:164` Rechtsgrundlage Rohwert „consent“ → „Einwilligung (Art. 6 Abs. 1 a)“ (auch contract/legal_obligation/legitimate_interest).
- `index.js:779` Gültigkeit „∞“ → „unbefristet“.
- Feld „Subjekt“/Platzhalter „Subjekt-ID“ → „Person“/„Kandidaten-ID“; Platzhalter „Zweck (optional — leer = nur Existenz)“ → „optional“; „— (nur Existenzprüfung)“ → „—“.
- `index.html:100` Ergebnis-Kasten `ctox-callout` (grün/rot): Prüf- und Auskunftsergebnis jetzt neutrale Karte (`ctox-card consent-result`), Sperren/Blockaden als Fehler in die Zone (`consent.gate.blocked`).
- `dataLockedHint` (Zone) „… Gib business_consents im App Store frei …“ → „Einwilligungsdaten im App Store freigeben.“

### intake
- `index.js:16/34` Untertitel „Generischer Mehrkanal-Eingang: normalisierte Bewerbungen aus Karriereseite/Jobbörse/E-Mail/QR.“ und `index.html:77` `<p class="intake-sub">` – entfernt; `locales/{de,en}.json` Schlüssel `subtitle` entfernt.
- `index.html:70` Kicker „Bewerbungseingang“ über dem Titel – entfernt.
- `index.js:191-192` Detailzeilen „ID appl_…“ und „Dedupe <key>“ – entfernt.
- Datum im Detail ohne Sekunden (TT.MM.JJJJ, HH:MM).

### interviews
- `index.html:8` Kicker „Recruiting“ – entfernt (Status-Kicker im Arbeitsbereich bleibt: er zeigt den Terminstatus).
- „No-Show“ → „Nicht erschienen“ (`index.js:24,31`, `index.html:43`).

### placements
- `index.html:9` Kicker „ATS“, `:68` Kicker „NEUE VERMITTLUNG“/„VERMITTLUNG“ (doppelt zum Titel „Vermittlung anlegen“) – entfernt.
- `index.html:98` Fußzeile „Eintrag wählen oder neue Vermittlung anlegen.“ bzw. „Placement: pl_1“ (technische ID) – entfernt (samt `composerHint`).
- Feldlabels „Kandidat-ID“/„Kunden-Account-ID“ → „Kandidat“/„Kunde“.

### submissions
- `index.html:9` Kicker „VORSTELLUNGEN“, `:70` „NEUE VORSTELLUNG“/„VORSTELLUNG“ – entfernt.
- `index.html:95` Fußzeile „Eintrag wählen oder neue Vorstellung anlegen.“ / „Submission: sub_1“ – entfernt.
- Listentitel „Kandidaten“ → „Vorstellungen“; „Kandidat-ID“/„Kunden-Account-ID“ → „Kandidat“/„Kunde“.
- `locales/{de,en}.json` `subtitle` („Kandidaten-Vorstellung an Kunden mit Doppel-Vorstellungs- und Consent-Schutz.“) – entfernt.

### nachweise
- `index.js:23/47` Untertitel „Generischer Nachweis-/Zertifikats-Tresor: … Einsatz-Gate.“ und `index.html:79` `<p class="nachweise-sub">` – entfernt; `locales` `subtitle` entfernt.
- `index.html:10,69` Kicker „ATS“ (2×) – entfernt.
- `index.js:215` Detailzeile „ID cred_…“ – entfernt.
- Test `tests/v4-api.test.mjs:103` verlangte den Untertitel → prüft jetzt, dass `[data-ats-sub]` fehlt und weder „Tresor“ noch „ATS“ sichtbar sind.

### esign
- `index.html:84` Ergebnis-Kasten `ctox-callout` („Signatur erfasst. Anfrage: sr-1 … Status: partially_signed“) – entfernt; Ergebnis als Meldung in der Zone (`esign.result`, info), Sperre als Fehler (`esign.blocked`); Status deutsch („teilweise signiert“) statt Rohwert.
- `index.html:68` Kicker „NEUE ANFRAGE“/„ANFRAGE“, Listenkicker „E-SIGNATUR“ – entfernt.
- `index.html:88` Fußzeile „Eintrag wählen oder neue Anfrage anlegen.“ / „Anfrage: sr-1“ – entfernt.
- Label „Unterzeichner-IDs (Komma-getrennt)“ → „Unterzeichner“, Hinweis als Tooltip „Mehrere durch Komma trennen“.
- `locales/{de,en}.json` `subtitle` – entfernt.
- Tests: Ergebnis wird in der Zone geprüft (`/sr-new/`, `/teilweise signiert/`, nicht `/partially_signed/`), kein `.ctox-callout` im Inhalt.

### matching
- `index.html:6,157,174` Kicker „Matching“ (Wiederholung des App-Namens) – entfernt.
- `index.html:57`/`ui/index.js:5287` Kicker „Match Workbench“ ohne Auswahl – leer (Titel „Anforderung auswählen“ genügt); `aria-label` → „Treffer“.
- `index.html:97`/`ui/index.js:5289` Fußzeile „Anforderung auswählen“ (doppelt zum Titel) – leer.
- `index.html:118`/`ui/index.js:5378` Fußzeile „Match auswählen · Pane ist standardmäßig verborgen“ – leer; Kicker „Objekt-Nachweise“ → „Nachweise“.
- `index.html:203` Erklärsatz „Hier kannst du markieren, ob bereits ein persönlicher Kontakt besteht.“ im Kontaktstatus-Modal – entfernt.
- Anlegen-Dialog (`ui/index.js:9065`) Erklärsatz „Hinterlege Titel und Beschreibung. Eine zugehörige Quelle wird automatisch angelegt …“ – mit der V2-Schicht entfernt.

### cv-print-builder
- `index.js:2205` Kopfhinweis „PDF geladen. Als Nächstes Parsing starten.“ – entfernt.
- `index.js:2215-2216` „· Korrektur und Template prüfen.“ / „· Druckansicht freigegeben.“ – entfernt (Vorlagenname bleibt).
- `index.js:2225` Rückfall „Status prüfen.“ – entfernt.
- Begriffe: Ansicht „Print“ → „Druckansicht“, Aktion „Parsing starten“ → „Analysieren“, „Print freigeben“ → „Freigeben“ (Guard-Test `cv-print-builder.test.mjs:48` angepasst).

### buchhaltung
- Musterdaten, `ASSETS_MOCK`, „99.4% Konfidenz“ (s. oben).
- `index.html:7` Kicker „Finanzwesen“ über „Belege & Buchungen“ – entfernt (bei 420 schob er den Titel unter die eigenen Kopfknöpfe); `index.html:65` statischer Kicker „KONTENRAHMEN“ – leer.
- Datum deutsch (TT.MM.JJJJ) statt ISO in Liste, Journal, Detail, Bankabgleich, Belegtabelle (`index.js:1081,1163,1386,1468,2126,2155,2306,3620`).
- `index.js:902-909` Bereichstitel: „Journal & Hauptbuch (GoBD-Immutable Ledger)“ → „Journal & Hauptbuch“, „Eingangsbelege & Vorkontierungs-Inbox“ → „Eingangsbelege“, „Automatisierter SEPA Bankabgleich“ → „Bankabgleich“, „Dienstreisen & Verpflegungsmehraufwand (VMA)“ → „Dienstreisen“, „Fahrtenbuch & Kilometererstattung“ → „Fahrtenbuch“, „Bilanz, GuV & Umsatzsteuer-Voranmeldung“ → „Auswertungen“, „Anlagenspiegel & Abschreibungen (AfA)“ → „Anlagenspiegel“, „SKR03 Kontenrahmen-Explorer“ → „Kontenrahmen SKR03“; Kicker-Doppel („BUCHUNGEN“, „BELEGE & OCR“ …) leer.
- „Soll (Debit)“/„Haben (Credit)“/„Soll-Konto (Debit)“/„Haben-Konto (Credit)“ → ohne englische Klammer.
- `index.js:1565` „🤖 CTOX-OCR Metadaten“ → „Belegerkennung“.
- `index.js:2194,2266,2307` grüne Kästen „🛡️ GoBD-Festgeschrieben (Unveränderbar) – Dieser Belegeintrag ist steuerrechtlich gesperrt …“ und „Abgeglichen & Verbucht – Diese Banktransaktion wurde GoBD-konform …“ → Badge „Festgeschrieben“ bzw. „Abgeglichen & Verbucht“, Erklärsatz als Tooltip.
- `index.js:2242` „Steuerliche Sonderregeln & Steuerberater-Tricks:“ → „Steuerliche Sonderregeln“.
- `index.js:2319-2321` blauer Kasten „🤖 Automatischer Abgleichsvorschlag: Wir haben einen offenen Beleg mit exakt demselben Betrag … gefunden:“ → neutrale Karte „Passender Beleg · <Betrag>“ (Aktion „Abgleich bestätigen“ bleibt).

### invoices
- Einheiten/Beträge/Datum (s. oben).
- Kicker „CTOX“ über „Rechnungen“ (`index.html:28`, `index.js:41`) – entfernt; Arbeitsbereich ohne Auswahl „RECHNUNGEN · Rechnung“ (`index.html:79`, `index.js:1079`) → nur Titel.
- `index.js:1501` Fußsatz „Skonto wird nur abgezogen, wenn das Zahlungsdatum vor dem Skonto-Deadline liegt. Das berechnet die lokale Zahlungslogik.“ → Tooltip am Feld „Skonto (€)“ („… vor Ablauf der Skontofrist gezahlt wird.“).
- `index.js:1538` „Diese Rechnung ist überfällig. Starte einen Mahnlauf, um einen Brief zu erzeugen.“ – entfernt (Knopf „Mahnlauf für diese Rechnung“ genügt).
- `index.js:1278` roter Kasten mit Validierungsfehlern im Editor – entfernt; die Gründe stehen im Tooltip des gesperrten „Buchen“-Knopfs.
- „Buchen (GoBD-post)“ → „Buchen“; „Dunning ist nur für …“ → „Mahnen ist nur bei überfälligen Rechnungen möglich.“; Zonenmeldung „Kein Kunde im CRM hinterlegt. Lege zuerst einen Kunden im „customers“-Modul an …“ → „Zuerst einen Kunden in „Kunden“ anlegen.“

### shiftflow
- `index.js:1434` Alert „Bitte erstelle zuerst ein aktives Projekt in der linken Spalte!“ → Zonenwarnung „Zuerst ein aktives Projekt anlegen.“
- `index.js:2926` Alert „Es sind keine freigegebenen Stunden im ausgewählten Zeitraum vorhanden, die abgerechnet werden können.“ → Zonenwarnung „Keine freigegebenen Stunden im Zeitraum.“
- `index.js:2984` Alert „Rechnungsentwurf erfolgreich erstellt und heruntergeladen.\n\nDu kannst diese Datei direkt im CTOX Rechnungs-Modul einlesen.“ → Zonenmeldung „Rechnungsentwurf heruntergeladen.“
- `index.js:2031` Hilfesatz `shiftflow-inspector-hint` „Klicke auf einen Tag, um den Mitarbeiter direkt für das Projekt einzuteilen (8:00 - 16:00 Uhr):“ → Tooltip an der Tagesleiste.
- `index.js:1929` Schichtnotiz im `ctox-callout` → schlichter Absatz.
- `locales/{de,en}.json`: Begrüßung „Hallo! Ich bin dein CTOX Planungs-Assistent …“ → „Wobei kann ich helfen?“, „Ich habe die Konfliktanalyse durchgeführt … Perfekte Dienstplanung.“ → „Keine Konflikte gefunden.“

## Regel 8 (reservierte Ecken, Owner 15.10. 20:36)

Geprüft in den Nachher-Shots 1280 und 420 aller 12 Apps (`TEXT-C-after/<app>/{1280,420}-01.jpg`): Icon oben links und
Knopfcluster (App/Code/□/×) oben rechts liegen über keinem Bedienelement der Apps; die Pane-Köpfe weichen über die
Rahmenregel aus (`.ctox-pane-header[data-v4-under-controls]` → `--shell-controls-inset`, FR6a/pane-heads.js), z. B.
consent-Rechte-Umschalter (1280) und Kopfknöpfe von consent/invoices/buchhaltung (420) stehen links vom Cluster. Eigene
App-CSS mit festen Pixeln war nicht nötig. Einziger Befund war buchhaltung 420: der umbrechende Titel lief unter die
eigenen Kopfknöpfe – behoben durch Entfernen des Kickers „Finanzwesen“ (Nachher-Shot neu aufgenommen).

## Prüfungen

- Alle Testdateien der 12 Apps grün: 34 Dateien, 0 Fehler (`CTOX_APP_API_STUB=…/68b83a3 node --import ./src/apps/business-os/v4/tools/node/ctox-shared.mjs --test modules/<app>/tests/*.test.mjs`, matching zusätzlich `test.mjs`). Neu bzw. verschärft: consent Art.-17-Bestätigung, buchhaltung kein Seed/keine Musterwerte, matching Anlegen über `api.shell.dialog`, invoices Stück/Euro-Eingabe und „€“-Anzeige, esign/nachweise Texte.
- Wächter `v4/tools/guards/textdiaet-rule.mjs --check --app <12 Apps>`: Exit 0, 55 Treffer (infobox 23, help-text 32), in 12 Datei/Regel-Paaren unter der Baseline. Die Baseline (`textdiaet-baseline.json`, Besitz TEXT-A) habe ich nicht gesenkt; mit `--write-baseline --app <12 Apps>` lässt sie sich nachziehen.
- Restliche Treffer überwiegend Agenten-Prompts (matching `ctoxCommandAdapter.js`, cv-print-builder `index.js:1958-1976`), Validierungsmeldungen (`invoices/core/invoice-validate.js`) und Datenkarten der Auswertungen (`buchhaltung/index.html:264-311` UStVA/GuV/DATEV als `ctox-callout` gestylt) – nicht als Hinweisboxen angefasst; shiftflow-Konfliktliste (`index.js:2900`, `ctox-callout is-danger` je Konflikt) ist Fachinhalt.

## Offen (nicht in diesem Strang)

- ATS-Vorlage „Auswahl befüllt Anlegeformular“ (A3/K5), Namen statt IDs (K4), Tastatur/Kontextmenü (K1/K2) – Struktur, nicht Textdiät.
- buchhaltung: 20 Inline-`onclick`, 21 `fibuNotice`, Navigation per Dropdown; matching: Begriffe „Matches/Matchen/Objekte“ im Inneren.
- invoices „+“ legt weiterhin sofort einen Entwurf für den ersten Kunden an (Audit 3.2, Fachablauf).

---

# Runde 2 (Koordinator 15.10., 45 min)

## 1. Namen statt IDs (alle 12 Apps)

- **invoices:** `partyName()` löst über `customer_accounts` auf (`name`, sonst `payload.name`/`payload.legal_name`), ohne Treffer „Unbekannt“ (vorher Rückfall auf `party_id`, z. B. „party-001“). Adresse eines Kunden wird als Zeile formatiert (vorher „[object Object]“), Menge „1“ statt „1.000“. Test: Rechnung mit unbekanntem Kunden zeigt „Unbekannt“, nie `party-404`.
- **ATS-Apps (placements, submissions, interviews, intake, nachweise, consent):** neuer Helfer `refName(r, basis)` je App. Er zeigt `<basis>_name`, `<basis>.name` oder `<basis>_title` aus dem Datensatz, sonst „Unbekannt“, nie die ID. Ersetzt wurden die Anzeigen von `candidate_id`, `client_account_id`, `client_contact_id`, `vacancy_id` und `subject_id` in Listen, Titeln, Details und Kontext-Labels. Aus submissions sind außerdem die Detailzeilen „Vorstellung: sub_1“ und „Consent: …“ sowie das Icon „ID kopieren“ entfernt. Bei interviews fällt `isc_…` aus der Scorecard-Zeile weg. In nachweise heißt „Subjekt“ jetzt „Person“, und submissions zeigt Zeiten ohne Sekunden.
- **esign:** Der Titel ist `document_title`, sonst der Vertragstyp, nie die Dokument-ID. Anfrage-ID und Artefakt-ID sind aus dem Detail entfernt. Unterzeichner werden mit Namen angezeigt, sonst „Unbekannt“; ihr Status ist deutsch („ausstehend/unterschrieben/abgelehnt“) statt Rohwert.
- **matching, cv-print-builder, buchhaltung, shiftflow:** Der Prüfer findet keine Roh-IDs (siehe Liste unten).
- **OFFEN (Manifest, nicht UI):** Die ATS-Apps haben keine Lesefreigabe auf eine Namensquelle. Die Manifeste nennen nur die eigene Collection (`placements`, `submissions`, `interview_meetings`, …); Bewerbungen (`applications`, Kandidatenname) und `customer_accounts` (Kunde) sind nicht freigegeben. Deshalb steht heute überall „Unbekannt → Unbekannt“. Die echte Auflösung braucht eine Manifest-Freigabe (read) auf `applications` und `customer_accounts`, dazu im Kern einen Namen am Datensatz. Das ist eine Entscheidung des Manifest- bzw. Architektur-Besitzers. `refName` greift sofort, sobald ein Name ankommt.
- Tests: Seeds tragen jetzt Namen (`candidate_name`, `subject_name`, `vacancy_title`, `document_title`, Unterzeichner `name`). Die Text-Asserts prüfen den Namen; zusätzlich gilt: über das Formular angelegt (nur ID) → „Unbekannt“, ID nicht sichtbar. Die Befehls-Nutzlasten prüfen weiter die IDs (Fachverhalten unverändert).

## 2. Rechnungen: Listenkopf in Spaltenbreite

- Import/Export liegen jetzt im „…“-Menü (`api.shell.menu`), der Kopf hat nur „+“ und „…“. Die Spalte ist standardmäßig 344 px breit (vorher 320), damit „Überfällig(1)“ nicht über die Kante läuft. Nachweis: `invoices/tests/shots/invoices-1280.png` (alle vier Reiter und der Kopf vollständig).

## 3. Rechnungen: erster Eintrag gewählt, leerer Zustand

- Beim Öffnen wählt `autoSelectFirst()` einmal je Mount den ersten sichtbaren Eintrag (wie Outlook/lexoffice), sobald `accounting_invoices` geliefert hat. Eine wiederhergestellte Auswahl oder „Details schließen“ hat Vorrang.
- Ist die Liste geladen und leer, steht im Arbeitsbereich „Noch keine Rechnungen.“ mit dem Knopf „Neue Rechnung“. Während des Ladens bleibt es leer (kein Ladetext).
- Tests: Mount prüft „RE-2026-0001 gewählt“, der leere Kern prüft genau „Noch keine Rechnungen.“ + „Neue Rechnung“, keine Zone.

## 4. Fußzeile „N Einträge · Band“ entfernt (alle 12 Apps)

- `<footer class="ctox-pane-footer"><span data-pg-footer>` ist aus den `index.html` aller 12 Apps entfernt, in invoices auch aus dem JS-Rahmenbau. Die Schreiber sind null-sicher (`if (node)`). Struktur-Tests (`intake/interviews/placements/submissions/nachweise/consent/esign/cv-print-builder/buchhaltung/shiftflow`) prüfen jetzt die Abwesenheit von `data-pg-footer`.
- Behalten: die matching-Fußzeilen der Mittel-/Nachweisspalte („3 Kandidaten · Quelle“). Sie nennen die Quelle und sind keine reine Wiederholung des Reiterzählers.

## 5. Werkzeug `v4/tools/rundgang/check-raw-ids.mjs`

- Öffnet jede App im echten v4-Rahmen über den Labor-Server des Rundgangs (Launcher-Klick, Maximieren). Gelesen wird `innerText` im geöffneten Zustand und nach Klick auf den ersten Datensatz. Gemeldet werden: `slug-id` (`/\b[a-z]+…[-_]{1,2}\d{2,}…\b/`), UUID, langes Hex (≥ 16, gemischt), Hash-Kürzel (7–12 Hex, ≥ 2 Ziffern), `snake_case`-Rohwerte. Nicht gemeldet: Belegnummern in Großschrift, Datum, Beträge, Normen, reine Zahlen, Wörter aus Hex-Buchstaben.
- Aufruf: `CTOX_APP_API_STUB=… node v4/tools/rundgang/check-raw-ids.mjs [--app a,b] [--width 1280] [--json f] [--md f]`
- Test: `node --test v4/tools/rundgang/check-raw-ids.test.mjs` (3 Tests: 9 positive, 10 negative Beispiele, Eindeutigkeit/Umfeld) grün.
- Grenze: ID-Slugs ohne Ziffern (z. B. „cand-alice“) erkennt die Regel bewusst nicht (Brief-Muster `\d{2,}`). Kundenmodule (`customer-modules/*`) lädt das Labor ohne Konfiguration nicht und meldet sie als „nicht geprüft“.

### Lauf über alle Apps (15.10., 1280, Stub 68b83a3, Fixture-Stand während des SEED-Umbaus)

42 Apps mit v4-Manifest: 38 ohne Treffer, 3 mit Treffern, 1 nicht geprüft. Volle Liste: `v4/tools/textdiaet/raw-ids.md` und `raw-ids.json`.

| App | Treffer | Roh-IDs (Art: Wert – Umfeld) |
|---|---|---|
| outbound-lead-generation | 0 | nicht geprüft: keine Laborkonfiguration: ENOENT: no such file or directory, scandir '/Users/michaelwelsch/Documents/ctox-shell-v4/src/apps/business-os/modules/ou |
| reports | 3 | slug-id: `module-001` – Kundenportal Bugs Offen Hoch module-001 · 14.10.26, 05:30 Projektplan<br>slug-id: `module-002` – ung 2026 Bugs Erledigt Mittel module-002 · 13.10.26, 03:30 Lieferanten<br>slug-id: `module-003` – eitende Bugs Erledigt Niedrig module-003 · 12.10.26, 01:30 Onboarding |
| support | 3 | slug-id: `team-001` – ITÄT all ZUORDNUNG owner TEAM team-001 KUNDE account-001 TICKET case<br>slug-id: `account-001` – UNG owner TEAM team-001 KUNDE account-001 TICKET case-001 AKTUALISIERT<br>slug-id: `case-001` – -001 KUNDE account-001 TICKET case-001 AKTUALISIERT 13. Okt., 20:30 |
| tickets | 6 | slug-id: `item-001` – Geschlossen (0) Ticket Offen item-001 · ctox Ticket Offen item-002<br>slug-id: `item-002` – item-001 · ctox Ticket Offen item-002 · ctox Ticket Offen item-003<br>slug-id: `item-003` – item-002 · ctox Ticket Offen item-003 · ctox Ticket Offen item-004<br>slug-id: `item-004` – item-003 · ctox Ticket Offen item-004 · ctox Ticket Offen item-005<br>slug-id: `item-005` – item-004 · ctox Ticket Offen item-005 · ctox Ticket Offen item-006<br>slug-id: `item-006` – item-005 · ctox Ticket Offen item-006 · ctox Ticket Offen VERLAUF E |

Die Treffer liegen in Apps anderer Stränge (reports, support, tickets) und sind dort zu klären. Wahrscheinlich sind es Fixture-IDs (`module-001`, `team-001`, `item-001`), die die App roh anzeigt.

## Vermerke (nicht von mir behoben)

- **Fixture-Probleme (`v4/tools/geometry/fixtures.mjs`, gehört SEED, nicht angefasst):** Rechnungen verwiesen auf `party-001…003` ohne passenden `customer_accounts`-Eintrag; seit dem SEED-Umbau lösen sich die Kunden auf. Die ATS-Fixtures haben nur IDs, deshalb „Unbekannt“. nachweise hat `valid_until_ms: 0` („abgelaufen vor 20742 Tage“), submissions `created_at_ms` klein („01.01.1970“).
- **Rahmen-Labor zum Zeitpunkt der Nachher-Shots (21:05):** In `TEXT-C-after/{invoices,placements}/1280-01.jpg` überlappen Suchfeld und Reiter den Spaltentitel, und der Rechnungs-Arbeitsbereich ist dunkel. In der App-eigenen Testseite (nur `base.css`, `invoices/tests/shots/invoices-1280.png`) ist beides sauber. Das Muster tritt app-übergreifend auf und deutet auf den parallel laufenden FR6a-Umbau (`v4/frame`), nicht auf die App-Dateien. Nach Abschluss von FR6a neu aufnehmen.

## Prüfungen Runde 2

- Alle Testdateien der 12 Apps grün (34 Dateien + `shiftflow/test.mjs`, 0 Fehler).
- `check-raw-ids.test.mjs` 3/3 grün; Prüfer-Lauf über alle Apps: meine 12 Apps 0 Treffer.
- Textdiät-Wächter `--check` für die 12 Apps: Exit 0.
