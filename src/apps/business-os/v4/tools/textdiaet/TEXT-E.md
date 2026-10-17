# TEXT-E – Text- und Gestaltungsdiät, Strang E (Owner-Mängel M-13, M-15)

Rahmen: `~/.local/state/shell-v4/briefs/text-diet-common.md` (Regeln 1–8). Pfade relativ zu
`src/apps/business-os/`. Zeilennummern = Stand vor der Änderung.

Wächter: `node v4/tools/guards/textdiaet-rule.mjs --summary` (unverändert genutzt).
Screenshots: Rundgang-Labor `v4/tools/rundgang/rundgang2.mjs --max-steps 3`, Stub-Pin 637caf6, 1280 + 420:
vorher `v4/tools/textdiaet/shots/TEXT-E/before/<app>/`, nachher `v4/tools/textdiaet/shots/TEXT-E/after/<app>/`.
App-Tests mit dem in den Testdateien vorgeschriebenen Stub-Pin 68b83a3.
Unteransichten buchhaltung (Belegkontierung, GuV, UStVA, DATEV): `shots/TEXT-E/after-deep/buchhaltung/` (rundgang2
`--max-steps 28`) und `shots/TEXT-E/after-sub/buchhaltung/` (Reiter per Klick, Hilfsskript im Scratchpad).
Hinweis: Die Vorher-Bilder von coding-agents und shiftflow entstanden erst nach deren Änderung (Lauf parallel gestartet).

## Vorher (16.10.2026, 07:27)

```
Textdiaet-Waechter: 253 Treffer in 22 Dateien (249 geprueft) – help-text 225, infobox 28
     78  modules/outbound  help-text=78
     72  customer-modules/outbound-lead-generation  help-text=68 infobox=4
     44  modules/research  help-text=44
     35  modules/buchhaltung  infobox=22 help-text=13
      8  modules/matching  help-text=8
      6  modules/cv-print-builder  help-text=6
      3  modules/coding-agents  help-text=3
      3  modules/invoices  help-text=3
      3  modules/shiftflow  infobox=1 help-text=2
      1  modules/news  infobox=1
Ueber der Baseline (3): modules/coding-agents/{index.js,locales/de.json,locales/en.json} [help-text] 1 > 0
```

## Je App

### coding-agents — vorher 3 (Baseline-Verstoß), nachher 0
- `index.js:18`, `locales/de.json:5` Leertext „Hier erscheinen die Läufe des Coding Agents. Starte einen in der
  Code-Ansicht einer App („Code ansehen“ im App-Menü).“ → „Noch keine Läufe – Start über „Code ansehen“ im App-Menü.“
- `locales/en.json:5` entsprechend „No runs yet – start via “View code” in the app menu.“
- Test `tests/v4-api.test.mjs:172` prüft den neuen Satz samt Startweg („Code ansehen“), weiter ohne Box und ohne Zone.
- Tests 8/8 grün.

### shiftflow — vorher 3, nachher 0
- `index.js:2899` Konfliktliste: jede Zeile war eine rote Box (`ctox-callout is-danger`) → schlichte Listenzeile
  mit rotem „!“-Marker und Trennlinie (`index.css` `.shiftflow-conflict-item`). Inhalt und Kontextattribute unverändert.
- `locales/{de,en}.json:71` `aiGenerateSuccess` (229 Z., „Ich habe erfolgreich einen optimierten Dienstplanentwurf …“) –
  nirgends benutzt (die App meldet `aiGenerateSuccessPlain` in die Zone) → Schlüssel entfernt.
- Tests 9/9 grün.

### news — vorher 1, nachher 0
- `index.js:869` Fehlalarm: `code = 'notice'` ist der Zonen-Code von `notify()`, keine Klasse → `v4-allow: text`
  im Kommentar davor. Keine Textänderung.
