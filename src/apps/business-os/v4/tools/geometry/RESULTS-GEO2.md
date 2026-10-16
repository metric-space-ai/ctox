# Geometrie-Labor – Strang GEO2 (15.10.2026)

Klon `~/Documents/ctox-shell-v4`, Kern-Stub-Pin `9fa9948` (`CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/9fa9948`).
Geändert nur unter `v4/tools/geometry/` und `v4/frame/` (theme + INTEGRATION.md). Keine App-Datei, kein `v4/sw.js`.

## Was geändert ist

| # | Befund (Strang VIS) | Änderung |
|---|---|---|
| 1 | G1 zählt Datentabellen, die in ihrem eigenen Container seitlich scrollen | `measure.mjs`: horizontales Scrollen eines Containers zählt nicht als G1, wenn er selbst oder ein Vorfahr **unterhalb von root** `data-scroll-x` trägt. Das Attribut an root selbst oder am Fenster zählt nicht; root-Scrollen und „ragt aus root“ bleiben streng. Erlaubte Fälle stehen als Hinweis `G1 erlaubt: …` in `notes`. |
| 2 | G6 bei 420 vergleicht mit einem ~640 px breiten Fenster | Ursache gemessen: Im Viewport 420 ist das Fenster ein Mobil-Blatt mit genau 420 px (frei und maximiert, `is-mobile-sheet`). Der Rahmen ist also nicht zu breit, auch nicht bei 620/640/660. Das freie **Vergleichsfenster** in 1280 klemmte der Fenster-Manager auf `layout.min_width` (640 bei 10 Apps, z. B. ctox, tickets). Das Labor veröffentlicht die App für das Vergleichsfenster jetzt ohne `min_width/min_height` (`nomin=1`). Es prüft die Breite: Weicht sie mehr als 2 px ab, ist G6 rot mit „nicht vergleichbar“. `results.json` → `g6.viewportWindowW/compareWindowW`. Kein Rahmenfehler, `v4/frame` dafür unverändert. |
| 3 | Labor lädt `ctox:lib/<name>@<major>/…` nicht (spreadsheets G3) | Der Labor-Server schreibt mit **der `rewriteShellRefs` aus `v4/sw.js`** um (per `vm` geladen, keine Kopie), also JS und CSS, `ctox:shared/` und `ctox:lib/`. Er löst Bibliothekspfade aus dem Quellordner in `v4/tools/isolation/standard-libs.json` auf (nur `approved` und im SW freigegeben: `ctox-office@1` → `vendor/ctox-office/`, `monaco@0` → `vendor/monaco/`). Sonst antwortet er 404, Pfadflucht ebenfalls 404. Auf Hinweis des Orchestrators (Kern §16.37) bildet er die **neue Form `/~a/lib:<name>@<major>/…`** nach. Die alte Form `/~a/shell@<v>/lib/…` bleibt vorerst mit auflösbar. Strang LIBS hat `sw.js` während dieses Strangs umgestellt (Arbeitsbaum 17:59, uncommittet): Das Labor erzeugt jetzt `/~a/lib:ctox-office@1/…`, und die 8 Tests samt Integration (spreadsheets) sind damit grün. Der Gesamtlauf 15:57Z lief noch mit der alten Form. Nebenwirkung: CSS-`ctox:shared/` wird jetzt in beiden Varianten umgeschrieben, wie im heutigen SW. |
| 4 | Kopf-Icons rutschen unter den Titel (knowledge, notes, outbound) | `v4/frame/theme/build-theme.mjs` erzeugt in `theme.css` zusätzlich 14 Kit-Regeln. Übernommen sind genau die obersten `app.css`-Regeln, deren Selektoren nur Klassen aus `KIT_CLASSES` tragen: `.ctox-pane-header/-title-row/-titles/-kicker/-title`, `.drawer-body`, `.drawer-header-row`, `.os-scrollbar`. Jede Klasse muss von mindestens einer App genutzt sein (Nutzung je Klasse im Kopfkommentar von `theme.css`: 30 bis 38 Apps für die Pane-Klassen, 3 bis 4 für die übrigen), sonst bricht der Generator ab. Nicht übernommen: `.drawer` (fixed-Shell-Chrome), `.list`, `.pane-title` (keine App nutzt sie als Klasse) und v2-Regeln, die an `.shell-window[…]` hängen. **G4-Anpassung:** Kicker und Pane-Titel kürzen in app.css per Ellipse. Im Kit entfallen `nowrap/overflow/text-overflow` und das dafür nötige `min-width: 0`, dazu kommt `overflow-wrap: break-word`. Zwei v4-Regeln legen fest, wer weicht: Der Kicker bleibt ganz, der Titel bricht an Wortgrenzen um. Ohne das Weglassen von `min-width: 0` brach „OUTBOUND“ mitten im Wort (Screenshot-Befund während des Strangs). |

