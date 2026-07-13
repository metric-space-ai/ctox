# CTOX Documents + CTOX Spreadsheets: Portplan und Fortschritt

Stand: 2026-07-13

Dieses Dokument ist das zentrale, menschenlesbare Arbeits- und
Fortschrittsprotokoll fuer die beiden CTOX-Forks **CTOX Documents** und
**CTOX Spreadsheets**. Die
maschinenlesbare Source of Truth fuer Feature-Status und Abhaengigkeiten ist
[`features.json`](../src/apps/business-os/office-engine/features.json). Dieses
Dokument wird bei jedem Statuswechsel in der Feature-Matrix im selben Change
aktualisiert.

## Ziel und aktueller Gesamtstand

CTOX Documents und CTOX Spreadsheets sind eigenstaendige Downstream-Forks. Sie
uebernehmen die benoetigte Editor- und Formatlogik aus dem gepinnten
Euro-Office-Source, besitzen aber eigene Produkt-IDs, Source-Entry-Points,
ESM-Builds, Browser-Runtimes, CTOX-Adapter, Business-OS-Chrome und
Release-Evidenz. Euro-Office bleibt ausschliesslich Upstream-Historie und
getrenntes Entwicklungs-Oracle. DocumentServer, Konvertierung, Speicherung und
Sitzungslogik werden vollstaendig durch CTOX-eigene Browser- und
Rust-Komponenten ersetzt.

Der Fork-Source liegt unter `office-engine/src/forks/ctox-documents`,
`office-engine/src/forks/ctox-spreadsheets` und dem gemeinsamen Fork-Core. Die
Business-OS-Apps mounten nur die beiden CTOX-Fork-Entry-Points hinter der
stabilen ESM-API. Der gepinnte Upstream-Build ist ein reproduzierbarer
Build-Input des Forks, kein zur Laufzeit eingebettetes Fremdprodukt.

Aktueller Stand:

- Gesamtfortschritt: 9 von 10 Arbeitsstroemen sind technisch abgenommen
  (90 % des Gesamtplans). Offen bleibt A10 nur fuer das reale Switch-Release,
  eine nachfolgende stabile Release-Periode und die danach erst erlaubte
  Entfernung der Legacy-Engines.