- Tests (Pin 68b83a3) 8/8 grün. Mit Pin 637caf6 scheitert „Befehl: Ausgabe erzeugen … failed“ ('failed' !== 'done'),
  auch ohne diese Änderung (reiner Kommentar) – Stub-Unterschied, nicht TEXT-E.

### buchhaltung — vorher 35 (infobox 22, help-text 13), nachher 0
Infoboxen (alle entfernt, Felder und Aktionen unverändert):
- `index.html:262` GuV-Rahmen und `:276-296` sechs UStVA-Kennziffer-Kacheln `ctox-callout` → `ctox-card`
  (neutrale Fläche, Innenabstand in `index.css`); Hervorhebung Kz. 66 / Zahllast bleibt.
- `index.html:309` DATEV-Box `ctox-callout` samt Erklärsatz „Generieren Sie eine offizielle DATEV-kompatible Export-CSV …“
  → keine Box; Sachinfo als Tooltip am Bereich („DATEV-Format v7.0/8.0, enthält alle festgeschriebenen Buchungen“).
- `index.js:2224/2237` Beleg-Schublade: gelbe Box um „Zahlungsweg“ und „Steuerliche Sonderregeln“, gelbe Labels
  (`fibu-text-warning`) → schlichte Feldgruppen.
- `index.js:2862` Bank-Split: gelbe Box + „🥞 Bank-Transaktions-Splits (Aufteilungsbuchung):“ → Feldgruppe „Aufteilung“.
- `index.js:3387/3399` Reisekosten: Boxen um Zahlungsweg und Tagesliste, gelbes Label → schlicht.
- `index.js:3743` Fahrtenbuch: Box um Zahlungsweg, gelbes Label → schlicht.
- `index.js:3936` Firmenwagen: Box „Steuerberater-Buchungsvorschlag (inkl. 19% USt):“ → Überschrift „Buchungssatz (inkl. 19 % USt)“.
- `index.js:3339/3624` Beträge in Reisekosten-/Fahrtenbuchliste in Gelb (`fibu-text-warning`) → normale Zahl (fett).
- `index.css:957` Regel `.fibu-text-warning` entfernt (nicht mehr benutzt).
- `index.html:238` Bilanz-Kopf: statische Pille „Aktiv“ (`data-report-date-badge`, von keinem Code gesetzt) entfernt.

Texte:
- `index.html:468` Leertext „Wählen Sie eine Banktransaktion oder einen Beleg aus, um die PDF-Vorschau und OCR-Daten
  anzuzeigen.“ → „Beleg oder Buchung wählen.“
- `index.html:476-487` Begrüßungsblase des Fibu-Agenten (Vorstellung + Aufzählung + „Frag mich z.B. …“) entfernt;
  der Platzhalter „Fibu-Agent fragen...“ bleibt.
- Doppelte/technische Titel: „DATEV EXTF CSV-Buchungsstapel Export“ → „DATEV-Export“, Reiter „DATEV EXTF-Export“ →
  „DATEV-Export“, Knopf „DATEV CSV-Stapel herunterladen (EXTF)“ → „Exportieren“, „Startdatum:/Enddatum:“ → „Von/Bis“,
  „Umsatzsteuer-Voranmeldung (UStVA / ELSTER)“ → „Umsatzsteuer-Voranmeldung“, „Gewinn- und Verlustrechnung
  (Gesamtkostenverfahren § 275 HGB)“ und „Erstellung der Bilanz nach § 266 HGB“ → „Gewinn- und Verlustrechnung“ /
  „Bilanz“ (Paragraf als Tooltip).
- Beleg-Schublade: Checkbox-Erklärungen in Klammern als Tooltip („Geschäftliche Bewirtung“ ← „70/30 Netto-Split,
  100 % Vorsteuer abzugsfähig“; „Geschenk an Partner“ ← „Über 35 € automatisch nicht abzugsfähig“; „Telefon/Internet“ ←
  „20 % Privatanteil werden automatisch herausgerechnet“); „Zahlungsweg (Konto-Gegenbuchung)“ u. ä. → „Zahlungsweg“;
  „Vorgeschlagener GoBD-Buchungssatz (Live-Vorschau):“ → „Buchungssatz“; „Verpflegungsmehraufwand (VMA) Live-Berechnung:“
  → „Verpflegungsmehraufwand“.
