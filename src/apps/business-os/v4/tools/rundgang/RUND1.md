# Bedienrundgang Strang RUND1 (Shell v4)

Stand: 15.10.2026, Lauf abgeschlossen (Endlauf mit dem endgültigen Skript, alle 20 Apps × 1280/420). Apps: app-store, appsec-pentest, browser, buchhaltung, calendar, coding-agents, consent,
conversations, creator, credentials, ctox, customers, cv-print-builder, desktop, documents, esign, explorer,
file-viewer, importer, intake.

## Werkzeug (auch für RUND2)

`v4/tools/rundgang/rundgang.mjs` (aus `src/apps/business-os/`):

    CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/7706796 \
      node v4/tools/rundgang/rundgang.mjs --app <id>[,<id>…] --tag RUND2 [--widths 1280,420] [--max 28]

- Echter v4-Rahmen + app-host gegen den Kern-Stub, Fixtures aus dem Geometrie-Labor (`appConfig` aus
  `geometry-v4.mjs`). Alle übrigen Apps mit v4-Manifest werden mit Titel veröffentlicht (Launcher realistisch,
  `shell.navigate`-Ziele vorhanden), montiert wird nur die geprüfte.
- Öffnen über den Launcher (`page.mouse.click` auf die Kachel), Maximieren über das Fenster-Layoutmenü (Klick).
- Suche: Wort aus den Fixture-Zeilen, das im Inhalt steht, tippen, Wirkung messen, wieder leeren.
- Danach Breitensuche über alle sichtbaren Bedienelemente im App-Inhalt und im App-Kopf (`registerHeader`):
  echter Mausklick, neu erscheinende Elemente kommen in die Warteschlange; `select` wird auf die nächste Option
  gestellt; vor Speichern/Anlegen/Senden werden leere Textfelder per Tastatur gefüllt. Destruktives
  (Löschen/Entfernen/Abmelden/Zurücksetzen …) wird nicht geklickt. Fremde Fenster (nach `shell.navigate`) schließt
  der Lauf per Schließen-Knopf.
- Je Schritt: Wirkung (Inhalt/DOM geändert), Writes (`put/patch/remove` + Ergebnis), Befehle (`api.command` +
  Endstatus), `shell.dialog/menu/openFile/navigate`, Downloads, Zone (Einträge, sichtbar), Seitenfehler,
  horizontales Scrollen, App-Element außerhalb des Fensters, neue Kinder an `document.body`.
- Ausgaben: `shots/<tag>/<app>/<w>-<nn>.jpg`, Kontaktbogen `sheet-<w>.png`, `steps-<w>.json` (mit `flags`),
  `shots/<tag>/summary.json`.

## Ergebnis