Werkzeug: `measure.mjs` (neu, Messfunktion aus `geometry-v4.mjs` ausgelagert, damit sie an Fixtures und Mutanten
testbar ist), `geometry-v4.test.mjs` (neu). `geometry-v4.mjs` klassifiziert nur noch beim direkten Aufruf
(`IS_MAIN`) und exportiert `serve/resolveLabPath/labBody/LIB_DIRS/loadSwRewrite/configs/appConfig` für Test und Sonden.

## Abnahme (ausgeführt)

| Befehl (aus Klon-Wurzel) | Ergebnis |
|---|---|
| `CTOX_APP_API_STUB=…/9fa9948 node --test src/apps/business-os/v4/tools/geometry/geometry-v4.test.mjs` | 8/8 grün (G1-Fixtures 2 positiv, 4 negativ; 2 Mutanten; SW-Umschreibung; Bibliotheksauflösung alt+neu inkl. 404; §16.37-Simulation; Integration tickets+spreadsheets 1280/420) |
| `node src/apps/business-os/v4/frame/lab/frame.test.mjs mock` | 60/60 bestanden |
| `node src/apps/business-os/v4/tools/guards/guard-v4.mjs` | Exit 0 |
| `node src/apps/business-os/v4/frame/theme/build-theme.mjs --check` | aktuell (30 hell, 24 dunkel) |
| `node --test src/apps/business-os/v4/tools/isolation/ctox-lib.test.mjs` | 6/6 (unverändert grün) |

**Mutationsbelege**

- G1 Mutant 1 (im Test): Ausnahme entfernt (`hscroll.push` statt Weiche). Die positive Fixture wird rot, die Ausnahme ist also tragend.
- G1 Mutant 2 (im Test): Attribut überall gültig (`return !!c`). Das Fenster-Attribut würde fälschlich grün, die Grenze „unterhalb von root“ ist also tragend.
- G6 (von Hand, danach zurückgesetzt): `nomin: true` → `nomin: false` im Runner. Der Integrationstest wird rot mit „Vergleichsfenster 640 px, expected 420“.
- Sonde vor dem Fix (`tickets`/`ctox`, Viewport 1280, Geometrie w=420): Ohne `nomin` ist das Fenster 640 px breit, mit `nomin` 420. Im Viewport 420 ist es frei wie maximiert 420 px (`is-mobile-sheet`), `scrollWidth` des Dokuments ebenfalls 420.

## Gesamtlauf `--shared` vorher/nachher

Vorher: 15:32Z, Stand vor jeder GEO2-Änderung. Nachher: 15:57Z, alle vier Punkte. 40 Apps × 3 Breiten. Rohdaten:
`out/geo2/results-before.json`, `out/geo2/results-after.json`. Fett = geändert, ✱ = App mit Änderung.