- Die Release-Kandidatenbasis `4e0b2b43` schloss den zuvor reproduzierbaren
  `business-os-app-audience-ui`-Fehler: Der reine
  Audience-/Policy-Smoke ist nicht mehr an die fachfremde spaete
  Datei-Collection-Renegotiation gekoppelt. Ein realer lokaler Zero-Retry-
  Browserlauf bestand Reload, Private-/Preview-/Restricted-Sichtbarkeit,
  Deep-Link-Sperre, Clean Profile und manipulierte Browser-Storage-Werte bei
  null Browserwarnungen, null Browserfehlern und null fehlgeschlagenen
  Requests. Main CI [Run 29277806719](https://github.com/metric-space-ai/ctox/actions/runs/29277806719)
  blieb beim letzten Evidenz-Checkpoint aktiv. Der dreizyklische Release-Soak
  [Run 29277829908](https://github.com/metric-space-ai/ctox/actions/runs/29277829908)
  bestaetigte außerdem beide echten CTOX-Documents-/CTOX-Spreadsheets-
  Restart-Gates ohne Retry und passierten den Clean-Tree-Guard. Production
  Readiness [Run 29277831739](https://github.com/metric-space-ai/ctox/actions/runs/29277831739)
  lief ueber den Audience-Gate hinaus und fand danach im
  `browser-lifecycle-ui`-Modus einen separaten Shell-Fehler: Ein waehrend des
  absichtlichen Datenbank-Neuaufbaus gestarteter Modul-Mount meldete den
  erwartbaren `IDBDatabase ... closing`-Abbruch als Browserfehler. Der
  Produktpfad behandelt einen solchen Desktop-/Modul-Mount jetzt als
  recoverable Data-Plane-Abbruch und schliesst das unvollstaendige Fenster.
  Ein realer lokaler Zero-Retry-Lifecycle-Lauf akzeptierte danach alle sieben
  Browserbefehle, endete fuer Session, Runtime und Tab auf `stopped` und hatte
  null Browserwarnungen, null Browserfehler, null 404s sowie null
  fehlgeschlagene Requests. Terminale CI-/Soak-/Readiness-Evidenz fuer den
  daraus entstehenden Folgekandidaten wird erst nach dessen Abschluss
  eingetragen.
- Die automatisierbare Security-/Privacy-Provenienz ist gegen den aktuellen
  Readiness-/Release-Quellstand neu gehasht und validiert. Alle elf
  menschlichen Kontrollpunkte, Reviewer und Datum bleiben bewusst
  `pending-signoff`; die Aktualisierung ist keine vorweggenommene Freigabe.
  Die menschliche Checkliste benennt jetzt dieselben elf maschinenpflichtigen
  Control-IDs; der Validator lehnt kuenftigen Drift zwischen Checkliste und
  JSON-Gate fail-closed ab.
- Der Production-Readiness-Lauf `29282605997` deckte im kalten
  App-Release-Szenario eine inkonsistente Deadline auf: Der schwere Modus
  erlaubte dem Shell-Startup 240 Sekunden, wartete danach aber nur 60 Sekunden
  auf den nativen Command-Peer, der unmittelbar nach Ablauf hochkam. Der
  Release-Modus verwendet nun fuer Startup und nativen Peer dieselbe
  240-Sekunden-Grenze, weiterhin mit genau einem Versuch und unveraenderten
  Nullbudgets fuer Browserwarnungen, Browserfehler und Request-Fehler. Ein
  lokaler realer Zero-Retry-Lauf bestand danach Publish, Team-Sichtbarkeit,
  Versionsbadge, Data Review, Rollback, redigierte Audits, Reload und
  Storage-Boundary in 106 Sekunden bei jeweils null Browser- und Requestfehlern.
- Der Production-Readiness-Lauf `29288387363` bestaetigte den korrigierten
  App-Release-Pfad, fand danach aber dieselbe sachfremde Kopplung im
  `business-os-dynamic-apps-ui`-Gate: Nach dem absichtlichen Runtime-Schema-
  Neustart wartete der reine Modul-/Command-Policy-Test auf `desktop_files`,
  obwohl er keine Datei liest oder schreibt. Dynamic-Apps- und Audience-
  Policy-Gates starten deshalb keine Desktop-File-Replikation mehr; die
  eigentlichen Datei-, Chunk-, Documents- und Spreadsheets-Gates bleiben
  unveraendert verpflichtend. Der Matrix-Self-Test sichert diese Trennung.
  Vollstaendige lokale Phase-8-Fixture-Assets verhindern zusaetzlich, dass
  der Lifecycle-Reload synthetische App-Imports oder Icons mit 404 beantwortet.
  Der reale Zero-Retry-Dynamic-Apps-Lauf bestand danach in 69 Sekunden mit
  null Browserwarnungen, null Browserfehlern, null 404s und null
  fehlgeschlagenen Requests.
- Der vor dem konfliktfreien Rebase auf `origin/main` attestierte
  Office-Integrations-Snapshot
  `b54aec3529ca1203d2df128c63dc29038e162198` ist lokal vollstaendig
  reproduziert: 44 Rust-Engine-, 25 native Operations-, 47
  CTOX-Office-Integrations-, 44 Browser-Office-, 11 Documents-Wrapper- und 10
  Spreadsheets-Wrapper-Tests sind gruen. Die beiden echten
  Browser-zu-Rust-Restart-Smokes fuer Documents und Spreadsheets bestanden
  jeweils in exakt einem Versuch bei sauberem Checkout und ohne Browserfehler.
- Der erneut gestartete, exakt gepinnte Euro-Office-v9.3.1-Oracle bestaetigte
  am 2026-07-13 fuer `document.edit-save` und
  `spreadsheet.sort-filter-tables` den harten Side-by-Side-Gate: beide Seiten
  `document-ready`, identische iframe-Geometrie und Konfiguration,
  `web-apps`-/`sdkjs`-Fork-Provenienz sowie Rust-erzeugtes DOCY/XLSY. Der
  Spreadsheet-Lauf hatte null Consolefehler und null Warnungen. Die lokalen
  Test-Tabs, Server, der Oracle-Container und die Colima-VM wurden danach
  beendet.
- Die vollstaendigen Roundtrip-Aggregatoren wurden am selben Kandidaten erneut
  ausgefuehrt: DOCX 11 Fixtures/204 Package-Parts und XLSX 11 Fixtures/147
  Package-Parts sind gruen.
- Architektur, ESM-Closures, Bridge, native Rust-Engine, Persistenz und Oracle-
  Harness sind fuer die festgelegte DOCX-/XLSX-Business-Paritaetsstufe
  implementiert und durch die unten verlinkten Differential-/Rollout-Gates
  belegt.
- Editor- und Formatport: 24 von 24 Feature-Gruppen (100 %) sind
  zwischen den CTOX-Forks und dem gepinnten Oracle
  `differential_passed`.
- Die CTOX-Skill-Packs `doc` und `spreadsheet` verwenden ausschließlich die
  CTOX-Documents-/CTOX-Spreadsheets-Flächen. Ihr CI-Guard gleicht alle
  Featuregruppen mit `features.json` und alle 26 nativen Operationen mit dem
  Rust-CLI-Dispatcher ab. Editor-Flows bleiben bis `shipped` an denselben
  typisierten App-Rollout gebunden; der Skill darf keinen externen
  Dokument-/Workbook-Stack als Fallback verwenden.
- Der Produktionsbuild liefert ausschließlich `runtime/ctox-documents.mjs`,
  `runtime/ctox-spreadsheets.mjs` und den gemeinsamen Fork-Core aus. Die
  frueheren vereinfachten Renderer liegen klar getrennt unter
  `src/legacy-runtime` und werden nicht in das Produktbundle kopiert.
- Oracle-/Bootstrap-Evidenz: Documents 12/12, Spreadsheets 12/12 sind mindestens
  `oracle_captured`; diese Evidenz allein beweist keine UI-Paritaet.
- Der CTOX-Spreadsheets-Fork wird real aus Cell-`sdkjs`,
  SpreadsheetEditor-`web-apps`, statische Assets und gemeinsam 759
  inventarisierte Upstream-Eingaben liegen hinter der ESM-/iframe-Kapsel. Ein headed
  Browser-Smoke erreichte ohne Console-Fehler Toolbar, Formelzeile, Canvas,
  Seitenleisten und Sheet-Leiste des CTOX-Produkts.
- Der CTOX-Documents-Fork wird ebenfalls real aus Word-`sdkjs`
  und DocumentEditor-`web-apps` werden aus den exakten v9.3.1-Submodule-SHAs
  erzeugt, gemeinsam mit der Spreadsheet-Closure inventarisiert und ueber
  `ctox-office-document.mjs` in derselben same-origin iframe-Kapsel geladen.
  Die vorherige vereinfachte Documents-Runtime ist nicht mehr der
  Standardpfad. `document.open-render-zoom` hat das native Rust-DOCY-Payload
  und den gleich grossen Split-Screen aus Oracle und CTOX-Documents-Fork
  inzwischen bestanden.
- Fuer `document.open-render-zoom` ist nun auch das echte Oracle-`DOCY;v10`-
  Payload erfasst. Rust validiert Header, absolute Tabellenverzeichnis-Offsets,
  zehn Tabellenbereiche und Payload-Hash. Mit diesem erfassten Payload erreichen
  Oracle und CTOX hinter der Fork-ESM-Closure bei 799,5 × 813 px beide
  `document-ready`, ohne Console-Fehler und mit sichtbar gleicher Word-Shell.
  Der Harness markiert den Lauf trotzdem absichtlich als ungueltig
  (`ctox-editor-payload-not-rust-generated`): Er beweist den echten UI-Port,
  aber noch keinen nativen DOCX→DOCY-Writer.
- Der im Word-SDK vorhandene Browser-OOXML-Einstieg wurde ebenfalls praktisch
  geprueft und als Produktionspfad verworfen: Die gepinnte AGPL-Closure enthaelt
  den Aufruf, aber nicht die benoetigte `CDocument.fromZip`-Implementierung.
  CTOX aktiviert diesen unvollstaendigen Addon-Pfad nicht und bleibt fuer den
  Editorvertrag bei DOCY v10.
- Der erste native Rust-DOCX→DOCY-v10-Writer ist jetzt im echten
  `document.open-render-zoom`-Split-Screen aktiv. Er schreibt Paragraphen,
  Runs, direkte Zeichenformatierung, Abstaende, Zeilenmetriken,
  Style-Border-Flattening, Tabellenstruktur sowie Seitengroesse und -raender.
  Oracle und CTOX Documents erreichen `document-ready` ohne
  Browserfehler und stimmen auf allen drei Fixture-Seiten bei 100/120 Prozent
  sichtbar ueberein. Der versionierte Playwright-CLI-Flow betaetigt ausserdem
  auf beiden Seiten den echten Statusleisten-Button „Vergrößern (⌘+=)“ zweimal
  und erreicht mit jeweils drei echten PageDown-Tastendruecken identisch Seite
  2 und Seite 3. Status ist deshalb `differential_passed`.
- `document.edit-save` laeuft ebenfalls in gleich grossen Oracle- und
  CTOX-Documents-Instanzen. Der native Rust-Writer uebernimmt direkte
  Zeichenformatierung und die Default-Run-Eigenschaften aus `styles.xml`,
  wodurch Fixture und Toolbar vor und nach der Bearbeitung sichtbar pari sind.
  Der versionierte Browserflow selektiert die Zielzeile relativ zum gemessenen
  Canvas, schreibt denselben Text, betaetigt beidseitig den echten Save-Button
  und belegt Oracle-Force-Save Status 6 sowie einen sauberen CTOX-Commit. Rust
  exportiert das gespeicherte DOCY zurueck nach DOCX; nur
  `word/document.xml` aendert sich, alle 16 anderen Escrow-Parts bleiben
  bytegleich, und der Export oeffnet wieder im Oracle. Status ist
  `differential_passed`.
- `document.undo-clipboard-keyboard` ist auf derselben gepinnten UI-Closure
  differential abgenommen. Tastatur-Undo (`⌘+Z`), Tastatur-Redo (`⌘+Y`), die
  dieselben Toolbar-Aktionen sowie Copy/Cut/Paste erzeugen in Oracle und CTOX
  denselben sichtbaren Zustand. Der Clipboard-Pfad hat die zusaetzlichen
  gepinnten Font-Assets `104` und `105` als reale Closure-Abhaengigkeit
  aufgedeckt; beide liegen nun im reproduzierbaren ESM-Build und in der
  Provenienz. Save, DOCY-Reopen, nativer DOCX-Export und Oracle-Reopen sind
  bestanden. Status ist `differential_passed`.
- `document.character-paragraph-formatting` ist differential abgenommen. Ein
  Fehler in der DOCX→DOCY-Styleauflösung wurde dabei sichtbar und behoben:
  Werte des Default-Absatzstyles überschreiben nun korrekt `pPrDefault`, statt
  zusätzlichen Absatzabstand zu erzeugen. Danach stimmen Oracle und CTOX
  Documents pixelnah in Geometrie und Layout überein. Der native
  DOCY→DOCX-Rückweg dekodiert jetzt Fett, Kursiv, Unterstreichung, Schriftgrad,
  RGB-Farbe, Ausrichtung, linken Einzug und Zeilenabstand und schreibt diese
  Eigenschaften unter Erhalt unbeteiligter OOXML-Attribute zurück. Status ist
  `differential_passed`.
- `document.styles-lists-numbering`, `document.tables` und
  `document.images-positioning` sind inzwischen ebenfalls differential
  abgenommen. Besonders fuer Images/Positioning ist der fruehere Fehler
  geschlossen: Der Harness akzeptiert nur noch exaktes `document-ready`, nicht
  `app-ready`, und CTOX rendert die echten DOCX-Medien in der gepinnten
  CTOX-Documents-UI ueber same-origin Blob-URLs statt ueber einen nachgebauten
  Renderer. Der Split-Screen-Flow selektiert und bearbeitet Bilder ueber die
  geerbten `sdkjs`-Image-Property-APIs, speichert beide Seiten, exportiert
  den CTOX-DOCY-Payload nativ zu DOCX und oeffnet diesen Export wieder im
  Oracle. Status ist `differential_passed`.
- Die ersten zwei Spreadsheet-Laeufe sind jetzt `differential_passed`: Rust erzeugt aus
  dem kanonischen XLSX selbst ein `XLSY;v10`-
  Payload, das ueber die oeffentliche ESM-Fassade den CTOX-Spreadsheets-Fork und
  `document-ready` erreicht. Der gleich konfigurierte Split-Screen zeigt
  dieselbe Editorsemantik und Dokumentansicht. Runtime-, Rust-Origin-,
  Konfigurations- und Geometriepruefung sind gueltig; der Antialiasing-Diff
  liegt innerhalb der versionierten, maskenfreien Review-Toleranz. Nach dem
  bestandenen `document.docx-roundtrip-corpus` navigiert ein versionierter
  Browserflow auf beiden Seiten über den echten Statusleisten-Tab von
  `Overview` nach `Details`; Sheet-Reihenfolge, Hidden-State, Werte, Styles und
  Geometrie stimmen sichtbar und semantisch überein.
- Die Rust-Engine validiert und schreibt inzwischen das echte `XLSY;v10`-
  Payload selbst:
  Header, Versionsfeld, absolute Table-Directory-Offsets, acht Tabellenbereiche
  und Payload-Hash werden ohne DocumentServer inventarisiert. Der erste
  inhaltliche Decoder liest ausserdem die drei Worksheet-Namen, IDs und den
  ausgeblendeten Zustand von `Archive` sowie alle 18 Shared Strings inklusive
  der drei Preservation-Marker direkt aus der v10-Binaerstruktur. Der native
  XLSB-Recorddecoder folgt den drei `XlsbPos`-Verweisen und liest alle 22
  sichtbaren/ausgeblendeten Zellen mit Adresse, Typ, Wert und Style-ID; die
  Oracle-Werte `Overview!B4=125000`, `Details!B4=42` und `Archive!B4=Closed`
  stimmen mit dem OOXML-Fixture ueberein.
  Kanonisches OOXML und natives Editorpayload besitzen getrennte
  Prepare-Vertragsfelder. Der erste Writer-Slice portiert Workbook/Worksheets,
  Shared Strings, XLSB-Zellrecords, Fonts, Fills, Cell-XFs, Ausrichtung,
  Spaltenbreiten und Zeilenhoehen fuer `spreadsheet.open-render-sheets`. Der
  Writer. Fuer `spreadsheet.edit-save` transcodiert die ESM-Kapsel das echte
  `asc_nativeGetFile()`-v2-Transportpayload mit koerperrelativen Offsets in ein
  reopen-faehiges XLSY-v10 mit absoluten Offsets. Rust materialisiert geaenderte
  Shared Strings zurueck nach XLSX. Die danach featureweise implementierten
  Zell-, Style-, Formel-, Objekt- und Druckpfade sind in der finalen
  Spreadsheet-Matrix 12/12 differential abgenommen.
- Die Produktisierung der Browser-Forks ist abgenommen: eigene Manifeste und
  Runtime-Entry-Points melden `ctox-documents-fork` beziehungsweise
  `ctox-spreadsheets-fork`; iframe-Titel und Runtime-Provenienz tragen die
  CTOX-Produktidentitaet. Ein gemeinsames Business-OS-Chrome-Theme uebernimmt
  Systemschrift, Oberflaechen, Linien, Teal-Akzent, Fokuszustaende, Menues,
  Statusleisten, Light/Dark und Reduced Motion; das fremde Header-Logo und der
  About-Einstieg sind entfernt. Beide realen Apps haben die Light/Dark-,
  DE/EN- und Compact/Wide-Browsermatrix ohne Browserfehler bestanden.
- `office.spreadsheet.prepare` verwendet fuer den unterstuetzten Open/Render-
  Slice jetzt den nativen Writer, speichert das XLSY als eigenen gehashten
  Editor-Blob vor der Versionsprojektion und laesst `blob_id` weiterhin auf
  das kanonische XLSX zeigen. OOXML und Editorpayload werden nicht mehr
  faelschlich auf denselben Blob abgebildet.

Korrektur vom 2026-07-11: Die zuvor als `differential_passed` gefuehrten
Features verwendeten rechts einen CTOX-/SuperDoc-Bootstrap beziehungsweise
einen selbst gebauten Spreadsheet-Grid-Renderer. Sie wurden deshalb auf
`oracle_captured` zurueckgesetzt. Die vorhandenen Screenshots, Flows und
OOXML-Nachweise bleiben nur als Oracle-/Backend-/Bootstrap-Evidenz erhalten.

## Nicht verhandelbares UI-Port-Gate

Ein CTOX-Office-Frontend gilt nur dann als portiert, wenn die ausgefuehrte
Browser-Runtime nachweislich aus dem gepinnten Euro-Office-Source stammt:

- Das Build-Metafile muss reale Eingaben aus `web-apps` und `sdkjs` fuer den
  jeweiligen Editor enthalten. Ein Adapter ohne diese Eingaben ist kein Port.
- Toolbar, Canvas/Grid, Formelzeile, Sheet-Tabs, Kontextmenues, Dialoge,
  Tastatursteuerung und Styles muessen aus der Euro-Office-Closure stammen.
- Eine selbst gebaute, vereinfachte oder nur semantisch kompatible
  Ersatzoberflaeche ist als Zielruntime verboten. Sie darf hoechstens als
  explizit benannter Entwicklungsfallback existieren und zaehlt fuer keinen
  Feature-Status.
- Vor jedem visuellen Lauf muss die CTOX-Seite maschinenlesbar nachweisen, dass
  ihre aktive Runtime die gepinnte `web-apps`-/`sdkjs`-Closure ist. Selektoren,
  Runtime-Kennung und Build-Provenienz werden mit der Evidenz gespeichert. Ein
  CTOX-eigenes Grid, eine nachgebaute Toolbar oder ein anderer Fallback macht
  den Lauf unabhaengig vom sichtbaren Ergebnis ungueltig.
- Der gleich grosse Split-Screen-Vergleich ist ein hartes Abnahme-Gate. Jede
  nicht maskierte Abweichung in Shell, Toolbar, Editorflaeche, Tabs, Menues oder
  Dialogen setzt den Lauf auf `failed`; semantische Gleichheit kann diesen
  Fehler nicht ueberstimmen.
- `frontend_ported` erfordert Build-Provenienz plus visuellen Split-Screen-
  Nachweis. `differential_passed` erfordert zusaetzlich identische Interaktion,
  Semantik, Save/Rust/OOXML und beidseitigen Reopen.
- Screenshots duerfen nur deterministische transiente Elemente wie Caret,
  Zeitstempel und Benutzeravatar maskieren. Andere Masken benoetigen eine
  explizite, versionierte Review-Begruendung.

## Festgelegte Zielarchitektur

```text
Business OS Documents / Spreadsheets
        |
        v
ctox-office-document.mjs / ctox-office-spreadsheet.mjs
        |
        v  typisierter MessageChannel
CTOX Documents Fork / CTOX Spreadsheets Fork
        |
        v  Business-OS-Chrome + isoliertes same-origin Editor-iframe
gegliederter Fork-Source + gepinnte Upstream-Abhaengigkeitsclosure
        |
        v
CTOX Browser Bridge -> RxDB/WebRTC -> Business-OS-Command-Pipeline
        |
        v
CTOX Rust Office Engine -> DOCX/XLSX, Versionen und Blob-Chunks
```

Produktionsregeln:

- Keine Euro-Office-Serverdienste, Datenbanken, Queues oder C++-Runtime.
- Keine Browser-Business-Daten ueber HTTP; HTTP liefert nur statische Assets,
  Bootstrap- und erlaubte Control-Plane-Daten.
- Kein automatisches Clonen, Fetchen oder Aktualisieren von Upstream.
- CTOX Documents und CTOX Spreadsheets besitzen getrennte Produktmanifeste,
  Runtime-Entry-Points und Browser-Evidenz. Gemeinsame Low-Level-Adapter duerfen
  im Fork-Core liegen; Produktidentitaet, UI-Chrome und Release-Gates bleiben
  getrennt.
- CTOX-spezifische Upstream-Aenderungen liegen als reviewbare Patches im
  Repository. `apply-ctox-office-upstream-patches.mjs` prueft vor Anwendung die
  exakten `sdkjs`-/`web-apps`-SHAs und arbeitet idempotent; der Office-Build
  bricht ohne gepatchten, gepinnten Source-Checkout ab und schreibt Patch-Pfad,
  SHA-256 und Status in die Bundle-Provenienz.
- Keine manuell bearbeiteten Monolith-Bundles; Source bleibt nach
  `web-apps`, `sdkjs`, `adapters` und `styles` gegliedert.
- Unverstandene OOXML-Parts werden aus der Originaldatei unveraendert erhalten.
- Jeder Save ist konfliktgeprueft und referenziert `base_version_id`.

## Arbeitsstroeme

| ID | Arbeitsstrom | Stand | Abnahmekriterium |
|---|---|---|---|
| A1 | Upstream-Pin und Lizenzinventar | abgenommen: v9.3.1, Commit, drei Source-SHAs, Container-Digest und AGPL-Inventar stehen in der reproduzierbaren Provenienz | Tag, Commit, Submodule-SHAs, Container-Digests und Lizenzen reproduzierbar |
| A2 | Fork-Source und reproduzierbare ESM-Builds | getrennte `ctox-documents`-/`ctox-spreadsheets`-Manifeste und Runtime-Entry-Points, gemeinsamer Fork-Core, gepinnte Abhaengigkeitsclosure und vollstaendige Hash-Provenienz sind implementiert | beide CTOX-Produkt-Entry-Points, reale `web-apps`-/`sdkjs`-Inputs, Fork-Source, Hashes, Provenienz und verbotene Closure-Pruefung |
| A3 | ESM-Fassade und iframe-Kapsel | abgenommen: produktspezifische Factorys `createCtoxDocumentsEditor()` und `createCtoxSpreadsheetsEditor()`, kompatibler stabiler Factory-Alias, Ready, Save, getrennte typisierte Remounts, Live-Theme-Weitergabe, produkt-eigener Titel, idempotenter MessageChannel-/iframe-Teardown und Reload | stabile Factory/API, CTOX-Fork-Runtime im iframe, MessageChannel, Lifecycle, Theme, Teardown und redigiertes `inspect()` |
| A3b | Business-OS-Produkt-UI | abgenommen: eigene Produkt-IDs/Titel/Manifeste, Business-OS-Fork-Chrome, entferntes Fremdbranding und vollständige 8-Fall-Matrix beider echten App-Mounts in Light/Dark, DE/EN und 360/640/1600 px; Live-Theme-Wechsel und null Browserfehler | [UI-Evidenz](../src/apps/business-os/office-engine/oracle/evidence/office.fork-business-os-ui.json) und [Browserflow](../src/apps/business-os/office-engine/oracle/flows/office.fork-business-os-ui.playwright.js) bleiben Pflichtgate |
| A4 | Business-OS-Bridge | abgenommen: beide Apps importieren ihre produktspezifische Factory und fuehren getrennte `ctox-documents`-/`ctox-spreadsheets`-Lifecycle-Handles; beide verwenden `ctx.db`, `ctx.commandBus`, Permissions und den RxDB/WebRTC-Sync; Integritaets-, Offline- und Restart-Gates bestehen ohne HTTP-Business-Datenpfad | ausschliesslich `ctx.db`, `ctx.commandBus`, Permissions und Presence; kein HTTP-Datenpfad |
| A5 | Rust Office Engine | OOXML-Grundlage, DOCY-/XLSY-v10-Parser und native Hin-/Rueckwege fuer alle 24 Featuregruppen; der zuvor faelschlich auf Documents verdrahtete Spreadsheet-Commit besitzt jetzt eigene XLSX-Version-, Blob-, Hash-, Konflikt- und Replay-Persistenz; komplexe allgemeine Writer bleiben ausserhalb der Business-Paritaet offen | Prepare, Apply, Export, Inspect, Hashing und OOXML-Escrow mit Unit-/Corpus-Tests |
| A6 | Persistenz und Konfliktmodell | abgenommen: atomare Business-Store-Commits, direkte terminale RxDB-Projektion, periodische Reconciliation, Hashpruefung, stale-base-Ablehnung, idempotenter Replay und Store-Reopen fuer DOCX/XLSX | idempotente Blob-/Versionsfolge, Hashpruefung, stale-base-Ablehnung und Reconciliation |
| A7 | Oracle-Harness | abgenommen; Split-Screen-Gate akzeptiert nur exaktes `document-ready`, prueft reale Euro-Office-Frames und ist fuer alle 24 Features samt Reopens belegt | gepinntes Oracle, deterministische Fixtures, harter visueller Split-Screen-Gate, Screenshots und Strukturvergleich |
| A8 | Documents-Format-/Feature-Paritaet | abgenommen: alle 12/12 vertikalen Features, nativer DOCX→DOCY→DOCX-Gesamtkorpus, Produkt-UI-Matrix und echter Business-OS-Mount | Release-Gates bis zur Legacy-Ablösung grün halten |
| A9 | Spreadsheets-Format-/Feature-Paritaet | abgenommen: alle 12/12, nativer 11-Fixture-/147-Part-XLSX-Gesamtkorpus, CTOX-/Oracle-Reopens, Produkt-UI-Matrix und echter Business-OS-Mount | Release-Gates bis zur Legacy-Ablösung grün halten |
| A10 | Rollout und Legacy-Abloesung | technische Rollout-Matrix bestanden: Chunk-/Hashintegritaet, Rechte, Konfliktweitergabe, Browser-Offline/Reconnect, Clean Profile, Reload, Locale, Shellstil, typisierter Engine-Rollback, Teardown und echter Office-Save ueber einen CTOX-/Peer-Neustart sind belegt; `ctox_documents` bzw. `ctox_spreadsheets` sind die typisierten Standards; Skill-Feature-/Rust-Op-Drift wird fail-closed geprueft; das Release-Gate erzeugt und archiviert ein hashgebundenes Kandidaten-Artefakt | reales Switch-Release ab mindestens `v0.3.32` und eine nachfolgende stabile Release-Periode beobachten; Legacy erst danach entfernen |

Die technischen Arbeitsstroeme A1 bis A9 sind abgenommen. A10 bleibt bis zum
maschinenlesbar definierten realen Release-Zeitfenster und der erlaubten
Legacy-Entfernung offen. Der blockierende Gesamtstatus steht zusaetzlich im
[`completion-audit.json`](../src/apps/business-os/office-engine/completion-audit.json);
`validate-completion-audit.mjs` gleicht ihn in `check:office` gegen Fork-
Artefakte, UI-Matrix, Feature-Evidenz, Business-OS-Mounts, Rust-Befehle,
Korpora und Rollout-Vertrag ab.

## Feature-Status: Documents

| Nr. | Feature-ID | Status | Abhaengigkeit | Evidenz / naechste Aktion |
|---:|---|---|---|---|
| 1 | `document.open-render-zoom` | `differential_passed` | - | Originale ESM-UI beidseitig `document-ready`; rechts liegt ein 4.854-Byte-DOCY aus dem nativen Rust-Writer. Seite 1 bei 100/120 Prozent sowie Seiten 2/3 bei 120 Prozent sind visuell pari. Der versionierte Playwright-CLI-Flow verwendet den echten Zoom-Button und zweimal drei reale PageDown-Tastendruecke; Oracle und CTOX melden identisch 120 %, Seite 2 und Seite 3. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.open-render-zoom.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.open-render-zoom.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.open-render-zoom.playwright.js) |
| 2 | `document.edit-save` | `differential_passed` | 1 | Originale DocumentEditor-UI, Default- und Direktformatierung sichtbar pari; gemessene Canvas-Geometrie 687×617 px je Seite. Derselbe End/Shift+Home/Text-Flow und der echte Save-Button liefern Oracle-Status 6 sowie CTOX-Version v2 mit `dirty=false`. Das 243.910-Byte-DOCY wird nativ nach DOCX exportiert, nur `word/document.xml` aendert sich, und der Export oeffnet im Oracle. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.edit-save.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.edit-save.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.edit-save.playwright.js) |
| 3 | `document.undo-clipboard-keyboard` | `differential_passed` | 2 | Originale DocumentEditor-UI bei identischer 687×617-Canvas-Geometrie. `⌘+Z`, `⌘+Y`, originale Undo-/Redo-Toolbar sowie Copy/Cut/Paste liefern sichtbar denselben Zustand mit zwei Zielmarkern. Das 244.313-Byte-DOCY oeffnet erneut in CTOX; Rust exportiert ein 35.379-Byte-DOCX, veraendert nur `word/document.xml`, und das Oracle oeffnet den Export. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.undo-clipboard-keyboard.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.undo-clipboard-keyboard.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.undo-clipboard-keyboard.playwright.js) |
| 4 | `document.character-paragraph-formatting` | `differential_passed` | 3 | Originale UI und initiales Layout nach Korrektur der Default-Style-Kaskade pari. Echte Toolbar-Aktionen setzen Fett, Kursiv, Unterstreichung, 18 pt, `953735`, Zentrierung, 709 Twip Einzug und 360 Twip/Auto-Zeilenabstand identisch. Das gespeicherte 245.515-Byte-DOCY öffnet in CTOX; der native 35.581-Byte-DOCX-Export enthält die exakten OOXML-Eigenschaften, erhält den 240-Twip-Kontrollabstand und öffnet im Oracle. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.character-paragraph-formatting.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.character-paragraph-formatting.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.character-paragraph-formatting.playwright.js) |
| 5 | `document.styles-lists-numbering` | `differential_passed` | 4 | Originale DocumentEditor-UI im Same-Origin-Split-Screen auf beiden Seiten; keine vereinfachte CTOX-Oberfläche. Der echte Toolbar-/Style-Gallery-Flow setzt Heading 1, Quote, Bullet-Nesting und Nummerierungsfortsetzung sichtbar pari; die terminale Toolbar meldet auf Oracle und CTOX `Heading 1`, Calibri 14. Rust schreibt Numbering- und Style-DOCY-Tabellen, berücksichtigt `latentStyles`, Theme-Fonts und direkte-vs.-Default-RunProperties. Der 365.784-Byte-CTOX-Editorpayload (`3ad1708e8e39784b502add62fb853d3002833fdc4b3a97f32856eae8138c8a76`) exportiert nativ zu einem 35.610-Byte-DOCX (`9bc2f7bff05ae8fa93a6369c418c61d4b59860d74aa4a0717a1879dddf8b3b1f`); nur `word/document.xml` und `word/numbering.xml` ändern sich, 15/17 Originalparts bleiben bytegleich, und der Export öffnet in CTOX sowie im Oracle. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.styles-lists-numbering.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.styles-lists-numbering.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.styles-lists-numbering.playwright.js) |
| 6 | `document.tables` | `differential_passed` | 5 | Originale DocumentEditor-Runtime im Same-Origin-Split-Screen auf beiden Seiten. Der eingecheckte Playwright-CLI-Flow nutzt das echte Tabellen-Kontextmenü für Zeilen-/Spalteneinfügung, echte Tastatureingabe für den Zelltext und dieselbe Runtime-Semantik für Merge/Split; Oracle und CTOX enden bei `mainShape=[4,4,4,4]`, `mergeShape=[2,2,2]`, `TABLE_EDITED_VALUE` und erhaltenem `NESTED_A1`/`NESTED_B2`. Oracle liefert terminale Save-Callbacks (`status 2/6`, saved 40.952 Byte, `c02be256fd7c2b3eeb9407dba4db3989695cd6a12dcec0305696aeaef3c0105f`). Der 370.119-Byte-CTOX-Editorpayload (`387262d0456a80d1fc396ca69e437a66e3d715e6859c1832c5a482cba5510266`) exportiert nativ zu einem 36.025-Byte-DOCX (`f0f8ca4c7ba1eea5c2e6524c668c531c76e6dc9158f24762421ed4927dd1f326`); nur `word/document.xml` ändert sich, 17/18 Originalparts inklusive `customXml/ctox-table-preserve.xml` bleiben erhalten, CTOX-Reopen und Oracle-Reopen sind `document-ready`. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.tables.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.tables.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.tables.playwright.js) |
| 7 | `document.images-positioning` | `differential_passed` | 6 | Originale DocumentEditor-Runtime im Same-Origin-Split-Screen auf beiden Seiten. Der Harness akzeptiert nur noch exaktes `document-ready`, damit `app-ready` nie als gueltige UI-Paritaet zaehlt. Die CTOX-ESM-Kapsel extrahiert `word/media/*` aus dem kanonischen DOCX, registriert same-origin Blob-URLs und rendert die echten DOCX-Bilder in der Euro-Office-UI ohne `/media/image*.png`-HTTP-Requests. Der eingecheckte Browserflow selektiert Inline- und Floating-Bild ueber gemessene Editor-Overlays, setzt Groesse, Square-Wrap und Position ueber originale `sdkjs`-Image-Property-APIs, speichert beide Seiten, captured den 364.909-Byte-CTOX-DOCY-Payload (`ffdfc104a4a05f15b5b58fe4741ea7672873a95ef9b9ca8cc123fd426982da00`) und exportiert ihn nativ zu einem 37.457-Byte-DOCX (`bd64c0283a6c227242e6e2c294795719c383bee958094acc16763ee5036e3597`). Nur `word/document.xml` aendert sich; 21/22 Originalparts inklusive Medien, CustomXML-Preservation und Relationships bleiben erhalten. CTOX-Reopen und Oracle-Reopen sind `document-ready`. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.images-positioning.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.images-positioning.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.images-positioning.playwright.js) |
| 8 | `document.sections-headers-footers` | `differential_passed` | 7 | CTOX Documents und Oracle im Same-Origin-Split-Screen. Der Clean-Key-Browserflow bedient Section-Setup, den zusätzlichen Next-Page-Abschnittsumbruch über die echte Runtime-API `add_SectionBreak` und die Link-to-Previous-Semantik über `HeadersAndFooters_LinkToPrevious(false)`. Oracle und CTOX starten bei `Seite 1 von 2` und enden beide bei `Seite 3 von 3` mit drei `nextPage`-Sections und gleicher Section-2-Default-Header-Semantik. Der aktuelle CTOX-Capture ist ein 365.179-Byte-DOCY (`6f643a61b41e999b3d99226dc3d9944d9a40b1c9638509d332fe358517af9a71`); der native Export erzeugt ein 37.361-Byte-DOCX (`4250111a41f9cddbad967f65594f1fc6f196cd3d75a6454d0cbe37f20c343abf`) mit drei `w:sectPr`-Blöcken, erhaltenen Section-Refs, neu materialisiertem `word/header3.xml`, neuer `document.xml.rels`-Relationship und `[Content_Types].xml`-Override. Keine Originalparts fehlen; `word/header3.xml` ist der beabsichtigte neue Part. CTOX-Reopen des gespeicherten Payloads und Oracle-Reopen des nativen Exports sind `document-ready`; im Clean-Key-Run wurden keine Browser-Console-Errors erfasst. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.sections-headers-footers.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.sections-headers-footers.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.sections-headers-footers.playwright.js) |
| 9 | `document.links-bookmarks-fields` | `differential_passed` | 8 | CTOX Documents und Oracle im Same-Origin-Split-Screen. Der echte Runtime-Flow selektiert dieselben Marker und verwendet `CHyperlinkProperty`/`add_Hyperlink`, `asc_GetBookmarksManager`/`asc_AddBookmark` und `UpdateAllFields`. Oracle und CTOX enden sichtbar identisch mit `CTOX_EXTERNAL_LINK`, beiden Bookmarks und NUMPAGES `1`. Rust schreibt und liest Hyperlink-, Bookmark-, FieldChar- und InstrText-DOCY-Records und materialisiert die neue externe Relationship beim DOCX-Export. Der 364.980-Byte-CTOX-Payload (`6f99b134fd5af0a7d52ede187059d87b45e0b1df3a49c6163a1cb4be5f2a6973`) exportiert zu 36.017 Byte (`ad352ab948857358cfc5989940fea992633a39b8b1448d6acd981abbfefbe800`); nur `word/document.xml` und `word/_rels/document.xml.rels` ändern sich, 16/18 Parts bleiben bytegleich, keine Parts fehlen oder kommen unbeabsichtigt hinzu. CTOX-Reopen und Oracle-Reopen sind `document-ready`, ohne Browserfehler. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.links-bookmarks-fields.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.links-bookmarks-fields.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.links-bookmarks-fields.playwright.js) |
| 10 | `document.comments-track-changes` | `differential_passed` | 9 | Oracle und die CTOX-Documents-Fork-UI in der CTOX-ESM-Kapsel erzeugen über den sichtbaren Track-Changes-Schalter und reale Tastatureingaben denselben Kommentar-Thread sowie dieselbe Accept/Reject-Semantik. Der native DOCY→DOCX-Export materialisiert drei Kommentare, Antwort-/Resolved-Metadaten und genau eine verbleibende Einfügung; CTOX- und Oracle-Reopen im Review-Modus sind bestanden ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.comments-track-changes.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.comments-track-changes.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.comments-track-changes.playwright.js)) |
| 11 | `document.drawings-charts` | `differential_passed` | 10 | Oracle und die CTOX-Documents-Fork-UI wählen Shape und Diagramm über gemessene Editorflächen und setzen über die gepinnten `Asc`-Objekt-APIs identisch Füllung `F4B183`, Rotation 90°, Diagrammgröße 14×7 cm und UI-Stil 102. Der 368.930-Byte-CTOX-DOCY-Payload wird nativ zu einem 42.530-Byte-DOCX exportiert; nur `word/document.xml` und `word/charts/chart1.xml` ändern sich, alle 19 übrigen Parts inklusive eingebettetem Workbook und CustomXML-Escrow bleiben bytegleich. Saved-DOCY-, CTOX-Export- und Oracle-Reopen bestehen jeweils den gleich großen Split-Screen ohne Browserfehler. ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.drawings-charts.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.drawings-charts.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/document.drawings-charts.playwright.js)) |
| 12 | `document.docx-roundtrip-corpus` | `differential_passed` | 11 | Alle elf Feature-Fixtures (419.713 Byte, 204 deklarierte Parts) durchlaufen jetzt tatsächlich `DOCX → nativer Rust-DOCY-v10 → nativer Rust-DOCX-Export`; Primärsemantik, sämtliche Originalparts und alle deklarierten Escrow-Parts bleiben erhalten. Der Korpus-Validator bestätigt 11/11 aktuelle Feature-Exporte samt CTOX-/Oracle-Reopen, Clean-Profile-, Console- und HTTP-Datengrenzen; 32 Rust- und 24 Browser/Bridge-Tests bestehen. ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.docx-roundtrip-corpus.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/document.docx-roundtrip-corpus.json)) |