- `index.js:1477` Belegliste: Rohwert-Pille `${status}` („posted“/„draft“, gelb) → „Gebucht“ (grün) / „Offen“ (neutral).
- Bestätigungen: Storno „Möchten Sie diese Buchung wirklich GoBD-konform stornieren? Es wird eine automatische
  Gegenbuchung (Storno) erzeugt.“ → „Es wird eine Gegenbuchung (Storno) erzeugt.“ (Titel „Buchung stornieren“ bleibt);
  Kontenrahmen neu initialisieren: gekürzt, Folgen bleiben genannt (Pflichtangabe vor destruktiver Aktion, `v4-allow`).
- Zone: „Der berechnete Verpflegungsmehraufwand beträgt 0,00 €. Es kann keine Buchung erzeugt werden.“ →
  „Verpflegungsmehraufwand 0,00 € – keine Buchung möglich.“
- Agenten-Antworten im Chat gekürzt (siehe offene Frage 1): „Vorkontierung: … Entwurf liegt in den Belegen.“,
  „Bankabgleich abgeschlossen.“, „Bilanz aktualisiert.“, „Nicht verstanden – z. B. „Bank abgleichen“ oder „Bilanz erstellen“.“
- `index.js:856/860` Agenten-Aufträge (Anlagegut erfassen, AfA-Lauf) sind kein Oberflächentext → `v4-allow: text`.
- Tests 21/21 grün, Vertrag 38/38, Design-System OK.

### outbound — vorher 78 (help-text), nachher 6
Die 78 Treffer waren bis auf wenige keine Oberflächentexte: 40 Briefing-Vorlagen (`CAMPAIGN_IDEA_TEMPLATES`, Text
landet editierbar im Briefing-Feld), Standardinhalte (`DEFAULT_ICP_CORE`, `DEFAULT_CHECKLIST`, `DEFAULT_CTA`,
Standardkampagnen-Ziel), Agenten-Prompts, Skillbook/Runbook und Writeback-Regeln. Diese Zeilen tragen jetzt
`// v4-allow: text (<Art>)`; 6 Treffer bleiben, weil sie mitten in mehrzeiligen Prompt-Vorlagen liegen
(`ICP_PROMPT_TEMPLATE` `:465/:467`, Mail-Prompt `:606/:662/:679`) – ein Kommentar dort würde den Prompt ändern.
Locale-Schlüssel der Agenten-Aufträge nach Konvention umbenannt (Wächter nimmt `…Prompt` aus, Funktion gleich):
`researchCompanyDataNew/Standard`, `researchMissingOrNew` → `…Prompt`, `importPromptLabel` → `importPrompt`.

Oberfläche (aus Bild `before/outbound/1280-01-open.png` und MAENGEL M-15):
- Kampagnenreiter „Alle(7) Aktiv(6) Planung(1) Beendet(0)“ → „Alle Aktiv Planung Beendet“ (Zählerspanne
  `data-pg-count` entfernt, `index.js:2763`).
- Kampagnenliste: „Aktiv · Keine Sequenz · 0 Leads“ → „Aktiv“; Sequenz nur, wenn eine existiert, Leads nur ab 1
  (`renderCampaignItem`, `:2852`).
- Firmenzeile: Feld-Captions „Recht: — / Mitarbeiter: — / Umsatz: —“ → nur vorhandene Werte („GmbH · 120 MA ·
  1.200.000 €“), Bedeutung als Tooltip; ohne Werte keine Zeile (`renderCompanyFacts`, beide Tabellenansichten `:3616/:3694`).