| App | 1280 vorher | 1280 nachher | 900 vorher | 900 nachher | 420 vorher | 420 nachher |
|---|---|---|---|---|---|---|
| app-store | OK | OK | OK | OK | G1 | G1 |
| appsec-pentest | OK | OK | OK | OK | OK | OK |
| browser | G3 G4 | G3 G4 | G3 G4 | G3 G4 | G3 G4 | G3 G4 |
| buchhaltung ✱ | OK | **G4** | OK | **G4** | G1 | **OK** |
| calendar ✱ | OK | OK | OK | OK | OK | **G4** |
| coding-agents | OK | OK | OK | OK | OK | OK |
| consent | OK | OK | OK | OK | OK | OK |
| conversations ✱ | OK | **G4** | OK | **G4** | G1 | **G1 G4** |
| creator | OK | OK | OK | OK | OK | OK |
| credentials | OK | OK | OK | OK | OK | OK |
| ctox ✱ | OK | OK | OK | OK | G6 | **G4** |
| customers ✱ | G1 | G1 | G1 | **G1 G4** | G1 | **G1 G4** |
| cv-print-builder | G1 | G1 | G1 | G1 | OK | OK |
| desktop | OK | OK | OK | OK | OK | OK |
| documents | G3 | G3 | G3 | G3 | G3 | G3 |
| esign | OK | OK | OK | OK | OK | OK |
| explorer | OK | OK | OK | OK | OK | OK |
| file-viewer | OK | OK | OK | OK | OK | OK |
| importer | OK | OK | OK | OK | OK | OK |
| intake | OK | OK | OK | OK | OK | OK |
| interviews | OK | OK | OK | OK | OK | OK |
| invoices | OK | OK | OK | OK | OK | OK |
| iot | OK | OK | OK | OK | OK | OK |
| knowledge | OK | OK | OK | OK | OK | OK |
| kundenpipeline | OK | OK | OK | OK | OK | OK |
| mail | OK | OK | OK | OK | OK | OK |
| matching | OK | OK | OK | OK | OK | OK |
| nachweise | OK | OK | OK | OK | OK | OK |
| news | G3 | G3 | G3 | G3 | G3 | G3 |
| notes | OK | OK | OK | OK | OK | OK |
| outbound ✱ | G1 | **OK** | G1 | **OK** | G1 | **OK** |
| placements | OK | OK | OK | OK | OK | OK |
| reports | OK | OK | OK | OK | OK | OK |
| research ✱ | OK | **G1 G4** | OK | **G4** | OK | **G4** |
| shiftflow ✱ | OK | OK | G1 | **OK** | G1 | **OK** |
| spreadsheets ✱ | G3 | **OK** | G3 | **OK** | G3 | **OK** |
| submissions | OK | OK | OK | OK | OK | OK |
| support ✱ | OK | OK | OK | **G4** | OK | **G4** |
| threads | OK | OK | OK | OK | OK | OK |
| tickets | OK | OK | OK | OK | OK | OK |

| Kriterium | rote Läufe vorher | rote Läufe nachher |
|---|---:|---:|
| G1 | 13 | 8 |
| G2 | 0 | 0 |
| G3 | 12 | 9 |
| G4 | 3 | 17 |
| G5 | 0 | 0 |
| G6 | 1 | 0 |
| **Läufe OK** | **94/120** | **93/120** |

### Was sich bewegt hat und warum

- **G1 −5:** outbound (1280/900/420) und buchhaltung 420 sind grün durch `data-scroll-x`. Der Hinweis nennt
  `outbound-table-scroll-unified 1382>882…396` und `fibu-scrollable-container 680>412`. shiftflow 900/420 ist grün, weil `.os-scrollbar`
  (Kit) die Bildlaufleiste auf 6 px setzt. Vorher scrollte der Planer um 3 px (513>510, 464>461).