## Feature-Status: Spreadsheets

| Nr. | Feature-ID | Status | Abhaengigkeit |
|---:|---|---|---|
| 1 | `spreadsheet.open-render-sheets` | `differential_passed` | `document.docx-roundtrip-corpus` |
| 2 | `spreadsheet.edit-save` | `differential_passed` | 1 |
| 3 | `spreadsheet.undo-clipboard-fill` | `differential_passed` | 2 |
| 4 | `spreadsheet.cell-format-rows-columns` | `differential_passed` | 3; originale UI, nativer XLSY→XLSX-Rückweg und Business-OS-Mount bestanden |
| 5 | `spreadsheet.formulas-references` | `differential_passed` | 4; originale Formelzeile/Clipboard-UI, nativer Formel-/Cache-Rückweg und Business-OS-Mount bestanden |
| 6 | `spreadsheet.multi-sheet-merge-freeze` | `differential_passed` | 5; originale Sheet-Tabs und Merge-Toolbar, nativer XLSY-Pane-/Merge-Rückweg, Oracle-/CTOX-Reopen und Business-OS-Mount bestanden ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.multi-sheet-merge-freeze.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.multi-sheet-merge-freeze.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.multi-sheet-merge-freeze.playwright.js)) |
| 7 | `spreadsheet.sort-filter-tables` | `differential_passed` | 6; originale Tabellen-/Filter-UI im Split-Screen, nativer XLSY-TableParts-/AutoFilter-/SortState-Rückweg, CTOX-/Oracle-Reopen und Business-OS-Mount bestanden ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.sort-filter-tables.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.sort-filter-tables.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.sort-filter-tables.playwright.js)) |
| 8 | `spreadsheet.validation-conditional-formatting` | `differential_passed` | 7; originaler Datenüberprüfungsdialog, native Validation-/ConditionalFormatting-/Dxf-Records, CTOX-/Oracle-Reopen und Business-OS-Mount bestanden ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.validation-conditional-formatting.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.validation-conditional-formatting.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.validation-conditional-formatting.playwright.js)) |
| 9 | `spreadsheet.comments-names-protection` | `differential_passed` | 8; originale Kommentar-Seitenleiste, Name-Manager und Schutz-Buttons im 799,5×813-px-Split-Screen; nativer XLSX→XLSY→XLSX-Rückweg für zwei Kommentare, zwei VML-Notes-Shapes, Defined Names und Worksheet-/Workbook-Protection; CTOX-/Oracle-Reopen und echter `modules/spreadsheets.mount(ctx)`-Commit über RxDB/WebRTC bestanden ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.comments-names-protection.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.comments-names-protection.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.comments-names-protection.playwright.js)) |
| 10 | `spreadsheet.charts` | `differential_passed` | 9; deterministische 16-Part-XLSX mit echtem Clustered-Column-Chart rendert und selektiert in beiden originalen SpreadsheetEditor-Frames. Der Split-Screen deckte das zunächst fehlende native Diagramm mit rotem Gate auf. Rust schreibt und liest jetzt nach den Original-`sdkjs`-Ankern echte Two-Cell-, `pptxDrawing`-, GraphicFrame-, Xfrm-, ChartSpace-, Series-Cache-, Solid-Fill- und Style-Records. Der sichtbare Original-UI-Flow setzt 14×8 cm und Stil 2; der native Export verändert ausschließlich `xl/charts/chart1.xml` und `xl/drawings/drawing1.xml`, erhält 14/16 Parts bytegleich und öffnet mit gleicher Geometrie in CTOX und im Oracle. Der echte `modules/spreadsheets.mount(ctx)`-Flow committed 17.631 Byte XLSY über `office.spreadsheet.commit`, `rxdb-webrtc`, `until=terminal` und endet mit `Gespeichert`. ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.charts.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.charts.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.charts.playwright.js)) |
| 11 | `spreadsheet.pivot-print-layout` | `differential_passed` | 10; deterministische 18-Part-XLSX mit echter Pivot-Cache-/PivotTable-Struktur, Druckbereich, Drucktiteln, A4-Querformat, Fit-to-page, Kopf-/Fußzeilen und manuellen Umbrüchen. Rust schreibt und liest die Original-XLSY-Records für Pivot-Caches, PivotTable-XML, SheetPr/View, PageMargins/PageSetup/PrintOptions, HeaderFooter und Breaks. Der sichtbare Original-UI-Flow benennt die Pivot-Tabelle in `CTOXRevenuePivot2026` um und aktiviert „Erste Seite anders“; der native Export verändert nur `xl/pivotTables/pivotTable1.xml` und `xl/worksheets/sheet1.xml`, erhält 16/18 Parts bytegleich und öffnet in CTOX und Oracle. Der echte Business-OS-Mount committed 8.427 Byte XLSY über `office.spreadsheet.commit`, `rxdb-webrtc`, `until=terminal` und endet mit `Gespeichert`. ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.pivot-print-layout.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.pivot-print-layout.json), [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.pivot-print-layout.playwright.js)) |
| 12 | `spreadsheet.xlsx-roundtrip-corpus` | `differential_passed` | 11; versionierter Corpus umfasst alle elf Feature-Fixtures und 147 OOXML-Package-Parts. Der Rust-Gate führt jede Fixture über XLSX→XLSY→XLSX und verlangt bei einem Identity-Edit vollständige Bytegleichheit jedes ursprünglichen Parts. Der Aggregator prüft Fixture-Hashes, alle elf Feature-Evidenzen, die Provenienz der CTOX-Fork-UI, CTOX-/Oracle-Reopens, Business-OS-RxDB/WebRTC-Nachweise und Browser-Datengrenzen. Dabei wurde ein unnötiges Rewrite unveränderten Worksheet-Schutzes gefunden und behoben. ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.xlsx-roundtrip-corpus.json), [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.xlsx-roundtrip-corpus.json), [Corpus](../tests/fixtures/office/spreadsheet/corpus.json)) |