- „Versteckte Firmen 0“ → Zahl nur ab 1 (Pille leer = unsichtbar, `index.css` `:empty`).
- Kampagnen-Editor: Hilfesatz „Dieser Text ist die zentrale Arbeitsanweisung der Campaign und bleibt später jederzeit
  editierbar.“ entfernt; Platzhalter „Beschreibe in natürlicher Sprache, wen …“ → „Zielgruppe, Kanal, Angebot, Folgeaktionen“.
- Recherche-Einstellungen: Platzhalter „z.B. Belege immer mit URL ausgeben, nur öffentlich belegbare …“ →
  „z. B. Belege mit URL, keine Personen recherchieren“ (de/en).
- `locales/{de,en}.json` unbenutzte Schlüssel `settingsFooterHint` („Gilt für diese Kampagne …“) und
  `activeOutreachIntro` („CTOX bereitet Nachrichten … vor …“) entfernt.
- Tests: `tests/v4-api.test.mjs`, `outbound.test.mjs`, `refresh.test.mjs`, `focus-navigation.test.mjs` 41/41 grün.

### outbound-lead-generation (customer-modules; `module.json`/`collections.schema.json` unberührt) — vorher 72 (help-text 68, infobox 4), nachher 0
Infobox-Treffer: Es sind keine Boxen (kein Rahmen, kein Hintergrund), sondern Meldezeilen am Ort; Klassen
umbenannt: `leadgen-import-notice` → `leadgen-inline-msg` (Sellify-Kampagnensuche `:3097`, Zugangs-Dialog `:6027`,
Quellen-Dialog `:6510`), `leadgen-review-notice` → `leadgen-review-error` (letzter Recherchefehler des Leads `:12373`).
Agenten-Prompts (Recherche-Standardablauf `:508-530`, Altfassung Belegregel `:1217` – wortgleicher Abgleich, nicht
geändert –, Adapter-Auftrag `:7926-7933`, Feldhinweise `:9242-9247`, Übergabe-Prompt `:10011`, Sellify-Vorwissen
`:10413-10446`) → `v4-allow: text`, Inhalt unverändert.