- **G3 −3:** spreadsheets lädt `ctox-office@1` und meldet keine Zone „Editor konnte nicht geladen werden …“ mehr.
- **G6 −1:** ctox 420 war rot nur wegen des 640-px-Vergleichsfensters („Spalten 1 vs 2“). Gegen 420 px ist es gleich.
- **G4 +14, bewusst sichtbar gemacht:** Alle neuen G4-Treffer sind **app-eigene** Ellipsen. Jede genannte App setzt in ihrer
  `index.css` `text-overflow: ellipsis` auf `.ctox-pane-title`/`-kicker`: buchhaltung :18, calendar :20, ctox :18,
  customers :18, conversations :21, support :177, research :103/:323. Die Kit-Regeln enthalten keine Ellipse. Vorher
  stand der Titel allein auf einer Zeile (Kopf-Icons darunter) und hatte Platz. Jetzt teilt er die Zeile mit den
  Icons, wie in v3, und die App-Ellipse greift. Arbeitsliste je App, keine Labor- oder Rahmenaufgabe.
- **research 1280 G1 neu:** `div.ctox-pane-tabs 642>586`. Die Reiterleiste der Mitte („Fachberic…“) ist schmaler,
  seit die Kopfzeilen der Spalten Titel und Icons nebeneinander tragen. App-Befund (Reiterleiste ohne Umbruch).

### Selbst angesehen (Kopfzeile, `out/geo2/<app>-<breite>-vorher|nachher.png`)

| App | 1280 | 420 |
|---|---|---|
| knowledge | vorher: „Knowledge“ oben, +/Export/Import eine Zeile tiefer über dem Suchfeld. Nachher: Titel und drei Icons auf einer Zeile | vorher wie 1280. Nachher: eine Zeile |
| notes | vorher: nur „Notesnook“, Titel abgeschnitten, Icons rechts neben Suche/Reitern verteilt. Nachher: Kicker NOTESNOOK + Titel „Notizen“ links, +/Spalte/Tag/Export/Import/Sperre rechts auf einer Zeile | zeigt bei 420 die Detailansicht (Editor), dort keine Pane-Kopfzeile. Werkzeugleiste ok |
| outbound | vorher: „Outbound / Campaigns & Sequenzen“ zweizeilig, vom Suchfeld überdeckt, Icons unter der Überschrift. Nachher: „OUTBOUND Campaigns & / Sequenzen“ + Icons auf einer Zeile, rechts „PROJEKTPLAN Projektplan Relaunch Kundenportal“ + Icons | nachher: „OUTBOUND Campaigns & Sequenzen“ + Icons. „PROJEKTPLAN Projektplan Relaunch / Kundenportal“ bricht an der Wortgrenze um, kein Abschneiden |

## Offen / nicht in diesem Strang

- App-Ellipsen auf Pane-Titeln (Liste oben): je App `text-overflow` entfernen oder `data-v4-allow-ellipsis` setzen.
- Sobald die LIBS-Umstellung von `v4/sw.js` gelandet ist, den Altpfad `/~a/shell@<v>/lib/…` in `resolveLabPath` und
  die zugehörige Testzeile streichen.
- Variante `frame` (ohne `--shared`) wurde in diesem Strang nicht neu über alle Apps gemessen.

## Dateien

- `v4/tools/geometry/measure.mjs` (neu): Messung G1–G5 und Signatur, G1-Ausnahme `data-scroll-x`
- `v4/tools/geometry/geometry-v4.mjs`: Import aus measure.mjs, SW-Umschreibung per vm, `ctox:lib` (alt + §16.37),
  G6-Vergleichsfenster `nomin` + Breitenprüfung, Exporte, `IS_MAIN`
- `v4/tools/geometry/geometry-v4.test.mjs` (neu): 8 Tests inkl. Mutanten und Integration
- `v4/tools/geometry/RESULTS-GEO2.md` (neu), `v4/tools/geometry/out/geo2/` (results vorher/nachher, 12 Screenshots)
- `v4/frame/theme/build-theme.mjs`: Kit-Regeln (KIT_CLASSES, Nutzungsprüfung, G4-Anpassung, KIT_V4)
- `v4/frame/theme/theme.css`: neu generiert (+`--motion-fast`, +14 Kit-Regeln)
- `v4/frame/INTEGRATION.md`: Z2 um die Kit-Regeln ergänzt