## Verbindlicher Ablauf pro Feature

1. Deterministisches Fixture erstellen und hashen.
2. Fixture im gepinnten Euro-Office-Oracle oeffnen.
3. Vollstaendige Benutzerinteraktion mit headed Browser-/Computer-Use
   aufzeichnen.
   Dabei wird der Oracle-/CTOX-Flow zusaetzlich im gleich grossen
   Zwei-iframe-Harness `oracle/side-by-side.html` ausgefuehrt, sofern die
   Interaktion nicht durch einen nativen Dialog getrennte Fenster erfordert.
4. Screenshots, Browser-Snapshot, Requests, Console, Export und semantischen
   Zustand erfassen.
5. Relevante `web-apps`-/`sdkjs`-Source-Anker dokumentieren.
6. Stabilen Playwright-CLI-/Node-Flow versionieren.
7. Die Browsersemantik aus der gepinnten `web-apps`-/`sdkjs`-Closure in den
   jeweiligen CTOX-Fork portieren; keine vereinfachte Ersatzoberflaeche. Die
   produktbezogene Chrome wird mit den Business-OS-Komponenten und -Tokens
   umgesetzt.
8. Import, Aenderung, Export und Preservation in Rust implementieren.
9. Identischen Flow gegen CTOX ausfuehren.
10. CTOX-Export in CTOX und Oracle erneut oeffnen.
11. Normalisierte OOXML-Struktur und unbeteiligte Package-Parts vergleichen.
12. Status nur mit vollstaendiger Evidenz weiterbewegen:
    `discovered -> oracle_captured -> frontend_ported -> rust_ported -> differential_passed -> shipped`.

Oracle-Capture darf unabhaengig vorbereitet werden. Ein Feature darf jedoch
erst auf `frontend_ported` oder hoeher gesetzt werden, wenn alle Eintraege in
`depends_on` mindestens `differential_passed` sind. Der Validator unter
[`validate-feature-matrix.mjs`](../src/apps/business-os/office-engine/oracle/validate-feature-matrix.mjs)
erzwingt diese Reihenfolge.

## Qualitaets- und Rollout-Gates

Fuer `differential_passed` sind verpflichtend:

- Build-Provenienz mit mindestens einem realen `web-apps`- und einem realen
  `sdkjs`-Input fuer den betroffenen Editor; `adapter-foundation-only` ist ein
  automatischer Fehlschlag,
- vollstaendige Editor-Aktions- und Informationsarchitektur: Toolbar,
  Formelzeile, Canvas/Grid, Tabs, Menues und Dialoge; kein Feature darf durch
  die Business-OS-Anpassung verschwinden,
- bestandener Split-Screen-Vergleich bei identischem Browser, Viewport, Device
  Scale, Fonts und Locale. Dokument-/Sheet-Inhalt, Layoutgeometrie und
  Interaktionswirkung werden gegen das Oracle differential verglichen. Die
  absichtlich geaenderte Produkt-Chrome wird nicht pixelgleich zum Oracle
  verlangt, sondern gegen versionierte Business-OS-Referenzscreenshots,
  Tokenwerte, Control-Inventar und Accessibility-Snapshots geprueft. Es gibt
  keine pauschale Screenshot-Maske fuer die Editorflaeche,
- maschinenlesbarer Nachweis auf beiden Seiten, dass Oracle und CTOX dieselbe
  gepinnte Euro-Office-Version, dieselbe Fixture, denselben Modus und dieselben
  UI-Konfigurationswerte verwenden; ein Vergleich von View- gegen Edit-Modus
  oder unterschiedlichen Customization-Optionen ist ungueltig,
- gleiche Benutzeraktion und sichtbare Wirkung im Oracle und in CTOX,
- gleicher normalisierter semantischer Zustand,
- Save und Reopen in CTOX,
- Reopen des CTOX-Exports im Oracle,
- keine unbeabsichtigte Entfernung unbeteiligter OOXML-Parts,
- keine Browserfehler oder fehlgeschlagenen Requests,
- kein HTTP-Pfad fuer Dokument- oder Business-Daten.

Fuer die Produkt-UI-Abnahme von CTOX Documents und CTOX Spreadsheets kommen
verpflichtend hinzu:

- eigene CTOX-Produkt-ID, eigener iframe-Titel, eigener Runtime-Entry-Point und
  eigenes Fork-Manifest,
- kein Euro-Office-Logo, About-Einstieg oder fremder Produktname in der
  Business-OS-Oberflaeche,
- Business-OS-Systemschrift, Oberflaechen-, Linien-, Akzent-, Fokus- und
  Statusrollen ohne app-eigene Parallelpalette,
- Light/Dark, Reduced Motion, DE/EN, Tastaturfokus und WCAG-2.2-AA-Kontrast,
- Containerbreiten 360, 640 und 1600 Pixel ohne Verlust essentieller Menues,
  Bearbeitungsaktionen, Auswahl oder Rueckweg,
- echter Mount in `modules/documents.mount(ctx)` beziehungsweise
  `modules/spreadsheets.mount(ctx)`, nicht nur ein isolierter Editor-Harness.

Vor `shipped` kommen zusaetzlich hinzu:

- Rust-Unit- und Corpus-Tests,
- ESM-API-, Lifecycle-, Permission-, Autosave- und Teardown-Tests,
- Cross-Language-Fixtures zwischen JavaScript und Rust,
- Offline/Reconnect, Daemon-Neustart, Reload und Clean-Profile,
- unvollstaendige Chunks, Hashfehler, stale base version und fehlende Rechte,
- Deutsch/Englisch sowie Windows-/macOS-Shellstil,
- bestandener kompletter DOCX- beziehungsweise XLSX-Korpus.

## Aktuelle Evidenz

`document.open-render-zoom` wurde am 2026-07-10 gegen Euro-Office v9.3.1
als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- Oracle-Container-Index:
  `sha256:bb7ba0a9609f395c7848f48f86c3219f187d804a28239ec949bc5e97e1e778a5`
- verwendeter ARM64-Plattform-Digest:
  `sha256:18703d2e72088b4df56caaa683dc17c67088e78c157f461d45ac41128fd66035`
- Fixture/CTOX-Export: 37.299 Byte,
  `sha256:3962efeaec4d1477a7b88b7cbbaad70b20ae26ff46085cf84a2b500e03a111fa`
- Browserflow: Seite 1 bei 100 Prozent, Zoom auf 120 Prozent, Navigation bis
  Seite 3, `dirty=false`, keine Console-Fehler oder -Warnungen.
- JavaScript Office-Suite: 3 von 3 Tests bestanden.
- Rust Office-Engine: 3 von 3 gezielten Tests bestanden.

`document.edit-save` wurde am 2026-07-11 als echter ESM-/Rust-Differentiallauf abgenommen:

- deterministisches Fixture: 36.863 Byte,
  `sha256:26cbc4974cb1ab2dbc8bb8ca64809b92b8e01fbdf40c6955a62d863bc517ecdd`,
- identische Dokumentformatierung und Editorsemantik zwischen CTOX und Oracle im harten
  Split-Screen-Gate; kein vereinfachter CTOX-Nachbau,
- identische Selektion per `End` und `Shift+Home` sowie identische Texteingabe
  relativ zu den gemessenen Editor-Canvas-Rechtecken,
- Dirty-Zustand nach Eingabe und sauberer Zustand nach Save,
- Save und Reopen in CTOX sowie Reopen des CTOX-Exports im Oracle bestanden,
- gespeichertes DOCY: 243.910 Byte,
  `sha256:2c23f0f728ddac7cdcac0bb4e04b8aaf0917f867103f62867bd515de9cc154a9`,
- kanonischer CTOX-Export: 35.367 Byte,
  `sha256:4502a2141da9e14292078bd877b5b1c8f7ddfd620176a30bea8180ccf7bcaced`,
- nur `word/document.xml` geaendert; 16 von 17 Original-Parts inhaltlich
  bytegleich, keine Original-Parts entfernt und keine Parts hinzugefuegt,
- versionierter Playwright-CLI-Differentialflow bestanden; Oracle-Callback
  `[1, 6]`, CTOX-Commit v2 und final `dirty=false`,
- JavaScript-Matrix/Office-Suite: 14 von 14 Tests bestanden,
- eigenstaendige Rust Office Engine: 20 Library- und 25 CLI-/Operations-Tests
  bestanden, einschliesslich vollständigem Dokument-Corpus und Escrow-Test.

`document.undo-clipboard-keyboard` wurde am 2026-07-11 als echter ESM-/Rust-Differentiallauf abgenommen:

- deterministisches Fixture: 36.890 Byte,
  `sha256:73b3e2525c26ab7b75f3adba363b18b23e8183bf3e5e90537e866a562626f7a2`,
- identische Original-Euro-Office-Oberflaeche, Eingabe sowie Undo/Redo per
  macOS-Tastaturkuerzel im Oracle und in CTOX; auch die originalen lokalisierten
  Toolbar-Aktionen liefern denselben Zustand,
- Copy, Cut und Paste ersetzen die Zielzeile reproduzierbar durch zwei
  Instanzen von `UNDO_CLIPBOARD_BASE_ONE`,
- Save und Reopen in CTOX sowie Reopen des CTOX-Exports im Oracle bestanden,
- gespeichertes DOCY: 244.313 Byte,
  `sha256:adce388b4a0a2106156142b0dd841c2d89c681c4ccdc28b0e9d7325929cf28a3`,
- kanonischer CTOX-Export: 35.379 Byte,
  `sha256:0a079216050651df2426e8cfe8d5cb93c48bff0d46e17120e39a0487f5060521`,
- der echte Clipboard-Pfad benoetigt die gepinnten Font-Binaries `104` und
  `105`; beide sind in Build-Provenienz und Runtime-Closure enthalten,
- nur `word/document.xml` geaendert; 16 von 17 Original-Parts bytegleich,
  keine Original-Parts entfernt und keine Parts hinzugefuegt,
- ein vollstaendiger Clean-Profile-Flow mit Eingabe, Undo, Redo, Clipboard,
  Save und Reopen ist ohne Console-Fehler, Warnungen oder Request-Fehler
  durchgelaufen,
- JavaScript-Matrix/Office-Suite: 15 von 15 Tests bestanden; Rust-Baseline
  unveraendert 20 Library- und 25 CLI-/Operations-Tests bestanden.

`document.character-paragraph-formatting` wurde am 2026-07-11 als echter ESM-/Rust-Differentiallauf abgenommen:

- deterministisches Fixture: 37.005 Byte,
  `sha256:a6e41f0d3839e77a7a5ec86d6c1c33dbb82b0248ebb75bf8e29d281f3258e3ff`,
- Fett, Kursiv, Unterstrichen, 18-Punkt-Schrift und Theme-Farbe `953735`
  sowie Zentrierung, 12,5-mm-Einzug und 1,5-facher Zeilenabstand im Oracle
  und CTOX über dieselben originalen Toolbar-Aktionen einzeln ausgeführt,
- Default-Absatzstyle-Kaskade korrigiert: `Normal` überschreibt jetzt
  `pPrDefault`; dadurch ist das initiale Split-Screen-Layout wieder pari,
- CTOX erzeugt exakt dieselben normalisierten OOXML-Werte: `w:b`, `w:i`,
  `w:u=single`, `w:sz=36`, `w:color=953735`, `w:jc=center`,
  `w:ind/@left=709` und `w:spacing/@line=360` mit `lineRule=auto`,
