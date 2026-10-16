# Bedienrundgang Strang RUND2 (Shell v4)

Stand: 15.10.2026, laufend fortgeschrieben. Endlauf und App-Tests gegen Stub-Pin 68b83a3 (vorher 7706796).

Werkzeug: `v4/tools/rundgang/rundgang2.mjs` (eigene Datei; `rundgang.mjs` von RUND1 unverändert).
Lauf: `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/7706796 node v4/tools/rundgang/rundgang2.mjs --app <id,…> [--widths 1280,420]`.
Je App und Breite: Launcher-Kachel per echter Maus, Fenster über Layout-Menü maximiert (1280; 420 = Mobil-Blatt),
dann echte Mausklicks auf Reiter, Listenzeilen/Karten, alle Knöpfe (zerstörende zuletzt), Auswahlfelder, Formular füllen
und absenden, Suche ohne Treffer und geleert. Mitgeschrieben je Schritt: `api.put/patch/remove/command` mit lokalem
und Core-Ergebnis, Zonenmeldungen, `api.shell.dialog/menu`-Anfragen, Seitenfehler, Geometrie (seitlicher Überlauf,
Elemente außerhalb des Fensters, Fremdelemente an `document.body`). Screenshots: `shots/RUND2/<app>/<breite>-NN-<schritt>.png`,
Protokoll je Lauf `shots/RUND2/<app>/<breite>.json`.
Hinweis: Die Screenshot-Verweise bei den Fehlerzeilen stammen aus den Vorläufen vor dem Fix; der Endlauf (nach allen Fixes) hat
`shots/RUND2/` neu geschrieben und zeigt den behobenen Zustand (Dateinamen können sich durch andere Schrittfolgen verschoben haben).

## Rahmen-, Shell- und Werkzeugbefunde (nicht behoben, nur gemeldet)