| App | Schritt | Ergebnis | Befund | behoben in |
|---|---|---|---|---|
| app-store | Launcher → öffnen, Suche, Bänder Marketplace/Templates/Installiert/Lokal/System/Alles, Karten/Liste, Filter (Kategorie/Sortierung), Export, GitHub-/ZIP-Install, „Zum App Creator“ | ok: Export lädt `ctox-app-store-…json`, GitHub-Install fragt per `shell.dialog`, App Creator öffnet per `shell.navigate("creator")`, Kategorie filtert | keiner in der App. Hinweis: Suchwort ohne Treffer im Marketplace (0 Einträge) ändert nichts – erwartbar | – |
| appsec-pentest | Suche, Bänder Aktiv/Archiviert/Alle, Filterleiste, Test anlegen (Dialog: Umfang, Profil, Speichern), Import/Export, Workbench-Reiter, Graph/Replays, Nacharbeit-Dialog, Authz-Befehle | ok nach Fix: Suche filtert, `ctox.appsec.assessment.create` done, `ctox.appsec.graph.build`/`replay.*`/`authz.*` done, Export lädt | (1) **Suche, Ansicht, Filter und Bänder wirkungslos** – App verdrahtete `data-pg-*` nicht (`autoWirePaneGrammar` fehlte, der v4-Rahmen tut es nicht) `modules/appsec-pentest/index.js:126`. (2) Leerer Filtertreffer zeigte englischen Zustandstext „No tests match the current filter.“ im Inhalt `index.js:1192`. (3) Dialoge (per `show()`) schlossen nicht mit Escape. Shots `appsec-pentest/1280-02.jpg` (Suche), `1280-31.jpg` (Dialog) | `index.js` (Import + `autoWirePaneGrammar(root)`, Leertreffer ohne Text, Escape → Schließen-Knopf); Tests `tests/v4-api.test.mjs` „Spaltengrammatik …“, „leerer Suchtreffer … Escape …“ |
| browser | Suche, Neue Sitzung, Import/Export, Karten/Liste, Filter, Bänder Alle/Aktiv/Beendet/Scraping, Leiste ein/aus, Präzise Eingabe, Erweiterte Aktionen, Adresse öffnen | ok nach Fix: `browser.session.start` done (2×), Export lädt, Import öffnet Dateiauswahl | (1) **Band „Scraping“: Adapterliste scrollte seitlich** (678 px Inhalt in 341 px, nowrap-Zeilen in auto-Rasterspur) `modules/browser/index.css:185`. (2) **Linke Pane 327 px Inhalt in 320 px**: Kopf-Icons und Reiter „Scraping“ rechts abgeschnitten (auto-Spur ohne `minmax(0,1fr)`) `index.css:58`. (3) Fußzeile blieb im Scraping-Band bei „6 Sitzungen · Alle“ `index.js:2144`. Shot `browser/1280-14.jpg` (vorher) | `index.css` (Schiene + Pane `grid-template-columns: minmax(0,1fr)`, Karte `min-width:0`), `index.js` (Fußzeile „n Adapter · Scraping“); Test „Scraping-Band …“ erweitert |
| buchhaltung | Suche (Belege, Konten), Neue Buchung (Editor), Beleg wählen, OCR-Evidenz ein/aus, Import/Export, Karten/Liste, Filter, Bänder, Archivpfad kopieren, Vorkontierung freigeben | ok nach Fix: Export lädt, Storno/Freigabe-Pfade erreichbar | (1) **„Archivpfad kopieren“ meldete „…kopiert!“ obwohl die Zwischenablage ablehnte**, dazu unbehandelte Ablehnung (Seitenfehler) `index.js:3120`. (2) **Buchungseditor @420 scrollte 4 px seitlich** (Aktionsleiste −14 px bei 10 px Innenpolster im Container ≤ 768) `index.css:1233`. (3) offen, nicht behoben: beim Öffnen schreibt die App ~42 `accounting_accounts` (Kontenrahmen-Seed) – Schreiben beim bloßen Ansehen; Info-Meldungen (`fibuNotice`, 20 Aufrufe) bleiben dauerhaft in der Zone. Shot `buchhaltung/420-06.jpg` | `index.js` (await `writeText`, Fehler → Zone `archive.copy_failed`), `index.css` (Aktionsleiste im Container-Block); Tests „Archivpfad kopieren …“, „Buchungseditor im schmalen Fenster …“ |
| calendar | Suche, Neu (Kalender), Import/Export, Karten/Liste, Filter, Bänder Kalender/Buchungsseiten, Ereignis wählen/bearbeiten, Buchungen ein/aus, Zeitraum vor/zurück/Heute, Monat/Woche/Tag/Liste | ok: alle Ansichten schalten, Export lädt, Import öffnet Dateiauswahl, Formulare öffnen | (1) **Frisch geöffnetes leeres Formular zeigte sofort rot „Kalendertitel ist erforderlich.“** (Validierung vor jeder Eingabe) `index.js:2083`. Shot `calendar/1280-04.jpg` (vorher) | `index.js` `installFormValidation`: Fehlertext erst nach Berührung/Absendeversuch, Speichern bleibt gesperrt; Test „Formular: kein Fehlertext vor der ersten Eingabe …“ |
| coding-agents | Suche (Apps, Verlauf), Aktualisieren, App wählen, Export, Source-Editor, Karten/Liste, Filter, Bänder Verlauf/Delegationen, Modell, Delegieren | ok nach Fix: `ctox.coding.turn` done, `ctox.coding.models` beim Öffnen, Export lädt | (1) **Suche, Filter und Bänder wirkungslos** – `autoWirePaneGrammar` fehlte (nur Kommentar) `modules/coding-agents/index.js:179`. (2) Rahmen: `shell.navigate("code-editor")` → Zone „Ziel „code-editor“ ist nicht vorhanden“ (code-editor hat kein v4-Manifest unter `v4/manifests/`) | `index.js` (Import + `autoWirePaneGrammar(root)`); (2) gemeldet, nicht behoben |
| consent | Suche, Neue Prüfung, Import/Export, Karten/Liste, Filter, Bänder Alle/Gültig/Offen/Erloschen, Betroffenenrechte, Einwilligung prüfen | ok: `ats.consent.check` done, Export lädt, Filter/Bänder wirken | Auskunft ohne Subjekt-ID → Zone „Subjekt-ID erforderlich.“ (korrekt in der Zone). Löschen (Art. 17) bewusst nicht geklickt | – |
| conversations | Suche, Import/Export, Karten/Liste, Filter Account/Richtung/Zeitraum, Kontext ein/aus | ok: Suche filtert, Export lädt | offen, nicht behoben: (1) „Kontext einblenden“ ohne Auswahl wirkungslos (`conversationContextVisible(false,…)`), Knopf bleibt aktiv `index.js:346`. (2) Kanal-Rohwerte im Inhalt („physical_letter“), unbekannter Kanal wird als „Alle Channels“ beschriftet (doppelt neben dem Band „Alle Channels“). Shot `conversations/1280-01.jpg` | – |
| creator | Suche, Aufträge importieren/exportieren, App-Erstellung starten, Bänder Apps/Aufträge, Beispielvorlagen, Optionale Einstellungen, URL hinzufügen, Datentabelle hinzufügen | ok: `ctox.business_os.app.create` done, Vorlage füllt den Prompt, Export lädt | (1) **Sticky-Leiste „App erstellen“ endete 37 px über dem Scrollrand**: weitergescrollte Beispiele schimmerten unter der Leiste durch (Innenpolster des Scrollbereichs) `modules/creator/index.css:175`. Offen: @420 ist die Leiste nicht sticky (Pane wächst mit dem Inhalt), gesperrter Knopf sehr kontrastarm (Kit `.ctox-button:disabled`) | `index.css` (Leiste bis an den Rand: `bottom`/`margin`/`padding` mit Polster-Variable, Container-Block 18 px); geprüft per Sonde @1280/420 |
| credentials | Suche, Neue Zugangsdaten, Import/Export, Karten/Liste, Filter, Bänder, Hinzufügen | ok: `ctox.secret.list` beim Öffnen, Schlüsselprüfung → Zone „Ungültiger Schlüssel …“ | offen (Owner-Regel „keine Infoboxen“): Infobox „Write-only: Werte werden verschlüsselt …“ und Hinweiszeile „Das Wertfeld bleibt immer leer …“ im Inhalt. Shot `credentials/1280-04.jpg` | – |
| ctox | Suche, Weitere Aktionen, Export, Karten/Liste, Filter, Bänder, Sortierung, Zoom ein/aus/einpassen, Aufgabe bearbeiten, Crew-Mitglied öffnen, Details, Priorität, Abbrechen, Crew/Kanäle verwalten | ok nach Fix: Export lädt, `ctox.command.cancel` done, Mitglied-Schublade öffnet | (1) **Klick auf ein Crew-Mitglied → Endlosrekursion** (`RangeError: Maximum call stack size exceeded` in `syncDetailDrawer`/`watchMemberRuns`: Merker erst nach `subscribe` gesetzt, Rückruf kommt sofort) `index.js:5501`. (2) Zoomfläche scrollt nach „Vergrößern“ seitlich – gewollte Pan-Fläche, war nicht markiert `index.js:2409`. (3) Rahmen: „Kanäle verwalten“ → `shell.navigate("shell",{open:"settings"})` → Zone „Ziel „shell“ ist nicht vorhanden“ | `index.js` (Merker vor `subscribe`, `data-scroll-x` an `.ctox-flow-canvas`); Test „Crew-Mitglied öffnen …“ (Gegenprobe ohne Fix rot); (3) gemeldet |
| customers | Suche, Kunde anlegen (Formular, Anlegen), Ansicht speichern, Import/Export, Karten/Liste, Filter, Bänder, Kontakt/Opportunity wählen, bearbeiten, speichern, archivieren | ok: `customers.account.create`, `customers.view.save`, `customers.contact.update`, `customers.contact.archive` done | keiner. @420 liegt „Anlegen“ unter dem Falz (erreichbar per Scrollen) | – |
| cv-print-builder | Suche, Neuer CV (Dateiauswahl), Profile importieren/exportieren, Karten/Liste, Filter, Bänder, Parsing starten, Bühne ein/aus, Ansichten | ok: Export lädt, Neuer CV öffnet Dateiauswahl | Parsing ohne PDF → Zone „CV-Quelle ist unvollständig: PDF-Datei fehlt.“ (Fixture ohne Datei, korrekt). Offen: Ansichtswechsel „Original“ schreibt `documents`/`document_versions` (Ansichtsmodus wird im Dokument gespeichert) | – |
| desktop | Launcher → Desktop, jedes Icon (öffnet die App per `shell.navigate`), Kontextmenü | ok nach Fix: 40 Icons an eigener Stelle, jedes Icon navigiert | (1) **Über den Launcher geöffnet lagen alle Icons dauerhaft auf 12/12 übereinander** @1280: Seed/Klemmung lief an der winzigen Fläche des aufgehenden Fensters (1 Spalte) und klemmte y auf die Flächenhöhe, obwohl die Icon-Fläche senkrecht scrollt; das Ergebnis wurde gespeichert `index.js:191`, `index.js:583`. Shot `desktop/1280-01.jpg` (vorher). Offen: lange Beschriftungen ohne Ellipse abgeschnitten („Bewerbungseinga…“) | `index.js` (Seed wartet per ResizeObserver auf echte Flächengröße, Maße über `clientWidth/Height`, y nicht mehr geklemmt); Test „Fläche wächst nach dem Mount …“ |
| documents | Suche, Leeres Word-Dokument, Dokument verwalten, Import, Runbooks, Export (DOCX/Markdown), Karten/Liste, Filter (Typ, Status, App, Quelle, Tags), Sortierung, Bearbeiten/Vorschau | ok: `files.put` + `documents`/`document_versions` + `office.document.prepare` done | DOCX-Editor → Zone „konnte nicht geladen werden: CTOX Rust prepare …“ – Stub ohne native Office-Vorbereitung (bekannter Zonen-Fall `docx-ohne-kern`). Offen: Öffnen schreibt `document_runbooks` (Seed beim Ansehen) | – |
| esign | Suche, Neu, Import/Export, Karten/Liste, Filter, Bänder, Anfrage wählen, Detail ein/aus, Vertragstyp, Anlegen | ok: `ats.signature.request` done, Export lädt | Rahmen/Zone: Info „Export erstellt.“ bleibt dauerhaft in der Zone (gleiches Muster buchhaltung) – Frage an `v4/shell/status-zone.js`, ob `info` selbst ausläuft | – |
| explorer | Suche, Aktionen-Menü (`shell.menu`), Neuer Ordner, Hochladen (Dialog, Dateiauswahl), Quellen, Sortieren, Details | ok nach Fix: Menü über `shell.menu`, Dateiauswahl öffnet | (1) **Upload-Dialog nicht per Escape schließbar**, verdeckte danach die ganze App `modules/explorer/index.js:696` | `index.js` (Escape → Schließen, Fokus in den Dialog); Test „Upload-Dialog: Escape schließt ihn“ |
| file-viewer | Launcher → ohne Datei, Herunterladen | ok nach Fix | (1) **„Herunterladen“ ohne geöffnete Datei klickbar, aber wirkungslos** `index.js:66` | `index.js` (Knopf gesperrt ohne Datei); Test „mount ohne Datei …“ erweitert |
| importer | Kategorie, Portierung starten (URL), Quelldatei, Projektordner | ok: ungültige URL → Zone „Bitte eine öffentliche https://github.com/owner/repo-URL eingeben.“; Dateiauswahl öffnet | keiner | – |
| intake | Suche, Neue Bewerbung, Import/Export, Karten/Liste, Filter, Bänder, Erfassen, Weitere Angaben | ok: `ats.intake.capture` done, Export lädt | keiner (neue Bewerbung erscheint im Stub nicht, weil der Stub keinen Handler hat) | – |

