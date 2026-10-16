# BUCH – Fibu-Agent-Attrappe durch den Standard-Kontext-Chat ersetzen (16.10.2026)

Auftrag: TEXT-E offene Frage 1 (`v4/tools/textdiaet/TEXT-E.md`). Der Reiter „Agentenprotokoll“ in der rechten
Spalte antwortete mit fest verdrahteten Texten (Hetzner-Rechnung HET-2026-98127, Konten 4930/1576/1600 mit festen
Beträgen 100,00/19,00/119,00 €, „Bankabgleich abgeschlossen.“, „Bilanz aktualisiert.“) nach 1 s `setTimeout` und
einem simulierten „Analysiere Anfrage...“.

## Schritt 1 – Bestand (gemessen vor der Änderung)

Attrappe:
- `index.html:456` Reiter `data-right-tab="agent"` „Agentenprotokoll“; `:471-481` Teilfläche
  `data-right-subpanel="agent"` mit Verlauf `[data-chat-messages]`, Eingabe `[data-chat-input]`
  („Fibu-Agent fragen...“) und Knopf `send-chat-msg`.
- `index.js:3207-3260` `handleSendAgentMsg` + `appendChatMessage`: Schlüsselwort-Weiche auf
  rechnung/hetzner/telekom, abgleich/bank, bilanz/hgb; feste Antworten, simuliertes Tippen (`setTimeout 1000`),
  danach `switchView(...)` bzw. ein echter `triggerAutoReconcile()` mit erfundener Erfolgsmeldung.
- `index.js:461-471`, `:780-786`, `:827-831`, `:933-940`, `activeRightTab` (Zustand + Schnappschuss, nie
  wiederhergestellt angewendet), `index.css` Chat-Blasen (`.fibu-agent-chat-*`, `.chat-message`, `.message-bubble`)
  und das Reiterband `.fibu-evidence-tabs`.
- Kein Befehl, keine Collection: die Attrappe schrieb nichts, außer im Bank-Zweig den echten Abgleich auszulösen.

Muster (Kontext-Chat, Commit `0a544610f`, `shared/context-chat.js`):
- Alle Apps, die ihn nutzen (documents, spreadsheets, knowledge, research, desktop, buchhaltung), bieten ihn
  im **Rechtsklick-Kontextmenü** an (Formular mit Modus data/ask/app + Textfeld) und senden über
  `sendContextChat` den Standardbefehl `ctox.chat.task` bzw. `ctox.app.modify.request`.
- Die Antwort hat **keinen Weg zurück in die Quell-App**: der Handler `modules/ctox/handlers/ctox.chat.task.js`
  (`_lib/context-chat.js`) legt den Turn in `business_chats` der ctox-App ab; documents/spreadsheets/desktop
  springen danach in die ctox-App, buchhaltung bewusst nicht (`V4-HANDLERS.md`: „ohne Sprung wie vorher“).
- buchhaltung enthält ein eigenes Kontextmenü `initBuchhaltungContextMenu` → `renderBuchhaltungContextMenu` →
  `dispatchBuchhaltungContextChat` → `sendContextChat` (Korrektur nach Messung: toter Code, siehe Befund unten).

Abnahme vorher: ratchet OK (guard 1838 · scan 3), Vertrag 38/38, Design-System OK, Textdiät buchhaltung 0,
App-Tests 21/21.

Befund beim Lesen: Das app-eigene Kontextmenü (`initBuchhaltungContextMenu`, `renderBuchhaltungContextMenu`,
`dispatchBuchhaltungContextChat`) wird **nirgends aufgerufen** und registriert keinen `contextmenu`-Listener (toter
Code; documents/spreadsheets genauso). In v4 liegt der Standardort im Rahmen: `v4/shell/app-host.js`
`bindContextMenu()` – Rechtsklick (bzw. Shift+F10) im Fenster → Menü „CTOX fragen…“ → Eingabeblatt →
`sendContextChat` → `ctox.chat.task` unter der App-Identität, Bezug = nächstes Element mit
`data-context-record-id/-type/-label`. Ein app-eigener `contextmenu`-Handler ist außerdem durch
`scripts/assert-app-platform-freeze.mjs` verboten. buchhaltung trägt diese Attribute bereits an allen
Datensatzzeilen (Buchungsliste, Konten, Journal, Belege, Bankzeilen, Anlagen, Reisen, Fahrten).

## Schritt 2 – Entscheidung: Reiter entfällt ganz

Nach Entfernen der festen Antworten, der Tipp-Simulation und der erfundenen Rechnung bleibt im Reiter nichts
Echtes: Es gibt keine Collection „Agentenprotokoll“, und das Muster hat **keinen** Antwortweg in die Quell-App (die
Antwort steht im Chat der ctox-App, `business_chats`). Eine eigene Antwortanzeige wäre erfunden. Deshalb:

- Reiter „Agentenprotokoll“, das ganze Reiterband (mit nur „Dokument & Extraktion“ übrig wäre es ein Ein-Reiter-
  Band) und die Chat-Teilfläche aus `index.html` entfernt; Spalte heißt jetzt nur „OCR-Evidenz“.
- `index.js`: `handleSendAgentMsg`, `appendChatMessage`, `switchRightSubpanel`, Reiter-/Chat-Elemente,
  Listener, `activeRightTab` (Zustand und Schnappschuss) und `[data-chat-input]` im Eingabe-Schnappschuss entfernt.
- `index.css`: Chat-Blasen (`.fibu-agent-chat-*`, `.chat-message`, `.message-bubble`) und `.fibu-evidence-tabs`.
- Kontext-Chat sitzt am Standardort des Musters (Rahmen-Kontextmenü „CTOX fragen…“); keine Änderung nötig, weil die
  Datensatz-Attribute schon da sind. Nach dem Senden springt der Rahmen in die ctox-App, wenn sie aktiviert ist.

Nicht verändert (bewusst): der tote eigene Kontextmenü-Code (siehe offene Fragen), Befehle AfA-Lauf/Anlagegut
(`business_os.data.modify`, echte Befehle).

## Schritt 3 – Tests

- `tests/v4-api.test.mjs`: Schnappschuss-Test nahm die Chat-Eingabe als „offene Eingabe“; jetzt das Suchfeld
  `[data-pg-search]` (gleiche Aussage).
- Neu `tests/kontext-chat.test.mjs` (7 Tests):
  1. statisch: 13 Merkmale der Attrappe (Antworttexte, `Rechnung HET-2026-98127)`, `Analysiere Anfrage`,
     `handleSendAgentMsg`, Reiter, Chat-Selektoren, Chat-CSS) in index.js/html/css/locales = 0;
  2. Gegenprobe: derselbe Detektor erkennt in den Originalzeilen der Attrappe ≥ 10 Merkmale;
  3. statisch: ≥ 8 Zeilenvorlagen mit `data-context-record-id/-type`;
  4. echter v4-Rahmen (Aufbau wie `v4/tools/rundgang/rundgang2.mjs`, Labor-Helfer `geometry-v4.mjs`): App über
     die Desktop-Kachel mit echter Maus öffnen, maximieren, Buchung anklicken → kein Reiter „Agentenprotokoll“, keine
     Chat-Elemente, Evidenzspalte sichtbar;
  5. Rechtsklick (echte Maus) auf die Buchung → Menü endet mit „CTOX fragen…“ → Klick → Text tippen → „Senden“
     → genau ein `ctox.chat.task` mit `source_app:'buchhaltung'`, `record_ref` = Datensatz, `context`
     {record_type, record_id, label}, `opts.record_id`; nach 1,3 s (länger als das frühere Tippen) ist der Text der
     App-Fläche unverändert, keine Antwortanzeige;
  6. Gegenprobe: Esc im Eingabeblatt → kein Befehl;
  7. Gegenprobe: Rechtsklick ohne Datensatz (Spaltentitel) → nur „CTOX fragen…“, Befehl mit leerem `record_ref`.
- Mutations-Gegenprobe: Reiter „Agentenprotokoll“ testweise wieder in `index.html` eingesetzt → Tests 1, 4, 5 rot
  (3 von 7); Datei danach zurückgesetzt (0 Treffer „Agentenprotokoll“), erneuter Lauf 7/7.

Ergebnisse (Lauf `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test modules/buchhaltung/tests/*.mjs`):
28/28 grün (vorher 21/21, +7 neu).

Screenshots (selbst angesehen): `tests/shots/buch-evidenz-1280.png` (Evidenzspalte ohne Reiterband),
`buch-kontextmenue-1280.png` (Rahmenmenü „CTOX fragen…“ an der Buchung), `buch-frage-1280.png` (Eingabeblatt
„Zu „Angebot_Wartung_2026.pdf““), `buch-gesendet-1280.png` (nach dem Senden: App-Fläche unverändert).
`buchhaltung-1280.png`/`-420.png` vom bestehenden Screenshot-Test neu geschrieben.

## Schritt 4 – Abnahme

- `node v4/debt/ratchet.mjs`: OK (guard 1838 · scan 3, unverändert)
- `node scripts/assert-shell-v2-contract.mjs`: 38/38
- `node scripts/assert-design-system.mjs`: OK
- Textdiät-Wächter `--app modules/buchhaltung`: 0 Treffer (wie vorher); `--check` Exit 0
- Nebenbei: `scripts/assert-app-platform-freeze.mjs` ist rot wegen `modules/explorer/index.js` (fremd, nicht Teil
  dieses Auftrags, unverändert).

## Offene Fragen