- Save und Reopen in CTOX sowie Reopen des kanonischen CTOX-Exports im Oracle
  bestanden,
- gespeichertes DOCY: 245.515 Byte,
  `sha256:5f50c0d26a9e91faf6ab2a2afb5034ef9e38527de0ae0b6067ec486c3ce94d42`,
- kanonischer CTOX-Export: 35.581 Byte,
  `sha256:c378aabc16b7769d0ba957310b6fa7af123176679c4627dbe27159d2e09bd40c`,
- nur `word/document.xml` geaendert; 16 von 17 Original-Parts bytegleich,
  keine Original-Parts entfernt und keine Parts hinzugefuegt,
- vollstaendiger Clean-Profile-Flow ohne Console-Fehler, Warnungen oder
  fehlgeschlagene Requests bestanden,
- Rust Office Engine: 21 Library- und 25 CLI-/Operations-Tests bestanden;
  JavaScript-Matrix/Office-Suite wird durch den Feature-Guard abgedeckt.

Historischer Zwischenstand vom 2026-07-11 fuer
`document.styles-lists-numbering` (zu diesem Zeitpunkt noch
`oracle_captured`; der spaetere Abschluss steht in der Feature-Tabelle):

- deterministisches Fixture: 37.033 Byte,
  `sha256:e484952184b361976888ed7b116e1b69ffaa1d97f9b7d2fc78e208c3558f03c5`,
- Heading 1, Quote, Bullet-Level 1 und die Fortsetzung der nummerierten Liste
  mit der sichtbaren Nummer 4 sind im gepinnten Oracle erfasst,
- Rust schreibt nun die originale DOCY-v10-Numbering-Tabelle einschließlich
  AbstractNums, Nums, Level-Text, Paragraph-/Run-Properties und synthetischen
  Bullet-Ebenen 1 bis 8,
- Rust schreibt nun zusätzlich die DOCY-v10-Styles-Tabelle aus
  `word/styles.xml` einschließlich Style-ID, Name, Typ, BasedOn, Next, Link,
  qFormat, uiPriority, Paragraph-Properties und Run-Properties; Theme-Fonts
  werden aus `word/theme/theme1.xml` aufgelöst,
- OOXML-`latentStyles`-Defaults/-Exceptions werden angewendet, damit verlinkte
  `*Char`-Styles nicht fälschlich als Gallery-Paragraph-Styles sichtbar
  werden; Default-RunProperties werden nicht mehr als direkte Run-Properties
  in jeden Dokument-Run geflattet,
- der aktuelle Prepared-Payload hat 37.272 Byte und
  `sha256:a5f1e06eac222c7420a3c132142eb7883bf6199c86ab61d394b2ed5a0b3c751b`,
- der gleich große 1.600-x-900-Split-Screen zeigt Original und CTOX mit
  derselben übernommenen Euro-Office-Toolbar, Canvas-Geometrie, Bullets und
  Nummerierung; Screenshot:
  `output/playwright/ctox-office/comparison/document.styles-lists-numbering/split-numbering-native.png`,
- der visuelle Review der identischen Ausgangsansicht ist bestanden; offen
  blieb zunächst die vollständige Differential-Abnahme. Inzwischen sind
  Heading/Quote/Einzug/Nummerierungsfortsetzung einmal vollständig über die
  originale UI gefahren; der 256.261-Byte-DOCY wurde korrekt nach OOXML
  decodiert, die fehlende Bullet-Ebene 1 wurde in `numbering.xml` ergänzt,
  15 von 17 Parts blieben bytegleich und beide Reopen-Prüfungen bestanden,
- der automatisierte Doppel-Flow ist eingecheckt und nutzt ausschließlich die
  originale Style-Gallery, Numbering-Library und das Kontextmenü. Die
  ursprüngliche Reflow-/Koordinatenursache ist beseitigt: Marker werden über
  Euro-Office Builder-Search selektiert; Paragraph-Style-Ziele verwenden
  `GetAllParagraphs()[0].Select()`, und Gallery-Kacheln werden als gemessene
  UI-Bounding-Boxes geklickt,
- zu diesem Zwischenstand blieb der Lauf rot: Die Dokumentfläche war nahe pari, aber die
  terminale Toolbar meldet beim selektierten Heading im Oracle `Calibri 14`
  und in CTOX `Arial 14`. Der spaetere Differentiallauf mit identischer
  gepinnter UI-Closure beseitigte diese
  Abweichung und hob das Feature wie oben belegt auf `differential_passed`.

Historischer Zwischenstand fuer `document.images-positioning`: Am 2026-07-11
wurde nur Oracle-/Bootstrap-Semantik erfasst; der erste Side-by-Side-Lauf am
2026-07-12 erhoehte den Status noch nicht. Der spaetere Abschluss mit echten
Blob-URLs und CTOX-Fork-UI ist in der Feature-Tabelle belegt:

- der Split-Screen-Harness laedt Original-Euro-Office v9.3.1 und CTOX mit
  identischer iframe-Geometrie und matching `comparison_config`; CTOX meldet
  `runtime=ctox-documents-fork` und DOCY v10,
- Screenshot
  `output/playwright/ctox-office/comparison/document.images-positioning/debug-initial-side-by-side.png`
  (`sha256:6ac1c3139fb27bdf971bc0020c3b4afb9814942d94ea4b554a4b68de625e433e`)
  zeigt den blockierenden Unterschied: Oracle rendert das blaue Inline-Bild
  und das orange Floating-Bild, CTOX rendert nur Text,
- Aktueller Stand nach dem ersten nativen Fix: `transcode_document_to_editor_payload`
  serialisiert `w:drawing` als DOCY-`pptxDrawing`-Run und der Rust-Test
  `document_prepare_preserves_drawing_runs_for_escrow_export` beweist zwei
  erhaltene Drawing-Runs inklusive Inline-/Floating-Extent und unveraenderter
  `CTOX_INLINE_IMAGE_TARGET`-/`CTOX_FLOATING_IMAGE_TARGET`-Escrow-XML. Der
  aktuelle Rust-Editorpayload
  `output/playwright/ctox-office/rust/document.images-positioning/ctox-rust.Editor.bin`
  ist ein valider 37.284-Byte-DOCY-v10-Payload
  (`sha256:31313e80bc3a3e7ed5a49e3b9eaa932fe88fe05d3c1fa42d0372210e241ac3c2`),
  enthaelt minimale Euro-Office-`PptxData`/ImageShape-Strukturen mit
  `media/image1.png` und `media/image2.png`,
- der erste CTOX-only-Smoke am 2026-07-12 erreichte mit diesem Payload
  `document-ready`, zeigte danach aber noch 404-Requests fuer
  `/src/apps/business-os/vendor/ctox-office/upstream/web-apps/apps/documenteditor/main/media/image1.png`
  und `media/image2.png`. Der zweite CTOX-only-Smoke am 2026-07-12 beseitigt
  diesen Blocker: die ESM-Kapsel extrahiert drei `word/media/*`-Parts aus den
  kanonischen DOCX-Bytes, registriert Blob-URLs fuer `media/image1.png` und
  `media/image2.png`, erzeugt keine `/media/image*.png`-HTTP-Requests mehr und
  rendert in der echten Euro-Office-Oberflaeche das blaue Inline-Bild sowie
  das orange Floating-Bild. Debug-Screenshots:
  `output/playwright/ctox-office/comparison/document.images-positioning/ctox-pptxdata-smoke.png`
  (`sha256:0e02feabd54c070e683cb0fb492c21cda31a3e927d3af165dc32c7e8c97dae93`)
  und
  `output/playwright/ctox-office/comparison/document.images-positioning/ctox-media-resolver-smoke.png`
  (`sha256:cd1559e02911d7f20b060632feb79cfaefbe7d044ed1192a11e62a4745da965c`),
- der erneute Browserlauf am 2026-07-12 wurde nicht als Differential-Evidenz
  gewertet: das temporäre Oracle erreichte nur `app-ready`, blieb wegen
  Callback/Save/Download-Testinfra im Fehlerzustand und lieferte deshalb kein
  gueltiges `document-ready`-Gate. Debug-Screenshot:
  `output/playwright/ctox-office/comparison/document.images-positioning/debug-after-drawing-run-preserve.png`
  (`sha256:f2366a374cdd4762d09dfb98d439f9450591c59703bd5ae0f1f12dccb3241b65`),
- das Feature blieb zu diesem Zeitpunkt deshalb `oracle_captured`; die
  Medienanzeige war nur CTOX-only-Evidenz. Die spaeter ausgefuehrte
  Split-Screen-Oracle-Abnahme mit echter Bildgroessen-, Wrap-, Positions-,
  Save-, Export-, CTOX-Reopen- und Oracle-Reopen-Pruefung ist in der
  Feature-Tabelle als `differential_passed` belegt. Die alten Aussagen dieses
  Absatzes sind historische Debug-Evidenz, nicht der aktuelle Status.

Vorhandene Oracle-/Bootstrap-Semantik:

- deterministisches Fixture mit Inline- und schwebendem DrawingML-Bild:
  38.869 Byte,
  `sha256:feb9d3611f3187ffa7068b92a452ec31b9e61fd1c120408609f542476b8c3339`,
- Inline-Breite 6,99 cm, quadratischer Textumbruch und Position 7,62 cm
  relativ zur Spalte / 0,89 cm relativ zum Absatz wurden im Oracle erfasst,
- CTOX exportiert exakt dieselben Extents und Offsets: `2516400 x 1260000`
  EMU, horizontal `2743200` EMU und vertikal `320400` EMU,
- Save und Reopen in CTOX sowie Reopen des kanonischen CTOX-Exports im
  Euro-Office-Oracle bestanden,
- kanonischer Rust-Export: 37.690 Byte,
  `sha256:cd5482732eb22acddeaccd533c7f40d1e12b323f3c4dc25290bef3a6b7feb035`,
- 20 von 22 Original-Parts bytegleich; beide echten Media-Parts, der separate
  Media-Control, die Document-Relationships und die Relationship-Control sind
  unveraendert, keine Parts fehlen oder wurden hinzugefuegt,
- vollstaendiger Clean-Profile-Flow ohne Console-Fehler oder fehlgeschlagene
  Dokumentrequests bestanden; JavaScript-Suite 3/3 und Rust-Suite 5/5 gruen.

`document.sections-headers-footers` wurde am 2026-07-11 als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- deterministisches Fixture mit zwei Abschnitten, First-/Default-Header,
  Default-Footer und separatem Escrow-Control: 38.930 Byte,
  `sha256:18b799f5ee03df7a39e3d38773896b07824a3053a9c1b42b3126f0d6c2408ab0`,
- Querformat, Abschnittsumbruch zur naechsten Seite, Header-/Footer-Abstaende,
  erste Seite anders und Aufloesen der vorherigen Header-Verknuepfung wurden
  im Oracle einzeln erfasst,
- CTOX erzeugt drei Abschnitte und drei Seiten mit denselben normalisierten
  Seitengroessen, Margins, Break-Typen, Title-Page-Flags und effektiven
  Header-/Footer-Referenzen,
- Save und Reopen in CTOX sowie Reopen des kanonischen CTOX-Exports im Oracle
  bestanden,
- kanonischer Rust-Export: 38.281 Byte,
  `sha256:fba7bbeaa54a0a0010fe45a7bc6123debc48ee320e50f4601b14258271f0ad59`,
- alle 21 Original-Parts erhalten, ein beabsichtigter neuer Header-Part
  hinzugefuegt und der unbeteiligte Custom-XML-Part bytegleich erhalten,

Update 2026-07-12: Der echte Same-Origin-Split-Screen-Flow bedient inzwischen
auch `HeadersAndFooters_LinkToPrevious(false)` ueber die Euro-Office-Runtime
und den zusaetzlichen Abschnittsumbruch ueber `add_SectionBreak`. Der native
Export materialisiert den dadurch entstehenden zusaetzlichen Default-Header als
`word/header3.xml`, ergaenzt `word/_rels/document.xml.rels` und
`[Content_Types].xml`, und referenziert den neuen Header im zweiten Abschnitt.
Der Clean-Key-Vollvergleich endet auf Oracle und CTOX mit `Seite 3 von 3`,
CTOX-Reopen des gespeicherten Payloads und Oracle-Reopen des nativen Exports
sind bestanden; der Featurestatus ist `differential_passed`.
- Clean-Profile-Flow ohne Console-Fehler oder fehlgeschlagene
  Dokumentrequests bestanden; JavaScript-Suite 3/3 und Rust-Suite 6/6 gruen.

`document.links-bookmarks-fields` wurde am 2026-07-12 auf der echten ESM-DocumentEditor-Runtime differential abgenommen:

- deterministisches Fixture mit bestehendem externen Link, Bookmark,
  NUMPAGES-Feld mit absichtlich veraltetem Cachewert und separatem
  Custom-XML-Escrow-Control: 37.450 Byte,
  `sha256:33412f5c2ca2a616f33d7ef15c0e2088a363a4fa7c0693f8404f26dd69733ebb`,
- beide Paneele laufen mit der originalen Euro-Office-UI; der versionierte
  headed Split-Screen-Flow verwendet auf beiden Seiten dieselben
  DocumentEditor-APIs für Selektion, Hyperlink, Bookmark und Feldaktualisierung,
- Oracle und CTOX erzeugen denselben Link `CTOX_EXTERNAL_LINK` auf
  `https://ctox.dev/office-oracle`, das Bookmark `ctox_oracle_bookmark` und
  den aktualisierten NUMPAGES-Wert `1`; bestehender Link und Bookmark bleiben
  erhalten,
- der native Rust-Writer portiert die DOCY-Records für Hyperlink, Bookmark,
  FieldChar und InstrText; der Rückweg materialisiert externe Hyperlink-
  Relationships wieder im OOXML-Package,
- Save und Reopen des 364.980-Byte-CTOX-DOCY in CTOX sowie Reopen des
  kanonischen CTOX-Exports im Oracle bestanden,
- kanonischer Rust-Export: 36.017 Byte,
  `sha256:ad352ab948857358cfc5989940fea992633a39b8b1448d6acd981abbfefbe800`,
- alle 18 Original-Parts erhalten, 16 davon bytegleich; nur
  `word/document.xml` und `word/_rels/document.xml.rels` ändern sich, der unbeteiligte
  Custom-XML-Part ist bytegleich, es fehlen keine Parts und es wurden keine
  unbeabsichtigten Parts hinzugefuegt,
- der Clean-Key-Flow meldet auf dem ESM-Capsule nach Save `dirty=false`, der
  Oracle-Callback erreicht Status 6, und Browser/Dokumentrequests bleiben
  fehlerfrei; Status ist `differential_passed`.

`document.comments-track-changes` wurde am 2026-07-11 als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- deterministisches Fixture mit bestehendem Kommentar, bestehender Einfügung,
  bestehender Löschung und separatem Custom-XML-Escrow-Control: 37.784 Byte,
  `sha256:2784f50e51b7f90bc8768fca72bd49b029e437c3d5dc5636e5f47e4835084497`,
