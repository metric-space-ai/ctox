# KIT1: Reiterüberlauf und Liste → Detail bei schmaler Breite (Pane-Kit, für alle Apps)

Strang KIT1 (Shell v4). Klon `~/Documents/ctox-shell-v4`, Branch `shell-v4/prototype`. Lauf am 16.10.2026 zwischen
00:38 und 01:40. Kein git außer Lesen, keine Subagents. Alle Messungen unten habe ich selbst ausgeführt.

## Ergebnis in Zahlen

| Messung (Geometrie-Labor, Stub a8765b9, 40 Apps) | ohne Kit (HEAD-Kit) | mit KIT1 |
|---|---|---|
| G11 schmal bei 420 grün | 13/40 | **39/40** (Ausnahme: matching, siehe unten) |
| Apps bei 420 ganz grün (G1–G11) | 8/40 | 25/40 |
| Apps bei 1280 ganz grün | 35/40 | 36/40 |
| Regressionen eines Kriteriums (G1–G9) gegen den Lauf ohne Kit, 1280 und 420 | – | **1** (iot G4, siehe unten) |

Die Vergleichsbasis „ohne Kit“ ist eine Kopie des Baums, in der `pane-narrow.js` entfernt ist und `frame.css`,
`pane-heads.js` und `column-resizers.js` auf HEAD stehen. Das G11-Labor ist dort dasselbe. Die App-CSS stammt in
beiden Läufen aus demselben Arbeitsstand, einschließlich der parallelen DARK-Änderungen.

Besser als ohne Kit sind G8 bei appsec-pentest, iot, shiftflow (420) und knowledge (1280) sowie G1 bei invoices und
iot.

Die eine Regression: Bei iot (leere Fixture) steht jetzt das Detail vorn, und dessen Titel „Messeplanung Hannover 2027“
endet in einer Ellipse (G4). Ohne Kit lag dieser Kopf unterhalb und wurde nicht gemessen.

Einen Titelumbruch im Detail-Kopf habe ich zweimal versucht und wieder herausgenommen:
- mit `overflow-wrap:anywhere` brach der Titel Buchstabe für Buchstabe um (invoices);
- an Wortgrenzen wurde er bei invoices abgeschnitten, ohne Ellipse.

Die Ellipse der App ist das kleinere Übel. Die übrigen roten Werte bei 420 (G4 „Kunden“/„Rechnungen“ u. a.) sind ohne
Kit genauso rot.

## Was gebaut ist

### 1. Reiter einzeilig mit „Mehr ▾“ (alle Breiten)
Datei `v4/frame/pane-narrow.js` (neu) plus Kit-Regeln in `v4/frame/frame.css`.

- Jede sichtbare `.ctox-pane-tabs` bleibt einzeilig: `nowrap`, die Reiter schrumpfen nicht.
- Die Grenze ist der nächste abschneidende Vorfahr (Pane, Scroller, root) abzüglich der Innenabstände dazwischen. Die
  eigene Box der Reiter zählt nicht, denn sie kann durch die noch sichtbaren Reiter über die Pane hinausragen. Das war
  der Fehler im ersten Wurf (customers 1280, G8).
- Was nicht passt, wird ausgeblendet (`data-v4-tab-overflow`, ganze Reiter). Der Zähler bleibt also am Reiter.
- Der aktive Reiter bleibt immer sichtbar.
- „Mehr ▾“ öffnet ein Menü im Fenster (`.v4-menu`) mit den verborgenen Reitern samt Zählern. Eine Auswahl klickt den
  Original-Reiter, das Routing der App bleibt die Quelle der Wahrheit. Esc oder ein Klick daneben schließt das Menü.
- Wirkt auch bei 1280. Beispiel customers: Workbench-Reiter „Übersicht … Dateien, Mehr ▾“ statt zwei Zeilen.

### 2. Liste → Detail unter 600 px Fensterbreite
Das gilt nur per Container-Query `@container business-app-window (max-width: 599.98px)`, nicht per Viewport.

- **Wann:** Ein Workspace (`.ctox-workspace` bzw. `[data-resize-frame]`, der äußerste mit mindestens zwei Panes) zeigt
  von Natur aus mindestens zwei Panes. Zeigt die App selbst nur eine (mail, support nach dem Tipp), tritt das Kit
  zurück.