1. **Hochladen von Belegen erfindet ebenfalls Daten** (`index.js` `handleBelegeUpload`, „Simulate OCR results“):
   jede hochgeladene Datei wird als Beleg „Hetzner Online GmbH“, Rechnung `HET-2026-98127`, 100,00 € netto /
   19,00 € USt / 119,00 € brutto, Datum 2026-05-22 angelegt (bzw. „Deutsche Telekom AG“, `TEL-7816-12`, wenn der
   Dateiname „telekom“/„phone“ enthält) – in echte Bücher (`accounting_receipts`). Das ist dieselbe Art Fälschung wie
   der Chat, liegt aber außerhalb des Chat-Auftrags; einen echten OCR-Befehl gibt es in buchhaltung nicht.
   Vorschlag: eigener Auftrag – Beleg nur mit Dateiname/Status anlegen und einen echten Extraktionsbefehl senden.
2. **Toter Kontextmenü-Code** in buchhaltung (`initBuchhaltungContextMenu` … `dispatchBuchhaltungContextChat`,
   ca. 200 Zeilen, plus `tests/buchhaltung.test.mjs` „context actions clamp locally“, das ihn statisch prüft):
   nie aufgerufen, darf wegen des Freeze-Wächters auch nicht angeschlossen werden. Löschen? (Gleiches Muster in
   documents, spreadsheets, knowledge, research.)
3. Nach dem Senden springt der Rahmen in die ctox-App (nur, wenn sie aktiviert ist). `V4-HANDLERS.md` beschreibt für
   den alten app-eigenen Weg „ohne Sprung“; der Rahmen entscheidet das jetzt einheitlich für alle Apps.

---

# BUCH-2 – Beleg-Upload ohne erfundene Buchhaltungsdaten (16.10.2026, Folgeauftrag zu offener Frage 1)

## Schema (gemessen)
`collections.schema.json` und `v4/schemas/buchhaltung.collections.schema.json` (identisch) für `accounting_receipts`:
Pflicht nur `id`, `file_storage_url`, `filename`, `status`, `updated_at_ms`; `additionalProperties: true`.
Lieferant, Rechnungsnummer, Datum, Beträge, Konto sind optional, aber als `string`/`number` typisiert. `null` passt
nicht zum Typ, deshalb werden sie **weggelassen**, nicht mit `null` geschrieben. Kein Konflikt mit dem Schema; das
Schema ist unverändert.

## Was entfernt wurde
- `handleBelegeUpload`: „Simulate OCR results“. Jede Datei wurde als Hetzner Online GmbH, Rechnung HET-2026-98127,
  netto 10000 / Steuer 1900 / brutto 11900 Cent, Datum 2026-05-22, Konto 4930/6815 angelegt; bei „telekom“/„phone“
  im Dateinamen als Deutsche Telekom AG, TEL-7816-12, Konto 4600/6600. Dazu ein vorgetäuschter Archivpfad
  `runtime/business-os/buchhaltung/storage/<name>`, obwohl die Datei nirgends abgelegt wurde.
- Evidenzspalte `renderOcrInvoicePreview`: eine nachgebildete Rechnung („Render Simulated OCR“) mit fester
  StNr/UStID DE814712499, Ersatznummer INV-98172, Ersatztitel EINGANGSRECHNUNG, erfundener Position „Dienstleistungen
  laut Leistungsbeschreibung“, fester Zeile „MwSt 19%“ und einer OCR-Konfidenz-Plakette. Jetzt zeigt sie eine
  Kit-Karte mit den gespeicherten Angaben (Dateityp, Größe, Hochgeladen, Lieferant, Rechnungsnummer, Belegdatum,
  Netto/Steuer/Brutto, jeweils „—“ wenn leer) und den Archivpfad. CSS der nachgebildeten Rechnung entfernt.
- OCR-Bezeichnungen ohne Erkennung: Ansicht „Belege & OCR“ → „Belege“, Kategorie „BELEGE & OCR“ → „BELEGE“,
  Spaltentitel und Umschalter „OCR-Evidenz“ → „Evidenz“ (index.html, index.js, locales `evidenceTitle`).
- Vorgetäuschte Werte in der Anzeige: Belegliste „Unbekannt“ → „—“, fehlende Beträge als „0,00 €“ und Steuer
  „0%/19%“ → „—“ (Steuersatz nur, wenn Netto und Steuer erfasst sind); Buchungsliste „Unbekannter Lieferant“ → leer,
  fehlender Betrag „—“ statt „0,00 €“.
- Buchungssatz-Vorschau: ohne erfasstes Konto wurde Aufwandskonto 4930 bzw. 6815 vorgeschlagen; jetzt bleibt die Zeile
  leer („—“). Sonderregeln (Bewirtung, Geschenk, Telefon) bleiben echte Fachregeln.