## Rahmen/Shell/Kern – gemeldet, nicht behoben

| Stelle | Schritt | erwartet / ist |
|---|---|---|
| `v4/frame/index.js:418` (`navigate`) + `v4/manifests/` | coding-agents „Im Source-Editor öffnen“ → `shell.navigate("code-editor")` | Editor öffnet / Zone „Ziel „code-editor“ ist nicht vorhanden“ (kein v4-Manifest für `desktop-apps/code-editor`) |
| `v4/frame/index.js` (`navigate`) | ctox „Kanäle verwalten“ → `shell.navigate("shell", {open:"settings", tab:"channels"})` | Einstellungen/Kanäle öffnen / Zone „Ziel „shell“ ist nicht vorhanden“ – der v4-Rahmen kennt kein Shell-Ziel für Einstellungen |
| `v4/shell/status-zone.js` | esign „Exportieren“, buchhaltung (20× `fibuNotice` info) | Erfolgs-Infos verschwinden von selbst / „Export erstellt.“ bleibt bis zur nächsten Meldung stehen (Zone sichtbar über alle Folgeschritte) |
| `v4/shell/app-host.js` (während des Laufs fremd in Arbeit, uncommittet) | credentials/appsec @420, browser/ctox @1280: Befehl `done` | still (Kommentar `app-host.js:205`: „done → still“) / Zone zeigte „Befehl „ctox.secret.list“ erledigt“; im aktuellen Code steht der Text nicht mehr – nach Abschluss der Fremdänderung erneut prüfen |
| v4-Rahmen allgemein | Spaltengrammatik `data-pg-*` | Der v3-Rahmen verdrahtete sie; v4 nicht. Jede App muss `autoWirePaneGrammar(root)` selbst rufen. Quer über alle Apps fehlt der Aufruf noch in **`modules/outbound`** (RUND2) |
| Kit `.ctox-button:disabled` (shared) | creator „App erstellen“ gesperrt | lesbar / opacity .5 auf hellem Blau, Beschriftung kaum lesbar |

