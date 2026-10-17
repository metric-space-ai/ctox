# FIX10 – restliche App-Mängel (Shell v4)

Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`, Kopf 6a59ed2f3, Basis `src/apps/business-os/`. Stand 16.10.2026.
Kein git außer Lesen, keine Subagents.

Geändert sind threads (Handler, Tests, Deklaration, V4-HANDLERS.md), reports (Handler entfernt, Deklaration, module.json, V4-PORT.md, ein Testkommentar), mail, invoices, appsec-pentest und research (UI und Tests) sowie `v4/frame/frame.css`.
Die Manifeste unter `v4/manifests/` sind neu erzeugt.

Mit FIX11 abgestimmt:
- Umbruch bei invoices und research erst auf 600 px, auf Hinweis von FIX11 dann auf die Shell-Stufe 768 px umgestellt. Der Vertrag verlangt das.
- conversations fasse ich nicht an; den Chip „Arbeitsauftrag“ übernimmt FIX11.

## 1. Tote Rückruf-Kette `reports.delegation.*`

Vor dem Löschen per grep über das ganze Repo geprüft, einschließlich `src/core`:
- `reports.delegation.*` wird nur von den reports-Handlern, threads `_approval_targets.js`, deren Tests und Doku genannt.
- Keine UI und kein Rust-Code sendet es.
- `threads.ctox_approval.request` mit `module:threads` sendet reports seit LOOP-2 nicht mehr. In `modules/reports/index.js` gibt es keinen Aufruf; die übrigen Absender sind threads selbst und der Kern.

Entfernt:
- `modules/reports/handlers/_delegation.js`, `reports.delegation.complete.js`, `reports.delegation.failed.js` und `handlers/tests/delegation.test.mjs`. Der Ordner `handlers/` ist damit leer und entfällt.
- `commands_in` und dessen `_basis` in `modules/reports/v4-manifest.json`.
- In threads `_approval_targets.js`:
  - die reports-Konstanten,
  - der Listeneintrag; `APPROVAL_CALLBACKS` ist jetzt `[]`,
  - die beiden `case`-Zweige; `dispatchApprovalCallback` gibt immer null zurück.
- `commands_in from:['reports']` (`threads.ctox_approval.request`) und die Basis in `modules/threads/v4-manifest.json`. Laut LOOP.md gehört es zur Kette.

Der allgemeine Rückrufweg in threads `_agent.js` bleibt bestehen:
- Eine Anfrage mit `callback` ist jetzt `invalid` („not a declared approval callback“).
- Altbestände werden abgeschlossen, aber nicht weitergeleitet (Log `warn`).

Texte:
- `modules/reports/module.json` (nur `layout.right` und `third_pane_justification`): Übergabe an den Coding Agent in der Code-Ansicht.
- `modules/threads/V4-HANDLERS.md`: Kasten „Stand FIX10“ über dem THR2-Abschnitt.
- `modules/reports/V4-PORT.md`: Nachtrag.
- Kommentar im reports-Test.

Tests:
- In `threads/handlers/tests/agent.test.mjs` und `targets.test.mjs` sind die Weiterleitungsprüfungen jetzt Nicht-Weiterleitungsprüfungen.
- Neu: „FIX10: Rückruf-Liste leer, kein Versand – auch nicht an reports.delegation.*“.
- Die Anfrage mit dem reports-Rückruf ist invalid, ein Altbestand mit reports-Rückruf wird nicht weitergeleitet.
- Die Generatorableitung erwartet `commands_out` = [].

## 2. mail: Einstellungen per Esc, nie über einer anderen Schublade

- Esc schließt die Mail-Einstellungen, der Fokus geht zurück auf den Einstellungen-Knopf.
- Beim Öffnen fokussieren die Einstellungen ihren Schließen-Knopf. So wirkt Esc auch nach einer Rückfrage über das Blatt.
- Die drei Schubladen (Verfassen, Neue Gruppe, Einstellungen) schließen sich gegenseitig.
  - `openComposer` schließt Einstellungen und Gruppen-Editor.
  - Gruppe und Einstellungen geben die Verfassen-Schublade über `releaseComposer()` frei.
  - Mit Inhalt fragt sie dabei wie beim Schließen „Entwurf behalten?“.
  - Abbrechen lässt den Entwurf offen, und die andere Schublade öffnet nicht.
- Vorher verwarf `openMailboxAdmin` einen Entwurf still per `closeComposer()`.

## 3. invoices: Positionen bei 420 als Karten

- Jede Positionszelle trägt `data-col` und `data-label`.
- Neu ist die Zelle „Betrag“ (`.invoices-line-amount`, netto = Menge × Einzelpreis). Sie ist nur in der Karte sichtbar; breit bleibt die Tabelle unverändert.
- Im Editor folgt der Betrag jeder Eingabe. Die Eingabefelder tragen eine `aria-label`.
- `@container business-app-window (max-width: 768px)` (einspaltige Shell-Stufe, kein @media) stapelt die Zellen zur Karte:
  - Zeile 1: Beschreibung und ×.
  - Zeile 2: Menge, Einheit, × Einzelpreis = Betrag.
  - Zeile 3: USt % … SKR.
- Kopf und Pos-Spalte entfallen dort.
- Die alte Regel `min-width: 620px` der Editortabelle ist entfernt.
- Gilt für die Detailansicht und den Editor.

## 4. appsec-pentest (nur index.js)

- Die Profilwahl (Einfach und Workbench) zeigt „Ausgewogener Test“, „Gründlicher Test“, „Maximaler Test“ und „Schneller Test“ statt standard/full/deep/quick. Die Werte bleiben.
  - Gesetzt werden die Texte in `applyStaticLabels` über `friendlyProfile`, also ohne index.html.
  - Die Scanner-Pille zeigt keine rohe Profilkennung mehr.
- Neu ist `toEpochMs`, exportiert, nach dem Muster von interviews und knowledge:
  - Zahlen unter 1e11 gelten als Sekunden, Text läuft über Date.parse.
  - 0, Ungültiges und alles vor 2000 zählt als „kein Datum“.
- `formatDay`, `formatTimestamp`, `formatTime` (neu) und `formatApprovalExpiry` formatieren in der App-Sprache: deutsch `13.10.2026` bzw. `13.10.2026, 10:00`, englisch en-GB.
- Ohne Datum steht nichts da, auch keine leere „until“-Pille.
- Ursache von „1/1/1970“: Die Fixture-Werte `updated_at_ms: 1–3` wurden ungeprüft als Millisekunden formatiert, im US-Format des Browsers.

## 5. research: Messwerte ohne Seitwärts-Scroll

- Beide Messwert-Tabellen kommen aus `renderMeasurementTable` mit Spaltenlisten (`DIRECT_/DERIVED_MEASUREMENT_COLUMNS`). Spalten und Zahlen sind unverändert.
- Jede Zelle trägt `data-label` und den vollen Wert als `title`. Der Kopf-Tooltip lautet jetzt „Beschriftung: Hilfe“.
- CSS:
  - `.research-measurements` hebt die `min-width: 1060px` der Quellentabelle auf.
  - Die Spalten sind fest, einzeilig, mit Ellipse und Innenabstand 6 px.
  - Die Spaltenanteile sind so gewählt, dass Zahlen bei den gemessenen Breiten der mittleren Spalte nie abgeschnitten werden: 656 px bei 1280, 562 px bei 900.
- Bis 768 px werden Karten „Beschriftung: Wert“ (zweispaltig), Quelle und Methode über die volle Breite.
- `__researchTestHooks` exportiert die beiden Render-Funktionen.

## 6. Desktop: G8 rot

Das Geometrie-Labor (`geometry-v4.mjs --app desktop`) zeigt: Es ist kein Element des Rahmen-Desktops. Es ist das erste Symbol `div.desktop-icon` der Desktop-App (`modules/desktop`), wenn sie als Fenster läuft. Es liegt bei 24/24 bzw. 12/12 unter dem Icon-Header (Zone 82 × 74).
- Auf HEAD rot bei 420, 900 und 1280, nicht nur bei 420.
- Die Desktop-Fläche des Rahmens (`.v4f-desktop`, FR6d) liegt nicht in einem Fenster und war nie betroffen.

Behoben in `v4/frame/frame.css`, wie im Auftrag:
- `.v4f .shell-window-content [data-app="desktop"] .desktop-icons` beginnt unter der höheren reservierten Zone: `top: max(--shell-icon-zone-height, --shell-controls-zone-height)`.
- Die Regel folgt dem Muster der vorhandenen Kit-Zonenregeln in frame.css. `desktop.js` und `modules/desktop` sind nicht angefasst.
- Die Shell-Optik bleibt: FR6d-G0 Desktop = 1 (100 %) in mock und stub.

## Abnahme (selbst gefahren, `CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9`)

| Prüfung | Ergebnis |
|---|---|
| App-Tests `node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/<app>/tests/*.test.mjs` | threads/handlers 73/73, threads 21/21, reports 10/10, mail 43/44 (1 skip MAIL_SHOTS), invoices 96/96, appsec-pentest 12/12, research 82/82, desktop 13/13 |
| `node v4/tools/manifest/gen-manifest.mjs` neu, danach `--check` | Exit 0; `gen-manifest.test.mjs` 43/43 |
| `node v4/os-primitives/check-caps.mjs --calls` | ABWEICHUNG 0 (202 Aufrufe, OK 195); `check-caps` OK |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 |
| `node v4/debt/ratchet.mjs` | OK |
| `v4/tools/geometry/contrast.mjs --app mail,invoices,appsec-pentest,research,reports,threads,desktop` bei 1280 und 420 | G10 OK, 0 zu schwach |
| Rahmen-Labor `node v4/frame/lab/frame.test.mjs mock` / `… stub` | 167/167, 198/198; FR6d-G0 desktop = 1 |
| `geometry-v4.mjs --app desktop --widths 420,900,1280` | G8 grün in allen drei Breiten (HEAD: G8 rot in allen drei). 900 bleibt G6 rot (Spaltenzählung), genauso auf HEAD |
| `geometry-v4.mjs --app mail,invoices,appsec-pentest,research,desktop --widths 420` | 4/5 OK. invoices G4: „Rechnungen“ im Kopf abgeschnitten, genauso auf HEAD (Kopf-CSS, siehe Offen) |
| `rundgang2.mjs --app mail,invoices,appsec-pentest,research --widths 420,900` | 8× OK, 0 abgelehnt, 0 Fehler. Geometrie bei 420: 0. Bei 900 noch invoices `ctox-table-wrap 547>504` und appsec `ctox-pane-scroll 530>478` (siehe Offen) |

Bilder angesehen:
- rundgang2-Bilder unter `$SCRATCH/fix10/r2`.
- Test-Bilder `modules/invoices/tests/shots/invoices-420{,-draft}.png`.
- Messwert-Karten unter `$SCRATCH/fix10/research-*.png`.
- Desktop unter `$SCRATCH/fix10/geo-desktop2/desktop/420.png`.

Gesehen:
- invoices 420: Detail „Beratung / 1 h × 100,00 € = 100,00 € / USt % 19 % … SKR 8400“, Editor mit Feldern in derselben Anordnung.
- mail 900: Einstellungen allein, keine Verfassen-Schublade darunter.
- appsec 420: „Profil: Ausgewogener Test“, Listendaten 13.10.2026 usw.
- research 420: Karten mit „Quelle“, „Propeller“, „RPM“ und mehr; 1280/900: Tabelle ohne Scroll, Text mit Ellipse, Zahlen vollständig.
- Desktop 420: Symbole unter dem Icon-Header.

`$SCRATCH` = `/private/tmp/claude-501/-Users-michaelwelsch-Documents-ctox/d0121889-1808-4958-a44a-06a489b1241e/scratchpad`

### Tests und Mutationsbelege

Jeweils so geprüft:
- App-Dateien durch den HEAD-Stand ersetzt (`git show`, nur lesend).
- Test gefahren.
- Zurückgestellt und per `cmp` bestätigt (Skript `$SCRATCH/fix10/mut.sh`).

| Punkt | Test | Ohne Fix (HEAD) |
|---|---|---|
| 1 | `threads/handlers/tests/targets.test.mjs` „FIX10: Rückruf-Liste leer …“, „request: Rückruf außerhalb der festen Liste“, „Altbestand mit nicht gelistetem Rückruf“; `agent.test.mjs` „request: Rückruf nur im Namensraum …“ | 5 rot: „expected rejection with invalid“ (2×), Weiterleitung an reports, Liste nicht leer |
| 2 | `mail/tests/v4-api.test.mjs` „FIX10: Mail-Einstellungen schließen per Esc …“ | „Esc schließt die Mail-Einstellungen“ actual `['[data-mail-mailbox-admin]']` |
| 3 | `invoices/tests/v4-api.test.mjs` „FIX10: Positionen bei 420 als Karten …“ | nur CSS alt: „Detail-Positionen scrollen seitlich: overflow 233“; JS+CSS alt: rot |
| 4 | `appsec-pentest/tests/fix10.test.mjs` | Optionen „standard/full“ statt „Ausgewogener/Gründlicher Test“ (rot) |
| 5 | `research/tests/fix10.test.mjs` (1280, 900, 420; Zahlen nie abgeschnitten) | nur CSS alt: „direct 1280: scrollt seitlich um 404px“ |
| 6 | `geometry-v4.mjs --app desktop` (G8) | frame.css alt: G8 rot bei 420 und 900 („div.desktop-icon unter Icon-Header“) |

## Offen / Übergabe

- **invoices 900:** Die Positionstabelle (breit, 8 Spalten) ist 547 px in einer 504 px breiten Detailspalte und scrollt dort seitlich. Bei 420 ist das behoben (Auftrag). Bei 900 verlangt der Vertrag die Tabellenform, weil es keine Shell-Stufe zwischen 768 und 1024 gibt. Kandidat: schmalere Eingabefelder bzw. Spalte „SKR“ kürzen.
- **invoices 420 G4:** „Rechnungen“ im Pane-Kopf abgeschnitten, schon auf HEAD. Das ist Kopf-CSS, also das Thema von FIX11; invoices ist für FIX11 aber gesperrt. Es braucht einen Besitzer.
- **appsec-pentest 900:** `ctox-pane-scroll 530>478` im rundgang2, nicht Teil von FIX10.
- **Desktop-App als Fenster:** Der Rahmen hat mit `.v4f-desktop` eine eigene Desktop-Fläche. Ob `modules/desktop` im v4-Rahmen überhaupt als Fenster startbar sein soll, ist offen. Das Labor startet sie als Fenster; die Regel in frame.css macht diesen Fall zonenfest.
- **Generator:** `gen-manifest.mjs` hat beim Neuerzeugen auch Manifeste anderer Apps neu geschrieben, wegen Dateiänderungen des parallelen FIX11 (consent, conversations, credentials, customers, cv-print-builder, desktop, documents, esign, explorer, intake, iot, matching, placements, shell, shiftflow, spreadsheets). Das ist reine Generatorausgabe; `--check` gibt 0.
- Geänderte Dateien unter `modules/*/tests/shots/*.png` stammen von den bestehenden Screenshot-Tests.