## Welche Felder ein neuer Beleg trägt
`id`, `filename` (Dateiname), `file_size` (Bytes), `mime_type` (Browser-Typ, sonst ''), `uploaded_at_ms`,
`status: 'draft'`, `updated_at_ms`, `file_storage_url: ''`.
- `status 'draft'` ist der vorhandene Zustand für einen offenen, noch nicht gebuchten Beleg (Anzeige „Offen“). Die
  App kennt nur `draft`/`posted`; ein neuer Wert „zu prüfen“ hätte eigene Anzeige und Filter gebraucht.
- `file_storage_url` ist Pflicht und bleibt leer, weil buchhaltung die Datei heute nicht ablegt (kein `api.files.put`).

## Zusätzlich: Buchen nur mit erfassten Angaben
`postReceiptDirectly` und `postReceiptWithAdvancedOptions` hätten aus einem leeren Beleg eine festgeschriebene
Buchung über 0 € ohne Datum gemacht. Jetzt prüft `receiptIsComplete` (Datum, Konto, Netto, Steuer, Brutto); fehlt
etwas, wird nichts gebucht und der Grund steht in der Zone (`buchhaltung.receipt.incomplete`).

## Tests
Neu `tests/beleg-upload.test.mjs` (4 Tests, Stub-Kern in Chromium wie `v4-api.test.mjs`):
1. Upload mit echter Maus (Klick auf die Ablagefläche → Dateiauswahl → PDF): genau ein neuer Beleg mit Dateiname,
   Größe = Bytezahl, MIME `application/pdf`, Upload-Zeit im Testfenster, Status `draft`, Archivpfad leer; kein Feld
   enthält Hetzner/HET-2026-98127/Telekom/TEL-7816/DE814712499/INV-98172, kein Zahlenfeld 10000/1900/11900;
   Lieferant, Rechnungsnummer, Datum, Beträge, Konto, USt-ID, OCR-Felder leer.
2. Dateiname `telekom_rechnung.pdf`: kein erfundener Lieferant.
3. Anzeige nach Klick auf den Beleg: kein Fantasiewert und kein „OCR“ im Fenster, Belegzeile ohne 100,00/19,00/119,00
   und „Unbekannt“, Evidenz zeigt `application/pdf`, Kontierungsfelder Lieferant/Rechnungsnummer/Belegdatum leer,
   Buchungssatz ohne 4930/6815, ohne Betrag, ohne NaN (auch mit „Geschäftliche Bewirtung“ per Mausklick).
4. „Buchen“ auf dem leeren Beleg: keine Buchung, Zoneneintrag `buchhaltung.receipt.incomplete`, keine Seitenfehler.

Gegenprobe mit dem alten Code (Stand vor BUCH-2, vor der Änderung nach `scratchpad/buch2-old/` kopiert;
`BUCH_OLD_DIR=<ordner>` liefert dessen index.js/html/css aus): **0/4 grün, 4 rot** (u. a. „supplier_name trägt einen
Fantasiewert“, Anzeige trifft auf „Hetzner…“). Neuer Code: 4/4.

Alle App-Tests: 32/32 (vorher 28; +4).

Screenshot (selbst angesehen): `tests/shots/buch2-upload-1280.png`. Zwei hochgeladene Belege mit „—“ in Lieferant,
Datum, Netto, USt und Brutto; Evidenz mit Dateityp/Größe/Upload-Zeit; Belegkontierung mit leerem Lieferant,
Rechnungsnummer und Belegdatum; Buchungssatz ohne Konto und Betrag.

## Abnahme
- `node v4/debt/ratchet.mjs`: OK
- `node scripts/assert-shell-v2-contract.mjs`: 38/38
- `node scripts/assert-design-system.mjs`: OK
- Textdiät buchhaltung: 0 Treffer; `--check` Exit 0

## Offene Fragen (BUCH-2)
1. **Die Belegkontierung lässt sich nicht ausfüllen:** Lieferant, Rechnungsnummer und Belegdatum sind `readonly`, und
   Felder für Netto/Steuer/Brutto und Konto gibt es im Editor nicht. Ein hochgeladener Beleg bleibt damit leer und
   (wegen der neuen Sperre) unbuchbar, bis jemand die Felder erfassen kann. Für den Arbeitsablauf ist das nötig:
   ein eigener Auftrag „Belegfelder editierbar, Speichern per api.patch“.
2. **Die Datei selbst wird nicht abgelegt.** Für GoBD bräuchte es `api.files.put` (wie cv-print-builder/app-store)
   und die Datei-Referenz in `file_storage_url`. Danach ließe sich die Datei in der Evidenz zeigen.
3. Bestehende Belege, die die alte Attrappe schon in echte Bücher geschrieben hat (Hetzner/Telekom mit 100/19/119 €),
   bleiben in den Daten der Kunden. Bereinigen, z. B. als Liste zur Prüfung?