Gekürzt (Aussage und Handlung bleiben, Datei:Zeile vorher):
- `:738` „Der Auftrag hat CTOX nicht erreicht. Der Lead bleibt offen. Bitte die Recherche erneut starten.“ → „Auftrag nicht bei CTOX angekommen – Lead bleibt offen, Recherche erneut starten.“
- `:1887` Feld löschen: „… Bereits recherchierte Werte bleiben an den Leads gespeichert, werden aber nicht mehr angezeigt.“ → „Feld „X“ löschen? Recherchierte Werte bleiben gespeichert, nur nicht sichtbar.“
- `:1906` „Der Rechercheablauf ist noch nicht geladen. Bitte kurz warten und erneut speichern.“ → „Rechercheablauf noch nicht geladen – gleich erneut speichern.“
- `:2125` Hilfezeile unter „Absender-Postfach“ → Tooltip am Feld.
- `:2330` „die Verbindung dieses Browsers zu CTOX war unterbrochen – bitte die Seite neu laden“ → „Verbindung zu CTOX unterbrochen – Seite neu laden“.
- `:2690` Tooltip „Noch offene Felder anhand der vollständigen Leads prüfen und recherchieren“ → „Offene Felder der vollständigen Leads recherchieren“.
- `:2958/:2960` Übergabezeile: „… Sellify-Sperrvermerke werden unmittelbar vor dem Schreiben erneut geprüft …“ → „Übergabe wird vorbereitet: Sperrvermerke werden geprüft …“; „Die letzte Übergabe wurde unterbrochen (Browser geschlossen). Erneut übergeben: …“ → „Letzte Übergabe unterbrochen – erneut übergeben, Angelegtes wird erkannt.“
- `:3824` Recherche zurücksetzen: → „Ergebnisse, Belege und Ansprechpartner von N Leads in „X“ werden gelöscht. Stammdaten bleiben.“
- `:4279` `research_error` „Der Kampagnen-Task ist beendet, bevor für diesen Lead ein Recherche-Command gestartet wurde …“ → „Kampagnenlauf endete vor dem Start dieser Recherche – Lead erneut einplanen.“ (Text wird nirgends verglichen).
- `:5963` Zugangs-Dialog: Erklärabsatz „E-Mail/Benutzername und Passwort werden als ein verschlüsseltes Credential im CTOX Secret Store gespeichert …“ → Tooltip am Passwortfeld.
- `:6320/:6324/:7764` „Die Quelle/Der Rechercheablauf wurde gespeichert, aber der Adapter-Abgleich konnte nicht gestartet werden: …“ → „Quelle gespeichert, Adapter-Abgleich nicht gestartet: …“ (bzw. „Rechercheablauf gespeichert, …“).
- `:6346` Testadresse, `:6439` „Diese Quelle nutzt automatisch die Unternehmensdomain …“ → „Nutzt die Domain des jeweiligen Leads.“, `:6518` „Neue Zugangsdaten oder „Zugang entfernen“ – nicht beides.“, `:6537/:6594` „Die Domain ist die stabile Quellen-ID …“ → „Domain nicht änderbar – für eine andere Domain neue Quelle anlegen.“, `:6745`, `:6792`, `:7331`, `:7721`, `:7762`.
- `:8490` Import-Hilfstext → „Excel, Text oder URL – Recherche erst nach der Sichtprüfung.“; `:8679`, `:8943`.
- `:9186/:9191/:9199` Sellify-Vorabgleich: „Sellify-Abgleich fehlgeschlagen, keine Recherche gestartet – erneut versuchen.“, „Kein Sellify-Treffer – Recherche startet ohne Vorwissen.“, „Firma nicht in Sellify (auch keine Namensvariante) – nur Neue Recherche möglich.“
- `:9899/:9905/:9911/:9985/:9998/:9999/:10007/:10102/:11612` Übergabe-Meldungen gekürzt (z. B. „Mindestens eine Person für die Übergabe auswählen.“, „Übergabe unterbrochen – erneut übergeben, nichts wird doppelt angelegt.“).
- guard-v4: kein neuer Vokabeltreffer (eine erste Fassung „Verbindung unterbrochen – …“ traf `de-verbindung`, umformuliert).
- Tests `tests/v4-api.test.mjs` 15/15 grün.

### research — vorher 44 (help-text), nachher 16
Oberfläche (Bild `before/research/1280-01-open.png`):
- Linke Spalte: Zähler „6 aktiv“ hinter „Aufgaben“ entfernt (er zählte alle Aufgaben, nicht die aktiven).
- Reiter „Quellen 0 · Kandidaten 0 · Messpunkte 0 · Wissen 0“: Zahl nur ab 1 (`countedTabButton`); Zahl bleibt in
  `aria-label`/Tooltip.
- Rechte Spalte: leere Überschrift „AUSGEWÄHLTE QUELLE“ ohne gewählte Quelle entfällt (Abschnitt `hidden`, eigene
  CSS-Regel, weil `.research-context-block` `display:grid` setzt); Rückfall „Selected Source“ → „Ausgewählte Quelle“.
  Der Abschnitt `[data-selected-source-section]` bleibt im DOM (Scroll-Ziel).
- `locales/{de,en}.json` unbenutzte Schlüssel `graphMethod` („Größe zeigt die fachliche Vernetzung …“) und
  `evidenceNoteText` entfernt; `defaultPromptGeneric` → `genericDefaultPrompt` (Agenten-Auftrag, Konvention `…Prompt`).
- Agenten-Aufträge, Dokument-Prompts (`DOCUMENT_PROMPTS`), Bewertungsregeln und Spalten-Tooltips der Messwerttabelle
  (`DIRECT_MEASUREMENT_COLUMNS`, landen im `title` der Kopfzelle) → `v4-allow: text`.
- Rest 16 = Fehlalarme bzw. toter Code (offene Fragen 3–5).
- Tests (`tests/*.test.mjs`, `test.mjs`) 139/139 grün.

