# Business OS App Creator Target Model

Dieses Dokument beschreibt, wie App-Erstellung in CTOX Business OS funktionieren
soll. Es ist ein Zielbild fuer Produkt, UI und Architektur.

## Kurzfassung

Der App Creator ist keine grosse Generator-Workbench. Er ist eine kleine
System-App, aehnlich wie Files: schnell oeffnen, Wunsch beschreiben, Vorlage
waehlen, Chat starten, Ergebnis installieren oder im App Store weiterbearbeiten.

Die vollstaendige App-Erstellung und Verwaltung gehoert in den App Store. Der
App Creator ist nur der reduzierte Einstiegspunkt in denselben Erstellungsfluss.

## Problem Mit Dem Aktuellen Muster

Der aktuelle Creator macht Generator-Interna zur Hauptoberflaeche:

- links Parameter und Prompt-Felder
- in der Mitte Spezifikations- und Deploy-Flaechen
- rechts installierte Apps und CTOX-Prompt-Artefakte
- sichtbare Build-Logs und interne Chat-Koordinationsprompts

Das ist fuer Nutzer die falsche Abstraktion. Wer auf "App erstellen" klickt,
will nicht eine Generator-Konsole bedienen. Der erwartete Ablauf ist:

1. Chat geht auf.
2. Nutzer beschreibt die App oder waehlt eine Vorlage.
3. CTOX fragt nach, wenn etwas fehlt.
4. CTOX erstellt eine Vorschau.
5. Nutzer installiert, verwirft oder bearbeitet weiter.

Alles andere ist Diagnose, Verwaltung oder Expertenmodus und gehoert nicht in
die schlanke Creator-App.

## Rollenverteilung

### App Creator

Der App Creator ist eine System-App mit minimaler Oberflaeche.

Er soll:

- eine neue App-Erstellung per Chat starten
- eine Vorlage als Startkontext an den Chat uebergeben
- laufende oder kuerzlich begonnene App-Entwuerfe fortsetzen
- nach erfolgreicher Erstellung direkt zur neuen App oder zum App Store
  weiterleiten
- nur einfache Status anzeigen: Idee, Spezifikation, Vorschau, Installation

Er soll nicht:

- App-Katalog, Versionen, Rollback, Updates oder Uninstall verwalten
- Build-Logs als primaere UI zeigen
- interne Prompts, Koordinations-IDs oder Harness-Artefakte anzeigen
- eine eigene parallele App-Store-Verwaltung nachbauen
- eine breite Drei-Spalten-Workbench sein

### App Store

Der App Store ist die vollstaendige App-Oberflaeche.

Er soll alle App-Creator-Funktionen vollstaendig enthalten:

- neue App von Scratch erstellen
- neue App aus Vorlage erstellen
- Templates durchsuchen, vergleichen und installieren
- installierte Custom Apps anzeigen
- Custom Apps oeffnen, bearbeiten, duplizieren, aktualisieren und entfernen
- Versionshistorie anzeigen
- Rollback ausfuehren
- Berechtigungen und Datenmodell vor Installation pruefen
- Quellpaket, Manifest und App-Dateien inspizieren
- Erstellungslaeufe, Fehler und Validierungsergebnisse nachvollziehen
- optional spaeter: App veroeffentlichen, exportieren oder in einen Katalog
  uebernehmen

Der App Store ist damit der vollstaendige Arbeitsplatz. Der App Creator ist nur
die schnelle Tuer in diesen Arbeitsplatz.

## Nutzerfluss

### Neue App Aus Dem App Creator

1. Nutzer oeffnet "App Creator".
2. Der erste Screen besteht aus einem Chat und einer kompakten Vorlagenauswahl.
3. Nutzer schreibt zum Beispiel: "Baue mir ein Notizbuch fuer mein Team mit
   Tags und Markdown."
4. Business Chat startet im Modus `app-create`.
5. CTOX stellt maximal die fehlenden Rueckfragen.
6. CTOX erstellt eine Spezifikation, Datenmodell, UI-Struktur und
   Berechtigungsentwurf.
7. Der Chat zeigt eine knappe Zusammenfassung und eine Vorschau-Aktion.
8. Nutzer klickt "Installieren" oder "Im App Store bearbeiten".
9. Nach erfolgreicher Installation wird die neue App geoeffnet und im App Store
   als installierte Custom App gefuehrt.