| Bereich | Datei:Zeile | Befund | Wirkung im Rundgang |
|---|---|---|---|
| Fixtures (GEO) | v4/tools/geometry/fixtures.mjs:128 | Regel für Löschflags erkennt `_deleted` nicht (führender Unterstrich); Zeile 139 setzt Booleans auf `i % 2 === 0` → jede zweite Fixture-Zeile ist ein Grabstein (`_deleted: true`). Betrifft interviews, iot, matching, nachweise, placements, shiftflow, submissions (alle Collections mit `_deleted` im Schema). | Geometrie-Labor misst diese Apps mit halb leeren Daten. rundgang2.mjs entfernt `_deleted` vor dem Seed. |
| Rahmen/Host | v4/shell/app-host.js:343-346 | `api.shell` = `...data.shell` + eigenes `setTitle/registerHeader/navigate`; `dialog()`/`menu()` kommen ungefiltert vom Kernel. Der Stub (7706796, ctox-app-api-stub.mjs:1578) beantwortet sie sofort mit `spec.default` ohne Oberfläche; der v4-Rahmen zeichnet keinen Dialog. | Jede App, die über `api.shell.dialog` fragt (tickets „Ticket anlegen“, Begründungen, Löschbestätigungen …), zeigt im Rahmen nichts – Knopf wirkt tot. Im Rundgang als `dialogs` je Schritt protokolliert. |
| Shell-Kit | shared/base.css:709-716 | `.ctox-pane-tabs { min-width: max-content }` – Reiterleisten schrumpfen nie und werden in schmalen Listenspalten (340 px) vom Pane abgeschnitten. | invoices 1280 „Überfällig(1“, tickets 1280 „Geschlossen(0“ abgeschnitten. |
| App-CSS (systemisch) | z. B. modules/tickets/index.css:52-60, modules/invoices/index.css | „Shell V2 header geometry“-Blöcke (padding 0, Pane-Rahmen entfernt, Zonen für v2-Kopf freigehalten) in ~30 Apps; im v4-Rahmen gibt es den v2-Kopf nicht mehr. | tickets/invoices wirken ohne Abstände und Kartenrahmen, anders als kundenpipeline/knowledge. Systemische Entscheidung, nicht je App im Rundgang umgebaut. |
| Rahmen/Kit-CSS | v4/frame (lädt shared/base.css und app.css nicht; vgl. geometry-v4.mjs `--shared`) | App-Tests (`tests/v4-api.test.mjs`) laden Kit-CSS, der v4-Rahmen nicht. Dadurch sind Darstellungsfehler im Rahmen in den App-Tests unsichtbar (Beispiel threads-Kicker: im Test einzeilig, im Rahmen dreizeilig). | Regressionstests prüfen deshalb teils die App-eigene CSS-Regel statt der Darstellung. |
| Stub 68b83a3 | ctox-app-api-stub.mjs:1995 (`makeBinding`) | Ohne aktive Shell-Version scheitert jede App beim Öffnen über den Launcher mit `app.activation_failed` („Keine aktive Shell-Version für die Instanz-URL“). Das Geometrie-Labor merkt es nicht (öffnet per `openWindow`). | rundgang2.mjs veröffentlicht `shell@lab`; App-URLs tragen dann `~shell@lab` – Labor-Auflösung (`labBinding`) kennt das nicht, rundgang2 entfernt das Segment. Bibliotheken (`ctox:lib/ctox-office@1`) bleiben im Labor dann ungebunden (spreadsheets-Editor lädt nicht; mit 7706796 scheiterte er erst am fehlenden Rust-prepare). |
| Rahmen/Stub (Werkzeug) | shared/window-manager.js:1323-1333 | „Maximieren“ liegt im Layout-Menü (`data-window-layout-control="toggle"` → `maximize`), kein Direktknopf. | rundgang2 klickt beides mit echter Maus. |

## Ergebnis je App

| App | Schritt | Ergebnis | Befund | behoben in |
|---|---|---|---|---|
| kundenpipeline | 420/900: Liste suchen, Reiter, Entscheidung wählen | **Fehler** | modules/kundenpipeline/index.css:243-256 (alt): unter 1180 px Fensterbreite wurde die Rail (Suche, Offen/Alle, Liste) per `display:none` entfernt – bei 900 und 420 gab es keinen Weg zu einer anderen Entscheidung. Schot vorher `shots/RUND2/kundenpipeline/420-01-open.png` | modules/kundenpipeline/index.css (768–1180: Liste links + Detail, Aktionen unter dem Detail; ≤767: untereinander), Test `tests/v4-api.test.mjs` „schmale Fenster (900/420)“ |
| kundenpipeline | 1280: Annehmen/Ablehnen | ok | `command kundenpipeline.decision.answer` angenommen, Zone nur geräumt | – |
| iot | „Asset anlegen“ bei leerem Band/Suchergebnis | **Fehler** | modules/iot/index.js:503-506: `renderTree()` leert den Baum bei leerem Filterergebnis und ignoriert `state.creating` → Knopf ohne Wirkung (Rundgang: Band „Alarme (0)“ aktiv) | modules/iot/index.js:503-507, Test „Anlegen bei leerem Suchergebnis“ (ohne Fix rot, gegengeprüft) |
| iot | Auftrag anlegen ohne Signal | ok | Zone `iot.auftrag.no_signal` (warn), Inhalt ohne Zustandstext | – |
| shiftflow | „Neue Schicht“ → Absenden | **Fehler** | modules/shiftflow/index.js:2420: Absendeknopf des Anlegen-Drawers hieß „Dienstplan“ (Schlüssel `tabScheduler` = Reitertitel) | index.js:2420 `shiftCreateSubmit` „Einplanen“ (de/en), Test im Schreib-Test; Rundgang: `put planning_shifts` angenommen |
| shiftflow | 420: Reiter Abrechnung | **Fehler** | modules/shiftflow/index.css:1424-1426: `flex-wrap: nowrap` der Datumsleiste schob „Filtern“ rechts aus dem Pane (429 > 414 px) | index.css (Datumsfelder schrumpfen, Knopf fest), Test „Abrechnung bei 420 px“ (ohne Fix rot) |
| shiftflow | 420: Abrechnungstabelle | Befund | `div.ctox-table-wrap` (8 Spalten) scrollt seitlich 864 > 406 px ohne `[data-scroll-x]`; Entscheidung GEO2 erlaubt Datentabellen nur mit Markierung – nicht still markiert, Orchestrator-Entscheid | offen |
| shiftflow | Dienstplan automatisch generieren | ok | 5× `put planning_shifts` angenommen; Zone-Info „Dienstplanentwurf … wurde angelegt.“ – Erfolgsmeldungen in der Zone regelt der Rahmen (FR4), kein App-Fehler | – |
| threads | Aktionsspalte: Aufklapp-Köpfe | **Fehler** | modules/threads/index.css:672: `<summary><h3>` – Pfeil und Überschrift standen in zwei Zeilen | index.css (h3 inline), Test „Aufklapp-Köpfe … in einer Zeile“ (ohne Fix rot) |
| threads | Thread wählen bei offener Aktionsspalte (1280) | **Fehler** | modules/threads/index.css:142-147: Projekt-Kicker ist ein `<button>` ohne `white-space: nowrap` → im v4-Rahmen dreizeilig („WARTUNGSVER TRAG“), Aufgabentitel aus dem 37-px-Kopf geschoben. `shots/RUND2/threads/1280-14-button_An_Teammitglied_bergeben.png`. Im App-Test unsichtbar, weil dort Kit-CSS (app.css/base.css) geladen ist, die der v4-Rahmen nicht lädt | index.css (nowrap), Test prüft die Regel in der App-CSS (ohne Fix rot) |
| threads | Claim, Watch, Snooze, Notiz, Benachrichtigungen speichern | ok | je ein `command threads.*` angenommen | – |
| threads | Reiterleiste links (1280) | Befund | „Erledigt (0“ abgeschnitten – shared/base.css:709-716 (Kit) | gemeldet |
| outbound | Sequenz & Versand (1280/420) | **Fehler** | modules/outbound/index.css:1812: Reiterleiste sollte umbrechen, Kit-`min-width: max-content` verhinderte es → `section.outbound-outreach` scrollte seitlich (967 > 906 bei 1280, 967 > 420 bei 420), „Skills & Vorlagen“ abgeschnitten | index.css `min-width: 0`, Test „Reiterleiste … bricht um“ (ohne Fix rot) |
| outbound | Importjob anlegen (Dialog) | **Fehler** | modules/outbound/locales/de.json:46 u. a.: 36 ASCII-Umschreibungen in sichtbaren Texten („fuer“, „spaeter“, „rechercheert“, „Schliessen“ …) | locales/de.json (36 Ersetzungen), Test „deutsche Texte ohne ASCII-Umschreibungen“ |
| outbound | Kampagne anlegen, Vorlage speichern | ok | `put outbound_campaigns`, `command outbound.letter_template.save` angenommen | – |
| outbound | Infotext „CTOX bereitet Nachrichten … vor“ und Importer-Hinweis | Befund | erklärende Texte im Inhalt (Owner-Regel: keine Infoboxen) – Produktentscheid | offen |
| support | Reiter, Gespräch wählen, Notiz, Übernehmen, Zuweisen, Snooze, Lösen, Vorschlag anwenden/ablehnen, Status/Priorität | ok | 10 Befehle `support.*` angenommen (420); Kontextspalte bedienbar | – |
| support | Status-/Prioritäts-Auswahl im Kontext | Befund | `<select>` ohne `aria-label` (Name nur aus Optionstexten „open waiting resolved“) | offen |
| tickets | „Ticket anlegen“ | Rahmen | fragt Titel über `api.shell.dialog` – im v4-Rahmen ohne Oberfläche (siehe Rahmenbefunde), Stub antwortet `null` → nichts passiert | gemeldet (v4/shell/app-host.js) |
| tickets | 1280 Liste | Befund | Status in Listenzeilen und Reiter „Geschlossen(0“ rechts abgeschnitten; V2-Kopfgeometrie (modules/tickets/index.css:45-60) entfernt Abstände/Rahmen | gemeldet (systemisch) |
| tickets | Zeile wählen | ok | Detail wechselt (separat geprüft: Kicker item-002 nach Klick auf Zeile 2) | – |
| mail | Neue Mail ohne Empfänger | ok | Zone `mail.composer` (warn), kein Text im Inhalt | – |
| mail | Einstellungen | Befund | Status-Punkt, Zähler „0 in Queue · 0 zugestellt · 0 fehlgeschlagen“ und Hinweistexte („Änderungen werden sofort angewendet“, „Das Passwort wird … entfernt“) im Inhalt (Owner-Regel: keine Zähler/Infotexte) | offen (Produktentscheid) |
| mail/outbound | Importer-Dialog ohne Datei | Shell | „Bitte mindestens eine Datei auswählen.“ als Text im Dialogfuß – shared/universal-importer.js:909 | gemeldet |
| invoices | Rechnung wählen → Reiter Mahnen (nicht überfällig) | **Fehler** | modules/invoices/index.js:1535 (alt): Zustandstext „Dunning ist nur für überfällige Rechnungen verfügbar.“ im Inhalt (dazu Englisch „Dunning“). `shots/RUND2/invoices/1280-12-nav_Mahnen.png` | index.js: Reiter „Mahnen“ nur bei `state === 'overdue'`, kein Text; Test „nicht überfällige Rechnung – kein Reiter Mahnen“ (ohne Fix rot) |
| invoices | Journal, XRechnung, Zahlung zuordnen, Neue Rechnung, Export, Filter | ok | `command invoices.payment.allocate`, `invoices.invoice.create` angenommen | – |
| invoices | Zahlungen-Reiter | Befund | Erklärtext „Skonto wird nur abgezogen … lokale Zahlungslogik“ im Inhalt; Zahlungs-ID als Freitextfeld | offen |
| invoices | 1280 Liste | Befund | Reiter „Überfällig(1“ abgeschnitten (Kit, s. o.); Kopf-Ikone rechts angeschnitten; V2-Kopfgeometrie | gemeldet |
| invoices | 420 Positions-/Journaltabelle | Befund | `div.ctox-table-wrap` scrollt seitlich 604/620 > 392 ohne `[data-scroll-x]` | offen (Orchestrator, wie shiftflow) |
| knowledge | Quellen/Tabellen mit vielen Einträgen | **Fehler** | modules/knowledge/index.css:1114-1121: Zweitumschalter `overflow-x: auto` → seitlicher Scroll 1465 > 766 (1280), 657 > 420 (420) | index.css `flex-wrap: wrap; overflow-x: hidden`, Test „Quellen-/Tabellen-Umschalter brechen um“ (ohne Fix rot) |
| knowledge | Book erstellen/importieren/exportieren, Filter, Sortierung | ok | 3 Befehle `ctox.knowledge.book.*` angenommen | – |
| nachweise | Öffnen 1280/420, Nachweis hinzufügen | ok (Befund) | Schreiben angenommen. Befund: Untertitel „Generischer Nachweis-/Zertifikats-Tresor …“ (modules/nachweise/index.html:79, index.js:23) ist Erklärtext im Inhalt; App-Test `tests/v4-api.test.mjs:103` verlangt ihn ausdrücklich → nicht geändert, Produktentscheid | offen |
| spreadsheets | Tabelle importieren (Schublade) | **Fehler** | modules/spreadsheets/index.js:1717-1736: rote Fehlerzeile „Datei erforderlich.“ sofort beim Öffnen im Formular (ebenso „Titel fehlt.“ im Anlegen-Formular). `shots/RUND2/spreadsheets/1280-03-button_Tabelle_importieren.png` | index.js: kein Text im Formular, Absenden bleibt gesperrt; falsche Datei → Zone `spreadsheets.import.file`, Schließen räumt; Test „Import-Schublade ohne Fehlerzeile“ (ohne Fix rot) |
| spreadsheets | Leere Tabelle erstellen | ok | `put spreadsheet_versions`, `put spreadsheets`, `command office.spreadsheet.prepare` angenommen | – |
| spreadsheets | Tabelle öffnen | Umgebung | Editor im Labor nicht ladbar („CTOX Rust prepare did not return a Euro-Office XLSY editor payload“) – korrekt als Zone `spreadsheets.editor` (error), kein Inhaltstext | – |
| notes | Öffnen / Notiz auswählen | **Fehler** | modules/notes/index.js:415-423 + 1228-1240: Laden einer Notiz in den Lexical-Editor löste den Autosave aus – Lexical meldet das Update erst nach dem Commit (`hydratingEditor` schon `false`) und normalisiert das HTML → `patch notes/<id>` (neuer `updated_at_ms`) schon beim bloßen Öffnen und bei jedem Auswählen | index.js: Hydrations-Update mit Tag `ctox-notes-hydrate`, Listener übernimmt dafür nur die Vergleichsbasis; Test „Öffnen und Auswählen … schreibt nichts“ (ohne Fix rot) |
| notes | Neue Notiz, Notizbuch, Tag, Sperren, Ansichten, Filter, Suche | ok | Schreiben angenommen; Befund: Fußzeile „1 Min. Lesezeit · 10 Wörter · 73 Zeichen“ (Zähler im Inhalt) | offen |
| outbound-lead-generation | Kartenansicht | **Fehler** | customer-modules/outbound-lead-generation/index.js:2880: Wächter-Kommentar `// v4-allow: label` stand innerhalb des Template-Literals und wurde auf jeder Lead-Karte als Text „// v4- allow: label“ gerendert. `shots/RUND2/outbound-lead-generation/1280-10-button_Neue_Kampagne.png` (Vorlauf) | index.js:2880 `${'' /* v4-allow: label */}` (Manifeste unberührt); Test „kein Wächter-Kommentar … in HTML-Vorlagen“ (ohne Fix rot) |
| outbound-lead-generation | Kampagne, Import (Datei/Sellify), Ansichten, Auswahlaktionen | ok (Befund) | Befehle/Schreibvorgänge angenommen; Einordnung zeigt je Feld „noch nicht recherchiert“/„eintragen“ (fachlich gewollt, nicht geändert) | – |
| outbound | Suche/Filter/Bänder der Spaltenliste | Rahmen | wie RUND1: Spalten-Grammatik in outbound nicht verdrahtet (Suche ohne Wirkung); löst der Rahmen generisch (FR4), in der App bewusst nicht verdrahtet | gemeldet |
| research | Research anlegen ohne Knowledge Base | Befund | Formular „Dashboard anlegen“: Domain-Auswahl gesperrt, Hinweis „Noch keine Knowledge Base verfügbar.“ bzw. „N lokale Knowledge Domains verfügbar.“ unter dem Feld (modules/research/index.js:4037-4041) – Zustandstext im Inhalt; Kopf „Ziehen: drehen · Scrollen: zoomen“, „Kein aktiver Lauf“ | offen |
| research | Aufgaben wählen, Nachrecherche, Diagramm, Reiter | ok | Nachrecherche ohne geladene Domain → Zone `research.run.blocked` (warn) | – |
| research | Messpunkte-Tabelle | Befund | `div.research-table-host` scrollt seitlich 1060 > 654 (1280) bzw. > 392 (420) ohne `[data-scroll-x]` | offen (Orchestrator) |
| mail | Posteingang mit Konten (Seed angepasst), Gespräch öffnen, Neue Mail, Einstellungen, Domain, Import | ok (Befund) | Validierungen gehen in die Zone (`mail.composer`, `mail.mailserver`); Postfach anlegen braucht Nutzer-Auswahl (Browser-Pflichtfeld). Ohne Seed-Anpassung zeigte die App 0 Nachrichten (Fixture kennt `channel: 'email'`/`account_key` nicht) | – |
| matching | Anforderung anlegen, Ansichten, Filter, Match-Filter | ok | `put matching_requirements` (Quelle + Anforderung) angenommen; mit korrigierten Fixture-`kind`-Werten 6 Anforderungen sichtbar | – |
| news | Thema anlegen, Reiter, Filter | ok | `put news_topics` angenommen; Fixture-Ausgaben melden `news.issue.running`/`news.issue.failed` korrekt in die Zone | – |
| placements | Vermittlung anlegen (Form + Weitere Angaben) | ok (Befund) | `command ats.placement.create` angenommen (`placement_type: null` = Festanstellung, gewollt). Fußzeile „Eintrag wählen oder neue Vermittlung anlegen.“ = Hinweistext | offen |
| submissions | Kandidat vorstellen | ok (Befund) | `command ats.submission.present` angenommen; gleicher Fußzeilen-Hinweis wie placements | offen |
| reports | An Coding Agent übergeben → Zur Freigabe einreichen | ok (Befund) | `command threads.ctox_approval.request` angenommen; Dialog-Hinweis „Die Delegation geht als Freigabe an Threads …“ = Erklärtext | offen |
| interviews | Termin anlegen, Termin bearbeiten, Filter, Ansichten | ok (Befund) | `put`/`patch interview_meetings` angenommen. Befund (Fixture/App): Termine mit `start` 0/leer erscheinen als „01.01.70, 01:00“, nach dem Bearbeiten als „ohne Termin“ – uneinheitlich; Bearbeiten speichert `start: 0`, wenn das Datumsfeld leer bleibt | offen |

## Apps, die schon beim bloßen Öffnen schreiben (Endlauf, Stub 68b83a3, 1280 und 420 gleich)

| App | Schreibvorgänge beim Öffnen | Einordnung |
|---|---|---|
| outbound-lead-generation | 21× `put outbound_lead_generation_sources`, 6× `put outbound_lead_generation_adapters`, 6× `put outbound_lead_generation_leads`, 6× `command outbound.sellify.lookup` | Befund: Seeding der Quellen-/Adapterkataloge plus Lead-Schreibvorgänge und Sellify-Abfragen ohne Nutzerhandlung (Manifeste/Handler nicht angefasst) |
| outbound | `put outbound_campaigns/outbound_default_campaign`, `patch outbound_campaigns`, 9× `command knowledge.command` (`data list`/`data create --domain outbound`) | Befund: Standard-Kampagne und Knowledge-DataFrames werden beim Öffnen angelegt (App-Test nennt das Anlegen der Standard-Kampagne bereits) |
| matching | 6× `put matching_results` (Matches werden beim Öffnen berechnet und gespeichert) | Befund |
| spreadsheets | 3× `put spreadsheet_runbooks` (`spreadsheet.summarize`, `.audit-formulas`, `.risk-review`) | Befund: Runbook-Seeding |
| notes | vor dem Fix: `patch notes/<ausgewählte Notiz>` | **behoben** (s. o.); Endlauf: 0 Schreibvorgänge beim Öffnen und Auswählen |
| alle übrigen 16 Apps | keine | – |

## Prüfung

- App-Tests (Stub 68b83a3), alle grün: kundenpipeline 8/8, iot 11/11, shiftflow 8/8, threads 14/14, outbound 11/11
  (+ focus-navigation/refresh/outbound.test.mjs), invoices 13/13 (+ 10 Unit-Tests), knowledge 12/12, spreadsheets 9/9
  (+ 2 Unit-Tests), notes 9/9, outbound-lead-generation 13/13. Jeder neue Regressionstest wurde gegen den Stand ohne
  Fix gegengeprüft (rot) – Ausnahme threads-Kicker: dort prüft der Test die App-CSS-Regel, weil der Test-Harness Kit-CSS lädt.
- `node v4/tools/guards/guard-v4.mjs --app <berührte Apps> --summary`: 0 Treffer.
- Geometrie-Labor (`geometry-v4.mjs --app` der 8 berührten Apps, Ausgabe nur in den Scratch-Ordner, GEO-results.json
  unberührt): alle G1–G7 grün bis auf zeitweise sichtbare Zonen-Erfolgsmeldungen („Befehl … erledigt“, FR4) und einen
  einmaligen G1-Seitenfehler bei spreadsheets unter Last; Wiederholung grün.
- Endlauf rundgang2: 21 Apps × 2 Breiten = 42 Läufe, 0 Abbrüche, 0 abgelehnte Schreibvorgänge/Befehle. Verbleibende
  Geometrie-Meldungen nur Datentabellen ohne `[data-scroll-x]` (shiftflow, invoices, research – Orchestrator-Entscheid).
- Beim Ausführen der App-Tests schreiben iot/invoices ihre Test-Screenshots (`tests/shots/*.png`) neu (Nebenwirkung der
  vorhandenen Tests).

## Werkzeug-Hinweise rundgang2.mjs

- Fixture-Anpassungen nur im Rundgang-Seed: `_deleted` entfernt, `user-001` → `owner` in Zuordnungsfeldern, mail
  (`channel: 'email'`, `account_key`, Kontoinhaber), matching (`kind`).
- Klicks nur auf erreichbare Elemente (Treffer per `elementFromPoint`), sonst Escape bzw. Schließen-Knopf des obersten
  Overlays; Formularabsenden über den Knopf im selben Formular/Container.
- Nicht automatisch bedient: native Datei-Dialoge, Pflicht-Auswahlfelder (Browser-Validierung), Drag & Drop.