### cv-print-builder — vorher 6, nachher 0
- `index.js:353` Bestätigung „… erneut parsen? Bestehende Parser-Tasks werden nicht gelöscht, die CVs erhalten neue
  Parser-Aufträge.“ → „N CV-PDFs erneut parsen? Jeder CV erhält einen neuen Parser-Auftrag, alte bleiben.“ (en entsprechend).
- `index.js:1958-1976` Parser-Auftrag an den Agenten → `v4-allow: text`.
- Tests 11/11 grün.

### matching (8) und invoices (3) — unverändert
Alle Treffer sind keine Oberflächentexte und liegen außerhalb der erlaubten Dateien (index.js/css/locales):
matching `ui/ctoxCommandAdapter.js:21-34`, `ui/matchingDefinition.js:62`, `ui/matchingTools.js:424`,
`ui/businessOsControls.js:1321` (Agenten-Prompts); invoices `core/invoice-validate.js:31/74/82`
(Validierungsfehler der Kernlogik). Siehe offene Frage 6.

## Nachher (gesamt)

```
Textdiaet-Waechter: 33 Treffer in 9 Dateien (249 geprueft) – help-text 33   (vorher 253: help-text 225, infobox 28)
     16  modules/research  help-text=16
      8  modules/matching  help-text=8
      6  modules/outbound  help-text=6
      3  modules/invoices  help-text=3
--check: Exit 0 (coding-agents nicht mehr über der Baseline)
```
- Infoboxen 28 → 0. Davon echte Boxen entfernt: buchhaltung 22 (Callout-Rahmen, gelbe Labels/Beträge), shiftflow 1
  (rote Konfliktboxen); als Fehlalarm geklärt: news 1 (Zonen-Code), outbound-lead-generation 4 (Meldezeilen ohne
  Rahmen, Klassen umbenannt).
- Hilfetexte 225 → 33. Echte Oberflächentexte entfernt/gekürzt: rund 60 Stellen (Liste je App oben). Der größere Teil
  der Senkung sind Agenten-Prompts, Vorlagen und Standardinhalte, die der Wächter nicht von Oberflächentext
  unterscheiden kann; sie tragen jetzt `v4-allow: text (<Art>)` bzw. einen `…Prompt`-Schlüssel. Inhalt unverändert.
- `textdiaet-baseline.json` nur abwärts für die bearbeiteten Apps geschrieben (`--write-baseline --app …`;
  geprüft: 0 Erhöhungen, 21 Senkungen).
- `node v4/debt/ratchet.mjs` OK (guard 1838 · scan 3), `--update` geschrieben (nur Zeitstempel, keine Zahl gestiegen).
  Zwischenstand: Ein Lauf meldete kurz „GESTIEGEN“ – `v4/frame/lab/maengel.mjs` (fremder Strang, english-raw-term/
  de-synchronisier) und einmal OLG `vocab:de-verbindung` durch meine erste Fassung (umformuliert, siehe oben).
- `node scripts/assert-shell-v2-contract.mjs` 38/38, `node scripts/assert-design-system.mjs` OK,
  `node --test v4/tools/guards/textdiaet-rule.test.mjs` 8/8.
- `CTOX_APP_API_STUB=…/637caf6 node v4/frame/lab/maengel.mjs M13,M15`: 9/9 JA (M-13 credentials 1280/420,
  buchhaltung 1280; M-15 mail, outbound, news 1280; M-10/NEBEN credentials).
- Rundgang nachher (rundgang2, 1280 + 420, 8 Apps): alle OK, 0 Seitenfehler, Schritt- und Schreibzahlen identisch zu
  vorher (buchhaltung 6/42, outbound 6/2, outbound-lead-generation 4–8/39, research 6–8/0 …).