- **Rollen:** Die erste Pane im DOM ist `list`, die zweite `detail`, weitere sind `aside`. Markiert wird mit
  `data-v4-md="list|detail"` am Workspace und `data-v4-md-role` an den Panes.
- **CSS:** Der Workspace wird zur Flex-Spalte. Im Listenmodus füllt die Liste das Fenster. Im Detailmodus füllt das
  Detail das Fenster (`flex: 0 0 100%`); Inspektor-Panes (`aside`) liegen darunter in derselben Spalte und sind per
  Scrollen erreichbar (tickets, research).
- **Öffnen:** Ein Tipp in der Liste wird scharf gestellt, ausgenommen Suche/Filter, Reiter, Texteingaben und
  Spaltenteiler. Das Detail öffnet sich, wenn die App danach
  - das Detail neu rendert (MutationObserver, Stiländerungen ausgenommen),
  - die getippte Zeile als ausgewählt markiert (`aria-selected`/`.is-selected`/…; Fall vorausgewählte Zeile, customers),
  - oder den ganzen Workspace neu rendert. Der Zustand hängt am Pfad des Workspace, nicht am Element.

  Das deckt auch „+“ im Listenkopf ab: die App rendert das Formular ins Detail. Kontrollkästchen in Zeilen zählen
  ebenfalls; calendar blendet damit einen Kalender ein und zeigt den Monat. Das Kit wählt nichts selbst aus und ändert
  keinen App-Zustand.
- **Zurück:** „‹ Zurück“ steht als erstes Element der Titelzeile im Detail-Kopf. pane-heads.js rückt die Zeile aus der
  Icon-Zone (G8). Der Knopf schaltet nur die Ansicht um, nicht die Auswahl der App.
- **Scrollstand:** Er wird beim Tipp gesichert, bevor die App die Liste neu rendert, über Element und Kindpfad. Das
  Labor prüft die Wiederherstellung auf ±2 px.
- **Leere Liste** (keine Zeilen, keine Bedienelemente im Listenkörper): Das Detail kommt zuerst, dort liegen „Neu“ bzw.
  die Startinhalte. „Zurück“ zeigt die leere Liste. Sobald der Nutzer selbst wechselt, gilt seine Wahl. Ohne diese
  Regel stünde man bei creator, credentials, iot und shiftflow (Fixture leer) vor einer leeren Liste ohne Weg weiter.
- **Detail-Kopf am linken Rand:** Die Titelzeile ist Flex mit Umbruch (Aktionen rutschen unter den Knopfcluster),
  der Titelblock behält mindestens 6em. Der Titel selbst bleibt wie in der App, meist mit Ellipse. Der Kopf ist
  mindestens so hoch wie die Icon-Zone, damit der Inhalt unter dem Icon beginnt (submissions). Zurück liegt auf eigener
  Ebene (`z-index`), damit kein App-Element darüberliegt.
- **Lebenszyklus:** wie pane-heads in `v4/shell/column-resizers.js` (attach nach mount, detach vor unmount).

### 3. pane-heads.js (Schutzzonen G8), zwei kleine Erweiterungen
- Flache Zeilen direkt unter dem Pane-Kopf (Geschwister, höchstens 64 px, mit Bedienelement) werden wie Kopfzeilen aus
  den Zonen gerückt. Nötig, weil Detail-Panes bei schmaler Breite am linken Rand stehen (calendar „Monat/Woche/Tag“).
  Dazu kommen Regeln `.ctox-pane-header ~ [data-v4-under-*]` in frame.css.
- Der Treffertest nutzt das Rechteck der Zeile samt direkter Kinder. Ein 44-px-Reiter in einer 37-px-Leiste ragte
  sonst in die Zone (appsec-pentest; behebt auch den alten G8 dort).

## Kriterium G11 „schmal“ (Labor)
Dateien: `v4/tools/geometry/g11.mjs` (neu, Seitenfunktionen) und `runG11` in `v4/tools/geometry/geometry-v4.mjs`.
Gemessen wird bei Breiten unter 600, im Lauf mit 420.

- a) Höchstens eine Pane im sichtbaren Bereich je Workspace. Gestapelte Liste+Detail ist rot; ein Inspektor unterhalb
  des Bildes zählt nicht.
