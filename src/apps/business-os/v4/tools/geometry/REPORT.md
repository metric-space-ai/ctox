# Shell v4 – Geometrie-Labor (Strang GEO)

Stand 15.10.2026, Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`), Kern-Stub-Pin `cff5ba5`
(`~/.local/state/shell-v4/stub-pins/cff5ba5`, per `CTOX_APP_API_STUB` überschreibbar). Keine App-, Shell- oder
Kerndatei geändert; geschrieben nur unter `v4/tools/geometry/`.

## Werkzeug

| Datei | Rolle |
|---|---|
| `geometry-v4.mjs` | Labor: Server (Wurzel business-os, `/~a/<app>@<ver>/` → `modules/<app>/`, `/~stub/` → Pin, `ctox:shared/` wie `v4/sw.js` umgeschrieben), Laborseite (Stub-Kernel → Fixtures per `sim.push` → `publishAppVersion` **ohne Titel** → `sim.startShell` mit dem echten `v4/frame/index.js` + `v4/shell/app-host.js` → `openWindow` → Maximieren über die Fenster-Steuerung), Messung G1–G6, Screenshots, `results.json`, Galerie `index.html` |
| `fixtures.mjs` | Fixture-Erzeuger aus `collections.schema.json` (eigene Collections 6 Zeilen, `foreign_collections` aus `v4/manifests/<id>.manifest.json` 4 Zeilen aus dem Schema der besitzenden App). Titelfelder tragen Firmennamen/Dokumenttitel (u. a. ein 92-Zeichen-Titel als Überlauf-Probe), IDs sind lesbar (`note-001`), Status-/Typwerte aus Vergleichen im App-Code abgeleitet, Geheimnis-/Hashfelder leer |
| `report-tables.mjs` | Markdown-Tabellen aus `out/<variante>/results.json` |

Lauf:

```
node v4/tools/geometry/geometry-v4.mjs [--app <id>[,<id>…]] [--widths 1280,900,420] [--shared] [--selftest] [--out <dir>]
node v4/tools/geometry/report-tables.mjs frame|shared-css [--detail]
```

- Ausgaben: `out/frame/` (echter Rahmen, Standard) bzw. `out/shared-css/` (`--shared`), je App `<breite>.png` und
  `g6-<breite>-in-1280.png`, dazu `results.json` und `index.html` (Galerie).
- **Portiert** = `index.js` exportiert `mount(<root>, <api> …)` (nicht `ctx`) **und** `scan.mjs --app <id>` ≤ 1 Stelle,
  wobei die eine Stelle nur `R17-schema` (schema.js-Rest) sein darf.
- **Variante `--shared`** lädt zusätzlich, was die v3-Shell jedem Fenster gab und der v4-Rahmen heute NICHT lädt:
  `shared/base.css` (Pane-Grammatik) und nur die Custom-Property-Deklarationen aus `app.css` (`:root`/`html`/`body`),
  außerdem schreibt sie `ctox:shared/` auch in CSS um. Sie ist eine Labor-Annahme, kein Rahmenverhalten.
- **`--selftest`** hängt nach dem Mount absichtlich lecke Regeln an (`[data-app=…] button`, `body`); G5 muss rot
  werden (gemessen: notes 1280 → G5 „50 Elemente außerhalb root ändern ihren Stil“, Beleg nennt den Fenster-Knopf
  `button.shell-window-control--layout`).

### Kriterien, wie gemessen

| | Messung im Browser (Chromium headless 1223, Playwright 1.60) |
|---|---|
| G1 | `.v4-app-root` scrollt nicht horizontal; kein innerer Container mit `overflow-x:auto/scroll` und `scrollWidth > clientWidth`; kein sichtbarer Teil (nach Clipping durch Vorfahren innerhalb root) eines Elements links/rechts oder oberhalb des Inhaltsanfangs; nichts Neues an `document.body`; Punktabtastung (27 Punkte je Fläche) über Fensterkopf, Zone und Taskleiste trifft kein App-Element |
| G2 | Labor veröffentlicht ohne Titel, der Host setzt vorab nur die App-ID; protokolliert wird jeder `setTitle`-Aufruf. Rot, wenn die App nie `setTitle` ruft, der Endtitel leer, gleich der App-ID, mit UUID oder mit Fixture-ID |
| G3 | sichtbare Textknoten + `placeholder` in root gegen alle 28 Muster aus `vocabulary.json`; ausgenommen `[data-v4-allow…]` und Literale aus Quellzeilen mit `v4-allow: label` (gleiche/folgende Zeile wie `guard-v4.mjs`); Zustandszone (`.v4-status-zone`) muss verborgen sein |
| G4 | sichtbares Element mit `text-overflow: ellipsis` und Überlauf oder `line-clamp` mit Überlauf, außer unter `[data-v4-allow-ellipsis]`; Ausgabe trennt „Liste“ (li/tr/role=row… oder ≥ 3 gleichklassige Geschwister) von „Text“ |
| G5 | entscheidend: berechnete Stile (21 Eigenschaften) aller Elemente außerhalb root mit und ohne App-Stylesheets (`disabled`); Beleg: App-Selektoren, die Elemente außerhalb root treffen |
| G6 | Signatur (Spaltenzahl nebeneinanderstehender Blöcke ≥ 100 px + Menge sichtbarer Bausteine) der maximierten App im Viewport B gegen ein freies, gleich breites Fenster im Viewport 1280 (gleich = gleiche Spalten und ≥ 90 % gleiche Bausteine); statisch: keine `@media (min/max-width)` in App-CSS |

## Lauf 15.10.2026 (13:50 UTC, calendar mit korrigierten Fixtures nachgemessen und eingemischt)

39 Apps gemessen, je 1280/900/420 = 117 Läufe je Variante. Übersprungen (nicht portiert): `conversations`
(kein `mount(root, api)`, Scan 102 Stellen). Während des Laufs portierten andere Stränge weiter: `coding-agents`,
`mail` und `cv-print-builder` kamen zwischen meinen Läufen hinzu. `app-store` war beim Hauptlauf portiert, zeigt beim
Nachlauf aber 5 Scan-Stellen (`R9-actor`, `modules/app-store/index.js:13,15,2466,2478`, plus `schema.js`). Deshalb
steht es in `results.json` unter `skipped` und hat trotzdem Messwerte.

**Kurzbilanz**

- Echter Rahmen (`frame`): 73/117 OK. Shared-CSS-Variante: 59/117 OK. Mit Styling werden mehr Texte abgeschnitten
  (G4 21 → 45), weil erst dann `text-overflow` und Spalten greifen. Die grünen Läufe im echten Rahmen sind also
  kein Qualitätsbeleg (Befund 1).
- **G2 grün in allen messbaren Läufen beider Varianten** (im echten Rahmen ohne documents/spreadsheets, Befund 2): jede App setzt ihren Titel selbst über `api.shell.setTitle`, kein
  Titel ist App-ID, UUID oder Datensatz-ID.
- **G5 grün in allen messbaren Läufen:** keine App-CSS verändert einen berechneten Stil außerhalb von root.
- **G6:** nur `desktop` hängt am Viewport (`@media 767px`). Bei allen anderen ist die Layout-Signatur im
  Viewport B gleich der eines gleich breiten Fensters in 1280. `documents`/`spreadsheets` sind im echten Rahmen nicht
  messbar (Befund 2).
- G1/G3/G4 sind die eigentliche Arbeitsliste je App (unten).

### Echter Rahmen (`out/frame/`)

Variante `frame`, 2026-10-15T13:50Z, Breiten 1280/900/420, 73/117 Läufe OK.

| App | 1280 | 900 | 420 | Titel | Tokens fehlen | Seitenfehler |
|---|---|---|---|---|---:|---:|
| app-store | OK | OK | OK | App Store | 21 | 0 |
| appsec-pentest | OK | OK | OK | Penetration Testing | 8 | 0 |
| browser | G3 | G3 | G3 G4 | Browser | 12 | 0 |
| buchhaltung | OK | OK | G1 | Buchhaltung | 28 | 0 |
| calendar | G4 | G4 | G4 | Kalender | 49 | 0 |
| coding-agents | OK | OK | OK | pi Coding Agent | 12 | 2 |
| consent | OK | OK | OK | Einwilligungen | 7 | 0 |
| creator | OK | OK | OK | App Creator | 7 | 0 |
| credentials | OK | OK | OK | Zugangsdaten | 2 | 0 |
| ctox | G3 | G3 | G1 G3 G4 | Crew | 36 | 0 |
| customers | OK | OK | OK | Kunden | 14 | 0 |
| cv-print-builder | OK | OK | OK | CV Print Builder | 11 | 0 |
| desktop | G6 | G6 | G6 | Desktop | 14 | 0 |
| documents | G1 | G1 G6 | G1 G6 | – | – | 1 |
| esign | OK | OK | G4 | E-Signatur | 7 | 0 |
| explorer | OK | OK | G1 | Explorer | 8 | 0 |
| file-viewer | OK | OK | OK | Datei-Viewer | 6 | 0 |
| importer | OK | OK | OK | App Importer | 14 | 0 |
| intake | OK | OK | OK | Bewerbungseingang | 4 | 0 |
| interviews | OK | OK | OK | Interviews | 13 | 0 |
| invoices | OK | OK | OK | Rechnungen | 5 | 0 |
| iot | OK | OK | OK | IoT | 14 | 0 |
| knowledge | G4 | G4 | G4 | Knowledge | 30 | 0 |
| kundenpipeline | OK | OK | OK | Decision Hub | 5 | 0 |
| mail | OK | G4 | G4 | Mail | 11 | 0 |
| matching | OK | OK | OK | Matching | 19 | 0 |
| nachweise | OK | G4 | G4 | Nachweise | 4 | 0 |
| news | G3 G4 | G3 G4 | G3 G4 | News | 29 | 0 |
| notes | G1 | G1 | OK | Notizen | 21 | 0 |
| outbound | G1 G3 | G1 G3 | G1 G3 G4 | Outbound | 27 | 0 |
| placements | OK | OK | OK | Vermittlungen | 2 | 0 |
| reports | OK | OK | G4 | Bugs & Features | 8 | 0 |
| research | G3 | G3 | G3 G4 | Web Research | 27 | 0 |
| shiftflow | OK | G1 | G1 | Einsatzplanung | 18 | 0 |
| spreadsheets | G1 | G1 G6 | G1 G6 | – | – | 1 |
| submissions | OK | OK | OK | Vorstellungen | 2 | 0 |
| support | OK | OK | G4 | Support | 10 | 0 |
| threads | OK | OK | G4 | Threads | 15 | 0 |
| tickets | OK | OK | OK | Tickets | 8 | 0 |

Fehlschläge je Kriterium (Läufe): G1 16 · G2 0 · G3 15 · G4 21 · G5 0 · G6 7

### Mit gemeinsamer Theme-CSS (`out/shared-css/`, Labor-Annahme)

Variante `shared-css`, 2026-10-15T13:50Z, Breiten 1280/900/420, 59/117 Läufe OK.

| App | 1280 | 900 | 420 | Titel | Tokens fehlen | Seitenfehler |
|---|---|---|---|---|---:|---:|
| app-store | OK | OK | G1 | App Store | 4 | 0 |
| appsec-pentest | OK | OK | OK | Penetration Testing | 0 | 0 |
| browser | G3 G4 | G3 G4 | G3 G4 | Browser | 0 | 0 |
| buchhaltung | OK | G1 | G1 | Buchhaltung | 6 | 0 |
| calendar | G4 | G4 | G4 | Kalender | 36 | 0 |
| coding-agents | OK | OK | OK | pi Coding Agent | 0 | 2 |
| consent | OK | OK | OK | Einwilligungen | 0 | 0 |
| creator | OK | OK | OK | App Creator | 0 | 0 |
| credentials | OK | OK | OK | Zugangsdaten | 0 | 0 |
| ctox | G3 G4 | G1 G3 G4 | G1 G3 G4 | Crew | 11 | 0 |
| customers | G1 | G1 | G1 | Kunden | 0 | 0 |
| cv-print-builder | OK | OK | OK | CV Print Builder | 0 | 0 |
| desktop | G6 | G6 | G6 | Desktop | 5 | 0 |
| documents | G3 G4 | G3 G4 | G3 | Dokumente | 6 | 0 |
| esign | G4 | G4 | G4 | E-Signatur | 0 | 0 |
| explorer | OK | OK | G1 | Explorer | 0 | 0 |
| file-viewer | OK | OK | OK | Datei-Viewer | 0 | 0 |
| importer | OK | OK | OK | App Importer | 8 | 0 |
| intake | OK | OK | OK | Bewerbungseingang | 0 | 0 |
| interviews | OK | OK | OK | Interviews | 0 | 0 |
| invoices | OK | OK | OK | Rechnungen | 1 | 0 |
| iot | OK | OK | OK | IoT | 0 | 0 |
| knowledge | G4 | G4 | G4 | Knowledge | 12 | 0 |
| kundenpipeline | OK | OK | OK | Decision Hub | 0 | 0 |
| mail | OK | G4 | G4 | Mail | 2 | 0 |
| matching | OK | OK | OK | Matching | 1 | 0 |
| nachweise | G4 | G4 | G4 | Nachweise | 0 | 0 |
| news | G3 G4 | G3 G4 | G3 G4 | News | 10 | 0 |
| notes | G4 | G1 G4 | G1 | Notizen | 2 | 0 |
| outbound | G1 G3 G4 | G1 G3 G4 | G1 G3 G4 | Outbound | 7 | 0 |
| placements | OK | OK | OK | Vermittlungen | 0 | 0 |
| reports | G4 | G4 | G4 | Bugs & Features | 2 | 0 |
| research | G3 G4 | G3 G4 | G3 G4 | Web Research | 16 | 0 |
| shiftflow | OK | G1 | G1 G4 | Einsatzplanung | 4 | 0 |
| spreadsheets | G4 | G4 | OK | Tabellen | 2 | 0 |
| submissions | OK | OK | OK | Vorstellungen | 0 | 0 |
| support | G4 | G4 | G4 | Support | 2 | 0 |
| threads | G4 | G4 | G4 | Threads | 0 | 0 |
| tickets | OK | OK | OK | Tickets | 1 | 0 |

Fehlschläge je Kriterium (Läufe): G1 16 · G2 0 · G3 18 · G4 45 · G5 0 · G6 3

**G1/G3 im Einzelnen (Variante `--shared`)**, G4-Listen und alle Belege: `node v4/tools/geometry/report-tables.mjs shared-css --detail`

- G1 horizontal scrollende Innen-Container: customers (`nav.ctox-view-switch` 457 > 400,
  `customers-workbench-tabs` 800 > 406), outbound (`outbound-funnel-tabs` 534 > 4…308, Tabelle 1382 > 396…882),
  buchhaltung (`fibu-scrollable-container` 742 > 412), ctox (`ctox-flow-canvas` 850 > 420), shiftflow (Planer
  784 > 461), notes (`nn-editor-toolbar` 554 > 406), explorer 420 (`app-explorer-sidebar` 1363 > 420), app-store 420
  (`store-category-list` 880 > 420). Kein Element ragt aus root, keines hängt an `document.body`, nichts überdeckt
  Kopf, Zone oder Taskleiste.
- G3 Zustandstexte im Inhalt: outbound „1 wartet“, „CTOX arbeitet gerade nicht: 1 wartet“, „0 läuft“; ctox
  „In Progress“; research „loading“ (Rohstatus aus den Daten). Zone sichtbar: browser („Lieferantenbewertung“, nur der
  Sitzungsname ohne Fehlertext), news („Ausgabe fehlgeschlagen: Lieferantenbewertung“), documents (Text mit ID). Alle
  drei Zonen kommen aus Fixture-Zeilen mit Fehlerstatus, die die App korrekt über `api.status.report` meldet. Der Fehler
  liegt im Text, nicht im Melden.
- G4 betrifft vor allem den 92-Zeichen-Fixture-Titel in Listen (notes, knowledge, reports, support, threads, news,
  esign, outbound, calendar-Chips). Entscheiden: entweder Umbruch statt Ellipse oder `data-v4-allow-ellipsis` an
  Listentiteln.

## Befunde am Rahmen, Ladeweg und Manifest (gelten für alle Apps)

1. **Der v4-Rahmen lädt die gemeinsame Theme-CSS nicht.** `v4/frame/index.js` bringt nur `frame.css`. Die
   App-CSS der portierten Apps baut auf Tokens aus `app.css` (`--accent`, `--text`, `--line`, `--muted`,
   `--control-radius` …) und auf der Pane-Grammatik in `shared/base.css` (`.ctox-pane-header`, Spalten, Icon-Masken)
   auf. Entwurf §6 setzt sie voraus („45 Apps hängen an den gemeinsamen Theme-Tokens und pane-grammar“). Im echten
   Rahmen (Variante `frame`) fehlen je App bis zu 49 Tokens (Spalte „Tokens fehlen“). Die Apps zerfallen sichtbar:
   Knöpfe in Browser-Grundstil, leere Icon-Knöpfe, Listen als graue Kästen, keine Spalten
   (`out/frame/customers/1280.png`, `out/frame/app-store/1280.png`, `out/frame/notes/1280.png` mit Sperr-Overlay im
   Inhalt). G1–G6 fangen „ungestylt“ nicht als eigenes Kriterium. Darum gibt es die Variante `--shared`.
   **Entscheidung nötig (Shell-Strang):** gemeinsame Theme-CSS als Shell-Asset laden (Tokens + base.css, ohne die
   v2-Fensterregeln aus app.css) oder je App über `manifest.styles` deklarieren.
2. **`ctox:shared/` in CSS wird nicht umgeschrieben.** `v4/sw.js` schreibt nur Import-Spezifizierer in `.js/.mjs` um.
   `modules/documents/index.css:1` und `modules/spreadsheets/index.css:1` beginnen mit
   `@import url('ctox:shared/office-workspace.css')`. Chromium meldet `net::ERR_UNKNOWN_URL_SCHEME`. Der `<link>` des
   Hosts feuert `error`, der Host meldet „CSS /~a/documents@1.0.0/index.css nicht ladbar“ und ruft `reportFailed`.
   Der Kernel setzt die App auf `failed`, das Fenster verschwindet sofort wieder und der Launcher zeigt die App nicht
   mehr. In Variante `frame` sind beide Apps deshalb nicht messbar. Die Variante `--shared` schreibt die Regel auch in
   CSS um, dort starten beide.
3. **`layout.icon_svg` ohne `xmlns`** in 6 Manifesten (app-store, calendar, coding-agents, news, shiftflow,
   spreadsheets), jeweils ohne `icon_asset`. Als `data:image/svg+xml` in `<img>` rendert Chromium das nicht: Titelzeile,
   Taskleiste und Launcher zeigen ein kaputtes Bild (`out/*/app-store/1280.png`, `calendar/1280.png`). Abhilfe: im
   Manifest-Generator oder in `svgDataUrl` (`v4/frame/index.js`) `xmlns` ergänzen.
4. **Der Scope `[data-app="<id>"]` umfasst auch das Fenster-Chrome.** Der Rahmen setzt `data-app` auf das
   Fensterelement (`handle.element.dataset.app`), nicht nur auf `.v4-app-root`. Eine App-Regel
   `[data-app="x"] button` trifft deshalb die Fenster-Knöpfe (Gegenprobe `--selftest`). Heute leckt keine App
   (G5 in keinem messbaren Lauf beider Varianten), aber die Grenze ist das Fenster, nicht der Inhaltsbereich (Entwurf §6:
   „Chrome liegt außerhalb davon“). Abhilfe: `data-app` am Fenster umbenennen (z. B. `data-window-app`).
5. **`.v4-app-root` ist kein Container** (`container-type` fehlt in `frame.css`). Container-Queries der Apps greifen
   nur auf app-eigene Container. G6 bleibt trotzdem grün, außer bei `desktop`
   (`modules/desktop/index.css`: `@media … 767px`).
6. **App-seitige Funde, nicht Labor:** `coding-agents` lädt `new URL('../<id>/icon.svg', import.meta.url)`
   (`modules/coding-agents/index.js:565`). Unter `/~a/<app>@<ver>/` wird daraus `/~a/coding-agents/icon.svg` ohne
   Version, also HTTP 404 (auch über den SW). `documents` schreibt eine Datensatz-ID in die Zone
   („… (current-version-001)“).
7. **Fixture-Fund (behoben, nicht App):** Kalender-Regelfelder (`recurrence_rule`) bekamen Titelwörter, rrule.js warf
   „Unknown RRULE property“. `fixtures.mjs` lässt Regelausdrücke jetzt leer. Calendar ist mit den neuen Fixtures
   nachgemessen.

## Selbst angesehen (Screenshots, Variante `--shared`, falls nicht anders genannt)

Angesehen bei 1280 und 420: notes, buchhaltung, customers, ctox (Crew), browser, outbound, app-store, desktop.
Nur bei 1280: knowledge, calendar, documents. Im echten Rahmen (`frame`): notes, app-store und customers bei 1280.

| App | sichtbare Mängel |
|---|---|
| alle Pane-Grammatik-Apps | Werkzeug-Icons im Spaltenkopf (Sortieren/Filter/+/Import/Export) liegen über oder unter dem Suchfeld und sind halb verdeckt (notes, customers, buchhaltung, app-store, outbound, Crew „···“). Ursache vermutlich: die Kopfzeilen-Geometrie steht in `app.css` unter `.shell-window[data-shell-contract="v2"]` und fehlt in v4 (Rahmen und Labor laden sie nicht) |
| notes | 1280: Listentitel „Rahmenvertrag … der Produ“ hart abgeschnitten. Der „Notesnook“-Kopf ist unten beschnitten. Im echten Rahmen liegt das PIN-Sperrfeld (1–9, „Standard PIN: 1234“) offen über dem Inhalt, obwohl keine Notiz gesperrt ist (Sichtbarkeit hängt an fehlender Shared-CSS). 420: Liste vollflächig, gut |
| buchhaltung | 420: Kontentabelle horizontal abgeschnitten (USt/Soll/Haben fehlen, G1). Belegliste nur 2 Zeilen hoch. Die Überschrift „SKR03 Kontenrahmen-Explorer“ wird vom Auswahlfeld überdeckt. „R“ als Platzhalter-Glyphe im Primärknopf |
| customers | Reiterleisten laufen rechts ohne Abschluss aus („Gefährd…“, „Dubletten“, G1 horizontal). Bearbeiten/Löschen-Icons unter der Reiterleiste abgeschnitten. Rohe Statuscodes im Inhalt („ready“, „failed“, „needs_review“). 420: Kundenliste nur 2 Zeilen hoch. „1 Kontakte“ |
| ctox (Crew) | Zwei leere Reiter-Kästen unter dem Titel. Zustandstexte im Inhalt: „In Progress“ (G3), „nicht live“, „noch kein Plan“, „keine Live-Messwerte“, „Prüfung steht aus“. Englische Rohstatus („Active“). 420: Ablaufgrafik rechts abgeschnitten (G1, `ctox-flow-canvas` 850 > 420), Aufgabenliste nur 1 Zeile |
| browser | Zone dauerhaft rot mit nur „Lieferantenbewertung“ (der Sitzungsname, ohne Fehlertext). „● Fehler“-Markierung oben rechts vom Kopf beschnitten. 420: Reiter „Kundentermin Vo…“ abgeschnitten, Steuer-Icons überlappen |
| outbound | Überschrift „Campaigns & Sequenzen“ vom Suchfeld überdeckt. Zustandstexte „1 wartet“, „CTOX arbeitet gerade nicht: 1 wartet“, „0 läuft“, „0 Fehler“ im Inhalt. Rohcodes „lead_qualified“, „reply_received“. Tabelle rechts abgeschnitten. 420: leere Auswahlfelder in der Filterleiste |
| app-store | Kaputtes Icon (icon_svg ohne xmlns). Kopf-Icons hinter dem Suchfeld. 420: Kategorien als horizontale Reihe rechts abgeschnitten („Installiert“, G1) |
| desktop | Inhalt bleibt bei 1280 und 420 leer und weiß, obwohl 6 `desktop_icons` und 6 `desktop_windows` als Fixture vorliegen |
| knowledge | Dunkles App-Theme im hellen Rahmen. Eine senkrechte Linie bei x≈640 läuft durch den ganzen Inhalt. Die Reiter „Runbooks/Quellen/Tabellen“ sind fast unsichtbar (helle Schrift auf hellem Grund). Listentitel „Budgetplanung Vertrieb S D“: das „ü“ fehlt, der Titel ist aus der ID abgeleitet statt aus dem Titelfeld |
| calendar | Kaputtes Icon. Termin-Chips mit dunkler Schrift auf Blau/Petrol, kaum lesbar. Vier leere Segment-Knöpfe oben rechts. „1 Termine“ |
| documents | Zone zeigt „Keine gespeicherte Dokumentversion gefunden … (current-version-001)“, also eine ID im Text. Editorfläche leer |

## Grenzen der Messung

- G6 vergleicht eine Signatur, kein Bild. Ein früher Lauf meldete bei explorer 420 einmal 2 statt 3 Spalten, im
  Wiederholungslauf nicht mehr. Einzelne G6-Abweichungen deshalb im Screenshot `g6-<breite>-in-1280.png` prüfen.
- G3 sieht nur sichtbaren Text und `placeholder`, keine `title`/`aria-label`. Die Ausnahme `v4-allow: label` gilt
  nur, wenn der sichtbare Text das gekennzeichnete Literal vollständig enthält. Über `t()`-Schlüssel übersetzte
  Beschriftungen erkennt sie nicht (dann `data-v4-allow` am Element setzen).
- Die Fixtures sind schemagetreu, aber nicht fachlich stimmig: Querverweise zeigen zyklisch auf vorhandene IDs,
  Statuswerte kommen aus Code-Vergleichen. Rohcodes wie „lead_qualified“ und „string“ im Bild sind teils Fixture-Werte.
  Sie zeigen aber auch, dass die Apps Rohcodes ungefiltert anzeigen.
- Ein Lauf über alle Apps dauert je Variante etwa 20 Minuten (5 Seitenladungen je App). Beide Varianten laufen
  parallel.