## Bildprüfung (selbst angesehen, 1280)
- outbound: Reiter ohne Zähler, Kampagnenliste ohne „0 Leads“/„Keine Sequenz“, Firmenzeilen ohne „Recht: —“-Captions,
  „Versteckte Firmen“ ohne „0“. Bedienbar wie vorher; Knopfwand je Zeile bleibt (Frage 2).
- buchhaltung: Belegkontierung ohne gelbe Kästen, Felder schlicht; UStVA-Kacheln neutral, DATEV-Bereich nur Von/Bis +
  „Exportieren“; Bilanz ohne „Aktiv“-Pille. Hauptansicht unverändert.
- research: keine Nullzähler, kein „6 aktiv“, kein leerer Abschnitt rechts.
- outbound-lead-generation: Hauptansicht unverändert (Änderungen sitzen in Dialogen/Meldungen).
- Nichts Klickbares unter App-Icon oder Fensterknöpfen neu entstanden (Regel 8 nicht berührt).

## Offene Fragen (Text oder Gestaltung, die ich bewusst nicht entfernt habe)

1. **buchhaltung, Fibu-Agent-Chat ist eine Attrappe** (`index.js:3210-3245`): Antworten sind fest verdrahtet
   (Hetzner-Rechnung HET-2026-98127 mit festen Beträgen, „Bilanz … 12.160,00 €“ entfernt, „2 neue Belege“ entfernt).
   Ich habe nur gekürzt und die erfundenen Zahlen gestrichen. Soll der Reiter „Agentenprotokoll“ ganz weg, bis er an
   echte CTOX-Befehle angeschlossen ist? (Logik, nicht Teil der Textdiät.)
2. **outbound, Knopfwand je Firmenzeile** (Qualifizieren, Ausschließen, Ausblenden, Nachrecherche, Validiert, Log) und
   4 Symbolknöpfe je Kampagne: Regel 6 verlangt eine Hauptaktion + „…“-Menü. Das ist ein Umbau (Menü), keine Kürzung –
   eigener Auftrag?
3. **outbound, Pille „1 wartet“**: Laut Owner-Regel gehört der Stand in die Zone; `tests/v4-api.test.mjs:136` verlangt
   aber eine leere Zone nach dem Öffnen und führt „\d+ wartet“ als Fachbegriff (TEXT-B-Entscheidung). Nicht geändert.
4. **research, toter Code**: `DRONE_SOURCES_METADATA` (`index.js:220-340`, Drohnen-/Militär-Musterkatalog) und
   `computedDecisionNotes` (`:3941`, mit den Schlüsseln `decisionNote*Body`) werden nirgends aufgerufen. 9 der 16
   Resttreffer. Löschen (Code, kein Text)?
5. **research, `defaultCriteriaText`** (Agenten-Kriterien, 2 Treffer in den Locales): Schlüssel wird von `test.mjs:1092`
   namentlich verlangt, deshalb nicht umbenannt. Zwei weitere Resttreffer sind Fehlalarme des Wächters auf
   `title="…"`-Attributen mitten in einem Template (`index.js:3604/3607`).
6. **matching/invoices**: Fehlalarme in `ui/*.js` bzw. `core/invoice-validate.js` (nicht freigegebene Dateien). Mit
   Freigabe der Dateien je ein `v4-allow: text`-Kommentar, oder der Wächter nimmt `ui/…Adapter|Definition|Tools` aus.
7. **outbound-lead-generation (thesen, Produktion)**: Meldungen nur gekürzt, keine entfernt; die Recherche-Kennzahlkacheln
   („0/31 recherchiert …“) und der Statuschip „Nicht gestartet“ bleiben (Owner-Fachregeln, TEXT-B). Vor dem nächsten
   thesen-Release sollte jemand die gekürzten Dialogtexte einmal bedienen.
8. **buchhaltung, Kennziffer-Beschriftungen** der UStVA („… [Kennziffer 81]“) und die Optionstexte der Zahlungswege
   („Privat getragen (Privatauslage des Inhabers - Gegenbuchung auf 1890)“) bleiben: Fachangaben, die Buchhalter
   brauchen.