- b) Reiter (`.ctox-pane-tabs`, `[role=tablist]`) einzeilig (±3 px) und nicht über den Rand.
- c) root ohne horizontalen Überlauf.
- d) Interaktiv mit echter Maus:
  1. Liste anscrollen.
  2. Zeile antippen; tut der Text nichts, das erste Bedienelement der Zeile.
  3. Detail sichtbar, Liste verborgen.
  4. „Zurück“ sichtbar, an seinem Mittelpunkt selbst getroffen, außerhalb der Zonen; kein Bedienelement des Details in
     den Zonen.
  5. „Zurück“ antippen: Liste sichtbar, Scrollstand ±2 px.

  Varianten: leere Liste (Detail zuerst, Zurück zur Liste) und App-eigene Detailansicht.

**Mutationsbeleg** (`geometry-v4.test.mjs`, Fixture-App mit Liste, Detail und acht Reitern mit Zählern im echten
frame.css, pane-heads.js und pane-narrow.js):
- ohne Kit ist G11 rot (2 Panes, Reiter abgeschnitten bzw. mehrzeilig); mit Kit grün, statisch und interaktiv;
- der Mehr-Menü-Fluss klickt den Original-Reiter, der Zähler bleibt im Menü, der neue aktive Reiter ist sichtbar;
- bei 900 px bleibt der Workspace zweispaltig;
- Kit-Mutanten machen G11 rot: Zurück-Handler entfernt, Scroll-Wiederherstellung entfernt, Reiterüberlauf aus;
- Kriteriums-Mutant ohne Spaltenzählung: die gestapelte Fixture ginge durch.

`node --test v4/tools/geometry/geometry-v4.test.mjs`: **23/23**.

## G11 je App (420, Endlauf)

- **Liste → Tipp → Detail → Zurück (mit Scrollstand) grün (20):** app-store, appsec-pentest, browser, buchhaltung,
  calendar, consent, conversations, ctox, customers, esign, intake, invoices, kundenpipeline, nachweise, outbound,
  placements, reports, research, submissions, tickets
- **Leere Liste (Fixture ohne Zeilen) → Detail zuerst, Zurück zur Liste, grün:** creator, credentials, cv-print-builder,
  iot, shiftflow (bei credentials, iot und shiftflow bleibt G4 rot, weil der Detail-Titel eine Ellipse hat)
- **App-eigene Detailansicht, Kit tritt zurück, grün:** support
- **Keine Liste/Detail-Teilung (eine Pane oder App-eigene Navigation), grün:** coding-agents, desktop, documents,
  explorer, file-viewer, importer, interviews, knowledge, mail, news, notes, spreadsheets, threads
- **Ausnahme, rot: matching.**
  - Im Labor-Lauf überdeckt `.ctox-pane-titles` der App den Zurück-Knopf. Der Treffertest trifft `div.ctox-pane-titles`,
    Zurück wirkt nicht.
  - In einer Einzelprobe mit derselben Fixture, Locale de-DE und vorheriger G1–G9-Messung ist Zurück dagegen getroffen
    und wirkt (`shots`: siehe Bilder unten).
  - Ursache offen. matching hat eine eigene schmale Spaltenlogik (`grid-row`, unterer Match-Details-Drawer, feste
    37-px-Titelzeile).
  - Ohne Kit ist matching bei 420 ebenfalls G11-rot (gestapelt). Folgeauftrag an den App-Besitzer: Titelzeile der
    `matching-main` bei schmaler Breite.

Hinweis zur Fixture: kundenpipeline, cv-print-builder und weitere hängen beim Zweig „leere Liste“ an den Daten. Bei
leerer Fixture zeigt das Kit das Detail zuerst. Mit Daten gilt Liste zuerst. Der Fixture-Fall ist im Labor eigens
geprüft.

## App-Dateien
Keine angefasst. Keine App umgeht das Kit so, dass ein App-Eingriff nötig war. App-Tests berührter Apps fallen daher
nicht an. Fälle, in denen die Kit-Annahme „erste Pane = Liste“ fachlich schwach ist, im Bericht statt im Code:

- **calendar:** Links liegen die Kalender (Ein/Aus), rechts der Monat. Der Monat ist über das Kontrollkästchen
  erreichbar, nicht über den Namen der Zeile.