## Grenzen des Rundgangs (Stub)

- Befehle enden im Stub `done` ohne Kern-Handler: neue Bewerbung (intake), neue Signaturanfrage (esign) usw. erscheinen
  nicht als Daten; geprüft ist der gesendete Befehl samt Nutzlast.
- DOCX braucht die native Office-Vorbereitung (documents): Zonenmeldung erwartet.
- Zwischenablage ist im Headless-Chromium verweigert: buchhaltung meldet das jetzt korrekt als Fehler.
- Nicht geklickt: Löschen/Entfernen/Zurücksetzen/Archivpfad-Papierkorb (destruktiv); Ziehen (Spaltenbreite,
  Icons) nur als Klick. Grenze 34 Klicks je App×Breite (`--max`); Rest steht in `steps-<w>.json` als Notiz.
- Klickbare Zeilen ohne Rolle werden über den Zeiger-Cursor gefunden (max. 12 je Durchgang); Zeilen mit eigenem
  Knopf darin nur über den Knopf.

## Prüfungen nach den Fixes

- `node --test modules/<app>/tests/v4-api.test.mjs` (Stub 7706796): appsec-pentest 9/9, browser 11/11,
  buchhaltung 10/10, calendar 10/10, coding-agents 7/7, creator 10/10, ctox 12/12, desktop 7/7, explorer 10/10,
  file-viewer 9/9. Gegenproben ohne Fix rot: appsec (Spaltengrammatik), ctox (Rekursion). calendar.test.mjs 16/16,
  desktop icon-layout/responsive/desktop je grün. `coding-agents.test.mjs` 16/16 nur mit
  `node --import ./v4/tools/node/ctox-shared.mjs` (neuer `ctox:shared`-Import, wie dort dokumentiert).
- `v4/tools/port/scan.mjs`: 0 Stellen für die geänderten Apps.
- Geometrie-Labor `geometry-v4.mjs --app <10 geänderte> --widths 1280,420`: 20/20 OK, Zonen-Fall 1/1.
- Rundgang-Endlauf: `shots/RUND1/<app>/sheet-1280.png|sheet-420.png` (Kontaktbögen, selbst angesehen),
  Rest-Flags nur noch „Zone sichtbar“ mit den oben genannten erwarteten bzw. gemeldeten Einträgen.

## Für RUND2

- Skript wiederverwenden: `--tag RUND2`, eigene Ausgabe unter `shots/RUND2/`. Als Modul importierbar
  (`serve`, `pageState`, `pageCandidates`, `OTHERS`) für eigene Sonden.
- Häufigstes Muster in RUND1: fehlendes `autoWirePaneGrammar` (Suche/Bänder tot) – in RUND2 betrifft das `outbound`.
- Weitere Muster: `subscribe` ruft sofort zurück (Merker vor `subscribe` setzen), Raster-/Grid-Spuren ohne
  `minmax(0,1fr)`, Maße während der Öffnen-Animation, Dialoge per `show()` ohne Escape, Validierungstext vor der
  ersten Eingabe.