### Neue App Aus Dem App Store

1. Nutzer oeffnet App Store.
2. Nutzer klickt "Neue App" oder waehlt ein Template.
3. Der App Store zeigt den vollstaendigen Flow:
   Template, Beschreibung, Datenmodell, Berechtigungen, Vorschau, Tests,
   Installation.
4. Der Chat ist dabei die Fuehrungsschicht, nicht ein separates Nebenfenster.
5. Die gleiche CTOX-Erstellungsengine wird verwendet wie beim schlanken App
   Creator.

### Bestehende App Weiterentwickeln

1. Nutzer oeffnet App Store.
2. Nutzer waehlt eine installierte App.
3. Nutzer klickt "Bearbeiten" oder "Mit KI erweitern".
4. Business Chat startet im Modus `app-modify` mit Modulkontext,
   Versionen, Manifest, Datenmodell und bekannten Fehlern.
5. CTOX erstellt eine neue Version, validiert sie und bietet Vorschau,
   Installation oder Rollback an.

Der App Creator darf fuer "letzten Entwurf fortsetzen" genutzt werden, aber
dauerhafte Verwaltung bleibt im App Store.

## Business OS UI-Vertrag

Die schlanke App-Creator-Oberflaeche besteht aus:

- Titelzeile: "App Creator"
- Chatbereich: dieselbe Business-Chat-Komponente wie im Shell-Kontext
- kompakte Vorlagenleiste oder Vorlagen-Popover
- kleine Fortschrittsanzeige: Idee, Spezifikation, Vorschau, Installation
- Link: "Im App Store weiterbearbeiten"

Die UI darf keine internen technischen Rohdaten als normalen Inhalt zeigen.
Diagnose darf es geben, aber nur hinter "Details" oder im App Store.

Sichtbare Status muessen nutzerlesbar sein:

- gut: "Spezifikation wird erstellt"
- gut: "Vorschau bereit"
- gut: "Installation fehlgeschlagen: Manifest enthaelt keine Startdatei"
- schlecht: `BROWSER_CHAT_COORD_...`
- schlecht: `[SYSTEM] App Creator initialisiert...`
- schlecht: "Spezifikation ist aktuell und installierbar", wenn der
  Installationsbutton faktisch nicht sinnvoll nutzbar ist

## CTOX-Seite

CTOX ist die Autoritaet fuer Erzeugung, Installation, Versionierung und
Validierung. Der Browser startet nur Befehle und zeigt replizierte Ergebnisse.

Der Zielablauf auf CTOX-Seite:

1. Browser schreibt einen Befehl in `business_commands`, zum Beispiel
   `ctox.business_os.app.create`, `ctox.business_os.app.modify`,
   `ctox.business_os.app.preview` oder `ctox.business_os.app.install`.
2. Der native CTOX-Peer validiert Actor, Rechte, Zielmodul, Template,
   Berechtigungen und Eingabedaten.
3. CTOX erzeugt daraus durable Arbeit im Harness/Queue-System.
4. Der Agent erstellt Spezifikation, Moduldateien, Tests und Manifest.
5. CTOX staged das Modul zuerst kontrolliert, validiert es und installiert es
   erst danach ueber den nativen Modulmanager.
6. CTOX schreibt Versionseintraege in `business_module_versions` mit passendem
   Ursprung wie `creator_deploy`, `edit`, `rollback` oder `install`.
7. CTOX publiziert Fortschritt, Fehler, Artefakte und Ergebnis wieder ueber
   Business-OS-Sync-Projektionen.

Der Chat darf niemals der einzige Speicherort fuer den Zustand sein. Jeder
laufende Erstellungsprozess braucht eine persistierte Run-/Command-Projektion,
damit Reload, zweiter Browser, Retry und Review sauber funktionieren.

## Datenpfad Und Guardrails

Alle Business-OS-Daten bleiben auf dem CTOX-DB-WebRTC-Pfad.

- keine HTTP-Fallbacks fuer App-Records, Commands, Modulstatus oder Manifeste
- keine Browser-zu-CTOX-Datenbruecke ausserhalb von CTOX DB
- keine direkten Patches an `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`
- neue first-class Collections nur ueber die bestehende Fixture- und
  Generatorstrecke