- **shiftflow:** Erste DOM-Pane ist die Planung, die zweite der Dienstplan. Mit leerer Fixture kommt der Dienstplan
  zuerst.

Vorschlag für später, nur auf Wunsch: Eine App deklariert die Rollen selbst (`data-v4-md-role` im Markup). Das Kit
würde sie übernehmen. Ich habe das nicht gebaut, weil es heute keine App braucht.

## Abnahme-Befehle (selbst ausgeführt)

| Befehl | Ergebnis |
|---|---|
| `node --test v4/tools/geometry/geometry-v4.test.mjs` | 23/23 (6 neue G11-/Kit-Tests mit Mutanten) |
| `CTOX_APP_API_STUB=…/a8765b9 node v4/tools/geometry/geometry-v4.mjs --widths 1280,420` | 39/40 G11 grün; gegen „ohne Kit“ eine Regression (iot G4, siehe oben); Ausgabe im Scratchpad |
| `node v4/frame/lab/frame.test.mjs mock` | **167/167** |
| `CTOX_APP_API_STUB=…/a8765b9 node v4/frame/lab/frame.test.mjs stub` | **198/198** |
| darin G0 Fensterrahmen | icon/nw/ne/sw/se/cluster = 1 (100 %) |
| darin FR6d-G0 | desktop/crew/start = 1 |
| darin TOP-1 Kopfleiste | hell-1280, dunkel-1280, dunkel-420 je links/Mitte/rechts = 1 |
| `node scripts/assert-shell-v2-contract.mjs` | 38/38 |
| `node v4/debt/ratchet.mjs` | OK (guard 1840) |

Zur Ratchet: Zwischendurch stand sie auf GESTIEGEN (`vocab:de-nicht-verfuegbar` 1 → 2). Ursache war
`v4/source/source-mode.js:872` aus dem parallelen Strang LOOP, nicht KIT1. Im Endlauf steht sie auf OK.

Nach dem letzten CSS-Stand lief `frame.test.mjs mock` noch einmal: **167/167**. Der Stub-Lauf (198/198) liegt vor
den letzten Detail-Kopf-Regeln. Diese wirken nur unter `[data-v4-md="detail"]` unter 600 px.

Das Geometrie-Labor (`geo-final3`) lief auf dem Endstand: 1280 36/40, 420 25/40, G11 39/40.

## Bilder (selbst angesehen, 420)
Im Scratchpad dieser Sitzung (`…/scratchpad/geo-final3/<app>/420.png` und `420-detail.png`):

- **customers:** Liste in voller Höhe mit Reitern „Alle Kunden(12), Aktive Kunden(3), Mehr ▾“. Detail mit „‹ Zurück“
  rechts vom Icon, Bearbeiten/Löschen in der zweiten Zeile, Workbench-Reiter „Übersicht, Kontakte, Mehr ▾“.
- **mail:** App-eigene Navigation, eine Pane, Reiter einzeilig. Die Fixture ist leer.
- **invoices:** Liste der sechs Rechnungen in voller Höhe, Reiter einzeilig. Detail mit Zurück und Feldern; Titel mit
  Ellipse wie in der App.
- **tickets:** Liste, dann Detail „Ticket Offen“ mit Zurück und Verlauf. Der Inspektor liegt darunter.
- **outbound:** Liste, dann Detail mit Zurück. Die Reiter „Kampagne/Sequenz & Versand“ stehen einzeilig, die
  Funnel-Reiter haben „Mehr ▾“.

## Offene Punkte
1. **matching:** Zurück im Labor-Lauf verdeckt, siehe oben. Ursache offen.
2. **Manifest:** `v4/manifests/shell.manifest.json` führt `pane-narrow.js` bereits (ein anderer Strang hat es neu
   erzeugt), aber mit älteren Hashes. `frame.css`, `pane-heads.js`, `pane-narrow.js` und `column-resizers.js` müssen
   bei der Integration mit `gen-manifest.mjs` neu gehasht werden. Ich habe es nicht selbst erzeugt, weil die Datei
   parallel von anderen Strängen geschrieben wird.
3. **rundgang2.mjs:** nicht gelaufen. Für den Brief galt „rundgang2 bzw. Geometrie-Labor“; das Geometrie-Labor lief
   über alle 40 Apps bei 1280 und 420, mit echtem Tipp und Zurück je App.