- Kommentar erstellen, beantworten und auflösen sowie getrackte Einfügung,
  getrackte Löschung, gezielte Ablehnung, Annahme aller übrigen Änderungen und
  eine verbleibende finale Revision wurden im gepinnten Oracle einzeln erfasst,
- CTOX erhält dieselbe Semantik: aufgelöste Kommentarwurzel mit offener
  Antwort, wiederhergestelltes `TRACK_DELETE_TARGET`, angenommene Einfügungen,
  entfernte bestehende Löschung und genau eine verbleibende `w:ins`-Revision
  mit `_CTOX_FINAL_REVIEW`,
- Save und Reopen in CTOX, Clean-Profile-Reopen und Reopen des kanonischen
  CTOX-Exports im Euro-Office-Oracle bestanden; Browser und Dokumentrequests
  blieben ohne Fehler oder Warnungen,
- kanonischer Rust-Export: 38.071 Byte,
  `sha256:1b40bed04a3db194c135ca2a86acdffdb253d8ac923eeb5cb014c36cb95a043e`,
- alle 19 Original-Parts erhalten, 13 davon bytegleich; der unbeteiligte
  Custom-XML-Part ist bytegleich, `word/commentsExtended.xml` wurde als
  beabsichtigter verstandener Part ergänzt und keine unbeabsichtigten Parts
  hinzugefügt,
- JavaScript-Suite 3/3, Rust-Suite 8/8, Rust-Formatcheck und
  Feature-Matrix-Validator bestanden.

Port-Update vom 2026-07-12: Der echte ESM-/UI-Closure-Split-Screen lädt nun
erstmals auch im CTOX-Paneel den importierten Kommentar sowie die bestehenden
`w:ins`-/`w:del`-Revisionen aus einem nativ von Rust erzeugten DOCY. Rust
schreibt dafür die Comments-Tabelle und die CommentStart-/CommentEnd-/
CommentReference-/Ins-/Del-Records. Der initiale semantische Gate ist bestanden.
Der versionierte Browserflow bedient inzwischen auf beiden Seiten den sichtbaren
Statusleisten-Schalter „Nachverfolgen von Änderungen“, erzeugt Änderungen über
reale Tastatureingaben und verwendet die gepinnte UI-Closure zum Annehmen und
Ablehnen. Der gespeicherte 364.997-Byte-CTOX-DOCY wird nativ zu einem
36.727-Byte-DOCX
(`sha256:4d2ffd42d91e291923657a41d744ba8d475ee7155825a0e6a5603348a4ec3356`)
exportiert. Der Rückweg materialisiert drei Kommentare einschließlich Antwort
und Auflösungsstatus in `word/comments.xml`/`word/commentsExtended.xml`, erhält
genau die verbleibende `_CTOX_FINAL_REVIEW`-Einfügung und entfernt die
angenommene Löschung. Alle 19 Originalparts bleiben erhalten; nur der
beabsichtigte Part `word/commentsExtended.xml` kommt hinzu. Der frische
Clean-Profile-Split-Screen-Lauf hat anschließend initiale und terminale
Semantik, Oracle-Status 6/2, CTOX-Save, CTOX-Reopen und Oracle-Reopen im echten
Review-Modus ohne Consolefehler bestanden. Status ist `differential_passed`.

`document.drawings-charts` wurde am 2026-07-11 als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- deterministisches Fixture mit moderner DrawingML-Business-Shape, echtem
  Säulendiagramm, eingebettetem XLSX-Workbook und separatem Custom-XML-Escrow:
  44.004 Byte,
  `sha256:8adf729d96daa34f10f11a538b84ac6e84769ad5fd29f9f113fa590c21c98a0a`,
- Diagrammbreite 14 cm bei gesperrtem Seitenverhältnis, Shape-Füllung
  `F4B183` und 90-Grad-Drehung wurden im gepinnten Oracle über reale
  Canvas-Selektion und die sichtbaren Einstellungsflächen erfasst,
- CTOX erzeugt dieselben normalisierten Werte: Diagramm-Extent
  `5040000 × 2520000` EMU, Chart-Style 102 mit Fallback 2, Shape-Füllung
  `F4B183` und DrawingML-Rotation 5.400.000 Einheiten,
- Save und Reopen in CTOX, Clean-Profile-Reopen und Reopen des kanonischen
  CTOX-Exports im Euro-Office-Oracle bestanden; Browser und Dokumentrequests
  blieben ohne Fehler oder Warnungen,
- kanonischer Rust-Export: 42.796 Byte,
  `sha256:0d9d38872c549cf2860a25fccb5d48b3ea59b6571da017e7e9cde376e607ec3f`,
- alle 21 Original-Parts erhalten, 15 davon bytegleich; das eingebettete
  Workbook und der unbeteiligte Custom-XML-Part sind bytegleich, keine Parts
  fehlen und es wurden keine unbeabsichtigten Parts hinzugefügt,
- JavaScript-Suite 3/3, Rust-Suite 9/9, Rust-Formatcheck und
  Feature-Matrix-Validator bestanden.

Port-Update vom 2026-07-12: Die alte Bootstrap-Evidenz wurde durch den harten
Gate mit der gepinnten UI-Closure ersetzt. Der native Rust-Writer erzeugt aus dem DOCX eine
echte PPTY-Shape und einen echten `Chart2`-Datensatz; beide werden rechts in
derselben gepinnten Euro-Office-ESM-Closure wie links gerendert. Der
versionierte Playwright-CLI-Flow selektiert Shape und Diagramm aus gemessenen
Editorflächen, setzt auf beiden Seiten über die originalen `Asc`-APIs
Füllung `F4B183`, Rotation 90 Grad, Größe 14 × 7 cm und Chart-UI-Stil 102 und
betätigt beide echten Save-Schaltflächen. Der gespeicherte
368.930-Byte-CTOX-DOCY wird nativ zu einem 42.530-Byte-DOCX
(`sha256:ee6501feaabd373f8a88fab7b049c0baf2f93d4420ba31c0e7446988eea52001`)
exportiert. Der neue DOCY-Rückweg dekodiert PPTY-Transform/Füllung,
Drawing-Extent und Chart-AlternateContent-Stil und materialisiert ausschließlich
`word/document.xml` und `word/charts/chart1.xml`. Alle 19 übrigen Parts,
insbesondere das eingebettete XLSX und das CustomXML-Escrow, bleiben bytegleich.
Initialzustand, Saved-DOCY-Reopen sowie der native CTOX-Export in CTOX und im
Oracle bestehen jeweils den gleich großen Split-Screen; Console
und Requests bleiben sauber. Status ist `differential_passed`.

`document.docx-roundtrip-corpus` wurde am 2026-07-11 als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- versioniertes Manifest mit elf deterministischen DOCX-Fixtures, insgesamt
  419.713 Byte und 204 deklarierten Package-Parts,
- alle elf vorherigen Feature-Gates, kanonischen CTOX-Export-Hashes,
  CTOX-Reopens, Euro-Office-Reopens und Clean-Profile-Nachweise automatisiert
  zusammengefasst und erneut validiert,
- Rust `prepare -> export` ist ohne Aenderung fuer alle elf Dateien
  byteidentisch; deklarierte Custom-XML-, Media- und Embedding-Escrow-Parts
  sind vorhanden und lesbar,
- Nicht-ZIP- und unvollstaendige DOCX-Packages werden abgewiesen,
- neuer Browser-Harness mit Original-Euro-Office links und CTOX rechts in
  zwei exakt 800 × 813 CSS-Pixel grossen iframes; beide erreichen durch
  dieselbe zweifache Zoom-Aktion 120 Prozent bei sauberer Console,
- Korpus-Validator, Feature-Matrix-Validator, JavaScript-Suite 3/3,
  Rust-Office-Engine 10/10 und Rust-Formatcheck bestanden.

Port-Update vom 2026-07-12: Der frühere „Identity“-Test, der beim Export
fälschlich erneut die DOCX-Quelldatei statt des vorbereiteten DOCY übergab,
wurde entfernt. Der Korpus-Test führt nun für alle elf Fixtures den echten
Pfad `DOCX → nativer DOCY-v10-Writer → nativer DOCY-Decoder/OOXML-Writer` aus.
Dabei wurden zwei reale Lücken geschlossen: leere OOXML-Runs werden beim
formatierenden Rückweg korrekt übersprungen, während Page-Break-, Line-Break-
und Tab-Runs explizit dekodiert und erhalten werden; außerdem erzeugt ein
unveränderter Shape-Transform kein doppeltes `rot`-Attribut mehr. Der echte
Korpuslauf erhält Primärtext, alle Originalparts und alle im Manifest genannten
Escrow-Parts bytegleich; ausschließlich verstandene neue Parts sind erlaubt
(etwa `commentsExtended.xml`). Der Korpus-Validator meldet 11 Fixtures und 204
deklarierte Parts, die vollständige Rust-Suite 32/32 und die Browser-/Bridge-
Suite 24/24. Weil alle elf vorgelagerten Features jeweils den echten
UI-Closure-Split-Screen, Save und beide Reopen-Richtungen belegen, ist der
aggregierte Status jetzt `differential_passed` und das Spreadsheet-Gate offen.

`spreadsheet.open-render-sheets` wurde am 2026-07-11 als Oracle-/Bootstrap-Semantik erfasst (kein Frontend-Port):

- deterministisches XLSX-Fixture mit den sichtbaren Sheets `Overview` und
  `Details`, dem ausgeblendeten Sheet `Archive`, 12 Package-Parts und
  `sha256:e0553218296a0224945569b84bddfad70e9fdee60605333982c171cfe9891043`,
- Original-Euro-Office v9.3.1 und der CTOX-ESM-Prototyp öffnen das Workbook
  side by side in zwei exakt gleich großen iframes; Navigation von `Overview`
  nach `Details` ist in beiden Browserflächen erfasst,
- CTOX-`inspect()` meldet drei Sheets, zwei sichtbare Sheets, die drei
  deterministischen Marker, `dirty=false` und den korrekten aktiven Tab,
- der Business-OS-Wrapper lädt XLSX aus den bestehenden RxDB-Blob-Chunks nur
  bei der typisierten Einstellung `office.spreadsheets_engine=ctox_spreadsheets`;
  die Legacy-Engine bleibt explizit auswählbar,
- `office.spreadsheet.prepare` und `.export` werden über den CommandBus mit
  `transport=rxdb-webrtc` ausgelöst; der native Rust-Export und der
  Browserexport sind byteidentisch mit dem XLSX-Fixture,
- eigenständige Rust-Office-Engine 11/11 inklusive XLSX-Manifest,
  Identity-Roundtrip und Custom-XML-Escrow bestanden,
- alle 12 Package-Parts einschließlich Relationships, Shared Strings, Styles,
  Worksheets und Custom-XML-Escrow sind byteidentisch; Clean-Profile-Reopen
  des kanonischen Exports in CTOX und Euro-Office sowie fehlerfreie
  Side-by-Side-Navigation bestanden.

Port-Update vom 2026-07-12: Nach Freigabe des Documents-Korpus wurde der
Spreadsheet-Gate mit einem frischen 1600 × 900 Browserlauf erneut ausgeführt.
Beide 799,5 × 813 Pixel großen Paneele laden die echte gepinnte
SpreadsheetEditor-Closure und erreichen exakt `document-ready`; rechts stammt
das 2.009-Byte-XLSY vollständig aus dem nativen Rust-Writer. Der versionierte
Browserflow prüft auf beiden Seiten `Overview`, `Details`, das ausgeblendete
`Archive`, unveränderten Dokumentstatus und die Runtime-Provenienz. Danach
klickt er beidseitig den echten `Details`-Tab in der Original-Statusleiste.
Formelzeile, Marker, Zellwerte 42/18, Styles und Rastergeometrie stimmen ohne
Masken sichtbar überein. Da die Interaktion nur navigiert, bleibt der
kanonische 4.951-Byte-XLSX-Escrow-Export byteidentisch und öffnet bereits in
CTOX sowie im Oracle. Status ist `differential_passed`; als nächstes folgt der
mutierende `spreadsheet.edit-save`-Rückweg.

`spreadsheet.edit-save` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- deterministisches 4.962-Byte-XLSX-Fixture mit Zielzelle `Overview!A2`,
  Preservation-Marker und separatem Custom-XML-Escrow,
- Zwei gleich grosse Original-SpreadsheetEditor-Instanzen fuehren identische
  Zellnavigation über das Namensfeld, Ersetzen von
  `CTOX_EDIT_CELL_ALPHA` durch `CTOX_EDIT_CELL_BRAVO_42`, Dirty/Clean und
  Save wurden im Original-Euro-Office und in CTOX erfasst,
- CTOX commitet ausschließlich über `office.spreadsheet.commit` mit
  `base_version_id=sheet_edit_v1`; die terminale Version ist
  `sheet_edit_v2`,
- die ESM-Kapsel normalisiert die real beobachteten koerperrelativen Offsets des
  `asc_nativeGetFile()`-v2-Saves in den absoluten XLSY-v10-Vertrag; der
  gespeicherte Payload hat 5.081 Byte und
  `sha256:1b85914157aed444a1813461d8efe460ed3c16067083be826e5b65d91ad9d0a4`,
- der native Rust-Export ersetzt nur `xl/sharedStrings.xml`; 11 von 12 Parts
  sind bytegleich, es fehlen
  keine Parts und es kommen keine unbeabsichtigten Parts hinzu,
- Custom-XML-Escrow und unbeteiligter Shared-String-Marker bleiben erhalten,
- der kanonische Export hat 4.966 Byte und
  `sha256:de4466940c2f9ff32aed62a757a5c93b741da82196bda3bc40af413be22fa4c8`,
- Clean-Profile-Reopen zeigt in CTOX und Euro-Office für A2 exakt denselben
  Wert; Console und Dokumentrequests bleiben fehlerfrei,
- der echte Business-OS-Pfad `modules/spreadsheets.mount(ctx)` lädt dieselbe
  ESM-Kapsel über die typisierte Einstellung, schreibt den 5.081-Byte-XLSY-Blob
  in `spreadsheet_blob_chunks` und dispatcht `office.spreadsheet.commit` mit
  `until=terminal`; der Legacy-CSV-Autosave ist fuer diesen Editor deaktiviert,
- Status ist `differential_passed`; `shipped` bleibt an die Rollout-Matrix
  gebunden.
- Aktuelle Verifikation: Office-/Spreadsheet-JavaScript 35/35, eigenstaendige
  Rust-Office-Engine 34/34 und Feature-Matrix 24/24 bestanden.

`spreadsheet.undo-clipboard-fill` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- zwei gleich grosse gepinnte Euro-Office-v9.3.1-SpreadsheetEditor-Instanzen
  wurden mit demselben 4.986-Byte-XLSX und derselben Interaktionsfolge betrieben,
- Editieren von A2, Undo auf `UNDO_FILL_BASE` und Redo auf
  `UNDO_FILL_BASE_ONE` stimmen überein,