Custom Apps sollen standardmaessig generische Business-OS-Datenmodelle nutzen:

- `business_definitions` fuer Definitionen, Schemas, Display-DSL und Prompts
- `business_records` fuer App-Daten
- vorhandene erlaubte Collections nur, wenn sie im Manifest deklariert sind

Eine generierte Custom App darf nicht heimlich eine neue native Sync-Collection
einfuehren. Wenn eine App wirklich eine first-class Collection braucht, ist das
kein normaler Creator-Deploy, sondern eine CTOX-Core-Aenderung mit
Wire-Contract-Generatoren und Guard-Tests.

## Generiertes Modul

Ein generiertes Business-OS-Modul besteht mindestens aus:

- `module.json`
- `index.html`
- `index.css`
- `index.js`
- optional `schema.js`
- optional `locales/*.json`
- Modul-Smoke-Test

Das Manifest deklariert:

- Modul-ID und Titel
- Einstiegspunkt
- verwendete Collections
- Berechtigungen
- Kategorie und Store-Metadaten
- Installationsscope
- ob die App editierbar, installierbar oder systemintern ist

Vor Installation muss CTOX pruefen:

- Manifest gueltig
- Modul-ID kollisionsfrei oder explizit als Upgrade bestaetigt
- keine verbotenen Imports oder Datenpfade
- keine undeclared Collections
- App laedt im Business-OS-Shell-Kontext
- relevante Smoke-Tests laufen
- RxDB-only-Guard bleibt gruen

## App Store Als Vollversion

Die vollstaendige Creator-Funktionalitaet lebt im App Store, weil der App Store
bereits der Ort fuer Katalog, Installation und Modul-Lebenszyklus ist.

Der App Store braucht dafuer eine eigene "Erstellen"-Sicht oder einen
"Neue App"-Flow:

- Scratch-App erstellen
- Template auswaehlen
- Prompt und Rueckfragen
- Datenmodell ansehen und bearbeiten
- Berechtigungen ansehen und genehmigen
- UI-Vorschau
- Test-/Validierungsstatus
- Installieren
- Version ansehen
- Rollback
- App oeffnen

Der App Creator ruft denselben Flow reduziert auf. Es darf keine zweite,
abweichende Creator-Engine geben.

## Akzeptanzkriterien

Ein Umbau ist richtig, wenn folgende Punkte erfuellt sind:

- Klick auf "App erstellen" oeffnet unmittelbar einen Chat im App-Erstellmodus.
- Template-Auswahl ist erhalten, aber nur als Startkontext fuer den Chat.
- App Creator zeigt keine Drei-Spalten-Workbench mehr.
- App Creator zeigt keine internen Prompts, IDs oder Logs im Hauptscreen.
- App Store enthaelt alle erweiterten Creator-Funktionen.
- App Creator und App Store verwenden dieselbe CTOX-Command-/Run-Engine.
- Installation und Upgrade sind native, validierte CTOX-Aktionen.
- App-Status ueberlebt Reload und zweiten Browser.
- Neue Custom-App-Daten laufen ueber CTOX DB/WebRTC, nicht ueber HTTP.
- Rollback und Versionierung laufen ueber `business_module_versions`.
- Fehler erscheinen als handlungsfaehige Chat-/App-Store-Meldungen.
- System-Apps wie App Creator und App Store sind nicht als normale
  Marketplace-Apps installierbar.

## Nicht-Ziele

- kein sichtbares Harness-Dashboard als Standard-Creator
- keine App-Erstellung nur per lokaler Browserlogik
- keine separaten HTTP-Deploy-Endpunkte fuer Business-OS-Records
- keine neuen Env-Var-Schalter fuer Runtime-Verhalten
- keine automatische Aenderung des CTOX-DB-Wire-Contracts durch eine
  generierte Custom App

## Zielzustand In Einem Satz

Der Nutzer spricht mit CTOX, nicht mit einem Generator-Formular: App Creator
startet den Chat, App Store verwaltet den gesamten App-Lebenszyklus, und CTOX
erzeugt, prueft, versioniert und installiert die App ueber den bestehenden
Business-OS-WebRTC- und Command-Pfad.