- Copy/Paste ueber die echten Toolbar-Schaltflaechen kopiert
  `COPY_SOURCE_TEXT` von A3 nach B3; der sichtbare Pfad
  `Mehr → Ausfüllen → Nach unten` übernimmt
  den numerischen Wert `125000` von B4 nach B5 und erhält den numerischen
  OOXML-Zelltyp,
- der echte ESM-Save hat 5.118 Byte und
  `sha256:71978fb2ad877418604f57278c844e35095ec7b3875a4738509efc72b5250e1e`;
  der CTOX-Commit verwendet `base_version_id=sheet_undo_v1` und liefert
  `sheet_undo_v2`,
- Rust haelt trotz der SDK-Deduplizierung die unbeteiligten Shared-String-
  Indizes stabil und ersetzt ausschließlich `xl/sharedStrings.xml` und
  `xl/worksheets/sheet1.xml`,
- 10 von 12 Original-Parts bleiben bytegleich, das Custom-XML-Escrow
  `SPREADSHEET_UNDO_ESCROW_9A31` bleibt erhalten,
- der kanonische CTOX-Export hat 4.981 Byte und
  `sha256:0f6cdc86331e2b904c95834eea2677c9a4c758a24492153ff37b3d573717f3bf`,
- Clean-Reopen im zweigeteilten Browser zeigt den CTOX-Export sowohl im
  Original-Euro-Office als auch im CTOX-ESM-Editor mit identischem A2, B3 und B5,
- derselbe Ablauf besteht im echten `modules/spreadsheets.mount(ctx)`-Wrapper;
  der 5.118-Byte-Editorblob wird ueber RxDB-Chunks gespeichert und
  `office.spreadsheet.commit` mit `until=terminal` dispatcht. Status ist
  `differential_passed`.

`spreadsheet.cell-format-rows-columns` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- A2 erhält in beiden Editoren Bold+Italic, B4 ein Euro-Buchhaltungsformat,
- Zeile 4 wird auf 27,75 pt und Spalte B auf 32,625 Zeichen gesetzt,
- Zeile 5 wird aus- und wieder eingeblendet; der finale Export hält sie
  sichtbar,
- beide Seiten verwenden die originale Euro-Office-v9.3.1-SpreadsheetEditor-UI
  im gleich großen 799,5×813-px-Split-Screen; A1 bleibt als Negativkontrolle
  fett und nicht kursiv,
- CTOX speichert einen nativen 5.408-Byte-XLSY-v10-Payload und materialisiert
  die Semantik ausschließlich in `xl/styles.xml` und
  `xl/worksheets/sheet1.xml`; die übrigen 10 von 12 Parts bleiben bytegleich,
- der Rust-Escrow-Export erhält `SPREADSHEET_FORMAT_ESCROW_4C72`, hat 5.132
  Byte und `sha256:bc2b6e9928482a50d050fed870799a9ef98a01a26d5340bab2da7ac0709b2668`,
- beim Side-by-side-Reopen meldet Euro-Office `document-ready`; CTOX liest
  Bold/Italic, Accounting, 27,75 pt und 32,625 Zeichen aus dem kanonischen
  XLSX zurück und rendert sie ohne Consolefehler; das fehlende originale
  `fonts_thumbnail.png.bin` ist aus dem gepinnten Oracle-Digest in der Closure,
- derselbe Ablauf besteht im echten `modules/spreadsheets.mount(ctx)`-Wrapper:
  `office.spreadsheet.commit`, `base_version_id=sheet_business_os_oracle_v1`,
  RxDB/WebRTC, `until=terminal`, Status `Gespeichert`.

`spreadsheet.formulas-references` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- relative (`B2*2`), absolute (`$B$2+5`), Bereichs- (`SUM(B2:B4)`) und
  Sheet-Referenzen (`'Details'!B4+1`) werden mit getrenntem Formeltext und
  Cachewert gelesen,
- der native Rust-Writer erzeugt aus dem 5.130-Byte-XLSX ein 2.667-Byte-XLSY-v10;
  beide gleich großen Originaleditoren öffnen es direkt und zeigen dieselbe
  lokalisierte Formelzeilen-Semantik,
- die sichtbare Änderung von D3 über die originale Formelzeile auf `D2*3`
  ergibt in Oracle und CTOX den
  Cachewert 15,
- Copy/Paste von B7 nach C7 verschiebt die relative Referenz von `B2+1` nach
  `C2+1` und ergibt 21; absolute Zeilen-/Spaltenmarker bleiben beim Shift
  erhalten,
- `1/0` bleibt als Formel mit dem Fehlercache `#DIV/0!` erhalten,
- der gespeicherte CTOX-Editorpayload hat 5.711 Byte und enthält getrennt
  Formeltext und Cachewerte (`D3=15`, `C7=21`, `B8=#DIV/0!`),
- Rust verändert ausschließlich `xl/worksheets/sheet1.xml`; 11 von 12 Parts
  einschließlich Shared Strings, Styles, Relationships und Custom-XML-Escrow
  bleiben bytegleich,
- der Rust-Escrow-Export hat 5.132 Byte und
  `sha256:eecbfbe9eef8f7a3dad7194ebde489006bb05520ad33643b86debd1107035722`,
- CTOX und das gepinnte Euro-Office öffnen den kanonischen Export erneut und
  zeigen identische Formeln und Cachewerte ohne Consolefehler,
- derselbe Ablauf besteht im echten `modules/spreadsheets.mount(ctx)`-Wrapper:
  RxDB/WebRTC, `office.spreadsheet.commit`, `until=terminal`, 5.711-Byte-
  Editorblob und Status `Gespeichert`.

`spreadsheet.multi-sheet-merge-freeze` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- die sichtbaren Sheets `Overview` und `Details` lassen sich in beiden
  Editoren wechseln; das verborgene Sheet `Archive` bleibt im Workbook
  erhalten und wird nicht als Tab angeboten,
- der vorhandene Merge `B2:C2` wird getrennt und `B3:C3` neu verbunden,
- die vorhandene Fixierung bei `B2` wird aufgehoben und bei `B3` mit
  `xSplit=1`, `ySplit=2` und `activePane=bottomRight` neu gesetzt,
- beide gleich großen Original-SpreadsheetEditor-Instanzen (799,5×813 px)
  bedienen die sichtbaren Sheet-Tabs und die originale Merge-and-Center-
  Toolbar; der Freeze-Befehl läuft über exakt `asc_freezePane`, das auch der
  originale `ViewTab` verwendet,
- der native Rust-Writer liest und schreibt Worksheet-Visibility,
  `MergeCells`, `SheetViews` und `Pane` als echte XLSY-v10-Records; dabei
  wurde die gepinnte `EActivePane`-Codierung bytegenau übernommen,
- Euro-Office exportiert dieselben `mergeCells`- und `pane`-Strukturen wie
  CTOX; der Oracle-Callback endet mit Status 6 und anschließend Status 2,
- der native Rust-Escrow-Export verändert ausschließlich
  `xl/worksheets/sheet1.xml`; 11 von 12 ursprünglichen Parts bleiben
  bytegleich und `SPREADSHEET_MERGE_FREEZE_ESCROW_D52B` bleibt erhalten,
- der kanonische XLSX-Export hat 4.906 Byte und
  `sha256:cd90242eeb9c7a3422ae446c61ecf38385cd1118e8f90f6c186bc784d462b38b`,
- CTOX und Euro-Office öffnen den Export im gleich großen Side-by-side-Harness
  erneut ohne Consolefehler,
- derselbe Ablauf besteht im echten `modules/spreadsheets.mount(ctx)`-Wrapper:
  `office.spreadsheet.commit`, RxDB/WebRTC, `until=terminal`, 4.786-Byte-
  Editorblob und Status `Gespeichert` ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.multi-sheet-merge-freeze.json),
  [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.multi-sheet-merge-freeze.json),
  [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.multi-sheet-merge-freeze.playwright.js)).

`spreadsheet.sort-filter-tables` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- `RevenueTable` wird als strukturierte Tabelle `A1:C6` mit
  `TableStyleMedium4` aus Worksheet-Relationship und `xl/tables/table1.xml`
  gelesen,
- beide 799,5×813 px großen Seiten verwenden die originale
  SpreadsheetEditor-UI aus der gepinnten ESM-Closure; `Tabellen-Design`,
  Header-Filterdialog, Sortieraktion und Filterwertbaum sind auf beiden Seiten
  dieselben Euro-Office-Oberflächen,
- der Browserflow leitet die Klickpositionen aus
  `Asc.editor.asc_getActiveCellCoord()` ab; es gibt keine fest verdrahteten
  Bildschirmkoordinaten,
- Sortierung nach `Revenue` absteigend ergibt in beiden Editoren
  `420, 310, 240, 180, 120`,
- der Tabellenfilter `Region = North` lässt ausschließlich die Zeilen 2 und 3
  sichtbar und persistiert die übrigen Zeilen als verborgen,
- der native Rust-Writer liest und schreibt Euro-Office-XLSY-v10-Records für
  TableParts, AutoFilter, FilterColumns, SortState, TableColumns,
  TableStyleInfo und den Hidden-Status der gefilterten Zeilen,
- der native Rust-Escrow-Export behandelt `xl/tables/*.xml` als verstandene
  Partfamilie; geändert werden ausschließlich `table1.xml` und `sheet1.xml`,
  während 12 von 14 Parts einschließlich
  `SPREADSHEET_TABLE_ESCROW_8B47` bytegleich bleiben,
- der kanonische Export hat 5.837 Byte und
  `sha256:0176e62baaee9cb4f31ebe2d2d8c1f150acbc664450928615dac817ae5a640bf`,
- CTOX und Euro-Office öffnen den gefilterten, sortierten Tabellenexport im
  Side-by-side-Harness erneut; derselbe Ablauf besteht im echten
  `modules/spreadsheets.mount(ctx)`-Wrapper mit RxDB/WebRTC,
  `office.spreadsheet.commit`, `until=terminal`, 5.399-Byte-Editorblob und
  Status `Gespeichert` ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.sort-filter-tables.json),
  [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.sort-filter-tables.json),
  [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.sort-filter-tables.playwright.js)).

`spreadsheet.validation-conditional-formatting` wurde am 2026-07-12 mit Oracle und derselben CTOX-Fork-UI differential abgenommen:

- beide 799,5×813 px großen Seiten verwenden die originale
  SpreadsheetEditor-UI; der Lauf öffnet auf beiden Seiten den echten Tab
  `Daten` und den echten Dialog `Datenüberprüfung`, nicht eine CTOX-
  Nachbildung,
- die Listenvalidierung in `B2` wird im Dialog von
  `Draft;Review;Final` auf `Draft;Review;Final;Approved` erweitert;
  anschließend speichern beide Editoren `Approved` in `B2`, den gültigen
  Ganzzahlwert `8` in `C2` und den bedingt formatierten Schwellenwert `80` in
  `E2`,
- der native Rust-Writer liest und schreibt XLSY-v10-Records für
  `DataValidations`, `DataValidation`, `ConditionalFormatting`, `CFRule`,
  `ColorScale`, `CFVO`, Spreadsheet-Farben und den eingebetteten
  Differential-Style der `cellIs`-Regel,
- das native Manifest erhält beide Validierungen sowie die drei
  ColorScale-Farben `FFF8696B`, `FFFFEB84`, `FF63BE7B` und den
  `cellIs`-Style `FFC6EFCE`/`FF006100`,
- der 6.022-Byte-CTOX-Editorpayload
  (`sha256:f798af4f819da0fe4f022e93aad01f2c00385fb8277f93f1e0db1f90faf75407`)
  exportiert nativ zu einem 5.446-Byte-XLSX
  (`sha256:0829a157bf2fe4673dab58f4d928def5a7080d9c7bcc8e309d0de2579f243351`),
- ausschließlich `xl/sharedStrings.xml` und `xl/worksheets/sheet1.xml`
  ändern sich; 10 von 12 Originalparts bleiben bytegleich, kein Part fehlt
  und `SPREADSHEET_VALIDATION_ESCROW_C19E` bleibt erhalten,
- CTOX und das gepinnte Euro-Office öffnen den nativen Export erneut und
  melden identisch `Approved`, `8`, `80` sowie die erweiterte
  Listenvalidierung,
- derselbe Ablauf mit der CTOX-Fork-UI besteht in
  `modules/spreadsheets.mount(ctx)` mit RxDB/WebRTC,
  `office.spreadsheet.commit`, `until=terminal`, 6.013-Byte-Editorblob und
  Status `Gespeichert` ([Evidenz](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.validation-conditional-formatting.json),
  [Flow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.validation-conditional-formatting.json),
  [Browserflow](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.validation-conditional-formatting.playwright.js)).

## Offene Risiken und bewusste Grenzen

- Documents und Spreadsheets verwenden die gepinnten originalen
  `web-apps`-/`sdkjs`-Runtimes hinter der ESM-/iframe-Fassade. Portiert und
  belegt ist bewusst nur die 24-Feature-Business-Closure, nicht der gesamte
  Euro-Office-Produktumfang.
- Euro-Office nutzt zahlreiche Globals und globale Styles; die iframe-Grenze
  bleibt deshalb vorerst erforderlich.
- Protokollkompatibilitaet und OOXML-Erhaltung sind erst fuer die abgenommenen
  Features belegt, nicht fuer den gesamten Office-Umfang.
- Canvas- und Font-Unterschiede koennen visuelle Diffs erzeugen; Browser,
  Fonts, Viewport und Device Scale muessen fuer Oracle-Flows fest bleiben.
- Live-Coediting, Makros, Praesentationen, PDF, Visio, Mobile und seltene
  Legacy-Formate sind nicht Teil der ersten Business-Paritaetsstufe.

## Produktions-Rollout-Matrix

Die Feature-Paritaet ist 24/24. Sie ist jedoch nicht gleichbedeutend mit der
Produktionsfreigabe. Fuer den Rollout gilt deshalb diese getrennte Matrix:

| Szenario | Stand | Beleg / naechstes Gate |
| --- | --- | --- |
| Unvollstaendige Blob-Chunks | automatisiert bestanden | Browser-Bridge verwirft den Blob mit `blob_incomplete` und kann ihn nach vollstaendiger RxDB-Replikation erneut laden. |
| SHA-256-Fehler | automatisiert bestanden | Kanonischer und Editor-Blob werden beim Lesen gegen `source_sha256` beziehungsweise `editor_sha256` geprueft; Abweichung endet mit `blob_hash_mismatch`. |
| Fehlende Schreibrechte | automatisiert bestanden | Commit endet vor Blob-Staging und Command-Dispatch mit `permission_denied`. |
| Stale Base Version | automatisiert bestanden | Der Browser erhaelt den nativen `version_conflict` unveraendert; Rust prueft geladene, aktuelle und autoritative Basisversion. |
| Offline / Reconnect | Browser-Bridge fuer beide Apps bestanden | Im echten Chromium wird der Editor-Blob offline lokal gestaged, waehrend `replicationUp=false` kein Command versendet und nach dem Online-Event exakt ein terminaler RxDB/WebRTC-Command dispatcht; der native Peer-Neustart wird mit dem Daemon-Restart gemeinsam abgenommen. |
| Daemon-Neustart | echte Saves fuer beide Apps bestanden | `office.document.commit` und `office.spreadsheet.commit` laufen jeweils waehrend einer offenen Browser-Session ueber CommandBus und RxDB/WebRTC; CTOX und der native Peer werden neu gestartet, danach sind v2-Zeiger und Version wieder im Browser und der kanonische Blob in Rust hashgeprueft lesbar. [Evidenz](../src/apps/business-os/office-engine/oracle/evidence/office.native-peer-restart.json) |
| Browser-Neuladen | beide Apps bestanden | Documents und Spreadsheets remounten nach Reload durch ihren echten Business-OS-Wrapper jeweils exakt eine CTOX-ESM-Kapsel mit CTOX-Fork-UI; Save/Reopen ist zusaetzlich in den Feature-Flows belegt. |
| Clean Profile | beide Apps bestanden | Zwei getrennte Chromium-Profile starteten mit leeren Local-/Session-Storage-Schluesseln, erreichten ueber die echten App-Wrapper die jeweilige CTOX-Fork-UI innerhalb der CTOX-ESM-Kapsel ohne Consolefehler und wurden anschliessend geschlossen. |
| Deutsch / Englisch | beide Apps bestanden | Beide echten Business-OS-Apps und CTOX-Fork-UIs erreichten in `en` den Tab `File` und in `de` den Tab `Datei`, jeweils ohne Consolefehler. |
| Windows-/macOS-Shellstil | beide Apps bestanden | Beide Apps liefen mit typisiertem `macos`- und `windows`-Shellstil in der headed Browsermatrix. |
| Legacy → CTOX → Legacy | beide Apps bestanden | Typisierte Umschaltung entfernte das CTOX-iframe bei `legacy` und erzeugte bei Rueckkehr zu `ctox_documents` bzw. `ctox_spreadsheets` exakt eine neue CTOX-Fork-ESM-Kapsel. |
| iframe-/MessageChannel-Teardown | beide Apps bestanden | Wiederholter Mount/Unmount und Reload hinterliessen in beiden Apps jeweils null beziehungsweise exakt ein iframe. |

Am 2026-07-13 bestanden nach dem neuen Integritaets- und Rollout-Slice 37
Office-Engine-JS-Tests, 7 Documents-Wrappertests und 10
Spreadsheets-Wrappertests. Der erwartete HyperFormula-No-License-Fallback im
Spreadsheets-Test bleibt eine protokollierte Warnung, kein Testfehler.

Der headed Spreadsheets-Produktions-Lifecycle lief danach zusaetzlich durch
`modules/spreadsheets.mount(ctx)`, `ctox-office-spreadsheet.mjs` und dessen
lokal CTOX-Fork-UI. Er belegt
`ctox_documents|ctox_spreadsheets → legacy → ctox_documents|ctox_spreadsheets`, Browser-Reload, `en`/`de` und
`macos`/`windows` ohne Consolefehler. Der versionierte Ablauf und die Evidenz
liegen in
[`spreadsheet.production-lifecycle.playwright.js`](../src/apps/business-os/office-engine/oracle/flows/spreadsheet.production-lifecycle.playwright.js)
und
[`spreadsheet.production-lifecycle.json`](../src/apps/business-os/office-engine/oracle/evidence/spreadsheet.production-lifecycle.json).

Der entsprechende Documents-Lauf ueber `modules/documents.mount(ctx)` und
`ctox-office-document.mjs` fand zunaechst einen echten fehlgeschlagenen
Asset-Request der CTOX-Fork-UI auf `icon-document.svg`. Der reproduzierbare
Vendor-Build nimmt deshalb nun neben dem Spreadsheet- auch das originale
Document-Header-Icon aus der gepinnten Theme-Closure auf; Provenienz und Hash
werden mitgebaut. Nach dem Rebuild bestanden Documents ebenfalls den gesamten
Rollback-/Reload-/Locale-/Shellstyle-Lauf ohne Consolefehler. Der erneute
headed Lauf prueft zudem im eingebetteten, CTOX-Documents-Fork, dass
Kommentare schreibbar sind und die Aenderungsverfolgung tatsaechlich in den
Modus `Ueberpruefung` wechselt
([Ablauf](../src/apps/business-os/office-engine/oracle/flows/document.production-lifecycle.playwright.js),
[Evidenz](../src/apps/business-os/office-engine/oracle/evidence/document.production-lifecycle.json)).

Der isolierte Clean-Profile-Erststart ist ebenfalls fuer beide Apps belegt.
Je ein neues Chromium-Profil hatte vor dem App-Mount exakt keine
Local-/Session-Storage-Schluessel, oeffnete die CTOX-Fork-UI in der jeweiligen
CTOX-ESM-Kapsel ohne
Consolefehler und wurde nach dem Screenshot geschlossen
([Ablauf](../src/apps/business-os/office-engine/oracle/flows/office.clean-profile.playwright.js),
[Evidenz](../src/apps/business-os/office-engine/oracle/evidence/office.clean-profile.json)).

Der Offline-/Reconnect-Lauf setzt Chromium fuer beide Apps nach vollstaendigem
Mount der CTOX-Fork-UI innerhalb der ESM-Kapsel offline. Der Editor-Blob wird weiter lokal in die
RxDB-Chunk-Collection geschrieben, aber `awaitInSync()` verhindert jeden
nativen Command. Erst das echte Browser-Online-Event setzt `replicationUp`
wieder hoch und loest exakt einen terminalen Command mit Transport
`rxdb-webrtc` aus; ein HTTP-Datenfallback existiert nicht
([Ablauf](../src/apps/business-os/office-engine/oracle/flows/office.offline-reconnect.playwright.js),
[Evidenz](../src/apps/business-os/office-engine/oracle/evidence/office.offline-reconnect.json)).

Beim anschliessenden nativen Restart-Audit wurde ein bis dahin von den
Browser-Mocks verdeckter Backendfehler gefunden: `office.spreadsheet.commit`
verwendete nach erfolgreichem XLSY-Export die Document-spezifische
Persistenzfunktion. Dadurch waeren Spreadsheet-Versionen und -Chunks in
Document-Collections gelandet. Der Rust-Pfad ist jetzt kind-spezifisch:
`spreadsheet_versions`, `spreadsheet_blob_chunks`, `spreadsheet_id`, XLSX-MIME,
autoritative Base-Version, idempotenter Replay und hashverifizierter Readback.
Der Regressionstest
`office_commits_survive_store_reopen_and_spreadsheets_never_write_document_collections`
schliesst alle Store-Handles vor dem Commit und besteht fuer DOCX und XLSX; er
prueft zusaetzlich, dass keine Spreadsheet-Version nach `document_versions`
leakt.

Die abschliessenden Restart-Smokes dispatchen einen echten
`office.document.commit` beziehungsweise `office.spreadsheet.commit` waehrend
jeweils eines beaufsichtigten CTOX-Prozess- und nativen Peer-Neustarts. Der
erste Lauf deckte eine Race-Behandlung fuer
abgebrochene alte Replication-Promises auf; der Office-spezifische Lauf fand
danach ausserdem, dass terminale Office-Commits zwar im nativen Store, aber
nicht rechtzeitig im RxDB-Projektionsstore sichtbar waren. Office-Commit schreibt
Dokument/Spreadsheet, Version und Blob-Chunks deshalb nach der atomaren
Business-Store-Transaktion direkt in die vorhandenen RxDB-Collections; der
periodische Projektor bleibt Reconciliation. Der finale Lauf erhielt im Browser
bei beiden Apps den v2-Zeiger und die v2-Version, Rust bestaetigte jeweils
einen vollstaendigen kanonischen Blob, und alle 23 Prozess-Lifecycle-Ereignisse
pro Lauf hatten bekannte Signale. Die waehrend des absichtlichen
HTTP-Server-Ausfalls beobachteten Asset-Requestfehler sind Restart-Evidenz,
kein Business-Datenfallback. Der Release-Soak-Workflow fuehrt beide Gates als
getrennte Ein-Versuch-Matrizen aus und laedt ihre strukturierten Resultate als
Artefakte hoch; die Trennung verhindert, dass ein globaler Shell-Startup-Flake
des zweiten Laufs durch einen unmittelbar vorherigen Vollsystemlauf verdeckt
oder per Retry akzeptiert wird.

Der normale Business-OS-CI- und Release-Pfad fuehrt `check:office` bereits
blockierend ueber das Paketkommando `npm test` aus. Damit werden die
24-Feature-Matrix, die ESM-UI-Closure-/Provenienz-Assertions und die
versionierte Zwei-Kind-Restart-Evidenz bei Pull Requests und Releases geprueft;
der Release-Soak ergaenzt die beiden realen Browser-/Daemon-Ausfuehrungen.

Das Release-Gate erzeugt daraus zusaetzlich ein einzelnes fail-closed
`ctox-office-release-candidate-evidence.json`. Der Capture validiert fuer beide
Office-Arten exakt einen erfolgreichen Versuch, dieselbe volle Git-Revision,
einen sauberen Source-Checkout und leere Evidence-Probleme. Er hasht beide
Restart-Matrizen sowie die versionierten DOCX-/XLSX-Korpus-Evidenzen und laedt
das Resultat gemeinsam mit der Produktions-Smoke-Evidenz hoch. Der finale
Release-Job haengt dasselbe JSON zusaetzlich als dauerhaftes Release-Asset an,
damit der Nachweis nicht von der Aufbewahrungsfrist eines Actions-Artefakts
abhaengt. Das Artefakt
bleibt bis zum erfolgreichen Ende des gesamten Workflows absichtlich
`pending_completion`; erst danach darf es mit dem tatsaechlichen
Release-Zeitpunkt und `release_workflow_status=passed` in `rollout.json`
uebernommen werden. Damit werden keine Release-Ergebnisse vorweggenommen, und
die spaetere Promotion muss keine Hashes manuell rekonstruieren.
Die beiden Matrixdateien entstehen unter dem ignorierten `runtime/build/`;
dadurch kann der erste Lauf den zweiten nicht als untracked Worktree-Aenderung
verunreinigen, und beide attestieren denselben sauberen Release-Commit.

Der Release-Bundle-Validator verlangt nun außerdem die Documents- und
Spreadsheets-Wrapper, beide ESM-Einstiegspunkte, iframe/Provenienz sowie die
CTOX-Documents-Fork-/SpreadsheetEditor-HTML- und Word-/Cell-SDK-
Einstiegspunkte. Der Office-Test hasht nicht mehr nur Stichproben, sondern jede
in `provenance.json` inventarisierte Runtime-Datei. Auch der Managed-Install-
Asset-Guard vergleicht diese kritischen Office-Pfade zwischen Source,
aktivem Release und persistentem Business-OS-State. Ein Release ohne die
echte Office-Closure scheitert damit vor der Auslieferung beziehungsweise beim
Installationsaudit.

Der abschliessende Wrapper-Audit fand zudem eine Produktluecke hinter einem
bereits bestandenen Engine-Feature: Die Documents-App uebergab der echten
DocumentEditor-Runtime bislang `comment:false` und `review:false`. Kommentare
und Aenderungsverfolgung sind nun bei vollstaendiger Schreibberechtigung auch
im realen Business-OS-Wrapper aktiviert und fallen bei fehlendem Schreibrecht
gemeinsam auf read-only zurueck. Ein Wrappertest pinnt beide
Berechtigungszustaende.

Die Runtime-Einstellungen starten nun typisiert mit `ctox_documents` bzw.
`ctox_spreadsheets`; ein
explizites `legacy` bleibt als Rollback erhalten und ist in Browser- und
Rust-Tests abgenommen. Damit ist die technische Standardumschaltung erfolgt.
Die Entfernung der Legacy-Implementierungen bleibt bewusst an eine weitere
stabile Release-Periode gebunden und kann nicht durch einen lokalen Testlauf
vorweggenommen werden.

Diese letzte Periode ist nicht mehr nur Prosa. Der maschinenlesbare
[`rollout.json`](../src/apps/business-os/office-engine/rollout.json)-Vertrag
haelt fest:

- die Default-Umschaltung liegt nach dem letzten veroeffentlichten Vor-Port-
  Release `0.3.27` und zugleich nach dem hoechsten vorhandenen Vor-Port-Tag
  `v0.3.31`; der naechste kollisionsfreie Switch-Kandidat ist damit mindestens
  `v0.3.32`. Der im Source-Manifest noch stehende Wert `0.3.22` ist bewusst
  keine Rollout-Semver-Grenze,
- das tatsaechliche Switch-Release muss Tag, Git-Revision, Zeitpunkt,
  erfolgreichen Release-Workflow und beide zero-retry Restart-Matrix-Hashes
  nachweisen,
- danach ist mindestens ein weiteres erfolgreiches Release mit beiden
  Restart-Matrizen und beiden Korpus-Evidenzhashes erforderlich,
- bis dahin muessen alle 24 Features `differential_passed`, beide typisierten
  Legacy-Rollbacks vorhanden und `legacy_removal_authorized=false` bleiben,
- erst mit vollstaendiger Release-Evidenz darf der Vertrag gleichzeitig
  `legacy_removal_authorized=true` und alle Featurestatus `shipped` zulassen.

`validate-rollout.mjs` prueft diesen Vertrag blockierend in `check:office` und
damit ueber `npm test` sowohl im normalen CI als auch im Release-Gate. Der
aktuelle Stand ist bewusst `stable_releases=0/1`: Die technische
Implementierung ist fertig, aber der zeitliche Produktionsnachweis wurde nicht
durch lokale Tests vorgetaeuscht. Die reale GitHub-Pruefung vom 2026-07-13
ergab als juengstes veroeffentlichtes Release weiterhin `v0.3.27`; ein
Switch-Release ab `v0.3.32` und damit auch ein stabiles Folgerelease existieren
noch nicht.

## Naechste konkrete Schritte

1. Beim ersten realen Release mit CTOX Documents und CTOX Spreadsheets als
   Standard das erzeugte
   `ctox-office-release-candidate-evidence.json` nach erfolgreichem Abschluss
   um den tatsaechlichen Release-Zeitpunkt und Status `passed` ergaenzen und in
   `rollout.json.default_switch_release` eintragen.
2. Mindestens ein nachfolgendes Release mit allen geforderten zero-retry- und
   Korpus-Hashes unter `qualifying_releases` erfassen.
3. Erst wenn der Validator dann die Freigabe bestaetigt, Legacy-Code entfernen,
   `legacy_removal_authorized=true` setzen und die 24 Featurestatus samt
   Evidenzdateien auf `shipped` heben.

## Pflege dieses Dokuments

Bei jedem Office-Port-Change sind mindestens folgende Punkte zu pruefen:

- Stimmen Status und Abhaengigkeiten mit `features.json` ueberein?
- Sind neue Evidenz- und Flow-Dateien verlinkt?
- Wurde der Zaehler fuer Documents, Spreadsheets und Gesamtstand aktualisiert?
- Sind neue Risiken, Entscheidungen oder Scope-Aenderungen dokumentiert?
- Sind die ausgefuehrten Checks und bekannte Blocker festgehalten?
